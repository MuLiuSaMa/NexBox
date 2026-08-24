use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[cfg(target_os = "windows")]
use winreg::enums::*;
#[cfg(target_os = "windows")]
use winreg::RegKey;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GpuInfo {
    /// 显卡当前名称
    pub name: String,
    /// 是否为核显（集成显卡）
    pub is_integrated: bool,
    /// 备份的原始名称（已备份时才有）
    pub original_name: Option<String>,
    /// 是否有可恢复的备份
    pub is_backed_up: bool,
    /// 显卡在 Enum\PCI 下的相对路径（vendor\device 格式），用于精确改写指定显卡
    pub key_path: String,
}

/// 单张显卡的备份记录（按注册表 key_path 精确区分核显/独显）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GpuBackupEntry {
    pub key_path: String,
    pub original_name: String,
    pub is_integrated: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GpuRenameResult {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GpuOption {
    pub id: String,
    pub name: String,
    pub category: String,
}

fn get_appdata_backup_path() -> Result<PathBuf, String> {
    let appdata_dir = dirs::config_dir()
        .ok_or("无法获取 APPDATA 目录")?
        .join("NexBox");
    std::fs::create_dir_all(&appdata_dir)
        .map_err(|e| format!("创建数据目录失败: {}", e))?;
    Ok(appdata_dir.join("gpu_rename_backup.json"))
}

fn get_install_dir_backup_path() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(|p| p.join("gpu_rename_backup.json"))
}

/// 备份数据：新版为多显卡数组；旧版为单个对象（兼容迁移）
enum BackupData {
    Multi(Vec<GpuBackupEntry>),
    /// 旧版单条备份（仅记录一个原始名称）
    Legacy(String),
}

fn read_backup_from(path: &std::path::Path) -> Option<BackupData> {
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    // 新版：多显卡数组
    if let Ok(entries) = serde_json::from_str::<Vec<GpuBackupEntry>>(&content) {
        if !entries.is_empty() {
            return Some(BackupData::Multi(entries));
        }
    }
    // 旧版：单个对象，仅提取原始名称
    #[derive(serde::Deserialize)]
    struct LegacyGpuInfo {
        original_name: String,
    }
    if let Ok(info) = serde_json::from_str::<LegacyGpuInfo>(&content) {
        return Some(BackupData::Legacy(info.original_name));
    }
    None
}

fn save_backup(entries: &[GpuBackupEntry]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(entries)
        .map_err(|e| format!("序列化备份数据失败: {}", e))?;

    // 写入 %APPDATA%/NexBox/ — 持久保留
    let appdata_path = get_appdata_backup_path()?;
    fs::write(&appdata_path, &json)
        .map_err(|e| format!("写入备份文件失败: {}", e))?;

    // 同时也写一份到安装目录 — 方便用户直接查看
    if let Some(install_path) = get_install_dir_backup_path() {
        let _ = fs::write(&install_path, &json);
    }

    Ok(())
}

fn load_backup() -> Result<Option<BackupData>, String> {
    // 优先从 %APPDATA% 读取
    if let Ok(appdata_path) = get_appdata_backup_path() {
        if let Some(info) = read_backup_from(&appdata_path) {
            return Ok(Some(info));
        }
    }

    // 回退到安装目录（兼容旧版本）
    if let Some(install_path) = get_install_dir_backup_path() {
        if let Some(info) = read_backup_from(&install_path) {
            // 自动迁移到 %APPDATA%（保持原始内容，旧版格式无需转换）
            if let Ok(appdata_path) = get_appdata_backup_path() {
                if let Ok(content) = std::fs::read_to_string(&install_path) {
                    let _ = std::fs::write(&appdata_path, content);
                }
            }
            return Ok(Some(info));
        }
    }

    Ok(None)
}

