//! 显示器信息公共缓存
//!
//! 多个页面（滤镜、准心、分辨率管理）都需要获取显示器型号。
//! 通过直接读取注册表中的 EDID 数据并解析显示器名称，
//! 速度极快（纯内存/注册表操作，不启动任何进程）。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 缓存有效期。
///
/// 显示器配置在一次使用会话中基本不会变化，60 秒足够安全；
/// 同时保证热插拔后一段时间能自动刷新。
const CACHE_TTL: Duration = Duration::from_secs(60);

struct EdidCache {
    fetched_at: Option<Instant>,
    /// (PNP ID, 完整 EDID 信息) 列表。PNP ID 用于按设备 ID 精确匹配，
    /// 避免不同 API 的枚举顺序不一致导致型号张冠李戴。
    entries: Vec<(String, EdidMonitorInfo)>,
}

/// 注册表 EDID 解析出的完整显示器信息。
///
/// 与 WMI WmiMonitorID / WmiMonitorBasicDisplayParams 同源（都来自 EDID），
/// 但无需 COM/WMI，纯注册表读取，速度更快且不受 WMI 服务异常影响。
#[derive(Debug, Clone, Default)]
pub struct EdidMonitorInfo {
    /// EDID 名称描述符（Tag 0xFC），如 "Mi Monitor"、"U24PF14"
    pub name: String,
    /// EDID 厂商码（bytes 8-9 解出，如 "XMI"、"SKY"）
    pub manufacturer_code: String,
    /// 序列号文本（描述符 0xFF 或数值序列号的十进制形式，与 WmiMonitorID 一致）
    pub serial: String,
    /// 最大水平图像尺寸（厘米），对应 WmiMonitorBasicDisplayParams.MaxHorizontalImageSize
    pub max_h_cm: u32,
    /// 最大垂直图像尺寸（厘米），对应 WmiMonitorBasicDisplayParams.MaxVerticalImageSize
    pub max_v_cm: u32,
}

static EDID_CACHE: Mutex<EdidCache> = Mutex::new(EdidCache {
    fetched_at: None,
    entries: Vec::new(),
});

/// 从原始 (PNP ID, EDID 信息) 条目构建型号名称列表（按名称去重，保留首次出现），
/// 与历史 `get_edid_monitor_names` 行为一致。
fn build_names(entries: &[(String, EdidMonitorInfo)]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut names = Vec::new();
    for (_, info) in entries {
        if info.name.is_empty() {
            continue;
        }
        if seen.insert(info.name.clone()) {
            names.push(info.name.clone());
        }
    }
    names
}

/// 获取 EDID 显示器型号名称（带 TTL 缓存）。
///
/// 通过读注册表 HKLM\SYSTEM\CurrentControlSet\Enum\DISPLAY\...\Device Parameters\EDID
/// 直接解析 EDID 中的 Monitor Name (Descriptor Tag 0xFC)，不启动任何外部进程。
pub fn get_edid_monitor_names() -> Vec<String> {
    // 先尝试读缓存
    {
        let lock = EDID_CACHE.lock().unwrap();
        if let Some(t) = lock.fetched_at {
            if t.elapsed() < CACHE_TTL {
                return build_names(&lock.entries);
            }
        }
    }

    // 缓存过期或不存在，重新查询
    log::info!("display_cache: EDID 缓存未命中，从注册表读取 EDID…");
    let entries = query_edid_via_registry();
    log::info!("display_cache: 注册表 EDID 查询完成，获取到 {} 个名称", entries.len());

    let mut lock = EDID_CACHE.lock().unwrap();
    lock.fetched_at = Some(Instant::now());
    lock.entries = entries;
    build_names(&lock.entries)
}

