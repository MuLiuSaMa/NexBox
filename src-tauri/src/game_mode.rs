//! 游戏模式（Game Mode）
//!
//! 后台扫描并压制"游戏之外的合法后台进程"，把资源让给前台游戏，游戏结束后还原。
//! 三档：默认 / 常规 / 竞技。
//! - 默认：不压制。
//! - 常规：将明显占资源的后台进程压制到 `Eco`（省电）级，保留后台可用。
//! - 竞技：将除豁免外的所有后台进程压制到 `Isolated`（隔离）级，只放行前台全屏游戏家族。
//!
//! 豁免（永不压制）：
//! - 核心系统/系统 UI 进程（内核、DWM、explorer、输入法宿主、WSL 等，对齐 Pavise 预设白名单）
//! - 输入/外设/音频软件、平台壳（Steam/Epic/EA/育碧/暴雪/GOG/Xbox 等）、网游加速器
//! - 反作弊进程、性能监控叠加层（Afterburner/RTSS）
//! - 笔记本厂商控制台（联想 Vantage、Armoury Crate、OMEN Hub、AWCC 等，风扇/功耗管理）
//! - 滤镜名单内正在运行的游戏进程家族
//! - 当前前台窗口的进程及其后代家族：用户正在交互的对象绝不压制
//!
//! 生效档位（以用户选择为准）：
//! - 顶栏手动选择「常规/竞技/默认」：生效档位即为所选档位（手动优先）。
//! - 弹窗可设「游戏启动时自动切换」档位（默认=关）：游戏运行时且用户未手动覆盖时，
//!   自动切到该档位；手动覆盖仅对当前游戏会话生效，游戏退出后自动清除，
//!   下次启动游戏仍按自动档切换（除非用户改了自动档设置）。
//!   竞技档下，滤镜名单内正在运行的游戏进程家族会被豁免（只压制其他后台进程）。
//!
//! 压制采用"完整组合 + 快照还原"：记录每个进程的原始值（优先级/亲和/I/O/内存/EcoQoS/GPU），
//! 模式关闭或进程退出时逐项还原。参考 `optimization.rs` 的 ACE 自动检测模式
//! （generation 代次控制线程生命周期 + `app.store` 持久化配置 + 进程快照还原）。

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sysinfo::{Pid, ProcessRefreshKind, System};
use tauri::Emitter;
use tauri_plugin_store::StoreExt;

use crate::anticheat;
use crate::game_filter;
use crate::optimization;

const STORE_FILE: &str = "game_mode.json";
const POLL_INTERVAL_SECS: u64 = 3;

// Windows 优先级类别
const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x00004000;
// I/O 优先级
const IO_VERY_LOW: i32 = 0;
const IO_LOW: i32 = 1;
// 内存页优先级
const MEM_VERY_LOW: i32 = 1;
const MEM_MEDIUM: i32 = 3;
// GPU 调度优先级类
const GPU_IDLE: i32 = 0;

// 常规档 CPU 占用阈值（%）：超过才压制
const REGULAR_CPU_THRESHOLD: f32 = 5.0;

// ─── 数据结构 ───

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum GameModePreset {
    #[default]
    Default,
    Regular,
    Competitive,
}

impl GameModePreset {
    fn as_str(&self) -> &'static str {
        match self {
            GameModePreset::Default => "default",
            GameModePreset::Regular => "regular",
            GameModePreset::Competitive => "competitive",
        }
    }
    fn from_str(s: &str) -> Self {
        match s {
            "regular" => GameModePreset::Regular,
            "competitive" => GameModePreset::Competitive,
            _ => GameModePreset::Default,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, PartialOrd, Debug)]
