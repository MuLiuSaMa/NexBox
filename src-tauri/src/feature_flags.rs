// NexBox - Windows 隐藏功能开关（Velocity 功能配置）模块
// 核心实现移植自 ViVe（https://github.com/thebookisclosed/ViVe）
// Copyright (C) 2019-2025 @thebookisclosed，GPL-3.0
// 通过 ntdll 的功能配置 API 与 FeatureManagement 注册表操作，
// 查询 / 启用 / 禁用 / 重置 Windows 的 A/B 实验功能开关。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use libloading::Library;
use serde::Serialize;
use tauri::{AppHandle, Manager};

// ============ 常量（对照 ViVe NativeEnums.cs） ============

const CFG_TYPE_BOOT: u32 = 0;
const CFG_TYPE_RUNTIME: u32 = 1;

pub const STATE_DEFAULT: u32 = 0;
pub const STATE_DISABLED: u32 = 1;
pub const STATE_ENABLED: u32 = 2;

/// 默认写入优先级：User（ViVeTool /enable 同款默认值）
const PRIORITY_USER: u32 = 8;
/// 内核不允许写入的不可变优先级（对照 FeatureManager.ImmutablePriorities）
const IMMUTABLE_PRIORITIES: [u32; 5] = [0, 1, 3, 9, 15];

const OP_FEATURE_STATE: u32 = 1;
const OP_VARIANT_STATE: u32 = 2;
const OP_RESET_STATE: u32 = 4;

const BSD_ITEM_FEATURE_CONFIGURATION_STATE: i32 = 17;
const BSD_STATE_UNINITIALIZED: i32 = 0;
const BSD_STATE_BOOT_PENDING: i32 = 1;

const STATUS_UNSUCCESSFUL: u32 = 0xC000_0001;
const STATUS_OBJECT_NAME_NOT_FOUND: u32 = 0xC000_0034;
/// 缓冲不足（调用方缓冲装不下全部配置，按 count 返回值扩容重试）
const STATUS_BUFFER_OVERFLOW: u32 = 0x8000_0005;
const STATUS_BUFFER_TOO_SMALL: u32 = 0xC000_0023;

/// Velocity 功能配置 API 需要的最低系统版本（Win10 18963）
const MIN_SUPPORTED_BUILD: u32 = 18963;

// ============ ntdll FFI（对照 ViVe NativeMethods.Ntdll.cs） ============
//
// 重要：功能配置（Velocity）API 是 Windows 10 1903（build 18362）才引入 ntdll 的。
// 若用 #[link(name = "ntdll")] 静态导入，exe 在 Win10 1809 等旧系统上加载时，会因
// 找不到 RtlQueryFeatureConfigurationChangeStamp 等导出点而直接无法启动
// （Entry Point Not Found）。因此这里全部改为运行时 GetProcAddress 动态解析：
// 系统不支持时仅让“功能开关”模块降级为不可用，不再拖垮整个程序启动。

type PQueryAllFeatureConfigurations = unsafe extern "system" fn(
    feature_configuration_type: u32,
    change_stamp: *mut u64,
    feature_configurations: *mut RtlFeatureConfiguration,
    feature_configuration_count: *mut u64,
) -> i32;
/// 单功能查询（当前命令未使用，保留供后续按 ID 即时查询）
type PQueryFeatureConfiguration = unsafe extern "system" fn(
    feature_id: u32,
    feature_configuration_type: u32,
    change_stamp: *mut u64,
    feature_configuration: *mut RtlFeatureConfiguration,
) -> i32;
type PQueryFeatureConfigurationChangeStamp = unsafe extern "system" fn() -> u64;
type PSetFeatureConfigurations = unsafe extern "system" fn(
    previous_change_stamp: *mut u64,
    feature_configuration_type: u32,
    feature_configurations: *const RtlFeatureConfigurationUpdate,
    feature_configuration_count: i32,
) -> i32;
type PSetSystemBootStatus = unsafe extern "system" fn(
    bsd_item_type: i32,
    data: *mut i32,
    data_length: i32,
    return_length: *mut i32,
) -> i32;
type PGetSystemBootStatus = unsafe extern "system" fn(
    bsd_item_type: i32,
    data: *mut i32,
    data_length: i32,
    return_length: *mut i32,
) -> i32;
type PCreateBootStatusDataFile = unsafe extern "system" fn(boot_status_path: *const u16) -> i32;

