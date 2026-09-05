//! FPS 监控模块 — 内嵌 ETW 版（参考图吧工具箱 TubaWinUi3 FpsService.cs 移植）
//!
//! 核心改进：
//! 1. 彻底移除外部 PresentMon 进程 — 不再 spawn 子进程、不再解析 stdout CSV
//! 2. 进程内直接开启 ETW 实时会话（DxgKrnl / Win32k 两个内核 Provider），
//!    监听 Present 事件族就地算帧率（与 PresentMon / CapFrameX 同源技术）
//! 3. 完整移植参考实现的「同帧去重 + 信源优先级」逻辑：
//!    PresentHistory(0xAB/0xD7) ＞ Win32k 合成(0xC9) ＞ 传统/MPO 事件，
//!    每帧只计入一次，防止 MPO/Win32k 双源导致 FPS 翻倍
//! 4. 保留原有统计管线：环形缓冲区(3000帧) + EMA(α=0.2) 平滑 + 1%/0.1% Low，
//!    前台目标消费端过滤不变，公共 API 全兼容
//!
//! 架构说明：
//! - etw::fps_monitor_main 线程：管理 ETW 会话生命周期，会话中断自动重连（上限 5 次）
//! - etw 回调线程（即 ProcessTrace 回调，单线程串行）：解析事件头 → 去重 → 目标过滤 → 统计管线
//! - process_tracker_loop 线程：追踪前台窗口，更新 TARGET_PROCESS，检测 FPS 过期归零
//!
//! 需要管理员权限才能启用内核 Provider（与旧 PresentMon 方案一致）。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use parking_lot::RwLock;

// ============ 全局状态（8 项） ============

/// 平滑后的 FPS 值，供 overlay 读取
static SMOOTHED_FPS: AtomicU32 = AtomicU32::new(0);
/// FPS 监控是否处于活跃状态
static FPS_ACTIVE: AtomicBool = AtomicBool::new(false);
/// 自身 overlay 窗口句柄（用于排除前台切换到自身 overlay）
static OVERLAY_HWND: AtomicU64 = AtomicU64::new(0);
/// 当前前台窗口 PID（钩子回调中快速存储）
static CURRENT_FG_PID: AtomicU32 = AtomicU32::new(0);
/// 当前前台目标进程名（小写 exe 文件名，如 "game.exe"）— RwLock 读多写少
static TARGET_PROCESS: RwLock<String> = parking_lot::RwLock::new(String::new());
/// 上次匹配到目标帧数据的时间戳（ms），用于检测 FPS 过期归零
static LAST_FRAME_TIME: AtomicU64 = AtomicU64::new(0);
/// 目标切换代次（每次切换 +1），ETW 回调检测到变化后清空缓冲区
static TARGET_GENERATION: AtomicU64 = AtomicU64::new(0);
/// 1% Low FPS（最慢 1% 帧的平均 FPS），参考 CapFrameX OnePercentLowAverage
static ONE_PCT_LOW_FPS: AtomicU32 = AtomicU32::new(0);
/// 0.1% Low FPS（最慢 0.1% 帧的平均 FPS），参考 CapFrameX ZerodotOnePercentLowAverage
static ZERO_DOT_ONE_PCT_LOW_FPS: AtomicU32 = AtomicU32::new(0);

/// 自进程 PID（ETW 回调中排除自身）
static SELF_PID: OnceLock<u32> = OnceLock::new();

// ============ Windows 前台窗口钩子 ============

#[cfg(target_os = "windows")]
mod win32_fg {
    use super::*;
    use std::ptr;
    use windows_sys::Win32::Foundation::*;
    use windows_sys::Win32::UI::Accessibility::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    static HOOK_HANDLE: Mutex<usize> = Mutex::new(0);

    /// 前台窗口切换回调 — 仅存储 PID，不做耗时操作
    unsafe extern "system" fn on_foreground_changed(
        _hook: HWINEVENTHOOK,
        _event: u32,
        hwnd: HWND,
        _id_object: i32,
        _id_child: i32,
        _id_event_thread: u32,
        _dw_event_time: u32,
    ) {
        if !FPS_ACTIVE.load(Ordering::SeqCst) {
            return;
        }
        // 排除自身 overlay 窗口
        let overlay = OVERLAY_HWND.load(Ordering::Relaxed) as usize;
        if overlay != 0 && hwnd as usize == overlay {
            return;
        }

        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid != 0 {
            CURRENT_FG_PID.store(pid, Ordering::Relaxed);
        }
    }

    /// 注册前台窗口切换事件钩子
    pub unsafe fn register_hook() -> bool {
        let hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            ptr::null_mut(),
            Some(on_foreground_changed),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        if !hook.is_null() {
            *HOOK_HANDLE.lock().unwrap() = hook as usize;
            true
        } else {
            log::warn!("FPS监控: 前台窗口 Hook 注册失败");
            false
        }
    }

    /// 注销前台窗口切换事件钩子
    pub unsafe fn unregister_hook() {
        let mut lock = HOOK_HANDLE.lock().unwrap();
        if *lock != 0 {
            UnhookWinEvent(*lock as *mut _);
            *lock = 0;
        }
    }

    /// 初始化时获取当前前台窗口 PID
    pub fn init_foreground_process() {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                return;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid != 0 {
                CURRENT_FG_PID.store(pid, Ordering::Relaxed);
            }
        }
    }
}

