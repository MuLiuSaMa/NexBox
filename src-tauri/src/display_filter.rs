use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::path::{Path, PathBuf};
use std::fs;
use std::io::Read;
use std::process::Command;
use tauri::Emitter;

// ─── Display enumeration (复用现有 CCD/GDI 枚举逻辑) ───

#[derive(serde::Serialize, Clone)]
pub struct DisplayInfo {
    pub index: usize,
    pub name: String,
    pub device_name: String,
    pub is_primary: bool,
    pub width: i32,
    pub height: i32,
}


static DISPLAY_DEVICES: Mutex<Option<Vec<String>>> = Mutex::new(None);

/// 系统关机/注销标志：当 Windows 广播 WM_QUERYENDSESSION / WM_ENDSESSION 时置位，
/// 用于在退出清理阶段跳过 xcalib 这类外部子进程调用（关机时系统运行库正在被拆除，子进程会初始化失败 0xc0000142）。
static SYSTEM_SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);
/// 应用自身正在退出（RunEvent::Exit 已触发某次清理）。用于退出准入：拒绝新的滤镜意图，
/// 并确保 game_filter 等后台任务不会在退出期间再写屏。
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);
/// Unique suffix for generated ICC files so concurrent displays never share a path.
static TEMP_ICC_SEQUENCE: AtomicU64 = AtomicU64::new(1);
/// 会话监控隐藏窗口句柄（保存为裸指针），防止窗口句柄被回收。
static SESSION_WATCH_HWND: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

#[cfg(target_os = "windows")]
fn enumerate_displays_via_ccd() -> Option<Vec<DisplayInfo>> {
    use windows_sys::Win32::Devices::Display::*;
    use std::mem;

    unsafe {
        let mut path_count: u32 = 0;
        let mut mode_count: u32 = 0;
        let status = QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut path_count,
            std::ptr::null_mut(),
            &mut mode_count,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        if status != 0 || path_count == 0 {
            return None;
        }

        let mut paths: Vec<DISPLAYCONFIG_PATH_INFO> = (0..path_count)
            .map(|_| mem::zeroed())
            .collect();
        let mut modes: Vec<DISPLAYCONFIG_MODE_INFO> = (0..mode_count)
            .map(|_| mem::zeroed())
            .collect();

        let status = QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            std::ptr::null_mut(),
        );
        if status != 0 {
            return None;
        }

        let mut displays = Vec::new();

        for (path_idx, path) in paths.iter().enumerate() {
            let source_info = &path.sourceInfo;
            let target_info = &path.targetInfo;

            let target_flags: u32 = std::ptr::read_unaligned(&target_info.Anonymous as *const _ as *const u32);
            let target_available = target_flags & 0x01;
            if target_available == 0 {
                continue;
            }

            let (width, height, pos_x, pos_y) = {
                let mode_idx = source_info.Anonymous.modeInfoIdx as usize;
                if mode_idx < modes.len() {
                    let mode = &modes[mode_idx];
                    if mode.infoType == DISPLAYCONFIG_MODE_INFO_TYPE_SOURCE {
                        let src = &mode.Anonymous.sourceMode;
                        (src.width as i32, src.height as i32, src.position.x, src.position.y)
                    } else {
                        continue;
                    }
                } else {
                    continue;
                }
            };

            if width <= 0 || height <= 0 {
                continue;
            }

            let is_primary = pos_x == 0 && pos_y == 0;

            let device_name = {
                let mut source_name: DISPLAYCONFIG_SOURCE_DEVICE_NAME = mem::zeroed();
                source_name.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
                source_name.header.size = mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32;
                source_name.header.adapterId = source_info.adapterId;
                source_name.header.id = source_info.id;

                if DisplayConfigGetDeviceInfo(&mut source_name.header as *mut _ as *mut _) == 0 {
                    let len = source_name.viewGdiDeviceName
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(source_name.viewGdiDeviceName.len());
                    if len > 0 {
                        let name = String::from_utf16_lossy(&source_name.viewGdiDeviceName[..len]);
                        if !name.is_empty() { name } else { format!("\\\\.\\DISPLAY{}", path_idx + 1) }
                    } else {
                        format!("\\\\.\\DISPLAY{}", path_idx + 1)
                    }
                } else {
                    format!("\\\\.\\DISPLAY{}", path_idx + 1)
                }
            };

            let monitor_model = {
                let mut target_name: DISPLAYCONFIG_TARGET_DEVICE_NAME = mem::zeroed();
                target_name.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
                target_name.header.size = mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32;
                target_name.header.adapterId = target_info.adapterId;
                target_name.header.id = target_info.id;

                if DisplayConfigGetDeviceInfo(&mut target_name.header as *mut _ as *mut _) == 0 {
                    let len = target_name.monitorFriendlyDeviceName
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(target_name.monitorFriendlyDeviceName.len());
                    if len > 0 {
                        let name = String::from_utf16_lossy(&target_name.monitorFriendlyDeviceName[..len]);
                        let trimmed = name.trim();
                        if !trimmed.is_empty() { trimmed.to_string() } else { String::new() }
                    } else {
                        String::new()
                    }
                } else {
                    get_monitor_model_name(&device_name)
                }
            };

            let name = if !monitor_model.is_empty() {
                format!("{} ({}x{})", monitor_model, width, height)
            } else {
                format!("{} ({}x{})", device_name.trim_start_matches("\\\\.\\"), width, height)
            };

            displays.push(DisplayInfo {
                index: displays.len(),
                name,
                device_name,
                is_primary,
                width,
                height,
            });
        }

        if displays.is_empty() { return None; }
        Some(displays)
    }
}

#[cfg(target_os = "windows")]
fn get_gdi_device_resolution(device_name: &str) -> (i32, i32) {
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS};
    unsafe {
        let tries = [device_name, device_name.trim_start_matches("\\\\.\\")];
        for name in tries {
            if name.is_empty() { continue; }
            let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            let mut dm: DEVMODEW = std::mem::zeroed();
            dm.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
            if EnumDisplaySettingsW(wide.as_ptr(), ENUM_CURRENT_SETTINGS, &mut dm) != 0 {
                let w = dm.dmPelsWidth as i32;
                let h = dm.dmPelsHeight as i32;
                if w > 0 && h > 0 { return (w, h); }
            }
        }
    }
    (0, 0)
}

#[cfg(target_os = "windows")]
fn enumerate_displays_via_gdi() -> Vec<DisplayInfo> {
    use windows_sys::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW,
        HDC, HMONITOR, MONITORINFOEXW,
    };

    struct MonitorData { displays: Vec<DisplayInfo> }

    unsafe extern "system" fn monitor_enum_proc(
        hmonitor: HMONITOR, _hdc: HDC,
        _rect: *mut windows_sys::Win32::Foundation::RECT,
        lparam: isize,
    ) -> i32 {
        let data = &mut *(lparam as *mut MonitorData);
        let mut info: MONITORINFOEXW = std::mem::zeroed();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

        if GetMonitorInfoW(hmonitor, &mut info as *mut _ as *mut _) != 0 {
            let device_name = String::from_utf16_lossy(
                &info.szDevice[..info.szDevice.iter().position(|&c| c == 0).unwrap_or(info.szDevice.len())],
            );
            let is_primary = (info.monitorInfo.dwFlags & 1) != 0;
            let mut width = info.monitorInfo.rcMonitor.right - info.monitorInfo.rcMonitor.left;
            let mut height = info.monitorInfo.rcMonitor.bottom - info.monitorInfo.rcMonitor.top;

            if width <= 0 || height <= 0 {
                let (fw, fh) = get_gdi_device_resolution(&device_name);
                if fw > 0 && fh > 0 { width = fw; height = fh; }
            }

            let index = data.displays.len();
            let monitor_model = get_monitor_model_name(&device_name);
            let name = if !monitor_model.is_empty() {
                format!("{} ({}x{})", monitor_model, width, height)
            } else {
                format!("{} ({}x{})", device_name, width, height)
            };

            data.displays.push(DisplayInfo { index, name, device_name: device_name.clone(), is_primary, width, height });
        }
        1
    }

    let mut data = MonitorData { displays: Vec::new() };
    unsafe {
        EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(monitor_enum_proc), &mut data as *mut _ as isize);
    }
    data.displays
}

fn is_generic_monitor_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("generic") || lower.contains("即插即用") || lower.contains("通用")
        || lower.contains("pnp") || lower.contains("standard monitor") || lower.contains("digital display")
        || lower.contains("analog display")
}

#[cfg(target_os = "windows")]
fn get_monitor_model_name(device_name: &str) -> String {
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplayDevicesW, DISPLAY_DEVICEW};
    use std::mem;
    unsafe {
        let device_name_wide: Vec<u16> = device_name.encode_utf16().chain(std::iter::once(0)).collect();
        let mut disp_device: DISPLAY_DEVICEW = mem::zeroed();
        disp_device.cb = mem::size_of::<DISPLAY_DEVICEW>() as u32;
        if EnumDisplayDevicesW(device_name_wide.as_ptr(), 0, &mut disp_device, 0) != 0 {
            let len = disp_device.DeviceString.iter().position(|&c| c == 0).unwrap_or(disp_device.DeviceString.len());
            if len > 0 {
                let model = String::from_utf16_lossy(&disp_device.DeviceString[..len]);
                let trimmed = model.trim();
                if !trimmed.is_empty() && !is_generic_monitor_name(trimmed) {
                    return trimmed.to_string();
                }
            }
        }
    }
    String::new()
}

#[cfg(target_os = "windows")]
fn enumerate_displays_inner() -> Vec<DisplayInfo> {
    let mut displays = enumerate_displays_via_ccd().unwrap_or_default();
    if displays.is_empty() {
        displays = enumerate_displays_via_gdi();
    }
    if displays.is_empty() {
        for i in 0..8 {
            let name = format!("\\\\.\\DISPLAY{}", i + 1);
            let (w, h) = get_gdi_device_resolution(&name);
            if w > 0 && h > 0 {
                displays.push(DisplayInfo {
                    index: displays.len(),
                    name: format!("DISPLAY{} ({}x{})", i + 1, w, h),
                    device_name: name,
                    is_primary: i == 0, width: w, height: h,
                });
            }
        }
    }
    if let Ok(mut lock) = DISPLAY_DEVICES.lock() {
        *lock = Some(displays.iter().map(|d| d.device_name.clone()).collect());
    }
    if displays.is_empty() {
        displays.push(DisplayInfo { index: 0, name: "DISPLAY1 (Primary)".to_string(), device_name: "DISPLAY1".to_string(), is_primary: true, width: 0, height: 0 });
        if let Ok(mut lock) = DISPLAY_DEVICES.lock() { *lock = Some(vec!["DISPLAY1".to_string()]); }
    }
    displays
}

// ─── Per-display state ───

#[derive(Clone)]
pub(crate) struct DisplayState {
    temperature: i32,
    brightness: i32,
    contrast: i32,
    saturation: i32,
    r_gamma: f64,
    g_gamma: f64,
    b_gamma: f64,
    mode: i32,
    icc_ramp: Option<[[u16; 256]; 3]>,
    icc_active: bool,
    active_icc_id: Option<String>,
    filter_active: bool,
    /// 本进程可能已修改该显示器的显示效果，尚未确认恢复成功。
    /// 在第一次可能修改屏幕的底层写入前置为 true；即使写入返回错误也保留 true；
    /// 仅在确认恢复成功后清除。用于退出/重复关闭时决定是否仍需恢复，避免仅凭
    /// `filter_active == false` 提前跳过恢复。
    restore_pending: bool,
    /// 是否处于多滤镜叠加模式（已应用叠加组合）
    stacked: bool,
    /// 已应用的叠加组合（应用顺序，即卡片点选顺序）
    stack_preset_ids: Vec<String>,
    /// Monotonic last-intent token. Older physical display writes are skipped.
    operation_generation: u64,
}

impl Default for DisplayState {
    fn default() -> Self {
        Self {
            temperature: 6500, brightness: 100, contrast: 100, saturation: 100,
            r_gamma: 1.0, g_gamma: 1.0, b_gamma: 1.0, mode: 0,
            icc_ramp: None, icc_active: false, active_icc_id: None, filter_active: false,
            restore_pending: false,
            stacked: false, stack_preset_ids: Vec::new(), operation_generation: 0,
        }
    }
}

static DISPLAY_STATES: Mutex<Option<Vec<Mutex<DisplayState>>>> = Mutex::new(None);
/// Serialize physical ICC/gamma writes per display, while different displays remain independent.
static DISPLAY_OPERATION_LOCKS: Mutex<Vec<Arc<Mutex<()>>>> = Mutex::new(Vec::new());
static ACTIVE_DISPLAY_INDEX: AtomicUsize = AtomicUsize::new(0);

/// 一条 gamma ramp（3 通道 × 256 项，每项 0..=65535）。
pub(crate) type GammaRamp = [[u16; 256]; 3];

/// 物理写屏后端。只负责“读当前 ramp / 写 ramp / 用 ICC 落屏 / 清成线性”，
/// **不参与版本号、退出准入、原始 Ramp 管理或 `restore_pending`** —— 那些属于协调层。
/// 生产实现复用现有 GDI + xcalib；测试实现可记录调用顺序并注入失败与暂停。
///
/// `read` 返回 `Result`（而非 `Option`），以保留捕获失败的具体原因。
pub(crate) trait GammaBackend: Send + Sync {
    fn read(&self, display: usize) -> Result<GammaRamp, String>;
    fn write(&self, display: usize, ramp: &GammaRamp) -> Result<(), String>;
    fn apply_icc(&self, display: usize, path: &Path) -> Result<(), String>;
    fn clear(&self, display: usize) -> Result<(), String>;
}

/// 真实后端：直接转发到现有 GDI / xcalib 实现（这些函数本身不再做协调，避免递归）。
pub(crate) struct SystemGammaBackend;

impl GammaBackend for SystemGammaBackend {
    fn read(&self, display: usize) -> Result<GammaRamp, String> {
        read_gamma_ramp(display).ok_or_else(|| format!("GetDeviceGammaRamp[{}] 读取失败", display))
    }
    fn write(&self, display: usize, ramp: &GammaRamp) -> Result<(), String> {
        write_gamma_ramp(display, ramp)
    }
    fn apply_icc(&self, display: usize, path: &Path) -> Result<(), String> {
        apply_icc_via_xcalib(path, display)
    }
    fn clear(&self, display: usize) -> Result<(), String> {
        clear_gamma_ramp_via_xcalib(display)
    }
}

static SYSTEM_BACKEND: SystemGammaBackend = SystemGammaBackend;

/// 首次应用滤镜前捕获的原始硬件 gamma ramp（按显示器 index 对齐，与 DISPLAY_STATES 键位一致）。
/// 退出/禁用时据此精确恢复，而不是用 `xcalib -c` 清成线性——那会把图形控制台 /
/// 系统颜色管理里设置的 sRGB 校色一并抹掉。
static ORIGINAL_RAMPS: Mutex<Vec<Mutex<Option<GammaRamp>>>> = Mutex::new(Vec::new());

/// 一次恢复的实际结果。用于把“精确恢复”与“降级线性清除”区分开，
/// 避免把降级清除当成普通“已恢复”报给用户。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RestoreOutcome {
    /// 已用捕获的原始 ramp 精确恢复。
    Restored,
    /// 无原始 ramp，用线性清除兜底：滤镜已移除，但**原有校色未保证恢复**。
    DegradedCleared,
    /// 该显示器未受影响、无待恢复记录：未执行任何写屏。
    NothingToDo,
}

/// 一次版本化执行器调用的结果：区分“实际执行”与“因版本过期而跳过”。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunResult<T> {
    /// 操作已实际执行，携带操作返回值。
    Executed(T),
    /// 操作因已有更新意图（版本不匹配）而跳过，未发生任何物理写屏。
    SkippedStale,
}

/// cleanup 的汇总计数（供调用方/测试直接断言，例如“清除失败计入 failed”）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct CleanupSummary {
    pub restored: usize,
    pub degraded: usize,
    pub idle: usize,
    pub failed: usize,
}

/// 失败回滚的明确结果：调用方按此收尾归属/记录，不得用单个布尔混义
/// “提交关闭成功”与“恢复成功”。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RollbackOutcome {
    /// 已精确恢复应用前的原始 ramp，回滚完成。
    Restored,
    /// 无原始 ramp，用线性清除兜底：滤镜已移除，原有校色未保证恢复。
    DegradedCleared,
    /// 该显示器无待恢复记录（未受影响），无需写屏。
    NoRestoreNeeded,
    /// 关闭未提交（版本已被取代/退出中/归属已失效）：未写屏，调用方应条件弃权。
    Superseded,
    /// 恢复失败：Restoring 归属 + restore_pending 保留，调用方不得清除记录。
    RestoreFailed(String),
}

/// 协调层上下文。生产路径用全局静态构造（[`global_ops`]）；测试用独立实例 +
/// 假后端，从而**不替换全局状态**也能并行运行真实协调逻辑。
#[derive(Clone, Copy)]
pub(crate) struct DisplayOps<'a> {
    states: &'a Mutex<Option<Vec<Mutex<DisplayState>>>>,
    op_locks: &'a Mutex<Vec<Arc<Mutex<()>>>>,
    ramps: &'a Mutex<Vec<Mutex<Option<GammaRamp>>>>,
    shutting_down: &'a AtomicBool,
    count: usize,
    backend: &'a dyn GammaBackend,
}

/// 一次“捕获原始 ramp 并复查版本”的结果：区分捕获被跳过（版本过期/退出）与
/// 已成功捕获。捕获期间可能被新的关闭意图推进版本，因此捕获完成后必须复查。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CaptureOutcome {
    /// 已确认捕获成功，且操作版本仍为最新（可继续写屏）。
    Captured,
    /// 捕获未执行或捕获期间版本已过期：调用方必须中止写屏（视为跳过）。
    Skip,
}

/// 构造生产用协调层上下文（短暂获取状态锁完成初始化，不跨驱动调用持有）。
pub(crate) fn global_ops() -> DisplayOps<'static> {
    ensure_display_states();
    DisplayOps {
        states: &DISPLAY_STATES,
        op_locks: &DISPLAY_OPERATION_LOCKS,
        ramps: &ORIGINAL_RAMPS,
        shutting_down: &SHUTTING_DOWN,
        count: display_count(),
        backend: &SYSTEM_BACKEND,
    }
}

/// Build a `DisplayState` for a given display index, loading any persisted
/// parameters/ICC from disk. `filter_active` is forced to `false` so we never
/// auto-apply a filter just because the per-display state vector is (re)built.
fn display_state_from_persisted(saved: &HashMap<usize, PersistentFilterState>, idx: usize) -> DisplayState {
    let mut st = DisplayState::default();
    if let Some(p) = saved.get(&idx) {
        st.filter_active = false; // never auto-apply on (re)init
        st.temperature = p.temperature;
        st.brightness = p.brightness;
        st.contrast = p.contrast;
        st.saturation = p.saturation;
        st.r_gamma = p.r_gamma;
        st.g_gamma = p.g_gamma;
        st.b_gamma = p.b_gamma;
        st.mode = p.mode;
        st.icc_active = p.icc_active;
        st.active_icc_id = p.active_icc_id.clone();
        st.stacked = p.stacked;
        st.stack_preset_ids = p.stack_preset_ids.clone();
        st.icc_ramp = p.icc_ramp.as_ref().map(|r| {
            let mut arr = [[0u16; 256]; 3];
            for ch in 0..3.min(r.len()) {
                for (i, &v) in r[ch].iter().enumerate().take(256) { arr[ch][i] = v; }
            }
            arr
        });
    }
    st
}

/// Ensure the per-display state vector exists and matches the actual number of
/// connected displays. Each display keeps its own independent filter state, so
/// switching monitors must not collapse onto a shared state.
///
/// This is critical: the frontend may call `get_filter_settings` (which triggers
/// this) before `get_displays` has populated `DISPLAY_DEVICES`. We therefore
/// lazily enumerate displays here and (re)size the state vector to the real
/// display count, preserving in-memory state for indexes that persist and only
/// defaulting for newly-added indexes.
pub(crate) fn ensure_display_states() {
    // Lazily enumerate displays so we know how many per-display states to keep.
    {
        let dev_lock = DISPLAY_DEVICES.lock().unwrap();
        if dev_lock.is_none() {
            drop(dev_lock);
            enumerate_displays_inner();
        }
    }

    let count = {
        let dev_lock = DISPLAY_DEVICES.lock().unwrap();
        dev_lock.as_ref().map(|d| d.len()).unwrap_or(1).max(1)
    };

    ensure_display_operation_locks(count);

    let mut lock = DISPLAY_STATES.lock().unwrap();
    match lock.as_mut() {
        // Already sized correctly — nothing to do.
        Some(states) if states.len() == count => {}
        // Resize: keep existing in-memory state, default only new indexes.
        Some(states) => {
            if states.len() > count {
                states.truncate(count);
            } else {
                let saved = load_all_filter_states();
                while states.len() < count {
                    let idx = states.len();
                    states.push(Mutex::new(display_state_from_persisted(&saved, idx)));
                }
            }
        }
        // First init.
        None => {
            let saved = load_all_filter_states();
            let states = (0..count)
                .map(|idx| Mutex::new(display_state_from_persisted(&saved, idx)))
                .collect::<Vec<_>>();
            *lock = Some(states);
        }
    }
}

/// 操作锁表维护：只增长、不截断。执行器会克隆 Arc 锁句柄，截断会在显示器数量
/// 减少再增加时于同一槽位创建新锁，破坏同槽位串行保证（新旧任务各持一把锁）。
/// 状态层仍有严格边界检查（with_state/submit/op_lock 对越界返回 None），
/// 锁存在不代表设备存在。生产与测试共用此函数，保证测试验证的正是生产逻辑。
fn grow_operation_locks(locks: &mut Vec<Arc<Mutex<()>>>, count: usize) {
    while locks.len() < count {
        locks.push(Arc::new(Mutex::new(())));
    }
}

fn ensure_display_operation_locks(count: usize) {
    let mut locks = DISPLAY_OPERATION_LOCKS.lock().unwrap();
    grow_operation_locks(&mut locks, count);
}

fn bump_operation_generation(state: &mut DisplayState) -> u64 {
    state.operation_generation = state.operation_generation.wrapping_add(1);
    if state.operation_generation == 0 {
        state.operation_generation = 1;
    }
    state.operation_generation
}

/// 协调层核心实现。所有方法都假定调用方按约定取锁：
/// - 状态锁（`states`）只在短暂修改/读取字段时持有，**绝不跨驱动调用或外部进程**；
/// - 显示器操作锁（`op_locks[i]`）串行化同一显示器的物理写屏；
/// - 锁顺序统一为「先操作锁 → 再短暂状态锁」，不存在反向嵌套。
impl<'a> DisplayOps<'a> {
    /// 短暂获取状态锁读取/修改单个显示器状态（不跨驱动调用）。
    /// **严格边界检查**：空集合或越界索引返回 `None`，不做夹取——
    /// 滤镜操作不能因索引失效而作用于另一台显示器。
    pub(crate) fn with_state<F, R>(&self, idx: usize, f: F) -> Option<R>
    where
        F: FnOnce(&mut DisplayState) -> R,
    {
        let lock = self.states.lock().unwrap();
        let states = lock.as_ref()?;
        let state = states.get(idx)?;
        let mut state = state.lock().unwrap();
        Some(f(&mut *state))
    }

    /// **退出准入的原子边界**：在同一状态锁内先检查退出标志，通过后才允许
    /// 修改期望状态、递增版本并取得任务数据。返回 `None` 表示正在退出、意图被拒绝。
    ///
    /// cleanup 也在同一状态锁内置位退出标志，因此“检查退出 + 接受意图”整体原子，
    /// 不会出现“入口检查通过后、cleanup 已递增版本，本任务又递增更高版本”的竞态。
    pub(crate) fn submit<F, R>(&self, idx: usize, f: F) -> Option<R>
    where
        F: FnOnce(&mut DisplayState) -> R,
    {
        let lock = self.states.lock().unwrap();
        let states = lock.as_ref()?;
        let state = states.get(idx)?;
        if self.shutting_down.load(Ordering::SeqCst) {
            return None;
        }
        let mut state = state.lock().unwrap();
        Some(f(&mut *state))
    }

    /// 退出第一步：在**同一状态锁保护下**置位退出标志并使所有已有任务失效。
    /// 返回每台显示器的新代号，供后续在操作锁内校验。
    pub(crate) fn begin_shutdown(&self) -> Vec<u64> {
        let lock = self.states.lock().unwrap();
        let states = lock.as_ref().expect("display states not initialized");
        // 先置标志：之后任何 submit 都会在本锁内看到它并拒绝新意图。
        self.shutting_down.store(true, Ordering::SeqCst);
        states
            .iter()
            .map(|state_mutex| {
                let mut state = state_mutex.lock().unwrap();
                state.filter_active = false;
                state.icc_active = false;
                state.active_icc_id = None;
                bump_operation_generation(&mut *state)
            })
            .collect()
    }

    pub(crate) fn is_shutting_down(&self) -> bool {
        self.shutting_down.load(Ordering::SeqCst)
    }

    /// 取指定显示器的操作锁句柄（只增长不截断，避免截断时新旧锁并存）。
    /// 越界索引返回 `None`——调用方必须先确认目标显示器存在，不能静默夹取到最后一台。
    pub(crate) fn op_lock(&self, idx: usize) -> Option<Arc<Mutex<()>>> {
        let mut locks = self.op_locks.lock().unwrap();
        while locks.len() < self.count.max(1) {
            locks.push(Arc::new(Mutex::new(())));
        }
        locks.get(idx).cloned()
    }

    /// 版本化执行器：取得显示器操作锁后，**在锁内**复查版本与退出标志。
    /// `allow_during_shutdown == false` 的普通应用/关闭任务在退出期间会被拒绝；
    /// 退出恢复走 `allow_during_shutdown == true` 的内部路径，不被该检查拦截。
    pub(crate) fn run_if_current<F, R>(
        &self,
        idx: usize,
        expected_generation: u64,
        operation_name: &str,
        allow_during_shutdown: bool,
        operation: F,
    ) -> Result<RunResult<R>, String>
    where
        F: FnOnce() -> Result<R, String>,
    {
        // 越界索引：明确拒绝（不夹取到其他显示器）。
        let operation_lock = self.op_lock(idx)
            .ok_or_else(|| format!("{}[{}]: 显示器索引越界，已拒绝操作", operation_name, idx))?;
        let _guard = operation_lock.lock().unwrap();
        // 锁内复查：退出标志 + 当前版本。
        let shutting_down = self.is_shutting_down();
        let current_generation = self
            .with_state(idx, |state| state.operation_generation)
            .ok_or_else(|| format!("{}[{}]: 显示器状态不存在，已拒绝操作", operation_name, idx))?;
        if current_generation != expected_generation {
            log::info!(
                "{}[{}]: stale display operation skipped (expected={}, current={})",
                operation_name,
                idx,
                expected_generation,
                current_generation
            );
            return Ok(RunResult::SkippedStale);
        }
        if shutting_down && !allow_during_shutdown {
            return Err(format!(
                "{}[{}]: 应用正在退出，普通写屏任务被拒绝",
                operation_name, idx
            ));
        }
        Ok(RunResult::Executed(operation()?))
    }

    /// 使 ramps 槽位与显示器数量对齐（只增长）。
    fn ensure_ramps(&self) {
        let mut lock = self.ramps.lock().unwrap();
        while lock.len() < self.count.max(1) {
            lock.push(Mutex::new(None));
        }
    }

    /// 首次写屏前捕获原始 ramp。已有捕获则复用；捕获失败返回 Err（调用方必须中止应用）。
    /// 要求调用方已持有该显示器的操作锁。**不**做版本复查（慢操作；由
    /// [`DisplayOps::capture_if_current`] 在捕获完成后复查版本）。
    pub(crate) fn capture(&self, idx: usize) -> Result<(), String> {
        self.ensure_ramps();
        let slot = self.ramps.lock().unwrap();
        let Some(cell) = slot.get(idx) else {
            return Err(format!("capture_original_ramp[{}]: 无此显示器", idx));
        };
        let mut guard = cell.lock().unwrap();
        if guard.is_some() {
            return Ok(()); // 已捕获，复用；不重新捕获以免被已加过滤镜的 ramp 覆盖
        }
        match self.backend.read(idx) {
            Ok(ramp) => {
                log::info!("capture_original_ramp[{}]: 已捕获原始 gamma ramp（含用户 sRGB 校色）", idx);
                *guard = Some(ramp);
                Ok(())
            }
            Err(e) => {
                // 捕获失败：不中止已有“可能被修改”的记录，也不继续写屏。
                log::error!("capture_original_ramp[{}]: 读取原始 ramp 失败，已中止应用: {}", idx, e);
                Err(format!("无法读取显示器 {} 的原始 gamma ramp，已中止应用: {}", idx, e))
            }
        }
    }

    /// 捕获 + 捕获后复查版本：捕获原始 ramp（如果尚未捕获），**完成后在操作锁内
    /// 复查版本与退出标志**——捕获是慢操作（GDI 读取/外部进程），期间可能被新的
    /// 关闭意图推进版本。版本已过期或正在退出时返回 `Skip`，调用方必须中止写屏。
    /// 要求调用方已持有该显示器的操作锁。
    pub(crate) fn capture_if_current(
        &self,
        idx: usize,
        expected_generation: u64,
        operation_name: &str,
    ) -> Result<CaptureOutcome, String> {
        self.capture(idx)?;
        let current_generation = self
            .with_state(idx, |s| s.operation_generation)
            .ok_or_else(|| format!("{}[{}]: 显示器状态不存在，已拒绝操作", operation_name, idx))?;
        if current_generation != expected_generation {
            log::info!(
                "{}[{}]: 捕获完成后版本已变化 (expected={}, current={})，中止写屏",
                operation_name,
                idx,
                expected_generation,
                current_generation
            );
            return Ok(CaptureOutcome::Skip);
        }
        if self.is_shutting_down() {
            log::info!("{}[{}]: 应用正在退出，捕获后中止写屏", operation_name, idx);
            return Ok(CaptureOutcome::Skip);
        }
        Ok(CaptureOutcome::Captured)
    }

    /// 读取保存的原始 ramp（副本，不移除）。
    pub(crate) fn peek_ramp(&self, idx: usize) -> Option<GammaRamp> {
        self.ensure_ramps();
        let slot = self.ramps.lock().unwrap();
        let cell = slot.get(idx)?;
        let guard = cell.lock().unwrap();
        guard.clone()
    }

    /// 确认恢复成功后清除保存的原始 ramp。
    pub(crate) fn clear_ramp(&self, idx: usize) {
        self.ensure_ramps();
        let slot = self.ramps.lock().unwrap();
        if let Some(cell) = slot.get(idx) {
            *cell.lock().unwrap() = None;
        }
    }

    pub(crate) fn set_restore_pending(&self, idx: usize, pending: bool) {
        self.with_state(idx, |s| s.restore_pending = pending);
    }

    /// 协调层落屏入口：在调用**可能修改屏幕**的后端操作前置位 restore_pending。
    /// 即使写屏失败也保留 true（可能已部分修改），仅在确认恢复成功后清除。
    pub(crate) fn apply_icc(&self, idx: usize, path: &Path) -> Result<(), String> {
        self.set_restore_pending(idx, true);
        self.backend.apply_icc(idx, path)
    }

