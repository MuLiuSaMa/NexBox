use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{
    tray::{TrayIcon, TrayIconBuilder},
    AppHandle, Emitter, Manager, Runtime, Window,
};

static TRAY_INITIALIZED: AtomicBool = AtomicBool::new(false);
static CLOSE_BEHAVIOR: LazyLock<Mutex<String>> = LazyLock::new(|| Mutex::new(String::from("ask")));
static DONT_ASK_AGAIN: AtomicBool = AtomicBool::new(false);

/// 悬停面板当前是否应显示（光标悬停在托盘图标上时为 true）
static HOVER_ACTIVE: AtomicBool = AtomicBool::new(false);
/// 悬停数据推送线程是否已启动（避免重复 spawn）
static HOVER_THREAD_SPAWNED: AtomicBool = AtomicBool::new(false);

/// 获取指定屏幕坐标所在显示器的工作区（已排除任务栏），返回物理像素 (x, y, width, height)。
#[cfg(target_os = "windows")]
fn get_monitor_work_area(px: i32, py: i32) -> Option<(i32, i32, i32, i32)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };

    unsafe {
        let point = POINT { x: px, y: py };
        let hmonitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        if hmonitor.is_null() {
            return None;
        }
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(hmonitor, &mut info) == 0 {
            return None;
        }
        Some((
            info.rcWork.left,
            info.rcWork.top,
            info.rcWork.right - info.rcWork.left,
            info.rcWork.bottom - info.rcWork.top,
        ))
    }
}

#[cfg(not(target_os = "windows"))]
fn get_monitor_work_area(_px: i32, _py: i32) -> Option<(i32, i32, i32, i32)> {
    None
}

/// 打开主窗口前确保其落在某个显示器内（避免离屏预热后从托盘打开时仍不可见）。
/// 仅当窗口当前完全位于所有显示器之外时才居中到主显示器；屏幕内则保持用户位置不动。
fn ensure_main_onscreen<R: Runtime>(app: &AppHandle<R>) -> Option<()> {
    let win = app.get_webview_window("main")?;
    let pos = win.outer_position().ok()?;
    let on_screen = app
        .available_monitors()
        .ok()?
        .iter()
        .any(|m| {
            let r = m.position();
            let s = m.size();
            let x = pos.x;
            let y = pos.y;
            x + 10 >= r.x && x - 10 <= r.x + s.width as i32
                && y + 10 >= r.y && y - 10 <= r.y + s.height as i32
        });
    if !on_screen {
        // 优先恢复到上次保存的位置（若仍在屏幕内），否则明确居中到主显示器：
        // 离屏预热位置(-30000,-30000)会导致 MonitorFromWindow(MONITOR_DEFAULTTONEAREST)
        // 返回副显示器，若改用 win.center() 会让窗口错误地出现在副显示器上。
        if let Some(saved) = crate::main_window::read_saved_position(app) {
            if crate::main_window::is_on_any_monitor(app, saved) {
                let _ = win.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
                    x: saved.0,
                    y: saved.1,
                }));
                return Some(());
            }
        }
        if let Some(primary) = app.primary_monitor().ok().flatten() {
            let p = primary.position();
            let s = primary.size();
            if let Ok(ws) = win.outer_size() {
                let x = p.x + (s.width as i32 - ws.width as i32) / 2;
                let y = p.y + (s.height as i32 - ws.height as i32) / 2;
                let _ = win.set_position(tauri::Position::Physical(
                    tauri::PhysicalPosition { x, y },
                ));
            }
        }
    }
    Some(())
}

/// 从托盘打开主窗口的统一入口：恢复任务栏 → 若处于离屏预热则归位到屏幕内 → 显示 → 恢复 → 聚焦。
pub(crate) fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_skip_taskbar(false);
        ensure_main_onscreen(app);
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        crate::emit_main_visibility(app, true);
    }
}