enum SuppressionLevel {
    None,
    Eco,
    Isolated,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct GameModeConfig {
    pub preset: GameModePreset,
    pub manual_enabled: bool,
    pub auto_enabled: bool,
    /// 游戏启动时自动切换的档位；`Default` 表示「关」（不自动切换）
    #[serde(default)]
    pub auto_preset: GameModePreset,
}

#[derive(Serialize)]
pub struct GameModeStatus {
    pub preset: String,
    pub effective_preset: String,
    pub manual_enabled: bool,
    pub auto_enabled: bool,
    pub active: bool,
    pub suppressed_count: usize,
    pub game_running: bool,
}

/// 进程原始值快照（还原用）。`Option` 为 None 表示查询失败/不可写，还原时跳过该项。
#[derive(Clone, Copy)]
struct ProcessSnapshot {
    pri: u32,
    aff: u64,
    io: Option<i32>,
    mem: Option<i32>,
    eco_on: bool,
    gpu: Option<i32>,
}

struct Entry {
    snapshot: ProcessSnapshot,
    level: SuppressionLevel,
}

// ─── 全局状态 ───

static CONFIG: Mutex<Option<GameModeConfig>> = Mutex::new(None);
static GENERATION: AtomicU64 = AtomicU64::new(0);
/// 被压制进程：pid → Entry（记录原始值用于还原）
static CORE: Mutex<Option<HashMap<u32, Entry>>> = Mutex::new(None);
/// 本会话中被判定"受保护/不可写"而免压的进程名（避免每轮重试）
static BLOCKED: Mutex<Option<HashSet<String>>> = Mutex::new(None);
/// 当前是否处于生效状态（供状态查询）
static ACTIVE: Mutex<bool> = Mutex::new(false);
/// 滤镜名单内是否有游戏在运行（由扫描线程更新，供状态查询）
static GAME_RUNNING: Mutex<bool> = Mutex::new(false);
/// 当前生效档位（由扫描线程维护；游戏运行时可为自动档或用户手动选择）
static EFFECTIVE_PRESET: Mutex<GameModePreset> = Mutex::new(GameModePreset::Default);
/// 用户是否已手动切换过（持久覆盖）：一旦手工切换，自动切换不再生效，直至重新设置自动档
static MANUAL_OVERRIDE: AtomicBool = AtomicBool::new(false);

// ─── 配置存取 ───

fn get_config() -> GameModeConfig {
    CONFIG
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or(GameModeConfig {
            preset: GameModePreset::Default,
            manual_enabled: false,
            auto_enabled: false,
            auto_preset: GameModePreset::Default,
        })
}

async fn load_persisted_config(app: &tauri::AppHandle) -> GameModeConfig {
    let default = GameModeConfig {
        preset: GameModePreset::Default,
        manual_enabled: false,
        auto_enabled: false,
        auto_preset: GameModePreset::Default,
    };
    match app.store(STORE_FILE) {
        Ok(store) => match store.get("config") {
            Some(v) => serde_json::from_value::<GameModeConfig>(v).unwrap_or(default),
            None => default,
        },
        Err(_) => default,
    }
}

async fn save_persisted_config(app: &tauri::AppHandle, config: &GameModeConfig) {
    if let Ok(store) = app.store(STORE_FILE) {
        store.set("config", serde_json::to_value(config).unwrap());
        let _ = store.save();
    }
}

fn apply_config(app: &tauri::AppHandle, config: GameModeConfig) {
    {
        let mut lock = CONFIG.lock().unwrap_or_else(|e| e.into_inner());
        *lock = Some(config);
    }
    // 配置变化 → 代次 +1，重启扫描线程
    let gen = GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    let app = app.clone();
    thread::spawn(move || {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sweep_loop(app, gen)));
    });
}

// ─── 豁免判定 ───

const CORE_SYSTEM_PROCESSES: &[&str] = &[
    // 内核与系统服务（Pavise PresetWhitelist 同款）
    "system", "secure system", "registry", "memory compression",
    "smss", "csrss", "wininit", "winlogon", "services", "lsass", "svchost",
    "dwm", "fontdrvhost", "audiodg", "wudfhost", "wmiprvse",
    // 外壳与系统 UI：被压会直接导致桌面/任务栏/输入法卡顿
    "explorer", "ctfmon", "textinputhost", "sihost", "taskhostw",
    "conhost", "dllhost", "runtimebroker", "applicationframehost",
    "shellexperiencehost", "startmenuexperiencehost", "searchhost",
    "lockapp", "logonui", "securityhealthsystray",
    // 虚拟化（WSL / Hyper-V）
    "vmmem", "vmmemwsl", "wslservice",
];

