use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicUsize, Ordering};
use std::sync::Mutex;
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
    /// 是否处于多滤镜叠加模式（已应用叠加组合）
    stacked: bool,
    /// 已应用的叠加组合（应用顺序，即卡片点选顺序）
    stack_preset_ids: Vec<String>,
}

impl Default for DisplayState {
    fn default() -> Self {
        Self {
            temperature: 6500, brightness: 100, contrast: 100, saturation: 100,
            r_gamma: 1.0, g_gamma: 1.0, b_gamma: 1.0, mode: 0,
            icc_ramp: None, icc_active: false, active_icc_id: None, filter_active: false,
            stacked: false, stack_preset_ids: Vec::new(),
        }
    }
}

static DISPLAY_STATES: Mutex<Option<Vec<Mutex<DisplayState>>>> = Mutex::new(None);
static ACTIVE_DISPLAY_INDEX: AtomicUsize = AtomicUsize::new(0);

/// 首次应用滤镜前捕获的原始硬件 gamma ramp（按显示器 index 对齐，与 DISPLAY_STATES 键位一致）。
/// 退出/禁用时据此精确恢复，而不是用 `xcalib -c` 清成线性——那会把图形控制台 /
/// 系统颜色管理里设置的 sRGB 校色一并抹掉。
static ORIGINAL_RAMPS: Mutex<Vec<Mutex<Option<[[u16; 256]; 3]>>>> = Mutex::new(Vec::new());

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

/// 确保 ORIGINAL_RAMPS 与显示器数量对齐；新增的槽位默认为 None（尚未捕获）。
fn ensure_original_ramps(count: usize) {
    let mut lock = ORIGINAL_RAMPS.lock().unwrap();
    if lock.len() > count {
        lock.truncate(count);
    } else {
        while lock.len() < count {
            lock.push(Mutex::new(None));
        }
    }
}

/// 读取指定显示器的滤镜是否开启（供 game_filter 模块使用）
pub(crate) fn is_filter_active(idx: usize) -> bool {
    with_display_state(idx, |state| state.filter_active)
}

/// 设置指定显示器的滤镜开关状态（供 game_filter 模块使用）
pub(crate) fn set_filter_active(idx: usize, active: bool) {
    with_display_state(idx, |state| state.filter_active = active);
}