/// 获取 PNP ID -> 显示器型号名称 的映射（带 TTL 缓存）。
///
/// 用于按显示器的 PNP 设备 ID 精确匹配型号，避免 WMI 顺序与注册表 EDID
/// 顺序不一致时把 A 显示器的型号错配到 B 显示器（型号颠倒）的问题。
pub fn get_edid_monitor_names_by_pnpid() -> HashMap<String, String> {
    {
        let lock = EDID_CACHE.lock().unwrap();
        if let Some(t) = lock.fetched_at {
            if t.elapsed() < CACHE_TTL {
                return lock
                    .entries
                    .iter()
                    .filter(|(_, info)| !info.name.is_empty())
                    .map(|(pnp, info)| (pnp.clone(), info.name.clone()))
                    .collect();
            }
        }
    }
    // 触发一次查询（同时填充缓存）
    get_edid_monitor_names();
    let lock = EDID_CACHE.lock().unwrap();
    lock.entries
        .iter()
        .filter(|(_, info)| !info.name.is_empty())
        .map(|(pnp, info)| (pnp.clone(), info.name.clone()))
        .collect()
}

/// 获取 PNP ID -> 完整 EDID 显示器信息 的映射（带 TTL 缓存）。
///
/// 供硬件信息页等使用：包含名称、厂商码、序列号、物理尺寸。
pub fn get_edid_monitor_infos_by_pnpid() -> HashMap<String, EdidMonitorInfo> {
    {
        let lock = EDID_CACHE.lock().unwrap();
        if let Some(t) = lock.fetched_at {
            if t.elapsed() < CACHE_TTL {
                return lock.entries.iter().cloned().collect();
            }
        }
    }
    // 触发一次查询（同时填充缓存）
    get_edid_monitor_names();
    let lock = EDID_CACHE.lock().unwrap();
    lock.entries.iter().cloned().collect()
}

/// 通过 EnumDisplayDevicesW 获取指定显示设备（如 "\\.\DISPLAY1"）的 PNP ID
/// （如 "DELA409"），用于按设备 ID 精确匹配 EDID 型号，
/// 避免不同 API 枚举顺序不一致导致型号张冠李戴。
#[cfg(target_os = "windows")]
pub fn get_pnp_id_for_device(device_name: &str) -> Option<String> {
    use std::mem;
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplayDevicesW, DISPLAY_DEVICEW};

    unsafe {
        let device_name_wide: Vec<u16> = device_name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut disp_device: DISPLAY_DEVICEW = mem::zeroed();
        disp_device.cb = mem::size_of::<DISPLAY_DEVICEW>() as u32;

        if EnumDisplayDevicesW(device_name_wide.as_ptr(), 0, &mut disp_device, 0) != 0 {
            let len = disp_device
                .DeviceID
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(disp_device.DeviceID.len());
            if len > 0 {
                let device_id = String::from_utf16_lossy(&disp_device.DeviceID[..len]);
                // DeviceID 格式: "MONITOR\PNPID\..."，提取 PNPID
                let prefix = "MONITOR\\";
                if let Some(pnp_start) = device_id.find(prefix) {
                    let after_prefix = &device_id[pnp_start + prefix.len()..];
                    if let Some(backslash_pos) = after_prefix.find('\\') {
                        return Some(after_prefix[..backslash_pos].to_string());
                    }
                    // 没有反斜杠时取到末尾
                    if !after_prefix.is_empty() {
                        return Some(after_prefix.to_string());
                    }
                }
            }
        }
    }
    None
}

#[cfg(not(target_os = "windows"))]
pub fn get_pnp_id_for_device(_device_name: &str) -> Option<String> {
    None
}

/// 强制刷新缓存（例如显示器配置变化时调用）。
#[allow(dead_code)]
pub fn invalidate() {
    let mut lock = EDID_CACHE.lock().unwrap();
    lock.fetched_at = None;
    lock.entries.clear();
}