/// 性能监控/叠加层：自身被压制会导致监控采样变慢、OSD 卡顿，永不压制
const PERF_MONITOR_PROCESSES: &[&str] = &["msiafterburner", "rtss"];

const INPUT_AUDIO_KEYWORDS: &[&str] = &[
    "keyboard", "mouse", "hotkey", "keymap", "macro", "autohotkey", "hid", "input", "ime",
    "pinyin", "qqpy", "sogou", "sgtool", "wetype", "iflyime", "wubi", "audio", "sound", "voice",
    "headset", "nahimic", "realtek", "rtkaud", "creative", "logi", "lghub", "lcore", "razer",
    "synapse", "icue", "corsair", "steelseries", "armoury", "wooting", "keychron", "dareu",
    "rapoo", "bloody", "a4tech", "vgn", "langtu", "gamepp",
];

const PLATFORM_SHELL_KEYWORDS: &[&str] = &[
    "steam", "epic", "wegame", "battle.net", "blizzard", "agent", "galaxy", "ubisoft",
    "uplay", "leagueclient", "riotclient", "eadesktop", "origin", "xbox", "gamebar",
];

/// 网游加速器（Pavise NetAcceleratorCatalog 同款）：被压制会直接恶化游戏延迟，永不压制
const ACCELERATOR_PROCESSES: &[&str] = &[
    "uu", "uu_ball", "xunyou", "leigod", "leigod_launcher", "leishensdk", "qiyou",
    "biubiu", "bbservice", "dolphinq", "wtfast",
];

const ACCELERATOR_KEYWORDS: &[&str] = &[
    "accelerat", "booster", "加速器", "xunyou", "leigod", "leishen", "qiyou", "biubiu",
    "dolphinq", "wtfast", "exitlag", "noping", "mudfish",
];

/// 笔记本厂商控制台/整机管家：负责风扇曲线、性能/功耗模式、灯效与驱动管理。
/// 被压制（EcoQoS+小核）会导致风扇失控、性能模式切换失效 → 热节流降频，永不压制。
const OEM_CONSOLE_KEYWORDS: &[&str] = &[
    // 联想 / 拯救者（Vantage、Legion Zone、联想电脑管家）
    "lenovo", "vantage", "legion",
    // 惠普 / 暗影精灵（OMEN Gaming Hub、HP Support Assistant）
    "omen", "hpsupport",
    // 戴尔 / 外星人（Alienware Command Center、Dell Optimizer、MyDell）
    "alienware", "awcc", "delloptimizer", "mydell",
    // 华硕 / ROG（Armoury Crate、MyASUS、奥创中心）
    "asus", "armoury", "ghelper",
    // 微星（MSI Center / Dragon Center / Creator Center）
    "msicenter", "msi center", "dragoncenter", "creatorcenter",
    // 宏碁（PredatorSense、NitroSense、Acer Care Center）
    "acer", "predator", "nitrosense",
    // 机械革命 / 神舟 / 炫龙 / 雷神等 Clevo 系（Control Center）
    "mechrevo", "controlcenter", "machenike", "thunderobot",
    // 七彩虹（iGame Center）
    "igamecenter",
    // 技嘉 / AORUS（Gigabyte Control Center、AORUS Engine）
    "gigabyte", "aorus",
    // 华为 / 荣耀（电脑管家、荣耀智慧互联）
    "huawei", "honor", "magicbook",
    // 小米 / Redmi（小米笔记本管家等）
    "xiaomi", "redmi",
    // 三星（Samsung Settings / Update）
    "samsung",
];