pub(crate) fn get_active_index() -> usize {
    let idx = ACTIVE_DISPLAY_INDEX.load(Ordering::SeqCst);
    ensure_display_states();
    let lock = DISPLAY_STATES.lock().unwrap();
    let states = lock.as_ref().unwrap();
    idx.min(states.len() - 1)
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
fn get_temp_icc_path() -> PathBuf {
    let config_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    let temp_dir = config_dir.join("NexBox").join("temp");
    let _ = fs::create_dir_all(&temp_dir);
    temp_dir.join("custom_filter.icc")
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

/// Apply filter: generates ICC from params (via icc_gen or build_icc_profile) and applies via xcalib.
pub(crate) fn apply_filter_to_display(idx: usize) -> Result<(), String> {
    // 首次应用前捕获原始硬件 ramp（每个显示器只捕获一次）。退出/禁用时据此精确
    // 恢复，避免 `xcalib -c` 把图形控制台/颜色管理里的 sRGB 校色清成线性。
    capture_original_ramp(idx);

    // 多滤镜叠加模式：优先走叠加组合（ramp 复合 → ICC → xcalib）
    {
        let stacked = with_display_state(idx, |s| s.stacked && !s.stack_preset_ids.is_empty());
        if stacked {
            log::info!("apply_filter_to_display[{}]: 叠加模式，应用滤镜组合", idx);
            return apply_stack_to_display(idx);
        }
    }

    let (icc_active, temperature, brightness, contrast, saturation, r_gamma, g_gamma, b_gamma, mode, _icc_ramp_opt) =
        with_display_state(idx, |state| {
            (state.icc_active, state.temperature, state.brightness, state.contrast,
             state.saturation, state.r_gamma, state.g_gamma, state.b_gamma,
             state.mode, state.icc_ramp.clone())
        });

    if icc_active {
        // ICC mode active — re-apply the stored ICC.
        // Necessary when the filter was toggled OFF when the preset was selected:
        // the ICC was recorded in state but never applied to the display.
        let active_id = with_display_state(idx, |s| s.active_icc_id.clone());
        log::info!("apply_filter_to_display[{}]: ICC mode active (id={:?}), re-applying ICC", idx, active_id);

        if let Some(ref id) = active_id {
            if let Some(filename) = id.strip_prefix("builtin_") {
                let icc_filename = format!("{}.icc", filename);
                if let Ok(icc_path) = get_builtin_icc_path(&icc_filename) {
                    return apply_icc_via_xcalib(&icc_path, idx);
                }
                log::warn!("apply_filter_to_display[{}]: builtin ICC '{}' not found", idx, icc_filename);
            }
            // User-imported ICC — re-apply from stored ramp
            if let Some(ramp) = with_display_state(idx, |s| s.icc_ramp.clone()) {
                let icc_data = build_icc_profile(&ramp, "NexBox ICC Preset");
                let temp_icc = get_temp_icc_path().with_file_name(format!("icc_reapply_{}.icc", id));
                if let Err(e) = fs::write(&temp_icc, &icc_data) {
                    log::error!("apply_filter_to_display[{}]: failed to write temp ICC: {}", idx, e);
                    return Err(format!("无法写入临时 ICC 文件: {}", e));
                }
                return apply_icc_via_xcalib(&temp_icc, idx);
            }
        }
        return Ok(());
    }

    // Detect truly neutral / identity parameters.
    // When the custom filter is reset to defaults and saved, clear the gamma
    // ramp to system default (xcalib -c).  Do NOT apply any specific ICC.
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
        return restore_display_default(idx);
    }

    let temp_icc = get_temp_icc_path();

    // Build ICC entirely in Rust — we control the colour-temperature formula
    // (kelvin_to_rgb_multipliers is fixed to return identity at 6500 K).
    // Avoid icc_gen.exe whose internal algorithm may produce non-neutral or
    // over-brightened output.
    let mode_enum = FilterMode::from_i32(mode);
    let custom_gamma = Some((r_gamma, g_gamma, b_gamma));
    let ramp = build_gamma_ramp(temperature, brightness, contrast, saturation, mode_enum, custom_gamma);
    let icc_data = build_icc_profile(&ramp, "NexBox Custom Filter");
    fs::write(&temp_icc, &icc_data).map_err(|e| format!("无法写入临时 ICC 文件: {}", e))?;
    apply_icc_via_xcalib(&temp_icc, idx)
}

/// Restore display to default. Prefer restoring the captured original gamma ramp
/// (preserves the console's sRGB calibration); fall back to xcalib -c (linear).
pub(crate) fn restore_display_default(idx: usize) -> Result<(), String> {
    if let Some(ramp) = take_original_ramp(idx) {
        log::info!("restore_display_default[{}]: 恢复应用前的原始 gamma ramp（进程内）", idx);
        return match write_gamma_ramp(idx, &ramp) {
            Ok(()) => Ok(()),
            Err(e) => {
                log::error!("restore_display_default[{}]: 进程内恢复失败，回退 xcalib -c: {}", idx, e);
                clear_gamma_ramp_via_xcalib(idx)
            }
        };
    }
    log::info!("restore_display_default[{}]: 无捕获的原始 ramp，回退 xcalib -c", idx);
    clear_gamma_ramp_via_xcalib(idx)
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
        let idx = resolve_display_index(display_index);
        let temperature = temperature.clamp(1000, 10000);
        let brightness = brightness.clamp(50, 150);
        let contrast = contrast.clamp(50, 150);
        let saturation = saturation.clamp(50, 150);
        let mode = mode.clamp(0, 9);
        let r_gamma = r_gamma.unwrap_or(1.0).clamp(0.50, 2.00);
        let g_gamma = g_gamma.unwrap_or(1.0).clamp(0.50, 2.00);
        let b_gamma = b_gamma.unwrap_or(1.0).clamp(0.50, 2.00);

        with_display_state(idx, |state| {
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
        });

        let actually_active = with_display_state(idx, |s| s.filter_active);
        if actually_active {
            let idx_move = idx;
            tauri::async_runtime::spawn_blocking(move || apply_filter_to_display(idx_move))
                .await.map_err(|e| format!("Filter apply error: {}", e))??;
        }

        save_all_filter_states();

        Ok(FilterResult {
            success: true, message: "滤镜设置已更新".to_string(),
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
        let idx = resolve_display_index(display_index);
        let already_active = with_display_state(idx, |state| {
            if state.filter_active { true } else { state.filter_active = true; false }
        });

        if already_active {
            return Ok(with_display_state(idx, |state| FilterResult {
                success: true, message: "滤镜已处于启用状态".to_string(),
                settings: Some(FilterSettings::from_display_state(state)),
                preview_filter: None, preview_tint_color: None, preview_tint_opacity: None,
            }));
        }

        let idx_move = idx;
        tauri::async_runtime::spawn_blocking(move || apply_filter_to_display(idx_move))
            .await.map_err(|e| format!("Filter apply error: {}", e))??;

        save_all_filter_states();

        Ok(with_display_state(idx, |state| FilterResult {
            success: true, message: "滤镜已启用".to_string(),
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
        let idx = resolve_display_index(display_index);
        let was_active = with_display_state(idx, |state| {
            if !state.filter_active { false } else { state.filter_active = false; true }
        });

        if was_active {
            let idx_move = idx;
            if let Err(e) = tauri::async_runtime::spawn_blocking(move || restore_display_default(idx_move))
                .await.map_err(|e| format!("Filter restore error: {}", e))?
            { log::error!("恢复默认显示失败: {}", e); }
        }

        save_all_filter_states();

        Ok(with_display_state(idx, |state| FilterResult {
            success: true, message: "滤镜已禁用".to_string(),
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
        let idx = get_active_index();
        let is_active = with_display_state(idx, |state| state.filter_active);
        let result = if is_active {
            // Disable
            with_display_state(idx, |state| state.filter_active = false);
            if let Err(e) = restore_display_default(idx) {
                log::error!("恢复默认显示失败: {}", e);
            }
            Ok(with_display_state(idx, |state| FilterResult {
                success: true, message: "滤镜已禁用".to_string(),
                settings: Some(FilterSettings::from_display_state(state)),
                preview_filter: None, preview_tint_color: None, preview_tint_opacity: None,
            }))
        } else {
            // Enable
            with_display_state(idx, |state| state.filter_active = true);
            if let Err(e) = apply_filter_to_display(idx) {
                log::error!("应用滤镜失败: {}", e);
                with_display_state(idx, |state| state.filter_active = false);
                return Err(e);
            }
            Ok(with_display_state(idx, |state| FilterResult {
                success: true, message: "滤镜已启用".to_string(),
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

/// 应用叠加组合到显示器：按 state.stack_preset_ids 顺序取 ramp → 复合 → 写 ICC → xcalib。
/// 供 apply_filter_to_display（开关/启动/游戏自动开启）与 apply_filter_stack 命令共用。
fn apply_stack_to_display(idx: usize) -> Result<(), String> {
    let ids = with_display_state(idx, |s| s.stack_preset_ids.clone());
    if ids.is_empty() {
        return Err("叠加组合为空".to_string());
    }
    capture_original_ramp(idx);
    let mut ramps = Vec::with_capacity(ids.len());
    for id in &ids {
        match preset_id_to_ramp(id) {
            Some(ramp) => ramps.push(ramp),
            None => return Err(format!("叠加滤镜解析失败，找不到滤镜: {}", id)),
        }
    }
    let composed = compose_ramps(&ramps);
    let temp_icc = get_temp_icc_path();
    let icc_data = build_icc_profile(&composed, "NexBox Filter Stack");
    fs::write(&temp_icc, &icc_data).map_err(|e| format!("无法写入临时 ICC 文件: {}", e))?;
    apply_icc_via_xcalib(&temp_icc, idx)
}

/// Clear the gamma ramp via xcalib (reset to system default / linear).
/// 仅在无法恢复捕获的原始 ramp 时作为兜底；系统关机/注销时跳过（外部子进程会
/// 因运行库被拆除而初始化失败 0xc0000142）。
fn clear_gamma_ramp_via_xcalib(display_index: usize) -> Result<(), String> {
    if is_system_shutting_down() {
        log::info!("clear_gamma_ramp[{}]: 系统关机/注销中，跳过 xcalib 恢复", display_index);
        return Ok(());
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

/// 在应用滤镜前捕获原始 ramp（每个显示器只捕获一次，供退出/禁用时精确恢复）。
fn capture_original_ramp(idx: usize) {
    ensure_display_states(); // 确保 DISPLAY_DEVICES 已枚举，get_display_hdc 才能取到设备名
    ensure_original_ramps(display_count());
    let slot = ORIGINAL_RAMPS.lock().unwrap();
    let Some(cell) = slot.get(idx) else { return };
    let mut guard = cell.lock().unwrap();
    if guard.is_none() {
        if let Some(ramp) = read_gamma_ramp(idx) {
            log::info!("capture_original_ramp[{}]: 已捕获原始 gamma ramp（含用户 sRGB 校色）", idx);
            *guard = Some(ramp);
        } else {
            log::warn!("capture_original_ramp[{}]: 读取原始 ramp 失败，恢复时回退 xcalib -c", idx);
        }
    }
}

/// 取走指定显示器的原始 ramp（恢复后清空，下次启用时重新捕获）。
fn take_original_ramp(idx: usize) -> Option<[[u16; 256]; 3]> {
    ensure_display_states(); // 确保 DISPLAY_DEVICES 已枚举，get_display_hdc 才能取到设备名
    ensure_original_ramps(display_count());
    let slot = ORIGINAL_RAMPS.lock().unwrap();
    let cell = slot.get(idx)?;
    let mut guard = cell.lock().unwrap();
    guard.take()
}

#[tauri::command]
pub async fn apply_preset(
    display_index: Option<usize>, preset_id: String, is_active: bool,
) -> Result<FilterResult, String> {
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

            with_display_state(idx, |state| {
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
            });

            let actually_active = with_display_state(idx, |s| s.filter_active);
            if actually_active {
                let icc_path_clone = icc_path.clone();
                let idx_move = idx;
                // 不阻塞返回：在后台线程应用 ICC，避免切换预设时因等待 xcalib
                // 应用 gamma（显示器会短暂刷新）而导致 UI“卡一下”。
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = tauri::async_runtime::spawn_blocking(move || apply_icc_via_xcalib(&icc_path_clone, idx_move)).await {
                        log::error!("apply_preset[{}]: 后台应用 ICC 失败: {}", idx_move, e);
                    }
                });
            }

            let (preview_filter, preview_tint_color, preview_tint_opacity) = compute_icc_preview(&ramp_array);

            save_all_filter_states();

            return Ok(with_display_state(idx, |state| FilterResult {
                success: true,
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
        let idx = resolve_display_index(display_index);

        if preset_ids.is_empty() {
            let was_stacked = with_display_state(idx, |s| s.stacked);
            if was_stacked {
                with_display_state(idx, |s| {
                    s.stacked = false;
                    s.stack_preset_ids.clear();
                    s.icc_active = false;
                    s.active_icc_id = None;
                    s.icc_ramp = None;
                    s.filter_active = false;
                });
                let idx_move = idx;
                tauri::async_runtime::spawn_blocking(move || restore_display_default(idx_move))
                    .await.map_err(|e| format!("清除叠加失败: {}", e))??;
            }
            save_all_filter_states();
            return Ok(with_display_state(idx, |state| FilterResult {
                success: true,
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

        with_display_state(idx, |state| {
            state.stack_preset_ids = preset_ids.clone();
            state.stacked = true;
            state.icc_ramp = Some(composed);
            state.icc_active = false;
            state.active_icc_id = None;
            state.filter_active = true;
        });

        // 后台应用，避免等待 xcalib 导致 UI 卡顿
        let idx_move = idx;
        tauri::async_runtime::spawn(async move {
            if let Err(e) = tauri::async_runtime::spawn_blocking(move || apply_stack_to_display(idx_move)).await {
                log::error!("apply_filter_stack[{}]: 后台应用叠加滤镜失败: {}", idx_move, e);
            }
        });

        save_all_filter_states();

        Ok(with_display_state(idx, |state| FilterResult {
            success: true,
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
        ensure_display_states();
        ensure_original_ramps(display_count());
        let num_displays = {
            let lock = DISPLAY_STATES.lock().unwrap();
            let states = lock.as_ref().unwrap();
            for state_mutex in states.iter() {
                let mut state = state_mutex.lock().unwrap();
                state.filter_active = false;
                state.icc_active = false;
                state.active_icc_id = None;
            }
            states.len()
        };
        // 只恢复"实际应用过滤镜"的显示器（即捕获过原始 ramp 的），从未开过滤镜的
        // 显示器完全不动。恢复用进程内 SetDeviceGammaRamp，关机/注销时也不产生
        // 子进程；xcalib 兜底路径由 clear_gamma_ramp_via_xcalib 内部保护。
        let mut restored = 0usize;
        for i in 0..num_displays {
            let has_orig = ORIGINAL_RAMPS
                .lock().unwrap()
                .get(i)
                .map(|m| m.lock().unwrap().is_some())
                .unwrap_or(false);
            if has_orig {
                if let Err(e) = restore_display_default(i) {
                    log::error!("cleanup[{}]: 恢复原始 ramp 失败: {}", i, e);
                } else {
                    restored += 1;
                }
            }
        }
        log::info!("cleanup: restored {} displays to pre-application state", restored);
    }
}

// ─── Startup restore ───

/// Called once on app startup to restore the filter state from the previous session.
/// If `auto_apply` is true the saved preset/ICC is applied immediately (toggle ON,
/// ICC applied to display).  Otherwise only the in-memory state is restored and the
/// frontend highlights the correct card with the toggle OFF.
#[tauri::command]
pub async fn restore_filter_state(display_index: Option<usize>, auto_apply: bool) -> Result<FilterResult, String> {
    let idx = resolve_display_index(display_index);
    log::info!("restore_filter_state[{}]: auto_apply={}", idx, auto_apply);

    // Load saved state into memory (filter_active forced to false for now)
    restore_state_on_startup();

    if auto_apply {
        // Turn the filter ON and re-apply the saved preset/ICC
        with_display_state(idx, |s| s.filter_active = true);
        log::info!("restore_filter_state[{}]: auto-apply enabled, re-applying filter", idx);
        let idx_move = idx;
        tauri::async_runtime::spawn_blocking(move || apply_filter_to_display(idx_move))
            .await.map_err(|e| format!("Startup filter apply error: {}", e))??;
    }

    Ok(with_display_state(idx, |state| FilterResult {
        success: true,
        message: if auto_apply { "滤镜已自动开启" } else { "滤镜状态已恢复" }.to_string(),
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
            let temp_path = get_temp_icc_path().with_file_name(format!("icc_preset_{}.icc", id));
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

        with_display_state(idx, |state| {
            state.icc_ramp = Some(ramp_array);
            state.icc_active = true;
            state.active_icc_id = Some(id.clone());
            state.stacked = false;
            state.stack_preset_ids.clear();
            if is_active && !state.filter_active { state.filter_active = true; }
        });

        let actually_active = with_display_state(idx, |s| s.filter_active);
        if actually_active {
            let icc_path_clone = icc_path.clone();
            let idx_move = idx;
            // 不阻塞返回：后台应用 ICC，避免切换 ICC 预设时 UI 卡顿
            tauri::async_runtime::spawn(async move {
                if let Err(e) = tauri::async_runtime::spawn_blocking(move || apply_icc_via_xcalib(&icc_path_clone, idx_move)).await {
                    log::error!("apply_icc_preset[{}]: 后台应用 ICC 失败: {}", idx_move, e);
                }
            });
        }

        let (preview_filter, preview_tint_color, preview_tint_opacity) = compute_icc_preview(&ramp_array);

        save_all_filter_states();

        Ok(with_display_state(idx, |state| FilterResult {
            success: true, message: format!("ICC 预设已应用"),
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

        Ok(FilterResult { success: true, message: "ICC 预设已删除".to_string(), settings: None, preview_filter: None, preview_tint_color: None, preview_tint_opacity: None })
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
