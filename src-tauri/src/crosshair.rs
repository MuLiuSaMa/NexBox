use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use tauri::Emitter;
use tauri::Manager;

static CROSSHAIR_ACTIVE: AtomicBool = AtomicBool::new(false);
static CROSSHAIR_HANDLE: AtomicPtr<std::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());
/// 准星窗口线程句柄，start 时等待旧线程完全退出，防止快速启停时出现双窗口
static WINDOW_THREAD: Mutex<Option<thread::JoinHandle<()>>> = Mutex::new(None);
/// 准星启停生命周期锁，避免按下/松开边沿与手动开关并发触发
static LIFECYCLE_LOCK: Mutex<()> = Mutex::new(());

/// 当前准星是否处于激活状态（供 crosshair_hold 按住模式查询）
pub(crate) fn is_active() -> bool {
    CROSSHAIR_ACTIVE.load(Ordering::SeqCst)
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct CrosshairSettings {
    pub enabled: bool,
    pub style: String,
    pub size: i32,
    pub thickness: i32,
    pub color: String,
    pub gap: i32,
    pub dot_size: i32,
    pub opacity: u8,
    pub monitor_index: i32,
    pub monitor_device_name: Option<String>,
    pub use_custom_image: bool,
    pub custom_image_path: Option<String>,
    pub offset_x: i32,
    pub offset_y: i32,
    pub screen_width: i32,
    pub screen_height: i32,
    pub outline_enabled: bool,
    pub outline_color: String,
    pub outline_thickness: i32,
}

impl Default for CrosshairSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            style: "Cross".to_string(),
            size: 20,
            thickness: 2,
            color: "#ff0000".to_string(),
            gap: 0,
            dot_size: 2,
            opacity: 255,
            monitor_index: -1,
            monitor_device_name: None,
            use_custom_image: false,
            custom_image_path: None,
            offset_x: 0,
            offset_y: 0,
            screen_width: 0,
            screen_height: 0,
            outline_enabled: false,
            outline_color: "#000000".to_string(),
            outline_thickness: 1,
        }
    }
}

#[derive(serde::Serialize, Clone)]
pub struct DisplayInfo {
    pub index: usize,
    pub name: String,
    pub device_name: String,
    pub is_primary: bool,
    pub width: i32,
    pub height: i32,
}

#[derive(serde::Serialize, Debug)]
pub struct CrosshairPreset {
    pub name: String,
    pub path: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct CrosshairResult {
    pub success: bool,
    pub message: String,
}

static CURRENT_SETTINGS: Mutex<Option<CrosshairSettings>> = Mutex::new(None);

/// 读取当前生效的准星参数（供 crosshair_hold 按住模式使用）
pub(crate) fn get_settings() -> CrosshairSettings {
    let lock = CURRENT_SETTINGS.lock().unwrap();
    lock.as_ref().cloned().unwrap_or_default()
}

#[tauri::command]
pub async fn get_crosshair_displays() -> Result<Vec<DisplayInfo>, String> {
    #[cfg(target_os = "windows")]
    {
        let result = tauri::async_runtime::spawn_blocking(|| {
        use windows_sys::Win32::Graphics::Gdi::{
            EnumDisplayMonitors, GetMonitorInfoW,
            HDC, HMONITOR, MONITORINFOEXW,
        };

        struct MonitorData {
            displays: Vec<DisplayInfo>,
        }

        unsafe extern "system" fn monitor_enum_proc(
            hmonitor: HMONITOR,
            _hdc: HDC,
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
                let width = info.monitorInfo.rcMonitor.right - info.monitorInfo.rcMonitor.left;
                let height = info.monitorInfo.rcMonitor.bottom - info.monitorInfo.rcMonitor.top;

                let monitor_model = get_monitor_model_name(&device_name);
                let name = if !monitor_model.is_empty() {
                    format!("{} ({}x{})", monitor_model, width, height)
                } else {
                    format!("{} ({}x{})", device_name, width, height)
                };

                data.displays.push(DisplayInfo {
                    index: data.displays.len(),
                    name,
                    device_name: device_name.clone(),
                    is_primary,
                    width,
                    height,
                });
            }
            1
        }

        let mut data = MonitorData {
            displays: Vec::new(),
        };

        unsafe {
            EnumDisplayMonitors(
                std::ptr::null_mut(),
                std::ptr::null(),
                Some(monitor_enum_proc),
                &mut data as *mut _ as isize,
            );
        }

        if data.displays.is_empty() {
            data.displays.push(DisplayInfo {
                index: 0,
                name: "DISPLAY1 (Primary)".to_string(),
                device_name: "DISPLAY1".to_string(),
                is_primary: true,
                width: 0,
                height: 0,
            });
        }

        // EDID 回退：EnumDisplayDevicesW 经常返回通用名称，尝试获取真实型号
        // 使用 PNP ID 精确匹配，消除枚举顺序不一致导致的名称错乱
        let is_fallback = |n: &str| n.starts_with('\\') || {
            let prefix = n.split(" (").next().unwrap_or(n);
            is_generic_monitor_name(prefix)
        };
        if data.displays.iter().any(|d| is_fallback(&d.name)) {
            let edid_by_pnp = get_edid_pnp_map();
            if !edid_by_pnp.is_empty() {
                for d in data.displays.iter_mut() {
                    if is_fallback(&d.name) {
                        if let Some(pnp_id) = get_pnp_id_for_device(&d.device_name) {
                            if let Some(edid_name) = edid_by_pnp.get(&pnp_id) {
                                d.name = format!("{} ({}x{})", edid_name, d.width, d.height);
                            }
                        }
                    }
                }
            }
        }

        Ok(data.displays)
        });
        result.await.map_err(|e| format!("枚举显示器失败: {}", e))?
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("此功能仅支持 Windows 系统".to_string())
    }
}