fn process_matches(process_name: &str, entry: &str) -> bool {
    let norm = |s: &str| -> String {
        let lower = s.to_ascii_lowercase();
        let stem = if lower.ends_with(".exe") {
            &s[..s.len() - 4]
        } else {
            s
        };
        stem.trim().to_ascii_lowercase()
    };
    norm(process_name) == norm(entry)
}

/// 剥离 ".exe" 后缀（sysinfo 在 Windows 上返回的进程名带后缀）
fn strip_exe_suffix(name: &str) -> &str {
    match name.strip_suffix(".exe").or_else(|| name.strip_suffix(".EXE")) {
        Some(s) => s,
        None => name,
    }
}

fn is_anticheat(name: &str) -> bool {
    for g in anticheat::GROUPS {
        for p in g.processes {
            if process_matches(name, p) {
                return true;
            }
        }
    }
    false
}

fn is_core_system(name: &str) -> bool {
    let low = strip_exe_suffix(name).to_ascii_lowercase();
    CORE_SYSTEM_PROCESSES.iter().any(|&s| low == s)
}

fn is_perf_monitor(name: &str) -> bool {
    let low = strip_exe_suffix(name).to_ascii_lowercase();
    PERF_MONITOR_PROCESSES.iter().any(|&s| low == s)
}

fn is_input_audio(name: &str) -> bool {
    let low = strip_exe_suffix(name).to_ascii_lowercase();
    INPUT_AUDIO_KEYWORDS.iter().any(|&k| low.contains(k))
}

fn is_platform_shell(name: &str) -> bool {
    let low = name.to_ascii_lowercase();
    PLATFORM_SHELL_KEYWORDS.iter().any(|&k| low.contains(k))
}

fn is_net_accelerator(name: &str) -> bool {
    let low = strip_exe_suffix(name).to_ascii_lowercase();
    if ACCELERATOR_PROCESSES.iter().any(|&s| low == s) {
        return true;
    }
    ACCELERATOR_KEYWORDS.iter().any(|&k| low.contains(k))
}

fn is_oem_console(name: &str) -> bool {
    let low = strip_exe_suffix(name).to_ascii_lowercase();
    OEM_CONSOLE_KEYWORDS.iter().any(|&k| low.contains(k))
}

// ─── 收集 pid 及其所有后代进程（用于竞技档只放行游戏家族） ───

/// 当前前台窗口所属进程 PID（用户正在交互的对象，永不压制）
fn foreground_pid() -> Option<u32> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            None
        } else {
            Some(pid)
        }
    }
}

fn collect_family(root: u32, system: &System) -> HashSet<u32> {
    let mut family = HashSet::new();
    if root == 0 {
        return family;
    }
    family.insert(root);
    loop {
        let mut added = false;
        for (_, process) in system.processes() {
            let pid = process.pid().as_u32();
            if family.contains(&pid) {
                continue;
            }
            if let Some(parent) = process.parent() {
                if family.contains(&parent.as_u32()) {
                    family.insert(pid);
                    added = true;
                }
            }
        }
        if !added {
            break;
        }
    }
    family
}

// ─── 快照 / 压制 / 还原 ───

fn capture_snapshot(pid: u32) -> Option<ProcessSnapshot> {
    let pri = optimization::query_process_priority(pid)?;
    Some(ProcessSnapshot {
        pri,
        aff: optimization::query_process_affinity(pid).unwrap_or(0),
        io: optimization::query_process_io_priority(pid),
        mem: optimization::query_process_memory_priority(pid),
        eco_on: optimization::query_process_eco_state(pid)
            .map(|(_, state)| state & 1 != 0)
            .unwrap_or(false),
        gpu: optimization::query_process_gpu_priority(pid),
    })
}