/// ntdll 功能配置相关 API 的函数指针集合（全部经 GetProcAddress 动态解析，进程内只加载一次）
struct NtdllApi {
    /// 持有 ntdll 句柄（ntdll 常驻系统，这里仅为确保符号来源稳定）
    #[allow(dead_code)]
    _lib: Library,
    query_all: Option<PQueryAllFeatureConfigurations>,
    #[allow(dead_code)]
    query_one: Option<PQueryFeatureConfiguration>,
    change_stamp: Option<PQueryFeatureConfigurationChangeStamp>,
    set_configs: Option<PSetFeatureConfigurations>,
    set_boot_status: Option<PSetSystemBootStatus>,
    get_boot_status: Option<PGetSystemBootStatus>,
    create_boot_status_file: Option<PCreateBootStatusDataFile>,
}

impl NtdllApi {
    /// 核心功能配置 API 是否齐备（决定“功能开关”模块是否可用）
    fn feature_config_supported(&self) -> bool {
        self.query_all.is_some() && self.set_configs.is_some() && self.change_stamp.is_some()
    }
}

static NT_API: OnceLock<Option<NtdllApi>> = OnceLock::new();

/// 解析 ntdll 中的功能配置符号；缺失（旧系统）时对应字段为 None。
fn get_api() -> Option<&'static NtdllApi> {
    NT_API
        .get_or_init(|| {
            let lib = unsafe { Library::new("ntdll.dll") }.ok()?;
            unsafe {
                let query_all = lib
                    .get::<PQueryAllFeatureConfigurations>(b"RtlQueryAllFeatureConfigurations")
                    .ok()
                    .map(|s| *s);
                let query_one = lib
                    .get::<PQueryFeatureConfiguration>(b"RtlQueryFeatureConfiguration")
                    .ok()
                    .map(|s| *s);
                let change_stamp = lib
                    .get::<PQueryFeatureConfigurationChangeStamp>(
                        b"RtlQueryFeatureConfigurationChangeStamp",
                    )
                    .ok()
                    .map(|s| *s);
                let set_configs = lib
                    .get::<PSetFeatureConfigurations>(b"RtlSetFeatureConfigurations")
                    .ok()
                    .map(|s| *s);
                let set_boot_status = lib
                    .get::<PSetSystemBootStatus>(b"RtlSetSystemBootStatus")
                    .ok()
                    .map(|s| *s);
                let get_boot_status = lib
                    .get::<PGetSystemBootStatus>(b"RtlGetSystemBootStatus")
                    .ok()
                    .map(|s| *s);
                let create_boot_status_file = lib
                    .get::<PCreateBootStatusDataFile>(b"RtlCreateBootStatusDataFile")
                    .ok()
                    .map(|s| *s);
                Some(NtdllApi {
                    _lib: lib,
                    query_all,
                    query_one,
                    change_stamp,
                    set_configs,
                    set_boot_status,
                    get_boot_status,
                    create_boot_status_file,
                })
            }
        })
        .as_ref()
}

/// 系统不支持功能配置 API 时的错误提示
fn api_unavailable() -> String {
    "当前系统不支持功能配置 API（需要 Windows 10 1903 或更高版本）".to_string()
}

// ============ 结构体（对照 ViVe NativeStructs.cs，布局必须完全一致） ============

/// 12 字节：FeatureId + CompactState 位域 + VariantPayload
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct RtlFeatureConfiguration {
    feature_id: u32,
    compact_state: u32,
    variant_payload: u32,
}

impl RtlFeatureConfiguration {
    /// bits 0-3
    fn priority(&self) -> u32 {
        self.compact_state & 0xF
    }
    /// bits 4-5
    fn enabled_state(&self) -> u32 {
        (self.compact_state & 0x30) >> 4
    }
    /// bit 6
    fn is_wexp(&self) -> bool {
        ((self.compact_state & 0x40) >> 6) == 1
    }
    /// bits 8-13
    fn variant(&self) -> u32 {
        (self.compact_state & 0x3F00) >> 8
    }
    /// bits 14-15
    fn variant_payload_kind(&self) -> u32 {
        (self.compact_state & 0xC000) >> 14
    }
}