/// 通过注册表枚举所有显示器的 EDID，解析出完整信息（名称/厂商码/序列号/尺寸）。
///
/// 返回 (PNP ID, EDID 信息) 列表。
/// 路径: HKLM\SYSTEM\CurrentControlSet\Enum\DISPLAY\<PNPID>\<InstanceID>\Device Parameters\EDID
#[cfg(target_os = "windows")]
fn query_edid_via_registry() -> Vec<(String, EdidMonitorInfo)> {
    use winreg::enums::*;
    use winreg::RegKey;

    let enum_key = match RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(
            r"SYSTEM\CurrentControlSet\Enum\DISPLAY",
            KEY_READ,
        ) {
        Ok(k) => k,
        Err(e) => {
            log::warn!("display_cache: 无法打开 DISPLAY 注册表项: {}", e);
            return Vec::new();
        }
    };

    let mut entries = Vec::new();

    // 遍历每个 PNP ID 子项（如 "DELA409", "SAM0F9E" 等）
    for pnp_result in enum_key.enum_keys() {
        let pnp_id = match pnp_result {
            Ok(id) => id,
            Err(_) => continue,
        };

        let instance_key = match enum_key.open_subkey_with_flags(&pnp_id, KEY_READ) {
            Ok(k) => k,
            Err(_) => continue,
        };

        // 遍历每个实例子项
        for inst_result in instance_key.enum_keys() {
            let instance_id = match inst_result {
                Ok(id) => id,
                Err(_) => continue,
            };

            // 读取 EDID 数据
            let edid_path = format!(r"{}\Device Parameters", instance_id);
            let dev_params = match instance_key.open_subkey_with_flags(&edid_path, KEY_READ) {
                Ok(k) => k,
                Err(_) => continue,
            };

            let edid_bytes = match dev_params.get_raw_value("EDID") {
                Ok(raw) => raw.bytes,
                Err(_) => continue,
            };

            if let Some(info) = parse_edid_monitor_info(&edid_bytes) {
                entries.push((pnp_id.clone(), info));
            }
        }
    }

    log::info!("display_cache: 注册表 EDID 扫描完成，找到 {} 个显示器", entries.len());
    entries
}

#[cfg(not(target_os = "windows"))]
fn query_edid_via_registry() -> Vec<(String, EdidMonitorInfo)> {
    Vec::new()
}