#[cfg(target_os = "windows")]
fn find_gpu_registry_keys() -> Result<Vec<(RegKey, String, bool)>, String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let enum_key = hklm.open_subkey("SYSTEM\\CurrentControlSet\\Enum\\PCI")
        .map_err(|e| format!("打开注册表键失败: {}", e))?;
    
    let mut gpu_keys = Vec::new();
    
    // 支持的显卡厂商 PCI Vendor ID: NVIDIA(10DE)、AMD(1002)、Intel(8086)
    let supported_vendors = ["VEN_10DE", "VEN_1002", "VEN_8086"];
    // 排除关键词：USB控制器等非显卡设备、网卡（Intel 网卡同为 VEN_8086，
    // 名称含 "Intel" 会被误判为显卡）、Microsoft 基础显示适配器（未安装驱动的占位设备）
    let exclude_keywords = [
        "usb", "controller", "控制器", "host", "xhci", "ehci", "uhci", "chipset", "smbus",
        "audio", "sound", "basic display",
        // 网卡相关：Intel 网卡名通常含 "Intel" 且不含已知显卡关键词，需显式排除
        "ethernet", "network", "网卡", "wlan", "wifi", "wi-fi", "wireless", "adapter",
        "connection", "lan", "nic", "bluetooth",
    ];
    // 显卡名称关键词（NVIDIA / AMD / Intel）
    // 注意：不能包含裸 "intel" —— Intel 的 ME 接口、PCIe 根端口、共享 SRAM、
    // TypeC PCIe 等非显卡设备名同样含 "Intel"，会被误判为显卡。
    // 只有强显卡特征词（graphics/iris/uhd/hd/arc 等）才算显卡。
    let gpu_keywords = [
        "nvidia", "geforce", "gtx", "rtx", "amd", "radeon",
        "uhd graphics", "iris", "hd graphics", "arc", "graphics",
    ];
    
    for vendor_result in enum_key.enum_keys() {
        let vendor_key_name = match vendor_result {
            Ok(name) => name,
            Err(_) => continue,
        };
        let vendor_key = match enum_key.open_subkey(&vendor_key_name) {
            Ok(key) => key,
            Err(_) => continue,
        };
        
        // 只处理 NVIDIA / AMD 厂商
        let vendor_upper = vendor_key_name.to_uppercase();
        if !supported_vendors.iter().any(|v| vendor_upper.contains(v)) {
            continue;
        }
        
        for device_result in vendor_key.enum_keys() {
            let device_key_name = match device_result {
                Ok(name) => name,
                Err(_) => continue,
            };
            let device_key = match vendor_key.open_subkey(&device_key_name) {
                Ok(key) => key,
                Err(_) => continue,
            };
            let key_path = vendor_key_name.clone() + "\\" + &device_key_name;
            
            // 排除 USB 控制器等非显卡设备
            let mut is_excluded = false;
            if let Ok(device_desc) = device_key.get_value::<String, _>("DeviceDesc") {
                let lower = device_desc.to_lowercase();
                if exclude_keywords.iter().any(|kw| lower.contains(kw)) {
                    is_excluded = true;
                }
            }
            if !is_excluded {
                if let Ok(friendly_name) = device_key.get_value::<String, _>("FriendlyName") {
                    let lower = friendly_name.to_lowercase();
                    if exclude_keywords.iter().any(|kw| lower.contains(kw)) {
                        is_excluded = true;
                    }
                }
            }
            if is_excluded {
                continue;
            }
            
            // 判断是否为显卡：
            // 1. 优先按 ClassGUID（显卡类 {4d36e968-...}）判断，NVIDIA / AMD 通用
            // 2. 回退按名称关键词判断（AMD 的 DeviceDesc 通常是硬件路径，需靠 FriendlyName 兜底）
            let mut is_gpu = false;
            if let Ok(class_guid) = device_key.get_value::<String, _>("ClassGUID") {
                let normalized = class_guid
                    .trim()
                    .trim_start_matches('{')
                    .trim_end_matches('}')
                    .to_uppercase();
                if normalized == "4D36E968-E325-11CE-BFC1-08002BE10318" {
                    is_gpu = true;
                }
            }
            if !is_gpu {
                if let Ok(class) = device_key.get_value::<String, _>("Class") {
                    if class.eq_ignore_ascii_case("Display") {
                        is_gpu = true;
                    }
                }
            }
            if !is_gpu {
                if let Ok(device_desc) = device_key.get_value::<String, _>("DeviceDesc") {
                    let lower = device_desc.to_lowercase();
                    if gpu_keywords.iter().any(|kw| lower.contains(kw)) {
                        is_gpu = true;
                    }
                }
            }
            if !is_gpu {
                if let Ok(friendly_name) = device_key.get_value::<String, _>("FriendlyName") {
                    let lower = friendly_name.to_lowercase();
                    if gpu_keywords.iter().any(|kw| lower.contains(kw)) {
                        is_gpu = true;
                    }
                }
            }
            
            if is_gpu {
                // 通过 LocationInformation 判断是否为核显（集成显卡）
                // 核显的 LocationInformation 通常包含 "Internal Graphics" 或 "on board"
                let is_integrated = check_is_integrated(&device_key, &key_path);
                log::debug!(
                    "显卡注册表: {} is_integrated={}",
                    key_path, is_integrated
                );
                gpu_keys.push((device_key, key_path, is_integrated));
            }
        }
    }
    
    Ok(gpu_keys)
}

/// 根据显卡名称判断是否为核显（Intel 核显 / AMD APU 集成显卡）
#[cfg(target_os = "windows")]
fn is_integrated_by_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    // 明确的中文标记
    if lower.contains("核显") {
        return true;
    }
    // Intel 核显特征：UHD Graphics / HD Graphics / Iris，排除 Intel Arc 独显
    if lower.contains("uhd") || lower.contains("hd graphics") || lower.contains("iris") {
        return true;
    }
    if lower.contains("intel") && lower.contains("graphics") && !lower.contains("arc") {
        return true;
    }
    // AMD APU 核显：Radeon 系列中，含 Graphics 或 Vega 的是核显（APU），
    // 仅 RX 系列是独显。注意 Vega 在 APU 上是核显（如 Vega 8 Graphics），
    // 680M/780M 等带数字的也是 APU 核显——切勿把数字/Vega 当作独显特征。
    if lower.contains("radeon") {
        let is_rx_discrete = lower.contains("rx");
        if !is_rx_discrete && (lower.contains("graphics") || lower.contains("vega")) {
            return true;
        }
    }
    false
}