/// 应用压制；返回是否完全成功
fn apply_level(pid: u32, level: SuppressionLevel, background_mask: u64) -> bool {
    let mut ok = true;
    match level {
        SuppressionLevel::Eco => {
            // 低于正常优先级 + EcoQoS：任务管理器显示"效率模式"绿叶
            ok &= optimization::set_process_priority_class(pid, BELOW_NORMAL_PRIORITY_CLASS);
            ok &= optimization::set_process_io_priority(pid, IO_LOW);
            ok &= optimization::set_process_memory_priority(pid, MEM_MEDIUM);
            ok &= optimization::set_process_eco_qos(pid, true);
        }
        SuppressionLevel::Isolated => {
            // 低于正常优先级 + EcoQoS：任务管理器显示"效率模式"绿叶
            ok &= optimization::set_process_priority_class(pid, BELOW_NORMAL_PRIORITY_CLASS);
            ok &= optimization::set_process_io_priority(pid, IO_VERY_LOW);
            ok &= optimization::set_process_memory_priority(pid, MEM_VERY_LOW);
            ok &= optimization::set_process_eco_qos(pid, true);
            if background_mask != 0 {
                ok &= optimization::set_process_affinity(pid, background_mask);
            }
            ok &= optimization::set_process_gpu_priority(pid, GPU_IDLE);
        }
        SuppressionLevel::None => {}
    }
    ok
}

/// 按快照还原进程原始值
fn restore_level(pid: u32, snapshot: &ProcessSnapshot) {
    optimization::set_process_priority_class(pid, snapshot.pri);
    if snapshot.aff != 0 {
        optimization::set_process_affinity(pid, snapshot.aff);
    }
    if let Some(io) = snapshot.io {
        optimization::set_process_io_priority(pid, io);
    }
    if let Some(mem) = snapshot.mem {
        optimization::set_process_memory_priority(pid, mem);
    }
    // EcoQoS：仅当原本未开启时关闭，否则保持
    if !snapshot.eco_on {
        optimization::set_process_eco_qos(pid, false);
    }
    if let Some(gpu) = snapshot.gpu {
        optimization::set_process_gpu_priority(pid, gpu);
    }
}

fn acquire(pid: u32, level: SuppressionLevel, background_mask: u64) -> bool {
    let mut core = CORE.lock().unwrap_or_else(|e| e.into_inner());
    let map = core.get_or_insert_with(HashMap::new);
    if let Some(entry) = map.get(&pid) {
        if entry.level == level {
            return true; // 已按同等级压制
        }
        // 等级变化：用已存快照重新压制
        let ok = apply_level(pid, level, background_mask);
        if let Some(e) = map.get_mut(&pid) {
            e.level = level;
        }
        return ok;
    }

    let Some(snapshot) = capture_snapshot(pid) else {
        return false;
    };
    let ok = apply_level(pid, level, background_mask);
    map.insert(pid, Entry { snapshot, level });
    ok
}

fn release_pid(pid: u32) {
    let mut core = CORE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(map) = core.as_mut() {
        if let Some(entry) = map.remove(&pid) {
            restore_level(pid, &entry.snapshot);
        }
    }
}

/// 释放全部压制（模式关闭 / 应用退出兜底）
fn release_all() {
    let mut core = CORE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(map) = core.as_mut() {
        let entries: Vec<(u32, ProcessSnapshot)> = map
            .drain()
            .map(|(pid, e)| (pid, e.snapshot))
            .collect();
        for (pid, snapshot) in entries {
            restore_level(pid, &snapshot);
        }
    }
    if let Some(blocked) = BLOCKED.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        blocked.clear();
    }
}

// ─── 扫描循环 ───