    /// 应用本次生成的临时 ICC，结束后清理该临时文件（外部进程已退出，安全）。
    pub(crate) fn apply_generated_icc(&self, idx: usize, path: &Path) -> Result<(), String> {
        let result = self.apply_icc(idx, path);
        if let Err(e) = std::fs::remove_file(path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                log::warn!("临时 ICC 文件清理失败 '{}': {}", path.display(), e);
            }
        }
        result
    }

    /// 恢复显示器。要求调用方已持有该显示器的操作锁（因此锁内判断 pending / ramp）。
    /// 越界索引返回 Err（不夹取到其他显示器）。
    pub(crate) fn restore(&self, idx: usize) -> Result<RestoreOutcome, String> {
        let pending = self
            .with_state(idx, |s| s.restore_pending)
            .ok_or_else(|| format!("restore[{}]: 显示器状态不存在，已拒绝操作", idx))?;
        if let Some(ramp) = self.peek_ramp(idx) {
            // 写回可能修改屏幕：在底层调用前置位待恢复标记，失败也保留（可能已部分修改）。
            self.set_restore_pending(idx, true);
            return match self.backend.write(idx, &ramp) {
                Ok(()) => {
                    // 确认恢复成功后才删除原始 ramp、清除待恢复标记。
                    self.clear_ramp(idx);
                    self.set_restore_pending(idx, false);
                    log::info!("restore[{}]: 已精确恢复应用前的原始 gamma ramp", idx);
                    Ok(RestoreOutcome::Restored)
                }
                Err(e) => {
                    // 写回失败：保留原始 ramp 与 restore_pending 供重试。不自动用 xcalib -c
                    // 清成线性——那不等于恢复用户原来的校色，不能据此报“恢复成功”。
                    log::error!("restore[{}]: 精确恢复失败，保留原始 ramp 供重试: {}", idx, e);
                    Err(e)
                }
            };
        }
        if pending {
            // 待恢复但无原始 ramp：只能降级线性清除。同样在调用前保留标记，失败可重试。
            self.set_restore_pending(idx, true);
            log::warn!("restore[{}]: 无原始 ramp，降级用线性清除兜底", idx);
            return match self.backend.clear(idx) {
                Ok(()) => {
                    self.set_restore_pending(idx, false);
                    Ok(RestoreOutcome::DegradedCleared)
                }
                // 清除失败（含系统关机/注销时跳过外部进程）：标记保留，
                // 调用方/cleanup 必须将其计入失败，不得当作清除成功。
                Err(e) => Err(e),
            };
        }
        // 从未修改过该显示器：无事可做，不写屏。
        Ok(RestoreOutcome::NothingToDo)
    }

    /// 退出清理：先在同一状态锁内禁止新意图并使旧任务失效，再逐台取操作锁、
    /// **在锁内**判断并执行恢复。不持状态锁等待操作锁或外部进程。
    /// 返回各显示器恢复的汇总计数（调用方可用于观测；失败计数不得计入 restored/degraded）。
    pub(crate) fn cleanup(&self) -> CleanupSummary {
        let cleanup_generations = self.begin_shutdown();
        let num_displays = cleanup_generations.len();
        let mut summary = CleanupSummary::default();

        for i in 0..num_displays {
            // 迭代索引来自 begin_shutdown 返回的长度，恒有效；仍防御性处理 None。
            let operation_lock = self.op_lock(i);
            let Some(operation_lock) = operation_lock else {
                log::error!("cleanup[{}]: 操作锁不存在，跳过该显示器恢复", i);
                summary.failed += 1;
                continue;
            };
            let _guard = operation_lock.lock().unwrap();
            // 已在操作锁内：退出期间不应再有普通任务递增版本（submit 已拒绝）。
            // 若仍观察到版本变化，不能当作“已清理”，而是继续在本锁内恢复。
            let current_generation = self
                .with_state(i, |s| s.operation_generation)
                .unwrap_or(u64::MAX); // 状态缺失视为异常版本，仍尝试恢复（restore 会报错）
            if current_generation != cleanup_generations[i] {
                log::error!(
                    "cleanup[{}]: 意外版本变化 ({} -> {})，仍在本锁内执行恢复",
                    i,
                    cleanup_generations[i],
                    current_generation
                );
            }
            match self.restore(i) {
                Ok(RestoreOutcome::Restored) => summary.restored += 1,
                Ok(RestoreOutcome::DegradedCleared) => summary.degraded += 1,
                Ok(RestoreOutcome::NothingToDo) => summary.idle += 1,
                Err(e) => {
                    // 恢复失败：restore_pending 保留 true，不声称已恢复。
                    log::error!("cleanup[{}]: 恢复失败，保留待恢复标记: {}", i, e);
                    summary.failed += 1;
                }
            }
        }
        log::info!(
            "cleanup: restored={} degraded={} idle={} failed={}",
            summary.restored,
            summary.degraded,
            summary.idle,
            summary.failed
        );
        summary
    }
}

/// 退出准入的快速检查（入口提前拒绝，优化体验）。
/// **权威检查在 [`DisplayOps::submit`] 的状态锁内**，此处不能代替它。
pub(crate) fn ensure_not_shutting_down() -> Result<(), String> {
    if SHUTTING_DOWN.load(Ordering::SeqCst) {
        return Err("应用正在退出，已拒绝新的滤镜操作".to_string());
    }
    Ok(())
}

/// 提交意图的统一入口：在状态锁内检查退出标志，通过后才修改状态 + 递增版本。
pub(crate) fn submit_intent<F, R>(idx: usize, f: F) -> Result<R, String>
where
    F: FnOnce(&mut DisplayState) -> R,
{
    global_ops()
        .submit(idx, f)
        .ok_or_else(|| "应用正在退出，已拒绝新的滤镜操作".to_string())
}

pub(crate) fn with_display_state<F, R>(idx: usize, f: F) -> R
where F: FnOnce(&mut DisplayState) -> R {
    ensure_display_states();
    let lock = DISPLAY_STATES.lock().unwrap();
    let states = lock.as_ref().unwrap();
    let idx = idx.min(states.len() - 1);
    let mut state = states[idx].lock().unwrap();
    f(&mut *state)
}

/// 当前连接的显示器数量（与 DISPLAY_STATES 键位一致，至少为 1）。
fn display_count() -> usize {
    DISPLAY_DEVICES.lock().unwrap().as_ref().map(|d| d.len()).unwrap_or(1).max(1)
}

/// 条件开启：在**单一状态锁闭包内**完成「当前未开启 → 置开启 → 递增版本」三件事。
/// 返回 `Some(新版本)`：本任务赢得开启权，可据此登记归属并派发版本化应用；
/// 返回 `None`：正在退出，或用户已在锁外预检查之后、本提交之前手动开启（版本已被
/// 推进）——资格不成立时**不创建有效自动归属，也不覆盖手动意图**。
/// 自动登记控制锁 + 控制状态：关闭命令与自动开启/登记共享的同步边界。
/// 控制状态（enabled + generation）的读取、修改和登记遵守**同一把锁边界**——
/// 锁外读取的 bool/代次快照不作为锁内依据。锁内不得等待物理恢复或跨 await。
pub(crate) fn auto_registration_guard() -> std::sync::MutexGuard<'static, AutoControlState> {
    static STATE: std::sync::Mutex<AutoControlState> = std::sync::Mutex::new(AutoControlState {
        enabled: false,
        generation: 0,
    });
    STATE.lock().unwrap()
}

/// 自动控制状态（由 auto_registration_guard 保护）。
pub(crate) struct AutoControlState {
    pub enabled: bool,
    pub generation: u64,
}

/// 自动开启入口：在**同一次控制锁持有**内完成「锁内读取 enabled → 锁内比较
/// generation 与 expected → 条件开启 → 登记归属」——**控制锁保持到归属登记完成
/// 之后才释放**，防止「检查通过 → 关闭命令清除归属 → 旧任务再登记」的窗口。
/// `expected_generation` 是调用方轮询线程启动时获得的代次，在锁内与实际当前代次
/// 比较——关闭后重开时旧代次即使当前 enabled=true 也会被拒绝。
/// 返回 `(display_idx, session, operation_generation)`：登记成功；
/// 返回 `None`：功能已关闭/代次已过期/滤镜已开启/正在退出——不创建归属、不写屏。
pub(crate) fn auto_register_owned(
    idx: usize,
    expected_generation: u64,
    session_counter: &AtomicU64,
    slot: &AutoOwnershipSlot,
) -> Option<(usize, u64, u64)> {
    let guard = auto_registration_guard();
    auto_register_owned_with(
        guard,
        idx,
        expected_generation,
        session_counter,
        slot,
        &global_ops(),
        |i| display_topology(i),
        None,
    )
}

/// 可测试变体：对注入的控制状态锁、DisplayOps 与拓扑查询执行（生产在全局控制锁内
/// 调用并传全局实例；测试传独立实例）。**测试路径内部不得隐式调用 global_ops() 或
/// 全局设备表。**
/// 控制锁保持到条件开启与归属登记**完成之后**才释放；锁内短暂取得状态锁/归属锁，
/// 不跨物理写屏或 await。
/// `before_enable` 测试钩子：在条件开启前于控制锁内调用（生产传 None）。
pub(crate) fn auto_register_owned_with(
    guard: std::sync::MutexGuard<'_, AutoControlState>,
    idx: usize,
    expected_generation: u64,
    session_counter: &AtomicU64,
    slot: &AutoOwnershipSlot,
    ops: &DisplayOps,
    topology: impl FnOnce(usize) -> Option<(String, usize)>,
    before_enable: Option<Box<dyn FnOnce() + Send>>,
) -> Option<(usize, u64, u64)> {
    if !guard.enabled {
        return None;
    }
    if guard.generation != expected_generation {
        return None;
    }
    // 测试钩子：交错测试在条件开启前暂停登记（仍持控制锁）。
    if let Some(hook) = before_enable {
        hook();
    }
    // 条件开启：同一状态锁闭包内「当前未开启 → 置开启 → 递增版本」。
    // 注意：仍在控制锁持有中——关闭命令需等到登记完成才能取得控制锁。
    let operation_generation = conditional_set_active_with(ops, idx, true)?;
    let session = session_counter.fetch_add(1, Ordering::Relaxed) + 1;
    let (device_name, display_count) = topology(idx)
        .unwrap_or_else(|| (String::new(), 1));
    *slot.lock().unwrap() = Some(AutoOwnership {
        session,
        display_idx: idx,
        device_name,
        display_count,
        operation_generation,
        state: AutoSessionState::Applying,
        restore_generation: None,
    });
    drop(guard); // 控制锁在登记完成后释放；物理应用在锁外执行。
    Some((idx, session, operation_generation))
}

/// 开关命令的锁内状态更新：**在同一控制锁边界内**检查当前状态 → 更新 enabled 与
/// generation（旧代次失效）→ 关闭时处置归属。返回 `(keep_restoring, new_generation)`：
/// keep_restoring 表示存在 Restoring 归属（调用方应安排恢复重试，锁外执行）；
/// new_generation 供开启分支启动新轮询线程。若开关状态未变化，返回 None。
pub(crate) fn auto_update_control_state(
    slot: &AutoOwnershipSlot,
    enabled: bool,
) -> Option<(bool, u64)> {
    let guard = auto_registration_guard();
    auto_update_control_state_with(guard, slot, enabled)
}

/// 可测试变体：对注入的控制状态锁执行（生产在全局控制锁内调用；测试用独立
/// Mutex<AutoControlState> 驱动，执行与生产共用的状态更新逻辑）。
pub(crate) fn auto_update_control_state_with(
    mut guard: std::sync::MutexGuard<'_, AutoControlState>,
    slot: &AutoOwnershipSlot,
    enabled: bool,
) -> Option<(bool, u64)> {
    if guard.enabled == enabled {
        return None;
    }
    let mut new_generation = guard.generation.wrapping_add(1);
    if new_generation == 0 {
        new_generation = 1;
    }
    guard.enabled = enabled;
    guard.generation = new_generation;
    let keep_restoring = {
        let mut ownership = slot.lock().unwrap();
        match ownership.as_ref() {
            Some(rec) if rec.state == AutoSessionState::Restoring => true,
            _ => {
                *ownership = None;
                false
            }
        }
    };
    Some((keep_restoring, new_generation))
}

/// 轮询线程启动时读取当前控制代次（锁内读取，用于与 expected 后续比较）。
pub(crate) fn auto_current_generation() -> u64 {
    auto_registration_guard().generation
}

/// 条件开启的可测试变体：对注入的 DisplayOps 实例执行（测试用独立上下文驱动）。
pub(crate) fn conditional_set_active_with(ops: &DisplayOps, idx: usize, active: bool) -> Option<u64> {
    ops.submit(idx, |s| {
        if s.filter_active == active {
            return None;
        }
        s.filter_active = active;
        Some(bump_operation_generation(s))
    })
    .flatten()
}

pub(crate) fn get_active_index() -> usize {
    let idx = ACTIVE_DISPLAY_INDEX.load(Ordering::SeqCst);
    ensure_display_states();
    let lock = DISPLAY_STATES.lock().unwrap();
    let states = lock.as_ref().unwrap();
    idx.min(states.len() - 1)
}

// ─── 自动（游戏）滤镜归属与条件回滚（Task D + Task F）───
//
// 归属模型替换原先的单个 AUTO_FILTER_ON 布尔：谁开启的（会话）、在哪台显示器、
// 对应哪个操作版本、处于什么状态。核心规则：
// 1. 应用完成的登记是**条件更新**：只有归属仍属于本会话且仍处 Applying 才置 Applied；
// 2. 过期/失败只能改变任务**自己拥有**的记录，不得清除新会话的归属；
// 3. 自动恢复永远针对登记时的目标显示器，不得重新解析 active index；
// 4. 归属核对与提交恢复意图在**同一次归属锁持有**内完成（归属锁 → 状态锁，
//    单一状态锁边界内核对版本并提交关闭，不允许先查版本、放锁、再无条件关闭）；
// 5. 用户手动操作会推进 operation_generation：版本不一致即视为用户接管，
//    自动任务弃权（丢弃自己的归属，不改新意图的状态、不写屏）；
// 6. 恢复失败保留可重试责任（Restoring 记录 + restore_pending），重试前仍检查接管。
//
// **统一锁顺序**：归属锁 → { 设备拓扑锁(DISPLAY_DEVICES) / 显示器状态锁 } → 操作锁。
// 归属锁只在本模块以下函数中获取，display_filter 的状态/操作锁路径从不回调归属锁，
// 不存在反向取锁。

/// 游戏自动滤镜的归属记录。
#[derive(Debug, Clone)]
pub(crate) struct AutoOwnership {
    /// 自动会话标识：每次自动开启尝试生成新值，用于条件登记/条件弃权。
    pub session: u64,
    /// 目标显示器索引：登记时确定，恢复时**不得**用 get_active_index() 替代。
    pub display_idx: usize,
    /// 登记时的设备名快照（拓扑失效检测：防止恢复到重排后的另一台显示器）。
    pub device_name: String,
    /// 登记时的显示器总数快照。
    pub display_count: usize,
    /// 自动应用对应的操作版本（应用意图提交时的版本）。
    pub operation_generation: u64,
    /// 归属状态。
    pub state: AutoSessionState,
    /// 恢复意图的版本（Restoring 阶段供重试使用）。
    pub restore_generation: Option<u64>,
}

/// 归属状态：Applying（应用进行中）→ Applied（游戏退出时负责恢复）→ Restoring（恢复中/待重试）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AutoSessionState {
    Applying,
    Applied,
    Restoring,
}

/// 归属槽位（game_filter 持有全局实例；测试可构造独立实例）。
pub(crate) type AutoOwnershipSlot = Mutex<Option<AutoOwnership>>;

/// 自动恢复决策结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AutoRestoreDecision {
    /// 已原子提交关闭意图：按返回的目标显示器与版本执行版本化恢复。
    Proceed { display_idx: usize, restore_generation: u64 },
    /// 版本已被用户操作接管：丢弃归属，不写屏。
    Superseded,
    /// 目标显示器拓扑已变化（数量/设备名不符）：明确退出，不写**任何**显示器。
    TargetGone,
    /// 归属不存在或不属于该会话（已被新自动会话接管）。
    NotOwned,
}

/// 条件登记：仅当槽位仍属于本会话、处于 Applying，**且操作版本仍未被推进**时置为
/// Applied。返回 false 表示归属已被新会话接管，或用户已手动操作推进了版本——
/// 同一会话并不代表它仍拥有当前显示状态，晚到任务不得登记成有效自动归属。
pub(crate) fn auto_mark_applied(slot: &AutoOwnershipSlot, session: u64, ops: &DisplayOps) -> bool {
    let mut guard = slot.lock().unwrap();
    match guard.as_mut() {
        Some(rec) if rec.session == session && rec.state == AutoSessionState::Applying => {
            let still_current = ops.with_state(rec.display_idx, |st| st.operation_generation)
                == Some(rec.operation_generation);
            if still_current {
                rec.state = AutoSessionState::Applied;
                true
            } else {
                // 用户已手动推进版本：本会话不拥有当前显示状态，条件弃权。
                *guard = None;
                false
            }
        }
        _ => false,
    }
}

/// 条件弃权：仅当槽位属于本会话时移除。过期/失败路径不得清除新会话的归属。
pub(crate) fn auto_release_if_owned(slot: &AutoOwnershipSlot, session: u64) -> bool {
    let mut guard = slot.lock().unwrap();
    match guard.as_ref() {
        Some(rec) if rec.session == session => {
            *guard = None;
            true
        }
        _ => false,
    }
}

/// 条件完成恢复：仅当槽位属于本会话且处于 Restoring 时移除（恢复成功收尾）。
pub(crate) fn auto_finish_restore(slot: &AutoOwnershipSlot, session: u64) -> bool {
    let mut guard = slot.lock().unwrap();
    match guard.as_ref() {
        Some(rec) if rec.session == session && rec.state == AutoSessionState::Restoring => {
            *guard = None;
            true
        }
        _ => false,
    }
}

/// 读取当前显示器拓扑（目标设备名 + 总数），供归属失效检测。
pub(crate) fn display_topology(idx: usize) -> Option<(String, usize)> {
    let lock = DISPLAY_DEVICES.lock().unwrap();
    let devs = lock.as_ref()?;
    let name = devs.get(idx)?.clone();
    Some((name, devs.len()))
}

impl<'a> DisplayOps<'a> {
    /// 条件关闭：在**单一状态锁边界**内核验版本，仍当前才置关并递增版本。
    /// 返回 `Some(新版本)`：本任务仍是最新意图，关闭已原子提交，可据此派发版本化恢复；
    /// 返回 `None`：版本已被更新意图接管（或应用正在退出），不修改任何状态。
    pub(crate) fn conditional_close(&self, idx: usize, expected_generation: u64) -> Option<u64> {
        self.submit(idx, |s| {
            if s.operation_generation != expected_generation {
                return None;
            }
            s.filter_active = false;
            Some(bump_operation_generation(s))
        })
        .flatten()
    }
}

/// 自动恢复决策：在**同一次归属锁持有**内完成「核对归属 → 拓扑失效检测 →
/// 原子条件关闭」。`topology(idx) -> Option<(设备名, 总数)>` 由调用方注入
/// （生产读全局设备表，测试注入独立拓扑）。
pub(crate) fn auto_restore_decision<F>(
    slot: &AutoOwnershipSlot,
    session: u64,
    ops: &DisplayOps,
    topology: F,
) -> AutoRestoreDecision
where
    F: FnOnce(usize) -> Option<(String, usize)>,
{
    let mut guard = slot.lock().unwrap();
    // 1. 归属核对：只处理自己拥有的 Applied 记录。
    let (idx, expected_gen, device_name, display_count) = {
        let Some(rec) = guard.as_ref() else {
            return AutoRestoreDecision::NotOwned;
        };
        if rec.session != session || rec.state != AutoSessionState::Applied {
            return AutoRestoreDecision::NotOwned;
        }
        (rec.display_idx, rec.operation_generation, rec.device_name.clone(), rec.display_count)
    };
    // 2. 拓扑失效检测：设备名或数量变化 → 明确退出，不写任何显示器。
    match topology(idx) {
        Some((name, count)) if name == device_name && count == display_count => {}
        _ => {
            *guard = None;
            return AutoRestoreDecision::TargetGone;
        }
    }
    // 3. 原子条件关闭：单一状态锁边界内核对版本并提交（不允许先查版本、放锁、再关闭）。
    match ops.conditional_close(idx, expected_gen) {
        Some(restore_generation) => {
            let rec = guard.as_mut().expect("归属记录存在性已在步骤 1 核对");
            rec.state = AutoSessionState::Restoring;
            rec.restore_generation = Some(restore_generation);
            AutoRestoreDecision::Proceed { display_idx: idx, restore_generation }
        }
        // 用户已接管（版本被推进）：丢弃旧归属，不覆盖用户选择。
        None => {
            *guard = None;
            AutoRestoreDecision::Superseded
        }
    }
}

/// 自动应用失败的统一条件回滚：在归属锁内核对本会话仍拥有 Applying 记录后，
/// 条件关闭（状态锁内原子核验版本），关闭成功则在**同一归属锁内**转为 Restoring
/// 并保存恢复版本，随后释放锁执行版本化恢复。恢复失败保留 Restoring 记录 +
/// restore_pending 供逐轮重试（统一规则 6：不得在恢复前删除唯一归属记录）。
/// 已过期则只记录失败，不修改新意图。
/// 返回回滚恢复的实际结果（调用方须按结果条件收尾，不能无条件清除归属）。
pub(crate) fn auto_rollback_failed_apply(
    slot: &AutoOwnershipSlot,
    session: u64,
    ops: &DisplayOps,
) -> RollbackOutcome {
    let mut guard = slot.lock().unwrap();
    let Some(rec) = guard.as_ref() else { return RollbackOutcome::Superseded };
    if rec.session != session || rec.state != AutoSessionState::Applying {
        return RollbackOutcome::Superseded;
    }
    let (idx, gen) = (rec.display_idx, rec.operation_generation);
    match ops.conditional_close(idx, gen) {
        Some(restore_generation) => {
            // 同一归属锁内转为 Restoring：恢复前不删除归属，失败保留重试责任。
            let rec = guard.as_mut().expect("归属记录存在性已在上面核对");
            rec.state = AutoSessionState::Restoring;
            rec.restore_generation = Some(restore_generation);
            drop(guard);
            // 同一版本化、串行写屏流程恢复；失败保留 Restoring + restore_pending。
            match restore_display_default_if_current_with(ops, idx, restore_generation) {
                Ok(RunResult::Executed(RestoreOutcome::Restored)) => RollbackOutcome::Restored,
                Ok(RunResult::Executed(RestoreOutcome::DegradedCleared)) => RollbackOutcome::DegradedCleared,
                Ok(RunResult::Executed(RestoreOutcome::NothingToDo)) => RollbackOutcome::NoRestoreNeeded,
                Ok(RunResult::SkippedStale) => RollbackOutcome::Superseded,
                Err(e) => RollbackOutcome::RestoreFailed(e),
            }
        }
        // 已过期：新意图接管，不改新意图、不写屏；记录由调用方条件释放。
        None => RollbackOutcome::Superseded,
    }
}

/// 应用失败的统一条件回滚（非自动路径：快捷键/命令层）：
/// 在状态锁内原子核验版本，仍当前则关闭并走版本化串行恢复；已过期只记录失败，
/// 不修改新意图、不写屏。可能已部分写屏时由 restore/restore_pending 保留责任。
pub(crate) fn rollback_failed_apply(idx: usize, failed_generation: u64) -> RollbackOutcome {
    let ops = global_ops();
    match ops.conditional_close(idx, failed_generation) {
        Some(restore_generation) => {
            match restore_display_default_if_current_with(&ops, idx, restore_generation) {
                Ok(RunResult::Executed(RestoreOutcome::Restored)) => RollbackOutcome::Restored,
                Ok(RunResult::Executed(RestoreOutcome::DegradedCleared)) => RollbackOutcome::DegradedCleared,
                Ok(RunResult::Executed(RestoreOutcome::NothingToDo)) => RollbackOutcome::NoRestoreNeeded,
                Ok(RunResult::SkippedStale) => RollbackOutcome::Superseded,
                Err(e) => RollbackOutcome::RestoreFailed(e),
            }
        }
        None => RollbackOutcome::Superseded,
    }
}

/// 生产便捷封装：使用全局协调层实例 + 全局设备拓扑。
pub(crate) fn auto_rollback_failed_apply_global(
    slot: &AutoOwnershipSlot,
    session: u64,
) -> RollbackOutcome {
    auto_rollback_failed_apply(slot, session, &global_ops())
}

/// 生产便捷封装：使用全局协调层实例 + 全局设备拓扑。
pub(crate) fn auto_restore_decision_global(slot: &AutoOwnershipSlot, session: u64) -> AutoRestoreDecision {
    auto_restore_decision(slot, session, &global_ops(), |idx| display_topology(idx))
}

fn resolve_display_index(display_index: Option<usize>) -> usize {
    display_index.unwrap_or_else(|| get_active_index())
}

// ─── Tool invocation layer (xcalib + icc_gen, via std::process::Command) ───

/// Get the path to a bundled tool in the resources directory.
fn get_tool_path(tool_name: &str) -> Result<PathBuf, String> {
    // In development: src-tauri/resources/binaries/icc-tools/
    // In production: resource_dir/binaries/icc-tools/
    let possible_paths = [
        // Dev path (relative to project root)
        PathBuf::from("src-tauri/resources/binaries/icc-tools").join(tool_name),
        // Dev path (relative to src-tauri)
        PathBuf::from("resources/binaries/icc-tools").join(tool_name),
        // Try from exe directory
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("resources/binaries/icc-tools").join(tool_name)))
            .unwrap_or_else(|| PathBuf::from("resources/binaries/icc-tools").join(tool_name)),
    ];

    for path in &possible_paths {
        if path.exists() {
            return Ok(path.clone());
        }
    }

    // Fallback: try Tauri resource dir via env
    if let Ok(resource_dir) = std::env::var("RESOURCE_DIR") {
        let path = PathBuf::from(resource_dir).join("binaries/icc-tools").join(tool_name);
        if path.exists() {
            return Ok(path);
        }
    }

    Err(format!("找不到工具程序: {} (搜索路径: {:?})", tool_name, possible_paths))
}

/// Get the path to a builtin ICC preset file.
/// Also tries the filename without the "NexBox_" prefix (build output may strip it).
fn get_builtin_icc_path(preset_filename: &str) -> Result<PathBuf, String> {
    // Try exact filename first
    if let Some(path) = try_find_icc_file(preset_filename) {
        return Ok(path);
    }

    // If not found and name starts with "NexBox_", try without the prefix
    if let Some(stripped) = preset_filename.strip_prefix("NexBox_") {
        if let Some(path) = try_find_icc_file(stripped) {
            log::info!("get_builtin_icc_path: found '{}' (without NexBox_ prefix)", stripped);
            return Ok(path);
        }
    }

    Err(format!("找不到内置 ICC 预设文件: {} (也尝试过不带 NexBox_ 前缀)", preset_filename))
}

fn try_find_icc_file(filename: &str) -> Option<PathBuf> {
    let exe_parent = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    let bases: [PathBuf; 3] = [
        PathBuf::from("src-tauri/resources/icc-presets"),
        PathBuf::from("resources/icc-presets"),
        exe_parent
            .as_ref()
            .map(|p| p.join("resources/icc-presets"))
            .unwrap_or_else(|| PathBuf::from("resources/icc-presets")),
    ];

    for base in &bases {
        let path = base.join(filename);
        if path.exists() {
            return Some(path);
        }
    }

    // Also check RESOURCE_DIR env var
    if let Ok(resource_dir) = std::env::var("RESOURCE_DIR") {
        let path = PathBuf::from(&resource_dir).join("icc-presets").join(filename);
        if path.exists() {
            return Some(path);
        }
    }

    None
}

/// Apply an ICC profile to the display using xcalib.exe.
/// Uses std::process::Command (CreateProcessW) — NOT PowerShell.
fn apply_icc_via_xcalib(icc_path: &Path, display_index: usize) -> Result<(), String> {
    let tool = get_tool_path("xcalib.exe")?;
    log::info!("apply_icc_via_xcalib[{}]: {} {}", display_index, tool.display(), icc_path.display());

    let mut cmd = Command::new(&tool);
    cmd.arg("-screen").arg(display_index.to_string());
    cmd.arg(icc_path);

    #[cfg(target_os = "windows")]
    {
        // CREATE_NO_WINDOW = 0x08000000 — prevents a console window from flashing
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }

    // 协调层（DisplayOps::apply_icc）会在调用本函数前置位 restore_pending；
    // 这里只做物理落屏，不参与待恢复标记管理（避免后端递归）。
    let output = cmd.output()
        .map_err(|e| format!("xcalib 调用失败: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        log::error!("xcalib 失败: stdout={}, stderr={}, code={:?}", stdout, stderr, output.status.code());
        return Err(format!("xcalib 应用 ICC 失败: {}", if stderr.is_empty() { stdout.to_string() } else { stderr.to_string() }));
    }

    log::info!("xcalib 应用成功: {}", icc_path.display());
    Ok(())
}

/// Get the temp ICC path for custom filter.
fn get_temp_icc_path(display_index: usize, label: &str) -> PathBuf {
    let config_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    let temp_dir = config_dir.join("NexBox").join("temp");
    let _ = fs::create_dir_all(&temp_dir);
    let sequence = TEMP_ICC_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    temp_dir.join(format!("{}_display_{}_{}.icc", label, display_index, sequence))
}

// ─── HDR detection ───

#[cfg(target_os = "windows")]
fn is_hdr_enabled() -> bool {
    use winreg::enums::*;
    use winreg::RegKey;

    if let Ok(video_settings) = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(r"Software\Microsoft\Windows\CurrentVersion\VideoSettings", KEY_READ)
    {
        for name_result in video_settings.enum_values() {
            let (name, value) = match name_result { Ok(v) => v, Err(_) => continue };
            if name.starts_with("EnableHdrForMonitor") {
                if value.vtype == winreg::enums::REG_DWORD && value.bytes.len() >= 4 {
                    let val = u32::from_le_bytes([value.bytes[0], value.bytes[1], value.bytes[2], value.bytes[3]]);
                    if val == 1 { return true; }
                }
            }
        }
    }
    false
}

#[cfg(not(target_os = "windows"))]
fn is_hdr_enabled() -> bool { false }

// ─── Filter mode and setting types ───

#[derive(serde::Serialize, Clone, Copy, PartialEq)]
pub enum FilterMode {
    Normal = 0, Vivid = 1, Movie = 2, Highlight = 3, Soft = 4,
    Gaming = 5, Reading = 6, DeExposure = 7, ShadowBoost = 8, BenQ = 9,
}

impl FilterMode {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => FilterMode::Vivid, 2 => FilterMode::Movie, 3 => FilterMode::Highlight,
            4 => FilterMode::Soft, 5 => FilterMode::Gaming, 6 => FilterMode::Reading,
            7 => FilterMode::DeExposure, 8 => FilterMode::ShadowBoost, 9 => FilterMode::BenQ,
            _ => FilterMode::Normal,
        }
    }
}

#[derive(serde::Serialize, Clone)]
pub struct FilterSettings {
    pub temperature: i32, pub brightness: i32, pub contrast: i32, pub saturation: i32,
    pub r_gamma: f64, pub g_gamma: f64, pub b_gamma: f64,
    pub s_curve: f64, pub r_boost: f64, pub g_boost: f64, pub b_boost: f64,
    pub mode: i32, pub is_active: bool,
    pub icc_active: bool, pub active_icc_id: Option<String>,
    pub preview_filter_icc: Option<String>,
    pub preview_tint_color_icc: Option<String>,
    pub preview_tint_opacity_icc: Option<f64>,
    /// 是否多滤镜叠加模式
    pub stacked: bool,
    /// 已应用的叠加组合 id 列表（应用顺序）
    pub stack_preset_ids: Vec<String>,
}

impl FilterSettings {
    fn from_display_state(state: &DisplayState) -> Self {
        let (preview_filter_icc, preview_tint_color_icc, preview_tint_opacity_icc) =
            if state.icc_active || state.stacked {
                if let Some(ref ramp) = state.icc_ramp {
                    let (pf, ptc, pto) = compute_icc_preview(ramp);
                    (if pf.is_empty() { None } else { Some(pf) }, ptc, pto)
                } else { (None, None, None) }
            } else { (None, None, None) };

        // ICC 激活时从真实 ramp 反推显示数值（温度/亮度/对比度/饱和度/gamma/S曲线/RGB增强），
        // 避免显示预设卡片上的硬编码参数。非 ICC 时使用 state 中已保存的参数。
        let (temperature, brightness, contrast, saturation, r_gamma, g_gamma, b_gamma, s_curve, r_boost, g_boost, b_boost) =
            if state.icc_active {
                if let Some(ref ramp) = state.icc_ramp {
                    let (t, b, c, s, g, sc, rb, gb, bb) = derive_params_from_icc_ramp(ramp);
                    (t, b, c, s, g, g, g, sc, rb, gb, bb)
                } else {
                    (state.temperature, state.brightness, state.contrast, state.saturation,
                     state.r_gamma, state.g_gamma, state.b_gamma, 0.0, 1.0, 1.0, 1.0)
                }
            } else {
                (state.temperature, state.brightness, state.contrast, state.saturation,
                 state.r_gamma, state.g_gamma, state.b_gamma, 0.0, 1.0, 1.0, 1.0)
            };

        FilterSettings {
            temperature, brightness, contrast, saturation,
            r_gamma, g_gamma, b_gamma,
            s_curve, r_boost, g_boost, b_boost,
            mode: state.mode, is_active: state.filter_active,
            icc_active: state.icc_active, active_icc_id: state.active_icc_id.clone(),
            preview_filter_icc, preview_tint_color_icc, preview_tint_opacity_icc,
            stacked: state.stacked,
            stack_preset_ids: state.stack_preset_ids.clone(),
        }
    }
}

#[derive(serde::Serialize)]
pub struct FilterResult {
    pub success: bool, pub message: String,
    /// 是否走了降级恢复（线性清除兜底，未保证还原原校色）。
    pub degraded: bool,
    /// 本次命令是否因被更新的操作取代而**未执行**写屏（版本过期跳过）。
    /// 语义：请求已正常处理（success=true），但操作本身没有发生；
    /// 调用方不能据此显示“已开启/已关闭/已清除成功”。
    #[serde(default)]
    pub skipped_stale: bool,
    pub settings: Option<FilterSettings>,
    pub preview_filter: Option<String>,
    pub preview_tint_color: Option<String>,
    pub preview_tint_opacity: Option<f64>,
}

#[derive(serde::Serialize)]
pub struct FilterPreset {
    pub id: String, pub name: String, pub mode: i32,
    pub temperature: i32, pub brightness: i32, pub contrast: i32, pub saturation: i32,
    pub description: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct CustomFilterSettings {
    pub temperature: i32, pub brightness: i32, pub contrast: i32, pub saturation: i32,
    #[serde(default = "default_one_f64")] pub r_gamma: f64,
    #[serde(default = "default_one_f64")] pub g_gamma: f64,
    #[serde(default = "default_one_f64")] pub b_gamma: f64,
}

fn default_one_f64() -> f64 { 1.0 }

impl Default for CustomFilterSettings {
    fn default() -> Self {
        Self { temperature: 6500, brightness: 100, contrast: 100, saturation: 100, r_gamma: 1.0, g_gamma: 1.0, b_gamma: 1.0 }
    }
}

static CUSTOM_SETTINGS: Mutex<Option<HashMap<usize, CustomFilterSettings>>> = Mutex::new(None);

fn get_settings_file_path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("NexBox").join("filter-settings.json")
}

// ─── Filter state persistence (survives app restart) ───