/// Check if a monitor name is a generic/placeholder (any language variant)
fn is_generic_monitor_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.contains("generic")
        || lower.contains("即插即用")
        || lower.contains("通用")
        || lower.contains("pnp")
        || lower.contains("standard monitor")
        || lower.contains("digital display")
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
                return String::new();
            }
        }
    }

    String::new()
}

/// 通过 EnumDisplayDevicesW 获取指定显示器的 PNP ID（如 "DELA409"）。
/// DeviceID 格式: "MONITOR\PNPID\..."
#[cfg(target_os = "windows")]
fn get_pnp_id_for_device(device_name: &str) -> Option<String> {
    use std::mem;
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplayDevicesW, DISPLAY_DEVICEW};

    unsafe {
        let device_name_wide: Vec<u16> = device_name.encode_utf16().chain(std::iter::once(0)).collect();
        let mut disp_device: DISPLAY_DEVICEW = mem::zeroed();
        disp_device.cb = mem::size_of::<DISPLAY_DEVICEW>() as u32;

        if EnumDisplayDevicesW(device_name_wide.as_ptr(), 0, &mut disp_device, 0) != 0 {
            let len = disp_device.DeviceID.iter().position(|&c| c == 0).unwrap_or(disp_device.DeviceID.len());
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

/// 获取 `get_edid_monitor_names_by_pnpid()` 返回的名称映射，
/// 用于跨模块共享（crosshair 需要此映射消除对 `get_edid_monitor_names` 的索引依赖）。
#[cfg(target_os = "windows")]
fn get_edid_pnp_map() -> std::collections::HashMap<String, String> {
    crate::display_cache::get_edid_monitor_names_by_pnpid()
}
#[cfg(target_os = "windows")]
mod win32 {
    use windows_sys::Win32::Foundation::*;
    use windows_sys::Win32::Graphics::Gdi::*;
    use windows_sys::Win32::Graphics::GdiPlus::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    use windows_sys::Win32::UI::Accessibility::*;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use std::ptr;
    use std::sync::atomic::Ordering;
    use std::sync::Mutex;
    use std::result::Result::Ok;

    static GDIPLUS_TOKEN: Mutex<Option<usize>> = Mutex::new(None);
    static WIN_EVENT_HOOK: Mutex<Option<usize>> = Mutex::new(None);

    pub unsafe fn init_gdiplus() -> bool {
        let mut token = GDIPLUS_TOKEN.lock().unwrap();
        if token.is_some() {
            return true;
        }

        let mut input = GdiplusStartupInput {
            GdiplusVersion: 1,
            DebugEventCallback: 0,
            SuppressBackgroundThread: 0,
            SuppressExternalCodecs: 0,
        };

        let mut token_value: usize = 0;
        let result = GdiplusStartup(&mut token_value, &mut input, ptr::null_mut());

        if result == 0 {
            *token = Some(token_value);
            true
        } else {
            log::error!("GDI+ init failed: {}", result);
            false
        }
    }

    pub unsafe fn shutdown_gdiplus() {
        let mut token = GDIPLUS_TOKEN.lock().unwrap();
        if let Some(t) = token.take() {
            GdiplusShutdown(t);
        }
    }

    unsafe extern "system" fn win_event_proc(
        _h_win_event_hook: *mut std::ffi::c_void,
        _event: u32,
        hwnd: HWND,
        id_object: i32,
        _id_child: i32,
        _dw_event_thread: u32,
        _dwms_event_time: u32,
    ) {
        if id_object != 0 || hwnd.is_null() {
            return;
        }
        let crosshair_hwnd = super::CROSSHAIR_HANDLE.load(Ordering::SeqCst);
        if crosshair_hwnd.is_null() {
            return;
        }
        if hwnd != crosshair_hwnd {
            SetWindowPos(
                crosshair_hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }

    pub unsafe fn install_topmost_guard() {
        let hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            ptr::null_mut(),
            Some(win_event_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        if !hook.is_null() {
            let mut lock = WIN_EVENT_HOOK.lock().unwrap();
            *lock = Some(hook as usize);
        }
    }

    pub unsafe fn uninstall_topmost_guard() {
        let mut lock = WIN_EVENT_HOOK.lock().unwrap();
        if let Some(hook) = lock.take() {
            UnhookWinEvent(hook as *mut std::ffi::c_void);
        }
    }

    fn parse_hex_color(hex: &str) -> (u8, u8, u8) {
        let hex = hex.trim_start_matches('#');
        if hex.len() < 6 {
            return (255, 0, 0);
        }
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
        (r, g, b)
    }

    unsafe fn draw_style_cross(
        graphics: *mut GpGraphics,
        pen: *mut GpPen,
        outline_pen: *mut GpPen,
        center_x: f32,
        center_y: f32,
        gap: f32,
        size: f32,
    ) {
        if size <= gap {
            return;
        }
        // 先画描边（更粗的笔），再画本体覆盖中心区域，留出描边边框
        if !outline_pen.is_null() {
            GdipDrawLine(graphics, outline_pen, center_x, center_y - gap - size, center_x, center_y - gap);
            GdipDrawLine(graphics, outline_pen, center_x, center_y + gap, center_x, center_y + gap + size);
            GdipDrawLine(graphics, outline_pen, center_x - gap - size, center_y, center_x - gap, center_y);
            GdipDrawLine(graphics, outline_pen, center_x + gap, center_y, center_x + gap + size, center_y);
        }
        GdipDrawLine(graphics, pen, center_x, center_y - gap - size, center_x, center_y - gap);
        GdipDrawLine(graphics, pen, center_x, center_y + gap, center_x, center_y + gap + size);
        GdipDrawLine(graphics, pen, center_x - gap - size, center_y, center_x - gap, center_y);
        GdipDrawLine(graphics, pen, center_x + gap, center_y, center_x + gap + size, center_y);
    }

    unsafe fn draw_style_circle(
        graphics: *mut GpGraphics,
        pen: *mut GpPen,
        outline_pen: *mut GpPen,
        center_x: f32,
        center_y: f32,
        size: f32,
    ) {
        if !outline_pen.is_null() {
            GdipDrawEllipse(
                graphics,
                outline_pen,
                center_x - size,
                center_y - size,
                size * 2.0,
                size * 2.0,
            );
        }
        GdipDrawEllipse(
            graphics,
            pen,
            center_x - size,
            center_y - size,
            size * 2.0,
            size * 2.0,
        );
    }

    unsafe fn draw_style_dot(
        graphics: *mut GpGraphics,
        brush: *mut GpBrush,
        outline_brush: *mut GpBrush,
        outline_thickness: f32,
        center_x: f32,
        center_y: f32,
        dot_size: f32,
    ) {
        // 描边：用描边颜色填充一个更大的圆，再画本体圆覆盖中心
        if !outline_brush.is_null() && outline_thickness > 0.0 {
            let r_out = (dot_size + outline_thickness * 2.0) / 2.0;
            GdipFillEllipse(graphics, outline_brush, center_x - r_out, center_y - r_out, r_out * 2.0, r_out * 2.0);
        }
        let r = dot_size / 2.0;
        GdipFillEllipse(graphics, brush, center_x - r, center_y - r, r * 2.0, r * 2.0);
    }

    unsafe fn draw_style_dot_box(
        graphics: *mut GpGraphics,
        pen: *mut GpPen,
        brush: *mut GpBrush,
        outline_pen: *mut GpPen,
        outline_brush: *mut GpBrush,
        outline_thickness: f32,
        center_x: f32,
        center_y: f32,
        size: f32,
        dot_size: f32,
    ) {
        // Draw outer rectangle (outline first, then original)
        if !outline_pen.is_null() {
            GdipDrawRectangle(graphics, outline_pen, center_x - size, center_y - size, size * 2.0, size * 2.0);
        }
        GdipDrawRectangle(graphics, pen, center_x - size, center_y - size, size * 2.0, size * 2.0);
        // Draw center dot (outline first, then original)
        if !outline_brush.is_null() && outline_thickness > 0.0 {
            let r_out = (dot_size + outline_thickness * 2.0) / 2.0;
            GdipFillEllipse(graphics, outline_brush, center_x - r_out, center_y - r_out, r_out * 2.0, r_out * 2.0);
        }
        let r = dot_size / 2.0;
        GdipFillEllipse(graphics, brush, center_x - r, center_y - r, r * 2.0, r * 2.0);
    }

    extern "system" {
        fn GdipLoadImageFromFile(
            filename: *const u16,
            image: *mut *mut std::ffi::c_void,
        ) -> i32;
        fn GdipDrawImageRectI(
            graphics: *mut GpGraphics,
            image: *mut std::ffi::c_void,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
        ) -> i32;
        fn GdipDisposeImage(image: *mut std::ffi::c_void) -> i32;
    }

    unsafe fn draw_custom_image(
        graphics: *mut GpGraphics,
        image_path: &str,
        center_x: f32,
        center_y: f32,
        size: f32,
    ) {
        let path_wide: Vec<u16> = image_path
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        let mut image: *mut std::ffi::c_void = ptr::null_mut();
        let status = GdipLoadImageFromFile(path_wide.as_ptr(), &mut image);

        if status != 0 || image.is_null() {
            return;
        }

        let half = size / 2.0;
        GdipDrawImageRectI(
            graphics,
            image,
            (center_x - half) as i32,
            (center_y - half) as i32,
            size as i32,
            size as i32,
        );

        GdipDisposeImage(image);
    }

    unsafe fn draw_crosshair(
        graphics: *mut GpGraphics,
        settings: &super::CrosshairSettings,
        center_x: f32,
        center_y: f32,
    ) {
        if settings.use_custom_image {
            if let Some(ref image_path) = settings.custom_image_path {
                if !image_path.is_empty() {
                    draw_custom_image(graphics, image_path, center_x, center_y, settings.size as f32);
                }
            }
            return;
        }

        let (r, g, b) = parse_hex_color(&settings.color);
        let argb: u32 =
            ((settings.opacity as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);

        let mut brush: *mut GpSolidFill = ptr::null_mut();
        GdipCreateSolidFill(argb, &mut brush);

        let mut pen: *mut GpPen = ptr::null_mut();
        GdipCreatePen1(argb, settings.thickness as f32, 2, &mut pen);

        GdipSetPenStartCap(pen, 2);
        GdipSetPenEndCap(pen, 2);

        // 创建描边笔和刷（当描边启用时）
        let outline_thickness_f = if settings.outline_enabled {
            settings.outline_thickness.max(0) as f32
        } else {
            0.0
        };
        let mut outline_pen: *mut GpPen = ptr::null_mut();
        let mut outline_brush: *mut GpSolidFill = ptr::null_mut();
        if settings.outline_enabled && outline_thickness_f > 0.0 {
            let (or, og, ob) = parse_hex_color(&settings.outline_color);
            let outline_argb: u32 =
                ((settings.opacity as u32) << 24) | ((or as u32) << 16) | ((og as u32) << 8) | (ob as u32);
            // 描边笔宽度 = 原始粗细 + 描边厚度 * 2，使得描边在原始线条两侧各延伸 outline_thickness 像素
            let outline_pen_width = settings.thickness as f32 + outline_thickness_f * 2.0;
            GdipCreatePen1(outline_argb, outline_pen_width, 2, &mut outline_pen);
            GdipSetPenStartCap(outline_pen, 2);
            GdipSetPenEndCap(outline_pen, 2);
            GdipCreateSolidFill(outline_argb, &mut outline_brush);
        }

        let size = settings.size as f32;
        let gap = settings.gap as f32;
        let dot_size = settings.dot_size as f32;

        match settings.style.as_str() {
            "Cross" => {
                draw_style_cross(graphics, pen, outline_pen, center_x, center_y, gap, size);
            }
            "Dot" => {
                draw_style_dot(graphics, brush as *mut GpBrush, outline_brush as *mut GpBrush, outline_thickness_f, center_x, center_y, dot_size);
            }
            "Circle" => {
                draw_style_circle(graphics, pen, outline_pen, center_x, center_y, size);
            }
            "CrossDot" => {
                draw_style_cross(graphics, pen, outline_pen, center_x, center_y, gap, size);
                draw_style_dot(graphics, brush as *mut GpBrush, outline_brush as *mut GpBrush, outline_thickness_f, center_x, center_y, dot_size);
            }
            "CircleCross" => {
                draw_style_circle(graphics, pen, outline_pen, center_x, center_y, size);
                draw_style_cross(graphics, pen, outline_pen, center_x, center_y, gap, size);
            }
            "DotBox" => {
                draw_style_dot_box(graphics, pen, brush as *mut GpBrush, outline_pen, outline_brush as *mut GpBrush, outline_thickness_f, center_x, center_y, size, dot_size);
            }
            _ => {
                draw_style_cross(graphics, pen, outline_pen, center_x, center_y, gap, size);
            }
        }

        if !outline_pen.is_null() {
            GdipDeletePen(outline_pen);
        }
        if !outline_brush.is_null() {
            GdipDeleteBrush(outline_brush as *mut GpBrush);
        }
        if !pen.is_null() {
            GdipDeletePen(pen);
        }
        if !brush.is_null() {
            GdipDeleteBrush(brush as *mut GpBrush);
        }
    }

    pub unsafe fn get_monitor_bounds(settings: &super::CrosshairSettings) -> (i32, i32, i32, i32) {
        if settings.monitor_index < 0 {
            return (
                0,
                0,
                GetSystemMetrics(SM_CXSCREEN),
                GetSystemMetrics(SM_CYSCREEN),
            );
        }

        struct AllMonitors {
            device_names: Vec<String>,
            bounds: Vec<(i32, i32, i32, i32)>,
        }

        let mut all = AllMonitors {
            device_names: Vec::new(),
            bounds: Vec::new(),
        };

        unsafe extern "system" fn enum_proc(
            hmonitor: HMONITOR,
            _hdc: HDC,
            _rect: *mut windows_sys::Win32::Foundation::RECT,
            lparam: isize,
        ) -> i32 {
            let data = &mut *(lparam as *mut AllMonitors);
            let mut info: MONITORINFOEXW = std::mem::zeroed();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            if GetMonitorInfoW(hmonitor, &mut info as *mut _ as *mut _) != 0 {
                let device_name = String::from_utf16_lossy(
                    &info.szDevice[..info.szDevice.iter().position(|&c| c == 0).unwrap_or(info.szDevice.len())],
                );
                data.device_names.push(device_name);
                data.bounds.push((
                    info.monitorInfo.rcMonitor.left,
                    info.monitorInfo.rcMonitor.top,
                    info.monitorInfo.rcMonitor.right,
                    info.monitorInfo.rcMonitor.bottom,
                ));
            }
            1
        }

        EnumDisplayMonitors(
            ptr::null_mut(),
            ptr::null(),
            Some(enum_proc),
            &mut all as *mut _ as isize,
        );

        // 优先按设备名称匹配（不受枚举顺序影响）
        if let Some(ref target_device) = settings.monitor_device_name {
            for (i, name) in all.device_names.iter().enumerate() {
                if name == target_device {
                    return all.bounds[i];
                }
            }
        }

        // 回退：按索引匹配
        if let Some(bounds) = all.bounds.get(settings.monitor_index as usize) {
            *bounds
        } else {
            (
                0,
                0,
                GetSystemMetrics(SM_CXSCREEN),
                GetSystemMetrics(SM_CYSCREEN),
            )
        }
    }

    unsafe fn render(hwnd: HWND, settings: &super::CrosshairSettings) {
        let (mon_left, mon_top, mon_right, mon_bottom) = get_monitor_bounds(settings);
        let screen_width = mon_right - mon_left;
        let screen_height = mon_bottom - mon_top;

        let dib_size = if settings.use_custom_image {
            (settings.size + 16).max(64)
        } else {
            let outline_extra = if settings.outline_enabled { settings.outline_thickness.max(0) } else { 0 };
            let extent = settings.size + settings.gap + settings.thickness + outline_extra;
            ((extent * 2 + 16) as i32).max(64)
        };

        let screen_dc = GetDC(ptr::null_mut());

        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = dib_size;
        bmi.bmiHeader.biHeight = -dib_size;
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;

        let mut bits: *mut std::ffi::c_void = ptr::null_mut();
        let hbitmap = CreateDIBSection(
            screen_dc,
            &bmi,
            DIB_RGB_COLORS,
            &mut bits,
            ptr::null_mut(),
            0,
        );
        ReleaseDC(ptr::null_mut(), screen_dc);

        if hbitmap.is_null() {
            return;
        }

        let mem_dc = CreateCompatibleDC(ptr::null_mut());
        let old_bmp = SelectObject(mem_dc, hbitmap as HGDIOBJ);

        let mut graphics: *mut GpGraphics = ptr::null_mut();
        if GdipCreateFromHDC(mem_dc, &mut graphics) != 0 {
            SelectObject(mem_dc, old_bmp);
            DeleteObject(hbitmap as HGDIOBJ);
            DeleteDC(mem_dc);
            return;
        }

        GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias);

        let mut clear_brush: *mut GpSolidFill = ptr::null_mut();
        GdipCreateSolidFill(0x00000000, &mut clear_brush);
        GdipFillRectangle(
            graphics,
            clear_brush as *mut GpBrush,
            0.0,
            0.0,
            dib_size as f32,
            dib_size as f32,
        );
        GdipDeleteBrush(clear_brush as *mut GpBrush);

        let center_x = dib_size as f32 / 2.0;
        let center_y = dib_size as f32 / 2.0;
        draw_crosshair(graphics, settings, center_x, center_y);

        GdipDeleteGraphics(graphics);

        if settings.use_custom_image && settings.opacity < 255 {
            let total = (dib_size * dib_size) as usize;
            let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, total);
            let opacity_factor = settings.opacity as u32;
            for pixel in pixels.iter_mut() {
                let a = *pixel >> 24;
                if a == 0 {
                    continue;
                }
                let new_a = a * opacity_factor / 255;
                *pixel = (*pixel & 0x00FFFFFF) | (new_a << 24);
            }
        }

        let win_x = mon_left + (screen_width - dib_size) / 2 + settings.offset_x;
        let win_y = mon_top + (screen_height - dib_size) / 2 + settings.offset_y;

        let ppt_dst = POINT { x: win_x, y: win_y };
        let psize = SIZE {
            cx: dib_size,
            cy: dib_size,
        };
        let ppt_src = POINT { x: 0, y: 0 };

        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };

        UpdateLayeredWindow(
            hwnd,
            ptr::null_mut(),
            &ppt_dst,
            &psize,
            mem_dc,
            &ppt_src,
            0,
            &blend,
            ULW_ALPHA,
        );

        SelectObject(mem_dc, old_bmp);
        DeleteObject(hbitmap as HGDIOBJ);
        DeleteDC(mem_dc);
    }

    pub unsafe fn create_window(settings: &super::CrosshairSettings) -> Result<HWND, String> {
        init_gdiplus();

        let h_instance = GetModuleHandleW(ptr::null());
        if h_instance.is_null() {
            return Err("Failed to get module handle".to_string());
        }

        let class_name = windows_sys::core::w!("NexBoxCrosshairOverlay");

        let wnd_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: h_instance,
            hIcon: ptr::null_mut(),
            hCursor: ptr::null_mut(),
            hbrBackground: ptr::null_mut(),
            lpszMenuName: ptr::null(),
            lpszClassName: class_name,
        };

        if RegisterClassW(&wnd_class) == 0 {
            let error = GetLastError();
            if error != 1410 {
                return Err(format!("RegisterClass failed: {}", error));
            }
        }

        let dib_size = if settings.use_custom_image {
            (settings.size + 16).max(64)
        } else {
            let outline_extra = if settings.outline_enabled { settings.outline_thickness.max(0) } else { 0 };
            let extent = settings.size + settings.gap + settings.thickness + outline_extra;
            ((extent * 2 + 16) as i32).max(64)
        };

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST
                | WS_EX_LAYERED
                | WS_EX_TRANSPARENT
                | WS_EX_TOOLWINDOW
                | WS_EX_NOACTIVATE,
            class_name,
            windows_sys::core::w!("NexBox Crosshair"),
            WS_POPUP,
            0,
            0,
            dib_size,
            dib_size,
            ptr::null_mut(),
            ptr::null_mut(),
            h_instance,
            ptr::null_mut(),
        );

        if hwnd.is_null() {
            return Err("Failed to create window".to_string());
        }

        ShowWindow(hwnd, SW_SHOW);

        render(hwnd, settings);

        Ok(hwnd)
    }

    pub unsafe fn destroy_window(hwnd: HWND) -> bool {
        if hwnd.is_null() {
            return false;
        }
        KillTimer(hwnd, 1);
        DestroyWindow(hwnd) != 0
    }

    pub const WM_CROSSHAIR_REFRESH: u32 = 0x8001;

    pub unsafe extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT {
                    hdc: ptr::null_mut(),
                    fErase: 0,
                    rcPaint: RECT {
                        left: 0,
                        top: 0,
                        right: 0,
                        bottom: 0,
                    },
                    fRestore: 0,
                    fIncUpdate: 0,
                    rgbReserved: [0u8; 32],
                };
                BeginPaint(hwnd, &mut ps);
                EndPaint(hwnd, &ps);
                0
            }
            WM_TIMER => {
                SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
                0
            }
            WM_DISPLAYCHANGE => {
                let settings = super::get_settings();
                render(hwnd, &settings);
                0
            }
            WM_CROSSHAIR_REFRESH => {
                let settings = super::get_settings();
                render(hwnd, &settings);
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

#[cfg(target_os = "windows")]
pub fn start(settings: CrosshairSettings) -> Result<CrosshairResult, String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    // 生命周期锁：保证 start/stop 与并发按下边沿互斥
    let _guard = LIFECYCLE_LOCK.lock().unwrap();

    if CROSSHAIR_ACTIVE.load(Ordering::SeqCst) {
        return Ok(CrosshairResult {
            success: true,
            message: "准心已处于启用状态".to_string(),
        });
    }

    // 等待上一窗口线程完全退出，防止快速启停时出现双窗口
    if let Some(handle) = WINDOW_THREAD.lock().unwrap().take() {
        let _ = handle.join();
    }

    CROSSHAIR_ACTIVE.store(true, Ordering::SeqCst);

    {
        let mut settings_lock = CURRENT_SETTINGS.lock().unwrap();
        *settings_lock = Some(settings.clone());
    }

    let handle = thread::spawn(move || unsafe {
        match win32::create_window(&settings) {
            Ok(hwnd) => {
                CROSSHAIR_HANDLE.store(hwnd, Ordering::SeqCst);

                SetTimer(hwnd, 1, 500, None);
                win32::install_topmost_guard();

                let mut msg: MSG = std::mem::zeroed();
                while CROSSHAIR_ACTIVE.load(Ordering::SeqCst) {
                    while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                        if msg.message == WM_QUIT {
                            break;
                        }
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }

                    if !CROSSHAIR_ACTIVE.load(Ordering::SeqCst) {
                        break;
                    }

                    thread::sleep(Duration::from_millis(50));
                }

                win32::uninstall_topmost_guard();
                win32::destroy_window(hwnd);
                CROSSHAIR_HANDLE.store(std::ptr::null_mut(), Ordering::SeqCst);
            }
            Err(e) => {
                log::error!("Failed to create crosshair window: {}", e);
                CROSSHAIR_ACTIVE.store(false, Ordering::SeqCst);
            }
        }
    });
    *WINDOW_THREAD.lock().unwrap() = Some(handle);

    Ok(CrosshairResult {
        success: true,
        message: "准心已启动".to_string(),
    })
}