fn sweep_loop(app: tauri::AppHandle, generation: u64) {
    let mut system = System::new();
    let mut prev_game_running = false;
    loop {
        if GENERATION.load(Ordering::Relaxed) != generation {
            break;
        }
        thread::sleep(Duration::from_secs(POLL_INTERVAL_SECS));
        if GENERATION.load(Ordering::Relaxed) != generation {
            break;
        }

        let config = get_config();
        system.refresh_processes_specifics(ProcessRefreshKind::everything());
        // 游戏运行状态：仅供状态展示与竞技档豁免游戏进程家族，也参与自动切换判定
        let game_running = game_filter::any_game_running(&system);
        // 游戏会话结束（运行→退出）：清除手动覆盖，恢复自动切换能力。
        // 这样下次启动游戏时仍按「自动档」切换（用户未改设置）。
        if prev_game_running && !game_running {
            MANUAL_OVERRIDE.store(false, Ordering::Relaxed);
        }
        prev_game_running = game_running;
        // 生效档位：游戏运行且未手动覆盖且设置了自动档时用自动档，否则用用户手动选择。
        // 手动覆盖（MANUAL_OVERRIDE）仅对当前游戏会话生效，游戏退出后自动清除。
        let manual_override = MANUAL_OVERRIDE.load(Ordering::Relaxed);
        let effective = if game_running
            && !manual_override
            && config.auto_preset != GameModePreset::Default
        {
            config.auto_preset
        } else {
            config.preset
        };
        let active = effective != GameModePreset::Default;

        {
            let mut cur = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
            *cur = active;
        }
        {
            let mut cur = GAME_RUNNING.lock().unwrap_or_else(|e| e.into_inner());
            *cur = game_running;
        }
        {
            let mut cur = EFFECTIVE_PRESET.lock().unwrap_or_else(|e| e.into_inner());
            if *cur != effective {
                *cur = effective;
                // 生效档位变化 → 通知前端顶栏同步
                let _ = app.emit("game-mode-effective-changed", effective.as_str());
            }
        }

        if !active {
            if !CORE.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map_or(true, |m| m.is_empty())
            {
                release_all();
            }
            continue;
        }

        let background_mask = anticheat::get_e_core_mask();
        let self_pid = std::process::id() as u32;

        // 竞技档：识别滤镜名单内正在运行的游戏进程，只放行其进程家族
        let game_pids = game_filter::running_game_pids(&system);
        let mut game_family: HashSet<u32> = HashSet::new();
        if effective == GameModePreset::Competitive {
            for pid in &game_pids {
                game_family.extend(collect_family(*pid, &system));
            }
        }

        // 前台窗口进程家族：无论档位与名单，用户正在交互的对象一律放行
        let fg_family = foreground_pid()
            .map(|pid| collect_family(pid, &system))
            .unwrap_or_default();

        let self_session = system
            .process(Pid::from_u32(self_pid))
            .and_then(|p| p.session_id());

        let mut should_suppress: HashSet<u32> = HashSet::new();

        for (pid_ref, process) in system.processes() {
            let pid = pid_ref.as_u32();
            if pid <= 4 || pid == self_pid {
                continue;
            }
            let name = process.name().to_string();
            if name.is_empty() {
                continue;
            }
            // 会话过滤：不同会话的进程不压制
            if let Some(sess) = self_session {
                if process.session_id().map(|s| s != sess).unwrap_or(false) {
                    continue;
                }
            }
            if is_anticheat(&name) || is_core_system(&name) || is_input_audio(&name)
                || is_platform_shell(&name) || is_perf_monitor(&name) || is_net_accelerator(&name)
                || is_oem_console(&name)
            {
                continue;
            }
            if game_family.contains(&pid) {
                continue;
            }
            // 前台窗口进程家族：绝不压制
            if fg_family.contains(&pid) {
                continue;
            }
            // 本会话免压名单
            if BLOCKED
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
                .map(|s| s.contains(&name.to_ascii_lowercase()))
                .unwrap_or(false)
            {
                continue;
            }
            // 常规档：只压明显占资源的
            if effective == GameModePreset::Regular && process.cpu_usage() < REGULAR_CPU_THRESHOLD
            {
                continue;
            }
            should_suppress.insert(pid);
        }

        let level = if effective == GameModePreset::Competitive {
            SuppressionLevel::Isolated
        } else {
            SuppressionLevel::Eco
        };

        // 压制本期目标
        for pid in &should_suppress {
            let ok = acquire(*pid, level, background_mask);
            if !ok {
                // 受保护/不可写：计入免压名单，本期不再重试
                if let Some(process) = system.process(Pid::from_u32(*pid)) {
                    let low = process.name().to_string().to_ascii_lowercase();
                    BLOCKED
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .get_or_insert_with(HashSet::new)
                        .insert(low);
                }
            }
        }

        // 释放本期不再需要压制的进程
        let core_map = CORE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(map) = core_map.as_ref() {
            let to_release: Vec<u32> = map.keys().copied().filter(|p| !should_suppress.contains(p)).collect();
            drop(core_map);
            for pid in to_release {
                release_pid(pid);
            }
        }
    }
}