#[derive(serde::Serialize, serde::Deserialize, Clone, Default)]
struct PersistentFilterState {
    filter_active: bool,
    temperature: i32,
    brightness: i32,
    contrast: i32,
    saturation: i32,
    r_gamma: f64,
    g_gamma: f64,
    b_gamma: f64,
    mode: i32,
    icc_active: bool,
    active_icc_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    icc_ramp: Option<Vec<Vec<u16>>>,
    #[serde(default)]
    stacked: bool,
    #[serde(default)]
    stack_preset_ids: Vec<String>,
}

fn save_all_filter_states() {
    ensure_display_states();
    let lock = DISPLAY_STATES.lock().unwrap();
    let states = lock.as_ref().unwrap();
    
    let mut data: HashMap<usize, PersistentFilterState> = HashMap::new();
    for (i, mtx) in states.iter().enumerate() {
        let s = mtx.lock().unwrap();
        data.insert(i, PersistentFilterState {
            filter_active: s.filter_active,
            temperature: s.temperature,
            brightness: s.brightness,
            contrast: s.contrast,
            saturation: s.saturation,
            r_gamma: s.r_gamma,
            g_gamma: s.g_gamma,
            b_gamma: s.b_gamma,
            mode: s.mode,
            icc_active: s.icc_active,
            active_icc_id: s.active_icc_id.clone(),
            icc_ramp: s.icc_ramp.map(|r| r.iter().map(|ch| ch.to_vec()).collect()),
            stacked: s.stacked,
            stack_preset_ids: s.stack_preset_ids.clone(),
        });
    }

    let path = get_settings_file_path();
    if let Some(parent) = path.parent() { let _ = fs::create_dir_all(parent); }
    
    let string_map: HashMap<String, &PersistentFilterState> = data.iter().map(|(k, v)| (k.to_string(), v)).collect();
    let mut existing: serde_json::Value = if path.exists() {
        fs::read_to_string(&path).ok().and_then(|c| serde_json::from_str(&c).ok()).unwrap_or(serde_json::json!({}))
    } else { serde_json::json!({}) };
    existing["filter-state"] = serde_json::to_value(&string_map).unwrap();
    if let Err(e) = fs::write(&path, serde_json::to_string_pretty(&existing).unwrap()) {
        log::error!("save_all_filter_states: failed to write: {}", e);
    }
}

fn load_all_filter_states() -> HashMap<usize, PersistentFilterState> {
    let path = get_settings_file_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(state_value) = json.get("filter-state") {
                    if let Ok(map) = serde_json::from_value::<HashMap<String, PersistentFilterState>>(state_value.clone()) {
                        return map.into_iter().filter_map(|(k, v)| k.parse::<usize>().ok().map(|idx| (idx, v))).collect();
                    }
                }
            }
        }
    }
    HashMap::new()
}

/// Called once on startup: loads saved filter state into memory and re-applies
/// the ICC if the filter was active when the app last closed.
pub fn restore_state_on_startup() {
    #[cfg(target_os = "windows")]
    {
        ensure_display_states();
        let saved = load_all_filter_states();
        if saved.is_empty() { return; }

        for (idx, pstate) in &saved {
            with_display_state(*idx, |state| {
                state.filter_active = false;  // don't auto-apply on startup
                state.temperature = pstate.temperature;
                state.brightness = pstate.brightness;
                state.contrast = pstate.contrast;
                state.saturation = pstate.saturation;
                state.r_gamma = pstate.r_gamma;
                state.g_gamma = pstate.g_gamma;
                state.b_gamma = pstate.b_gamma;
                state.mode = pstate.mode;
                state.icc_active = pstate.icc_active;
                state.active_icc_id = pstate.active_icc_id.clone();
                state.icc_ramp = pstate.icc_ramp.as_ref().map(|r| {
                    let mut arr = [[0u16; 256]; 3];
                    for ch in 0..3.min(r.len()) {
                        for (i, &v) in r[ch].iter().enumerate().take(256) { arr[ch][i] = v; }
                    }
                    arr
                });
            });
            log::info!("restore_state_on_startup[{}]: loaded state (icc={}, toggle=OFF)", idx, pstate.icc_active);
        }
    }
}

fn load_custom_settings_from_file() -> HashMap<usize, CustomFilterSettings> {
    let path = get_settings_file_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(settings_value) = json.get("custom-filter-settings") {
                    if let Ok(map) = serde_json::from_value::<HashMap<String, CustomFilterSettings>>(settings_value.clone()) {
                        return map.into_iter().filter_map(|(k, v)| k.parse::<usize>().ok().map(|idx| (idx, v))).collect();
                    }
                }
            }
        }
    }
    HashMap::new()
}

fn save_custom_settings_to_file(settings: &HashMap<usize, CustomFilterSettings>) -> Result<(), String> {
    let path = get_settings_file_path();
    if let Some(parent) = path.parent() { let _ = fs::create_dir_all(parent); }
    let string_map: HashMap<String, &CustomFilterSettings> = settings.iter().map(|(k, v)| (k.to_string(), v)).collect();
    let mut existing: serde_json::Value = if path.exists() {
        fs::read_to_string(&path).ok().and_then(|c| serde_json::from_str(&c).ok()).unwrap_or(serde_json::json!({}))
    } else { serde_json::json!({}) };
    existing["custom-filter-settings"] = serde_json::to_value(&string_map).unwrap();
    fs::write(&path, serde_json::to_string_pretty(&existing).unwrap()).map_err(|e| format!("无法保存设置: {}", e))?;
    Ok(())
}

fn get_or_load_custom_settings() -> HashMap<usize, CustomFilterSettings> {
    let mut settings_lock = CUSTOM_SETTINGS.lock().unwrap();
    if settings_lock.is_none() {
        let settings = load_custom_settings_from_file();
        *settings_lock = Some(settings.clone());
        settings
    } else {
        settings_lock.as_ref().unwrap().clone()
    }
}

// ─── Gamma calculation (复用，用于导出 ICC 和 CSS 预览) ───

fn kelvin_to_rgb_multipliers(temperature: i32) -> (f64, f64, f64) {
    // 6500K (D65) is the standard white point — should produce no tint.
    // Without this early return the formula below gives green≈0.997, blue≈0.981
    // at 6500K, which makes the "Normal" preset visibly brighter/warmer than no filter.
    if temperature == 6500 {
        return (1.0, 1.0, 1.0);
    }
    let temp = temperature as f64 / 100.0;
    let red = if temp <= 66.0 { 1.0 } else {
        let r = temp - 60.0;
        (329.698727446 * r.powf(-0.1332047592) / 255.0).clamp(0.0, 1.0)
    };
    let green = if temp <= 66.0 {
        ((99.4708025861 * temp.ln() - 161.1195681661) / 255.0).clamp(0.0, 1.0)
    } else {
        let g = temp - 60.0;
        (288.1221695283 * g.powf(-0.0755148492) / 255.0).clamp(0.0, 1.0)
    };
    let blue = if temp >= 66.0 { 1.0 } else if temp <= 19.0 { 0.0 } else {
        let b = temp - 10.0;
        ((138.5177312231 * b.ln() - 305.0447927307) / 255.0).clamp(0.0, 1.0)
    };
    (red, green, blue)
}

fn apply_gamma_curve(input: f64, gamma: f64) -> f64 { input.powf(1.0 / gamma) }

fn apply_s_curve(input: f64, strength: f64) -> f64 {
    let strength = strength.clamp(-0.5, 0.5);
    let x = input - 0.5;
    (0.5 + x * (1.0 + strength * (1.0 - 4.0 * x * x))).clamp(0.0, 1.0)
}

fn build_gamma_ramp(
    temperature: i32, brightness: i32, contrast: i32, saturation: i32,
    mode: FilterMode, custom_gamma: Option<(f64, f64, f64)>,
) -> [[u16; 256]; 3] {
    let (r_temp_mult, g_temp_mult, b_temp_mult) = kelvin_to_rgb_multipliers(temperature);
    let brightness_factor = brightness as f64 / 100.0;
    let contrast_factor = contrast as f64 / 100.0;
    let sat_factor = saturation as f64 / 100.0;

    let (gamma, s_curve_strength, r_boost, g_boost, b_boost): (f64, f64, f64, f64, f64) = match mode {
        FilterMode::Normal => (1.0, 0.0, 1.0, 1.0, 1.0),
        FilterMode::Vivid => (0.95, 0.08, 1.02, 1.0, 1.03),
        FilterMode::Movie => (1.05, -0.05, 1.0, 0.98, 0.96),
        FilterMode::Highlight => (0.92, 0.05, 1.0, 1.0, 1.0),
        FilterMode::Soft => (1.08, -0.08, 0.98, 1.0, 1.02),
        FilterMode::Gaming => (0.96, 0.1, 1.0, 1.0, 1.02),
        FilterMode::Reading => (1.0, 0.0, 1.0, 0.99, 0.97),
        FilterMode::DeExposure => (0.96, -0.05, 1.0, 1.0, 1.0),
        FilterMode::ShadowBoost => (1.12, 0.03, 1.0, 1.0, 1.0),
        FilterMode::BenQ => (1.12, 0.08, 1.0, 1.0, 1.02),
    };

    let use_per_channel = custom_gamma.is_some()
        && (custom_gamma.unwrap().0 - 1.0).abs() > 0.001
        || (custom_gamma.unwrap_or((1.0, 1.0, 1.0)).1 - 1.0).abs() > 0.001
        || (custom_gamma.unwrap_or((1.0, 1.0, 1.0)).2 - 1.0).abs() > 0.001;

    let (r_gamma, g_gamma, b_gamma) = custom_gamma.unwrap_or((gamma, gamma, gamma));
    let mut ramp = [[0u16; 256]; 3];

    for i in 0..256 {
        let input = i as f64 / 255.0;

        let (r_adj, g_adj, b_adj) = if use_per_channel {
            (apply_gamma_curve(input, r_gamma), apply_gamma_curve(input, g_gamma), apply_gamma_curve(input, b_gamma))
        } else {
            let adj = apply_gamma_curve(input, gamma);
            (adj, adj, adj)
        };

        let r_adj = apply_s_curve(r_adj, s_curve_strength);
        let g_adj = apply_s_curve(g_adj, s_curve_strength);
        let b_adj = apply_s_curve(b_adj, s_curve_strength);

        let r_adj = ((r_adj - 0.5) * contrast_factor + 0.5) * brightness_factor;
        let g_adj = ((g_adj - 0.5) * contrast_factor + 0.5) * brightness_factor;
        let b_adj = ((b_adj - 0.5) * contrast_factor + 0.5) * brightness_factor;

        let r_base = r_adj.clamp(0.0, 1.0) * 65535.0;
        let g_base = g_adj.clamp(0.0, 1.0) * 65535.0;
        let b_base = b_adj.clamp(0.0, 1.0) * 65535.0;

        let r_final = (r_base * r_temp_mult * r_boost).min(65535.0);
        let g_final = (g_base * g_temp_mult * g_boost).min(65535.0);
        let b_final = (b_base * b_temp_mult * b_boost).min(65535.0);

        let r_luma = 0.299 * r_final; let g_luma = 0.587 * g_final; let b_luma = 0.114 * b_final;
        let luma = r_luma + g_luma + b_luma;

        let r_out = if (sat_factor - 1.0).abs() > 0.001 { luma + (r_final - luma) * sat_factor } else { r_final };
        let g_out = if (sat_factor - 1.0).abs() > 0.001 { luma + (g_final - luma) * sat_factor } else { g_final };
        let b_out = if (sat_factor - 1.0).abs() > 0.001 { luma + (b_final - luma) * sat_factor } else { b_final };

        ramp[0][i] = r_out.clamp(0.0, 65535.0) as u16;
        ramp[1][i] = g_out.clamp(0.0, 65535.0) as u16;
        ramp[2][i] = b_out.clamp(0.0, 65535.0) as u16;
    }

    // Monotonic constraint
    for channel in 0..3 {
        for i in 1..256 {
            if ramp[channel][i] < ramp[channel][i - 1] { ramp[channel][i] = ramp[channel][i - 1]; }
        }
    }
    ramp[0][0] = 0; ramp[1][0] = 0; ramp[2][0] = 0;
    ramp[0][255] = 65535; ramp[1][255] = 65535; ramp[2][255] = 65535;
    ramp
}

// ─── ICC preview (CSS filter approximation, 复用) ───

fn compute_icc_preview(ramp: &[[u16; 256]; 3]) -> (String, Option<String>, Option<f64>) {
    let mut ch_brightness = [1.0f64; 3];
    for c in 0..3 {
        let mut sum = 0.0; let mut count = 0u32;
        for i in 32..224 {
            let identity = (i as u32 * 256) as u16;
            if identity > 0 { sum += ramp[c][i as usize] as f64 / identity as f64; count += 1; }
        }
        if count > 0 { ch_brightness[c] = sum / count as f64; }
    }
    let avg_brightness = (ch_brightness[0] + ch_brightness[1] + ch_brightness[2]) / 3.0;
    if (avg_brightness - 1.0).abs() < 0.015
        && (ch_brightness[0] - ch_brightness[1]).abs() < 0.015
        && (ch_brightness[1] - ch_brightness[2]).abs() < 0.015
    { return (String::new(), None, None); }

    let mut filters: Vec<String> = Vec::new();
    if (avg_brightness - 1.0).abs() > 0.01 { filters.push(format!("brightness({:.3})", avg_brightness.clamp(0.3, 2.5))); }
    let filter_str = filters.join(" ");

    let drift_r = ch_brightness[0] - avg_brightness;
    let drift_g = ch_brightness[1] - avg_brightness;
    let drift_b = ch_brightness[2] - avg_brightness;
    let max_drift = drift_r.abs().max(drift_g.abs()).max(drift_b.abs());

    if max_drift > 0.02 {
        let r = ((0.5 + drift_r * 3.0).clamp(0.0, 1.0) * 255.0) as u8;
        let g = ((0.5 + drift_g * 3.0).clamp(0.0, 1.0) * 255.0) as u8;
        let b = ((0.5 + drift_b * 3.0).clamp(0.0, 1.0) * 255.0) as u8;
        let opacity = (max_drift * 1.5).min(0.4);
        (filter_str, Some(format!("#{:02X}{:02X}{:02X}", r, g, b)), Some(opacity))
    } else { (filter_str, None, None) }
}

/// 从 ICC gamma ramp 反推近似显示参数（温度/亮度/对比度/饱和度/gamma）。
/// 用于让右侧「当前设置」面板显示真实生效的数值，而不是预设卡片的硬编码参数。
/// 返回 (temperature, brightness, contrast, saturation, gamma, s_curve, r_boost, g_boost, b_boost)。
fn derive_params_from_icc_ramp(ramp: &[[u16; 256]; 3]) -> (i32, i32, i32, i32, f64, f64, f64, f64, f64) {
    // 每通道平均增益（32..224 区间，与 compute_icc_preview 一致）
    let mut ch_brightness = [1.0f64; 3];
    for c in 0..3 {
        let mut sum = 0.0; let mut count = 0u32;
        for i in 32..224 {
            let identity = (i as u32 * 256) as u16;
            if identity > 0 { sum += ramp[c][i as usize] as f64 / identity as f64; count += 1; }
        }
        if count > 0 { ch_brightness[c] = sum / count as f64; }
    }
    let avg_brightness = (ch_brightness[0] + ch_brightness[1] + ch_brightness[2]) / 3.0;

    // 亮度 = 平均增益 × 100
    let brightness = (avg_brightness * 100.0).round().clamp(50.0, 150.0) as i32;

    // 饱和度 = 各通道相对平均的偏移程度（偏移越大越饱和）
    let spread = (ch_brightness[0] - avg_brightness).abs()
        .max((ch_brightness[1] - avg_brightness).abs())
        .max((ch_brightness[2] - avg_brightness).abs());
    let saturation = (100.0 + spread * 120.0).round().clamp(50.0, 150.0) as i32;

    // 色温：红/蓝通道相对强弱 → 偏暖(红强)温度低，偏冷(蓝强)温度高
    let red_blue_ratio = if ch_brightness[2] > 0.001 { ch_brightness[0] / ch_brightness[2] } else { 1.0 };
    let temperature = (6500.0 - (red_blue_ratio - 1.0) * 2500.0).round().clamp(1000.0, 10000.0) as i32;

    // 对比度：暗部(32..96)与亮部(160..224)增益之比，比值越大对比度越高
    let mut dark_sum = 0.0; let mut dark_count = 0u32;
    let mut light_sum = 0.0; let mut light_count = 0u32;
    for c in 0..3 {
        for i in 32..96 {
            let identity = (i as u32 * 256) as u16;
            if identity > 0 { dark_sum += ramp[c][i as usize] as f64 / identity as f64; dark_count += 1; }
        }
        for i in 160..224 {
            let identity = (i as u32 * 256) as u16;
            if identity > 0 { light_sum += ramp[c][i as usize] as f64 / identity as f64; light_count += 1; }
        }
    }
    let dark_avg = if dark_count > 0 { dark_sum / dark_count as f64 } else { 1.0 };
    let light_avg = if light_count > 0 { light_sum / light_count as f64 } else { 1.0 };
    let contrast = (100.0 + (light_avg - dark_avg) * 120.0).round().clamp(50.0, 150.0) as i32;

    // gamma：用中间调(128)反推，output = input^(1/gamma) → gamma = ln(input)/ln(output)
    let input_mid: f64 = 128.0 / 255.0;
    let output_mid = ramp[0][128] as f64 / 65535.0;
    let gamma = if output_mid > 0.001 {
        (input_mid.ln() / output_mid.ln()).clamp(0.5, 2.0)
    } else { 1.0 };

    // S-Curve：暗部压暗 + 亮部提亮 的程度（相对于中性），与对比度方向一致
    // 用 暗部增益与 1 的偏差、亮部增益与 1 的偏差 平均来近似 S 曲线强度
    let dark_dev = 1.0 - dark_avg;      // >0 表示暗部被压暗
    let light_dev = light_avg - 1.0;    // >0 表示亮部被提亮
    let s_curve = ((dark_dev + light_dev) * 0.5).clamp(-0.5, 0.5);

    // RGB Boost：各通道增益相对平均的比值（>1 表示该通道被加强）
    let r_boost = if avg_brightness > 0.001 { ch_brightness[0] / avg_brightness } else { 1.0 };
    let g_boost = if avg_brightness > 0.001 { ch_brightness[1] / avg_brightness } else { 1.0 };
    let b_boost = if avg_brightness > 0.001 { ch_brightness[2] / avg_brightness } else { 1.0 };

    (temperature, brightness, contrast, saturation, gamma, s_curve, r_boost, g_boost, b_boost)
}

// ─── ICC parsing (复用) ───

fn read_u32_be(data: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]])
}
fn read_u16_be(data: &[u8], offset: usize) -> u16 { u16::from_be_bytes([data[offset], data[offset + 1]]) }

fn parse_icc_file(file_path: &str) -> Result<IccPreset, String> {
    let mut file = fs::File::open(file_path).map_err(|e| format!("无法打开文件: {}", e))?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).map_err(|e| format!("无法读取文件: {}", e))?;

    if data.len() < 132 { return Err("文件太小，不是有效的 ICC 文件".to_string()); }
    if &data[36..40] != b"acsp" { return Err("不是有效的 ICC 文件（magic number 不正确）".to_string()); }

    let profile_size = read_u32_be(&data, 0) as usize;
    if data.len() < profile_size { return Err("ICC 文件大小不匹配".to_string()); }

    let tag_count = read_u32_be(&data, 128) as usize;
    if data.len() < 132 + tag_count * 12 { return Err("ICC 标签表损坏".to_string()); }

    let mut vcgt_offset: Option<u32> = None;
    let mut r_trc_offset: Option<u32> = None;
    let mut g_trc_offset: Option<u32> = None;
    let mut b_trc_offset: Option<u32> = None;

    for i in 0..tag_count {
        let tag_start = 132 + i * 12;
        let tag_sig = &data[tag_start..tag_start + 4];
        let tag_offset = read_u32_be(&data, tag_start + 4);
        match tag_sig {
            b"vcgt" => vcgt_offset = Some(tag_offset),
            b"rTRC" => r_trc_offset = Some(tag_offset),
            b"gTRC" => g_trc_offset = Some(tag_offset),
            b"bTRC" => b_trc_offset = Some(tag_offset),
            _ => {}
        }
    }

    if r_trc_offset.is_none() {
        for i in 0..tag_count {
            let tag_start = 132 + i * 12;
            let tag_sig = &data[tag_start..tag_start + 4];
            if tag_sig == b"kTRC" {
                let offset = read_u32_be(&data, tag_start + 4);
                r_trc_offset = Some(offset); g_trc_offset = Some(offset); b_trc_offset = Some(offset);
                break;
            }
        }
    }

    fn read_s15fixed16(data: &[u8], offset: usize) -> f64 {
        i32::from_be_bytes([data[offset], data[offset+1], data[offset+2], data[offset+3]]) as f64 / 65536.0
    }

    let parse_curve = |offset: u32| -> Result<[u16; 256], String> {
        let off = offset as usize;
        if off + 12 > data.len() { return Err("曲线数据偏移超出文件范围".to_string()); }
        let curve_type = &data[off..off + 4];
        let mut ramp = [0u16; 256];

        if curve_type == b"curv" {
            let count = read_u32_be(&data, off + 8) as usize;
            if off + 12 + count * 2 > data.len() { return Err("曲线数据长度超出文件范围".to_string()); }
            if count == 0 { for i in 0..256 { ramp[i] = (i * 257) as u16; } }
            else if count == 1 {
                let gamma = read_u16_be(&data, off + 12) as f64 / 256.0;
                for i in 0..256 { ramp[i] = ((i as f64 / 255.0).powf(gamma) * 65535.0).clamp(0.0, 65535.0) as u16; }
            } else {
                for i in 0..256 {
                    let src_idx = (i as f64 / 255.0 * (count - 1) as f64) as usize;
                    let frac = (i as f64 / 255.0 * (count - 1) as f64) - src_idx as f64;
                    let v0 = read_u16_be(&data, off + 12 + src_idx * 2);
                    let v1 = if src_idx + 1 < count { read_u16_be(&data, off + 12 + (src_idx + 1) * 2) } else { v0 };
                    ramp[i] = ((v0 as f64 + (v1 as f64 - v0 as f64) * frac) as u16).min(65535);
                }
            }
        } else if curve_type == b"para" {
            if off + 16 > data.len() { return Err("参数化曲线数据不完整".to_string()); }
            let func_type = read_u16_be(&data, off + 8);
            let params_offset = off + 12;
            for i in 0..256 {
                let x = i as f64 / 255.0;
                let y = match func_type {
                    0 => { let g = read_s15fixed16(&data, params_offset); x.powf(g) }
                    1 => { let g = read_s15fixed16(&data, params_offset); let a = read_s15fixed16(&data, params_offset + 4); let b = read_s15fixed16(&data, params_offset + 8); let threshold = if a.abs() > 1e-10 { -b / a } else { 0.0 }; if x >= threshold { (a * x + b).max(0.0).powf(g) } else { 0.0 } }
                    2 => { let g = read_s15fixed16(&data, params_offset); let a = read_s15fixed16(&data, params_offset + 4); let b = read_s15fixed16(&data, params_offset + 8); let c = read_s15fixed16(&data, params_offset + 12); let threshold = if a.abs() > 1e-10 { -b / a } else { 0.0 }; if x >= threshold { (a * x + b).max(0.0).powf(g) + c } else { c } }
                    3 => { let g = read_s15fixed16(&data, params_offset); let a = read_s15fixed16(&data, params_offset + 4); let b = read_s15fixed16(&data, params_offset + 8); let c = read_s15fixed16(&data, params_offset + 12); let d = read_s15fixed16(&data, params_offset + 16); if x >= d { (a * x + b).max(0.0).powf(g) + c } else { c * x } }
                    4 => { let g = read_s15fixed16(&data, params_offset); let a = read_s15fixed16(&data, params_offset + 4); let b = read_s15fixed16(&data, params_offset + 8); let c = read_s15fixed16(&data, params_offset + 12); let d = read_s15fixed16(&data, params_offset + 16); let e = read_s15fixed16(&data, params_offset + 20); let f = read_s15fixed16(&data, params_offset + 24); if x >= d { (a * x + b).max(0.0).powf(g) + e } else { c * x + f } }
                    _ => return Err(format!("不支持的参数化曲线函数类型: {}", func_type)),
                };
                ramp[i] = (y.clamp(0.0, 1.0) * 65535.0).clamp(0.0, 65535.0) as u16;
            }
        } else {
            return Err(format!("不支持的曲线类型: {:?}（仅支持 'curv' 和 'para'）", std::str::from_utf8(curve_type).unwrap_or("?")));
        }
        Ok(ramp)
    };

    let parse_vcgt = |offset: u32| -> Result<[[u16; 256]; 3], String> {
        let off = offset as usize;
        if off + 18 > data.len() { return Err("vcgt 数据不完整".to_string()); }
        let formula_type = read_u32_be(&data, off + 8);
        if formula_type != 0 { return Err(format!("不支持的 vcgt 公式类型: {}", formula_type)); }
        let channels = read_u16_be(&data, off + 12) as usize;
        let entries = read_u16_be(&data, off + 14) as usize;
        let entry_size = read_u16_be(&data, off + 16) as usize;
        if channels != 3 || entries != 256 || entry_size != 2 {
            return Err(format!("不支持的 vcgt 格式: channels={}, entries={}, entry_size={}", channels, entries, entry_size));
        }
        let data_start = off + 18;
        let data_end = data_start + channels * entries * entry_size;
        if data_end > data.len() { return Err("vcgt 数据超出文件范围".to_string()); }
        let mut ramp = [[0u16; 256]; 3];
        for ch in 0..3 {
            let ch_start = data_start + ch * entries * entry_size;
            for i in 0..entries { ramp[ch][i] = read_u16_be(&data, ch_start + i * entry_size); }
        }
        Ok(ramp)
    };

    let ramp = if let Some(vcgt_off) = vcgt_offset {
        match parse_vcgt(vcgt_off) {
            Ok(vcgt_ramp) => { log::info!("Using vcgt tag for gamma ramp"); vcgt_ramp }
            Err(e) => {
                log::warn!("vcgt 解析失败: {}，回退到 TRC 曲线", e);
                let r_ramp = parse_curve(r_trc_offset.ok_or("ICC 文件中未找到 rTRC 曲线")?)?;
                let g_ramp = parse_curve(g_trc_offset.ok_or("ICC 文件中未找到 gTRC 曲线")?)?;
                let b_ramp = parse_curve(b_trc_offset.ok_or("ICC 文件中未找到 bTRC 曲线")?)?;
                [r_ramp, g_ramp, b_ramp]
            }
        }
    } else {
        let r_ramp = parse_curve(r_trc_offset.ok_or("ICC 文件中未找到 rTRC 曲线")?)?;
        let g_ramp = parse_curve(g_trc_offset.ok_or("ICC 文件中未找到 gTRC 曲线")?)?;
        let b_ramp = parse_curve(b_trc_offset.ok_or("ICC 文件中未找到 bTRC 曲线")?)?;
        [r_ramp, g_ramp, b_ramp]
    };

    let name = Path::new(file_path).file_stem().and_then(|s| s.to_str()).unwrap_or("ICC Profile").to_string();
    let description = format!("ICC 配置文件: {}", name);

    let id = format!("icc_{}", name);

    Ok(IccPreset { id, name, ramp: ramp.iter().map(|ch| ch.to_vec()).collect(), description })
}

// ─── ICC profile generation (复用，用于导出) ───

fn push_u32_be(buf: &mut Vec<u8>, val: u32) { buf.extend_from_slice(&val.to_be_bytes()); }
fn push_u16_be(buf: &mut Vec<u8>, val: u16) { buf.extend_from_slice(&val.to_be_bytes()); }
fn push_s15fixed16(buf: &mut Vec<u8>, val: f64) { buf.extend_from_slice(&((val * 65536.0).round() as i32).to_be_bytes()); }
fn pad_to_4(buf: &mut Vec<u8>) { while buf.len() % 4 != 0 { buf.push(0); } }

fn build_icc_profile(ramp: &[[u16; 256]; 3], description: &str) -> Vec<u8> {
    let mut blocks: Vec<([u8; 4], Vec<u8>)> = Vec::new();

    // desc
    { let mut d = Vec::new(); d.extend_from_slice(b"desc"); d.extend_from_slice(&[0u8; 4]);
      let desc_bytes = description.as_bytes(); push_u32_be(&mut d, desc_bytes.len() as u32 + 1);
      d.extend_from_slice(desc_bytes); d.push(0); pad_to_4(&mut d);
      push_u32_be(&mut d, 0); push_u32_be(&mut d, 0); push_u16_be(&mut d, 2); d.push(0); d.extend_from_slice(&[0u8; 67]);
      blocks.push((*b"desc", d)); }

    // cprt
    { let mut d = Vec::new(); d.extend_from_slice(b"text"); d.extend_from_slice(&[0u8; 4]);
      d.extend_from_slice(b"NexBox Exported ICC Profile\0"); pad_to_4(&mut d);
      blocks.push((*b"cprt", d)); }

    // wtpt
    { let mut d = Vec::new(); d.extend_from_slice(b"XYZ "); d.extend_from_slice(&[0u8; 4]);
      push_s15fixed16(&mut d, 0.9505); push_s15fixed16(&mut d, 1.0000); push_s15fixed16(&mut d, 1.0890);
      blocks.push((*b"wtpt", d)); }

    // rXYZ/gXYZ/bXYZ
    { let colorants: [([u8;4], f64, f64, f64); 3] = [
        (*b"rXYZ", 0.4360, 0.2225, 0.0139),
        (*b"gXYZ", 0.3851, 0.7169, 0.0971),
        (*b"bXYZ", 0.1431, 0.0606, 0.7141),
      ];
      for (sig, x, y, z) in colorants {
        let mut d = Vec::new(); d.extend_from_slice(b"XYZ "); d.extend_from_slice(&[0u8; 4]);
        push_s15fixed16(&mut d, x); push_s15fixed16(&mut d, y); push_s15fixed16(&mut d, z);
        blocks.push((sig, d));
      } }

    // rTRC/gTRC/bTRC (identity)
    { let mut d = Vec::new(); d.extend_from_slice(b"curv"); d.extend_from_slice(&[0u8; 4]); push_u32_be(&mut d, 0);
      blocks.push((*b"rTRC", d.clone())); blocks.push((*b"gTRC", d.clone())); blocks.push((*b"bTRC", d)); }

    // vcgt
    { let mut d = Vec::new(); d.extend_from_slice(b"vcgt"); d.extend_from_slice(&[0u8; 4]);
      push_u32_be(&mut d, 0); push_u16_be(&mut d, 3); push_u16_be(&mut d, 256); push_u16_be(&mut d, 2);
      for ch in 0..3 { for i in 0..256 { push_u16_be(&mut d, ramp[ch][i]); } }
      blocks.push((*b"vcgt", d)); }

    let num_tags = blocks.len();
    let header_size: usize = 128;
    let tag_table_size: usize = 4 + num_tags * 12;
    let data_start = header_size + tag_table_size;

    let mut all_data: Vec<u8> = Vec::new();
    let mut entries: Vec<([u8; 4], usize, usize)> = Vec::new();
    for (sig, data) in &blocks {
        let offset = data_start + all_data.len();
        entries.push((*sig, offset, data.len()));
        all_data.extend_from_slice(data);
        pad_to_4(&mut all_data);
    }

    let profile_size = data_start + all_data.len();
    let mut profile = Vec::with_capacity(profile_size);
    push_u32_be(&mut profile, profile_size as u32);
    push_u32_be(&mut profile, 0); // CMM
    push_u32_be(&mut profile, 0x0210_0000); // version 2.1
    profile.extend_from_slice(b"mntr"); profile.extend_from_slice(b"RGB "); profile.extend_from_slice(b"XYZ ");
    push_u16_be(&mut profile, 2025); push_u16_be(&mut profile, 1); push_u16_be(&mut profile, 1);
    push_u16_be(&mut profile, 0); push_u16_be(&mut profile, 0); push_u16_be(&mut profile, 0);
    profile.extend_from_slice(b"acsp");
    push_u32_be(&mut profile, 0); push_u32_be(&mut profile, 0); push_u32_be(&mut profile, 0); push_u32_be(&mut profile, 0);
    profile.extend_from_slice(&[0u8; 8]);
    push_u32_be(&mut profile, 0);
    push_s15fixed16(&mut profile, 0.9642); push_s15fixed16(&mut profile, 1.0000); push_s15fixed16(&mut profile, 0.8249);
    push_u32_be(&mut profile, 0);
    profile.extend_from_slice(&[0u8; 16]); profile.extend_from_slice(&[0u8; 28]);

    push_u32_be(&mut profile, num_tags as u32);
    for (sig, offset, size) in &entries { profile.extend_from_slice(sig); push_u32_be(&mut profile, *offset as u32); push_u32_be(&mut profile, *size as u32); }
    profile.extend_from_slice(&all_data);

    let final_size = profile.len() as u32;
    profile[0..4].copy_from_slice(&final_size.to_be_bytes());
    profile
}

// ─── ICC preset management ───

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct IccPreset {
    pub id: String, pub name: String,
    pub ramp: Vec<Vec<u16>>, pub description: String,
}

impl IccPreset {
    fn to_ramp_array(&self) -> [[u16; 256]; 3] {
        let mut ramp = [[0u16; 256]; 3];
        for c in 0..3 { for i in 0..256 { ramp[c][i] = self.ramp[c][i]; } }
        ramp
    }
}

#[derive(serde::Serialize, Clone)]
pub struct IccPresetInfo { pub id: String, pub name: String, pub description: String }

#[derive(serde::Serialize)]
pub struct IccImportResult { pub success: bool, pub message: String, pub preset: Option<IccPresetInfo> }

static ICC_PRESETS: Mutex<Option<Vec<IccPreset>>> = Mutex::new(None);

fn get_icc_file_path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("NexBox").join("icc_presets.json")
}