// ============ 排除名单与进程名匹配 ============

/// 排除系统/桌面进程（文件名去 .exe 后缀后大小写不敏感比较，移植自图吧工具箱 Excluded 名单）
fn is_excluded_process(name: &str) -> bool {
    const EXCLUDED: &[&str] = &[
        "dwm", "explorer", "searchhost", "shellexperiencehost",
        "startmenuexperiencehost", "runtimebroker", "applicationframehost",
        "sihost", "taskhostw", "ctfmon", "msedgewebview2", "microsoftedge",
        "searchapp", "svchost", "csrss", "smss", "lsass", "wininit",
        "services", "winlogon", "fontdrvhost", "dllhost", "conhost",
        "taskmgr", "systemsettings", "windowsterminal", "cmd", "powershell",
        "catpawai", "textinputhost", "shell", "memory compression",
        "registry", "memcompression", "idle", "system", "ntoskrnl",
        "interrupt", "dpcs", "nexbox",
    ];
    let stem = strip_exe_suffix(name);
    EXCLUDED.iter().any(|e| stem.eq_ignore_ascii_case(e))
}

/// 判断进程是否为非游戏进程（大小写不敏感，避免 to_lowercase 堆分配）
fn is_non_game_process(name: &str) -> bool {
    const NON_GAME_PROCESSES: &[&str] = &[
        "explorer.exe", "dwm.exe", "windowsterminal.exe", "cmd.exe",
        "powershell.exe", "conhost.exe", "catpawai.exe", "nexbox.exe",
        "msedgewebview2.exe", "searchhost.exe", "shellexperiencehost.exe",
        "sihost.exe", "ctfmon.exe", "textinputhost.exe",
        "applicationframehost.exe", "winlogon.exe", "fontdrvhost.exe",
        "systemsettings.exe", "taskmgr.exe",
    ];
    NON_GAME_PROCESSES.iter().any(|p| name.eq_ignore_ascii_case(p))
}

/// 去除 .exe 后缀（大小写不敏感）
#[inline]
fn strip_exe_suffix(s: &str) -> &str {
    if s.len() >= 4 && s[s.len() - 4..].eq_ignore_ascii_case(".exe") {
        &s[..s.len() - 4]
    } else {
        s
    }
}

/// 进程名匹配：支持带/不带 .exe 后缀，以及 java/javaw 互通
///
/// 优化：使用 eq_ignore_ascii_case 避免 to_lowercase() 堆分配
/// 参考 CapFrameX OnlineMetricService.UpdateOnlineMetrics 的消费端过滤
fn process_name_matches(app: &str, target: &str) -> bool {
    if target.is_empty() {
        return false;
    }
    // 大小写不敏感直接比较
    if app.eq_ignore_ascii_case(target) {
        return true;
    }
    // 去除 .exe 后缀后再比较
    let app_no_exe = strip_exe_suffix(app);
    let target_no_exe = strip_exe_suffix(target);
    if app_no_exe.eq_ignore_ascii_case(target_no_exe) {
        return true;
    }
    // java ↔ javaw 互通（Minecraft Java 版可能用 javaw.exe）
    let app_is_java = app_no_exe.eq_ignore_ascii_case("java")
        || app_no_exe.eq_ignore_ascii_case("javaw");
    let target_is_java = target_no_exe.eq_ignore_ascii_case("java")
        || target_no_exe.eq_ignore_ascii_case("javaw");
    app_is_java && target_is_java
}

/// 当前 Unix 毫秒时间戳
#[inline]
fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ============ 帧时间环形缓冲区 ============

/// 固定大小环形缓冲区，O(1) 写入，O(n) 计算（n 为窗口内帧数）
///
/// 参考 CapFrameX CircularBuffer 和 capframex-linux timing.c 的 frame_buffer
struct FrameTimeBuffer {
    data: Vec<f64>,
    head: usize,
    count: usize,
}

impl FrameTimeBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            data: vec![0.0; capacity],
            head: 0,
            count: 0,
        }
    }

    /// O(1) 写入，自动覆盖最旧数据
    fn push(&mut self, value: f64) {
        if self.data.is_empty() {
            return;
        }
        self.data[self.head] = value;
        self.head = (self.head + 1) % self.data.len();
        if self.count < self.data.len() {
            self.count += 1;
        }
    }

    fn clear(&mut self) {
        self.head = 0;
        self.count = 0;
    }

    /// 计算窗口内平均帧时间对应的 FPS
    ///
    /// FPS = 1000 * count / sum(frametimes)
    /// 等价于 CapFrameX 的 GetFpsMetricValue(Average)
    fn average_fps(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        let sum: f64 = if self.count < self.data.len() {
            // 缓冲区未满，有效数据在 [0..count)
            self.data[..self.count].iter().sum()
        } else {
            // 缓冲区已满，整个数组都有效
            self.data.iter().sum()
        };
        if sum <= 0.0 {
            return 0.0;
        }
        1000.0 * self.count as f64 / sum
    }

    /// Calculate x% Low FPS (average FPS of the worst x% frames)
    ///
    /// Algorithm (from CapFrameX GetPercentageHighAverageSequence):
    /// 1. Sort frame times ascending
    /// 2. Get the (1-x) quantile value as threshold
    /// 3. Select all frame times >= threshold (the slowest x%)
    /// 4. Average those frame times
    /// 5. FPS = 1000 / average
    ///
    /// Parameter: low_percent — 0.01 for 1% Low, 0.001 for 0.1% Low
    fn percentile_low_fps(&self, low_percent: f64) -> f64 {
        if self.count == 0 {
            return 0.0;
        }

        // Collect valid frame times
        let samples: Vec<f64> = if self.count < self.data.len() {
            self.data[..self.count].to_vec()
        } else {
            self.data.clone()
        };

        if samples.is_empty() {
            return 0.0;
        }

        // Sort ascending
        let mut sorted = samples;
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // Get the (1 - low_percent) quantile as threshold
        // e.g. 1% Low -> p_quantile = 0.99 -> 99th percentile of frame times
        let p_quantile = 1.0 - low_percent;
        let q_idx = (p_quantile * (sorted.len() - 1) as f64).round() as usize;
        let q_idx = q_idx.min(sorted.len() - 1);
        let quantile_val = sorted[q_idx];

        // Select all frame times >= threshold (the slowest low_percent)
        let low_frametimes: Vec<f64> = sorted
            .iter()
            .filter(|&&x| x >= quantile_val)
            .cloned()
            .collect();

        if low_frametimes.is_empty() {
            return 0.0;
        }

        let avg = low_frametimes.iter().sum::<f64>() / low_frametimes.len() as f64;
        if avg <= 0.0 {
            return 0.0;
        }

        1000.0 / avg
    }
}