/// 检查 GPU 是否为核显（集成显卡）
/// 判断优先级：
/// 1. 名称特征判断（Intel UHD/HD/Iris、AMD APU Graphics、中文"核显"）
/// 2. LocationInformation 判断（核显通常标记为 "Internal Graphics" / "on board"）
/// 3. Vendor ID 兜底：若 vendor 为 Intel(8086) 且名称不含 "Arc"，则视为核显
///    （Intel 独显仅有 Arc 系列，其余 Intel 显卡均为核显。
///     此兜底用于应对核显被旧版改名工具改成独显名字后名称判断失效的情况。）
///
/// 注意：不能依据 LocationInformation 是否包含 "bus 0" 来判断 ——
/// 独立显卡的 LocationInformation 通常是 "PCI bus 0, device X, function Y"，同样包含 "bus 0"，
/// 此前该判断会把独显误判为核显，导致显卡列表中只剩核显。
#[cfg(target_os = "windows")]
fn check_is_integrated(device_key: &RegKey, key_path: &str) -> bool {
    let key_path_upper = key_path.to_uppercase();

    // 1. 名称特征判断（Intel UHD/HD/Iris、AMD APU Graphics、中文"核显"）
    let read_name = |key: &RegKey| -> Option<String> {
        if let Ok(name) = key.get_value::<String, _>("FriendlyName") {
            return Some(name);
        }
        key.get_value::<String, _>("DeviceDesc").ok()
    };
    if let Some(name) = read_name(device_key) {
        if is_integrated_by_name(&name) {
            log::debug!("检测到核显(名称): {} ({})", key_path, name);
            return true;
        }
    }

    // 2. LocationInformation 判断（核显通常标记为 "Internal Graphics" / "on board"）
    for instance_result in device_key.enum_keys() {
        let instance_name = match instance_result {
            Ok(name) => name,
            Err(_) => continue,
        };
        if let Ok(instance_key) = device_key.open_subkey(&instance_name) {
            if let Ok(location) = instance_key.get_value::<String, _>("LocationInformation") {
                let lower = location.to_lowercase();
                if lower.contains("internal graphics")
                    || lower.contains("on board")
                    || lower.contains("internal")
                {
                    log::debug!(
                        "检测到核显: {} LocationInformation={}",
                        key_path, location
                    );
                    return true;
                }
            }
        }
    }

    // 3. Vendor ID 兜底：Intel(8086) 非 Arc 独显即为核显
    //    用于应对核显被旧版改名工具改成独显名字后名称判断失效的情况
    if key_path_upper.contains("VEN_8086") {
        let is_arc_by_name = read_name(device_key)
            .map(|n| n.to_lowercase().contains("arc"))
            .unwrap_or(false);
        if !is_arc_by_name {
            log::debug!(
                "检测到核显(Vendor兜底): {} (Intel 非 Arc)",
                key_path
            );
            return true;
        }
    }

    false
}

/// 从注册表键读取显卡名称（FriendlyName 优先，回退 DeviceDesc）
#[cfg(target_os = "windows")]
fn read_gpu_name(key: &RegKey) -> Option<String> {
    if let Ok(name) = key.get_value::<String, _>("FriendlyName") {
        return Some(name);
    }
    if let Ok(name) = key.get_value::<String, _>("DeviceDesc") {
        let parts: Vec<&str> = name.split(';').collect();
        return Some(if parts.len() > 1 { parts[1].to_string() } else { name });
    }
    None
}

/// 应用备份信息到显卡列表：
/// - Multi：按 key_path 精确映射原始名（is_backed_up 与 original_name 恒一致）
/// - Legacy：旧版单条备份无法按 key_path 映射，仅主显卡（优先独显）视为已备份并附上
///   原始名，与 Legacy 恢复逻辑（只恢复第一张独显）保持一致，
///   避免出现「已备份但右侧没有原始显卡」的显示不一致。
#[cfg(target_os = "windows")]
fn apply_backup_to_list(list: &mut Vec<GpuInfo>, backup: &Option<BackupData>) {
    let mut backup_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Some(BackupData::Multi(entries)) = backup {
        for e in entries {
            backup_map.insert(e.key_path.clone(), e.original_name.clone());
        }
    }
    for g in list.iter_mut() {
        let original_name = backup_map.get(&g.key_path).cloned();
        g.original_name = original_name.clone();
        g.is_backed_up = original_name.is_some();
    }
    if let Some(BackupData::Legacy(legacy_name)) = backup {
        let primary_index = list
            .iter()
            .position(|g| !g.is_integrated)
            .unwrap_or(0);
        if let Some(primary) = list.get_mut(primary_index) {
            primary.is_backed_up = true;
            primary.original_name = Some(legacy_name.clone());
        }
    }
}