#[cfg(not(target_os = "windows"))]
pub fn start(_settings: CrosshairSettings) -> Result<CrosshairResult, String> {
    Err("此功能仅支持 Windows 系统".to_string())
}

#[cfg(target_os = "windows")]
pub fn stop() -> Result<CrosshairResult, String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE};

    // 生命周期锁：保证 start/stop 与并发按下边沿互斥
    let _guard = LIFECYCLE_LOCK.lock().unwrap();

    if !CROSSHAIR_ACTIVE.load(Ordering::SeqCst) {
        return Ok(CrosshairResult {
            success: true,
            message: "准心已处于关闭状态".to_string(),
        });
    }

    CROSSHAIR_ACTIVE.store(false, Ordering::SeqCst);

    unsafe {
        let hwnd = CROSSHAIR_HANDLE.load(Ordering::SeqCst);
        if !hwnd.is_null() {
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
        }
    }

    Ok(CrosshairResult {
        success: true,
        message: "准心已关闭".to_string(),
    })
}

#[cfg(not(target_os = "windows"))]
pub fn stop() -> Result<CrosshairResult, String> {
    Err("此功能仅支持 Windows 系统".to_string())
}

/// Toggle crosshair on/off. Used by global hotkey.
pub fn toggle_crosshair_sync(app_handle: &tauri::AppHandle) -> Result<CrosshairResult, String> {
    let result = if CROSSHAIR_ACTIVE.load(Ordering::SeqCst) {
        stop()
    } else {
        let settings = get_settings();
        start(settings)
    };

    if result.is_ok() {
        let _ = app_handle.emit("crosshair-status-changed", ());
    }

    result
}