/// 32 字节（8 × u32），字段顺序对照 C# RTL_FEATURE_CONFIGURATION_UPDATE 的声明顺序
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct RtlFeatureConfigurationUpdate {
    feature_id: u32,
    priority: u32,
    enabled_state: u32,
    enabled_state_options: u32,
    variant: u32,
    variant_payload_kind: u32,
    variant_payload: u32,
    operation: u32,
}

impl RtlFeatureConfigurationUpdate {
    fn new_reset(feature_id: u32, priority: u32) -> Self {
        Self {
            feature_id,
            priority,
            enabled_state: 0,
            enabled_state_options: 0,
            variant: 0,
            variant_payload_kind: 0,
            variant_payload: 0,
            operation: OP_RESET_STATE,
        }
    }
}

// ============ 功能 ID 混淆（对照 ViVe ObfuscationHelpers.cs） ============
// 注册表键名使用混淆后的功能 ID。
// 注意：C# RotateRight32(value, -1) 的移位数按 & 31 截断，等价于循环右移 31（即左移 1）。

fn obfuscate_feature_id(id: u32) -> u32 {
    ((id ^ 0x7416_1A4E).swap_bytes() ^ 0x8FB2_3D4F)
        .rotate_left(1)
        ^ 0x833E_A8FF
}

/// 反混淆：供单元测试与后续注册表键展示使用
#[allow(dead_code)]
fn deobfuscate_feature_id(id: u32) -> u32 {
    ((id ^ 0x833E_A8FF)
        .rotate_right(1)
        ^ 0x8FB2_3D4F)
        .swap_bytes()
        ^ 0x7416_1A4E
}

// ============ 核心操作（对照 ViVe FeatureManager.cs） ============

/// 查询整个功能存储。
/// 关键点（实测 Windows 11 24H2 / build 26100）：ntdll 会把配置数量按 64 位写出，
/// 若用 i32 承接会越界写坏相邻栈变量（release 下 capacity 被清零 → 访问违例 / 死循环），
/// 因此这里必须用 u64 承接 count。同时该版本传 null 缓冲取数量也会触发访问违例，
/// 故采用一次性大缓冲直取，装不下时按内核返回的数量扩容重试。
fn query_all_configurations(
    cfg_type: u32,
) -> Result<(Vec<RtlFeatureConfiguration>, u64), String> {
    let api = get_api().ok_or_else(api_unavailable)?;
    let query_all = api.query_all.ok_or_else(api_unavailable)?;
    unsafe {
        let mut capacity: usize = 8192;
        loop {
            let mut configs = vec![RtlFeatureConfiguration::default(); capacity];
            let mut count = capacity as u64;
            let mut change_stamp = 0u64;
            let hres = query_all(
                cfg_type,
                &mut change_stamp,
                configs.as_mut_ptr(),
                &mut count,
            );
            if hres != 0 {
                // 缓冲不足（0x80000005 / 0xC0000023）：按内核返回的数量扩容重试
                let status = hres as u32;
                if status == STATUS_BUFFER_OVERFLOW || status == STATUS_BUFFER_TOO_SMALL {
                    let need = (count as usize).max(capacity.saturating_mul(2));
                    if need > capacity {
                        capacity = need;
                        continue;
                    }
                }
                return Err(ntstatus_to_message(hres));
            }
            let written = count as usize;
            if written < capacity {
                configs.truncate(written);
                return Ok((configs, change_stamp));
            }
            // 恰好装满可能被截断，扩容一倍重试
            capacity = capacity.saturating_mul(2);
        }
    }
}

/// 校验优先级可写（不可变优先级抛错，与 FeatureManager.SetFeatureConfigurations 一致）
fn validate_priority(priority: u32) -> Result<(), String> {
    if IMMUTABLE_PRIORITIES.contains(&priority) {
        Err(format!(
            "优先级 {} 是系统不可变优先级，不允许写入",
            priority
        ))
    } else {
        Ok(())
    }
}