// ============ ETW 采集（内嵌会话，参考图吧工具箱 FpsService.cs） ============

#[cfg(target_os = "windows")]
mod etw {
    use super::*;
    use std::mem::size_of;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Diagnostics::Etw::*;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    // ---- Provider GUID（与图吧工具箱 FpsService.cs 完全一致） ----
    // Microsoft-Windows-DxgKrnl: 802EC45A-1E99-4B83-9920-87C98277BA9D
    const DXGKRNL_PROVIDER: windows_sys::core::GUID = windows_sys::core::GUID {
        data1: 0x802EC45A,
        data2: 0x1E99,
        data3: 0x4B83,
        data4: [0x99, 0x20, 0x87, 0xC9, 0x82, 0x77, 0xBA, 0x9D],
    };
    // Microsoft-Windows-Win32k: 8C416C79-D49B-4F01-A467-E56D3AA8234C
    const WIN32K_PROVIDER: windows_sys::core::GUID = windows_sys::core::GUID {
        data1: 0x8C416C79,
        data2: 0xD49B,
        data3: 0x4F01,
        data4: [0xA4, 0x67, 0xE5, 0x6D, 0x3A, 0xA8, 0x23, 0x4C],
    };

    /// ETW 实时会话名（私有会话；停启/崩溃后靠「先停同名旧会话」防残留）
    fn session_name() -> Vec<u16> {
        "NexBox_FPS\0".encode_utf16().collect()
    }

    /// GUID 比较（windows-sys 0.59 的 GUID 未实现 PartialEq）
    fn guid_eq(a: &windows_sys::core::GUID, b: &windows_sys::core::GUID) -> bool {
        a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
    }

    // ---- Present 事件族（值 = EventHeader.EventDescriptor.Id，与参考实现一致） ----
    const PRESENT: u16 = 0x00B8; // Present（传统/全屏独占）
    const PRESENT_HISTORY_START: u16 = 0x00AB; // PresentHistory_Start（现代接管源，最高优先级）
    const PRESENT_HISTORY_DETAILED: u16 = 0x00D7; // PresentHistoryDetailed_Start
    const WIN32K_PRESENT: u16 = 0x00C9; // Win32k TokenCompositionSurfaceObject（合成/窗口化）
    const BLT: u16 = 0x00A6; // Blt_Info（MPO blt 路径）
    const MMIO_FLIP: u16 = 0x0074; // MMIOFlip_Info（MPO flip 路径）
    const MMIO_FLIP_MPO: u16 = 0x0103; // MMIOFlip_MPO
    const MMIO_FLIP_MPO3: u16 = 0x0182; // MMIOFlip_MPO3
    const FLIP: u16 = 0x00A8; // Flip_Info（硬件翻转）
    const FLIP_MPO: u16 = 0x00FC; // FlipMultiPlaneOverlay_Info
    const INDEPENDENT_FLIP: u16 = 0x010A; // IndependentFlip_Info

    // ---- 时间窗口（EventHeader.TimeStamp 单位为 100ns；差值计算与时钟源无关） ----
    /// 0.1ms：同一帧去重窗口。参考实现用 1ms（口径 FPS≤1000），但超高帧率场景
    /// （MC 高配 2000+ FPS，帧间隔 ~0.5ms）真实帧会被误判为同帧重复、帧率被腰斩。
    /// 同帧多源事件（0xA6+0x74 / 0xAB+0xD7 等）间隔为微秒级（<0.1ms），0.1ms 窗口
    /// 仍能挡住；漏网的由统计层 MIN_FRAME_MS 下限兜底。理论上限 10000 FPS。
    const SAME_FRAME_WINDOW_TICKS: i64 = 1_000;
    /// 4ms：Win32k 一帧可对应多个 composition surface（多窗口/UI 层）
    const WIN32K_FRAME_WINDOW_TICKS: i64 = 40_000;
    /// 500ms：信源优先级 shadow 窗口
    const MODE_WINDOW_TICKS: i64 = 5_000_000;
    /// 帧时间下限 0.1ms 与上限 2000ms，与旧 CSV 口径一致
    const MIN_FRAME_MS: f64 = 0.1;
    const MAX_FRAME_MS: f64 = 2000.0;
    /// 进程去重状态 5 分钟无帧即移除（对齐参考实现的 5min 清理）
    const STALE_PROC_TICKS: i64 = 300 * 10_000_000;