// ─── 初始化 ───

pub async fn init(app: tauri::AppHandle) -> Result<(), String> {
    let config = load_persisted_config(&app).await;
    {
        let mut lock = CONFIG.lock().map_err(|e| e.to_string())?;
        *lock = Some(config);
    }
    // 始终启动扫描线程（默认档也要能检测游戏运行并自动切竞技）
    let gen = GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    let app_clone = app.clone();
    thread::spawn(move || {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sweep_loop(app_clone, gen)));
    });
    Ok(())
}

/// 应用退出时调用：释放所有压制，避免残留
pub fn shutdown() {
    release_all();
}

// ─── Tauri 命令 ───

#[tauri::command]
pub async fn game_mode_get_config(_app: tauri::AppHandle) -> Result<GameModeConfig, String> {
    Ok(get_config())
}

#[tauri::command]
pub async fn game_mode_set_preset(
    app: tauri::AppHandle,
    preset: String,
) -> Result<(), String> {
    let mut config = get_config();
    let new = GameModePreset::from_str(&preset);
    config.preset = new;
    if new == GameModePreset::Default {
        config.manual_enabled = false;
    } else {
        // 顶栏选择模式即激活（等价于手动开启）
        config.manual_enabled = true;
    }
    // 手动切换即视为手动覆盖：自动切换不再生效，直至重新设置自动档
    MANUAL_OVERRIDE.store(true, Ordering::Relaxed);
    save_persisted_config(&app, &config).await;
    apply_config(&app, config);
    Ok(())
}

#[tauri::command]
pub async fn game_mode_set_manual(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut config = get_config();
    if config.preset == GameModePreset::Default {
        return Err("请先选择常规或竞技模式".to_string());
    }
    config.manual_enabled = enabled;
    save_persisted_config(&app, &config).await;
    apply_config(&app, config);
    Ok(())
}

#[tauri::command]
pub async fn game_mode_set_auto(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let mut config = get_config();
    config.auto_enabled = enabled;
    save_persisted_config(&app, &config).await;
    apply_config(&app, config);
    Ok(())
}

/// 设置「游戏启动时自动切换」档位（default=关 / regular=常规 / competitive=竞技）。
/// 重新设置自动档会清除手动覆盖标记，恢复自动切换能力。
#[tauri::command]
pub async fn game_mode_set_auto_preset(
    app: tauri::AppHandle,
    preset: String,
) -> Result<(), String> {
    let mut config = get_config();
    config.auto_preset = GameModePreset::from_str(&preset);
    MANUAL_OVERRIDE.store(false, Ordering::Relaxed);
    save_persisted_config(&app, &config).await;
    apply_config(&app, config);
    Ok(())
}

#[tauri::command]
pub async fn game_mode_get_status(_app: tauri::AppHandle) -> Result<GameModeStatus, String> {
    let config = get_config();
    let active = *ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
    let game_running = *GAME_RUNNING.lock().unwrap_or_else(|e| e.into_inner());
    let suppressed_count = CORE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|m| m.len())
        .unwrap_or(0);
    let effective = *EFFECTIVE_PRESET.lock().unwrap_or_else(|e| e.into_inner());
    Ok(GameModeStatus {
        preset: config.preset.as_str().to_string(),
        effective_preset: effective.as_str().to_string(),
        manual_enabled: config.manual_enabled,
        auto_enabled: config.auto_enabled,
        active,
        suppressed_count,
        game_running,
    })
}