/// 写 Runtime 存储。previous_change_stamp 传 0 跳过并发检查（与 ViVeTool 默认行为一致）。
fn set_runtime_configurations(updates: &[RtlFeatureConfigurationUpdate]) -> Result<(), String> {
    let api = get_api().ok_or_else(api_unavailable)?;
    let set_configs = api.set_configs.ok_or_else(api_unavailable)?;
    let mut prev_stamp = 0u64;
    let hres = unsafe {
        set_configs(
            &mut prev_stamp,
            CFG_TYPE_RUNTIME,
            updates.as_ptr(),
            updates.len() as i32,
        )
    };
    if hres != 0 {
        Err(ntstatus_to_message(hres))
    } else {
        Ok(())
    }
}

/// 写 Boot 存储。ntdll 的设置 API 只作用于 Runtime，
/// 持久化需按内核行为直接写 FeatureManagement\Overrides 注册表（对照
/// FeatureManager.SetFeatureConfigurationsInRegistry，不含 UserPolicy 分支——
/// 本模块固定 User 优先级）。
fn set_boot_configurations_in_registry(
    updates: &[RtlFeatureConfigurationUpdate],
) -> Result<(), String> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    for u in updates {
        let obf = obfuscate_feature_id(u.feature_id);
        let subkey = format!(
            r"SYSTEM\CurrentControlSet\Control\FeatureManagement\Overrides\{}\{}",
            u.priority, obf
        );
        if u.operation & OP_RESET_STATE != 0 {
            // 删除该功能的覆盖键树，不存在则忽略
            let _ = hklm.delete_subkey_all(&subkey);
        } else {
            let (key, _) = hklm
                .create_subkey(&subkey)
                .map_err(|e| format!("创建注册表键失败: {}", e))?;
            if u.operation & OP_FEATURE_STATE != 0 {
                key.set_value("EnabledState", &u.enabled_state)
                    .map_err(|e| format!("写入 EnabledState 失败: {}", e))?;
                key.set_value("EnabledStateOptions", &u.enabled_state_options)
                    .map_err(|e| format!("写入 EnabledStateOptions 失败: {}", e))?;
            }
            if u.operation & OP_VARIANT_STATE != 0 {
                key.set_value("Variant", &u.variant)
                    .map_err(|e| format!("写入 Variant 失败: {}", e))?;
                key.set_value("VariantPayload", &u.variant_payload)
                    .map_err(|e| format!("写入 VariantPayload 失败: {}", e))?;
                key.set_value("VariantPayloadKind", &u.variant_payload_kind)
                    .map_err(|e| format!("写入 VariantPayloadKind 失败: {}", e))?;
            }
        }
    }
    Ok(())
}

/// Boot 存储写入后更新 LKG 状态为 BootPending（对照 ViVeTool UpdateLKGStatus）。
/// 尽力而为：BSD 文件缺失时先创建；失败只记录日志，不让主操作报错。
fn update_lkg_status() {
    let Some(api) = get_api() else { return };
    let (Some(get_boot_status), Some(set_boot_status), Some(create_boot_status_file)) =
        (api.get_boot_status, api.set_boot_status, api.create_boot_status_file)
    else {
        return;
    };
    unsafe {
        let mut current = BSD_STATE_UNINITIALIZED;
        let mut result = get_boot_status(
            BSD_ITEM_FEATURE_CONFIGURATION_STATE,
            &mut current,
            4,
            std::ptr::null_mut(),
        );
        if result != 0 {
            if result as u32 == STATUS_OBJECT_NAME_NOT_FOUND {
                result = create_boot_status_file(std::ptr::null());
                if result != 0 {
                    log::warn!("初始化 Boot 状态数据文件失败: 0x{:08X}", result as u32);
                    return;
                }
                current = BSD_STATE_UNINITIALIZED;
            } else {
                log::warn!("查询 LKG 状态失败: 0x{:08X}", result as u32);
                return;
            }
        }
        if current != BSD_STATE_BOOT_PENDING {
            let mut new_state = BSD_STATE_BOOT_PENDING;
            let result = set_boot_status(
                BSD_ITEM_FEATURE_CONFIGURATION_STATE,
                &mut new_state,
                4,
                std::ptr::null_mut(),
            );
            if result != 0 {
                log::warn!("设置 LKG 状态失败: 0x{:08X}", result as u32);
            }
        }
    }
}

