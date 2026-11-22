//! 反作弊进程优化模块
//!
//! 以「反作弊分组」为单位（ACE / EasyAntiCheat / BattlEye / Vanguard / GameGuard /
//! EA / NEAC / FACEIT 等），每个分组独立卡片，功能与 ACE 一致：
//! 降优先级 + 锁 E 核 + 效能模式 + 注册表强制限制。
//! 与 ACE 实现的差异：核心分配从「锁 CPU0」改为「锁到 E 核（小核）」，
//! 通过 GetLogicalProcessorInformation 识别 1 线程核心（E 核）计算亲和性掩码。
//!
//! 复用 `optimization.rs` 的句柄打开 / 管理员判断 / 亲和性设置 / 效能模式。

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sysinfo::System;
use tauri_plugin_store::StoreExt;
use winreg::enums::*;
use winreg::RegKey;

use crate::optimization;

// ─── 反作弊分组目录 ───

/// 单个反作弊分组定义
pub struct AcGroupDef {
    pub key: &'static str,
    pub name: &'static str,
    pub vendor: &'static str,
    /// 该反作弊保护的游戏（用于卡片副标题）
    pub games: &'static str,
    pub processes: &'static [&'static str],
}

const GROUP_ACE: &[&str] = &[
    "SGuard64", "SGuardSvc64", "ACE-Tray", "ACE-BASE", "ACE-BASE64", "ACE-PC",
    "ACE-Helper", "SGuard", "SGuardSvc", "AntiCheatExpert", "AntiCheatExpert.Service",
];
const GROUP_EAC: &[&str] = &["EasyAntiCheat", "EasyAntiCheat_EOS"];
const GROUP_BATTEYE: &[&str] = &["BEService", "BEService_x64"];
const GROUP_VANGUARD: &[&str] = &["vgc", "vgtray"];
const GROUP_GAMEGUARD: &[&str] = &[
    "GameMon", "GameMon.des", "GameMon64", "GameMon64.des", "npggNT", "npggNT.des", "GameGuard",
];
const GROUP_EA: &[&str] = &["EAAntiCheat.GameService", "EAAntiCheat.GameServiceLauncher"];
const GROUP_NEAC: &[&str] = &["NeacSafe64", "NeacSafe", "nac"];
const GROUP_FACEIT: &[&str] = &["faceitservice", "faceitclient", "faceit"];

pub(crate) const GROUPS: &[AcGroupDef] = &[
    AcGroupDef {
        key: "ace",
        name: "ACE 反作弊",
        vendor: "腾讯",
        games: "英雄联盟 无畏契约 三角洲行动 CF 等",
        processes: GROUP_ACE,
    },
    AcGroupDef {
        key: "eac",
        name: "EasyAntiCheat",
        vendor: "Epic",
        games: "Apex 堡垒之夜 永劫无间 幻兽帕鲁 等",
        processes: GROUP_EAC,
    },
    AcGroupDef {
        key: "battleye",
        name: "BattlEye",
        vendor: "",
        games: "PUBG 彩虹六号 DayZ 逃离塔科夫 等",
        processes: GROUP_BATTEYE,
    },
    AcGroupDef {
        key: "vanguard",
        name: "Vanguard",
        vendor: "Riot",
        games: "无畏契约 英雄联盟",
        processes: GROUP_VANGUARD,
    },
    AcGroupDef {
        key: "gameguard",
        name: "nProtect GameGuard",
        vendor: "",
        games: "DNF 等韩系网游",
        processes: GROUP_GAMEGUARD,
    },
    AcGroupDef {
        key: "ea",
        name: "EA 反作弊",
        vendor: "EA",
        games: "战地 2042 战地 6 FC 等",
        processes: GROUP_EA,
    },
    AcGroupDef {
        key: "neac",
        name: "NEAC 反作弊",
        vendor: "网易",
        games: "部分网易游戏",
        processes: GROUP_NEAC,
    },
    AcGroupDef {
        key: "faceit",
        name: "FACEIT 反作弊",
        vendor: "",
        games: "CS2 第三方竞技平台",
        processes: GROUP_FACEIT,
    },
];

fn group_by_key(key: &str) -> Option<&'static AcGroupDef> {
    GROUPS.iter().find(|g| g.key == key)
}

