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

/// 托盘图标句柄：托盘菜单的「清理内存」在后台线程执行完后需要短暂改写
/// tooltip 显示结果，而悬停刷新线程持有的是它自己的克隆，这里存一份全局引用。
static TRAY_ICON: LazyLock<Mutex<Option<TrayIcon<tauri::Wry>>>> =
    LazyLock::new(|| Mutex::new(None));

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
        return;
    }
    // 主窗口不存在（节约内存模式下已销毁）→ 重建，等待前端就绪后自动显示。
    // 覆盖全部恢复入口：托盘左键 / 托盘菜单 / 单实例二次启动 / show_window 命令 / 检查更新
    crate::main_window::rebuild_main_window(app);
}

pub fn init_tray(app: &AppHandle) -> Result<TrayIcon, Box<dyn std::error::Error>> {
    if TRAY_INITIALIZED.load(Ordering::SeqCst) {
        return Err("Tray already initialized".into());
    }

    let tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();
            match event {
                // tray-icon 对同一次点击会先后投递 Down 与 Up 两个 Click 事件，
                // 只认按下：否则菜单会被触发两次。
                tauri::tray::TrayIconEvent::Click {
                    button: tauri::tray::MouseButton::Left,
                    button_state: tauri::tray::MouseButtonState::Down,
                    ..
                } => {
                    show_main_window(app);
                }
                tauri::tray::TrayIconEvent::Click {
                    button: tauri::tray::MouseButton::Right,
                    button_state: tauri::tray::MouseButtonState::Down,
                    rect,
                    ..
                } => {
                    #[cfg(target_os = "windows")]
                    show_tray_menu_native(app, &rect);
                    #[cfg(not(target_os = "windows"))]
                    let _ = (app, &rect);
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

    if let Ok(mut slot) = TRAY_ICON.lock() {
        *slot = Some(tray.clone());
    }

    TRAY_INITIALIZED.store(true, Ordering::SeqCst);

    Ok(tray)
}

/* ------------------------------------------------------------------ */
/* Windows：托盘右键菜单 —— Windows 原生 TrackPopupMenu                 */
/*                                                                      */
/* 旧实现是一个常驻隐藏的 WebView2 窗口（label=tray-menu）+ 低级鼠标钩子  */
/* 守护（约 470 行）。原生菜单由系统模态循环驱动：点外部自动关闭、Esc、   */
/* 任务栏/开始菜单抢占前台等全部行为内置，守护代码整体退役。             */
/* 换掉后少一个常驻 WebView2 渲染进程（约 100~200MB 提交内存）与其上的   */
/* React 运行时。                                                      */
/* ------------------------------------------------------------------ */
#[cfg(target_os = "windows")]
mod native_menu {
    use std::sync::atomic::{AtomicIsize, Ordering};

    use tauri::{AppHandle, Emitter};
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DestroyMenu, IsWindow, PostMessageW,
        SetForegroundWindow, ShowWindow, TrackPopupMenu, HMENU, MF_SEPARATOR, MF_STRING,
        SW_SHOWNA, TPM_BOTTOMALIGN, TPM_CENTERALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_NULL,
        WS_EX_TOOLWINDOW, WS_POPUP,
    };

    /// 菜单项 ID（TrackPopupMenu + TPM_RETURNCMD 的返回值）
    const ID_OPEN_MAIN: u32 = 1001;
    const ID_CLEAN_MEMORY: u32 = 1002;
    const ID_CHECK_UPDATE: u32 = 1003;
    const ID_EXIT: u32 = 1004;

    /// 托盘菜单专用 owner 窗口句柄（懒创建，进程退出随进程销毁）
    static OWNER_HWND: AtomicIsize = AtomicIsize::new(0);

    /// UTF-8 → NUL 结尾 UTF-16（AppendMenuW 需要；调用同步完成，无需保活）
    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// 获取（或创建）托盘菜单的 owner 窗口：1×1、屏幕外、WS_EX_TOOLWINDOW。
    ///
    /// 为什么不用主窗口：托盘弹出菜单的标准前置是 SetForegroundWindow(owner)，
    /// 若 owner 是主窗口，主窗口隐藏时会被系统记为前台 → 任务栏出现它的幽灵按钮。
    /// WS_EX_TOOLWINDOW 永不产生任务栏按钮，即使处于前台；离屏 1×1 也不可见。
    /// owner 只用于承接菜单的模态循环与前台转移，不处理任何消息（默认 DefWindowProc）。
    fn ensure_owner_window() -> HWND {
        let cached = OWNER_HWND.load(Ordering::SeqCst) as HWND;
        if !cached.is_null() && unsafe { IsWindow(cached) != 0 } {
            return cached;
        }
        unsafe {
            let class = to_wide("Static"); // 系统预注册类，无需 RegisterClass
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                class.as_ptr(),
                std::ptr::null(),
                WS_POPUP,
                -32000,
                -32000,
                1,
                1,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            );
            if hwnd.is_null() {
                log::error!("[tray] 创建托盘菜单 owner 窗口失败");
                return std::ptr::null_mut();
            }
            // SW_SHOWNA：显示但不激活（对完全隐藏的窗口 SetForegroundWindow 行为不可靠）
            ShowWindow(hwnd, SW_SHOWNA);
            OWNER_HWND.store(hwnd as isize, Ordering::SeqCst);
            hwnd
        }
    }

    /// 商店版（MSIX 包内运行）判定：与 main.rs set_packaged_aumid 同一依据。
    /// 商店政策不允许引导站外更新，故商店版不显示「检查更新」。
    fn is_store_packaged() -> bool {
        std::env::current_exe()
            .ok()
            .map(|p| p.to_string_lossy().to_lowercase().contains("\\windowsapps\\"))
            .unwrap_or(false)
    }

    /// 弹出原生托盘菜单（阻塞至菜单关闭，跑在 tao 事件循环线程——TrackPopupMenu
    /// 的标准用法：模态循环期间 owner 窗口消息照常派发，不卡主窗口）。
    pub fn show(app: &AppHandle, rect: &tauri::Rect) {
        let (px, py) = match rect.position {
            tauri::Position::Physical(p) => (p.x, p.y),
            tauri::Position::Logical(p) => (p.x as i32, p.y as i32),
        };
        let (sw, _sh) = match rect.size {
            tauri::Size::Physical(s) => (s.width as i32, s.height as i32),
            tauri::Size::Logical(s) => (s.width as i32, s.height as i32),
        };

        unsafe {
            let menu: HMENU = CreatePopupMenu();
            if menu.is_null() {
                log::error!("[tray] CreatePopupMenu 失败");
                return;
            }

            AppendMenuW(menu, MF_STRING, ID_OPEN_MAIN as usize, to_wide("打开主窗口").as_ptr());
            AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
            AppendMenuW(menu, MF_STRING, ID_CLEAN_MEMORY as usize, to_wide("清理内存").as_ptr());
            if !is_store_packaged() {
                AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
                AppendMenuW(menu, MF_STRING, ID_CHECK_UPDATE as usize, to_wide("检查更新").as_ptr());
            }
            AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
            AppendMenuW(menu, MF_STRING, ID_EXIT as usize, to_wide("退出").as_ptr());

            // owner：专用隐藏工具窗口（见 ensure_owner_window 注释）。
            // SetForegroundWindow 是托盘弹出菜单的标准前置（MSDN Shell_NotifyIcon 惯例），
            // 确保点击菜单外部/按 Esc 时菜单能正确关闭；失败也无碍，系统模态循环兜底。
            let hwnd = ensure_owner_window();
            if hwnd.is_null() {
                DestroyMenu(menu);
                return;
            }
            SetForegroundWindow(hwnd);

            // 水平居中于图标，底部对齐图标顶部（任务栏在下方的默认布局）；
            // 屏幕边缘/任务栏避让由系统自动处理，无需手工钳制。
            let cmd = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_CENTERALIGN,
                px + sw / 2,
                py,
                0,
                hwnd,
                std::ptr::null(),
            ) as u32;

            PostMessageW(hwnd, WM_NULL, 0, 0);
            DestroyMenu(menu);

            match cmd {
                ID_OPEN_MAIN => super::show_main_window(app),
                ID_CLEAN_MEMORY => super::spawn_memory_cleanup(),
                ID_CHECK_UPDATE => {
                    super::show_main_window(app);
                    let _ = app.emit("check-update", ());
                }
                ID_EXIT => super::perform_exit(app),
                _ => {} // 0 = 未选择（点击外部/Esc）
            }
        }
    }
}