fn ntstatus_to_message(status: i32) -> String {
    match status as u32 {
        0xC000_0022 => "拒绝访问：需要管理员权限".to_string(),
        STATUS_UNSUCCESSFUL => "操作失败：功能存储已发生变化，请重试".to_string(),
        STATUS_OBJECT_NAME_NOT_FOUND => "操作失败：系统数据对象不存在".to_string(),
        _ => format!("操作失败 (0x{:08X})", status as u32),
    }
}

// ============ 功能字典 ============

static FEATURE_DICTIONARY: OnceLock<HashMap<u32, String>> = OnceLock::new();

/// 解析资源目录下的 FeatureDictionary.pfs（源自 ViVe Extra/FeatureDictionary.pfs）
fn get_vive_resource_dir(app: &AppHandle) -> Option<PathBuf> {
    // 1. Tauri resource_dir（生产环境 + 部分开发现境）
    if let Ok(resource_dir) = app.path().resource_dir() {
        let candidates = [
            resource_dir.join("vive"),
            resource_dir.join("resources").join("vive"),
            resource_dir.join("_up_").join("resources").join("vive"),
            resource_dir
                .join("_up_")
                .join("_up_")
                .join("src-tauri")
                .join("resources")
                .join("vive"),
        ];
        for path in &candidates {
            if path.exists() {
                return Some(path.clone());
            }
        }
    }

    // 2. exe 相对路径（开发环境：exe 在 src-tauri/target/debug/ 下）
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            let candidates = [
                parent.join("vive"),
                parent.join("resources").join("vive"),
                parent.join("..").join("..").join("resources").join("vive"),
                parent
                    .join("..")
                    .join("..")
                    .join("..")
                    .join("src-tauri")
                    .join("resources")
                    .join("vive"),
            ];
            for path in &candidates {
                if path.exists() {
                    if let Ok(canon) = path.canonicalize() {
                        return Some(canon);
                    }
                    return Some(path.clone());
                }
            }
        }
    }

    // 3. 编译时路径（开发环境最可靠）
    let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("vive");
    if dev_path.exists() {
        return Some(dev_path);
    }

    None
}

/// 加载字典（行格式 "功能名,ID"），进程内只解析一次；文件缺失时返回空表
fn load_dictionary(app: &AppHandle) -> &'static HashMap<u32, String> {
    FEATURE_DICTIONARY.get_or_init(|| {
        let mut map = HashMap::new();
        let Some(dir) = get_vive_resource_dir(app) else {
            log::warn!("未找到功能字典目录 resources/vive");
            return map;
        };
        let path = dir.join("FeatureDictionary.pfs");
        let Ok(content) = std::fs::read_to_string(&path) else {
            log::warn!("读取功能字典失败: {}", path.display());
            return map;
        };
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Some((name, id)) = line.split_once(',') else {
                continue;
            };
            if let Ok(id) = id.trim().parse::<u32>() {
                map.entry(id).or_insert_with(|| name.trim().to_string());
            }
        }
        log::info!("功能字典已加载: {} 条", map.len());
        map
    })
}

// ============ 系统信息 ============

fn get_os_build() -> u32 {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    hklm
        .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
        .and_then(|key| key.get_value::<String, _>("CurrentBuildNumber"))
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0)
}

// ============ Tauri 命令 ============

#[derive(Serialize)]
pub struct FeatureFlagsStatus {
    pub supported: bool,
    pub os_build: u32,
    pub is_admin: bool,
    pub boot_pending: bool,
    pub dictionary_count: usize,
    pub change_stamp: u64,
}