// ─── E 核亲和性掩码 ───

/// 用 GetLogicalProcessorInformation 识别 E 核（小核）：
/// 1 线程的核心 = E 核，2 线程 = P 核（大核）。返回 E 核的逻辑处理器位掩码。
/// 失败或无 E 核时返回 0（调用方回退到锁 CPU0）。
pub fn get_e_core_mask() -> u64 {
    use windows_sys::Win32::System::SystemInformation::{
        GetLogicalProcessorInformation, RelationProcessorCore, SYSTEM_LOGICAL_PROCESSOR_INFORMATION,
    };
    unsafe {
        let mut len: u32 = 0;
        // 第一次调用获取所需缓冲区大小
        GetLogicalProcessorInformation(std::ptr::null_mut(), &mut len);
        if len == 0 {
            return 0;
        }
        let count = (len as usize) / std::mem::size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION>();
        if count == 0 {
            return 0;
        }
        let mut buf = vec![0u8; len as usize];
        let ok = GetLogicalProcessorInformation(
            buf.as_mut_ptr() as *mut SYSTEM_LOGICAL_PROCESSOR_INFORMATION,
            &mut len,
        );
        if ok == 0 {
            return 0;
        }
        let arr = std::slice::from_raw_parts(
            buf.as_ptr() as *const SYSTEM_LOGICAL_PROCESSOR_INFORMATION,
            count,
        );
        let mut mask: u64 = 0;
        for info in arr {
            if info.Relationship == RelationProcessorCore {
                let m = info.ProcessorMask as u64;
                if m != 0 && m.count_ones() == 1 {
                    mask |= m;
                }
            }
        }
        mask
    }
}

fn e_core_mask_or_fallback() -> u64 {
    let e = get_e_core_mask();
    if e != 0 {
        e
    } else {
        1 // 无 E 核时回退到 CPU0
    }
}

// ─── 统一结果结构 ───

#[derive(serde::Serialize)]
pub struct AntiCheatPartialResult {
    pub success: bool,
    pub message: String,
    pub count: u32,
    pub found_count: u32,
}

#[derive(serde::Serialize)]
pub struct PerfTweakResult {
    pub success: bool,
    pub message: String,
}

/// 根据 found/count 生成统一文案
fn ac_message(group: &AcGroupDef, found: u32, count: u32, ok_template: &str) -> String {
    if found == 0 {
        return format!("未找到运行中的 {} 进程", group.name);
    }
    if count == 0 {
        return format!(
            "发现 {} 个 {} 进程，但无法修改（反作弊保护了这些进程）",
            found, group.name
        );
    }
    if count < found {
        return format!(
            "{}（另有 {} 个受反作弊保护无法修改）",
            ok_template.replace("{}", &count.to_string()),
            found - count
        );
    }
    ok_template.replace("{}", &count.to_string())
}

// ─── 进程匹配辅助 ───

fn strip_exe_suffix(s: &str) -> &str {
    if s.len() >= 4 && s[s.len() - 4..].eq_ignore_ascii_case(".exe") {
        &s[..s.len() - 4]
    } else {
        s
    }
}

fn process_matches(process_name: &str, entry: &str) -> bool {
    strip_exe_suffix(process_name).eq_ignore_ascii_case(strip_exe_suffix(entry))
}

fn find_group_pids(system: &System, procs: &[&str]) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    for process in system.processes().values() {
        let name = process.name().to_string();
        if procs.iter().any(|n| process_matches(&name, n)) {
            out.push((process.pid().as_u32(), name));
        }
    }
    out
}

// ─── 分组操作实现 ───

fn limit_priority_impl(group: &AcGroupDef) -> (u32, u32) {
    let mut system = System::new();
    system.refresh_processes();
    let matches = find_group_pids(&system, group.processes);
    let mut count = 0u32;
    for (pid, _) in &matches {
        if optimization::set_process_low_priority(*pid) {
            count += 1;
        }
    }
    (matches.len() as u32, count)
}

fn restrict_affinity_impl(group: &AcGroupDef, mask: u64) -> (u32, u32) {
    let mut system = System::new();
    system.refresh_processes();
    let matches = find_group_pids(&system, group.processes);
    let mut count = 0u32;
    for (pid, _) in &matches {
        if optimization::set_process_affinity(*pid, mask) {
            count += 1;
        }
    }
    (matches.len() as u32, count)
}