#[tauri::command]
pub async fn get_crosshair_status() -> Result<CrosshairSettings, String> {
    let mut settings = get_settings();
    settings.enabled = CROSSHAIR_ACTIVE.load(Ordering::SeqCst);

    #[cfg(target_os = "windows")]
    {
        let (left, top, right, bottom) = unsafe { win32::get_monitor_bounds(&settings) };
        settings.screen_width = right - left;
        settings.screen_height = bottom - top;
    }

    Ok(settings)
}

#[tauri::command]
pub async fn toggle_crosshair(app_handle: tauri::AppHandle) -> Result<CrosshairResult, String> {
    toggle_crosshair_sync(&app_handle)
}

#[tauri::command]
pub async fn update_crosshair_settings(
    settings: CrosshairSettings,
) -> Result<CrosshairResult, String> {
    let was_active = CROSSHAIR_ACTIVE.load(Ordering::SeqCst);

    {
        let mut settings_lock = CURRENT_SETTINGS.lock().unwrap();
        *settings_lock = Some(settings.clone());
    }

    if was_active {
        #[cfg(target_os = "windows")]
        {
            let hwnd = CROSSHAIR_HANDLE.load(Ordering::SeqCst);
            if !hwnd.is_null() {
                unsafe {
                    windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                        hwnd,
                        win32::WM_CROSSHAIR_REFRESH,
                        0,
                        0,
                    );
                }
                return Ok(CrosshairResult {
                    success: true,
                    message: "设置已更新".to_string(),
                });
            }
        }
    }

    if settings.enabled || was_active {
        let mut start_settings = settings;
        start_settings.enabled = true;
        start(start_settings)?;
    }

    Ok(CrosshairResult {
        success: true,
        message: "设置已更新".to_string(),
    })
}

