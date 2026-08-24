//! 键盘物理媒体键（播放/暂停、上一曲、下一曲、停止）捕获
//!
//! 背景：主窗口是 WebView2（Chromium）。低层键盘钩子(WH_KEYBOARD_LL)在 WebView 聚焦时
//! 会被 WebView2 自己的媒体键钩子抢在前而拿不到键（表现为：后台工作、前台无反应）。
//! 改用两条与“焦点无关”、由操作系统保证送达的路径：
//!   1. WM_APPCOMMAND：媒体键会转成 WM_APPCOMMAND 发给聚焦窗口；若子控件不处理，
//!      会自动冒泡到父窗口。我们在主窗口(WebView2 的父窗口)子类化拦截它。
//!   2. RegisterHotKey：把媒体 VK 注册为系统全局热键，在任何焦点状态下都投递 WM_HOTKEY。
//! 两路都转发 `smtc:control` 事件给前端（与 smtc.rs 共用事件名，前端无需改动）；
//! 前端已对同一动作做 ~150ms 去重，避免 SMTC 与这两条链路重合时重复响应。

#[cfg(not(target_os = "windows"))]
mod imp {
    /// 非 Windows 平台 no-op
    pub fn start(_app: tauri::AppHandle) {}
}

#[cfg(target_os = "windows")]
mod imp {
    use std::sync::OnceLock;