fn load_icc_presets_from_file() -> Vec<IccPreset> {
    let path = get_icc_file_path();
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(presets) = serde_json::from_str::<Vec<IccPreset>>(&content) { return presets; }
    }
    Vec::new()
}

fn save_icc_presets_to_file(presets: &[IccPreset]) -> Result<(), String> {
    let path = get_icc_file_path();
    if let Some(parent) = path.parent() { let _ = fs::create_dir_all(parent); }
    fs::write(&path, serde_json::to_string_pretty(presets).map_err(|e| format!("序列化失败: {}", e))?)
        .map_err(|e| format!("无法保存: {}", e))?;
    Ok(())
}

fn get_or_load_icc_presets() -> Vec<IccPreset> {
    let mut lock = ICC_PRESETS.lock().unwrap();
    if lock.is_none() {
        let presets = load_icc_presets_from_file();
        *lock = Some(presets.clone());
        presets
    } else { lock.as_ref().unwrap().clone() }
}

/// Load builtin ICC presets from resources/icc-presets/ directory.
fn load_builtin_icc_preset_infos() -> Vec<IccPresetInfo> {
    let mut result = Vec::new();

    // Scan all possible icc-presets directories
    let search_dirs = [
        PathBuf::from("src-tauri/resources/icc-presets"),
        PathBuf::from("resources/icc-presets"),
    ];

    let mut icc_dir: Option<PathBuf> = None;
    for dir in &search_dirs {
        if dir.exists() { icc_dir = Some(dir.clone()); break; }
    }

    if icc_dir.is_none() {
        if let Ok(p) = std::env::current_exe() {
            if let Some(parent) = p.parent() {
                let dir = parent.join("resources/icc-presets");
                if dir.exists() { icc_dir = Some(dir); }
            }
        }
    }

    if let Some(dir) = icc_dir {
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("icc") || e.eq_ignore_ascii_case("icm")).unwrap_or(false) {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        // Skip preset ICCs that already appear in the filter preset grid.
                        // These are the NexBox_* files (鲜艳, 电影, 去曝光Pro, etc.).
                        if stem.starts_with("NexBox_") {
                            continue;
                        }
                        let description = format!("内置 ICC 预设: {}", stem);
                        result.push(IccPresetInfo {
                            id: format!("builtin_{}", stem),
                            name: stem.to_string(),
                            description,
                        });
                    }
                }
            }
        }
    }

    // Sort by name for consistent ordering
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

/// Get the file path for a builtin ICC preset by its filename.
fn get_builtin_icc_filename(preset_id: &str) -> Option<String> {
    // preset_id format: "builtin_NexBox_游戏"
    let filename = preset_id.strip_prefix("builtin_")?;
    Some(format!("{}.icc", filename))
}

// ─── User filter presets (复用) ───

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct UserFilterPreset {
    pub id: String, pub name: String,
    pub temperature: i32, pub brightness: i32, pub contrast: i32, pub saturation: i32,
    #[serde(default = "default_one_f64")] pub r_gamma: f64,
    #[serde(default = "default_one_f64")] pub g_gamma: f64,
    #[serde(default = "default_one_f64")] pub b_gamma: f64,
}

#[derive(serde::Serialize, Clone)]
pub struct UserFilterPresetInfo {
    pub id: String, pub name: String,
    pub temperature: i32, pub brightness: i32, pub contrast: i32, pub saturation: i32,
    pub r_gamma: f64, pub g_gamma: f64, pub b_gamma: f64,
}

static USER_FILTER_PRESETS: Mutex<Option<Vec<UserFilterPreset>>> = Mutex::new(None);

fn get_user_filter_presets_file_path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("NexBox").join("user-filter-presets.json")
}

fn load_user_filter_presets_from_file() -> Vec<UserFilterPreset> {
    let path = get_user_filter_presets_file_path();
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(presets) = serde_json::from_str::<Vec<UserFilterPreset>>(&content) { return presets; }
    }
    Vec::new()
}

fn save_user_filter_presets_to_file(presets: &[UserFilterPreset]) -> Result<(), String> {
    let path = get_user_filter_presets_file_path();
    if let Some(parent) = path.parent() { let _ = fs::create_dir_all(parent); }
    fs::write(&path, serde_json::to_string_pretty(presets).map_err(|e| format!("序列化失败: {}", e))?)
        .map_err(|e| format!("无法保存: {}", e))?;
    Ok(())
}

// ─── Tauri commands ───

#[tauri::command]
pub async fn get_displays() -> Result<Vec<DisplayInfo>, String> {
    #[cfg(target_os = "windows")]
    {
        let displays = tauri::async_runtime::spawn_blocking(|| enumerate_displays_inner())
            .await.map_err(|e| format!("枚举显示器失败: {}", e))?;

        if !displays.is_empty() && displays.iter().all(|d| d.width <= 0 || d.height <= 0) {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let retry = tauri::async_runtime::spawn_blocking(|| enumerate_displays_inner())
                .await.map_err(|e| format!("枚举显示器重试失败: {}", e))?;
            if retry.iter().any(|d| d.width > 0 && d.height > 0) { return Ok(retry); }
        }
        Ok(displays)
    }
    #[cfg(not(target_os = "windows"))]
    { Err("此功能仅支持 Windows 系统".to_string()) }
}

#[tauri::command]
pub async fn set_active_display(display_index: usize) -> Result<(), String> {
    ensure_display_states();
    ACTIVE_DISPLAY_INDEX.store(display_index, Ordering::SeqCst);
    Ok(())
}

/// Check filter support (xcalib availability + HDR status)
#[derive(serde::Serialize)]
pub struct GammaSupportInfo {
    pub display_index: usize,
    pub supported: bool,
    pub caps_value: i32,
    pub ramp_readable: bool,
    pub hdr_enabled: bool,
    pub reason: String,
}

#[tauri::command]
pub async fn check_gamma_support(display_index: Option<usize>) -> Result<GammaSupportInfo, String> {
    let idx = resolve_display_index(display_index);

    // Check if xcalib.exe exists
    let xcalib_available = get_tool_path("xcalib.exe").is_ok();
    let hdr_enabled = is_hdr_enabled();

    let supported = xcalib_available && !hdr_enabled;

    let reason = if hdr_enabled {
        "检测到 Windows HDR 已开启，请关闭 HDR 后重试。".to_string()
    } else if xcalib_available {
        "xcalib 工具可用，滤镜应能正常工作".to_string()
    } else {
        "找不到 xcalib.exe 工具程序，请检查安装是否完整".to_string()
    };

    Ok(GammaSupportInfo {
        display_index: idx, supported, caps_value: 0, ramp_readable: xcalib_available,
        hdr_enabled, reason,
    })
}

#[tauri::command]
pub async fn get_filter_settings(display_index: Option<usize>) -> Result<FilterSettings, String> {
    let idx = resolve_display_index(display_index);
    Ok(with_display_state(idx, |state| FilterSettings::from_display_state(state)))
}

// ─── 协调层：应用滤镜（要求调用方已持有该显示器的操作锁）───

impl<'a> DisplayOps<'a> {
    /// 将当前状态应用到显示器。要求调用方已持有该显示器的操作锁。
    /// 越界索引返回 Err（不夹取到其他显示器）。
    ///
    /// 注意：本函数假定调用方已通过 [`DisplayOps::run_if_current`] 在操作锁内做过
    /// 版本复查，且内部 `capture_if_current` 会在捕获完成后**再次复查版本**——
    /// 捕获是慢操作，期间可能被新的关闭意图推进版本，必须防止“已关闭但滤镜随后
    /// 又被写回屏幕”。
    pub(crate) fn apply(&self, idx: usize, expected_generation: u64, operation_name: &str) -> Result<RunResult<()>, String> {
        // 首次写屏前捕获原始硬件 ramp（每个显示器只捕获一次）。退出/禁用时据此精确
        // 恢复，避免线性清除把图形控制台/颜色管理里的 sRGB 校色清掉。
        // 捕获失败则中止应用，绝不继续写屏；捕获完成后复查版本，过期则中止写屏。
        if self.capture_if_current(idx, expected_generation, operation_name)? == CaptureOutcome::Skip {
            log::info!("apply_filter[{}]: 捕获后版本已过期/退出，中止应用写屏", idx);
            // 捕获后的版本/退出复查发现过期：必须把“跳过”向外传播为 SkippedStale，
            // 不能包装成 Executed(())——调用方会把未应用的滤镜登记为成功。
            return Ok(RunResult::SkippedStale);
        }

        // 多滤镜叠加模式：优先走叠加组合（ramp 复合 → ICC → 落屏）
        {
            let stacked = self
                .with_state(idx, |s| s.stacked && !s.stack_preset_ids.is_empty())
                .ok_or_else(|| format!("apply_filter[{}]: 显示器状态不存在，已拒绝操作", idx))?;
            if stacked {
                log::info!("apply_filter_to_display[{}]: 叠加模式，应用滤镜组合", idx);
                return self.apply_stack(idx, expected_generation, operation_name);
            }
        }

        let (icc_active, temperature, brightness, contrast, saturation, r_gamma, g_gamma, b_gamma, mode) =
            self.with_state(idx, |state| {
                (state.icc_active, state.temperature, state.brightness, state.contrast,
                 state.saturation, state.r_gamma, state.g_gamma, state.b_gamma, state.mode)
            })
            .ok_or_else(|| format!("apply_filter[{}]: 显示器状态不存在，已拒绝操作", idx))?;

        if icc_active {
            // ICC 模式：重新应用已保存的 ICC。
            let active_id = self
                .with_state(idx, |s| s.active_icc_id.clone())
                .ok_or_else(|| format!("apply_filter[{}]: 显示器状态不存在，已拒绝操作", idx))?;
            log::info!("apply_filter_to_display[{}]: ICC mode active (id={:?}), re-applying ICC", idx, active_id);

            if let Some(ref id) = active_id {
                if let Some(filename) = id.strip_prefix("builtin_") {
                    let icc_filename = format!("{}.icc", filename);
                    if let Ok(icc_path) = get_builtin_icc_path(&icc_filename) {
                        return self.apply_icc(idx, &icc_path).map(RunResult::Executed);
                    }
                    log::warn!("apply_filter_to_display[{}]: builtin ICC '{}' not found", idx, icc_filename);
                }
                // 用户导入 ICC —— 从保存的 ramp 重新应用
                if let Some(ramp) = self
                    .with_state(idx, |s| s.icc_ramp.clone())
                    .ok_or_else(|| format!("apply_filter[{}]: 显示器状态不存在，已拒绝操作", idx))?
                {
                    let icc_data = build_icc_profile(&ramp, "NexBox ICC Preset");
                    let temp_icc = get_temp_icc_path(idx, "icc_reapply");
                    if let Err(e) = fs::write(&temp_icc, &icc_data) {
                        log::error!("apply_filter_to_display[{}]: failed to write temp ICC: {}", idx, e);
                        return Err(format!("无法写入临时 ICC 文件: {}", e));
                    }
                    return self.apply_generated_icc(idx, &temp_icc).map(RunResult::Executed);
                }
            }
            return Ok(RunResult::Executed(()));
        }

        // 识别完全中性的参数：重置到默认时恢复原始 ramp，不应用任何特定 ICC。
        let is_identity = temperature == 6500
            && brightness == 100
            && contrast == 100
            && saturation == 100
            && mode == 0  // Normal
            && (r_gamma - 1.0).abs() < 0.001
            && (g_gamma - 1.0).abs() < 0.001
            && (b_gamma - 1.0).abs() < 0.001;

        if is_identity {
            log::info!("apply_filter_to_display[{}]: identity params → restore original ramp", idx);
            return self.restore(idx).map(|_| RunResult::Executed(()));
        }

        let temp_icc = get_temp_icc_path(idx, "custom_filter");
        let mode_enum = FilterMode::from_i32(mode);
        let custom_gamma = Some((r_gamma, g_gamma, b_gamma));
        let ramp = build_gamma_ramp(temperature, brightness, contrast, saturation, mode_enum, custom_gamma);
        let icc_data = build_icc_profile(&ramp, "NexBox Custom Filter");
        fs::write(&temp_icc, &icc_data).map_err(|e| format!("无法写入临时 ICC 文件: {}", e))?;
        self.apply_generated_icc(idx, &temp_icc).map(RunResult::Executed)
    }

    /// 应用叠加组合。要求调用方已持有该显示器的操作锁。
    pub(crate) fn apply_stack(&self, idx: usize, expected_generation: u64, operation_name: &str) -> Result<RunResult<()>, String> {
        let ids = self
            .with_state(idx, |s| s.stack_preset_ids.clone())
            .ok_or_else(|| format!("apply_stack[{}]: 显示器状态不存在，已拒绝操作", idx))?;
        if ids.is_empty() {
            return Err("叠加组合为空".to_string());
        }
        // 捕获失败则中止；capture 已持有 ramp 时直接复用，不会重复捕获或覆盖原始值。
        // 捕获完成后复查版本：捕获期间可能被新的关闭意图推进版本，过期则中止写屏。
        if self.capture_if_current(idx, expected_generation, operation_name)? == CaptureOutcome::Skip {
            log::info!("apply_stack[{}]: 捕获后版本已过期/退出，中止叠加写屏", idx);
            // 与 apply 一致：捕获后的版本复查发现过期，向外传播为 SkippedStale。
            return Ok(RunResult::SkippedStale);
        }
        let mut ramps = Vec::with_capacity(ids.len());
        for id in &ids {
            match preset_id_to_ramp(id) {
                Some(ramp) => ramps.push(ramp),
                None => return Err(format!("叠加滤镜解析失败，找不到滤镜: {}", id)),
            }
        }
        let composed = compose_ramps(&ramps);
        let temp_icc = get_temp_icc_path(idx, "filter_stack");
        let icc_data = build_icc_profile(&composed, "NexBox Filter Stack");
        fs::write(&temp_icc, &icc_data).map_err(|e| format!("无法写入临时 ICC 文件: {}", e))?;
        self.apply_generated_icc(idx, &temp_icc).map(RunResult::Executed)
    }
}

// ─── 生产路径薄封装（委托到全局协调层）───

/// 生产使用的应用包装逻辑（可在测试中用独立 DisplayOps 实例驱动）：
/// 捕获后版本复查发现过期时，向调用方返回**单层** `SkippedStale`。
pub(crate) fn apply_filter_if_current_with(
    ops: &DisplayOps,
    idx: usize,
    generation: u64,
) -> Result<RunResult<()>, String> {
    ops.run_if_current(idx, generation, "apply_filter", false, || {
        ops.apply(idx, generation, "apply_filter")
    })
    .map(|r| match r {
        RunResult::Executed(inner) => inner,
        RunResult::SkippedStale => RunResult::SkippedStale,
    })
}

pub(crate) fn apply_filter_to_display_if_current(idx: usize, generation: u64) -> Result<RunResult<()>, String> {
    apply_filter_if_current_with(&global_ops(), idx, generation)
}

/// 版本化恢复执行器（可在测试中用独立 DisplayOps 实例驱动）。
pub(crate) fn restore_display_default_if_current_with(
    ops: &DisplayOps,
    idx: usize,
    generation: u64,
) -> Result<RunResult<RestoreOutcome>, String> {
    ops.run_if_current(idx, generation, "restore_filter", false, || ops.restore(idx))
}

pub(crate) fn restore_display_default_if_current(idx: usize, generation: u64) -> Result<RunResult<RestoreOutcome>, String> {
    restore_display_default_if_current_with(&global_ops(), idx, generation)
}

#[tauri::command]
pub async fn set_filter_settings(
    display_index: Option<usize>,
    temperature: i32, brightness: i32, contrast: i32, saturation: i32,
    mode: i32, is_active: bool,
    r_gamma: Option<f64>, g_gamma: Option<f64>, b_gamma: Option<f64>,
) -> Result<FilterResult, String> {
    #[cfg(target_os = "windows")]
    {
        ensure_not_shutting_down()?;
        let idx = resolve_display_index(display_index);
        let temperature = temperature.clamp(1000, 10000);
        let brightness = brightness.clamp(50, 150);
        let contrast = contrast.clamp(50, 150);
        let saturation = saturation.clamp(50, 150);
        let mode = mode.clamp(0, 9);
        let r_gamma = r_gamma.unwrap_or(1.0).clamp(0.50, 2.00);
        let g_gamma = g_gamma.unwrap_or(1.0).clamp(0.50, 2.00);
        let b_gamma = b_gamma.unwrap_or(1.0).clamp(0.50, 2.00);

        let (actually_active, operation_generation) = with_display_state(idx, |state| {
            state.temperature = temperature;
            state.brightness = brightness;
            state.contrast = contrast;
            state.saturation = saturation;
            state.r_gamma = r_gamma;
            state.g_gamma = g_gamma;
            state.b_gamma = b_gamma;
            state.mode = mode;
            state.icc_active = false;
            state.active_icc_id = None;
            state.stacked = false;
            state.stack_preset_ids.clear();
            if is_active && !state.filter_active { state.filter_active = true; }
            let generation = if state.filter_active {
                bump_operation_generation(state)
            } else {
                state.operation_generation
            };
            (state.filter_active, generation)
        });

        if actually_active {
            let idx_move = idx;
            let outcome = tauri::async_runtime::spawn_blocking(move || apply_filter_to_display_if_current(idx_move, operation_generation))
                .await.map_err(|e| format!("Filter apply error: {}", e))?;
            match outcome {
                Ok(RunResult::Executed(())) => {}
                Ok(RunResult::SkippedStale) => {
                    // 已被更新意图接管：如实返回，不得报“已更新”。
                    log::info!("set_filter_settings[{}]: 应用任务已过期，未执行写屏", idx);
                    save_all_filter_states();
                    return Ok(with_display_state(idx, |state| FilterResult {
                        success: true,
                        message: "操作已被更新的操作取代，未执行".to_string(),
                        skipped_stale: true,
                        degraded: false,
                        settings: Some(FilterSettings::from_display_state(state)),
                        preview_filter: None, preview_tint_color: None, preview_tint_opacity: None,
                    }));
                }
                Err(e) => {
                    // 统一条件回滚：状态锁内原子核验版本，仍当前才关闭并恢复默认显示。
                    // 恢复结果必须记录，不能静默丢弃（可能部分写屏）。
                    match rollback_failed_apply(idx, operation_generation) {
                        RollbackOutcome::Restored => {
                            log::warn!("滤镜应用失败，已精确恢复默认显示: {}", e);
                        }
                        RollbackOutcome::DegradedCleared => {
                            log::warn!("滤镜应用失败，已降级清除滤镜: {}", e);
                        }
                        RollbackOutcome::NoRestoreNeeded => {}
                        RollbackOutcome::Superseded => {
                            log::info!("滤镜应用失败，恢复意图已被更新的操作取代（未执行）: {}", e);
                        }
                        RollbackOutcome::RestoreFailed(re) => {
                            log::error!("滤镜应用失败且回滚恢复也失败（保留待恢复标记）: {}; {}", e, re);
                        }
                    }
                    save_all_filter_states();
                    return Err(format!("滤镜应用失败: {}", e));
                }
            }
        }

        save_all_filter_states();

        Ok(FilterResult {
            success: true, message: "滤镜设置已更新".to_string(),
            skipped_stale: false,
            degraded: false,
            settings: Some(FilterSettings {
                temperature, brightness, contrast, saturation, r_gamma, g_gamma, b_gamma,
                s_curve: 0.0, r_boost: 1.0, g_boost: 1.0, b_boost: 1.0,
                mode, is_active: actually_active, icc_active: false, active_icc_id: None,
                preview_filter_icc: None, preview_tint_color_icc: None, preview_tint_opacity_icc: None,
                stacked: false, stack_preset_ids: Vec::new(),
            }),
            preview_filter: None, preview_tint_color: None, preview_tint_opacity: None,
        })
    }
    #[cfg(not(target_os = "windows"))]
    { Err("此功能仅支持 Windows 系统".to_string()) }
}

#[tauri::command]
pub async fn enable_filter(display_index: Option<usize>) -> Result<FilterResult, String> {
    #[cfg(target_os = "windows")]
    {
        ensure_not_shutting_down()?;
        let idx = resolve_display_index(display_index);
        let (already_active, operation_generation) = with_display_state(idx, |state| {
            if state.filter_active {
                (true, state.operation_generation)
            } else {
                state.filter_active = true;
                let generation = bump_operation_generation(state);
                (false, generation)
            }
        });

        if already_active {
            return Ok(with_display_state(idx, |state| FilterResult {
                success: true, message: "滤镜已处于启用状态".to_string(),
                skipped_stale: false,
                degraded: false,
                settings: Some(FilterSettings::from_display_state(state)),
                preview_filter: None, preview_tint_color: None, preview_tint_opacity: None,
            }));
        }

        let idx_move = idx;
        let outcome = tauri::async_runtime::spawn_blocking(move || apply_filter_to_display_if_current(idx_move, operation_generation))
            .await.map_err(|e| format!("Filter apply error: {}", e))?;
        // 应用被更新的意图接管（SkippedStale）：如实返回，不得报“已启用”。
        let (message, skipped_stale) = match outcome {
            Ok(RunResult::Executed(())) => ("滤镜已启用".to_string(), false),
            Ok(RunResult::SkippedStale) => {
                log::info!("enable_filter[{}]: 应用任务已过期，未执行写屏", idx);
                ("操作已被更新的操作取代，未执行".to_string(), true)
            }
            Err(e) => {
                // 统一条件回滚：状态锁内原子核验版本，仍当前才关闭并恢复默认显示。
                match rollback_failed_apply(idx, operation_generation) {
                    RollbackOutcome::Restored => {
                        log::warn!("enable_filter[{}]: 应用失败，已精确恢复默认显示: {}", idx, e);
                    }
                    RollbackOutcome::DegradedCleared => {
                        log::warn!("enable_filter[{}]: 应用失败，已降级清除滤镜: {}", idx, e);
                    }
                    RollbackOutcome::NoRestoreNeeded => {}
                    RollbackOutcome::Superseded => {
                        log::info!("enable_filter[{}]: 应用失败，恢复意图已被更新的操作取代（未执行）: {}", idx, e);
                    }
                    RollbackOutcome::RestoreFailed(re) => {
                        log::error!("enable_filter[{}]: 应用失败且回滚恢复也失败（保留待恢复标记）: {}; {}", idx, e, re);
                    }
                }
                save_all_filter_states();
                return Err(format!("滤镜应用失败: {}", e));
            }
        };

        save_all_filter_states();

        Ok(with_display_state(idx, |state| FilterResult {
            success: true,
            message,
            skipped_stale,
            degraded: false,
            settings: Some(FilterSettings::from_display_state(state)),
            preview_filter: None, preview_tint_color: None, preview_tint_opacity: None,
        }))
    }
    #[cfg(not(target_os = "windows"))]
    { Err("此功能仅支持 Windows 系统".to_string()) }
}

#[tauri::command]
pub async fn disable_filter(display_index: Option<usize>) -> Result<FilterResult, String> {
    #[cfg(target_os = "windows")]
    {
        ensure_not_shutting_down()?; // 快速检查（权威检查在 submit 的状态锁内）
        let idx = resolve_display_index(display_index);
        // 每个被接受的关闭意图都在同一状态同步边界内：检查退出 + 置 filter_active=false
        // + 递增版本，然后派发一个版本化恢复。恢复是否真正写屏由 restore 在操作锁内
        // 根据 restore_pending / 原始 ramp 决定：未受影响的显示器返回 NothingToDo，不写屏。
        let operation_generation = submit_intent(idx, |state| {
            state.filter_active = false;
            bump_operation_generation(state)
        })?;

        let idx_move = idx;
        let outcome = tauri::async_runtime::spawn_blocking(move || restore_display_default_if_current(idx_move, operation_generation))
            .await.map_err(|e| format!("Filter restore error: {}", e))?;
        let (message, degraded, skipped_stale) = match outcome {
            Ok(RunResult::Executed(RestoreOutcome::Restored)) => ("滤镜已禁用".to_string(), false, false),
            Ok(RunResult::Executed(RestoreOutcome::DegradedCleared)) => {
                // 降级清除：滤镜已移除，但原有校色未保证恢复，必须如实告知。
                log::warn!("disable_filter[{}]: 降级清除成功，原有校色未保证恢复", idx);
                ("滤镜已关闭（降级清除：未能恢复原有校色）".to_string(), true, false)
            }
            Ok(RunResult::Executed(RestoreOutcome::NothingToDo)) => ("滤镜已禁用".to_string(), false, false),
            Ok(RunResult::SkippedStale) => {
                // 已有更新意图接管（如重新开启），本次关闭的恢复被正确跳过。
                // 不得报“已禁用”完成：只说明请求已被更新操作取代。
                log::info!("disable_filter[{}]: 恢复任务已过期，跳过", idx);
                ("操作已被更新的操作取代，未执行".to_string(), false, true)
            }
            Err(e) => {
                // 恢复失败必须可见、可重试。restore_pending 保留 true，下次关闭仍会重试。
                log::error!("disable_filter[{}]: 恢复默认显示失败: {}", idx, e);
                save_all_filter_states();
                return Err(format!("滤镜恢复失败: {}", e));
            }
        };

        save_all_filter_states();

        Ok(with_display_state(idx, |state| FilterResult {
            success: true, message, degraded, skipped_stale,
            settings: Some(FilterSettings::from_display_state(state)),
            preview_filter: None, preview_tint_color: None, preview_tint_opacity: None,
        }))
    }
    #[cfg(not(target_os = "windows"))]
    { Err("此功能仅支持 Windows 系统".to_string()) }
}

#[tauri::command]
pub async fn toggle_filter(display_index: Option<usize>) -> Result<FilterResult, String> {
    let idx = resolve_display_index(display_index);
    let is_active = with_display_state(idx, |state| state.filter_active);
    if is_active { disable_filter(display_index).await } else { enable_filter(display_index).await }
}

/// Toggle filter on/off. Used by global hotkey.
pub fn toggle_filter_sync(app_handle: &tauri::AppHandle) -> Result<FilterResult, String> {
    #[cfg(target_os = "windows")]
    {
        ensure_not_shutting_down()?;
        let idx = get_active_index();
        let is_active = with_display_state(idx, |state| state.filter_active);
        let result = if is_active {
            // 关闭：每个被接受的关闭意图都在同一状态同步边界内完成（检查退出 + 置关
            // + 递增版本），然后派发版本化恢复；恢复在操作锁内决定是否真正写屏。
            let operation_generation = submit_intent(idx, |state| {
                state.filter_active = false;
                bump_operation_generation(state)
            })?;
            let (message, degraded, skipped_stale) = match restore_display_default_if_current(idx, operation_generation) {
                Ok(RunResult::Executed(RestoreOutcome::Restored)) => ("滤镜已禁用".to_string(), false, false),
                Ok(RunResult::Executed(RestoreOutcome::DegradedCleared)) => {
                    log::warn!("toggle_filter_sync[{}]: 降级清除成功，原有校色未保证恢复", idx);
                    ("滤镜已关闭（降级清除：未能恢复原有校色）".to_string(), true, false)
                }
                Ok(RunResult::Executed(RestoreOutcome::NothingToDo)) => ("滤镜已禁用".to_string(), false, false),
                Ok(RunResult::SkippedStale) => {
                    log::info!("toggle_filter_sync[{}]: 恢复任务已过期，跳过", idx);
                    ("操作已被更新的操作取代，未执行".to_string(), false, true)
                }
                Err(e) => {
                    // 恢复失败必须可见、可重试（restore_pending 保留 true）。
                    log::error!("toggle_filter_sync[{}]: 恢复默认显示失败: {}", idx, e);
                    return Err(format!("滤镜恢复失败: {}", e));
                }
            };
            Ok(with_display_state(idx, |state| FilterResult {
                success: true, message, degraded, skipped_stale,
                settings: Some(FilterSettings::from_display_state(state)),
                preview_filter: None, preview_tint_color: None, preview_tint_opacity: None,
            }))
        } else {
            // 启用
            let operation_generation = submit_intent(idx, |state| {
                state.filter_active = true;
                bump_operation_generation(state)
            })?;
            match apply_filter_to_display_if_current(idx, operation_generation) {
                Ok(RunResult::Executed(())) => {}
                Ok(RunResult::SkippedStale) => {
                    log::info!("toggle_filter_sync[{}]: 应用任务已过期，跳过", idx);
                }
                Err(e) => {
                    log::error!("toggle_filter_sync[{}]: 应用滤镜失败: {}", idx, e);
                    // 统一条件回滚：在状态锁内原子核验版本——仍当前才关闭并走
                    // 同一版本化、串行恢复流程（可能已部分写屏，restore_pending 由
                    // 恢复路径保留）；已过期只记录失败，不修改新意图、不写屏。
                    match rollback_failed_apply(idx, operation_generation) {
                        RollbackOutcome::Restored => {
                            log::warn!("toggle_filter_sync[{}]: 应用失败，已精确恢复默认显示", idx);
                        }
                        RollbackOutcome::DegradedCleared => {
                            log::warn!("toggle_filter_sync[{}]: 应用失败，已降级清除滤镜", idx);
                        }
                        RollbackOutcome::NoRestoreNeeded => {}
                        RollbackOutcome::Superseded => {
                            log::info!("toggle_filter_sync[{}]: 应用失败，恢复意图已被更新的操作取代（未执行）", idx);
                        }
                        RollbackOutcome::RestoreFailed(re) => {
                            log::error!("toggle_filter_sync[{}]: 应用失败且回滚恢复也失败（保留待恢复标记）: {}", idx, re);
                        }
                    }
                    return Err(e);
                }
            }
            Ok(with_display_state(idx, |state| FilterResult {
                success: true, message: "滤镜已启用".to_string(), skipped_stale: false, degraded: false,
                settings: Some(FilterSettings::from_display_state(state)),
                preview_filter: None, preview_tint_color: None, preview_tint_opacity: None,
            }))
        };

        if result.is_ok() { let _ = app_handle.emit("filter-status-changed", ()); }
        result
    }
    #[cfg(not(target_os = "windows"))]
    { Err("此功能仅支持 Windows 系统".to_string()) }
}

// ─── Filter presets ───

#[tauri::command]
pub async fn get_filter_presets() -> Result<Vec<FilterPreset>, String> {
    Ok(vec![
        FilterPreset { id: "de-exposure-pro".to_string(), name: "去曝光Pro".to_string(), mode: 0, temperature: 6500, brightness: 100, contrast: 100, saturation: 100, description: "专业去曝光，保护高光细节".to_string() },
        FilterPreset { id: "vivid".to_string(), name: "鲜艳".to_string(), mode: 1, temperature: 6800, brightness: 102, contrast: 105, saturation: 115, description: "增强色彩饱和度，画面更鲜艳".to_string() },
        FilterPreset { id: "movie".to_string(), name: "电影".to_string(), mode: 2, temperature: 5800, brightness: 98, contrast: 95, saturation: 95, description: "电影质感，柔和色调".to_string() },
        FilterPreset { id: "highlight".to_string(), name: "高亮".to_string(), mode: 3, temperature: 7200, brightness: 110, contrast: 102, saturation: 100, description: "提高亮度，适合暗光环境".to_string() },
        FilterPreset { id: "soft".to_string(), name: "柔和".to_string(), mode: 4, temperature: 5200, brightness: 98, contrast: 92, saturation: 95, description: "柔和画面，减少眼睛疲劳".to_string() },
        FilterPreset { id: "gaming".to_string(), name: "游戏".to_string(), mode: 5, temperature: 6800, brightness: 103, contrast: 108, saturation: 110, description: "增强对比度和色彩，适合游戏".to_string() },
        FilterPreset { id: "reading".to_string(), name: "阅读".to_string(), mode: 6, temperature: 4800, brightness: 95, contrast: 100, saturation: 92, description: "暖色调，保护眼睛".to_string() },
        FilterPreset { id: "de-exposure".to_string(), name: "去曝光".to_string(), mode: 7, temperature: 6500, brightness: 92, contrast: 103, saturation: 98, description: "压暗高光，降低过度曝光，恢复高光细节".to_string() },
        FilterPreset { id: "shadow-boost".to_string(), name: "暗部增强".to_string(), mode: 8, temperature: 6500, brightness: 106, contrast: 94, saturation: 104, description: "提亮暗部阴影，让黑暗角落的敌人无处遁形".to_string() },
        FilterPreset { id: "dam-contrast".to_string(), name: "大坝降低对比度".to_string(), mode: 0, temperature: 6500, brightness: 100, contrast: 100, saturation: 100, description: "降低对比度，保护高光细节，画面更柔和".to_string() },
        FilterPreset { id: "aerospace".to_string(), name: "航天推荐".to_string(), mode: 0, temperature: 6500, brightness: 100, contrast: 100, saturation: 100, description: "航天基地专属色彩调教".to_string() },
        FilterPreset { id: "whiter".to_string(), name: "偏白".to_string(), mode: 0, temperature: 6500, brightness: 100, contrast: 100, saturation: 100, description: "整体偏白调，亮部更通透".to_string() },
        FilterPreset { id: "bluish".to_string(), name: "偏蓝".to_string(), mode: 0, temperature: 6500, brightness: 100, contrast: 100, saturation: 100, description: "冷色偏蓝调，画面更清爽".to_string() },
        FilterPreset { id: "cool-tone".to_string(), name: "原亮 冷色调".to_string(), mode: 0, temperature: 6500, brightness: 100, contrast: 100, saturation: 100, description: "保持原亮度，冷色调呈现".to_string() },
        FilterPreset { id: "delta-super".to_string(), name: "三角洲超级推荐".to_string(), mode: 0, temperature: 6500, brightness: 100, contrast: 100, saturation: 100, description: "三角洲行动超级推荐调校，压暗画面突出目标".to_string() },
        FilterPreset { id: "delta-a".to_string(), name: "三角洲推荐A".to_string(), mode: 0, temperature: 6500, brightness: 100, contrast: 100, saturation: 100, description: "三角洲行动推荐方案A，适度提亮画面".to_string() },
        FilterPreset { id: "delta-b".to_string(), name: "三角洲推荐B".to_string(), mode: 0, temperature: 6500, brightness: 100, contrast: 100, saturation: 100, description: "三角洲行动推荐方案B，高亮增强，暗处更清晰".to_string() },
        FilterPreset { id: "delta-c".to_string(), name: "三角洲推荐C".to_string(), mode: 0, temperature: 6500, brightness: 100, contrast: 100, saturation: 100, description: "三角洲行动推荐方案C，轻度提亮，观感自然".to_string() },
        FilterPreset { id: "delta-d".to_string(), name: "三角洲推荐D".to_string(), mode: 0, temperature: 6500, brightness: 100, contrast: 100, saturation: 100, description: "三角洲行动推荐方案D，压暗画面，减少眩光".to_string() },
        FilterPreset { id: "delta-e".to_string(), name: "三角洲推荐E".to_string(), mode: 0, temperature: 6500, brightness: 100, contrast: 100, saturation: 100, description: "三角洲行动推荐方案E，压暗偏冷，久玩舒适".to_string() },
        FilterPreset { id: "benq".to_string(), name: "明基(仿游戏加加)".to_string(), mode: 9, temperature: 6700, brightness: 110, contrast: 110, saturation: 140, description: "仿游戏加加明基滤镜：暗部提亮+色彩自然饱和，FPS 找人更快".to_string() },
    ])
}