    /// ETW 线程句柄（stop 时 join，确保会话清理完成）
    static MONITOR_THREAD: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);
    /// 当前 OpenTrace 处理句柄值（stop 时强制 CloseTrace 兜底）
    static PROCESS_HANDLE: AtomicU64 = AtomicU64::new(0);

    /// 每进程去重/信源状态（仅由 ProcessTrace 回调线程独占读写，无需锁）
    struct EtwProcState {
        /// 最近一次被计数的 Present（同帧去重基准）
        last_present_ticks: i64,
        /// 最近 PresentHistory 事件 tick（最高信源）
        last_history_ticks: i64,
        /// 最近 Win32k 合成事件 tick（次高信源）
        last_win32k_ticks: i64,
    }

    impl Default for EtwProcState {
        fn default() -> Self {
            Self {
                last_present_ticks: 0,
                last_history_ticks: 0,
                last_win32k_ticks: 0,
            }
        }
    }

    /// ETW 回调运行时状态（全部由 ProcessTrace 回调线程独占，无需锁）
    struct EtwRuntime {
        /// 每进程 Present 去重状态
        procs: HashMap<u32, EtwProcState>,
        /// 进程名缓存（10s TTL，防 PID 复用误判）
        name_cache: HashMap<u32, (String, Instant)>,
        /// 帧时间环形缓冲区（3000 帧 ≈ 12.5s@240fps / 50s@60fps）
        buffer: FrameTimeBuffer,
        /// EMA 平滑状态
        smoothed_fps: f64,
        first_frame: bool,
        /// 1% Low / 0.1% Low 计算计时器
        last_low_calc: Instant,
        /// 目标进程最近一次被计数的 Present tick（帧间隔基准）
        last_target_ticks: Option<i64>,
        /// 已处理的目标切换代次
        last_gen: u64,
    }

    impl EtwRuntime {
        fn new() -> Self {
            Self {
                procs: HashMap::new(),
                name_cache: HashMap::new(),
                buffer: FrameTimeBuffer::new(3000),
                smoothed_fps: 0.0,
                first_frame: true,
                last_low_calc: Instant::now(),
                last_target_ticks: None,
                last_gen: 0,
            }
        }
    }

    /// EVENT_TRACE_PROPERTIES + 尾部会话名缓冲区（固定堆栈布局，规避字节数组对齐问题）
    #[repr(C)]
    struct TracePropsBuffer {
        props: EVENT_TRACE_PROPERTIES,
        name: [u16; 16],
    }

    impl TracePropsBuffer {
        fn new(name: &[u16]) -> Self {
            let mut b: TracePropsBuffer = unsafe { std::mem::zeroed() };
            b.props.Wnode.BufferSize = size_of::<TracePropsBuffer>() as u32;
            b.props.Wnode.Guid = windows_sys::core::GUID::from_u128(0);
            b.props.BufferSize = 64; // KB/缓冲
            b.props.MinimumBuffers = 8;
            b.props.MaximumBuffers = 64;
            b.props.LogFileMode = EVENT_TRACE_REAL_TIME_MODE;
            b.props.LoggerNameOffset = size_of::<EVENT_TRACE_PROPERTIES>() as u32;
            b.props.LogFileNameOffset = 0; // 无日志文件，纯实时
            for (i, &c) in name.iter().take(b.name.len()).enumerate() {
                b.name[i] = c;
            }
            b
        }
    }

    /// 停止同名的旧 ETW 会话（按名控制，崩溃残留也会被回收）
    fn stop_session_by_name(name: &[u16]) {
        let mut buf = TracePropsBuffer::new(name);
        unsafe {
            ControlTraceW(
                CONTROLTRACE_HANDLE { Value: 0 },
                name.as_ptr(),
                &mut buf.props,
                EVENT_TRACE_CONTROL_STOP,
            );
        }
    }

    /// 同帧去重 + 信源优先级（逐行移植图吧工具箱 FpsService.TryRecordPresent）
    ///
    /// 每进程信源优先级（以最近 500ms 内出现为准）：
    /// PresentHistory(0xAB/0xD7) ＞ Win32k 合成(0xC9) ＞ 传统 Present/MPO 事件。
    /// 低层信源只在高层信源缺席时作为兜底，避免同帧被多个事件双计。
    fn record_present(st: &mut EtwProcState, id: u16, ticks: i64) -> bool {
        if matches!(id, PRESENT_HISTORY_START | PRESENT_HISTORY_DETAILED) {
            st.last_history_ticks = ticks;
        } else if id == WIN32K_PRESENT {
            st.last_win32k_ticks = ticks;
        }

        // 同帧去重：一帧画面内核可能发出多个事件（微秒级间隔）
        let dup_window = if id == WIN32K_PRESENT {
            WIN32K_FRAME_WINDOW_TICKS
        } else {
            SAME_FRAME_WINDOW_TICKS
        };
        if ticks - st.last_present_ticks < dup_window {
            return false;
        }

        // 高层信源 500ms 内活跃时，忽略低层事件
        if !matches!(id, PRESENT_HISTORY_START | PRESENT_HISTORY_DETAILED)
            && ticks - st.last_history_ticks < MODE_WINDOW_TICKS
        {
            return false;
        }
        if matches!(
            id,
            PRESENT
                | BLT
                | MMIO_FLIP
                | MMIO_FLIP_MPO
                | MMIO_FLIP_MPO3
                | FLIP
                | FLIP_MPO
                | INDEPENDENT_FLIP
        ) && ticks - st.last_win32k_ticks < MODE_WINDOW_TICKS
        {
            return false;
        }

        st.last_present_ticks = ticks;
        true
    }

    /// 解析进程名（QueryFullProcessImageNameW，PROCESS_QUERY_LIMITED_INFORMATION 无需高权限）
    fn resolve_process_name(pid: u32) -> Option<String> {
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return None;
            }
            let mut buf = [0u16; 4096];
            let mut size = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut size);
            CloseHandle(h);
            if ok == 0 || size == 0 {
                return None;
            }
            let path = String::from_utf16_lossy(&buf[..size as usize]);
            let fname = path
                .rsplit(['\\', '/'])
                .next()
                .unwrap_or("")
                .to_string();
            if fname.is_empty() {
                return None;
            }
            Some(fname)
        }
    }

    /// 带 10s TTL 缓存的进程名查询（回调热路径，避免每次 OpenProcess）
    fn get_cached_name(rt: &mut EtwRuntime, pid: u32) -> Option<String> {
        if let Some((name, born)) = rt.name_cache.get(&pid) {
            if born.elapsed() < Duration::from_secs(10) {
                return Some(name.clone());
            }
        }
        let name = resolve_process_name(pid)?;
        rt.name_cache.insert(pid, (name.clone(), Instant::now()));
        Some(name)
    }

    /// 清理长期无帧的进程状态（对齐参考实现的 5min 移除）
    fn prune_stale(rt: &mut EtwRuntime, now_ticks: i64) {
        rt.procs
            .retain(|_, st| now_ticks - st.last_present_ticks < STALE_PROC_TICKS);
        rt.name_cache
            .retain(|_, (_, born)| born.elapsed() < Duration::from_secs(10));
    }

    /// ETW 事件回调：解析事件头 → 过滤 → 去重 → 目标过滤 → 统计管线
    /// 单线程串行派发（ProcessTrace 回调），EtwRuntime 位于该线程栈上。
    unsafe extern "system" fn on_event_record(record: *mut EVENT_RECORD) {
        if record.is_null() || !FPS_ACTIVE.load(Ordering::SeqCst) {
            return;
        }

        let h = &(*record).EventHeader;
        let pid = h.ProcessId;
        if pid <= 0 || pid == 4 || pid == *SELF_PID.get_or_init(|| std::process::id()) {
            return;
        }
        if pid == 0 {
            return;
        }

        let id = h.EventDescriptor.Id;
        let ticks = h.TimeStamp;

        // Provider + 事件 ID 过滤：只关心两 Provider 的 Present 事件族
        let is_present = if guid_eq(&h.ProviderId, &DXGKRNL_PROVIDER) {
            matches!(
                id,
                PRESENT
                    | PRESENT_HISTORY_START
                    | PRESENT_HISTORY_DETAILED
                    | BLT
                    | MMIO_FLIP
                    | MMIO_FLIP_MPO
                    | MMIO_FLIP_MPO3
                    | FLIP
                    | FLIP_MPO
                    | INDEPENDENT_FLIP
            )
        } else if guid_eq(&h.ProviderId, &WIN32K_PROVIDER) {
            id == WIN32K_PRESENT
        } else {
            false
        };
        if !is_present {
            return;
        }

        // 运行时状态（由 OpenTraceW 时传入的 Context 回填到 UserContext）
        let rt = &mut *((*record).UserContext as *mut EtwRuntime);

        // 进程名（10s TTL 缓存）+ 排除名单
        let name = match get_cached_name(rt, pid) {
            Some(n) => n,
            None => return, // 进程已退出/取不到名，视为排除
        };
        if is_excluded_process(&name) {
            return;
        }

        // 同帧去重 + 信源优先级 → 决定本帧是否计入
        let st = rt.procs.entry(pid).or_default();
        if !record_present(st, id, ticks) {
            return;
        }

        // 消费端过滤：仅统计当前前台目标进程
        {
            let target = TARGET_PROCESS.read();
            if !process_name_matches(&name, &target) {
                return;
            }
        }
        LAST_FRAME_TIME.store(now_unix_ms(), Ordering::Relaxed);

        // 目标切换 → 清空统计（原 CSV reader 的 generation 检测逻辑）
        let gen = TARGET_GENERATION.load(Ordering::Relaxed);
        if gen != rt.last_gen {
            rt.buffer.clear();
            rt.smoothed_fps = 0.0;
            rt.first_frame = true;
            rt.last_target_ticks = None;
            rt.last_gen = gen;
            ONE_PCT_LOW_FPS.store(0, Ordering::Relaxed);
            ZERO_DOT_ONE_PCT_LOW_FPS.store(0, Ordering::Relaxed);
        }

        // 帧时间推进：上一目标帧与本帧的 tick 差值 → ms（100ns 单位）
        if let Some(prev) = rt.last_target_ticks {
            let ms = (ticks - prev) as f64 / 10_000.0;
            if ms < MIN_FRAME_MS {
                // 假帧（同帧重复事件漏网，间隔 <0.1ms）：不进统计、不重置基准，
                // 让下一帧的间隔仍从上一个真帧起算，避免把真实帧间隔切短、读数偏高
                return;
            }
            if ms < MAX_FRAME_MS {
                rt.buffer.push(ms);
                let raw_fps = rt.buffer.average_fps();
                if raw_fps > 0.0 {
                    if rt.first_frame {
                        rt.smoothed_fps = raw_fps;
                        rt.first_frame = false;
                    } else {
                        // EMA 系数 0.2：值越小越平滑，越大越灵敏
                        rt.smoothed_fps = 0.2 * raw_fps + 0.8 * rt.smoothed_fps;
                    }
                    SMOOTHED_FPS.store(rt.smoothed_fps.round() as u32, Ordering::Relaxed);
                }

                // 每 ~1 秒计算 1% Low / 0.1% Low
                if rt.last_low_calc.elapsed() >= Duration::from_secs(1) {
                    if rt.buffer.count >= 100 {
                        let one_low = rt.buffer.percentile_low_fps(0.01);
                        if one_low > 0.0 {
                            ONE_PCT_LOW_FPS.store(one_low.round() as u32, Ordering::Relaxed);
                        }
                    }
                    if rt.buffer.count >= 1000 {
                        let z_one_low = rt.buffer.percentile_low_fps(0.001);
                        if z_one_low > 0.0 {
                            ZERO_DOT_ONE_PCT_LOW_FPS.store(z_one_low.round() as u32, Ordering::Relaxed);
                        }
                    }
                    rt.last_low_calc = Instant::now();
                    prune_stale(rt, ticks);
                }
            }
        }
        // 真帧 / 超长间隔帧（暂停后恢复）：更新基准
        rt.last_target_ticks = Some(ticks);
    }

    /// 单次 ETW 会话：创建 → 启用 Provider → 阻塞消费 → 清理。
    /// 返回 false 表示会话无法继续（需彻底放弃或重试无意义）。
    fn run_etw_session() -> bool {
        let mut name = session_name();
        // 预清理：停止可能残留的同名会话（崩溃/上次退出异常）
        stop_session_by_name(&name);

        // 1. 创建实时私有会话
        let mut session = CONTROLTRACE_HANDLE { Value: 0 };
        let mut started = false;
        for _attempt in 0..2 {
            let mut buf = TracePropsBuffer::new(&name);
            let r = unsafe {
                StartTraceW(
                    &mut session,
                    name.as_ptr(),
                    &mut buf.props as *mut EVENT_TRACE_PROPERTIES,
                )
            };
            if r == 0 {
                started = true;
                break;
            }
            if r == 183 {
                // ERROR_ALREADY_EXISTS：同名会话还在，停掉再试一次
                stop_session_by_name(&name);
                continue;
            }
            if r == 5 {
                // ERROR_ACCESS_DENIED
                log::error!("FPS监控: ETW 会话创建失败（需要管理员权限），错误码 {r}");
                return false;
            }
            log::error!("FPS监控: ETW 会话创建失败，错误码 {r}");
            return false;
        }
        if !started {
            log::error!("FPS监控: 无法创建 ETW 会话");
            return false;
        }

        // 2. 启用两个内核 Provider（level=Verbose，与参考 EnableProvider(Guid) 默认一致）
        let mut enabled = false;
        if unsafe {
            EnableTraceEx2(
                session,
                &DXGKRNL_PROVIDER,
                EVENT_CONTROL_CODE_ENABLE_PROVIDER,
                TRACE_LEVEL_VERBOSE as u8,
                0,
                0,
                0,
                std::ptr::null(),
            )
        } != 0
        {
            log::warn!("FPS监控: 启用 DxgKrnl Provider 失败");
        } else {
            enabled = true;
        }
        if unsafe {
            EnableTraceEx2(
                session,
                &WIN32K_PROVIDER,
                EVENT_CONTROL_CODE_ENABLE_PROVIDER,
                TRACE_LEVEL_VERBOSE as u8,
                0,
                0,
                0,
                std::ptr::null(),
            )
        } != 0
        {
            log::warn!("FPS监控: 启用 Win32k Provider 失败");
        } else {
            enabled = true;
        }
        if !enabled {
            log::error!("FPS监控: 两个内核 Provider 均启用失败，放弃本会话");
            stop_session_by_name(&name);
            return false;
        }

        log::info!("FPS监控: ETW 会话已启动（DxgKrnl/Win32k 全局捕获，前台切换不重启）");

        // 3. 打开并阻塞消费（回调中完成统计管线）
        let mut runtime = EtwRuntime::new();
        let mut logfile: EVENT_TRACE_LOGFILEW = unsafe { std::mem::zeroed() };
        logfile.LoggerName = name.as_mut_ptr();
        logfile.Anonymous1.ProcessTraceMode =
            PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD;
        logfile.Anonymous2.EventRecordCallback = Some(on_event_record);
        logfile.Context = (&mut runtime as *mut EtwRuntime) as *mut core::ffi::c_void;

        let process_handle = unsafe { OpenTraceW(&mut logfile) };
        if process_handle.Value == u64::MAX {
            log::error!("FPS监控: OpenTraceW 失败");
            stop_session_by_name(&name);
            return false;
        }
        PROCESS_HANDLE.store(process_handle.Value, Ordering::SeqCst);

        let handles = [process_handle];
        let status = unsafe {
            ProcessTrace(
                handles.as_ptr(),
                1,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        unsafe {
            CloseTrace(process_handle);
        }
        PROCESS_HANDLE.store(0, Ordering::SeqCst);
        stop_session_by_name(&name);

        if status != 0 {
            log::warn!("FPS监控: ProcessTrace 退出 code={status}");
        } else {
            log::info!("FPS监控: ETW 会话已停止");
        }

        // 会话自然结束时若仍处于活跃状态 → 需要重连
        FPS_ACTIVE.load(Ordering::SeqCst)
    }

    /// ETW 会话生命周期主循环（复刻旧方案的「重试上限 + 30s 宽限」逻辑）
    fn fps_monitor_main() {
        const MAX_RESTARTS: u32 = 5;
        let mut restart_count = 0u32;

        while FPS_ACTIVE.load(Ordering::SeqCst) {
            let session_start = Instant::now();
            let keep = run_etw_session();

            if !keep {
                log::error!("FPS监控: ETW 会话无法建立/恢复，停止监控");
                break;
            }
            if !FPS_ACTIVE.load(Ordering::SeqCst) {
                break;
            }

            // 上次会话运行超过 30 秒说明是正常运行后断开，重置重启计数器
            if session_start.elapsed() >= Duration::from_secs(30) {
                restart_count = 0;
            }
            restart_count += 1;
            if restart_count > MAX_RESTARTS {
                log::error!("FPS监控: ETW 会话重连超过 {MAX_RESTARTS} 次，放弃");
                break;
            }
            log::warn!("FPS监控: ETW 会话中断，2秒后重连 (第{restart_count}/{MAX_RESTARTS})");
            thread::sleep(Duration::from_secs(2));
        }

        FPS_ACTIVE.store(false, Ordering::SeqCst);
        log::info!("FPS监控: ETW 采集线程退出");
    }

    /// 启动 ETW 采集线程（由 start_fps_monitor 调用）
    pub fn spawn_monitor() {
        *MONITOR_THREAD.lock().unwrap() = Some(thread::spawn(fps_monitor_main));
    }

    /// 停止 ETW 采集并等待线程退出（由 stop_fps_monitor 调用）
    ///
    /// 停止顺序：FPS_ACTIVE=false（已由调用方设置）→ 按名停会话 → 兜底 CloseTrace →
    /// join 线程，确保会话无残留。
    pub fn stop_and_join() {
        stop_session_by_name(&session_name());
        // 兜底：直接关闭 trace 处理句柄强制 ProcessTrace 返回
        let ph = PROCESS_HANDLE.load(Ordering::SeqCst);
        if ph != 0 {
            unsafe {
                CloseTrace(PROCESSTRACE_HANDLE { Value: ph });
            }
        }
        if let Some(handle) = MONITOR_THREAD.lock().unwrap().take() {
            let _ = handle.join();
        }
    }
}

// ============ 进程追踪线程 ============

/// 进程追踪线程：追踪前台窗口，更新 TARGET_PROCESS，检测 FPS 过期归零
///
/// - 500ms 轮询兜底（Hook 已足够可靠，轮询仅兜底）
/// - 去抖动 500ms（减少 Alt+Tab 抖动）
fn process_tracker_loop() {
    use sysinfo::{Pid, System};

    let mut sys = System::new();
    let mut last_pid: u32 = 0;

    // 前台切换去抖动状态
    let mut pending_name: Option<String> = None;
    let mut pending_since: Option<Instant> = None;
    const DEBOUNCE_DELAY: Duration = Duration::from_millis(500);

    while FPS_ACTIVE.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(500));
        if !FPS_ACTIVE.load(Ordering::SeqCst) {
            break;
        }

        // 1. 兜底轮询前台窗口（防止 Hook 漏发）
        #[cfg(target_os = "windows")]
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::*;
            let hwnd = GetForegroundWindow();
            if !hwnd.is_null() {
                let overlay = OVERLAY_HWND.load(Ordering::Relaxed) as usize;
                if overlay == 0 || hwnd as usize != overlay {
                    let mut pid = 0u32;
                    GetWindowThreadProcessId(hwnd, &mut pid);
                    if pid != 0 {
                        CURRENT_FG_PID.store(pid, Ordering::Relaxed);
                    }
                }
            }
        }

        // 2. 检测 FPS 过期（无匹配数据超过 3 秒 → 归零显示）
        let now = now_unix_ms();
        let last_frame = LAST_FRAME_TIME.load(Ordering::Relaxed);
        if last_frame > 0 && now > last_frame + 3000 {
            if SMOOTHED_FPS.load(Ordering::Relaxed) != 0 {
                log::info!(
                    "FPS监控: 帧率数据已过期 ({}ms 无匹配)，FPS 归零",
                    now - last_frame
                );
                SMOOTHED_FPS.store(0, Ordering::Relaxed);
            }
        }

        // 3. 查询进程名（耗时操作放在后台线程）
        let pid = CURRENT_FG_PID.load(Ordering::Relaxed);
        if pid == 0 {
            continue;
        }

        // PID 变化时才刷新进程列表
        if pid != last_pid {
            sys.refresh_processes();
            last_pid = pid;
        }

        if let Some(process) = sys.process(Pid::from_u32(pid)) {
            let name = process.name().to_string();
            let current_target = TARGET_PROCESS.read().clone();

            if name == current_target {
                // 与当前目标一致，取消待提交的变更
                pending_name = None;
                pending_since = None;
            } else {
                // 进程已变化，启动/更新去抖动计时
                if pending_name.as_deref() != Some(name.as_str()) {
                    pending_name = Some(name.clone());
                    pending_since = Some(Instant::now());
                }

                // 检查去抖动是否到期
                if let (Some(ref pending), Some(since)) = (&pending_name, pending_since) {
                    if since.elapsed() >= DEBOUNCE_DELAY {
                        let is_game = !pending.is_empty() && !is_non_game_process(pending.as_str());

                        {
                            let mut target = TARGET_PROCESS.write();
                            *target = if is_game { pending.clone() } else { String::new() };
                        }

                        if !is_game {
                            SMOOTHED_FPS.store(0, Ordering::Relaxed);
                        }

                        // 递增代次，通知 ETW 回调清空缓冲区
                        TARGET_GENERATION.fetch_add(1, Ordering::Relaxed);

                        log::info!(
                            "FPS监控: 前台进程 → {} (pid={}){}",
                            pending,
                            pid,
                            if is_game { "" } else { " (非游戏)" }
                        );

                        pending_name = None;
                        pending_since = None;
                    }
                }
            }
        }
    }
}