#[tauri::command]
pub async fn pick_crosshair_image() -> Result<Option<String>, String> {
    let file = rfd::FileDialog::new()
        .set_title("选择准心图片")
        .add_filter(
            "Images",
            &["png", "jpg", "jpeg", "bmp", "gif", "webp"],
        )
        .pick_file();
    Ok(file.map(|f| f.to_string_lossy().to_string()))
}

#[tauri::command]
pub async fn get_preset_crosshair_path(
    app_handle: tauri::AppHandle,
    filename: String,
) -> Result<String, String> {
    let presets_dir = find_crosshair_presets_dir(&app_handle)
        .ok_or_else(|| "找不到准心预设资源目录".to_string())?;

    let img_path = presets_dir.join(&filename);
    if img_path.exists() {
        return Ok(img_path.to_string_lossy().to_string());
    }

    Err(format!("找不到预设准心图片: {}", filename))
}

/// 获取内置准心预设图片列表
#[tauri::command]
pub async fn get_crosshair_presets(app: tauri::AppHandle) -> Result<Vec<CrosshairPreset>, String> {
    // 尝试查找 crosshair-presets 资源目录
    let presets_dir = find_crosshair_presets_dir(&app)
        .ok_or_else(|| "找不到准心预设资源目录".to_string())?;

    let mut presets = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&presets_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    let ext_lower = ext.to_string_lossy().to_lowercase();
                    if ext_lower == "png" || ext_lower == "jpg" || ext_lower == "jpeg"
                        || ext_lower == "bmp" || ext_lower == "gif" || ext_lower == "webp"
                    {
                        let name = path.file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        presets.push(CrosshairPreset {
                            name,
                            path: path.to_string_lossy().to_string(),
                        });
                    }
                }
            }
        }
    }

    // 按文件名排序，保持一致性
    presets.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(presets)
}