// 注意：命令必须为 async。同步命令会在主线程执行，查询/字典遍历会卡住 UI。
#[tauri::command]
pub async fn feature_flags_status(app: AppHandle) -> Result<FeatureFlagsStatus, String> {
    let os_build = get_os_build();
    let api = get_api();
    // 需要 Win10 1903+ 的 ntdll 且核心功能配置 API 全部解析成功才算可用
    let supported = os_build >= MIN_SUPPORTED_BUILD
        && api.map(|a| a.feature_config_supported()).unwrap_or(false);
    let mut boot_pending = false;
    if let Some(api) = api {
        if let Some(get_boot_status) = api.get_boot_status {
            unsafe {
                let mut current = BSD_STATE_UNINITIALIZED;
                let result = get_boot_status(
                    BSD_ITEM_FEATURE_CONFIGURATION_STATE,
                    &mut current,
                    4,
                    std::ptr::null_mut(),
                );
                if result == 0 {
                    boot_pending = current == BSD_STATE_BOOT_PENDING;
                }
            }
        }
    }
    let change_stamp = api
        .and_then(|a| a.change_stamp)
        .map(|f| unsafe { f() })
        .unwrap_or(0);
    Ok(FeatureFlagsStatus {
        supported,
        os_build,
        is_admin: crate::optimization::is_admin(),
        boot_pending,
        dictionary_count: load_dictionary(&app).len(),
        change_stamp,
    })
}

#[derive(Serialize, Clone)]
pub struct FeatureFlagEntry {
    pub feature_id: u32,
    pub name: Option<String>,
    pub priority: u32,
    /// 0=默认 1=已禁用 2=已启用
    pub enabled_state: u32,
    pub variant: u32,
    pub variant_payload_kind: u32,
    pub is_wexp: bool,
    /// false = 仅字典命中，当前存储中无自定义配置
    pub has_config: bool,
}

/// 查询功能配置列表。store: "runtime" | "boot"；
/// search 匹配 ID 或名称（字典），字典命中但无配置的条目也会返回（has_config=false）。
/// named_only：仅在无搜索词时生效，过滤掉字典无法识别名称的内部 servicing 条目。
#[tauri::command]
pub async fn feature_flags_query(
    app: AppHandle,
    store: String,
    search: String,
    named_only: Option<bool>,
    limit: Option<u32>,
) -> Result<Vec<FeatureFlagEntry>, String> {
    let cfg_type = if store.eq_ignore_ascii_case("boot") {
        CFG_TYPE_BOOT
    } else {
        CFG_TYPE_RUNTIME
    };
    let dictionary = load_dictionary(&app);
    let (configs, _) = query_all_configurations(cfg_type)?;

    let search = search.trim().to_lowercase();
    let mut entries: Vec<FeatureFlagEntry> = configs
        .iter()
        .map(|c| {
            let name = dictionary.get(&c.feature_id).cloned();
            FeatureFlagEntry {
                feature_id: c.feature_id,
                name: name.clone(),
                priority: c.priority(),
                enabled_state: c.enabled_state(),
                variant: c.variant(),
                variant_payload_kind: c.variant_payload_kind(),
                is_wexp: c.is_wexp(),
                has_config: true,
            }
        })
        .filter(|e| {
            if search.is_empty() {
                return true;
            }
            e.feature_id.to_string().contains(&search)
                || e.name
                    .as_ref()
                    .map(|n| n.to_lowercase().contains(&search))
                    .unwrap_or(false)
        })
        .collect();

    // 浏览模式（无搜索词）下可只看有名称的条目；
    // 大部分本机配置是 Windows 内部 servicing 项，字典无法识别，仅显示数字无操作价值
    if search.is_empty() && named_only.unwrap_or(false) {
        entries.retain(|e| e.name.is_some());
    }

    // 搜索时附带字典命中但无配置的功能，便于按名称启用
    if !search.is_empty() {
        let present: std::collections::HashSet<u32> =
            entries.iter().map(|e| e.feature_id).collect();
        for (id, name) in dictionary.iter() {
            if present.contains(id) {
                continue;
            }
            if id.to_string().contains(&search) || name.to_lowercase().contains(&search) {
                entries.push(FeatureFlagEntry {
                    feature_id: *id,
                    name: Some(name.clone()),
                    priority: PRIORITY_USER,
                    enabled_state: STATE_DEFAULT,
                    variant: 0,
                    variant_payload_kind: 0,
                    is_wexp: false,
                    has_config: false,
                });
            }
        }
    }

    // ViVeTool 同款"ID 直达"：输入合法功能 ID 时，即使系统存储/字典里都没有，
    // 也返回一个可直接启用的虚拟条目（底层 API 本来就是按 ID 操作，不要求功能已存在）。
    if !search.is_empty() {
        if let Ok(id) = search.parse::<u32>() {
            let present: std::collections::HashSet<u32> =
                entries.iter().map(|e| e.feature_id).collect();
            if !present.contains(&id) {
                entries.push(FeatureFlagEntry {
                    feature_id: id,
                    name: None,
                    priority: PRIORITY_USER,
                    enabled_state: STATE_DEFAULT,
                    variant: 0,
                    variant_payload_kind: 0,
                    is_wexp: false,
                    has_config: false,
                });
            }
        }
    }

    if search.is_empty() {
        // 浏览模式（ViVeTool /query 同款）：列出全部功能，名字只是注释。
        // 有名字的优先展示，其余按 ID 排序；受 limit 约束
        entries.sort_by(|a, b| {
            b.name
                .is_some()
                .cmp(&a.name.is_some())
                .then_with(|| a.feature_id.cmp(&b.feature_id))
        });
        let limit = limit.unwrap_or(500).min(50_000) as usize;
        entries.truncate(limit);
    } else {
        // 搜索模式：按相关性排序（名字命中 > 名字前缀命中 > 有配置 > ID 升序），
        // 不再按 ID 硬截断，避免搜索词命中较多时把用户要找的功能挤出结果
        entries.sort_by(|a, b| {
            score_search(b, &search)
                .cmp(&score_search(a, &search))
                .then_with(|| a.feature_id.cmp(&b.feature_id))
        });
    }
    Ok(entries)
}

