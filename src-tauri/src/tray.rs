use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
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

/// 从托盘打开主窗口的统一入口：收起托盘菜单 → 恢复任务栏 → 若处于离屏预热则归位到屏幕内 → 显示 → 恢复 → 聚焦。
pub(crate) fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    // 菜单可能正显示着却没拿到焦点（系统前台锁场景，失焦事件不会来），
    // 这里无条件先收起，避免主窗口后面挂着一个残菜单。
    if let Some(menu) = app.get_webview_window("tray-menu") {
        if menu.is_visible().unwrap_or(false) {
            let _ = menu.set_always_on_top(false);
            let _ = menu.hide();
        }
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_skip_taskbar(false);
        ensure_main_onscreen(app);
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        crate::emit_main_visibility(app, true);
    }
}

/// 托盘图标当前矩形（物理像素），静态数组依次为 left/top/right/bottom。
/// 用 4 个 AtomicI32 而不是 Mutex：低级鼠标钩子回调里读取不能加锁。
#[cfg(target_os = "windows")]
static TRAY_ICON_RECT: [AtomicI32; 4] = [
    AtomicI32::new(0),
    AtomicI32::new(0),
    AtomicI32::new(0),
    AtomicI32::new(0),
];
#[cfg(target_os = "windows")]
static TRAY_ICON_RECT_KNOWN: AtomicBool = AtomicBool::new(false);

/// 记录托盘图标当前矩形（Shell 每次上报事件都会带上，任务栏重排后自动更新）。
#[cfg(target_os = "windows")]
fn remember_tray_icon_rect(rect: &tauri::Rect) {
    let (x, y) = match rect.position {
        tauri::Position::Physical(p) => (p.x, p.y),
        tauri::Position::Logical(p) => (p.x as i32, p.y as i32),
    };
    let (w, h) = match rect.size {
        tauri::Size::Physical(s) => (s.width as i32, s.height as i32),
        tauri::Size::Logical(s) => (s.width as i32, s.height as i32),
    };
    TRAY_ICON_RECT[0].store(x, Ordering::SeqCst);
    TRAY_ICON_RECT[1].store(y, Ordering::SeqCst);
    TRAY_ICON_RECT[2].store(x + w, Ordering::SeqCst);
    TRAY_ICON_RECT[3].store(y + h, Ordering::SeqCst);
    TRAY_ICON_RECT_KNOWN.store(true, Ordering::SeqCst);
}

/// 托盘图标当前矩形 (left, top, right, bottom)，物理像素；尚未知的返回 None。
#[cfg(target_os = "windows")]
pub(crate) fn tray_icon_rect() -> Option<(i32, i32, i32, i32)> {
    if !TRAY_ICON_RECT_KNOWN.load(Ordering::SeqCst) {
        return None;
    }
    Some((
        TRAY_ICON_RECT[0].load(Ordering::SeqCst),
        TRAY_ICON_RECT[1].load(Ordering::SeqCst),
        TRAY_ICON_RECT[2].load(Ordering::SeqCst),
        TRAY_ICON_RECT[3].load(Ordering::SeqCst),
    ))
}