#[cfg(target_os = "windows")]
use native_menu::show as show_tray_menu_native;

/// 托盘菜单「清理内存」：后台线程执行待机缓存清理 + 工作集收紧（与内存清理页
/// 「一键清理」同一组底层函数），完成后把结果短暂写进托盘 tooltip。
/// 菜单本身不等结果（原生菜单不支持原地刷新文案），点完即关、结果悬停可见。
#[cfg(target_os = "windows")]
fn spawn_memory_cleanup() {
    std::thread::spawn(move || {
        let freed1 = crate::optimization::clean_standby_memory_inner();
        let freed2 = crate::optimization::trim_working_set_inner();
        let total = freed1 + freed2;

        // 悬停刷新线程每秒会覆盖 tooltip；悬停期间不抢写，结果留给非悬停时刻。
        // （毫秒级差异下多数用户点完菜单鼠标已离开图标，能看到结果文案）
        if HOVER_ACTIVE.load(Ordering::SeqCst) {
            return;
        }
        let Some(tray) = TRAY_ICON
            .lock()
            .ok()
            .and_then(|g| g.clone())
        else {
            return;
        };
        let text = if total > 0 {
            format!("已释放 {total} MB")
        } else {
            "内存已清理".to_string()
        };
        let _ = tray.set_tooltip(Some(text));
        std::thread::sleep(std::time::Duration::from_secs(2));
        let _ = tray.set_tooltip::<&str>(None);
    });
}

/// 退出应用：先隐藏所有窗口，避免退出时 WebView2 销毁后短暂露出原生标题栏。
pub(crate) fn perform_exit(app: &tauri::AppHandle) {
    for label in &["main", "desktop-lyrics", "vertical-overlay", "dynamic-island"] {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.hide();
        }
    }
    // 给 Windows 消息队列一点时间处理隐藏操作
    std::thread::sleep(std::time::Duration::from_millis(50));
    app.exit(0);
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
    // 节约内存(实验性)：销毁主窗口释放内存；被守卫拦下（音乐播放中）则回退为仅隐藏
    if crate::main_window::memory_saving_enabled()
        && crate::main_window::destroy_main_window(window.app_handle())
    {
        return Ok(());
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
    perform_exit(&app);
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