/// 搜索相关性打分：返回 (名字命中, 前缀命中, 有配置)，越大越靠前
fn score_search(entry: &FeatureFlagEntry, search: &str) -> (u8, u8, bool) {
    let name = entry
        .name
        .as_ref()
        .map(|n| n.to_lowercase())
        .unwrap_or_default();
    let name_hit = name.contains(search);
    let prefix_hit = name.starts_with(search);
    (name_hit as u8, prefix_hit as u8, entry.has_config)
}

/// 启用/禁用功能。state: "enabled" | "disabled"；persist_boot=true 时同时写入
/// Boot 存储（重启后仍生效）并更新 LKG 状态。固定 User 优先级（ViVeTool 默认）。
#[tauri::command]
pub async fn feature_flags_set(
    _app: AppHandle,
    id: u32,
    state: String,
    persist_boot: bool,
) -> Result<String, String> {
    let enabled_state = match state.to_lowercase().as_str() {
        "enabled" | "enable" => STATE_ENABLED,
        "disabled" | "disable" => STATE_DISABLED,
        _ => return Err(format!("无效的状态: {}（应为 enabled 或 disabled）", state)),
    };
    validate_priority(PRIORITY_USER)?;

    let update = RtlFeatureConfigurationUpdate {
        feature_id: id,
        priority: PRIORITY_USER,
        enabled_state,
        enabled_state_options: 0,
        variant: 0,
        variant_payload_kind: 0,
        variant_payload: 0,
        // 与原版 ViVeTool /enable 一致：同时写入 VariantState（清空原 Variant 字段）
        operation: OP_FEATURE_STATE | OP_VARIANT_STATE,
    };

    set_runtime_configurations(&[update])?;

    if persist_boot {
        set_boot_configurations_in_registry(&[update])?;
        update_lkg_status();
    }

    Ok(if persist_boot {
        format!("功能 {} 已更新（重启后保持生效）", id)
    } else {
        format!("功能 {} 已更新（仅本次开机有效）", id)
    })
}

/// 重置功能的自定义配置。store: "runtime" | "boot" | "both"（默认 both），
/// priority 默认 User。
#[tauri::command]
pub async fn feature_flags_reset(
    _app: AppHandle,
    id: u32,
    store: String,
    priority: Option<u32>,
) -> Result<String, String> {
    let priority = priority.unwrap_or(PRIORITY_USER);
    validate_priority(priority)?;
    let store = store.to_lowercase();
    let (do_runtime, do_boot) = match store.as_str() {
        "runtime" => (true, false),
        "boot" => (false, true),
        _ => (true, true),
    };

    if do_runtime {
        set_runtime_configurations(&[RtlFeatureConfigurationUpdate::new_reset(id, priority)])?;
    }
    if do_boot {
        set_boot_configurations_in_registry(&[RtlFeatureConfigurationUpdate::new_reset(
            id, priority,
        )])?;
        update_lkg_status();
    }

    Ok(format!("功能 {} 的自定义配置已重置", id))
}