/// 右键托盘图标：把菜单窗口定位到图标上方并弹出。
///
/// 弹出后尽力把菜单变成真正的前台窗口（`set_focus` 内部的裸 SetForegroundWindow
/// 在托盘场景经常静默失败），并武装「外部点击守护」兜底关闭。
fn show_tray_menu(app: &AppHandle, rect: tauri::Rect) {
    let Some(menu_window) = app.get_webview_window("tray-menu") else {
        return;
    };
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

    #[cfg(target_os = "windows")]
    {
        remember_tray_icon_rect(&rect);
        if let Ok(hwnd) = menu_window.hwnd() {
            menu_guard::force_foreground(hwnd.0);
            menu_guard::arm(app, hwnd.0);
        }
    }
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
                // 只认按下：否则菜单会被重定位两次并重置守护宽限期。
                tauri::tray::TrayIconEvent::Click {
                    button: tauri::tray::MouseButton::Left,
                    button_state: tauri::tray::MouseButtonState::Down,
                    rect,
                    ..
                } => {
                    #[cfg(target_os = "windows")]
                    remember_tray_icon_rect(&rect);
                    show_main_window(app);
                }
                tauri::tray::TrayIconEvent::Click {
                    button: tauri::tray::MouseButton::Right,
                    button_state: tauri::tray::MouseButtonState::Down,
                    rect,
                    ..
                } => {
                    show_tray_menu(app, rect);
                }
                tauri::tray::TrayIconEvent::Enter { rect, .. } => {
                    #[cfg(target_os = "windows")]
                    remember_tray_icon_rect(&rect);
                    start_tray_hover_tooltip(tray);
                }
                tauri::tray::TrayIconEvent::Move { rect, .. } => {
                    #[cfg(target_os = "windows")]
                    remember_tray_icon_rect(&rect);
                }
                tauri::tray::TrayIconEvent::Leave { rect, .. } => {
                    #[cfg(target_os = "windows")]
                    remember_tray_icon_rect(&rect);
                    stop_tray_hover_tooltip(tray);
                }
                _ => {}
            }
        })
        .build(app)?;

    TRAY_INITIALIZED.store(true, Ordering::SeqCst);

    Ok(tray)
}