fn find_crosshair_presets_dir(app: &tauri::AppHandle) -> Option<PathBuf> {

    // 1. 通过 Tauri resource_dir 查找
    if let Ok(resource_dir) = app.path().resource_dir() {
        let candidates = [
            resource_dir.join("crosshair-presets"),
            resource_dir.join("resources").join("crosshair-presets"),
            resource_dir.join("_up_").join("resources").join("crosshair-presets"),
            resource_dir.join("_up_").join("_up_").join("src-tauri").join("resources").join("crosshair-presets"),
        ];
        for path in &candidates {
            if path.exists() {
                return Some(path.clone());
            }
        }
    }

    // 2. 通过 exe 路径查找（开发环境）
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            let candidates = [
                parent.join("crosshair-presets"),
                parent.join("resources").join("crosshair-presets"),
                parent.join("..").join("..").join("resources").join("crosshair-presets"),
                parent.join("..").join("..").join("..").join("src-tauri").join("resources").join("crosshair-presets"),
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

    // 3. 编译时路径（开发环境）
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dev_path = manifest_dir.join("resources").join("crosshair-presets");
    if dev_path.exists() {
        return Some(dev_path);
    }

    None
}

pub fn cleanup() {
    if CROSSHAIR_ACTIVE.load(Ordering::SeqCst) {
        let _ = stop();
    }
    #[cfg(target_os = "windows")]
    unsafe {
        win32::shutdown_gdiplus();
    }
}