/// 通过 WMI 获取真实显卡列表（Win32_VideoController 只含显示适配器，不会出现
/// Intel ME/PCIe 根端口等杂项设备），并用每张显卡的 PNPDeviceID 精确定位
/// Enum\PCI 注册表键，实现「通过显卡反查注册表」。
#[cfg(target_os = "windows")]
fn get_gpu_list_from_wmi() -> Vec<GpuInfo> {
    use crate::hardware;

    let static_gpus = hardware::get_gpus_static_from_wmi();
    if static_gpus.is_empty() {
        return Vec::new();
    }

    let backup = load_backup().ok().flatten();

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let mut list: Vec<GpuInfo> = Vec::new();
    for gs in &static_gpus {
        let pnp = gs.pnp_device_id.trim();
        // 只处理 PCI 总线上的真实显卡；ROOT\...（远程/虚拟显示适配器）等直接跳过
        if !pnp.starts_with("PCI\\") {
            log::debug!("跳过非 PCI 显卡(WMI): {}", pnp);
            continue;
        }
        // PNPDeviceID 去掉 "PCI\" 前缀即 Enum\PCI 下的 key_path（vendor\device\instance）
        let key_path = pnp[4..].to_string();
        let device_key = match hklm.open_subkey(format!(
            "SYSTEM\\CurrentControlSet\\Enum\\PCI\\{}",
            key_path
        )) {
            Ok(k) => k,
            Err(e) => {
                log::warn!("WMI 显卡无对应注册表键，跳过: {} ({}) {}", gs.name, key_path, e);
                continue;
            }
        };
        let Some(name) = read_gpu_name(&device_key) else {
            continue;
        };
        list.push(GpuInfo {
            name,
            is_integrated: check_is_integrated(&device_key, &key_path),
            original_name: None,
            is_backed_up: false,
            key_path,
        });
    }

    // 独显在前，核显在后；同类型按原名
    list.sort_by(|a, b| {
        a.is_integrated
            .cmp(&b.is_integrated)
            .then_with(|| a.name.cmp(&b.name))
    });

    // 应用备份信息（Multi 按 key_path 映射；Legacy 仅标记主显卡并附上原始名）
    apply_backup_to_list(&mut list, &backup);

    log::info!(
        "WMI 显卡映射到注册表: {} 张: {:?}",
        list.len(),
        list.iter().map(|g| g.name.as_str()).collect::<Vec<_>>()
    );
    list
}

/// 列出全部显卡（核显 + 独显），独显排前，并关联备份的原始名称。
#[cfg(target_os = "windows")]
fn get_gpu_list_inner() -> Result<Vec<GpuInfo>, String> {
    // 首选 WMI：只枚举真实显卡并用 PNPDeviceID 精确定位注册表键
    let wmi_list = get_gpu_list_from_wmi();
    if !wmi_list.is_empty() {
        return Ok(wmi_list);
    }
    log::warn!("WMI 未取到可用显卡，回退到注册表 Display 类枚举");

    let gpu_keys = find_gpu_registry_keys()?;
    if gpu_keys.is_empty() {
        return Err("未找到显卡注册表信息".to_string());
    }

    // 备份信息由 apply_backup_to_list 统一处理（Multi 按 key_path 映射；Legacy 仅主显卡）
    let backup = load_backup()?;

    let mut list: Vec<GpuInfo> = Vec::new();
    for (key, key_path, is_integrated) in &gpu_keys {
        let Some(name) = read_gpu_name(key) else {
            continue;
        };
        // 双保险：跳过 Microsoft 基础显示适配器（未安装驱动的占位设备）
        if name.to_lowercase().contains("basic display") {
            log::info!("跳过基础显示适配器(列表): {} ({})", name, key_path);
            continue;
        }
        list.push(GpuInfo {
            name,
            is_integrated: *is_integrated,
            original_name: None,
            is_backed_up: false,
            key_path: key_path.clone(),
        });
    }

    // 独显在前，核显在后；同类型按原名
    list.sort_by(|a, b| {
        a.is_integrated
            .cmp(&b.is_integrated)
            .then_with(|| a.name.cmp(&b.name))
    });

    // 应用备份信息（Multi 按 key_path 映射；Legacy 仅标记主显卡并附上原始名）
    apply_backup_to_list(&mut list, &backup);

    Ok(list)
}

#[cfg(not(target_os = "windows"))]
fn get_gpu_list_inner() -> Result<Vec<GpuInfo>, String> {
    Err("此功能仅支持 Windows 系统".to_string())
}