/// Windows 专属：托盘菜单的前台激活 + 「点击菜单外自动收起」守护。
///
/// 关闭菜单原本只依赖窗口失焦事件，但两种情况下会失效：
/// 1) 右键托盘图标时「最后一次输入事件」属于 Explorer 的前台线程，本进程不满足系统的
///    前台判据，裸 SetForegroundWindow（Tauri `set_focus` 内部即此）静默返回 0 → 菜单
///    从未进入前台 → 永远不产生 Focused(false)，表现为「点哪儿都不关」；
/// 2) 即使拿到前台，点任务栏 / 开始菜单 / 通知中心这类不移走前台窗口的点击也不触发失焦。
/// 所以先尽力把菜单变成真正的前台窗口（顺带让 Esc 可用），再在菜单可见期间挂一个
/// 低级鼠标钩子兜底：菜单外的按下即关闭菜单，并按场景决定是否吞掉那一次点击。
#[cfg(target_os = "windows")]
mod menu_guard {
    use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, AtomicUsize, Ordering};
    use std::sync::{LazyLock, Mutex};

    use tauri::{AppHandle, Manager};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, TRUE, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows_sys::Win32::System::SystemInformation::GetTickCount64;
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, SendInput, SetActiveWindow, SetFocus, INPUT, INPUT_0, INPUT_KEYBOARD,
        KEYBDINPUT, KEYEVENTF_KEYUP, VK_LBUTTON, VK_MBUTTON, VK_MENU, VK_RBUTTON,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, CallNextHookEx, GetAncestor, GetClassNameW, GetCursorPos,
        GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId, IsWindow, IsWindowVisible,
        KillTimer, SetForegroundWindow, SetTimer, SetWindowPos, SetWindowsHookExW, ShowWindow,
        SwitchToThisWindow, UnhookWindowsHookEx, GA_ROOT, HWND_TOP, MSLLHOOKSTRUCT,
        SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_SHOW, WH_MOUSE_LL, WM_LBUTTONDBLCLK,
        WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDBLCLK, WM_MBUTTONDOWN, WM_MBUTTONUP,
        WM_NCLBUTTONDOWN, WM_NCLBUTTONUP, WM_NCRBUTTONDBLCLK, WM_NCRBUTTONDOWN, WM_NCRBUTTONUP,
        WM_NCMBUTTONDOWN, WM_NCMBUTTONUP, WM_NCXBUTTONDOWN, WM_NCXBUTTONUP, WM_RBUTTONDBLCLK,
        WM_RBUTTONDOWN, WM_RBUTTONUP, WM_XBUTTONDOWN, WM_XBUTTONUP,
    };

    /// 菜单窗口根 HWND（0 = 未武装）
    static MENU_HWND: AtomicIsize = AtomicIsize::new(0);
    /// 已安装的 WH_MOUSE_LL 钩子句柄（0 = 未安装）；钩子装在的主线程消息泵一直
    /// 在跑，回调能第一时间被派发，不会因为独立线程睡眠而拖慢全系统鼠标
    static HOOK: AtomicIsize = AtomicIsize::new(0);
    /// 主线程定时器 id（0 = 未创建），用于检查菜单可见性并收尾卸钩
    static TIMER_ID: AtomicUsize = AtomicUsize::new(0);
    /// 钩子回调是否处于拦截状态
    static ARMED: AtomicBool = AtomicBool::new(false);
    /// 已吞掉一次按下，需连带吞掉配对的抬起（否则目标窗口会收到孤立 UP）
    static SWALLOW_UP: AtomicBool = AtomicBool::new(false);
    /// 钩子请求关闭菜单（由定时器回调在主线程执行）
    static CLOSE_REQUESTED: AtomicBool = AtomicBool::new(false);
    /// 本次弹出时刻，用于宽限期
    static SHOW_TICK: AtomicU64 = AtomicU64::new(0);
    /// 钩子安装失败时轮询兜底的上一轮按键状态
    static PREVIOUS_DOWN: AtomicBool = AtomicBool::new(false);
    /// 菜单窗口句柄获取入口（定时器回调拿不到上下文，存一份）
    static APP: LazyLock<Mutex<Option<AppHandle>>> = LazyLock::new(|| Mutex::new(None));

    /// 弹出后忽略这段时间内的点击：右键打开菜单时鼠标仍处于按下状态，
    /// 不过滤会刚弹出就被同一次操作关掉。
    const GRACE_MS: u64 = 250;
    /// 主线程定时器间隔：只用于「菜单已收起→卸钩」和钩子安装失败时的轮询兜底
    const TICK_MS: u32 = 30;

    /// 是否已真正取得前台：用当前前台窗口校验，不看 API 返回值（失败时返回值不可信）。
    unsafe fn is_foreground(hwnd: HWND) -> bool {
        let fg = GetForegroundWindow();
        !fg.is_null() && GetAncestor(fg, GA_ROOT) as isize == hwnd as isize
    }

    /// 串联输入队列后激活：与当前前台线程、菜单窗口线程双向 AttachThreadInput，
    /// 这是非前台进程抢前台最可靠的一级（要求本线程有消息队列，托盘回调所在的
    /// tao 事件循环线程满足该条件）。无论成败都成对解除附加。
    unsafe fn activate_via_attach(hwnd: HWND) -> bool {
        let cur = GetCurrentThreadId();
        let fg = GetForegroundWindow();
        let fg_tid = if fg.is_null() {
            0
        } else {
            GetWindowThreadProcessId(fg, std::ptr::null_mut())
        };
        let menu_tid = GetWindowThreadProcessId(hwnd, std::ptr::null_mut());
        let mut attached_fg = false;
        let mut attached_menu = false;
        if fg_tid != 0 && fg_tid != cur {
            attached_fg = AttachThreadInput(cur, fg_tid, TRUE) != 0;
        }
        if menu_tid != 0 && menu_tid != cur && menu_tid != fg_tid {
            attached_menu = AttachThreadInput(cur, menu_tid, TRUE) != 0;
        }
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
        SetActiveWindow(hwnd);
        SetFocus(hwnd);
        let ok = is_foreground(hwnd);
        if attached_menu {
            AttachThreadInput(cur, menu_tid, 0);
        }
        if attached_fg {
            AttachThreadInput(cur, fg_tid, 0);
        }
        ok
    }

    /// 注入一帧 ALT 按下 + 抬起：`LockSetForegroundWindow` 文档规定按下 ALT 会让系统
    /// 重新放开 SetForegroundWindow 调用，且不需要改用户的前台锁超时设置。
    unsafe fn tap_alt() {
        let key = |flags: u32| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_MENU,
                    wScan: 0x38, // Alt 的扫描码，注入结果与真实按键一致
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let inputs = [key(0), key(KEYEVENTF_KEYUP)];
        let _ = SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        );
    }

    /// 强制让托盘菜单成为前台窗口，逐级升级；返回最终是否成功（失败时靠点击守护兜底）。
    pub fn force_foreground(hwnd: HWND) -> bool {
        unsafe {
            if is_foreground(hwnd) {
                return true;
            }
            // 1) 常规：置顶 + 显示 + SetForegroundWindow
            SetWindowPos(
                hwnd,
                HWND_TOP,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
            BringWindowToTop(hwnd);
            SetForegroundWindow(hwnd);
            if is_foreground(hwnd) {
                return true;
            }
            // 2) 串联输入队列后重试
            if activate_via_attach(hwnd) {
                return true;
            }
            // 3) 注入 ALT 解除前台锁后再试一次
            tap_alt();
            if activate_via_attach(hwnd) {
                return true;
            }
            // 4) 最后兜底：Alt+Tab 使用的老接口
            SwitchToThisWindow(hwnd, TRUE);
            if is_foreground(hwnd) {
                return true;
            }
            log::warn!("[tray] 菜单未能取得前台焦点，本次由点击守护负责关闭");
            false
        }
    }

    /// 菜单弹出后调用：记录 HWND、重新开始宽限期，并在**主线程**上装好钩子与定时器。
    ///
    /// 为什么不开专用线程装钩子：低级钩子回调需要安装线程泵消息才会执行，而系统
    /// 在等回调返回前会挂起该次输入；专用线程的任何一次睡眠（如 Sleep(20ms)）都会
    /// 把全局鼠标输入卡住同样时长。装在 tao 事件循环线程（即托盘回调所在线程），
    /// 该线程本身一直在阻塞等消息，回调能第一时间派发。
    pub fn arm(app: &AppHandle, hwnd: HWND) {
        if let Ok(mut slot) = APP.lock() {
            *slot = Some(app.clone());
        }
        unsafe {
            SHOW_TICK.store(GetTickCount64(), Ordering::SeqCst);
            MENU_HWND.store(hwnd as isize, Ordering::SeqCst);
            SWALLOW_UP.store(false, Ordering::SeqCst);
            CLOSE_REQUESTED.store(false, Ordering::SeqCst);
            PREVIOUS_DOWN.store(false, Ordering::SeqCst);
            ARMED.store(true, Ordering::SeqCst);
            if HOOK.load(Ordering::SeqCst) == 0 {
                let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(hook_proc), std::ptr::null_mut(), 0);
                if hook.is_null() {
                    // 极罕见（被安全策略拦掉）：退化为定时器轮询，不吞点击但保证能关掉菜单
                    log::warn!("[tray] 点击守护钩子安装失败，退化为轮询模式");
                } else {
                    HOOK.store(hook as isize, Ordering::SeqCst);
                }
            }
            if TIMER_ID.load(Ordering::SeqCst) == 0 {
                let id = SetTimer(std::ptr::null_mut(), 0, TICK_MS, Some(tick_proc));
                if id == 0 {
                    // 没有定时器就没人卸钩，会把钩子常驻→全系统输入变慢，立刻回退
                    log::warn!("[tray] 点击守护定时器创建失败，本次仅依赖失焦关闭");
                    if HOOK.load(Ordering::SeqCst) != 0 {
                        disarm();
                    }
                    return;
                }
                TIMER_ID.store(id, Ordering::SeqCst);
            }
        }
    }

    /// 卸下钩子与定时器并复位状态（只能在主线程调用）。
    unsafe fn disarm() {
        let hook = HOOK.swap(0, Ordering::SeqCst);
        if hook != 0 {
            UnhookWindowsHookEx(hook as *mut _);
        }
        let id = TIMER_ID.swap(0, Ordering::SeqCst);
        if id != 0 {
            KillTimer(std::ptr::null_mut(), id);
        }
        ARMED.store(false, Ordering::SeqCst);
        SWALLOW_UP.store(false, Ordering::SeqCst);
        MENU_HWND.store(0, Ordering::SeqCst);
    }

    /// 主线程定时器回调：菜单一不可见（失焦/菜单项/Esc 等任何关闭路径）立即卸钩，
    /// 不给系统输入加常驻延迟；同时也处理钩子提出的关闭请求与轮询兜底。
    unsafe extern "system" fn tick_proc(_: HWND, _: u32, _: usize, _: u32) {
        let hwnd = MENU_HWND.load(Ordering::SeqCst) as HWND;
        if hwnd.is_null() || IsWindow(hwnd) == 0 || IsWindowVisible(hwnd) == 0 {
            disarm();
            return;
        }
        if CLOSE_REQUESTED.swap(false, Ordering::SeqCst) {
            hide_menu();
        } else if HOOK.load(Ordering::SeqCst) == 0 && poll_outside_click(hwnd) {
            hide_menu();
        }
    }

    /// 收起菜单（定时器回调已在主线程，直接操作窗口）。
    fn hide_menu() {
        let Some(app) = APP
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
        else {
            return;
        };
        if let Some(w) = app.get_webview_window("tray-menu") {
            let _ = w.set_always_on_top(false);
            let _ = w.hide();
        }
    }

    fn is_press(msg: u32) -> bool {
        matches!(
            msg,
            WM_LBUTTONDOWN
                | WM_RBUTTONDOWN
                | WM_MBUTTONDOWN
                | WM_XBUTTONDOWN
                | WM_LBUTTONDBLCLK
                | WM_RBUTTONDBLCLK
                | WM_MBUTTONDBLCLK
                | WM_NCLBUTTONDOWN
                | WM_NCRBUTTONDOWN
                | WM_NCMBUTTONDOWN
                | WM_NCXBUTTONDOWN
        )
    }

    fn is_release(msg: u32) -> bool {
        matches!(
            msg,
            WM_LBUTTONUP
                | WM_RBUTTONUP
                | WM_MBUTTONUP
                | WM_XBUTTONUP
                | WM_NCLBUTTONUP
                | WM_NCRBUTTONUP
                | WM_NCMBUTTONUP
                | WM_NCXBUTTONUP
        )
    }

    /// 点是否落在菜单窗口矩形外。取不到矩形时按「外部」处理：宁可多关一次也不留残菜单。
    unsafe fn point_outside_menu(hwnd: HWND, pt: POINT) -> bool {
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return true;
        }
        pt.x < rect.left || pt.x > rect.right || pt.y < rect.top || pt.y > rect.bottom
    }

    /// 点击是否落在我们自己的托盘图标上（含 4px 容差：高分屏缩放后边界常有 1px 误差）。
    unsafe fn on_own_tray_icon(pt: POINT) -> bool {
        let Some((l, t, r, b)) = super::tray_icon_rect() else {
            return false;
        };
        pt.x >= l - 4 && pt.x <= r + 4 && pt.y >= t - 4 && pt.y <= b + 4
    }

    /// 点击是否落在 Shell UI（任务栏 / 开始菜单 / 通知中心 / 托盘溢出面板）上。
    /// 只用本地开销较小的判定：`WindowFromPoint` 会跨进程发送 WM_NCHITTEST，
    /// 在钩子回调（本线程即主线程）里调用可能因目标窗口繁忙而长时间阻塞，
    /// 反而把全系统鼠标卡住，故改用下面两路：
    /// 1) 点落在「显示器矩形 - 工作区矩形」的任务栏条带内（几何判定，不依赖类名）；
    /// 2) 当前前台窗口是 Shell 浮层（开始菜单/通知中心/隐藏图标面板这类会取前台）。
    unsafe fn over_shell_ui(pt: POINT) -> bool {
        const SHELL_CLASSES: [&str; 6] = [
            "Shell_TrayWnd",
            "Shell_SecondaryTrayWnd",
            "NotifyIconOverflowWindow",
            "TopLevelWindowForOverflowXamlIsland",
            "Windows.UI.Core.CoreWindow",
            "Shell_RoamingWindow",
        ];
        // 1) 任务栏条带
        let hmon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        if !hmon.is_null() {
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(hmon, &mut info) != 0 {
                let m = info.rcMonitor;
                let w = info.rcWork;
                let in_monitor =
                    pt.x >= m.left && pt.x < m.right && pt.y >= m.top && pt.y < m.bottom;
                let in_work = pt.x >= w.left && pt.x < w.right && pt.y >= w.top && pt.y < w.bottom;
                if in_monitor && !in_work {
                    return true;
                }
            }
        }
        // 2) 前台窗口就是 Shell 浮层
        let fg = GetForegroundWindow();
        if fg.is_null() {
            return false;
        }
        let root = GetAncestor(fg, GA_ROOT);
        let mut buf = [0u16; 64];
        let n = GetClassNameW(root, buf.as_mut_ptr(), buf.len() as i32);
        if n <= 0 {
            return false;
        }
        let cls = String::from_utf16_lossy(&buf[..n as usize]);
        SHELL_CLASSES.contains(&cls.as_str())
    }

    /// 是否吞掉这次用于关闭菜单的点击。
    /// - 自己图标上的右键：吞。否则 Explorer 会再投一次托盘事件把菜单重新弹出来。
    /// - 自己图标上的左键：不吞。保持「左键点图标打开主窗口」既有行为（菜单同时收起）。
    /// - 任务栏 / 开始菜单 / 通知中心：不吞，让这些 Shell UI 照常响应。
    /// - 其他位置（桌面、别的程序客户区）：吞掉，与 Windows 原生弹出菜单一致。
    unsafe fn should_swallow(msg: u32, pt: POINT) -> bool {
        if on_own_tray_icon(pt) {
            return matches!(
                msg,
                WM_RBUTTONDOWN | WM_NCRBUTTONDOWN | WM_RBUTTONDBLCLK | WM_NCRBUTTONDBLCLK
            );
        }
        !over_shell_ui(pt)
    }

    /// 低级鼠标钩子回调（跑在主线程上，必须极快返回）：只做原子读 + 必要时几次
    /// 本地 user32 调用，绝不做跨进程命中测试。
    unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 && ARMED.load(Ordering::SeqCst) {
            let msg = wparam as u32;
            let info = lparam as *const MSLLHOOKSTRUCT;
            if !info.is_null() {
                let pt = (*info).pt;
                if is_press(msg) {
                    let in_grace =
                        GetTickCount64().wrapping_sub(SHOW_TICK.load(Ordering::SeqCst)) < GRACE_MS;
                    let hwnd = MENU_HWND.load(Ordering::SeqCst) as HWND;
                    if !in_grace && !hwnd.is_null() && point_outside_menu(hwnd, pt) {
                        CLOSE_REQUESTED.store(true, Ordering::SeqCst);
                        if should_swallow(msg, pt) {
                            SWALLOW_UP.store(true, Ordering::SeqCst);
                            return 1;
                        }
                    }
                } else if is_release(msg) && SWALLOW_UP.swap(false, Ordering::SeqCst) {
                    return 1;
                }
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
    }

    /// 钩子安装失败时的兜底：轮询按键按下边沿 + 光标位置。此路径不吞点击，但保证能关闭。
    /// 同时看 GetAsyncKeyState 的两个位：符号位是当前按下状态（配合上一轮做边沿
    /// 检测），低位表示自上次调用以来被按下过，避免 30ms 间隔内一闪而过的点击漏报。
    unsafe fn poll_outside_click(hwnd: HWND) -> bool {
        if GetTickCount64().wrapping_sub(SHOW_TICK.load(Ordering::SeqCst)) < GRACE_MS {
            return false;
        }
        let l = GetAsyncKeyState(VK_LBUTTON as i32);
        let r = GetAsyncKeyState(VK_RBUTTON as i32);
        let m = GetAsyncKeyState(VK_MBUTTON as i32);
        let edge = (l & 1) != 0 || (r & 1) != 0 || (m & 1) != 0;
        let down =
            (l as u16 & 0x8000) != 0 || (r as u16 & 0x8000) != 0 || (m as u16 & 0x8000) != 0;
        let prev = PREVIOUS_DOWN.swap(down, Ordering::SeqCst);
        if !edge && !(down && !prev) {
            return false;
        }
        let mut pt: POINT = std::mem::zeroed();
        if GetCursorPos(&mut pt) == 0 {
            return true;
        }
        point_outside_menu(hwnd, pt)
    }
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
    for label in &["main", "tray-menu", "desktop-lyrics", "vertical-overlay"] {
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