/// 从 EDID 原始二进制数据中解析完整显示器信息。
///
/// EDID 128 字节块布局：
/// - bytes 8-9：厂商码（5+6+5 位大端打包，如 0x0AF3 → "SKY"）
/// - bytes 12-15：数值序列号（小端 u32）
/// - bytes 21/22：最大水平/垂直图像尺寸（厘米），对应 WmiMonitorBasicDisplayParams
/// - 4 个描述符块（0x36 起，每个 18 字节）：Tag 0xFC = Monitor Name，0xFF = Serial Number
fn parse_edid_monitor_info(edid: &[u8]) -> Option<EdidMonitorInfo> {
    if edid.len() < 128 {
        return None;
    }

    // 检查 EDID 头标识
    if edid[0] != 0x00 || edid[1] != 0xFF || edid[2] != 0xFF || edid[3] != 0xFF
        || edid[4] != 0xFF || edid[5] != 0xFF || edid[6] != 0xFF || edid[7] != 0x00
    {
        log::warn!("display_cache: EDID 头校验失败，跳过");
        return None;
    }

    let mut info = EdidMonitorInfo::default();

    // 厂商码：bytes 8-9 大端 16 位，3 个 5 位字母（1-26 → 'A'-'Z'）
    let word = ((edid[8] as u16) << 8) | edid[9] as u16;
    let mfr_code: String = [(word >> 10) & 0x1F, (word >> 5) & 0x1F, word & 0x1F]
        .iter()
        .filter_map(|&v| {
            if (1..=26).contains(&v) {
                Some(char::from(b'A' + (v - 1) as u8))
            } else {
                None
            }
        })
        .collect();
    info.manufacturer_code = mfr_code;

    // 数值序列号（小端 u32），WmiMonitorID.SerialNumberID 的十进制形式即来源于此
    let numeric_serial = u32::from_le_bytes([edid[12], edid[13], edid[14], edid[15]]);

    // 4 个描述符块，每个 18 字节：[0..2]=0x0000 标志, [3]=tag, [5..18]=文本
    for block_idx in 0..4 {
        let offset = 0x36 + block_idx * 18;
        if offset + 18 > edid.len() {
            break;
        }
        if edid[offset] != 0x00 || edid[offset + 1] != 0x00 {
            continue;
        }
        let tag = edid[offset + 3];
        let text: String = edid[offset + 5..offset + 18]
            .iter()
            .take_while(|&&b| b != 0x0A && b != 0x00)
            .map(|&b| b as char)
            .collect();
        let text = text.trim().to_string();
        match tag {
            0xFC if info.name.is_empty() => {
                if !text.is_empty() {
                    log::info!(
                        "display_cache: EDID 解析到 Monitor Name: '{}' (block={})",
                        text, block_idx
                    );
                }
                info.name = text;
            }
            0xFF if info.serial.is_empty() => info.serial = text,
            _ => {}
        }
    }

    // 无文本序列号描述符时，用数值序列号的十进制形式（与 WmiMonitorID 输出一致）
    if info.serial.is_empty() && numeric_serial != 0 {
        info.serial = numeric_serial.to_string();
    }

    // 物理尺寸（厘米）
    info.max_h_cm = edid[21] as u32;
    info.max_v_cm = edid[22] as u32;

    Some(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edid_parse_real_data() {
        // 构造一个包含 Monitor Name 描述符的最小 EDID
        let mut edid = vec![0u8; 128];
        // EDID 头
        edid[0..8].copy_from_slice(&[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00]);
        // 厂商码：SKY → S=19, K=11, Y=25
        let word: u16 = ((19 - 1) << 10) | ((11 - 1) << 5) | (25 - 1);
        edid[8] = (word >> 8) as u8;
        edid[9] = (word & 0xFF) as u8;
        // 数值序列号 16843009 = 0x01010101
        edid[12..16].copy_from_slice(&16843009u32.to_le_bytes());
        // 物理尺寸 53 x 30 cm
        edid[21] = 53;
        edid[22] = 30;
        // 第一个描述符块 (offset 0x36): Monitor Name (tag 0xFC)
        // 18 字节描述符布局: [0..2]=0x0000 标志, [2]=保留, [3]=tag, [4]=保留,
        // [5..18]=13 字节名称。名称必须从 offset+5 开始，写进 [3] 会覆盖 tag。
        edid[0x36 + 3] = 0xFC;
        let name_bytes = b"DELL S2721QS";
        let name_len = name_bytes.len().min(13);
        edid[0x36 + 5..0x36 + 5 + name_len].copy_from_slice(&name_bytes[..name_len]);

        let result = parse_edid_monitor_info(&edid).expect("应解析成功");
        assert_eq!(result.name, "DELL S2721QS");
        assert_eq!(result.manufacturer_code, "SKY");
        assert_eq!(result.serial, "16843009");
        assert_eq!(result.max_h_cm, 53);
        assert_eq!(result.max_v_cm, 30);
    }

    #[test]
    fn test_edid_parse_no_name() {
        let mut edid = vec![0u8; 128];
        edid[0..8].copy_from_slice(&[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00]);
        // 所有描述符都是其他类型 (tag != 0xFC)
        edid[0x36 + 3] = 0xFF;
        edid[0x48 + 3] = 0xFD;
        edid[0x5A + 3] = 0xFC; // Monitor name but empty
        edid[0x6C + 3] = 0x10;

        let result = parse_edid_monitor_info(&edid).expect("头合法应返回 Some");
        assert_eq!(result.name, "");
    }
}