#[cfg(target_os = "windows")]
fn rename_gpu(new_name: &str, target_key_path: &str) -> Result<(), String> {
    log::info!(
        "开始修改显卡 {} 的名称为: {}",
        target_key_path, new_name
    );

    // 策略：只改写 target_key_path 指定的显卡，避免影响其他显卡
    // 1. Enum\PCI：通过 vendor\device 精确匹配 target_key_path
    // 2. Class：通过 MatchingDeviceId 与目标显卡一致来匹配
    // 3. Video：通过当前名称与目标显卡当前名称一致来匹配
    // 原生 winreg 直写，不再启动 PowerShell（更快、无弹进程开销）
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    // 打开目标显卡的 Enum\PCI 键（需写权限）
    let enum_path = format!("SYSTEM\\CurrentControlSet\\Enum\\PCI\\{}", target_key_path);
    let target_key = hklm
        .open_subkey_with_flags(&enum_path, KEY_READ | KEY_WRITE)
        .map_err(|e| format!("打开目标显卡注册表键失败: {} ({})", e, enum_path))?;

    // 读取目标显卡当前名称与 MatchingDeviceId，用于 Class/Video 精确匹配
    let target_current_name =
        read_gpu_name(&target_key).ok_or_else(|| "读取目标显卡当前名称失败".to_string())?;
    let target_matching_id: Option<String> = target_key.get_value("MatchingDeviceId").ok();

    log::info!("目标显卡: {} 当前名称: {}", target_key_path, target_current_name);

    let mut modified = false;

    // 1. 修改 Enum\PCI 下目标显卡的 FriendlyName / DeviceDesc
    match target_key.set_value("FriendlyName", &new_name) {
        Ok(_) => {
            log::info!("成功修改 FriendlyName");
            modified = true;
        }
        Err(e) => log::warn!("修改 FriendlyName 失败: {}", e),
    }
    if let Ok(device_desc) = target_key.get_value::<String, _>("DeviceDesc") {
        // 带分号时保留硬件路径前缀，重写后半段名称；不带分号（部分机器 DeviceDesc 为纯名称）则直接整体改写
        let new_desc = if let Some((prefix, _)) = device_desc.split_once(';') {
            format!("{};{}", prefix, new_name)
        } else {
            new_name.to_string()
        };
        match target_key.set_value("DeviceDesc", &new_desc) {
            Ok(_) => {
                log::info!("成功修改 DeviceDesc");
                modified = true;
            }
            Err(e) => log::warn!("修改 DeviceDesc 失败: {}", e),
        }
    }

    // 2. 修改 Class 下匹配的显卡键（通过 MatchingDeviceId 精确匹配）
    if let Some(mid) = target_matching_id {
        let class_path =
            r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
        if let Ok(class_key) = hklm.open_subkey_with_flags(class_path, KEY_READ | KEY_WRITE) {
            for sub in class_key.enum_keys().filter_map(Result::ok) {
                // 只处理形如 00XX 的驱动键（Properties/Configuration 等非数字子键跳过）
                if !sub.starts_with("00")
                    || sub.len() < 3
                    || !sub[2..].chars().all(|c| c.is_ascii_digit())
                {
                    continue;
                }
                if let Ok(sub_key) = class_key.open_subkey_with_flags(&sub, KEY_READ | KEY_WRITE) {
                    if let Ok(smid) = sub_key.get_value::<String, _>("MatchingDeviceId") {
                        if smid == mid {
                            match sub_key.set_value("DriverDesc", &new_name) {
                                Ok(_) => {
                                    log::info!("找到目标显卡 Class 键: {} 成功修改 DriverDesc", sub);
                                    modified = true;
                                }
                                Err(e) => log::warn!("修改 Class DriverDesc 失败: {} {}", sub, e),
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. 修改 Control\Video 下匹配的显卡键（任一名称字段等于目标当前名称时改写）
    let video_path = r"SYSTEM\CurrentControlSet\Control\Video";
    if let Ok(video_key) = hklm.open_subkey(video_path) {
        for guid in video_key.enum_keys().filter_map(Result::ok) {
            if let Ok(guid_key) = video_key.open_subkey(&guid) {
                for sub in guid_key.enum_keys().filter_map(Result::ok) {
                    if let Ok(sub_key) = guid_key.open_subkey_with_flags(&sub, KEY_READ | KEY_WRITE) {
                        let fields = [
                            sub_key.get_value::<String, _>("DriverDesc").ok(),
                            sub_key.get_value::<String, _>("DeviceDesc").ok(),
                            sub_key.get_value::<String, _>("Description").ok(),
                            sub_key.get_value::<String, _>("FriendlyName").ok(),
                        ];
                        if fields.iter().flatten().any(|v| *v == target_current_name) {
                            for n in ["DriverDesc", "DeviceDesc", "Description", "FriendlyName"] {
                                if sub_key.get_value::<String, _>(n).is_ok() {
                                    match sub_key.set_value(n, &new_name) {
                                        Ok(_) => {
                                            log::info!(
                                                "找到目标显卡 Video 键: {}\\{} 成功修改 {}",
                                                guid, sub, n
                                            );
                                            modified = true;
                                        }
                                        Err(e) => log::warn!("修改 Video {} 失败: {} {}", n, sub, e),
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if modified {
        log::info!("显卡名称修改成功！");
        Ok(())
    } else {
        Err("未能修改任何显卡注册表键".to_string())
    }
}

#[cfg(not(target_os = "windows"))]
fn rename_gpu(_new_name: &str, _target_key_path: &str) -> Result<(), String> {
    Err("此功能仅支持 Windows 系统".to_string())
}

/// 获取全部显卡列表（核显 + 独显，独显在前）
#[tauri::command]
pub async fn get_gpu_list() -> Result<Vec<GpuInfo>, String> {
    get_gpu_list_inner()
}

/// 获取默认显卡（优先独显），用于兼容旧调用方
#[tauri::command]
pub async fn get_gpu_info() -> Result<GpuInfo, String> {
    let list = get_gpu_list_inner()?;
    list.iter()
        .find(|g| !g.is_integrated)
        .or_else(|| list.first())
        .cloned()
        .ok_or_else(|| "未找到显卡".to_string())
}

#[tauri::command]
pub async fn get_gpu_options() -> Result<Vec<GpuOption>, String> {
    Ok(vec![
        // 低端显卡（NVIDIA）
        GpuOption {
            id: "gtx650".to_string(),
            name: "NVIDIA GeForce GTX 650".to_string(),
            category: "low-end".to_string(),
        },
        GpuOption {
            id: "gtx750".to_string(),
            name: "NVIDIA GeForce GTX 750".to_string(),
            category: "low-end".to_string(),
        },
        GpuOption {
            id: "gtx750ti".to_string(),
            name: "NVIDIA GeForce GTX 750 Ti".to_string(),
            category: "low-end".to_string(),
        },
        GpuOption {
            id: "gtx1050".to_string(),
            name: "NVIDIA GeForce GTX 1050".to_string(),
            category: "low-end".to_string(),
        },
        GpuOption {
            id: "gtx1050ti".to_string(),
            name: "NVIDIA GeForce GTX 1050 Ti".to_string(),
            category: "low-end".to_string(),
        },
        // 低端显卡（AMD）
        GpuOption {
            id: "r7240".to_string(),
            name: "AMD Radeon R7 240".to_string(),
            category: "low-end".to_string(),
        },
        GpuOption {
            id: "rx460".to_string(),
            name: "AMD Radeon RX 460".to_string(),
            category: "low-end".to_string(),
        },
        GpuOption {
            id: "rx560".to_string(),
            name: "AMD Radeon RX 560".to_string(),
            category: "low-end".to_string(),
        },
        GpuOption {
            id: "rx550".to_string(),
            name: "AMD Radeon RX 550".to_string(),
            category: "low-end".to_string(),
        },
        GpuOption {
            id: "rx570".to_string(),
            name: "AMD Radeon RX 570".to_string(),
            category: "low-end".to_string(),
        },
        GpuOption {
            id: "rx580".to_string(),
            name: "AMD Radeon RX 580".to_string(),
            category: "low-end".to_string(),
        },
        GpuOption {
            id: "rx590".to_string(),
            name: "AMD Radeon RX 590".to_string(),
            category: "low-end".to_string(),
        },
        // 高端显卡（NVIDIA）
        GpuOption {
            id: "rtx4080".to_string(),
            name: "NVIDIA GeForce RTX 4080".to_string(),
            category: "high-end".to_string(),
        },
        GpuOption {
            id: "rtx4090".to_string(),
            name: "NVIDIA GeForce RTX 4090".to_string(),
            category: "high-end".to_string(),
        },
        GpuOption {
            id: "rtx5080".to_string(),
            name: "NVIDIA GeForce RTX 5080".to_string(),
            category: "high-end".to_string(),
        },
        GpuOption {
            id: "rtx5090".to_string(),
            name: "NVIDIA GeForce RTX 5090".to_string(),
            category: "high-end".to_string(),
        },
        // 高端显卡（AMD）
        GpuOption {
            id: "rx6700xt".to_string(),
            name: "AMD Radeon RX 6700 XT".to_string(),
            category: "high-end".to_string(),
        },
        GpuOption {
            id: "rx6750gre".to_string(),
            name: "AMD Radeon RX 6750 GRE".to_string(),
            category: "high-end".to_string(),
        },
        GpuOption {
            id: "rx6800".to_string(),
            name: "AMD Radeon RX 6800".to_string(),
            category: "high-end".to_string(),
        },
        GpuOption {
            id: "rx6800xt".to_string(),
            name: "AMD Radeon RX 6800 XT".to_string(),
            category: "high-end".to_string(),
        },
        GpuOption {
            id: "rx6900xt".to_string(),
            name: "AMD Radeon RX 6900 XT".to_string(),
            category: "high-end".to_string(),
        },
        GpuOption {
            id: "rx7600".to_string(),
            name: "AMD Radeon RX 7600".to_string(),
            category: "high-end".to_string(),
        },
        GpuOption {
            id: "rx7700xt".to_string(),
            name: "AMD Radeon RX 7700 XT".to_string(),
            category: "high-end".to_string(),
        },
        GpuOption {
            id: "rx7800xt".to_string(),
            name: "AMD Radeon RX 7800 XT".to_string(),
            category: "high-end".to_string(),
        },
        GpuOption {
            id: "rx7900gre".to_string(),
            name: "AMD Radeon RX 7900 GRE".to_string(),
            category: "high-end".to_string(),
        },
        GpuOption {
            id: "rx7900xt".to_string(),
            name: "AMD Radeon RX 7900 XT".to_string(),
            category: "high-end".to_string(),
        },
        GpuOption {
            id: "rx7900xtx".to_string(),
            name: "AMD Radeon RX 7900 XTX".to_string(),
            category: "high-end".to_string(),
        },
        GpuOption {
            id: "rx9060xt".to_string(),
            name: "AMD Radeon RX 9060 XT".to_string(),
            category: "high-end".to_string(),
        },
        GpuOption {
            id: "rx9070xt".to_string(),
            name: "AMD Radeon RX 9070 XT".to_string(),
            category: "high-end".to_string(),
        },
    ])
}

#[cfg(target_os = "windows")]
#[tauri::command]
pub async fn apply_gpu_rename(
    new_name: String,
    target_key_path: String,
) -> Result<GpuRenameResult, String> {
    if target_key_path.trim().is_empty() {
        return Err("未指定要改写的显卡".to_string());
    }

    let backup = load_backup()?;

    // 已有备份：Multi 按 key_path 复用；Legacy 旧版无法按 key_path 映射，或首次无备份，则重建
    let mut entries: Vec<GpuBackupEntry> = match backup {
        Some(BackupData::Multi(e)) => e,
        Some(BackupData::Legacy(_)) | None => Vec::new(),
    };

    // 备份为空（首次应用 / 旧版格式）时，先枚举全部显卡（按 key_path 区分核显/独显）
    if entries.is_empty() {
        let gpu_keys = find_gpu_registry_keys()?;
        entries = gpu_keys
            .iter()
            .filter_map(|(key, key_path, is_integrated)| {
                read_gpu_name(key).map(|name| GpuBackupEntry {
                    key_path: key_path.clone(),
                    original_name: name,
                    is_integrated: *is_integrated,
                })
            })
            .collect();
    }

    // 确保本次要改写的显卡一定在备份中。
    // find_gpu_registry_keys 依赖 ClassGUID/名称关键词启发式，个别机器可能漏检目标显卡
    // （WMI 能列出但不被其识别），导致改写成功却没有该显卡的备份、无法恢复。
    if !entries.iter().any(|e| e.key_path == target_key_path) {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let enum_path = format!("SYSTEM\\CurrentControlSet\\Enum\\PCI\\{}", target_key_path);
        if let Ok(target_key) = hklm.open_subkey(&enum_path) {
            if let Some(name) = read_gpu_name(&target_key) {
                entries.push(GpuBackupEntry {
                    key_path: target_key_path.clone(),
                    original_name: name,
                    is_integrated: check_is_integrated(&target_key, &target_key_path),
                });
            }
        }
    }

    if entries.is_empty() {
        return Err("未找到可备份的显卡".to_string());
    }
    save_backup(&entries)?;

    rename_gpu(&new_name, &target_key_path)?;

    Ok(GpuRenameResult {
        success: true,
        message: format!("显卡名称已更改为: {}", new_name),
    })
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub async fn apply_gpu_rename(
    new_name: String,
    target_key_path: String,
) -> Result<GpuRenameResult, String> {
    let _ = (new_name, target_key_path);
    Err("此功能仅支持 Windows 系统".to_string())
}

/// 按备份逐张恢复 Enum\PCI 下的显卡键，Class/Video 驱动键按 MatchingDeviceId 关联恢复。
#[cfg(target_os = "windows")]
fn restore_gpu_by_entries(entries: &[GpuBackupEntry]) -> Result<(), String> {
    // 恢复兜底名称：优先第一张独显的原始名，否则第一条记录（用于 Video 下无法精确匹配的场景）
    let fallback = entries
        .iter()
        .find(|e| !e.is_integrated)
        .or_else(|| entries.first())
        .map(|e| e.original_name.as_str())
        .unwrap_or("")
        .to_string();

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    // 构建 key_path -> original_name 与 matching_id -> original_name 两个映射
    let mut key_path_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut matching_id_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for e in entries {
        key_path_map.insert(e.key_path.clone(), e.original_name.clone());
        let enum_path = format!("SYSTEM\\CurrentControlSet\\Enum\\PCI\\{}", e.key_path);
        if let Ok(k) = hklm.open_subkey(&enum_path) {
            if let Ok(mid) = k.get_value::<String, _>("MatchingDeviceId") {
                matching_id_map.insert(mid, e.original_name.clone());
            }
        }
    }

    // 1. 按 key_path 精确恢复 Enum\PCI 下每张显卡
    let pci_path = r"SYSTEM\CurrentControlSet\Enum\PCI";
    if let Ok(pci_key) = hklm.open_subkey(pci_path) {
        for vendor in pci_key.enum_keys().filter_map(Result::ok) {
            let vupper = vendor.to_uppercase();
            if !(vupper.contains("VEN_10DE")
                || vupper.contains("VEN_1002")
                || vupper.contains("VEN_8086"))
            {
                continue;
            }
            if let Ok(vendor_key) = pci_key.open_subkey(&vendor) {
                for device in vendor_key.enum_keys().filter_map(Result::ok) {
                    let key_path = format!("{}\\{}", vendor, device);
                    if let Some(orig) = key_path_map.get(&key_path) {
                        if let Ok(device_key) =
                            vendor_key.open_subkey_with_flags(&device, KEY_READ | KEY_WRITE)
                        {
                            match device_key.set_value("FriendlyName", orig) {
                                Ok(_) => log::info!("恢复 FriendlyName: {} -> {}", key_path, orig),
                                Err(e) => log::warn!("恢复 FriendlyName 失败: {} ({})", key_path, e),
                            }
                            if let Ok(desc) = device_key.get_value::<String, _>("DeviceDesc") {
                                let new_desc = if let Some((prefix, _)) = desc.split_once(';') {
                                    format!("{};{}", prefix, orig)
                                } else {
                                    orig.to_string()
                                };
                                let _ = device_key.set_value("DeviceDesc", &new_desc);
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. 通过 MatchingDeviceId 精确恢复 Class 下显卡驱动键
    let class_path =
        r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
    if let Ok(class_key) = hklm.open_subkey_with_flags(class_path, KEY_READ | KEY_WRITE) {
        for sub in class_key.enum_keys().filter_map(Result::ok) {
            if !sub.starts_with("00")
                || sub.len() < 3
                || !sub[2..].chars().all(|c| c.is_ascii_digit())
            {
                continue;
            }
            if let Ok(sub_key) = class_key.open_subkey_with_flags(&sub, KEY_READ | KEY_WRITE) {
                if let Ok(mid) = sub_key.get_value::<String, _>("MatchingDeviceId") {
                    if let Some(orig) = matching_id_map.get(&mid) {
                        match sub_key.set_value("DriverDesc", orig) {
                            Ok(_) => log::info!("恢复 Class DriverDesc: {} -> {}", sub, orig),
                            Err(e) => log::warn!("恢复 Class DriverDesc 失败: {} ({})", sub, e),
                        }
                    }
                }
            }
        }
    }

    // 3. 恢复 Control\Video 下显卡键（Video 下无 MatchingDeviceId，使用 fallback 恢复 GPU 类子键）
    let video_path = r"SYSTEM\CurrentControlSet\Control\Video";
    if let Ok(video_key) = hklm.open_subkey(video_path) {
        for guid in video_key.enum_keys().filter_map(Result::ok) {
            if let Ok(guid_key) = video_key.open_subkey(&guid) {
                for sub in guid_key.enum_keys().filter_map(Result::ok) {
                    if let Ok(sub_key) = guid_key.open_subkey_with_flags(&sub, KEY_READ | KEY_WRITE) {
                        let d1 = sub_key.get_value::<String, _>("DriverDesc").unwrap_or_default();
                        let d2 = sub_key.get_value::<String, _>("DeviceDesc").unwrap_or_default();
                        let d3 = sub_key.get_value::<String, _>("Description").unwrap_or_default();
                        let check = format!("{} {} {}", d1, d2, d3).to_lowercase();
                        let is_gpu = [
                            "nvidia", "geforce", "gtx", "rtx", "amd", "radeon", "intel",
                            "uhd graphics", "iris", "hd graphics",
                        ]
                        .iter()
                        .any(|kw| check.contains(kw));
                        if is_gpu {
                            for n in ["DriverDesc", "DeviceDesc", "Description", "FriendlyName"] {
                                if sub_key.get_value::<String, _>(n).is_ok() {
                                    let _ = sub_key.set_value(n, &fallback);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    log::info!("显卡名称恢复完成");
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn restore_gpu_by_entries(_entries: &[GpuBackupEntry]) -> Result<(), String> {
    Err("此功能仅支持 Windows 系统".to_string())
}

#[tauri::command]
pub async fn restore_gpu_name() -> Result<GpuRenameResult, String> {
    let backup = load_backup()?;

    let result = match backup {
        Some(BackupData::Multi(entries)) => {
            restore_gpu_by_entries(&entries)?;
            GpuRenameResult {
                success: true,
                message: format!("显卡名称已恢复为原始名称（共 {} 张显卡）", entries.len()),
            }
        }
        Some(BackupData::Legacy(original_name)) => {
            // Legacy 备份无 key_path 信息，只能恢复第一张 GPU（优先独显）
            #[cfg(target_os = "windows")]
            {
                let gpu_keys = find_gpu_registry_keys()?;
                let target_key_path = gpu_keys
                    .iter()
                    .find(|(_, _, is_integrated)| !is_integrated)
                    .or_else(|| gpu_keys.first())
                    .map(|(_, key_path, _)| key_path.clone())
                    .ok_or_else(|| "未找到可恢复的显卡".to_string())?;
                rename_gpu(&original_name, &target_key_path)?;
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = original_name;
                return Err("此功能仅支持 Windows 系统".to_string());
            }
            GpuRenameResult {
                success: true,
                message: format!("显卡名称已恢复为: {}", original_name),
            }
        }
        None => {
            return Ok(GpuRenameResult {
                success: false,
                message: "未找到备份文件，无法恢复".to_string(),
            })
        }
    };

    // 删除两处的备份文件
    if let Ok(appdata_path) = get_appdata_backup_path() {
        let _ = fs::remove_file(appdata_path);
    }
    if let Some(install_path) = get_install_dir_backup_path() {
        let _ = fs::remove_file(install_path);
    }

    Ok(result)
}