pub fn init_tray<R: Runtime>(app: &AppHandle<R>) -> Result<TrayIcon<R>, Box<dyn std::error::Error>> {
    if TRAY_INITIALIZED.load(Ordering::SeqCst) {
        return Err("Tray already initialized".into());
    }

    let tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();
            match event {
                tauri::tray::TrayIconEvent::Click {
                    button: tauri::tray::MouseButton::Left,
                    ..
                } => {
                    show_main_window(app);
                }
                tauri::tray::TrayIconEvent::Click {
                    button: tauri::tray::MouseButton::Right,
                    rect,
                    ..
                } => {
                    if let Some(menu_window) = app.get_webview_window("tray-menu") {
                        let (px, py) = match rect.position {
                            tauri::Position::Physical(p) => (p.x, p.y),
                            tauri::Position::Logical(p) => (p.x as i32, p.y as i32),
                        };
                        let (sw, _sh) = match rect.size {
                            tauri::Size::Physical(s) => (s.width as i32, s.height as i32),
                            tauri::Size::Logical(s) => (s.width as i32, s.height as i32),
                        };
                        // 使用窗口实际物理尺寸计算偏移，避免 DPI 缩放导致菜单侵入任务栏
                        let (mw, mh) = menu_window
                            .outer_size()
                            .ok()
                            .map(|s| (s.width as i32, s.height as i32))
                            .unwrap_or((190, 184));

                        // 默认：菜单底部对齐托盘图标顶部（任务栏在屏幕下方）
                        let mut x = px + sw / 2 - mw / 2;
                        let mut y = py - mh;

                        // 依据所在显示器的工作区(不含任务栏)钳制，确保菜单完整显示在任务栏上方
                        if let Some((wx, wy, ww, wh)) = get_monitor_work_area(px, py) {
                            if mw <= ww && mh <= wh {
                                x = x.clamp(wx, wx + ww - mw);
                                y = y.clamp(wy, wy + wh - mh);
                            } else {
                                // 工作区容纳不下时退回屏幕内
                                x = x.clamp(wx, wx + ww - mw.min(ww));
                                y = y.clamp(wy, wy + wh - mh.min(wh));
                            }
                        } else {
                            x = x.max(0);
                            y = y.max(0);
                        }

                        let _ = menu_window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
                            x,
                            y,
                        }));
                        let _ = menu_window.set_always_on_top(true);
                        let _ = menu_window.show();
                        let _ = menu_window.set_focus();
                    }
                }
                tauri::tray::TrayIconEvent::Enter { .. } => {
                    start_tray_hover_tooltip(tray);
                }
                tauri::tray::TrayIconEvent::Leave { .. } => {
                    stop_tray_hover_tooltip(tray);
                }
                _ => {}
            }
        })
        .build(app)?;

    TRAY_INITIALIZED.store(true, Ordering::SeqCst);

    Ok(tray)
}

/// 生成托盘悬停提示文本（核心四项：CPU/GPU 占用+温度、内存、磁盘）。
/// 受原生 tooltip 128 字符硬限制，仅保留最关键的指标；数据取自常驻轮询缓存，不额外采样。
fn build_hover_tooltip() -> String {
    let snap = crate::overlay_panel::current_hover_snapshot();
    let disk = crate::hardware::disk_usage_percent();
    let s = snap.as_ref();

    let pct = |v: Option<f64>| match v {
        Some(x) => format!("{:.0}%", x),
        None => "--".to_string(),
    };
    let temp = |v: Option<f64>| match v {
        Some(x) => format!("{:.0}°C", x),
        None => "--".to_string(),
    };

    let cpu_usage = s.and_then(|x| x.cpu_usage).map(|v| v as f64);
    let cpu_temp = s.and_then(|x| x.cpu_temp);
    let gpu_usage = s.and_then(|x| x.gpu_usage).map(|v| v as f64);
    let gpu_temp = s.and_then(|x| x.gpu_temp);
    let memory = s.and_then(|x| x.memory_usage);

    format!(
        "CPU {} {}\r\nGPU {} {}\r\n内存 {}\r\n磁盘 {}",
        pct(cpu_usage),
        temp(cpu_temp),
        pct(gpu_usage),
        temp(gpu_temp),
        pct(memory),
        pct(disk),
    )
}