// ============ 公共 API（与旧方案完全兼容） ============

/// 启动 FPS 监控
pub fn start_fps_monitor() {
    if FPS_ACTIVE.load(Ordering::SeqCst) {
        return;
    }

    // ETW 内核 Provider 需要管理员权限（与旧 PresentMon 方案一致）
    if !crate::optimization::is_admin() {
        log::error!("FPS监控: 需要管理员权限才能开启帧率监控（ETW 内核事件采集）");
        return;
    }

    FPS_ACTIVE.store(true, Ordering::SeqCst);

    #[cfg(target_os = "windows")]
    {
        win32_fg::init_foreground_process();
        unsafe {
            win32_fg::register_hook();
        }
    }

    // 启动 ETW 采集线程
    #[cfg(target_os = "windows")]
    etw::spawn_monitor();

    // 启动前台窗口追踪线程
    thread::spawn(|| {
        process_tracker_loop();
    });

    log::info!("FPS监控: 已启动 (内嵌 ETW — DxgKrnl/Win32k Present 事件)");
}

/// 停止 FPS 监控
pub fn stop_fps_monitor() {
    if !FPS_ACTIVE.load(Ordering::SeqCst) {
        return;
    }
    FPS_ACTIVE.store(false, Ordering::SeqCst);

    // 停止 ETW 会话并等待采集线程退出（确保无残留会话）
    #[cfg(target_os = "windows")]
    etw::stop_and_join();

    #[cfg(target_os = "windows")]
    unsafe {
        win32_fg::unregister_hook();
    }

    SMOOTHED_FPS.store(0, Ordering::Relaxed);
    ONE_PCT_LOW_FPS.store(0, Ordering::Relaxed);
    ZERO_DOT_ONE_PCT_LOW_FPS.store(0, Ordering::Relaxed);
    LAST_FRAME_TIME.store(0, Ordering::Relaxed);
    TARGET_GENERATION.store(0, Ordering::Relaxed);
    *TARGET_PROCESS.write() = String::new();
    CURRENT_FG_PID.store(0, Ordering::Relaxed);

    log::info!("FPS监控: 已停止");
}