fn set_efficiency_impl(group: &AcGroupDef) -> (u32, u32) {
    let mut system = System::new();
    system.refresh_processes();
    let matches = find_group_pids(&system, group.processes);
    let mut count = 0u32;
    for (pid, _) in &matches {
        if optimization::enable_process_efficiency_mode(*pid) {
            count += 1;
        }
    }
    (matches.len() as u32, count)
}

// ─── 注册表强制限制（IFEO PerfOptions）───

fn ifeo_perf_options_path(process_name: &str) -> String {
    format!(
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\{}\PerfOptions",
        strip_exe_suffix(process_name)
    )
}

fn apply_registry_impl(group: &AcGroupDef) -> Result<PerfTweakResult, String> {
    for proc_name in group.processes {
        let path = ifeo_perf_options_path(proc_name);
        let (key, _) = RegKey::predef(HKEY_LOCAL_MACHINE)
            .create_subkey(&path)
            .map_err(|e| format!("创建注册表键失败 ({}): {}", proc_name, e))?;
        // CpuPriorityClass: 1=Idle, IoPriority: 1=VeryLow（让反作弊进程让位）
        key.set_value("CpuPriorityClass", &1u32)
            .map_err(|e| format!("写入 CpuPriorityClass 失败 ({}): {}", proc_name, e))?;
        key.set_value("IoPriority", &1u32)
            .map_err(|e| format!("写入 IoPriority 失败 ({}): {}", proc_name, e))?;
    }
    Ok(PerfTweakResult {
        success: true,
        message: format!("已为 {} 应用注册表强制限制（相关进程重启后生效）", group.name),
    })
}

fn restore_registry_impl(group: &AcGroupDef) -> Result<PerfTweakResult, String> {
    for proc_name in group.processes {
        let path = ifeo_perf_options_path(proc_name);
        if let Ok(key) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(&path) {
            let _ = key.delete_value("CpuPriorityClass");
            let _ = key.delete_value("IoPriority");
        }
    }
    Ok(PerfTweakResult {
        success: true,
        message: format!("已恢复 {} 的注册表强制限制（相关进程重启后生效）", group.name),
    })
}

// ─── 自动检测（每分组独立）───