/// Map a parametric preset id to its corresponding builtin ICC filename.
fn preset_id_to_builtin_icc(preset_id: &str) -> Option<String> {
    match preset_id {
        "de-exposure-pro" => Some("NexBox_去曝光Pro.icc".to_string()),
        "vivid" => Some("NexBox_鲜艳.icc".to_string()),
        "movie" => Some("NexBox_电影.icc".to_string()),
        "highlight" => Some("NexBox_高亮.icc".to_string()),
        "soft" => Some("NexBox_柔和.icc".to_string()),
        "gaming" => Some("NexBox_游戏.icc".to_string()),
        "reading" => Some("NexBox_阅读.icc".to_string()),
        "de-exposure" => Some("NexBox_去曝光.icc".to_string()),
        "shadow-boost" => Some("NexBox_暗部增强.icc".to_string()),
        "dam-contrast" => Some("NexBox_大坝降低对比度.icc".to_string()),
        "aerospace" => Some("NexBox_航天推荐.icc".to_string()),
        "whiter" => Some("NexBox_偏白.icc".to_string()),
        "bluish" => Some("NexBox_偏蓝.icc".to_string()),
        "cool-tone" => Some("NexBox_原亮 冷色调.icc".to_string()),
        "delta-super" => Some("NexBox_三角洲超级推荐.icc".to_string()),
        "delta-a" => Some("NexBox_三角洲推荐A.icc".to_string()),
        "delta-b" => Some("NexBox_三角洲推荐B.icc".to_string()),
        "delta-c" => Some("NexBox_三角洲推荐C.icc".to_string()),
        "delta-d" => Some("NexBox_三角洲推荐D.icc".to_string()),
        "delta-e" => Some("NexBox_三角洲推荐E.icc".to_string()),
        _ => None,
    }
}

// ─── 多滤镜叠加（ramp 复合）───

/// 将一组 Gamma ramp 按顺序复合为一条 ramp（LUT 复合，等价像素依次经过各滤镜曲线）。
/// 从恒等 ramp 出发，对每个滤镜 f：composed[i] = f[composed[i] / 257]（最近邻插值）。
fn compose_ramps(ramps: &[[[u16; 256]; 3]]) -> [[u16; 256]; 3] {
    let mut composed = [[0u16; 256]; 3];
    for c in 0..3 {
        for i in 0..256 { composed[c][i] = (i * 257) as u16; }
    }
    for ramp in ramps {
        let mut next = [[0u16; 256]; 3];
        for c in 0..3 {
            for i in 0..256 {
                let idx = ((composed[c][i] as usize) / 257).min(255);
                next[c][i] = ramp[c][idx];
            }
        }
        composed = next;
    }
    // 单调约束 + 端点固定（与 build_gamma_ramp 一致）
    for c in 0..3 {
        for i in 1..256 {
            if composed[c][i] < composed[c][i - 1] { composed[c][i] = composed[c][i - 1]; }
        }
    }
    composed[0][0] = 0; composed[1][0] = 0; composed[2][0] = 0;
    composed[0][255] = 65535; composed[1][255] = 65535; composed[2][255] = 65535;
    composed
}

/// 将任意滤镜 id（内置预设 / ICC 配置文件 / 我的滤镜预设）解析为 Gamma ramp。
/// 与前端卡片 id 空间一一对应；任一来源命中即返回。
fn preset_id_to_ramp(id: &str) -> Option<[[u16; 256]; 3]> {
    // 1. 内置参数化预设：优先内置 ICC 文件；解析失败或未配置 ICC 文件（如明基 benq）时回退参数生成
    if let Some(icc_filename) = preset_id_to_builtin_icc(id) {
        if let Ok(icc_path) = get_builtin_icc_path(&icc_filename) {
            if let Ok(parsed) = parse_icc_file(icc_path.to_str().unwrap_or("")) {
                return Some(parsed.to_ramp_array());
            }
            log::warn!("preset_id_to_ramp[{}]: 内置 ICC '{}' 解析失败，回退参数生成", id, icc_filename);
        }
    }
    if let Ok(presets) = get_filter_presets_sync() {
        if let Some(p) = presets.iter().find(|x| x.id == id) {
            return Some(build_gamma_ramp(
                p.temperature, p.brightness, p.contrast, p.saturation,
                FilterMode::from_i32(p.mode), None,
            ));
        }
    }
    // 1b. 内置 ICC 文件 id（形如 builtin_NexBox_*，来自单选预设/重置应用后的 active_icc_id）
    if let Some(stem) = id.strip_prefix("builtin_") {
        let icc_filename = format!("{}.icc", stem);
        if let Ok(icc_path) = get_builtin_icc_path(&icc_filename) {
            if let Ok(parsed) = parse_icc_file(icc_path.to_str().unwrap_or("")) {
                return Some(parsed.to_ramp_array());
            }
        }
    }
    // 2. ICC 配置文件（用户导入，id 由导入时生成）
    if let Some((_id, ramp)) = get_or_load_icc_presets()
        .iter()
        .find(|p| p.id == id)
        .map(|p| (p.id.clone(), p.to_ramp_array()))
    {
        return Some(ramp);
    }
    // 3. 我的滤镜预设（参数化）
    let user_presets = {
        let mut lock = USER_FILTER_PRESETS.lock().unwrap();
        if lock.is_none() {
            let loaded = load_user_filter_presets_from_file();
            *lock = Some(loaded.clone());
            loaded
        } else { lock.as_ref().unwrap().clone() }
    };
    if let Some(p) = user_presets.iter().find(|x| x.id == id) {
        return Some(build_gamma_ramp(
            p.temperature, p.brightness, p.contrast, p.saturation,
            FilterMode::Normal, Some((p.r_gamma, p.g_gamma, p.b_gamma)),
        ));
    }
    None
}

/// 同步取内置预设列表（供 ramp 解析器使用，避免在同步上下文 `await`）。
fn get_filter_presets_sync() -> Result<Vec<FilterPreset>, String> {
    Ok(vec![
        FilterPreset { id: "de-exposure-pro".to_string(), name: "去曝光Pro".to_string(), mode: 0, temperature: 6500, brightness: 100, contrast: 100, saturation: 100, description: "专业去曝光，保护高光细节".to_string() },
        FilterPreset { id: "vivid".to_string(), name: "鲜艳".to_string(), mode: 1, temperature: 6800, brightness: 102, contrast: 105, saturation: 115, description: "增强色彩饱和度，画面更鲜艳".to_string() },
        FilterPreset { id: "movie".to_string(), name: "电影".to_string(), mode: 2, temperature: 5800, brightness: 98, contrast: 95, saturation: 95, description: "电影质感，柔和色调".to_string() },
        FilterPreset { id: "highlight".to_string(), name: "高亮".to_string(), mode: 3, temperature: 7200, brightness: 110, contrast: 102, saturation: 100, description: "提高亮度，适合暗光环境".to_string() },
        FilterPreset { id: "soft".to_string(), name: "柔和".to_string(), mode: 4, temperature: 5200, brightness: 98, contrast: 92, saturation: 95, description: "柔和画面，减少眼睛疲劳".to_string() },
        FilterPreset { id: "gaming".to_string(), name: "游戏".to_string(), mode: 5, temperature: 6800, brightness: 103, contrast: 108, saturation: 110, description: "增强对比度和色彩，适合游戏".to_string() },
        FilterPreset { id: "reading".to_string(), name: "阅读".to_string(), mode: 6, temperature: 4800, brightness: 95, contrast: 100, saturation: 92, description: "暖色调，保护眼睛".to_string() },
        FilterPreset { id: "de-exposure".to_string(), name: "去曝光".to_string(), mode: 7, temperature: 6500, brightness: 92, contrast: 103, saturation: 98, description: "压暗高光，降低过度曝光，恢复高光细节".to_string() },
        FilterPreset { id: "shadow-boost".to_string(), name: "暗部增强".to_string(), mode: 8, temperature: 6500, brightness: 106, contrast: 94, saturation: 104, description: "提亮暗部阴影，让黑暗角落的敌人无处遁形".to_string() },
        FilterPreset { id: "dam-contrast".to_string(), name: "大坝降低对比度".to_string(), mode: 0, temperature: 6500, brightness: 100, contrast: 100, saturation: 100, description: "降低对比度，保护高光细节，画面更柔和".to_string() },
        FilterPreset { id: "aerospace".to_string(), name: "航天推荐".to_string(), mode: 0, temperature: 6500, brightness: 100, contrast: 100, saturation: 100, description: "航天基地专属色彩调教".to_string() },
        FilterPreset { id: "whiter".to_string(), name: "偏白".to_string(), mode: 0, temperature: 6500, brightness: 100, contrast: 100, saturation: 100, description: "整体偏白调，亮部更通透".to_string() },
        FilterPreset { id: "bluish".to_string(), name: "偏蓝".to_string(), mode: 0, temperature: 6500, brightness: 100, contrast: 100, saturation: 100, description: "冷色偏蓝调，画面更清爽".to_string() },
        FilterPreset { id: "cool-tone".to_string(), name: "原亮 冷色调".to_string(), mode: 0, temperature: 6500, brightness: 100, contrast: 100, saturation: 100, description: "保持原亮度，冷色调呈现".to_string() },
        FilterPreset { id: "delta-super".to_string(), name: "三角洲超级推荐".to_string(), mode: 0, temperature: 6500, brightness: 100, contrast: 100, saturation: 100, description: "三角洲行动超级推荐调校，压暗画面突出目标".to_string() },
        FilterPreset { id: "delta-a".to_string(), name: "三角洲推荐A".to_string(), mode: 0, temperature: 6500, brightness: 100, contrast: 100, saturation: 100, description: "三角洲行动推荐方案A，适度提亮画面".to_string() },
        FilterPreset { id: "delta-b".to_string(), name: "三角洲推荐B".to_string(), mode: 0, temperature: 6500, brightness: 100, contrast: 100, saturation: 100, description: "三角洲行动推荐方案B，高亮增强，暗处更清晰".to_string() },
        FilterPreset { id: "delta-c".to_string(), name: "三角洲推荐C".to_string(), mode: 0, temperature: 6500, brightness: 100, contrast: 100, saturation: 100, description: "三角洲行动推荐方案C，轻度提亮，观感自然".to_string() },
        FilterPreset { id: "delta-d".to_string(), name: "三角洲推荐D".to_string(), mode: 0, temperature: 6500, brightness: 100, contrast: 100, saturation: 100, description: "三角洲行动推荐方案D，压暗画面，减少眩光".to_string() },
        FilterPreset { id: "delta-e".to_string(), name: "三角洲推荐E".to_string(), mode: 0, temperature: 6500, brightness: 100, contrast: 100, saturation: 100, description: "三角洲行动推荐方案E，压暗偏冷，久玩舒适".to_string() },
        FilterPreset { id: "benq".to_string(), name: "明基(仿游戏加加)".to_string(), mode: 9, temperature: 6700, brightness: 110, contrast: 110, saturation: 140, description: "仿游戏加加明基滤镜：暗部提亮+色彩自然饱和，FPS 找人更快".to_string() },
    ])
}