    use tauri::{AppHandle, Emitter, Manager};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, UnregisterHotKey, VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE,
        VK_MEDIA_PREV_TRACK, VK_MEDIA_STOP,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
        GetMessageW, RegisterClassExW, SetWindowLongPtrW, TranslateMessage, UnregisterClassW,
        WNDCLASSEXW, MSG, WM_APPCOMMAND, WM_HOTKEY,
    };

    /// 供回调线程向前端 emit 使用（与 smtc.rs 一致）
    static APP: OnceLock<AppHandle> = OnceLock::new();
    /// 主窗口原窗口过程（子类化时保存），用于把非媒体类消息原样交回
    static ORIGINAL_WNDPROC: Mutex<Option<isize>> = Mutex::new(None);

    use std::sync::Mutex;

    // RegisterHotKey 热键 id
    const ID_PLAYPAUSE: i32 = 0x10;
    const ID_PREV: i32 = 0x11;
    const ID_NEXT: i32 = 0x12;
    const ID_STOP: i32 = 0x13;
    // WM_APPCOMMAND 的媒体命令 id
    const CMD_MEDIA_NEXTTRACK: i32 = 11;
    const CMD_MEDIA_PREVIOUS: i32 = 12;
    const CMD_MEDIA_STOP: i32 = 13;
    const CMD_MEDIA_PLAY_PAUSE: i32 = 14;
    const CMD_MEDIA_PLAY: i32 = 46;
    const CMD_MEDIA_PAUSE: i32 = 47;

    /// 启动媒体键捕获（在 setup 中调用一次）
    pub fn start(app: AppHandle) {
        let _ = APP.set(app.clone());

        // 路径 1：子类化主窗口，拦截冒泡上来的 WM_APPCOMMAND 媒体命令
        if let Some(win) = app.get_webview_window("main") {
            if let Ok(hwnd) = win.hwnd() {
                let hwnd = hwnd.0 as *mut core::ffi::c_void;
                // GWLP_WNDPROC = -4
                let prev = unsafe { SetWindowLongPtrW(hwnd, -4, wnd_proc as *const () as isize) };
                if prev != 0 {
                    *ORIGINAL_WNDPROC.lock().unwrap() = Some(prev);
                    log::info!("[MediaKeys] 已子类化主窗口拦截 WM_APPCOMMAND 媒体键");
                } else {
                    log::error!(
                        "[MediaKeys] 子类化主窗口失败(错误码 {})",
                        std::io::Error::last_os_error()
                    );
                }
            }
        }

        // 路径 2：RegisterHotKey 注册媒体 VK 为系统全局热键（与焦点无关）
        std::thread::spawn(move || {
            let hinst = unsafe { GetModuleHandleW(std::ptr::null()) };
            let class_name: Vec<u16> = "NexBoxMediaKeyWin\0".encode_utf16().collect();
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: 0,
                lpfnWndProc: Some(hotkey_wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinst,
                hIcon: std::ptr::null_mut(),
                hCursor: std::ptr::null_mut(),
                hbrBackground: std::ptr::null_mut(),
                lpszMenuName: std::ptr::null(),
                lpszClassName: class_name.as_ptr(),
                hIconSm: std::ptr::null_mut(),
            };
            if unsafe { RegisterClassExW(&wc) } == 0 {
                log::error!(
                    "[MediaKeys] 注册消息窗口类失败(错误码 {})",
                    std::io::Error::last_os_error()
                );
                return;
            }
            // 消息专用窗口 HWND_MESSAGE = (HWND)-3
            let hwnd = unsafe {
                CreateWindowExW(
                    0,
                    class_name.as_ptr(),
                    std::ptr::null(),
                    0,
                    0,
                    0,
                    0,
                    0,
                    (-3isize) as *mut core::ffi::c_void,
                    std::ptr::null_mut(),
                    hinst,
                    std::ptr::null(),
                )
            };
            if hwnd.is_null() {
                log::error!(
                    "[MediaKeys] 创建消息窗口失败(错误码 {})",
                    std::io::Error::last_os_error()
                );
                return;
            }
            let hotkeys = [
                (ID_PLAYPAUSE, VK_MEDIA_PLAY_PAUSE),
                (ID_PREV, VK_MEDIA_PREV_TRACK),
                (ID_NEXT, VK_MEDIA_NEXT_TRACK),
                (ID_STOP, VK_MEDIA_STOP),
            ];
            let mut registered = 0usize;
            for (id, vk) in hotkeys {
                // MOD_NOREPEAT = 0x4000，避免按住时重复响应
                if unsafe { RegisterHotKey(hwnd, id, 0x4000u32, vk.into()) } != 0 {
                    registered += 1;
                } else {
                    log::warn!(
                        "[MediaKeys] 注册媒体热键 vk=0x{:X} 失败(错误码 {})",
                        vk,
                        std::io::Error::last_os_error()
                    );
                }
            }
            log::info!("[MediaKeys] 媒体键系统热键注册 {}/{}", registered, hotkeys.len());

            // 消息循环：投递 WM_HOTKEY 到此消息窗口
            let mut msg: MSG = unsafe { std::mem::zeroed() };
            while unsafe { GetMessageW(&mut msg, hwnd, 0, 0) } > 0 {
                unsafe {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            for (id, _) in hotkeys {
                unsafe { UnregisterHotKey(hwnd, id) };
            }
            unsafe {
                DestroyWindow(hwnd);
                UnregisterClassW(class_name.as_ptr(), hinst);
            }
        });
    }

    /// 主窗口子类化窗口过程：拦截 WM_APPCOMMAND 媒体命令 → 转发并吞掉
    unsafe extern "system" fn wnd_proc(
        hwnd: windows_sys::Win32::Foundation::HWND,
        msg: u32,
        wparam: usize,
        lparam: isize,
    ) -> isize {
        if msg == WM_APPCOMMAND {
            if let Some(action) = appcommand_action(lparam) {
                emit_control(action);
                // 已处理：返回 TRUE(1)，阻止继续向 DefaultWindowProc / 壳钩子冒泡
                return 1;
            }
        }
        let orig = ORIGINAL_WNDPROC.lock().unwrap().unwrap_or(0);
        if orig != 0 {
            let p: unsafe extern "system" fn(
                windows_sys::Win32::Foundation::HWND,
                u32,
                usize,
                isize,
            ) -> isize = std::mem::transmute(orig);
            CallWindowProcW(Some(p), hwnd, msg, wparam, lparam)
        } else {
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }

    /// RegisterHotKey 的消息窗口过程：WM_HOTKEY → 转发
    unsafe extern "system" fn hotkey_wnd_proc(
        hwnd: windows_sys::Win32::Foundation::HWND,
        msg: u32,
        wparam: usize,
        _lparam: isize,
    ) -> isize {
        if msg == WM_HOTKEY {
            let action = match wparam as i32 {
                ID_PLAYPAUSE => "play-pause",
                ID_PREV => "prev",
                ID_NEXT => "next",
                ID_STOP => "stop",
                _ => "",
            };
            if action != "" {
                emit_control(action);
                return 0;
            }
        }
        DefWindowProcW(hwnd, msg, wparam, _lparam)
    }

    /// 从 WM_APPCOMMAND 的 lParam 解析媒体命令 → 动作
    fn appcommand_action(lparam: isize) -> Option<&'static str> {
        // cmd = HIWORD(lParam) & ~FAPPCOMMAND_MASK(0x8000)
        let cmd = (((lparam >> 16) & 0xFFFF) as i32) & 0x7FFF;
        match cmd {
            CMD_MEDIA_NEXTTRACK => Some("next"),
            CMD_MEDIA_PREVIOUS => Some("prev"),
            CMD_MEDIA_STOP => Some("stop"),
            CMD_MEDIA_PLAY_PAUSE | CMD_MEDIA_PLAY | CMD_MEDIA_PAUSE => Some("play-pause"),
            _ => None,
        }
    }

    fn emit_control(action: &str) {
        if let Some(app) = APP.get() {
            let _ = app.emit("smtc:control", serde_json::json!({ "action": action }));
        }
    }
}

/// 启动键盘媒体键捕获（在 setup 中调用）
pub fn start(app: tauri::AppHandle) {
    imp::start(app);
}