const AC_DETECT_INTERVAL_SECS: u64 = 5;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct AcAutoDetectConfig {
    pub enabled: bool,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct AcAutoDetectStats {
    pub is_running: bool,
    pub last_check: Option<String>,
    pub total_optimized: u32,
    pub currently_optimized: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct AcAutoDetectStatus {
    pub enabled: bool,
    pub is_running: bool,
    pub last_check: Option<String>,
    pub total_optimized: u32,
    pub currently_optimized: Vec<String>,
}

static AUTO_DETECT_CFG: Mutex<Option<std::collections::HashMap<String, AcAutoDetectConfig>>> =
    Mutex::new(None);
static AUTO_DETECT_GEN: AtomicU64 = AtomicU64::new(0);
static AUTO_DETECT_STATS: Mutex<Option<std::collections::HashMap<String, AcAutoDetectStats>>> =
    Mutex::new(None);
static AUTO_DETECT_ENABLED: AtomicBool = AtomicBool::new(false);

fn store_key(group_key: &str) -> String {
    format!("anticheat_auto_detect_{}.json", group_key)
}

fn load_persisted_config(app: &tauri::AppHandle, group_key: &str) -> AcAutoDetectConfig {
    match app.store(format!(
        "anticheat_auto_detect_{}.json",
        group_key
    )) {
        Ok(store) => {
            if let Some(v) = store.get("config") {
                if let Ok(c) = serde_json::from_value::<AcAutoDetectConfig>(v) {
                    return c;
                }
            }
        }
        Err(e) => log::warn!("Failed to open {} store: {}", store_key(group_key), e),
    }
    AcAutoDetectConfig::default()
}

async fn save_persisted_config(app: &tauri::AppHandle, group_key: &str, config: &AcAutoDetectConfig) {
    if let Ok(store) = app.store(store_key(group_key)) {
        store.set("config", serde_json::to_value(config).unwrap());
        let _ = store.save();
    }
}

fn detect_and_optimize(group: &AcGroupDef) -> Vec<String> {
    let mut optimized = Vec::new();
    let mut system = System::new();
    system.refresh_processes();
    let matches = find_group_pids(&system, group.processes);
    let e_mask = e_core_mask_or_fallback();
    for (pid, name) in &matches {
        let mut this = false;
        if optimization::enable_process_efficiency_mode(*pid)
            || optimization::set_process_low_priority(*pid)
        {
            this = true;
        }
        if optimization::set_process_affinity(*pid, e_mask) {
            this = true;
        }
        if this {
            optimized.push(name.clone());
        }
    }
    optimized
}

fn ac_auto_detect_loop(group: &'static AcGroupDef, generation: u64) {
    loop {
        if AUTO_DETECT_GEN.load(Ordering::Relaxed) != generation {
            break;
        }
        thread::sleep(Duration::from_secs(AC_DETECT_INTERVAL_SECS));
        if AUTO_DETECT_GEN.load(Ordering::Relaxed) != generation {
            break;
        }
        let enabled = {
            let cfg = AUTO_DETECT_CFG.lock().unwrap();
            cfg.as_ref()
                .and_then(|m| m.get(group.key))
                .map(|c| c.enabled)
                .unwrap_or(false)
        };
        if !enabled {
            continue;
        }
        let optimized = detect_and_optimize(group);
        let mut stats = AUTO_DETECT_STATS.lock().unwrap();
        let map = stats.get_or_insert_with(std::collections::HashMap::new);
        let s = map.entry(group.key.to_string()).or_default();
        s.is_running = true;
        s.last_check = Some(chrono::Local::now().to_rfc3339());
        s.total_optimized = s.total_optimized.saturating_add(optimized.len() as u32);
        s.currently_optimized = optimized;
    }
    let mut stats = AUTO_DETECT_STATS.lock().unwrap();
    if let Some(map) = stats.as_mut() {
        if let Some(s) = map.get_mut(group.key) {
            s.is_running = false;
        }
    }
}

// ─── Tauri 命令 ───

/// 获取分组列表（含进程名，供前端渲染卡片）
#[tauri::command]
pub fn anticheat_get_groups() -> Vec<serde_json::Value> {
    GROUPS
        .iter()
        .map(|g| {
            serde_json::json!({
                "key": g.key,
                "name": g.name,
                "vendor": g.vendor,
                "games": g.games,
                "processes": g.processes,
            })
        })
        .collect()
}

#[tauri::command]
pub async fn anticheat_limit_priority(group_key: String) -> Result<AntiCheatPartialResult, String> {
    let group = group_by_key(&group_key).ok_or_else(|| format!("未知分组: {}", group_key))?;
    if !optimization::is_admin() {
        return Err("修改反作弊进程需要管理员权限。当前 NexBox 未以管理员身份运行，请右键「以管理员身份运行」后重试".to_string());
    }
    let (found, count) = limit_priority_impl(group);
    Ok(AntiCheatPartialResult {
        success: count > 0,
        message: ac_message(group, found, count, "已限制 {} 个进程优先级"),
        count,
        found_count: found,
    })
}

#[tauri::command]
pub async fn anticheat_restrict_affinity(group_key: String) -> Result<AntiCheatPartialResult, String> {
    let group = group_by_key(&group_key).ok_or_else(|| format!("未知分组: {}", group_key))?;
    if !optimization::is_admin() {
        return Err("修改反作弊进程需要管理员权限。当前 NexBox 未以管理员身份运行，请右键「以管理员身份运行」后重试".to_string());
    }
    let mask = e_core_mask_or_fallback();
    let (found, count) = restrict_affinity_impl(group, mask);
    Ok(AntiCheatPartialResult {
        success: count > 0,
        message: ac_message(group, found, count, "已限制 {} 个进程使用 E 核"),
        count,
        found_count: found,
    })
}

#[tauri::command]
pub async fn anticheat_set_efficiency(group_key: String) -> Result<AntiCheatPartialResult, String> {
    let group = group_by_key(&group_key).ok_or_else(|| format!("未知分组: {}", group_key))?;
    if !optimization::is_admin() {
        return Err("修改反作弊进程需要管理员权限。当前 NexBox 未以管理员身份运行，请右键「以管理员身份运行」后重试".to_string());
    }
    let (found, count) = set_efficiency_impl(group);
    Ok(AntiCheatPartialResult {
        success: count > 0,
        message: ac_message(group, found, count, "已为 {} 个进程开启调度优化"),
        count,
        found_count: found,
    })
}

#[tauri::command]
pub async fn anticheat_apply_registry(group_key: String) -> Result<PerfTweakResult, String> {
    let group = group_by_key(&group_key).ok_or_else(|| format!("未知分组: {}", group_key))?;
    if !optimization::is_admin() {
        return Err("写入注册表需要管理员权限。当前 NexBox 未以管理员身份运行，请右键「以管理员身份运行」后重试".to_string());
    }
    apply_registry_impl(group)
}

#[tauri::command]
pub async fn anticheat_restore_registry(group_key: String) -> Result<PerfTweakResult, String> {
    let group = group_by_key(&group_key).ok_or_else(|| format!("未知分组: {}", group_key))?;
    if !optimization::is_admin() {
        return Err("写入注册表需要管理员权限。当前 NexBox 未以管理员身份运行，请右键「以管理员身份运行」后重试".to_string());
    }
    restore_registry_impl(group)
}

#[tauri::command]
pub async fn anticheat_set_auto_detect(
    app: tauri::AppHandle,
    group_key: String,
    enabled: bool,
) -> Result<(), String> {
    let group = group_by_key(&group_key).ok_or_else(|| format!("未知分组: {}", group_key))?;
    let gen = AUTO_DETECT_GEN.fetch_add(1, Ordering::Relaxed) + 1;
    let config = AcAutoDetectConfig { enabled };
    {
        let mut cfg = AUTO_DETECT_CFG.lock().map_err(|e| e.to_string())?;
        cfg.get_or_insert_with(std::collections::HashMap::new)
            .insert(group_key.clone(), config.clone());
    }
    AUTO_DETECT_ENABLED.store(enabled, Ordering::Relaxed);
    save_persisted_config(&app, &group_key, &config).await;
    if enabled {
        let group: &'static AcGroupDef = group;
        thread::spawn(move || {
            let _ = std::panic::catch_unwind(|| ac_auto_detect_loop(group, gen));
        });
    }
    Ok(())
}

#[tauri::command]
pub async fn anticheat_get_auto_detect_status(
    _app: tauri::AppHandle,
    group_key: String,
) -> Result<AcAutoDetectStatus, String> {
    let enabled = {
        let cfg = AUTO_DETECT_CFG.lock().map_err(|e| e.to_string())?;
        cfg.as_ref()
            .and_then(|m| m.get(&group_key))
            .map(|c| c.enabled)
            .unwrap_or(false)
    };
    let stats = AUTO_DETECT_STATS
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .and_then(|m| m.get(&group_key))
        .cloned()
        .unwrap_or_default();
    Ok(AcAutoDetectStatus {
        enabled,
        is_running: stats.is_running && enabled,
        last_check: stats.last_check,
        total_optimized: stats.total_optimized,
        currently_optimized: stats.currently_optimized,
    })
}

/// 应用启动时调用：恢复持久化的自动检测配置并启动对应线程
pub async fn init(app: tauri::AppHandle) -> Result<(), String> {
    let mut cfg = AUTO_DETECT_CFG.lock().map_err(|e| e.to_string())?;
    let map = cfg.get_or_insert_with(std::collections::HashMap::new);
    let mut needs_start = Vec::new();
    for group in GROUPS {
        let persisted = load_persisted_config(&app, group.key);
        if persisted.enabled {
            map.insert(group.key.to_string(), persisted.clone());
            needs_start.push((group.key, persisted));
        }
    }
    drop(cfg);
    for (key, _persisted) in needs_start {
        let gen = AUTO_DETECT_GEN.fetch_add(1, Ordering::Relaxed) + 1;
        let group = group_by_key(key).ok_or_else(|| format!("未知分组: {}", key))?;
        let group: &'static AcGroupDef = group;
        thread::spawn(move || {
            let _ = std::panic::catch_unwind(|| ac_auto_detect_loop(group, gen));
        });
    }
    Ok(())
}