/// 应用叠加组合的协调层实现见 [`DisplayOps::apply_stack`]（要求调用方已持有操作锁）。
/// Clear the gamma ramp via xcalib (reset to system default / linear).
/// 仅在无法恢复捕获的原始 ramp 时作为兜底；系统关机/注销时跳过（外部子进程会
/// 因运行库被拆除而初始化失败 0xc0000142）。
fn clear_gamma_ramp_via_xcalib(display_index: usize) -> Result<(), String> {
    if is_system_shutting_down() {
        // 系统关机/注销：外部子进程会因运行库被拆除而初始化失败 0xc0000142。
        // 不得假装“清除成功”：
        // - restore() 会保留 restore_pending（Err 分支不清标记）；
        // - 调用方/cleanup 会把该显示器计入失败，而不是 restored/degraded。
        log::warn!("clear_gamma_ramp[{}]: 系统关机/注销中，跳过 xcalib 恢复（未执行清除）", display_index);
        return Err("系统关机/注销中，已跳过 xcalib 清除，未执行清除".to_string());
    }
    let tool = get_tool_path("xcalib.exe")?;
    log::info!("clear_gamma_ramp[{}]: resetting via xcalib -c", display_index);

    let mut cmd = Command::new(&tool);
    cmd.arg("-screen").arg(display_index.to_string());
    cmd.arg("-c");

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let output = cmd.output()
        .map_err(|e| format!("xcalib reset 失败: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::error!("xcalib reset 失败: {}", stderr);
        return Err(format!("xcalib reset 失败: {}", stderr));
    }
    log::info!("xcalib reset 成功");
    Ok(())
}

// ─── Original gamma ramp capture/restore（进程内 API，避免 xcalib -c 抹掉 sRGB 校色）───

// windows-sys 0.59 / windows 0.58 未导出 GetDeviceGammaRamp / SetDeviceGammaRamp，
// 这里直接声明 gdi32 导出，无需新增依赖。
#[cfg(target_os = "windows")]
#[link(name = "gdi32")]
extern "system" {
    fn GetDeviceGammaRamp(hdc: *mut core::ffi::c_void, lp_ramp: *mut u16) -> i32;
    fn SetDeviceGammaRamp(hdc: *mut core::ffi::c_void, lp_ramp: *const u16) -> i32;
}

/// 根据显示器 index 取 GDI 设备名（如 \\.\DISPLAY1）创建屏幕 DC。
/// 与 get_gdi_device_resolution 一致地处理有无 `\\\\.\\` 前缀两种情况。
fn get_display_hdc(idx: usize) -> Option<windows_sys::Win32::Graphics::Gdi::HDC> {
    use std::ptr;
    use windows_sys::Win32::Graphics::Gdi::CreateDCW;

    let device_names = DISPLAY_DEVICES.lock().unwrap();
    let device_name = device_names.as_ref()?.get(idx)?;
    let trimmed = device_name.trim_start_matches("\\\\.\\");
    for name in [trimmed, device_name.as_str()] {
        if name.is_empty() { continue; }
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let dc = unsafe { CreateDCW(ptr::null(), wide.as_ptr(), ptr::null(), ptr::null()) };
        if !dc.is_null() { return Some(dc); }
    }
    None
}

/// 读取当前硬件 gamma ramp（读不到返回 None）。
fn read_gamma_ramp(idx: usize) -> Option<[[u16; 256]; 3]> {
    use windows_sys::Win32::Graphics::Gdi::DeleteDC;
    let dc = get_display_hdc(idx)?;
    let mut ramp = [[0u16; 256]; 3];
    let ok = unsafe { GetDeviceGammaRamp(dc, ramp.as_mut_ptr() as *mut u16) };
    unsafe { let _ = DeleteDC(dc); }
    if ok != 0 { Some(ramp) } else { None }
}

/// 写入 gamma ramp（SetDeviceGammaRamp，进程内 API，不产生子进程，退出/关机时也安全）。
fn write_gamma_ramp(idx: usize, ramp: &[[u16; 256]; 3]) -> Result<(), String> {
    use windows_sys::Win32::Graphics::Gdi::DeleteDC;
    let dc = get_display_hdc(idx)
        .ok_or_else(|| format!("write_gamma_ramp[{}]: 无法获取显示器 DC", idx))?;
    let ok = unsafe { SetDeviceGammaRamp(dc, ramp.as_ptr() as *const u16) };
    unsafe { let _ = DeleteDC(dc); }
    if ok != 0 {
        log::info!("write_gamma_ramp[{}]: 原始 ramp 恢复成功", idx);
        Ok(())
    } else {
        Err(format!("write_gamma_ramp[{}]: SetDeviceGammaRamp 失败", idx))
    }
}

// 捕获 / 原始 ramp / 待恢复标记的协调层实现见 [`DisplayOps`]（capture / peek_ramp /
// clear_ramp / set_restore_pending），要求调用方持有该显示器操作锁时再读取判断。

#[tauri::command]
pub async fn apply_preset(
    display_index: Option<usize>, preset_id: String, is_active: bool,
) -> Result<FilterResult, String> {
    ensure_not_shutting_down()?;
    let idx = resolve_display_index(display_index);
    let presets = get_filter_presets().await?;
    let preset = presets.iter().find(|p| p.id == preset_id)
        .ok_or_else(|| format!("未找到预设: {}", preset_id))?;

    // Parametric presets → try to apply the matching builtin ICC file directly.
    if let Some(icc_filename) = preset_id_to_builtin_icc(&preset_id) {
        if let Ok(icc_path) = get_builtin_icc_path(&icc_filename) {
            log::info!(
                "apply_preset[{}]: preset '{}' → builtin ICC '{}'",
                idx, preset_id, icc_path.display()
            );

            // Parse the ICC to get ramp for CSS preview
            let ramp_array = match parse_icc_file(icc_path.to_str().unwrap_or("")) {
                Ok(parsed) => parsed.to_ramp_array(),
                Err(_) => [[0u16; 256]; 3],
            };

            let (actually_active, operation_generation) = with_display_state(idx, |state| {
                state.temperature = preset.temperature;
                state.brightness = preset.brightness;
                state.contrast = preset.contrast;
                state.saturation = preset.saturation;
                state.r_gamma = 1.0;
                state.g_gamma = 1.0;
                state.b_gamma = 1.0;
                state.mode = preset.mode;
                state.icc_ramp = Some(ramp_array);
                state.icc_active = true;
                state.active_icc_id = Some(format!("builtin_{}", icc_filename.trim_end_matches(".icc")));
                state.stacked = false;
                state.stack_preset_ids.clear();
                if is_active && !state.filter_active { state.filter_active = true; }
                let generation = if state.filter_active {
                    bump_operation_generation(state)
                } else {
                    state.operation_generation
                };
                (state.filter_active, generation)
            });
            if actually_active {
                let icc_path_clone = icc_path.clone();
                let idx_move = idx;
                // 不阻塞返回：在后台线程应用 ICC，避免切换预设时因等待 xcalib
                // 应用 gamma（显示器会短暂刷新）而导致 UI“卡一下”。
                // 但用户可能在后台应用完成前关闭滤镜；再次确认开关仍为开启，
                // 避免“已关闭但后台任务随后又把该滤镜写回屏幕”的竞态。
                tauri::async_runtime::spawn(async move {
                    // 捕获失败会以内层 Err 返回，必须显式提取，避免被静默吞掉。
                    // 捕获后版本复查发现过期：以 SkippedStale 传播（不登记成功、不报完成）。
                    let task_result = tauri::async_runtime::spawn_blocking(move || {
                        let ops = global_ops();
                        ops.run_if_current(idx_move, operation_generation, "apply_preset", false, || {
                            // 捕获完成后复查版本：捕获期间可能被新的关闭意图推进版本，
                            // 过期则中止写屏（避免“已关闭但滤镜随后被写回屏幕”）。
                            if ops.capture_if_current(idx_move, operation_generation, "apply_preset")? == CaptureOutcome::Skip {
                                log::info!("apply_preset[{}]: 捕获后版本已过期/退出，中止 ICC 写屏", idx_move);
                                return Ok(RunResult::SkippedStale);
                            }
                            ops.apply_icc(idx_move, &icc_path_clone).map(RunResult::Executed)
                        })
                    }).await;
                    match task_result {
                        Ok(Ok(RunResult::SkippedStale))
                        | Ok(Ok(RunResult::Executed(RunResult::SkippedStale))) => {
                            log::info!("apply_preset[{}]: 应用已过期，未执行写屏", idx_move);
                        }
                        Ok(Ok(RunResult::Executed(RunResult::Executed(())))) => {}
                        Ok(Err(e)) => log::error!("apply_preset[{}]: 后台应用 ICC 失败: {}", idx_move, e),
                        Err(join_err) => log::error!("apply_preset[{}]: 后台任务 join 失败: {}", idx_move, join_err),
                    }
                });
            }

            let (preview_filter, preview_tint_color, preview_tint_opacity) = compute_icc_preview(&ramp_array);

            save_all_filter_states();

            return Ok(with_display_state(idx, |state| FilterResult {
                success: true,
                skipped_stale: false,
                degraded: false,
                message: format!("已应用预设: {}", preset.name),
                settings: Some(FilterSettings::from_display_state(state)),
                preview_filter: if preview_filter.is_empty() { None } else { Some(preview_filter) },
                preview_tint_color, preview_tint_opacity,
            }));
        }
        // ICC file not found → fall through to parameter-based generation
        log::warn!("apply_preset[{}]: builtin ICC '{}' not found, falling back to icc_gen", idx, icc_filename);
    }

    // Fallback: generate ICC from parameters (legacy / custom behavior)
    set_filter_settings(display_index, preset.temperature, preset.brightness, preset.contrast, preset.saturation, preset.mode, is_active, None, None, None).await
}

/// 应用多滤镜叠加组合。`preset_ids` 按点选顺序排列（首个作用在输入层，末个作用在输出层）。
/// - 非空：校验每个滤镜可解析 → 复合 ramp → 落屏（filter_active=true）并持久化。
/// - 空：若此前处于叠加模式，则清除叠加并恢复默认显示；否则 no-op。
#[tauri::command]
pub async fn apply_filter_stack(
    display_index: Option<usize>,
    preset_ids: Vec<String>,
) -> Result<FilterResult, String> {
    #[cfg(target_os = "windows")]
    {
        ensure_not_shutting_down()?;
        let idx = resolve_display_index(display_index);

        if preset_ids.is_empty() {
            let was_stacked = with_display_state(idx, |s| s.stacked);
            if was_stacked {
                let operation_generation = with_display_state(idx, |s| {
                    s.stacked = false;
                    s.stack_preset_ids.clear();
                    s.icc_active = false;
                    s.active_icc_id = None;
                    s.icc_ramp = None;
                    s.filter_active = false;
                    bump_operation_generation(s)
                });
                let idx_move = idx;
                let outcome = tauri::async_runtime::spawn_blocking(move || restore_display_default_if_current(idx_move, operation_generation))
                    .await.map_err(|e| format!("清除叠加失败: {}", e))?;
                let (message, degraded, skipped_stale) = match outcome {
                    Ok(RunResult::Executed(RestoreOutcome::Restored)) => ("叠加滤镜已清除".to_string(), false, false),
                    Ok(RunResult::Executed(RestoreOutcome::DegradedCleared)) => {
                        log::warn!("apply_filter_stack[{}]: 降级清除成功，原有校色未保证恢复", idx);
                        ("叠加滤镜已清除（降级清除：未能恢复原有校色）".to_string(), true, false)
                    }
                    Ok(RunResult::Executed(RestoreOutcome::NothingToDo)) => ("叠加滤镜已清除".to_string(), false, false),
                    Ok(RunResult::SkippedStale) => {
                        log::info!("apply_filter_stack[{}]: 清除任务已过期，跳过", idx);
                        ("操作已被更新的操作取代，未执行".to_string(), false, true)
                    }
                    Err(e) => return Err(format!("清除叠加滤镜恢复失败: {}", e)),
                };
                save_all_filter_states();
                return Ok(with_display_state(idx, |state| FilterResult {
                    success: true, message, degraded, skipped_stale,
                    settings: Some(FilterSettings::from_display_state(state)),
                    preview_filter: None, preview_tint_color: None, preview_tint_opacity: None,
                }));
            }
            save_all_filter_states();
            return Ok(with_display_state(idx, |state| FilterResult {
                success: true, skipped_stale: false, degraded: false,
                message: "叠加滤镜已清除".to_string(),
                settings: Some(FilterSettings::from_display_state(state)),
                preview_filter: None, preview_tint_color: None, preview_tint_opacity: None,
            }));
        }

        // 全校验：任一滤镜解析失败则整体报错，不污染状态
        let mut ramps = Vec::with_capacity(preset_ids.len());
        for id in &preset_ids {
            match preset_id_to_ramp(id) {
                Some(ramp) => ramps.push(ramp),
                None => return Err(format!("找不到滤镜: {}", id)),
            }
        }
        let composed = compose_ramps(&ramps);

        let operation_generation = with_display_state(idx, |state| {
            state.stack_preset_ids = preset_ids.clone();
            state.stacked = true;
            state.icc_ramp = Some(composed);
            state.icc_active = false;
            state.active_icc_id = None;
            state.filter_active = true;
            bump_operation_generation(state)
        });

        // 后台应用，避免等待 xcalib 导致 UI 卡顿
        let idx_move = idx;
        tauri::async_runtime::spawn(async move {
            let task_result = tauri::async_runtime::spawn_blocking(move || {
                let ops = global_ops();
                ops.run_if_current(idx_move, operation_generation, "apply_filter_stack", false, || {
                    ops.apply_stack(idx_move, operation_generation, "apply_filter_stack")
                })
            }).await;
            match task_result {
                Ok(Ok(RunResult::SkippedStale))
                | Ok(Ok(RunResult::Executed(RunResult::SkippedStale))) => {
                    log::info!("apply_filter_stack[{}]: 应用已过期，未执行写屏", idx_move);
                }
                Ok(Ok(RunResult::Executed(RunResult::Executed(())))) => {}
                Ok(Err(e)) => log::error!("apply_filter_stack[{}]: 后台应用叠加滤镜失败: {}", idx_move, e),
                Err(join_err) => log::error!("apply_filter_stack[{}]: 后台任务 join 失败: {}", idx_move, join_err),
            }
        });

        save_all_filter_states();

        Ok(with_display_state(idx, |state| FilterResult {
            success: true,
            skipped_stale: false,
            degraded: false,
            message: format!("已应用 {} 个叠加滤镜", preset_ids.len()),
            settings: Some(FilterSettings::from_display_state(state)),
            preview_filter: None, preview_tint_color: None, preview_tint_opacity: None,
        }))
    }
    #[cfg(not(target_os = "windows"))]
    { Err("此功能仅支持 Windows 系统".to_string()) }
}

// ─── System session watch (shutdown/logoff detection) ───

/// 读取系统关机/注销标志。返回 true 表示 Windows 正在结束会话（关机/注销）。
pub(crate) fn is_system_shutting_down() -> bool {
    SYSTEM_SHUTTING_DOWN.load(Ordering::SeqCst)
}

/// 初始化会话监控隐藏窗口，用于捕获系统关机/注销广播（WM_QUERYENDSESSION / WM_ENDSESSION），
/// 提前置位 SYSTEM_SHUTTING_DOWN。必须在主线程（Tauri setup）调用，主消息循环才能派发广播到该窗口。
#[cfg(target_os = "windows")]
pub fn init_session_watch() {
    use windows_sys::core::w;
    use windows_sys::Win32::Foundation::{GetLastError, HWND};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, RegisterClassW, WNDCLASSW, WS_POPUP,
    };

    unsafe {
        let h_instance = GetModuleHandleW(std::ptr::null());
        if h_instance.is_null() {
            log::warn!("init_session_watch: 获取模块句柄失败");
            return;
        }

        let class_name = w!("NexBoxSessionWatch");
        let wnd_class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(session_watch_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: h_instance,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name,
        };

        if RegisterClassW(&wnd_class) == 0 && GetLastError() != 1410 {
            log::warn!("init_session_watch: 注册窗口类失败: {}", GetLastError());
            return;
        }

        let hwnd: HWND = CreateWindowExW(
            0,
            class_name,
            w!("NexBox Session Watch"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            h_instance,
            std::ptr::null_mut(),
        );

        if hwnd.is_null() {
            log::warn!("init_session_watch: 创建窗口失败: {}", GetLastError());
            return;
        }

        SESSION_WATCH_HWND.store(hwnd, Ordering::SeqCst);
        log::info!("init_session_watch: 会话监控窗口已就绪, hwnd={:?}", hwnd);
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn session_watch_proc(
    hwnd: windows_sys::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DefWindowProcW, WM_ENDSESSION, WM_QUERYENDSESSION,
    };

    match msg {
        WM_QUERYENDSESSION | WM_ENDSESSION => {
            if !SYSTEM_SHUTTING_DOWN.load(Ordering::SeqCst) {
                SYSTEM_SHUTTING_DOWN.store(true, Ordering::SeqCst);
                log::info!("会话结束消息(0x{:X})：标记系统关机/注销，跳过退出时的 xcalib 恢复", msg);
            }
            // WM_QUERYENDSESSION 返回 TRUE(1) 允许结束会话；WM_ENDSESSION 走默认处理
            if msg == WM_QUERYENDSESSION {
                return 1;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

pub fn cleanup() {
    #[cfg(target_os = "windows")]
    {
        // 委托协调层：在同一状态锁内置位退出标志 + 使全部旧任务失效，再逐台取操作锁
        // 在锁内判断并执行恢复。退出期间新意图会在 submit 的状态锁内被拒绝。
        global_ops().cleanup();
    }
}

// ─── Startup restore ───

/// Called once on app startup to restore the filter state from the previous session.
/// If `auto_apply` is true the saved preset/ICC is applied immediately (toggle ON,
/// ICC applied to display).  Otherwise only the in-memory state is restored and the
/// frontend highlights the correct card with the toggle OFF.
#[tauri::command]
pub async fn restore_filter_state(display_index: Option<usize>, auto_apply: bool) -> Result<FilterResult, String> {
    ensure_not_shutting_down()?;
    let idx = resolve_display_index(display_index);
    log::info!("restore_filter_state[{}]: auto_apply={}", idx, auto_apply);

    // Load saved state into memory (filter_active forced to false for now)
    restore_state_on_startup();

    if auto_apply {
        // Turn the filter ON and re-apply the saved preset/ICC
        let operation_generation = with_display_state(idx, |s| {
            s.filter_active = true;
            bump_operation_generation(s)
        });
        log::info!("restore_filter_state[{}]: auto-apply enabled, re-applying filter", idx);
        let idx_move = idx;
        let outcome = tauri::async_runtime::spawn_blocking(move || apply_filter_to_display_if_current(idx_move, operation_generation))
            .await.map_err(|e| format!("Startup filter apply error: {}", e))?;
        let (message, skipped_stale) = match outcome {
            Ok(RunResult::Executed(())) => {
                ("滤镜已自动开启".to_string(), false)
            }
            Ok(RunResult::SkippedStale) => {
                log::info!("restore_filter_state[{}]: 启动应用已过期，未执行写屏", idx);
                ("操作已被更新的操作取代，未执行".to_string(), true)
            }
            Err(e) => {
                // 统一条件回滚：状态锁内原子核验版本，仍当前才关闭并恢复默认显示。
                match rollback_failed_apply(idx, operation_generation) {
                    RollbackOutcome::Restored => {
                        log::warn!("restore_filter_state[{}]: 应用失败，已精确恢复默认显示: {}", idx, e);
                    }
                    RollbackOutcome::DegradedCleared => {
                        log::warn!("restore_filter_state[{}]: 应用失败，已降级清除滤镜: {}", idx, e);
                    }
                    RollbackOutcome::NoRestoreNeeded => {}
                    RollbackOutcome::Superseded => {
                        log::info!("restore_filter_state[{}]: 应用失败，恢复意图已被更新的操作取代（未执行）: {}", idx, e);
                    }
                    RollbackOutcome::RestoreFailed(re) => {
                        log::error!("restore_filter_state[{}]: 应用失败且回滚恢复也失败（保留待恢复标记）: {}; {}", idx, e, re);
                    }
                }
                return Err(format!("滤镜应用失败: {}", e));
            }
        };
        let skipped_stale_result = skipped_stale;
        return Ok(with_display_state(idx, |state| FilterResult {
            success: true,
            message,
            skipped_stale: skipped_stale_result,
            degraded: false,
            settings: Some(FilterSettings::from_display_state(state)),
            preview_filter: None, preview_tint_color: None, preview_tint_opacity: None,
        }));
    }

    Ok(with_display_state(idx, |state| FilterResult {
        success: true,
        skipped_stale: false,
        degraded: false,
        message: "滤镜状态已恢复".to_string(),
        settings: Some(FilterSettings::from_display_state(state)),
        preview_filter: None, preview_tint_color: None, preview_tint_opacity: None,
    }))
}

// ─── Custom filter settings commands ───

#[tauri::command]
pub async fn get_custom_filter_settings(display_index: Option<usize>) -> Result<CustomFilterSettings, String> {
    let idx = resolve_display_index(display_index);
    Ok(get_or_load_custom_settings().get(&idx).cloned().unwrap_or_default())
}

#[tauri::command]
pub async fn save_custom_filter_settings(
    display_index: Option<usize>, temperature: i32, brightness: i32,
    contrast: i32, saturation: i32,
    r_gamma: Option<f64>, g_gamma: Option<f64>, b_gamma: Option<f64>,
) -> Result<CustomFilterSettings, String> {
    let idx = resolve_display_index(display_index);
    let settings = CustomFilterSettings {
        temperature: temperature.clamp(1000, 10000), brightness: brightness.clamp(50, 150),
        contrast: contrast.clamp(50, 150), saturation: saturation.clamp(50, 150),
        r_gamma: r_gamma.unwrap_or(1.0).clamp(0.50, 2.00),
        g_gamma: g_gamma.unwrap_or(1.0).clamp(0.50, 2.00),
        b_gamma: b_gamma.unwrap_or(1.0).clamp(0.50, 2.00),
    };
    let mut all_settings = get_or_load_custom_settings();
    all_settings.insert(idx, settings.clone());
    save_custom_settings_to_file(&all_settings)?;
    *CUSTOM_SETTINGS.lock().unwrap() = Some(all_settings);
    Ok(settings)
}

#[tauri::command]
pub async fn export_custom_filter(display_index: Option<usize>) -> Result<Option<String>, String> {
    #[cfg(target_os = "windows")]
    {
        let idx = resolve_display_index(display_index);
        let settings = get_or_load_custom_settings().get(&idx).cloned().unwrap_or_default();

        let ramp = build_gamma_ramp(settings.temperature, settings.brightness, settings.contrast, settings.saturation, FilterMode::Normal, Some((settings.r_gamma, settings.g_gamma, settings.b_gamma)));
        let default_name = "NexBox_Custom.icc";
        let result = rfd::FileDialog::new().set_title("导出自定义滤镜为 ICC").add_filter("ICC 文件", &["icc", "icm"]).set_file_name(default_name).save_file();
        let path = match result { Some(p) => p, None => return Ok(None) };
        let icc_data = build_icc_profile(&ramp, "NexBox Custom Filter");
        fs::write(&path, &icc_data).map_err(|e| format!("无法保存文件: {}", e))?;
        log::info!("Custom ICC exported: {} ({} bytes)", path.display(), icc_data.len());
        Ok(path.to_str().map(|s| s.to_string()))
    }
    #[cfg(not(target_os = "windows"))]
    { Err("此功能仅支持 Windows 系统".to_string()) }
}

// ─── User filter preset commands ───

#[tauri::command]
pub async fn get_user_filter_presets() -> Result<Vec<UserFilterPresetInfo>, String> {
    let presets = {
        let mut lock = USER_FILTER_PRESETS.lock().unwrap();
        if lock.is_none() {
            let loaded = load_user_filter_presets_from_file();
            *lock = Some(loaded.clone());
            loaded
        } else { lock.as_ref().unwrap().clone() }
    };
    Ok(presets.iter().map(|p| UserFilterPresetInfo {
        id: p.id.clone(), name: p.name.clone(),
        temperature: p.temperature, brightness: p.brightness, contrast: p.contrast, saturation: p.saturation,
        r_gamma: p.r_gamma, g_gamma: p.g_gamma, b_gamma: p.b_gamma,
    }).collect())
}

#[tauri::command]
pub async fn save_user_filter_preset(
    id: Option<String>, name: String,
    temperature: i32, brightness: i32, contrast: i32, saturation: i32,
    r_gamma: Option<f64>, g_gamma: Option<f64>, b_gamma: Option<f64>,
) -> Result<UserFilterPresetInfo, String> {
    let mut presets = {
        let mut lock = USER_FILTER_PRESETS.lock().unwrap();
        if lock.is_none() { let loaded = load_user_filter_presets_from_file(); *lock = Some(loaded.clone()); loaded } else { lock.as_ref().unwrap().clone() }
    };

    let new_id = id.unwrap_or_else(|| format!("preset_{}", chrono::Utc::now().timestamp()));
    let r_gamma = r_gamma.unwrap_or(1.0);
    let g_gamma = g_gamma.unwrap_or(1.0);
    let b_gamma = b_gamma.unwrap_or(1.0);

    let preset = UserFilterPreset {
        id: new_id.clone(), name: name.clone(),
        temperature: temperature.clamp(1000, 10000), brightness: brightness.clamp(50, 150),
        contrast: contrast.clamp(50, 150), saturation: saturation.clamp(50, 150),
        r_gamma: r_gamma.clamp(0.50, 2.00), g_gamma: g_gamma.clamp(0.50, 2.00), b_gamma: b_gamma.clamp(0.50, 2.00),
    };

    if let Some(existing) = presets.iter_mut().find(|p| p.id == new_id) { *existing = preset.clone(); }
    else { presets.push(preset.clone()); }

    save_user_filter_presets_to_file(&presets)?;
    *USER_FILTER_PRESETS.lock().unwrap() = Some(presets.clone());

    Ok(UserFilterPresetInfo {
        id: new_id, name, temperature: preset.temperature, brightness: preset.brightness,
        contrast: preset.contrast, saturation: preset.saturation,
        r_gamma: preset.r_gamma, g_gamma: preset.g_gamma, b_gamma: preset.b_gamma,
    })
}

#[tauri::command]
pub async fn apply_user_filter_preset(
    display_index: Option<usize>, id: String, is_active: bool,
) -> Result<FilterResult, String> {
    let presets = {
        let mut lock = USER_FILTER_PRESETS.lock().unwrap();
        if lock.is_none() { let loaded = load_user_filter_presets_from_file(); *lock = Some(loaded.clone()); loaded } else { lock.as_ref().unwrap().clone() }
    };
    let preset = presets.iter().find(|p| p.id == id).ok_or("未找到自定义预设".to_string())?;

    set_filter_settings(
        display_index, preset.temperature, preset.brightness, preset.contrast, preset.saturation,
        0, is_active, Some(preset.r_gamma), Some(preset.g_gamma), Some(preset.b_gamma),
    ).await
}

#[tauri::command]
pub async fn delete_user_filter_preset(id: String) -> Result<(), String> {
    let mut presets = {
        let mut lock = USER_FILTER_PRESETS.lock().unwrap();
        if lock.is_some() { lock.take().unwrap() } else { load_user_filter_presets_from_file() }
    };
    let len_before = presets.len();
    presets.retain(|p| p.id != id);
    if presets.len() == len_before { *USER_FILTER_PRESETS.lock().unwrap() = Some(presets); return Err("未找到要删除的自定义滤镜预设".to_string()); }
    save_user_filter_presets_to_file(&presets)?;
    *USER_FILTER_PRESETS.lock().unwrap() = Some(presets);
    Ok(())
}

// ─── ICC profile commands ───

#[tauri::command]
pub async fn select_icc_file() -> Result<Option<String>, String> {
    #[cfg(target_os = "windows")]
    {
        let result = rfd::FileDialog::new()
            .set_title("选择 ICC 色彩配置文件")
            .add_filter("ICC 文件", &["icc", "icm"])
            .pick_file();
        Ok(result.and_then(|p| p.to_str().map(|s| s.to_string())))
    }
    #[cfg(not(target_os = "windows"))]
    { Err("此功能仅支持 Windows 系统".to_string()) }
}

#[tauri::command]
pub async fn import_icc_profile(path: String) -> Result<IccImportResult, String> {
    #[cfg(target_os = "windows")]
    {
        let preset = parse_icc_file(&path)?;
        let info = IccPresetInfo { id: preset.id.clone(), name: preset.name.clone(), description: preset.description.clone() };
        let mut presets = { let mut lock = ICC_PRESETS.lock().unwrap(); if lock.is_some() { lock.take().unwrap() } else { load_icc_presets_from_file() } };
        presets.push(preset);
        save_icc_presets_to_file(&presets)?;
        *ICC_PRESETS.lock().unwrap() = Some(presets);
        Ok(IccImportResult { success: true, message: "ICC 文件已导入".to_string(), preset: Some(info) })
    }
    #[cfg(not(target_os = "windows"))]
    { Err("此功能仅支持 Windows 系统".to_string()) }
}

#[tauri::command]
pub async fn get_icc_presets() -> Result<Vec<IccPresetInfo>, String> {
    #[cfg(target_os = "windows")]
    {
        // Combine builtin ICC presets + user-imported ICC presets
        let mut result = load_builtin_icc_preset_infos();

        let user_presets = get_or_load_icc_presets();
        for p in &user_presets {
            result.push(IccPresetInfo { id: p.id.clone(), name: p.name.clone(), description: p.description.clone() });
        }
        Ok(result)
    }
    #[cfg(not(target_os = "windows"))]
    { Ok(Vec::new()) }
}

#[tauri::command]
pub async fn apply_icc_preset(
    display_index: Option<usize>, id: String, is_active: bool,
) -> Result<FilterResult, String> {
    #[cfg(target_os = "windows")]
    {
        ensure_not_shutting_down()?;
        let idx = resolve_display_index(display_index);

        // Determine the ICC file path
        let icc_path: PathBuf = if id.starts_with("builtin_") {
            // Builtin ICC preset from resources
            let filename = get_builtin_icc_filename(&id)
                .ok_or_else(|| format!("无法解析内置 ICC 预设 ID: {}", id))?;
            get_builtin_icc_path(&filename)?
        } else {
            // User-imported ICC preset: find in icc_presets.json, write to temp file
            let presets = get_or_load_icc_presets();
            let preset = presets.iter().find(|p| p.id == id).ok_or("未找到 ICC 预设".to_string())?;
            let temp_path = get_temp_icc_path(idx, "icc_preset");
            let ramp = preset.to_ramp_array();
            let icc_data = build_icc_profile(&ramp, &preset.name);
            fs::write(&temp_path, &icc_data).map_err(|e| format!("无法写入 ICC 文件: {}", e))?;
            temp_path
        };

        log::info!("apply_icc_preset[{}]: applying ICC '{}' via xcalib", idx, icc_path.display());

        // Store ICC ramp in state for CSS preview
        let ramp_array = if id.starts_with("builtin_") {
            // Parse the builtin ICC file to get the ramp for preview
            match parse_icc_file(icc_path.to_str().unwrap_or("")) {
                Ok(preset) => preset.to_ramp_array(),
                Err(_) => [[0u16; 256]; 3], // Fallback to identity
            }
        } else {
            // For user presets, get ramp from stored data
            let presets = get_or_load_icc_presets();
            presets.iter().find(|p| p.id == id)
                .map(|p| p.to_ramp_array())
                .unwrap_or([[0u16; 256]; 3])
        };

        let (actually_active, operation_generation) = with_display_state(idx, |state| {
            state.icc_ramp = Some(ramp_array);
            state.icc_active = true;
            state.active_icc_id = Some(id.clone());
            state.stacked = false;
            state.stack_preset_ids.clear();
            if is_active && !state.filter_active { state.filter_active = true; }
            let generation = if state.filter_active {
                bump_operation_generation(state)
            } else {
                state.operation_generation
            };
            (state.filter_active, generation)
        });
        if actually_active {
            let icc_path_clone = icc_path.clone();
            let idx_move = idx;
            let generated_temp_icc = !id.starts_with("builtin_");
            // 不阻塞返回：后台应用 ICC，避免切换 ICC 预设时 UI 卡顿
            tauri::async_runtime::spawn(async move {
                let path_for_cleanup = icc_path_clone.clone();
                let task_result = tauri::async_runtime::spawn_blocking(move || {
                    let ops = global_ops();
                    ops.run_if_current(idx_move, operation_generation, "apply_icc_preset", false, || {
                        // 捕获失败会以内层 Err 返回，不得静默吞掉。
                        // 捕获完成后复查版本：捕获期间可能被新的关闭意图推进版本，过期则中止写屏。
                        if ops.capture_if_current(idx_move, operation_generation, "apply_icc_preset")? == CaptureOutcome::Skip {
                            log::info!("apply_icc_preset[{}]: 捕获后版本已过期/退出，中止 ICC 写屏", idx_move);
                            return Ok(RunResult::SkippedStale);
                        }
                        if generated_temp_icc {
                            ops.apply_generated_icc(idx_move, &icc_path_clone).map(RunResult::Executed)
                        } else {
                            ops.apply_icc(idx_move, &icc_path_clone).map(RunResult::Executed)
                        }
                    })
                }).await;
                if generated_temp_icc {
                    let _ = fs::remove_file(&path_for_cleanup);
                }
                match task_result {
                    Ok(Ok(RunResult::SkippedStale))
                    | Ok(Ok(RunResult::Executed(RunResult::SkippedStale))) => {
                        log::info!("apply_icc_preset[{}]: 应用已过期，未执行写屏", idx_move);
                    }
                    Ok(Ok(RunResult::Executed(RunResult::Executed(())))) => {}
                    Ok(Err(e)) => log::error!("apply_icc_preset[{}]: 后台应用 ICC 失败: {}", idx_move, e),
                    Err(join_err) => log::error!("apply_icc_preset[{}]: 后台任务 join 失败: {}", idx_move, join_err),
                }
            });
        }

        let (preview_filter, preview_tint_color, preview_tint_opacity) = compute_icc_preview(&ramp_array);

        save_all_filter_states();

        Ok(with_display_state(idx, |state| FilterResult {
            success: true, message: format!("ICC 预设已应用"),
            skipped_stale: false,
            degraded: false,
            settings: Some(FilterSettings::from_display_state(state)),
            preview_filter: if preview_filter.is_empty() { None } else { Some(preview_filter) },
            preview_tint_color, preview_tint_opacity,
        }))
    }
    #[cfg(not(target_os = "windows"))]
    { Err("此功能仅支持 Windows 系统".to_string()) }
}

#[tauri::command]
pub async fn delete_icc_preset(id: String) -> Result<FilterResult, String> {
    #[cfg(target_os = "windows")]
    {
        // Only user-imported ICC presets can be deleted (not builtin)
        if id.starts_with("builtin_") {
            return Err("内置 ICC 预设不可删除".to_string());
        }

        let mut presets = { let mut lock = ICC_PRESETS.lock().unwrap(); if lock.is_some() { lock.take().unwrap() } else { load_icc_presets_from_file() } };
        let len_before = presets.len();
        presets.retain(|p| p.id != id);
        if presets.len() == len_before { *ICC_PRESETS.lock().unwrap() = Some(presets); return Err("未找到要删除的 ICC 预设".to_string()); }
        save_icc_presets_to_file(&presets)?;
        *ICC_PRESETS.lock().unwrap() = Some(presets);

        Ok(FilterResult { success: true, skipped_stale: false, degraded: false, message: "ICC 预设已删除".to_string(), settings: None, preview_filter: None, preview_tint_color: None, preview_tint_opacity: None })
    }
    #[cfg(not(target_os = "windows"))]
    { Err("此功能仅支持 Windows 系统".to_string()) }
}

// ─── ICC Profile Export ───

#[tauri::command]
pub async fn export_preset_as_icc(preset_id: String) -> Result<Option<String>, String> {
    #[cfg(target_os = "windows")]
    {
        // If builtin ICC preset, just copy the file
        if preset_id.starts_with("builtin_") {
            let filename = get_builtin_icc_filename(&preset_id)
                .ok_or_else(|| format!("无法解析内置 ICC 预设 ID: {}", preset_id))?;
            let src_path = get_builtin_icc_path(&filename)?;

            let default_name = filename.clone();
            let result = rfd::FileDialog::new()
                .set_title("保存 ICC 色彩配置文件")
                .add_filter("ICC 文件", &["icc", "icm"])
                .set_file_name(&default_name)
                .save_file();
            let path = match result { Some(p) => p, None => return Ok(None) };
            fs::copy(&src_path, &path).map_err(|e| format!("无法复制文件: {}", e))?;
            log::info!("Builtin ICC exported: {} -> {}", src_path.display(), path.display());
            return Ok(path.to_str().map(|s| s.to_string()));
        }

        // 若预设映射了内置 ICC 文件（如去曝光Pro参数为中性值，参数化导出无效），
        // 直接复制内置文件导出，与应用时的效果保持一致。
        if let Some(icc_filename) = preset_id_to_builtin_icc(&preset_id) {
            let src_path = get_builtin_icc_path(&icc_filename)?;

            let default_name = icc_filename.clone();
            let result = rfd::FileDialog::new()
                .set_title("保存 ICC 色彩配置文件")
                .add_filter("ICC 文件", &["icc", "icm"])
                .set_file_name(&default_name)
                .save_file();
            let path = match result { Some(p) => p, None => return Ok(None) };
            fs::copy(&src_path, &path).map_err(|e| format!("无法复制文件: {}", e))?;
            log::info!("Builtin ICC exported: {} -> {}", src_path.display(), path.display());
            return Ok(path.to_str().map(|s| s.to_string()));
        }

        // For parametric presets, generate ICC from parameters
        let presets = get_filter_presets().await?;
        let preset = presets.iter().find(|p| p.id == preset_id).ok_or(format!("未找到预设: {}", preset_id))?;

        let mode = FilterMode::from_i32(preset.mode);
        let ramp = build_gamma_ramp(preset.temperature, preset.brightness, preset.contrast, preset.saturation, mode, None);

        let default_name = format!("NexBox_{}.icc", preset.name);
        let result = rfd::FileDialog::new()
            .set_title("保存 ICC 色彩配置文件")
            .add_filter("ICC 文件", &["icc", "icm"])
            .set_file_name(&default_name)
            .save_file();
        let path = match result { Some(p) => p, None => return Ok(None) };

        let description = format!("NexBox {} Filter", preset.name);
        let icc_data = build_icc_profile(&ramp, &description);
        fs::write(&path, &icc_data).map_err(|e| format!("无法保存文件: {}", e))?;
        log::info!("ICC profile exported: {} ({} bytes) from preset '{}'", path.display(), icc_data.len(), preset.name);
        Ok(path.to_str().map(|s| s.to_string()))
    }
    #[cfg(not(target_os = "windows"))]
    { Err("此功能仅支持 Windows 系统".to_string()) }
}

#[cfg(all(test, target_os = "windows"))]
mod delta_icc_tests {
    use super::*;

    /// 三角洲系列内置 ICC：每个预设都能找到文件、解析出非恒等 ramp，
    /// 且能派生出预览/数值面板所需的参数。
    #[test]
    fn delta_presets_parse_to_meaningful_ramps() {
        let ids = ["delta-super", "delta-a", "delta-b", "delta-c", "delta-d", "delta-e"];
        for id in ids {
            let filename = preset_id_to_builtin_icc(id)
                .unwrap_or_else(|| panic!("{}: preset_id_to_builtin_icc 未映射", id));
            let path = get_builtin_icc_path(&filename)
                .unwrap_or_else(|e| panic!("{}: 找不到内置 ICC {}: {}", id, filename, e));
            let parsed = parse_icc_file(path.to_str().unwrap())
                .unwrap_or_else(|e| panic!("{}: 解析 {} 失败: {}", id, filename, e));

            let ramp = parsed.to_ramp_array();
            // 非恒等：至少一个通道在中间调偏离线性超过 2%
            let max_dev = (0..3).map(|c| {
                (32..224).map(|i| {
                    (ramp[c][i] as f64 / (i as u32 * 256) as f64 - 1.0).abs()
                }).fold(0.0f64, f64::max)
            }).fold(0.0f64, f64::max);
            assert!(max_dev > 0.02, "{}: ramp 看起来是恒等的 (max_dev={})", id, max_dev);

            // 预览/数值派生不 panic 且亮度数值落在 50–150 显示区间
            let (t, b, c, s, _g, _sc, _rb, _gb, _bb) = derive_params_from_icc_ramp(&ramp);
            assert!((50..=150).contains(&b), "{}: 派生亮度 {} 越界", id, b);
            assert!((50..=150).contains(&c) && (50..=150).contains(&s), "{}: 派生对比/饱和越界", id);
            assert!((1000..=10000).contains(&t), "{}: 派生色温 {} 越界", id, t);
            let _ = compute_icc_preview(&ramp);
        }
    }
}

#[cfg(all(test, target_os = "windows"))]
mod coordination_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use std::sync::mpsc::{channel, RecvTimeoutError};
    use std::sync::Arc;
    use std::sync::Mutex as StdMutex;
    use std::thread;
    use std::time::Duration;

    /// 记录一次后端调用的操作类型。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Op {
        Capture,
        ApplyIcc,
        Write,
        Clear,
    }

    /// 测试注入：置位后，假后端的 `clear` 模拟生产后端在系统关机/注销时的行为——
    /// 跳过外部进程且**不报告清除成功**（返回 Err）。仅测试模块内可见。
    /// 用 thread_local 而非进程级静态：cargo test 默认并行运行各测试线程，
    /// 进程级标志会在测试间泄漏（一个测试置位会污染并行测试的 clear 语义）。
    thread_local! {
        static TEST_FORCE_SYSTEM_SHUTDOWN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    /// 测试上下文：完全独立的协调层实例 + 假后端。不触碰全局静态状态、
    /// 不触碰真实 GDI / xcalib / 显示器。假后端：
    /// - `fail`: 指定某一步返回 Err；
    /// - `pause_on` / `pause_hit` / `release_rx`: 指定某一步在进入时阻塞，等待
    ///   `release_tx` 放行（用于制造可控时序；等待可超时，防止挂死）；
    /// - 记录调用顺序与最终模拟屏幕值，供断言“没有迟到写屏”。
    struct TestContext {
        ops: DisplayOps<'static>,
        order: Arc<StdMutex<Vec<Op>>>,
        screen: Arc<StdMutex<GammaRamp>>,
        fail: Arc<StdMutex<Option<Op>>>,
        pause_on: Arc<StdMutex<Option<Op>>>,
        pause_hit: Arc<StdMutex<Option<Op>>>,
        /// 后端阻塞等待的“放行”通道（backend 侧持有 receiver）。
        release_rx: Arc<StdMutex<Option<std::sync::mpsc::Receiver<()>>>>,
        /// 测试侧持有的“放行”发送端。
        release_tx: Option<std::sync::mpsc::Sender<()>>,
    }

    impl TestContext {
        fn new(count: usize) -> Self {
            let order: Arc<StdMutex<Vec<Op>>> = Arc::new(StdMutex::new(Vec::new()));
            let screen: Arc<StdMutex<GammaRamp>> = Arc::new(StdMutex::new(
                [[0u16; 256]; 3],
            ));
            let fail: Arc<StdMutex<Option<Op>>> = Arc::new(StdMutex::new(None));
            let pause_on: Arc<StdMutex<Option<Op>>> = Arc::new(StdMutex::new(None));
            let pause_hit: Arc<StdMutex<Option<Op>>> = Arc::new(StdMutex::new(None));
            let release_rx: Arc<StdMutex<Option<std::sync::mpsc::Receiver<()>>>> =
                Arc::new(StdMutex::new(None));
            let backend = FakeBackend {
                order: Arc::clone(&order),
                screen: Arc::clone(&screen),
                fail: Arc::clone(&fail),
                pause_on: Arc::clone(&pause_on),
                pause_hit: Arc::clone(&pause_hit),
                release_rx: Arc::clone(&release_rx),
            };
            let states: Mutex<Option<Vec<Mutex<DisplayState>>>> = Mutex::new(Some(
                (0..count)
                    .map(|_| Mutex::new(DisplayState::default()))
                    .collect(),
            ));
            let op_locks: Mutex<Vec<Arc<Mutex<()>>>> = Mutex::new(
                (0..count).map(|_| Arc::new(Mutex::new(()))).collect(),
            );
            let ramps: Mutex<Vec<Mutex<Option<GammaRamp>>>> = Mutex::new(
                (0..count).map(|_| Mutex::new(None)).collect(),
            );
            let shutting_down = AtomicBool::new(false);
            let ops = DisplayOps {
                states: &states,
                op_locks: &op_locks,
                ramps: &ramps,
                shutting_down: &shutting_down,
                count,
                backend: &backend,
            };
            // 让 fake backend 与 DisplayOps 共享同一 release 通道
            let _ = backend;
            // 泄漏以避免借用在测试结束时失效 —— 但我们需要借用仅在测试体内有效：
            // 这里改用 Box::leak 以得到 'static 借用。
            let backend_leaked: &'static FakeBackend = Box::leak(Box::new(backend));
            let states_leaked: &'static Mutex<Option<Vec<Mutex<DisplayState>>>> =
                Box::leak(Box::new(states));
            let op_locks_leaked: &'static Mutex<Vec<Arc<Mutex<()>>>> = Box::leak(Box::new(op_locks));
            let ramps_leaked: &'static Mutex<Vec<Mutex<Option<GammaRamp>>>> =
                Box::leak(Box::new(ramps));
            let shutting_down_leaked: &'static AtomicBool = Box::leak(Box::new(shutting_down));
            let ops = DisplayOps {
                states: states_leaked,
                op_locks: op_locks_leaked,
                ramps: ramps_leaked,
                shutting_down: shutting_down_leaked,
                count,
                backend: backend_leaked,
            };
            TestContext {
                ops,
                order,
                screen,
                fail,
                pause_on,
                pause_hit,
                release_rx,
                release_tx: None,
            }
        }

        /// 让假后端在指定操作上暂停一次。测试持发送端，backend 持接收端。
        fn arm_pause(&mut self, op: Op) {
            let (tx, rx) = channel();
            *self.pause_on.lock().unwrap() = Some(op);
            *self.release_rx.lock().unwrap() = Some(rx);
            self.release_tx = Some(tx);
        }

        /// 放行暂停的后端调用。
        fn release_pause(&self) {
            self.release_tx.as_ref().unwrap().send(()).unwrap();
        }

        fn wait_paused(&self) {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                let hit = *self.pause_hit.lock().unwrap();
                if hit.is_some() {
                    return;
                }
                assert!(std::time::Instant::now() < deadline, "未等到暂停信号");
                thread::sleep(Duration::from_millis(2));
            }
        }

        fn log_order(&self) -> Vec<Op> {
            self.order.lock().unwrap().clone()
        }

        fn screen_value(&self) -> GammaRamp {
            *self.screen.lock().unwrap()
        }
    }

    /// 假后端：仅做物理 I/O 模拟，不参与版本/退出/待恢复协调逻辑（与真后端职责相同）。
    struct FakeBackend {
        order: Arc<StdMutex<Vec<Op>>>,
        screen: Arc<StdMutex<GammaRamp>>,
        fail: Arc<StdMutex<Option<Op>>>,
        pause_on: Arc<StdMutex<Option<Op>>>,
        pause_hit: Arc<StdMutex<Option<Op>>>,
        release_rx: Arc<StdMutex<Option<std::sync::mpsc::Receiver<()>>>>,
    }

    impl GammaBackend for FakeBackend {
        fn read(&self, display: usize) -> Result<GammaRamp, String> {
            self.order.lock().unwrap().push(Op::Capture);
            self.maybe_pause(Op::Capture);
            if *self.fail.lock().unwrap() == Some(Op::Capture) {
                return Err("模拟捕获失败".to_string());
            }
            Ok([[1u16; 256]; 3])
        }
        fn write(&self, display: usize, ramp: &GammaRamp) -> Result<(), String> {
            self.order.lock().unwrap().push(Op::Write);
            self.maybe_pause(Op::Write);
            if *self.fail.lock().unwrap() == Some(Op::Write) {
                return Err("模拟写回失败".to_string());
            }
            *self.screen.lock().unwrap() = *ramp;
            Ok(())
        }
        fn apply_icc(&self, display: usize, path: &Path) -> Result<(), String> {
            self.order.lock().unwrap().push(Op::ApplyIcc);
            self.maybe_pause(Op::ApplyIcc);
            if *self.fail.lock().unwrap() == Some(Op::ApplyIcc) {
                return Err("模拟应用 ICC 失败".to_string());
            }
            *self.screen.lock().unwrap() = [[2u16; 256]; 3];
            Ok(())
        }
        fn clear(&self, display: usize) -> Result<(), String> {
            self.order.lock().unwrap().push(Op::Clear);
            self.maybe_pause(Op::Clear);
            if *self.fail.lock().unwrap() == Some(Op::Clear) {
                return Err("模拟线性清除失败".to_string());
            }
            // 模拟生产后端的关机语义：系统关机/注销时跳过外部清除进程，
            // 必须返回 Err（未执行清除），不得假装清除成功。
            if TEST_FORCE_SYSTEM_SHUTDOWN.with(|f| f.get()) {
                return Err("系统关机/注销中，已跳过 xcalib 清除，未执行清除".to_string());
            }
            *self.screen.lock().unwrap() = [[0u16; 256]; 3];
            Ok(())
        }
    }

    impl FakeBackend {
        fn maybe_pause(&self, op: Op) {
            let should_pause = *self.pause_on.lock().unwrap() == Some(op);
            if !should_pause {
                return;
            }
            *self.pause_hit.lock().unwrap() = Some(op);
            // 等待测试放行（可超时，防止挂死）。取走 receiver 只等一次。
            let rx = self.release_rx.lock().unwrap().take();
            if let Some(rx) = rx {
                let _ = rx.recv_timeout(Duration::from_secs(5));
            }
        }
    }

    // ─── 场景 1：首次捕获失败 → 不调用任何应用写屏 ───
    #[test]
    fn capture_failure_aborts_apply() {
        let ctx = TestContext::new(1);
        *ctx.fail.lock().unwrap() = Some(Op::Capture);
        let err = ctx.ops.apply(0, 1, "apply_filter");
        assert!(err.is_err(), "捕获失败必须中止应用");
        let order = ctx.log_order();
        assert_eq!(order, vec![Op::Capture], "捕获失败后不得有任何应用写屏: {:?}", order);
        assert_eq!(ctx.screen_value(), [[0u16; 256]; 3], "屏幕不得被修改");
    }

    // ─── 场景 2：直接 ICC 首次应用 → 先捕获，再应用 ───
    #[test]
    fn direct_icc_applies_capture_then_icc() {
        let ctx = TestContext::new(1);
        ctx.ops.with_state(0, |s| {
            s.icc_active = true;
            s.active_icc_id = Some("builtin_xxx".to_string());
        });
        // 内置 ICC 不存在时 apply 会走用户 ramp 分支；这里用捕获+恢复验证顺序：
        let err = ctx.ops.apply(0, 1, "apply_filter");
        // 若内置 ICC 不存在且无 icc_ramp，apply 返回 Ok(()) 但已捕获 —— 验证顺序以 order 为准
        let order = ctx.log_order();
        assert!(order.contains(&Op::Capture), "必须首先捕获: {:?}", order);
        let first = order.first().copied();
        assert_eq!(first, Some(Op::Capture), "捕获必须是第一次后端调用: {:?}", order);
        let _ = err;
    }

    // ─── 场景 3：精确恢复失败后重试 → 原始 Ramp 保留；第二次写回相同数据 ───
    #[test]
    fn exact_restore_failure_keeps_ramp_and_retries() {
        let ctx = TestContext::new(1);
        // 先捕获（后端 read 成功，写入 [[1;256];3] 到 ramps）
        ctx.ops.capture(0).unwrap();
        // 模拟一次恢复：先让写回失败
        *ctx.fail.lock().unwrap() = Some(Op::Write);
        let r1 = ctx.ops.restore(0);
        assert!(r1.is_err(), "写回失败必须返回 Err");
        assert!(ctx.ops.peek_ramp(0).is_some(), "失败后原始 ramp 必须保留");
        assert!(ctx.ops.with_state(0, |s| s.restore_pending).unwrap(), "失败后待恢复标记必须保留");
        // 第二次恢复成功：必须写回相同数据
        *ctx.fail.lock().unwrap() = None;
        let r2 = ctx.ops.restore(0);
        assert!(matches!(r2, Ok(RestoreOutcome::Restored)), "第二次应精确恢复");
        assert_eq!(ctx.screen_value(), [[1u16; 256]; 3], "写回数据必须是捕获的原始 ramp");
        assert!(ctx.ops.peek_ramp(0).is_none(), "恢复成功后 ramp 应删除");
        assert!(!ctx.ops.with_state(0, |s| s.restore_pending).unwrap(), "恢复成功后待恢复标记应清除");
    }

    // ─── 场景 4：已关闭但待恢复 → 再次关闭仍执行恢复 ───
    #[test]
    fn repeated_close_retries_restore() {
        let ctx = TestContext::new(1);
        // 应用已改屏：捕获原始 ramp（[[1;256];3]），pending=true。
        ctx.ops.capture(0).unwrap();
        ctx.ops.set_restore_pending(0, true);
        // 第一次关闭的恢复写回失败：
        *ctx.fail.lock().unwrap() = Some(Op::Write);
        let r1 = ctx.ops.restore(0);
        assert!(r1.is_err(), "写回失败必须返回 Err");
        assert!(ctx.ops.with_state(0, |s| s.restore_pending).unwrap(), "恢复失败后仍待恢复");
        assert!(ctx.ops.peek_ramp(0).is_some(), "失败后原始 ramp 必须保留");
        // 再次关闭（重试）：写回成功，精确恢复。
        *ctx.fail.lock().unwrap() = None;
        let r = ctx.ops.restore(0);
        assert!(matches!(r, Ok(RestoreOutcome::Restored)), "再次关闭必须实际重试恢复");
        assert_eq!(ctx.screen_value(), [[1u16; 256]; 3], "写回数据必须是捕获的原始 ramp");
        assert!(!ctx.ops.with_state(0, |s| s.restore_pending).unwrap(), "恢复成功后待恢复标记应清除");
    }

    // ─── 场景 5：从未修改过屏幕 → 关闭/退出不无故线性清除 ───
    #[test]
    fn never_modified_no_unnecessary_clear() {
        let ctx = TestContext::new(1);
        let r = ctx.ops.restore(0);
        assert!(matches!(r, Ok(RestoreOutcome::NothingToDo)), "未受影响不应执行任何写屏");
        assert!(ctx.log_order().is_empty(), "不应有后端调用: {:?}", ctx.log_order());
        // 退出清理同样不写屏
        ctx.ops.cleanup();
        assert!(ctx.log_order().is_empty(), "cleanup 不应写屏: {:?}", ctx.log_order());
    }

    // ─── 场景 6：应用进行中退出 → 应用结束后恢复，退出等待恢复完成 ───
    #[test]
    fn exit_during_apply_waits_and_restores() {
        let mut ctx = TestContext::new(1);
        // 预捕获，避免 apply 在 capture 处暂停（我们用 ApplyIcc 暂停）。
        ctx.ops.capture(0).unwrap();
        // 用生成器产生一个操作版本（与生产一致：先提交意图再应用）。
        let gen = ctx.ops.submit(0, |s| {
            s.icc_active = true;
            s.active_icc_id = Some("builtin_xxx".to_string());
            s.icc_ramp = Some([[5u16; 256]; 3]);
            bump_operation_generation(s)
        }).unwrap();
        // 在 ApplyIcc 上设置暂停点，制造“写屏进行中”的时序。
        ctx.arm_pause(Op::ApplyIcc);
        // 应用线程通过**生产使用的版本化、持操作锁包装入口**执行
        //（run_if_current 内部持有该显示器操作锁——cleanup 会等待它）。
        let ops = ctx.ops;
        let apply_handle = thread::spawn(move || {
            apply_filter_if_current_with(&ops, 0, gen)
        });
        // 等待应用进入暂停点（写屏前，已持操作锁、已置 pending=true）。
        ctx.wait_paused();
        // 此时退出：cleanup 先置退出标志并使任务失效，然后等待操作锁
        //（被应用持有），应用完成后才拿到锁并恢复。
        let ops2 = ctx.ops;
        let cleanup_handle = thread::spawn(move || ops2.cleanup());
        // **同步证据**：等待 cleanup 已完成 begin_shutdown（置位退出标志），
        // 再放行应用——避免测试退化为"应用先完成、cleanup 后运行"。
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !ctx.ops.is_shutting_down() {
            assert!(std::time::Instant::now() < deadline, "cleanup 未在超时内置位退出标志");
            thread::sleep(Duration::from_millis(5));
        }
        // 放行应用（通过 release 通道恢复，而非等待超时）。
        ctx.release_pause();
        let summary = cleanup_handle.join().unwrap();
        let apply_result = apply_handle.join().unwrap();
        // 检查结果而不只是 join：cleanup 汇总恢复为 1，应用结果为 Executed（写屏完成）。
        assert_eq!(summary.restored, 1, "退出后必须完成一次精确恢复: {:?}", summary);
        match apply_result {
            Ok(RunResult::Executed(_)) => {}
            other => panic!("应用应完成 Executed（被阻塞后放行），实际 {:?}", other),
        }
        // 最终屏幕必须回到原始 ramp（[[1;256];3]）。
        assert_eq!(ctx.screen_value(), [[1u16; 256]; 3], "退出后不得残留应用滤镜");
        // 调用顺序：应用写屏后跟一次恢复写回。
        let order = ctx.log_order();
        let apply_pos = order.iter().position(|o| *o == Op::ApplyIcc).unwrap();
        let write_pos = order.iter().position(|o| *o == Op::Write).unwrap();
        assert!(apply_pos < write_pos, "必须先应用后恢复: {:?}", order);
        assert_eq!(ctx.ops.with_state(0, |s| s.restore_pending).unwrap(), false, "恢复完成后标记应清除");
    }

    // ─── 场景 7：退出后提交新应用 → 被拒绝，不发生物理写屏 ───
    #[test]
    fn submit_after_exit_is_rejected() {
        let ctx = TestContext::new(1);
        ctx.ops.begin_shutdown();
        let res = ctx.ops.submit(0, |s| {
            s.filter_active = true;
            bump_operation_generation(s)
        });
        assert!(res.is_none(), "退出后提交必须被拒绝");
        assert!(!ctx.ops.with_state(0, |s| s.filter_active).unwrap(), "状态不得被修改");
        assert!(ctx.log_order().is_empty(), "不得发生任何后端调用");
    }

    // ─── 场景 8：关闭任务已过期 → 不关闭后续开启的滤镜，也不报告恢复完成 ───
    #[test]
    fn stale_close_does_not_restore_newer_filter() {
        let ctx = TestContext::new(1);
        // 捕获原始 ramp
        ctx.ops.capture(0).unwrap();
        // 第一次关闭意图：版本 g1
        let g1 = ctx.ops.with_state(0, |s| {
            s.filter_active = false;
            bump_operation_generation(s)
        }).unwrap();
        // 用户随后重新开启：版本 g2（更新的意图）
        let g2 = ctx.ops.with_state(0, |s| {
            s.filter_active = true;
            bump_operation_generation(s)
        }).unwrap();
        assert_ne!(g1, g2);
        // 旧的关闭任务 g1 被执行：必须跳过
        let r = ctx.ops.run_if_current(0, g1, "restore_filter", false, || ctx.ops.restore(0));
        match r {
            Ok(RunResult::SkippedStale) => {}
            Ok(RunResult::Executed(_)) => panic!("过期关闭任务不得执行恢复"),
            Err(e) => panic!("不应报错: {}", e),
        }
        // 状态仍保持开启
        assert!(ctx.ops.with_state(0, |s| s.filter_active).unwrap(), "后续开启的滤镜不得被关闭");
        // 未报告恢复完成：没有 Write 写回
        assert_eq!(ctx.log_order(), vec![Op::Capture], "不得发生恢复写屏: {:?}", ctx.log_order());
    }

    // ─── 场景 9：入口检查后发生退出 → 随后的锁内提交被拒绝 ───
    #[test]
    fn entry_check_then_exit_rejects_in_lock_submit() {
        let ctx = TestContext::new(1);
        // 入口快速检查通过（未退出）
        assert!(!ctx.ops.is_shutting_down());
        // 退出在状态锁内置位（模拟 cleanup 已经执行）
        ctx.ops.begin_shutdown();
        // 锁内提交必须拒绝
        let res = ctx.ops.submit(0, |s| {
            s.filter_active = true;
            bump_operation_generation(s)
        });
        assert!(res.is_none());
        assert!(!ctx.ops.with_state(0, |s| s.filter_active).unwrap());
    }

    // ─── 场景 10：恢复写回进行中连续关闭两次 → 不丢失最终恢复任务，最终屏幕恢复 ───
    #[test]
    fn close_twice_during_restore_still_restores() {
        let mut ctx = TestContext::new(1);
        // 应用已改屏：捕获了原始 ramp（[[1;256];3]），pending=true。
        ctx.ops.capture(0).unwrap();
        ctx.ops.set_restore_pending(0, true);
        // 在恢复的写回步骤上设置暂停点，制造“第一次关闭的恢复正在执行”的时序。
        ctx.arm_pause(Op::Write);
        let ops = ctx.ops;
        let restore_thread = thread::spawn(move || ops.restore(0));
        // 等待写回暂停命中（恢复已持有操作锁）
        ctx.wait_paused();
        // 关闭两次：都被接受（版本递增 + 各自派发版本化恢复）。由于第一次恢复
        // 持有操作锁，后续恢复任务会在锁上等待；锁内版本检查会让过期者跳过。
        let g1 = ctx.ops.with_state(0, |s| {
            s.filter_active = false;
            bump_operation_generation(s)
        }).unwrap();
        let g2 = ctx.ops.with_state(0, |s| {
            s.filter_active = false;
            bump_operation_generation(s)
        }).unwrap();
        let _ = (g1, g2);
        // 放行写回：第一次恢复完成（精确恢复，ramp 删除、pending 清除）。
        ctx.release_pause();
        restore_thread.join().unwrap();
        // 最终：pending 已清除，屏幕回到原始 ramp（没有丢失最终恢复任务）。
        assert!(!ctx.ops.with_state(0, |s| s.restore_pending).unwrap(), "最终恢复任务不得丢失");
        assert_eq!(ctx.screen_value(), [[1u16; 256]; 3], "最终屏幕必须恢复");
    }

    // ─── 场景 11：捕获期间连续关闭两次 → 最终恢复任务不丢失 ───
    // 与场景 10（恢复写回暂停时再次关闭）不同：这里初始**没有原始 Ramp、没有
    // pending**——应用任务已获准执行但仍在捕获原始 ramp（后端 read 尚未返回）。
    // 该时序覆盖“应用尚未捕获、待恢复记录尚未建立”时的漏洞：
    // 第二次关闭不能因开关已关闭 / pending 未置位而放弃安排恢复。
    #[test]
    fn close_twice_during_capture_still_restores() {
        let mut ctx = TestContext::new(1);
        // 初始：无 ramp、无 pending（默认即为 None/false）。
        assert!(ctx.ops.peek_ramp(0).is_none(), "初始不应有原始 ramp");
        assert!(!ctx.ops.with_state(0, |s| s.restore_pending).unwrap(), "初始不应有待恢复标记");

        // A：提交开启并进入应用执行器（先捕获再应用 ICC）。
        // 应用路径使用生产执行器 run_if_current + apply，捕获即 run 内的 capture 步骤。
        let gen_apply = ctx.ops.submit(0, |s| {
            s.filter_active = true;
            s.icc_active = true;
            s.active_icc_id = Some("builtin_xxx".to_string());
            s.icc_ramp = Some([[9u16; 256]; 3]); // 用户 ICC ramp（apply 会用它写临时 ICC）
            bump_operation_generation(s)
        }).unwrap();
        // 在捕获（read）处暂停：应用已取得操作锁，后端 read 尚未返回。
        ctx.arm_pause(Op::Capture);
        let ops = ctx.ops;
        let apply_handle = thread::spawn(move || {
            ops.run_if_current(0, gen_apply, "apply_filter", false, || ops.apply(0, gen_apply, "apply_filter"))
        });
        ctx.wait_paused(); // 应用已暂停在捕获

        // B：第一次关闭（在应用暂停期间提交，尚未取得操作锁）。
        let g1 = ctx.ops.submit(0, |s| {
            s.filter_active = false;
            bump_operation_generation(s)
        });
        assert!(g1.is_some(), "第一次关闭必须被接受");
        // C：第二次关闭（在应用暂停期间提交，成为最新版本）。
        let g2 = ctx.ops.submit(0, |s| {
            s.filter_active = false;
            bump_operation_generation(s)
        });
        assert!(g2.is_some(), "第二次关闭必须被接受");
        // B/C 提交必须发生在 A 暂停期间（此时应用尚未完成捕获）。
        assert_eq!(*ctx.pause_hit.lock().unwrap(), Some(Op::Capture), "提交应在捕获暂停期间发生");

        // 放行捕获：应用继续。此时应用任务已过期（版本已被两次关闭推进）——
        // capture_if_current 在捕获完成后复查版本，把“未执行写屏”传播为 SkippedStale，
        // 而不是包装成 Executed——调用方不会把未应用的滤镜登记为成功。
        ctx.release_pause();
        let apply_result = apply_handle.join().unwrap();
        // run_if_current 外层包一层 Executed（本次初次检查通过），内层是 apply 的结果：
        // 捕获后版本复查发现过期必须为内层 SkippedStale——不得出现内层 Executed（那是“已写屏”）。
        match apply_result {
            Ok(RunResult::Executed(RunResult::SkippedStale)) => {}
            Ok(RunResult::Executed(RunResult::Executed(()))) => {
                panic!("捕获后版本已过期，应用必须报告 SkippedStale，不得报 Executed")
            }
            Ok(RunResult::SkippedStale) => panic!("初次锁内检查不应过期（版本在捕获后才被推进）"),
            Err(e) => panic!("应用任务不应失败: {}", e),
        }
        // 执行关闭恢复：用生产执行器逐次派发版本化恢复。
        // g1（过期）可跳过，但必须安排；g2（最新）承担最终恢复。
        let r1 = ctx.ops.run_if_current(0, g1.unwrap(), "restore_filter", false, || ctx.ops.restore(0));
        let r2 = ctx.ops.run_if_current(0, g2.unwrap(), "restore_filter", false, || ctx.ops.restore(0));
        match r1 {
            Ok(RunResult::SkippedStale) => {}
            other => panic!("第一次关闭恢复应过期跳过（其版本已被第二次关闭推进），实际 {:?}", other.map(|_| ())),
        }
        match r2 {
            Ok(RunResult::Executed(_)) => {}
            other => panic!("第二次关闭必须承担最终恢复，实际 {:?}", other.map(|_| ())),
        }
        // 核心：应用任务不得产生 ICC 写屏（捕获后版本复查生效）。
        assert!(!ctx.log_order().contains(&Op::ApplyIcc), "捕获后版本已过期，不得发生应用写屏: {:?}", ctx.log_order());

        // 最终断言：
        // - 最后一次物理写屏是恢复（Write），之后没有迟到应用（ApplyIcc 不得在 Write 之后）。
        let order = ctx.log_order();
        let last_write = order.iter().rposition(|o| *o == Op::Write);
        assert!(last_write.is_some(), "必须发生恢复写屏: {:?}", order);
        let after_last_write = &order[last_write.unwrap() + 1..];
        assert!(!after_last_write.contains(&Op::ApplyIcc), "恢复之后不得有迟到应用: {:?}", order);
        // - 最终模拟屏幕等于原始 ramp（[[1;256];3]，捕获值）。
        assert_eq!(ctx.screen_value(), [[1u16; 256]; 3], "最终屏幕必须恢复为原始 ramp");
        // - pending 清除；原始 ramp 按成功恢复规则释放。
        assert!(!ctx.ops.with_state(0, |s| s.restore_pending).unwrap(), "恢复完成后待恢复标记应清除");
        assert!(ctx.ops.peek_ramp(0).is_none(), "恢复成功后原始 ramp 应释放");
    }

    // ─── 场景 12：无效显示器索引 → 明确拒绝，不触碰任何显示器后端 ───
    #[test]
    fn invalid_display_index_is_rejected() {
        let ctx = TestContext::new(1);
        // 空状态集合：with_state/submit/op_lock/restore 都必须拒绝。
        //（TestContext 构造 1 台；用 count=0 构造空集合验证空集合路径）
        // 这里用越界索引 5 验证：不得夹取到 0 号显示器。
        assert!(ctx.ops.with_state(5, |s| s.filter_active).is_none(), "越界索引不得夹取");
        assert!(ctx.ops.submit(5, |s| s.filter_active).is_none(), "越界索引提交必须拒绝");
        assert!(ctx.ops.op_lock(5).is_none(), "越界索引不得返回锁");
        assert!(ctx.ops.restore(5).is_err(), "越界索引恢复必须报错");
        assert!(ctx.ops.apply(5, 1, "apply_filter").is_err(), "越界索引应用必须报错");
        assert!(ctx.ops.run_if_current(5, 1, "apply_filter", false, || Ok(())).is_err(),
                "越界索引执行器必须拒绝");
        // 未发生任何后端调用（未触碰任何显示器）。
        assert!(ctx.log_order().is_empty(), "越界索引不得触碰任何显示器后端: {:?}", ctx.log_order());
        // 0 号显示器状态不受影响。
        assert!(!ctx.ops.with_state(0, |s| s.filter_active).unwrap());
    }

    // ─── 降级结果 1：无原始 Ramp、待恢复、清除成功 → 返回降级结果 ───
    #[test]
    fn degraded_clear_reports_degraded() {
        let ctx = TestContext::new(1);
        ctx.ops.set_restore_pending(0, true);
        let r = ctx.ops.restore(0);
        assert!(matches!(r, Ok(RestoreOutcome::DegradedCleared)), "应返回降级清除结果: {:?}", r);
        assert!(!ctx.ops.with_state(0, |s| s.restore_pending).unwrap(), "清除成功后标记清除");
        assert_eq!(ctx.screen_value(), [[0u16; 256]; 3]);
        let order = ctx.log_order();
        assert_eq!(order, vec![Op::Clear], "仅执行线性清除: {:?}", order);
    }

    // ─── 降级结果 2：无原始 Ramp、待恢复、清除失败 → 返回错误，待恢复保留 ───
    #[test]
    fn degraded_clear_failure_keeps_pending() {
        let ctx = TestContext::new(1);
        ctx.ops.set_restore_pending(0, true);
        *ctx.fail.lock().unwrap() = Some(Op::Clear);
        let r = ctx.ops.restore(0);
        assert!(r.is_err(), "清除失败必须返回 Err");
        assert!(ctx.ops.with_state(0, |s| s.restore_pending).unwrap(), "失败后待恢复标记必须保留");
        // 第二次恢复成功
        *ctx.fail.lock().unwrap() = None;
        let r2 = ctx.ops.restore(0);
        assert!(matches!(r2, Ok(RestoreOutcome::DegradedCleared)));
        assert!(!ctx.ops.with_state(0, |s| s.restore_pending).unwrap());
    }

    // ─── 关机跳过清除：未执行清除不得报告清除成功 ───
    // 系统关机/注销时 clear 因跳过外部进程而未执行；restore 必须报错、
    // 保留待恢复标记，cleanup 计入失败而不是 restored/degraded。
    // 通过测试注入开关模拟“系统关机”标志，不模拟真实 Windows 关机。
    #[test]
    fn system_shutdown_skipped_clear_is_not_success() {
        let ctx = TestContext::new(1);
        ctx.ops.set_restore_pending(0, true);
        TEST_FORCE_SYSTEM_SHUTDOWN.with(|f| f.set(true));
        let r = ctx.ops.restore(0);
        TEST_FORCE_SYSTEM_SHUTDOWN.with(|f| f.set(false));
        assert!(r.is_err(), "关机时跳过清除必须返回错误，不得报告 DegradedCleared: {:?}", r);
        // 未执行清除：待恢复标记保留，屏幕未被修改。
        assert!(ctx.ops.with_state(0, |s| s.restore_pending).unwrap(), "未执行清除，待恢复标记必须保留");
        assert_eq!(ctx.screen_value(), [[0u16; 256]; 3], "屏幕不得被修改");
        let order = ctx.log_order();
        assert_eq!(order, vec![Op::Clear], "仅尝试过一次清除: {:?}", order);

        // cleanup 同样不能把它计入成功：pending 仍为 true（restored/degraded 路径都会清除标记）。
        // CleanupSummary 现在直接返回计数（failed/restored/degraded/idle），不再仅靠
        // "pending 保留" 间接推断失败计入。
        TEST_FORCE_SYSTEM_SHUTDOWN.with(|f| f.set(true));
        let summary = ctx.ops.cleanup();
        TEST_FORCE_SYSTEM_SHUTDOWN.with(|f| f.set(false));
        assert_eq!(summary.failed, 1, "关机跳过清除必须计入 failed: {:?}", summary);
        assert_eq!(summary.restored, 0, "未发生精确恢复，不得计入 restored: {:?}", summary);
        assert_eq!(summary.degraded, 0, "未发生降级清除，不得计入 degraded: {:?}", summary);
        assert_eq!(summary.idle, 0, "存在待恢复目标，cleanup 不得计入 idle: {:?}", summary);
        assert!(ctx.ops.with_state(0, |s| s.restore_pending).unwrap(), "cleanup 计入失败，待恢复标记保留");
    }

    // ─── 捕获后跳过必须传播为 SkippedStale（单元级：apply 直接返回）───
    #[test]
    fn apply_reports_skipped_stale_when_version_changes_during_capture() {
        let mut ctx = TestContext::new(1);
        let gen = ctx.ops.submit(0, |s| {
            s.filter_active = true;
            s.icc_active = true;
            s.active_icc_id = Some("builtin_xxx".to_string());
            s.icc_ramp = Some([[9u16; 256]; 3]);
            bump_operation_generation(s)
        }).unwrap();
        ctx.arm_pause(Op::Capture);
        let ops = ctx.ops;
        let handle = thread::spawn(move || {
            ops.run_if_current(0, gen, "apply_filter", false, || ops.apply(0, gen, "apply_filter"))
        });
        ctx.wait_paused();
        // 捕获暂停期间推进版本（模拟一次更新的关闭意图）。
        let _ = ctx.ops.submit(0, |s| {
            s.filter_active = false;
            bump_operation_generation(s)
        });
        ctx.release_pause();
        // run_if_current 对 apply 闭包结果外层包一层 Executed；内层 SkippedStale
        // 必须保留——这就是“未执行”信号，不得丢失。
        match handle.join().unwrap() {
            Ok(RunResult::Executed(RunResult::SkippedStale)) => {}
            other => panic!("捕获后版本过期必须返回 SkippedStale，实际 {:?}", other.map(|_| ())),
        }
        assert!(!ctx.log_order().contains(&Op::ApplyIcc), "不得发生应用写屏: {:?}", ctx.log_order());
    }

    // ─── 自动归属层测试 ───
    // 以下测试覆盖"自动开启/失败/退出/接管"场景下，归属记录的条件登记、条件弃权、
    // 拓扑失效检测等行为。归属锁与设备拓扑均通过测试注入的独立 slot/闭包控制，
    // 不触碰全局 static 状态。

    /// 构造一条归属记录（默认小工具，便于测试直接构造各种 state/版本组合）。
    fn make_record(session: u64, idx: usize, gen: u64, state: AutoSessionState) -> AutoOwnership {
        AutoOwnership {
            session,
            display_idx: idx,
            device_name: format!("DEV{}", idx),
            display_count: 1,
            operation_generation: gen,
            state,
            restore_generation: None,
        }
    }

    // ─── 场景 13：旧自动任务应用失败需要回滚，但新意图已提交 → 旧失败不得取消新任务 ───
    #[test]
    fn old_failed_apply_does_not_cancel_new_intent() {
        // 场景：旧自动会话 1 应用失败需要回滚（auto_rollback_failed_apply），
        // 但用户已经提交了新的开启意图（会话 2 已登记 Applying）。
        // 期望：旧任务的回滚因版本过期而条件关闭被拒绝，不得清除/改写会话 2 的记录，
        // 不得写屏，新意图保留。
        let ctx = TestContext::new(1);
        // 旧自动会话 1：开启意图，登记版本 g1。
        let g1 = ctx.ops.submit(0, |s| {
            s.filter_active = true;
            bump_operation_generation(s)
        }).unwrap();
        let slot: AutoOwnershipSlot = StdMutex::new(Some(make_record(1, 0, g1, AutoSessionState::Applying)));
        // 新意图（会话 2）提交，版本推进到 g2，登记覆盖 slot。
        let g2 = ctx.ops.submit(0, |s| {
            s.filter_active = true;
            bump_operation_generation(s)
        }).unwrap();
        *slot.lock().unwrap() = Some(make_record(2, 0, g2, AutoSessionState::Applying));
        // 旧会话 1 的回滚：版本 g1 已过期，conditional_close 返回 None → 不写屏、不动状态。
        let rolled = auto_rollback_failed_apply(&slot, 1, &ctx.ops);
        assert_eq!(rolled, RollbackOutcome::Superseded, "旧会话已过期，rollback 必须返回 Superseded");
        // 旧会话的条件弃权：归属已属于会话 2，不得被会话 1 释放。
        assert!(!auto_release_if_owned(&slot, 1), "旧会话不得释放新会话的归属");
        // slot 仍是会话 2 的 Applying 记录，版本为 g2。
        let rec = slot.lock().unwrap().clone().expect("slot 必须仍持有归属");
        assert_eq!(rec.session, 2);
        assert_eq!(rec.state, AutoSessionState::Applying);
        assert_eq!(rec.operation_generation, g2);
        // 新意图的 filter_active 保留。
        assert_eq!(ctx.ops.with_state(0, |s| s.filter_active), Some(true));
        // 未发生任何后端调用（无 Capture/ApplyIcc/Write/Clear）。
        assert!(ctx.log_order().is_empty(), "旧失败回滚不得写屏: {:?}", ctx.log_order());
    }

    // ─── 场景 14：旧自动任务 SkippedStale → 条件弃权不得清除新会话归属 ───
    #[test]
    fn old_skipped_apply_does_not_clear_new_ownership() {
        // 场景：旧自动会话 1 的 apply 路径返回 SkippedStale，调用 auto_release_if_owned
        // 试图条件弃权；归属已被会话 2 接管，旧会话 1 不得清除新会话的记录。
        let ctx = TestContext::new(1);
        // slot 直接放置会话 2 的 Applying 记录（任意版本）。
        let g2 = ctx.ops.submit(0, |s| {
            s.filter_active = true;
            bump_operation_generation(s)
        }).unwrap();
        let slot: AutoOwnershipSlot = StdMutex::new(Some(make_record(2, 0, g2, AutoSessionState::Applying)));
        // 旧会话 1 试图条件弃权：记录不属于 session 1，必须返回 false。
        assert!(!auto_release_if_owned(&slot, 1), "旧会话 1 不得释放会话 2 的归属");
        // slot 仍为会话 2 的记录。
        let rec = slot.lock().unwrap().clone().expect("会话 2 归属必须保留");
        assert_eq!(rec.session, 2);
        assert_eq!(rec.state, AutoSessionState::Applying);
        // 同时验证：会话 2 自己可以正常条件弃权（确认归属状态合法）。
        assert!(auto_release_if_owned(&slot, 2), "会话 2 应能释放自己的归属");
        assert!(slot.lock().unwrap().is_none());
    }

    // ─── 场景 15：旧自动任务晚到成功 → 不得登记为当前自动会话 ───
    #[test]
    fn late_success_of_old_auto_task_not_registered() {
        // 场景：旧自动会话 1 的 apply 实际写屏完成（晚到），调用 auto_mark_applied
        // 试图登记为 Applied；归属已被会话 2 接管，旧会话 1 不得改写他人记录。
        let ctx = TestContext::new(1);
        let g2 = ctx.ops.submit(0, |s| {
            s.filter_active = true;
            bump_operation_generation(s)
        }).unwrap();
        let slot: AutoOwnershipSlot = StdMutex::new(Some(make_record(2, 0, g2, AutoSessionState::Applying)));
        // 旧会话 1 试图登记 Applied：session 不匹配 → 必须返回 false。
        assert!(!auto_mark_applied(&slot, 1, &ctx.ops), "旧会话 1 不得登记覆盖会话 2 的记录");
        // slot 仍为会话 2 的 Applying（未被改写）。
        let rec = slot.lock().unwrap().clone().expect("会话 2 归属必须保留");
        assert_eq!(rec.session, 2);
        assert_eq!(rec.state, AutoSessionState::Applying, "旧会话不得改写 state 为 Applied");
    }

    // ─── 场景 16：自动恢复针对登记时的目标显示器，不查询 active index ───
    #[test]
    fn auto_restore_targets_original_display_not_active() {
        // 场景：自动开启目标是显示器 A（index 0），随后"界面切到显示器 B"。
        // 游戏结束触发自动恢复时，恢复决策必须用登记时的 display_idx=0，
        // 不得因界面切换到 B 而去恢复 B。拓扑闭包与登记一致 → Proceed。
        let ctx = TestContext::new(2);
        // 自动开启显示器 A：捕获原 ramp 并置 pending。
        let g1 = ctx.ops.submit(0, |s| {
            s.filter_active = true;
            bump_operation_generation(s)
        }).unwrap();
        ctx.ops.capture(0).unwrap();
        ctx.ops.set_restore_pending(0, true);
        // 手工构造归属记录：display_idx=0, device_name="DEV0", display_count=2（与拓扑一致）。
        let slot: AutoOwnershipSlot = StdMutex::new(Some(AutoOwnership {
            session: 1,
            display_idx: 0,
            device_name: "DEV0".to_string(),
            display_count: 2,
            operation_generation: g1,
            state: AutoSessionState::Applied,
            restore_generation: None,
        }));
        // 决策：拓扑闭包返回 ("DEV0", 2) — 与登记匹配 → Proceed{display_idx:0, ..}。
        let decision = auto_restore_decision(&slot, 1, &ctx.ops, |idx| {
            Some((format!("DEV{}", idx), 2))
        });
        let rgen = match decision {
            AutoRestoreDecision::Proceed { display_idx, restore_generation } => {
                assert_eq!(display_idx, 0, "恢复必须针对登记时的显示器 A（index 0），不得切到 B");
                restore_generation
            }
            other => panic!("拓扑一致时必须 Proceed，实际 {:?}", other),
        };
        // 执行版本化恢复。
        let r = restore_display_default_if_current_with(&ctx.ops, 0, rgen).unwrap();
        assert!(matches!(r, RunResult::Executed(RestoreOutcome::Restored)),
                "恢复必须执行成功: {:?}", r);
        // 收尾：从 Applied 转 Restoring 后由 finish_restore 移除。
        assert!(auto_finish_restore(&slot, 1));
        assert!(slot.lock().unwrap().is_none(), "恢复成功后 slot 必须清空");
        // 显示器 B 完全未被触碰。
        assert_eq!(ctx.ops.with_state(1, |s| (s.filter_active, s.restore_pending)),
                   Some((false, false)), "显示器 B 不得被自动恢复触碰");
        // 后端调用顺序：一次 Capture（开启时）+ 一次 Write（恢复）。
        assert_eq!(ctx.log_order(), vec![Op::Capture, Op::Write],
                   "必须恰好一次捕获 + 一次恢复写屏: {:?}", ctx.log_order());
    }

    // ─── 场景 17：用户接管（版本被推进）→ Superseded，不覆盖用户选择 ───
    #[test]
    fn user_takeover_during_game_stops_auto_restore() {
        // 场景：自动开启后用户在游戏过程中手动修改 → 游戏结束自动恢复时
        // 版本已被用户推进，决策必须返回 Superseded，不得写屏。
        let ctx = TestContext::new(1);
        let g1 = ctx.ops.submit(0, |s| {
            s.filter_active = true;
            bump_operation_generation(s)
        }).unwrap();
        ctx.ops.capture(0).unwrap();
        ctx.ops.set_restore_pending(0, true);
        let slot: AutoOwnershipSlot = StdMutex::new(Some(make_record(1, 0, g1, AutoSessionState::Applied)));
        // 用户手动操作：关闭滤镜并推进版本（g2）。
        let _ = ctx.ops.submit(0, |s| {
            s.filter_active = false;
            bump_operation_generation(s)
        }).unwrap();
        // 决策：conditional_close 因版本已过期返回 None → Superseded，slot 清空。
        let decision = auto_restore_decision(&slot, 1, &ctx.ops, |_idx| Some(("DEV0".to_string(), 1)));
        assert!(matches!(decision, AutoRestoreDecision::Superseded),
                "用户接管后必须 Superseded，实际 {:?}", decision);
        assert!(slot.lock().unwrap().is_none(), "Superseded 必须丢弃旧归属");
        // 未发生任何恢复写屏：log_order 仍只有开启时的 Capture。
        assert_eq!(ctx.log_order(), vec![Op::Capture], "自动恢复不得写屏: {:?}", ctx.log_order());
        // 用户意图保留（filter_active=false）。
        assert_eq!(ctx.ops.with_state(0, |s| s.filter_active), Some(false));
    }

    // ─── 场景 18：恢复失败保留重试责任；用户接管停止旧重试 ───
    #[test]
    fn auto_restore_failure_keeps_retry_and_user_takeover_stops_it() {
        // 场景 6a：恢复失败 → slot 必须保留 Restoring 状态以承担重试。
        let ctx = TestContext::new(1);
        let g1 = ctx.ops.submit(0, |s| {
            s.filter_active = true;
            bump_operation_generation(s)
        }).unwrap();
        ctx.ops.capture(0).unwrap();
        ctx.ops.set_restore_pending(0, true);
        let slot: AutoOwnershipSlot = StdMutex::new(Some(make_record(1, 0, g1, AutoSessionState::Applied)));
        let decision = auto_restore_decision(&slot, 1, &ctx.ops, |_idx| Some(("DEV0".to_string(), 1)));
        let rgen = match decision {
            AutoRestoreDecision::Proceed { restore_generation, .. } => restore_generation,
            other => panic!("拓扑一致必须 Proceed，实际 {:?}", other),
        };
        // 写回失败：restore 返回 Err。
        *ctx.fail.lock().unwrap() = Some(Op::Write);
        let r = restore_display_default_if_current_with(&ctx.ops, 0, rgen);
        assert!(r.is_err(), "写回失败必须返回 Err");
        // slot 仍存在，state==Restoring 且 restore_generation==Some(rgen)。
        let rec = slot.lock().unwrap().clone().expect("失败后 slot 必须保留归属");
        assert_eq!(rec.state, AutoSessionState::Restoring,
                   "恢复失败后状态必须为 Restoring 以保留重试责任");
        assert_eq!(rec.restore_generation, Some(rgen),
                   "恢复失败后 restore_generation 必须保留以重试");

        // 场景 6b：重试成功 → 收尾清理。
        *ctx.fail.lock().unwrap() = None;
        let r = restore_display_default_if_current_with(&ctx.ops, 0, rgen).unwrap();
        assert!(matches!(r, RunResult::Executed(RestoreOutcome::Restored)),
                "重试必须恢复成功: {:?}", r);
        assert!(auto_finish_restore(&slot, 1));
        assert!(slot.lock().unwrap().is_none(), "成功后 slot 必须清空");
        assert_eq!(ctx.screen_value(), [[1u16; 256]; 3], "屏幕必须恢复为原始 ramp");
        assert!(!ctx.ops.with_state(0, |s| s.restore_pending).unwrap(),
                "恢复成功后待恢复标记必须清除");

        // 场景 6c：用户接管 → 重试必须跳过期、不得写屏、归属被会话 2 接管并最终释放。
        // 重新登记会话 2：开启 + 捕获 + pending + Applied 记录。
        let g3 = ctx.ops.submit(0, |s| {
            s.filter_active = true;
            bump_operation_generation(s)
        }).unwrap();
        ctx.ops.capture(0).unwrap();
        ctx.ops.set_restore_pending(0, true);
        let slot: AutoOwnershipSlot = StdMutex::new(Some(make_record(2, 0, g3, AutoSessionState::Applied)));
        let decision = auto_restore_decision(&slot, 2, &ctx.ops, |_idx| Some(("DEV0".to_string(), 1)));
        let rgen2 = match decision {
            AutoRestoreDecision::Proceed { restore_generation, .. } => restore_generation,
            other => panic!("拓扑一致必须 Proceed，实际 {:?}", other),
        };
        // 写回失败：记录进入 Restoring。
        *ctx.fail.lock().unwrap() = Some(Op::Write);
        let r = restore_display_default_if_current_with(&ctx.ops, 0, rgen2);
        assert!(r.is_err(), "写回失败必须返回 Err");
        // 用户接管：推进版本（g4）。
        let _ = ctx.ops.submit(0, |s| {
            s.filter_active = true;
            bump_operation_generation(s)
        }).unwrap();
        // 重试旧恢复 rgen2：版本已过期，必须 SkippedStale，不得写屏。
        let order_before = ctx.log_order().len();
        let r = restore_display_default_if_current_with(&ctx.ops, 0, rgen2).unwrap();
        assert!(matches!(r, RunResult::SkippedStale),
                "用户接管后旧重试必须 SkippedStale，实际 {:?}", r);
        assert_eq!(ctx.log_order().len(), order_before,
                   "旧重试不得产生新写屏: {:?}", ctx.log_order());
        // 会话 2 条件弃权：slot 必须清空。
        assert!(auto_release_if_owned(&slot, 2));
        assert!(slot.lock().unwrap().is_none());
    }

    // ─── 场景 19：拓扑变化（设备名/数量不符）→ TargetGone，不写任何显示器 ───

    // ─── 场景 20：生产应用包装层 apply_filter_if_current_with 必须返回单层 SkippedStale ───
    #[test]
    fn production_apply_wrapper_reports_single_layer_skipped_stale() {
        // 场景：apply_filter_if_current_with 是生产使用的应用包装层。捕获期间
        // 版本被推进时必须向调用方返回**单层** SkippedStale（不是被 run_if_current
        // 外层包成 Executed(SkippedStale) 的双层结构），同时不发生应用写屏。
        let mut ctx = TestContext::new(1);
        // 准备：登记意图（启用 filter + icc + 自定义 icc_ramp，模拟真实启用路径）。
        let gen = ctx.ops.submit(0, |s| {
            s.filter_active = true;
            s.icc_active = true;
            s.active_icc_id = Some("builtin_xxx".to_string());
            s.icc_ramp = Some([[9u16; 256]; 3]);
            bump_operation_generation(s)
        }).unwrap();
        // 在 Capture 暂停点制造时序：apply 内部捕获期间被用户推进版本。
        ctx.arm_pause(Op::Capture);
        let ops = ctx.ops;
        let handle = thread::spawn(move || {
            apply_filter_if_current_with(&ops, 0, gen)
        });
        ctx.wait_paused();
        // 暂停期间推进版本（关闭意图）。
        let _ = ctx.ops.submit(0, |s| {
            s.filter_active = false;
            bump_operation_generation(s)
        });
        ctx.release_pause();
        // 结果必须精确为 Ok(RunResult::SkippedStale)——单层，不是 Ok(Executed(SkippedStale))。
        match handle.join().unwrap() {
            Ok(RunResult::SkippedStale) => {}
            Ok(RunResult::Executed(_)) => panic!("生产包装层必须返回单层 SkippedStale，不得为 Executed(...)"),
            Err(e) => panic!("不应报错: {}", e),
        }
        // 未发生 ApplyIcc 写屏。
        assert!(!ctx.log_order().contains(&Op::ApplyIcc),
                "不得发生应用写屏: {:?}", ctx.log_order());
    }

    // ─── 第 3A 收尾：资格竞争 / 在途关闭 / 晚到接管 ───

    // 场景 21：自动开启资格竞争（原子条件开启）
    // 自动逻辑预检查（读到关闭）后、条件开启提交前，用户先手动开启并推进版本；
    // 条件开启必须被拒绝（返回 None），不创建自动归属、不覆盖手动意图、不写屏。
    #[test]
    fn conditional_enable_rejected_when_user_opened_first() {
        let ctx = TestContext::new(1);
        // 模拟自动逻辑预检查：初始为关闭。
        assert_eq!(ctx.ops.with_state(0, |s| s.filter_active), Some(false));
        // 用户先手动开启（提交意图并推进版本到 g1）。
        let g1 = ctx.ops.submit(0, |s| {
            s.filter_active = true;
            bump_operation_generation(s)
        }).unwrap();
        // 自动逻辑的条件开启：当前已开启 → 必须拒绝，不得覆盖、不得推进版本。
        let g2 = conditional_set_active_with(&ctx.ops, 0, true);
        assert!(g2.is_none(), "用户已手动开启，条件开启必须返回 None");
        // 版本未被自动逻辑推进。
        assert_eq!(ctx.ops.with_state(0, |s| s.operation_generation), Some(g1));
        // 无后端调用（无 Capture/Write/Clear）。
        assert!(ctx.log_order().is_empty(), "被拒绝的条件开启不得写屏: {:?}", ctx.log_order());
        // 手动意图保留。
        assert_eq!(ctx.ops.with_state(0, |s| s.filter_active), Some(true));
    }

    // 场景 22：关闭自动功能时应用仍在途
    // 自动应用完成写屏（Executed）后、条件登记前，关闭自动功能（沿用旧版语义：
    // 效果保留、控制权移交用户）。关闭命令已将归属槽置为 None（abandon_auto_control
    // _on_disable 彻底释放 Applying），在途应用的条件登记因此被拒绝，不留残留。
    #[test]
    fn mark_applied_rejected_after_feature_disabled() {
        let ctx = TestContext::new(1);
        let g1 = ctx.ops.submit(0, |s| {
            s.filter_active = true;
            bump_operation_generation(s)
        }).unwrap();
        // 应用写屏已成功、版本仍当前（g1）。
        // 功能刚被关闭 → abandon_auto_control_on_disable 已把归属槽置 None。
        let slot: AutoOwnershipSlot = StdMutex::new(None);
        assert!(!auto_mark_applied(&slot, 1, &ctx.ops), "功能已关闭，不得登记 Applied");
        assert!(slot.lock().unwrap().is_none(), "不留半截归属");
    }

    // 场景 23：同一会话的晚到成功 + 用户已推进版本 → 条件登记拒绝并弃权
    // 自动应用写屏完成后、条件登记前，用户手动操作推进了版本（仍是同一会话归属，
    // 但显示状态已不属于该任务）。auto_mark_applied 必须返回 false 并条件弃权，
    // 不得登记 Applied——防止游戏结束时误关用户刚手动改过的滤镜。
    #[test]
    fn late_success_same_session_rejected_when_user_took_over() {
        let ctx = TestContext::new(1);
        let g1 = ctx.ops.submit(0, |s| {
            s.filter_active = true;
            bump_operation_generation(s)
        }).unwrap();
        let slot: AutoOwnershipSlot = StdMutex::new(Some(make_record(1, 0, g1, AutoSessionState::Applying)));
        // 用户手动操作推进版本到 g2（filter_active 仍 true）。
        let _g2 = ctx.ops.submit(0, |s| {
            s.filter_active = true;
            bump_operation_generation(s)
        }).unwrap();
        // 同一会话 1 的晚到成功：版本已过期 → 必须返回 false。
        assert!(!auto_mark_applied(&slot, 1, &ctx.ops), "版本已被用户推进，不得登记 Applied");
        // 条件弃权：不留半截归属。
        assert!(slot.lock().unwrap().is_none(), "归属必须被条件弃权清理");
    }

    // 场景 24：自动应用失败 → 回滚恢复也失败 → Restoring 保留供逐轮重试，
    // 调用方收尾不得再次释放保留的记录（审查第 2 点的核心）。
    #[test]
    fn auto_rollback_failure_keeps_restoring_and_caller_does_not_release() {
        let ctx = TestContext::new(1);
        // 自动开启提交版本 g1，登记 Applying 归属。
        let g1 = ctx.ops.submit(0, |s| {
            s.filter_active = true;
            bump_operation_generation(s)
        }).unwrap();
        ctx.ops.capture(0).unwrap();
        ctx.ops.set_restore_pending(0, true);
        let slot: AutoOwnershipSlot = StdMutex::new(Some(make_record(1, 0, g1, AutoSessionState::Applying)));
        // 应用失败后触发回滚：条件关闭成功（版本仍当前 g1），转 Restoring 执行恢复；
        // 恢复写回失败 → 必须返回 RestoreFailed，不得伪装成成功。
        *ctx.fail.lock().unwrap() = Some(Op::Write);
        let outcome = auto_rollback_failed_apply(&slot, 1, &ctx.ops);
        match outcome {
            RollbackOutcome::RestoreFailed(_) => {}
            other => panic!("恢复失败必须报告 RestoreFailed，实际 {:?}", other),
        }
        // slot 必须保留 Restoring 记录 + restore_generation（重试责任未丢失）。
        let rec = slot.lock().unwrap().clone().expect("回滚恢复失败后归属必须保留");
        assert_eq!(rec.state, AutoSessionState::Restoring, "失败后必须为 Restoring");
        assert!(rec.restore_generation.is_some(), "restore_generation 必须保留以重试");
        // 调用方收尾：生产路径（game_filter Err 分支）在 RestoreFailed 分支**不调用**
        // auto_release_if_owned，记录因此保留。这里直接验证记录未被清除。
        assert!(slot.lock().unwrap().is_some(), "调用方收尾不得清除保留的恢复记录");
        // 重试实际执行：解除失败，restore_display_default_if_current 恢复成功 → 收尾清空。
        *ctx.fail.lock().unwrap() = None;
        let rgen = rec.restore_generation.expect("已有恢复版本");
        let r = restore_display_default_if_current_with(&ctx.ops, 0, rgen).unwrap();
        assert!(matches!(r, RunResult::Executed(RestoreOutcome::Restored)), "重试必须精确恢复");
        assert!(auto_finish_restore(&slot, 1), "恢复成功后必须收尾清空归属");
        assert!(slot.lock().unwrap().is_none());
        // restore_pending 清除、屏幕恢复。
        assert_eq!(ctx.ops.with_state(0, |s| s.restore_pending), Some(false));
        assert_eq!(*ctx.screen.lock().unwrap(), [[1u16; 256]; 3], "屏幕必须恢复为原始 ramp");
    }

    // 场景 25：自动回滚 Superseded（新意图接管）→ 不写屏、不修改新会话状态。
    #[test]
    fn auto_rollback_superseded_does_not_touch_new_intent() {
        let ctx = TestContext::new(1);
        let g1 = ctx.ops.submit(0, |s| {
            s.filter_active = true;
            bump_operation_generation(s)
        }).unwrap();
        let slot: AutoOwnershipSlot = StdMutex::new(Some(make_record(1, 0, g1, AutoSessionState::Applying)));
        // 新意图提交，版本推进到 g2。
        let g2 = ctx.ops.submit(0, |s| {
            s.filter_active = true;
            bump_operation_generation(s)
        }).unwrap();
        *slot.lock().unwrap() = Some(make_record(2, 0, g2, AutoSessionState::Applying));
        // 旧会话 1 的回滚：版本已过期 → Superseded，不写屏。
        let outcome = auto_rollback_failed_apply(&slot, 1, &ctx.ops);
        assert_eq!(outcome, RollbackOutcome::Superseded, "旧会话回滚必须 Superseded");
        // 新会话 2 的归属保留、状态未动。
        let rec = slot.lock().unwrap().clone().expect("会话 2 归属必须保留");
        assert_eq!(rec.session, 2);
        assert_eq!(rec.state, AutoSessionState::Applying);
        assert_eq!(ctx.ops.with_state(0, |s| s.filter_active), Some(true));
        assert!(ctx.log_order().is_empty(), "Superseded 回滚不得写屏: {:?}", ctx.log_order());
    }

    // 场景 26：操作锁身份在显示器数量缩减再增加后保持不变
    // 执行器会克隆 Arc 锁句柄；若按数量截断再重建，同一槽位会出现两把锁，
    // 破坏同槽位串行保证。用**生产共用函数** grow_operation_locks 维护，
    // 覆盖会被截断的 2 号槽位：初始 3 → 缩减 1 → 再增长 3，2 号槽位必须仍是原 Arc。
    #[test]
    fn operation_lock_identity_survives_shrink_and_grow() {
        // 初始 3 台显示器（生产逻辑增长）。
        let mut locks: Vec<Arc<Mutex<()>>> = Vec::new();
        grow_operation_locks(&mut locks, 3);
        // 持有 2 号槽位旧锁句柄（任务 A 正在执行/等待）。
        let old_lock = locks[2].clone();
        // 模拟显示器数量缩减到 1：新实现只增长，不截断，2 号槽位保留。
        grow_operation_locks(&mut locks, 1);
        assert_eq!(locks.len(), 3, "只增长不截断：数量不得因请求 1 而缩小");
        // 再增长回 3。
        grow_operation_locks(&mut locks, 3);
        assert!(Arc::ptr_eq(&locks[2], &old_lock),
                "2 号槽位锁身份必须保持不变（旧实现 truncate(1) 会在这里重建新锁）");
        // 串行保证：同一锁句柄下互斥仍成立。
        let _g = old_lock.lock().unwrap();
        let second = locks[2].clone();
        let try_second = second.try_lock();
        assert!(try_second.is_err(), "同槽位不得并行进入物理操作");
    }

    // ─── 控制状态同步边界：关闭与登记竞争 ───
    // 用独立 Mutex<AutoControlState> + 独立 slot 驱动与生产共用的
    // auto_register_owned_with / auto_update_control_state_with。

    // 测试 A：关闭先完成，旧登记随后到达 → 必须拒绝。
    // 旧任务持有 expected_generation=N；关闭流程已更新状态并处置归属；
    // 旧登记在锁内比较代次 → 拒绝，不推进版本、不写 Applying、不写屏。
    #[test]
    fn registration_rejected_after_disable_completes_first() {
        let ctx = TestContext::new(1);
        // 初始控制状态：enabled=true, generation=7（旧线程持 N=7）。
        let state: StdMutex<AutoControlState> = StdMutex::new(AutoControlState { enabled: true, generation: 7 });
        let slot: AutoOwnershipSlot = StdMutex::new(None);
        let session_counter = AtomicU64::new(0);
        // 旧线程持有代次 7（尚未取得控制锁）。
        let expected_generation = 7u64;
        // 关闭流程先完成：enabled=false, generation→8（代次推进使旧线程失效）。
        let update = {
            let guard = state.lock().unwrap();
            auto_update_control_state_with(guard, &slot, false).expect("状态变化必须返回更新")
        };
        assert!(!update.0, "无 Restoring 归属");
        assert_eq!(update.1, 8);
        // 旧登记随后到达：锁内读取 enabled=false → 拒绝。
        let guard = state.lock().unwrap();
        let result = auto_register_owned_with(
            guard, 0, expected_generation, &session_counter, &slot, &ctx.ops,
            |i| Some((format!("DEV{}", i), 1)), None,
        );
        assert!(result.is_none(), "关闭后旧登记必须被拒绝");
        // 操作版本未因旧登记推进；无 Applying 记录；无写屏。
        assert_eq!(ctx.ops.with_state(0, |s| s.operation_generation), Some(0));
        assert!(slot.lock().unwrap().is_none(), "不得创建 Applying 记录");
        assert!(ctx.log_order().is_empty(), "不得写屏: {:?}", ctx.log_order());
    }

    // 测试 B：关闭后重开，旧代次不能登记；当前代次可正常登记。
    #[test]
    fn stale_generation_rejected_after_reopen() {
        let ctx = TestContext::new(1);
        // 初始 enabled=true, generation=7（旧线程持 N=7）。
        let state: StdMutex<AutoControlState> = StdMutex::new(AutoControlState { enabled: true, generation: 7 });
        let slot: AutoOwnershipSlot = StdMutex::new(None);
        let session_counter = AtomicU64::new(0);
        let stale_generation = 7u64;
        // 关闭（generation→8）→ 重新开启（generation→9，enabled=true）。
        {
            let guard = state.lock().unwrap();
            auto_update_control_state_with(guard, &slot, false).unwrap();
        }
        let reopen = {
            let guard = state.lock().unwrap();
            auto_update_control_state_with(guard, &slot, true).unwrap()
        };
        assert!(reopen.0 == false, "无 Restoring 归属");
        let current_generation = reopen.1; // 9
        // 旧线程（N=7）尝试登记：即使当前 enabled=true，代次不匹配 → 拒绝。
        let guard = state.lock().unwrap();
        let stale = auto_register_owned_with(
            guard, 0, stale_generation, &session_counter, &slot, &ctx.ops,
            |i| Some((format!("DEV{}", i), 1)), None,
        );
        assert!(stale.is_none(), "旧代次即使 enabled=true 也必须被拒绝");
        // 当前代次（9）的合法登记成功。
        let guard = state.lock().unwrap();
        let current = auto_register_owned_with(
            guard, 0, current_generation, &session_counter, &slot, &ctx.ops,
            |i| Some((format!("DEV{}", i), 1)), None,
        )
            .expect("当前代次登记必须成功");
        assert_eq!(current.1, 1, "第一个会话 id=1");
        // 成功登记必须推进传入 ctx.ops 的版本，且归属记录版本与它一致。
        let state_version = ctx.ops.with_state(0, |s| s.operation_generation).unwrap();
        assert_eq!(state_version, current.2, "归属记录的操作版本必须与状态锁内版本一致");
        // 旧任务不能覆盖当前会话归属。
        let guard = state.lock().unwrap();
        let stale_again = auto_register_owned_with(
            guard, 0, stale_generation, &session_counter, &slot, &ctx.ops,
            |i| Some((format!("DEV{}", i), 1)), None,
        );
        assert!(stale_again.is_none(), "旧代次不得覆盖当前会话归属");
        let rec = slot.lock().unwrap().clone().expect("当前会话归属保留");
        assert_eq!(rec.session, 1);
        // 未发生写屏（登记本身不写屏；版本推进因条件开启发生一次）。
        assert!(ctx.log_order().is_empty(), "登记不得写屏: {:?}", ctx.log_order());
    }

    // 测试 C：登记通过控制状态检查后、条件开启前暂停（仍持控制锁）→
    // 关闭线程被阻塞；放行登记完成后关闭才完成，无关闭后的重新登记。
    #[test]
    fn disable_blocked_until_registration_completes() {
        let ctx = TestContext::new(1);
        // 独立控制状态（enabled=true, generation=1）与独立归属槽。
        let state: StdMutex<AutoControlState> = StdMutex::new(AutoControlState { enabled: true, generation: 1 });
        let slot: AutoOwnershipSlot = StdMutex::new(None);
        let session_counter = AtomicU64::new(0);
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel::<()>();
        // 登记线程：持控制锁，在条件开启前停在钩子（锁未释放）。
        let state_c = std::sync::Arc::new(state);
        let slot_c = std::sync::Arc::new(slot);
        let session_c = std::sync::Arc::new(session_counter);
        let ctx_ops = ctx.ops;
        let register_handle = {
            let state = std::sync::Arc::clone(&state_c);
            let slot = std::sync::Arc::clone(&slot_c);
            let counter = std::sync::Arc::clone(&session_c);
            let entered_tx = entered_tx.clone();
            thread::spawn(move || {
                let guard = state.lock().unwrap();
                auto_register_owned_with(
                    guard, 0, 1, &counter, &slot, &ctx_ops,
                    |i| Some((format!("DEV{}", i), 1)),
                    Some(Box::new(move || {
                        // 超时或通道断开不得自动放行登记——必须使测试失败。
                        entered_tx.send(()).expect("entered 信号发送失败");
                        release_rx
                            .recv_timeout(Duration::from_secs(10))
                            .expect("登记钩子等待放行超时或通道断开：测试失败");
                    })),
                )
            })
        };
        // 等登记线程进入钩子（条件开启前、仍持控制锁、版本未推进）。
        entered_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("登记线程必须进入钩子");
        assert_eq!(ctx.ops.with_state(0, |s| s.operation_generation), Some(0),
                   "钩子在条件开启前，版本不得推进");
        // 用 try_lock 证明登记线程确实持有控制锁（WouldBlock），而非依赖调度推测。
        assert!(matches!(
            state_c.try_lock(),
            Err(std::sync::TryLockError::WouldBlock)
        ), "登记线程必须仍持有控制锁");
        // 关闭线程：通过信号确认它已到达取锁前，再断言其被阻塞。
        let (close_started_tx, close_started_rx) = std::sync::mpsc::channel::<()>();
        let close_started_tx_c = close_started_tx.clone();
        let (close_done_tx, close_done_rx) = std::sync::mpsc::channel::<()>();
        let slot_for_close = std::sync::Arc::clone(&slot_c);
        let state_for_close = std::sync::Arc::clone(&state_c);
        let close_handle = thread::spawn(move || {
            close_started_tx_c.send(()).expect("close started 信号发送失败");
            let guard = state_for_close.lock().unwrap();
            auto_update_control_state_with(guard, &slot_for_close, false);
            close_done_tx.send(()).expect("close done 信号发送失败");
        });
        close_started_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("关闭线程必须到达取锁前");
        // 关闭线程已尝试取锁：登记仍持锁 → 关闭不得完成。
        assert!(matches!(
            close_done_rx.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ), "关闭不得在登记完成前取得控制锁");
        // 放行登记（send 必须成功）。
        release_tx.send(()).expect("release 信号发送失败");
        let registered = register_handle
            .join()
            .expect("登记线程不得 panic")
            .expect("登记必须成功（代次有效）");
        assert_eq!(registered.1, 1, "会话 id=1");        // 登记完成后，关闭线程取得控制锁并完成（close done 信号到达）。
        close_handle.join().unwrap();
        close_done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("关闭必须在登记完成后完成");
        // 最终：开关状态已关闭（直接断言 enabled==false）、非 Restoring 归属已释放、
        // 无关闭完成后的重新登记。
        assert!(!state_c.lock().unwrap().enabled, "关闭后控制状态 enabled 必须为 false");
        assert!(slot_c.lock().unwrap().is_none(), "关闭处置必须清除非 Restoring 归属");
        // 登记推进了 ctx.ops 的版本（条件开启发生一次）；无写屏。
        assert_ne!(ctx.ops.with_state(0, |s| s.operation_generation), Some(0), "登记必须推进版本");
        assert!(ctx.log_order().is_empty(), "登记与关闭都不得写屏: {:?}", ctx.log_order());
    }


    // 场景：同型号同端口替换，系统身份依据无法区分（设备路径+目标 id 全同的
    // 两个槽位）→ 解析必须拒绝（Ambiguous），不写屏。

    // 场景：同型号但设备路径不同（身份可区分）→ 解析唯一成功。

    // 场景：索引重排后按身份解析到正确设备（不依赖索引）。

    // 场景：无关设备增减不影响原目标解析。

    // 场景：目标消失 → Gone；重现且身份匹配 → 解析成功。

    // 场景：身份依据不足（设备路径为空）→ 拒绝，不按型号/端口猜测。

    // 场景：共享显示源（同一 CCD source、不同 identity）→ Ambiguous 拒绝独立写屏。

    // 场景：用户接管（自动会话过期）不删除原始校色数据——恢复记录管理函数。

    // 场景：无原始 Ramp 的降级清除场景，身份仍保留（恢复管理函数）。

    // 场景：新目标进入不覆盖旧记录（调用登记函数）。

    // 场景：目标消失——保留原记录，不转交给替代设备（调用管理函数）。

    // 场景：新操作继续使用原目标——不覆盖最初捕获的原始 Ramp。

    // ─── 收尾 1：唯一匹配 vs 跨拓扑恢复授权 ───
    // 快照 1 只有 A（身份字段 X）；A 被移除；快照 2 只有替代设备 B（系统仍提供 X）。
    // 任何单一快照都没有两个 X——唯一匹配成功，但跨拓扑恢复**不得授权**（无连续性
    // 观察且无更强身份依据 → Unconfirmed，不写屏）。

    // ─── 收尾 2：快照版本接入捕获核验 + 空寻址拒绝 ───
    // 解析目标 → 开始捕获 → 拓扑变化 → 捕获返回：旧捕获结果不被接受为有效
    // 恢复记录，不执行后续应用（版本核验，不接真实写屏）。

    // 空 gdi_path：唯一身份匹配但无法寻址 → 拒绝返回可写目标。

    // ─── 收尾 4：ICC 离线格式样本（当前解析行为 vs 未来校准写屏策略）───
    // 构造最小 ICC 文件：128 字节头部 + tag 表 + 标签数据。
    // 当前 parse_icc_file 行为：vcgt 失败回退 TRC（有告警）。
    // 未来 GDI 校准写屏策略：未批准该回退，需要独立严格提取（本批不接入）。

    /// 构造最小 ICC：头部 + 1 个标签。
    fn make_min_icc(tag: &[u8; 4], tag_data: &[u8]) -> Vec<u8> {
        let data_off = 132u32 + 12; // tag 表从 132，数据紧随其后（144）。
        let mut buf = Vec::new();
        // 头部 128 字节：大小(0)、magic(36)、... 其余填充 0。
        buf.resize(128, 0u8);
        buf[0..4].copy_from_slice(&((data_off + tag_data.len() as u32)).to_be_bytes());
        buf[36..40].copy_from_slice(b"acsp");
        // 标签表（128 起）：tag_count=1（用 extend 追加，不做越界切片）。
        buf.extend_from_slice(&1u32.to_be_bytes());
        buf.extend_from_slice(tag);
        buf.extend_from_slice(&data_off.to_be_bytes());   // ← 数据偏移（不是 132）
        buf.extend_from_slice(&(tag_data.len() as u32).to_be_bytes());
        // 标签数据从 144 起。
        buf.extend_from_slice(tag_data);
        buf
    }

    /// vcgt 标签数据（formula_type=0, 3 通道, 256 项, u16BE）。
    fn make_vcgt_ramp(channel_val: u16) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"vcgt");
        v.extend_from_slice(&0u32.to_be_bytes()); // reserved
        v.extend_from_slice(&0u32.to_be_bytes()); // formula_type=0
        v.extend_from_slice(&3u16.to_be_bytes()); // channels
        v.extend_from_slice(&256u16.to_be_bytes()); // entries
        v.extend_from_slice(&2u16.to_be_bytes()); // entry_size
        for _ in 0..3 * 256 {
            v.extend_from_slice(&channel_val.to_be_bytes());
        }
        v
    }

    /// curv 标签数据（count=1 gamma 表，作为 TRC）。
    fn make_curv_trc(gamma_fixed: u16) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(b"curv");
        v.extend_from_slice(&0u32.to_be_bytes()); // reserved
        v.extend_from_slice(&1u32.to_be_bytes()); // count=1
        v.extend_from_slice(&gamma_fixed.to_be_bytes()); // gamma (16.16? 实际 u16/256)
        v
    }

    // 场景：合法 vcgt（formula=0, 3×256×u16）→ 解析成功，ramp 为 vcgt 值。
    #[test]
    fn icc_valid_vcgt_parses() {
        let dir = std::env::temp_dir().join("nexbox_icc_test_valid");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("valid_vcgt.icc");
        let vcgt = make_vcgt_ramp(0x0102);
        std::fs::write(&path, make_min_icc(b"vcgt", &vcgt)).unwrap();
        let preset = parse_icc_file(path.to_str().unwrap()).expect("合法 vcgt 必须解析成功");
        assert_eq!(preset.ramp.len(), 3);
        assert_eq!(preset.ramp[0][0], 0x0102, "vcgt ramp 值正确");
        let _ = std::fs::remove_file(&path);
    }

    // 场景：非支持 vcgt（formula_type=1）+ 合法 TRC → 当前行为回退 TRC（有告警），
    // 不报错。这是**当前导入行为**；未来校准写屏是否允许该回退需独立策略（未批准）。
    #[test]
    fn icc_unsupported_vcgt_falls_back_to_trc() {
        let dir = std::env::temp_dir().join("nexbox_icc_test_fb");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("fallback.icc");
        // 非支持 vcgt：formula_type=1。
        let mut vcgt_bad = Vec::new();
        vcgt_bad.extend_from_slice(b"vcgt");
        vcgt_bad.extend_from_slice(&0u32.to_be_bytes());
        vcgt_bad.extend_from_slice(&1u32.to_be_bytes()); // formula_type=1 不支持
        vcgt_bad.extend_from_slice(&3u16.to_be_bytes());
        vcgt_bad.extend_from_slice(&256u16.to_be_bytes());
        vcgt_bad.extend_from_slice(&2u16.to_be_bytes());
        // 两个标签：vcgt + rTRC（合法 curv）。
        let trc = make_curv_trc(0x0100); // gamma=1.0
        // 回退路径需要 rTRC/gTRC/bTRC 三个（parse_icc_file 读三个通道）。
        let tag_count = 4u32;
        let data_off = 132u32 + tag_count * 12; // 132 + 48 = 180
        let vcgt_off = data_off;
        let r_trc_off = data_off + vcgt_bad.len() as u32;
        let g_trc_off = r_trc_off + trc.len() as u32;
        let b_trc_off = g_trc_off + trc.len() as u32;
        let mut buf = Vec::new();
        buf.resize(128, 0u8);
        buf[0..4].copy_from_slice(&(b_trc_off + trc.len() as u32).to_be_bytes());
        buf[36..40].copy_from_slice(b"acsp");
        buf.extend_from_slice(&tag_count.to_be_bytes());
        // 标签 1: vcgt
        buf.extend_from_slice(b"vcgt");
        buf.extend_from_slice(&vcgt_off.to_be_bytes());
        buf.extend_from_slice(&(vcgt_bad.len() as u32).to_be_bytes());
        // 标签 2-4: rTRC/gTRC/bTRC
        for (sig, off) in [(b"rTRC", r_trc_off), (b"gTRC", g_trc_off), (b"bTRC", b_trc_off)] {
            buf.extend_from_slice(sig);
            buf.extend_from_slice(&off.to_be_bytes());
            buf.extend_from_slice(&(trc.len() as u32).to_be_bytes());
        }
        buf.extend_from_slice(&vcgt_bad);
        buf.extend_from_slice(&trc);
        buf.extend_from_slice(&trc);
        buf.extend_from_slice(&trc);
        std::fs::write(&path, &buf).unwrap();
        // 当前行为：vcgt formula=1 不支持 → 回退 rTRC（curv count=1 gamma=1.0）→ 成功。
        let preset = parse_icc_file(path.to_str().unwrap()).expect("当前行为回退 TRC 应成功（有告警）");
        // gamma=1.0 → ramp[i] = (i/255)^1.0 * 65535。
        assert_eq!(preset.ramp[0][255], 65535, "回退 TRC gamma=1.0 末端为 65535");
        let _ = std::fs::remove_file(&path);
    }

    // 场景：缺失校准标签（无 vcgt 无 TRC）→ 报错，不静默生成效果。
    #[test]
    fn icc_missing_calibration_tags_errors() {
        let dir = std::env::temp_dir().join("nexbox_icc_test_missing");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("missing.icc");
        // 仅一个无关标签（如 desc），无 vcgt/TRC。
        let mut buf = Vec::new();
        buf.resize(128, 0u8);
        buf[36..40].copy_from_slice(b"acsp");
        // tag_count=1（extend 追加，不做越界切片）。
        buf.extend_from_slice(&1u32.to_be_bytes());
        buf.extend_from_slice(b"desc");
        buf.extend_from_slice(&144u32.to_be_bytes()); // 数据偏移 144
        buf.extend_from_slice(&4u32.to_be_bytes());
        buf.extend_from_slice(b"none");
        let total = 144 + 4;
        buf[0..4].copy_from_slice(&(total as u32).to_be_bytes());
        std::fs::write(&path, &buf).unwrap();
        let r = parse_icc_file(path.to_str().unwrap());
        assert!(r.is_err(), "缺失校准标签必须报错，不静默生成效果");
        let _ = std::fs::remove_file(&path);
    }

    // 场景：截断数据（vcgt 数据不足）→ 报错或回退，不 panic。
    #[test]
    fn icc_truncated_vcgt_no_panic() {
        let dir = std::env::temp_dir().join("nexbox_icc_test_trunc");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("truncated.icc");
        // vcgt 声明 3×256×u16 但数据不足。
        let mut vcgt = Vec::new();
        vcgt.extend_from_slice(b"vcgt");
        vcgt.extend_from_slice(&0u32.to_be_bytes());
        vcgt.extend_from_slice(&0u32.to_be_bytes());
        vcgt.extend_from_slice(&3u16.to_be_bytes());
        vcgt.extend_from_slice(&256u16.to_be_bytes());
        vcgt.extend_from_slice(&2u16.to_be_bytes());
        vcgt.extend_from_slice(&[0u8; 10]); // 只给 10 字节，远不足
        // 截断到恰好缺尾巴
        std::fs::write(&path, make_min_icc(b"vcgt", &vcgt[..vcgt.len() - 5])).unwrap();
        // 不 panic：Err（vcgt 解析失败且无 TRC → 整体 Err）或 Ok 都接受，只断言不崩溃。
        let r = parse_icc_file(path.to_str().unwrap());
        assert!(r.is_ok() || r.is_err(), "截断数据不得 panic");
        let _ = std::fs::remove_file(&path);
    }

    // ─── 收尾 2：假后端驱动的捕获前后核验共用流程 ───
    // 用共用函数 capture_with_verify：解析目标 → 读取 Ramp → 再取当前拓扑核验。
    // 假后端读取期间切换快照（版本推进）→ 返回拒绝，不提交、不覆盖既有记录。

    // ─── 收尾 3：条件清理边界 ───
    // record_on_restore_success_if_current：旧记录的恢复结果不能清掉后来
    // 替换或重新绑定的记录。
}