/// 获取缓存的平滑 FPS 值
pub fn get_cached_fps() -> Option<u32> {
    let fps = SMOOTHED_FPS.load(Ordering::Relaxed);
    if fps == 0 {
        None
    } else {
        Some(fps)
    }
}

/// 获取缓存的 1% Low FPS 值（最慢 1% 帧的平均 FPS）
///
/// 参考 CapFrameX EMetric.OnePercentLowAverage：
/// 取 99 分位帧时间作为阈值，筛选 >= 阈值的帧（最慢 1%），计算平均帧时间，
/// FPS = 1000 / 平均帧时间
pub fn get_cached_1low_fps() -> Option<u32> {
    let fps = ONE_PCT_LOW_FPS.load(Ordering::Relaxed);
    if fps == 0 {
        None
    } else {
        Some(fps)
    }
}

/// 获取缓存的 0.1% Low FPS 值（最慢 0.1% 帧的平均 FPS）
///
/// 参考 CapFrameX EMetric.ZerodotOnePercentLowAverage：
/// 取 99.9 分位帧时间作为阈值，筛选 >= 阈值的帧（最慢 0.1%），计算平均帧时间，
/// FPS = 1000 / 平均帧时间
pub fn get_cached_01low_fps() -> Option<u32> {
    let fps = ZERO_DOT_ONE_PCT_LOW_FPS.load(Ordering::Relaxed);
    if fps == 0 {
        None
    } else {
        Some(fps)
    }
}

/// 设置自身 overlay 窗口句柄（用于排除前台切换到自身 overlay）
pub fn set_overlay_hwnd(hwnd: u64) {
    OVERLAY_HWND.store(hwnd, Ordering::SeqCst);
    log::info!("FPS监控: Overlay窗口句柄设置为 {hwnd:#X}");
}

/// 清除自身 overlay 窗口句柄
pub fn clear_overlay_hwnd() {
    OVERLAY_HWND.store(0, Ordering::SeqCst);
    log::info!("FPS监控: Overlay窗口句柄已清除");
}

/// 清理资源
pub fn cleanup() {
    stop_fps_monitor();
}