// ============ 单元测试 ============

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::size_of;

    #[test]
    fn struct_layouts_match_windows() {
        assert_eq!(size_of::<RtlFeatureConfiguration>(), 12);
        assert_eq!(size_of::<RtlFeatureConfigurationUpdate>(), 32);
    }

    #[test]
    fn compact_state_bitfield_packing() {
        let mut c = RtlFeatureConfiguration {
            feature_id: 1234,
            compact_state: 0,
            variant_payload: 77,
        };
        // User 优先级 8 + Enabled 2 + Variant 33 + PayloadKind 2 + Wexp
        c.compact_state = (8 & 0xF) | (2 << 4) | (1 << 6) | (33 << 8) | (2 << 14);
        assert_eq!(c.priority(), 8);
        assert_eq!(c.enabled_state(), STATE_ENABLED);
        assert!(c.is_wexp());
        assert_eq!(c.variant(), 33);
        assert_eq!(c.variant_payload_kind(), 2);
        // 边界：variant 最大 63、priority 最大 15
        c.compact_state = 15 | (63 << 8);
        assert_eq!(c.priority(), 15);
        assert_eq!(c.variant(), 63);
    }

    #[test]
    fn feature_id_obfuscation_roundtrip() {
        for id in [0u32, 1, 0x12345678, 0xDEADBEEF, 999999999, u32::MAX] {
            assert_eq!(deobfuscate_feature_id(obfuscate_feature_id(id)), id);
        }
    }

    /// 已知向量：按 ViVe ObfuscationHelpers.cs 的 C# 语义手工推算
    /// obfuscate(0x12345678) = 0xF0C296AC
    #[test]
    fn feature_id_obfuscation_known_vector() {
        assert_eq!(obfuscate_feature_id(0x1234_5678), 0xF0C2_96AC);
        assert_eq!(deobfuscate_feature_id(0xF0C2_96AC), 0x1234_5678);
    }

    #[test]
    fn immutable_priorities_rejected() {
        for p in IMMUTABLE_PRIORITIES {
            assert!(validate_priority(p).is_err());
        }
        assert!(validate_priority(PRIORITY_USER).is_ok());
    }

    /// 真机只读冒烟测试：查询真实 Runtime 存储（不需管理员）
    #[test]
    #[ignore = "live 只读验证"]
    fn live_query_runtime_store() {
        let (configs, stamp) = query_all_configurations(CFG_TYPE_RUNTIME)
            .expect("查询 Runtime 存储失败");
        println!("change stamp = {}, 共 {} 条配置", stamp, configs.len());
        for c in configs.iter().take(5) {
            println!(
                "  id={} priority={} state={}",
                c.feature_id,
                c.priority(),
                c.enabled_state()
            );
        }
        assert!(!configs.is_empty(), "Runtime 存储不应为空");
    }

    /// 真机只读交叉验证：Boot 存储来自注册表 Overrides 键，
    /// 取 API 返回的功能 ID 现算混淆值，确认对应注册表键真实存在，
    /// 以此验证混淆算法与内核一致。
    #[test]
    #[ignore = "live 只读验证"]
    fn live_obfuscation_matches_registry() {
        use winreg::enums::HKEY_LOCAL_MACHINE;
        use winreg::RegKey;

        let (configs, _) =
            query_all_configurations(CFG_TYPE_BOOT).expect("查询 Boot 存储失败");
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let base = r"SYSTEM\CurrentControlSet\Control\FeatureManagement\Overrides";
        let mut checked = 0;
        let mut matched = 0;
        for c in &configs {
            if c.priority() >= 16 {
                continue; // UserPolicy 等不落在 Overrides\<priority> 下
            }
            let path = format!(r"{}\{}\{}", base, c.priority(), obfuscate_feature_id(c.feature_id));
            checked += 1;
            if hklm.open_subkey(&path).is_ok() {
                matched += 1;
            }
        }
        println!("Boot 存储 {} 条，其中 {} 条已验证混淆键存在", checked, matched);
        // 绝大多数真实条目都应对应真实注册表键
        assert!(checked == 0 || matched as f64 / checked as f64 > 0.8);
    }
}