/// 光标悬停托盘图标：立即更新一次提示，并启动每秒刷新（仅悬停期间运行，离开即停止）。
fn start_tray_hover_tooltip<R: Runtime>(tray: &TrayIcon<R>) {
    let _ = tray.set_tooltip(Some(build_hover_tooltip()));

    HOVER_ACTIVE.store(true, Ordering::SeqCst);
    if !HOVER_THREAD_SPAWNED.swap(true, Ordering::SeqCst) {
        let tray = tray.clone();
        std::thread::spawn(move || {
            while HOVER_ACTIVE.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(1000));
                if !HOVER_ACTIVE.load(Ordering::SeqCst) {
                    break;
                }
                let _ = tray.set_tooltip(Some(build_hover_tooltip()));
            }
            HOVER_THREAD_SPAWNED.store(false, Ordering::SeqCst);
        });
    }
}

/// 光标离开托盘图标：停止刷新并清除提示文本。
fn stop_tray_hover_tooltip<R: Runtime>(tray: &TrayIcon<R>) {
    HOVER_ACTIVE.store(false, Ordering::SeqCst);
    let _ = tray.set_tooltip::<&str>(None);
}

#[tauri::command]
pub async fn minimize_to_tray<R: Runtime>(window: Window<R>) -> Result<(), String> {
    // 前端在此被调用，说明前端(WebView2)已成功加载 → 标记开机自启模式下前端就绪，
    // 供启动诊断区分“正常最小化启动”与“只启动后端、前端未起来”。
    crate::AUTOSTART_FRONTEND_READY.store(true, std::sync::atomic::Ordering::SeqCst);
    if crate::AUTOSTART_MODE.load(std::sync::atomic::Ordering::SeqCst) {
        log::info!("[autostart] 前端已就绪，正常进入最小化启动");
    }
    window.hide().map_err(|e| e.to_string())?;
    crate::emit_main_visibility(&window.app_handle(), false);
    Ok(())
}

#[tauri::command]
pub async fn show_window<R: Runtime>(window: Window<R>) -> Result<(), String> {
    // 从任意窗口调用均打开主窗口（复用离屏归位逻辑）
    show_main_window(window.app_handle());
    Ok(())
}

#[tauri::command]
pub fn get_close_behavior() -> String {
    CLOSE_BEHAVIOR.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_close_behavior(behavior: String) {
    if let Ok(mut b) = CLOSE_BEHAVIOR.lock() {
        *b = behavior;
    }
}

#[tauri::command]
pub fn get_dont_ask_again() -> bool {
    DONT_ASK_AGAIN.load(Ordering::SeqCst)
}

#[tauri::command]
pub fn set_dont_ask_again(value: bool) {
    DONT_ASK_AGAIN.store(value, Ordering::SeqCst);
}

#[tauri::command]
pub fn exit_app(app: tauri::AppHandle) {
    // 先隐藏所有窗口，避免退出时 WebView2 销毁后短暂露出原生标题栏
    for label in &["main", "tray-menu", "desktop-lyrics", "lyrics-unlock-btn", "vertical-overlay"] {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.hide();
        }
    }
    // 给 Windows 消息队列一点时间处理隐藏操作
    std::thread::sleep(std::time::Duration::from_millis(50));
    app.exit(0);
}

#[tauri::command]
pub fn check_update_and_show(app: AppHandle) {
    show_main_window(&app);
    let _ = app.emit("check-update", ());
}

pub fn cleanup() {
    HOVER_ACTIVE.store(false, Ordering::SeqCst);
    TRAY_INITIALIZED.store(false, Ordering::SeqCst);
}
