//! 键盘物理媒体键（播放/暂停、上一曲、下一曲、停止）统一接管
//!
//! 背景：主窗口是 WebView2（Chromium），聚焦时会自行消费 WM_APPCOMMAND，
//! 任何依赖「按键 → 窗口消息」链路的方案（窗口子类化/低层钩子转发）在前台都不可靠。
//! 因此全模式统一使用操作系统保证送达、与焦点无关的 RegisterHotKey 全局热键：
//!
//!   - 开关开启：WM_HOTKEY → 转发 `smtc:control` 给前端控制内置播放器；
//!     同时注册本应用 SMTC 媒体会话（音量浮层/锁屏显示）。
//!   - 开关关闭：热键保持注册（继续抢占物理媒体键，WebView2 无从插手），
//!     但 WM_HOTKEY 改为直接命令外部音乐客户端的 SMTC 会话
//!     （TryTogglePlayPauseAsync 等）——「仅新境盒不响应，其他软件正常」；
//!     同时停用本应用 SMTC 会话（smtc_clear），音量浮层/锁屏不再显示新境盒。
//!
//! 若个别播放器抢注了同一批热键导致本应用注册失败，按键会走系统原生流程直达该
//! 播放器——效果同样正确（原生流程在后台本就可用，前台则因热键被对方持有而直达）。

#[cfg(not(target_os = "windows"))]
mod imp {
    /// 非 Windows 平台 no-op
    pub fn start(_app: tauri::AppHandle) {}
}

#[cfg(target_os = "windows")]
mod imp {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicBool, Ordering};

    use tauri::{AppHandle, Emitter, Manager};
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, UnregisterHotKey, VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE,
        VK_MEDIA_PREV_TRACK, VK_MEDIA_STOP,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
        PeekMessageW, PM_REMOVE, RegisterClassExW, SetWindowLongPtrW, TranslateMessage,
        UnregisterClassW, MSG, WNDCLASSEXW, WM_APPCOMMAND, WM_HOTKEY,
    };

    /// 供回调线程向前端 emit 使用（与 smtc.rs 一致）
    static APP: OnceLock<AppHandle> = OnceLock::new();
    /// 主窗口原窗口过程（子类化时保存），用于把非媒体类消息原样交回
    static ORIGINAL_WNDPROC: Mutex<Option<isize>> = Mutex::new(None);
    /// 媒体键控制总开关（由设置/命令控制）
    static ENABLED: AtomicBool = AtomicBool::new(true);
    /// 主窗口句柄（用于恢复原窗口过程），以 isize 保存避免原始指针非 Send
    static MAIN_HWND: Mutex<Option<isize>> = Mutex::new(None);
    /// RegisterHotKey 消息窗口句柄（应用生命周期内创建一次），以 isize 保存
    static MSG_HWND: Mutex<Option<isize>> = Mutex::new(None);
    /// 消息窗口线程是否已创建
    static THREAD_CREATED: AtomicBool = AtomicBool::new(false);

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

    // RegisterHotKey 热键注册表
    const HOTKEYS: [(i32, u16); 4] = [
        (ID_PLAYPAUSE, VK_MEDIA_PLAY_PAUSE),
        (ID_PREV, VK_MEDIA_PREV_TRACK),
        (ID_NEXT, VK_MEDIA_NEXT_TRACK),
        (ID_STOP, VK_MEDIA_STOP),
    ];

    /// 应用媒体键开关状态。`enabled` 由调用方决定：
    /// setup 时读取设置决定；运行时开关命令由参数强制指定。
    pub fn apply(app: AppHandle, enabled: bool) {
        log::info!("[MediaKeys] apply(enabled={enabled})");
        ENABLED.store(enabled, Ordering::SeqCst);
        let _ = APP.set(app.clone());

        if enabled {
            // 子类化主窗口：兜底拦截冒泡上来的 WM_APPCOMMAND 媒体命令
            install_main_subclass(&app);
        } else {
            restore_main_subclass();
        }

        // 无论开关状态，热键始终注册；按下后按 ENABLED 路由：
        //   开 → 内置播放器 / 关 → 外部 SMTC 会话
        ensure_hotkey_thread();
        request_hotkeys(true);
    }

    /// 常驻子类化主窗口（幂等）：拦截冒泡上来的 WM_APPCOMMAND 媒体命令
    fn install_main_subclass(app: &AppHandle) {
        if MAIN_HWND.lock().unwrap().is_some() {
            return;
        }
        if let Some(win) = app.get_webview_window("main") {
            if let Ok(hwnd) = win.hwnd() {
                let hwnd = hwnd.0 as *mut core::ffi::c_void;
                // GWLP_WNDPROC = -4
                let prev = unsafe { SetWindowLongPtrW(hwnd, -4, wnd_proc as *const () as isize) };
                if prev != 0 {
                    *ORIGINAL_WNDPROC.lock().unwrap() = Some(prev);
                    *MAIN_HWND.lock().unwrap() = Some(hwnd as isize);
                    log::info!("[MediaKeys] 已子类化主窗口拦截 WM_APPCOMMAND 媒体键");
                } else {
                    log::error!(
                        "[MediaKeys] 子类化主窗口失败(错误码 {})",
                        std::io::Error::last_os_error()
                    );
                }
            }
        }
    }

    /// 恢复主窗口原窗口过程（关闭开关时调用；关闭模式下按键由全局热键直接处理）
    fn restore_main_subclass() {
        let orig = ORIGINAL_WNDPROC.lock().unwrap().take();
        let main_hwnd = *MAIN_HWND.lock().unwrap();
        if let (Some(orig), Some(main_hwnd)) = (orig, main_hwnd) {
            unsafe { SetWindowLongPtrW(main_hwnd as HWND, -4, orig) };
            *MAIN_HWND.lock().unwrap() = None;
            log::info!("[MediaKeys] 已恢复主窗口原窗口过程");
        }
    }

    /// 热键控制命令：true=注册（注销暂未使用，保留扩展位）
    static HOTKEY_TX: OnceLock<std::sync::mpsc::Sender<bool>> = OnceLock::new();

    fn ensure_hotkey_thread() {
        if THREAD_CREATED.swap(true, Ordering::SeqCst) {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel::<bool>();
        let _ = HOTKEY_TX.set(tx);
        std::thread::Builder::new()
            .name("media-key-hotkey".into())
            .spawn(move || unsafe {
                let hinst = GetModuleHandleW(std::ptr::null());
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
                if RegisterClassExW(&wc) == 0 {
                    log::error!(
                        "[MediaKeys] 注册消息窗口类失败(错误码 {})",
                        std::io::Error::last_os_error()
                    );
                    return;
                }
                // 消息专用窗口 HWND_MESSAGE = (HWND)-3
                let hwnd = CreateWindowExW(
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
                );
                if hwnd.is_null() {
                    log::error!(
                        "[MediaKeys] 创建消息窗口失败(错误码 {})",
                        std::io::Error::last_os_error()
                    );
                    return;
                }
                *MSG_HWND.lock().unwrap() = Some(hwnd as isize);
                log::info!("[MediaKeys] 热键消息窗口就绪");

                // 消息循环：抽 WM_HOTKEY + 处理注册命令（应用生命周期内持续运行）
                let mut msg: MSG = std::mem::zeroed();
                loop {
                    while PeekMessageW(&mut msg, hwnd, 0, 0, PM_REMOVE) != 0 {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                    match rx.try_recv() {
                        Ok(_) => {
                            let registered = register_all_hotkeys(hwnd);
                            log::info!(
                                "[MediaKeys] 媒体键系统热键注册 {}/{}",
                                registered,
                                HOTKEYS.len()
                            );
                            if registered < HOTKEYS.len() {
                                log::warn!(
                                    "[MediaKeys] 部分热键被其他程序占用，对应按键将按系统原生流程路由"
                                );
                            }
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                        Err(std::sync::mpsc::TryRecvError::Empty) => {}
                    }
                    std::thread::sleep(std::time::Duration::from_millis(15));
                }
                for (id, _) in HOTKEYS {
                    UnregisterHotKey(hwnd, id);
                }
                DestroyWindow(hwnd);
                UnregisterClassW(class_name.as_ptr(), hinst);
            })
            .expect("spawn media-key-hotkey thread");
    }

    /// 请求在热键线程内注册媒体热键。
    /// 通道先于线程创建，线程启动前的请求会被缓冲，不会丢失。
    fn request_hotkeys(_register: bool) {
        if let Some(tx) = HOTKEY_TX.get() {
            let _ = tx.send(true);
        }
    }

    /// 注册全部媒体热键，返回成功数量（MOD_NOREPEAT=0x4000 避免按住重复响应）
    fn register_all_hotkeys(hwnd: HWND) -> usize {
        let mut registered = 0usize;
        for (id, vk) in HOTKEYS {
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
        registered
    }

    /// 当前媒体键捕获是否启用
    pub fn is_enabled() -> bool {
        ENABLED.load(Ordering::SeqCst)
    }

    /// 主窗口子类化窗口过程：拦截 WM_APPCOMMAND 媒体命令 → 转发并吞掉
    unsafe extern "system" fn wnd_proc(
        hwnd: windows_sys::Win32::Foundation::HWND,
        msg: u32,
        wparam: usize,
        lparam: isize,
    ) -> isize {
        if msg == WM_APPCOMMAND {
            if ENABLED.load(Ordering::SeqCst) {
                if let Some(action) = appcommand_action(lparam) {
                    emit_control(action);
                    // 已处理：返回 TRUE(1)，阻止继续向 DefaultWindowProc / 壳钩子冒泡
                    return 1;
                }
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

    /// RegisterHotKey 的消息窗口过程：
    /// - 开启：WM_HOTKEY → 转发前端控制内置播放器
    /// - 关闭：WM_HOTKEY → 直接命令外部音乐客户端的 SMTC 会话（仅新境盒不响应）
    unsafe extern "system" fn hotkey_wnd_proc(
        _hwnd: windows_sys::Win32::Foundation::HWND,
        msg: u32,
        wparam: usize,
        lparam: isize,
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
                if ENABLED.load(Ordering::SeqCst) {
                    emit_control(action);
                } else {
                    log::info!("[MediaKeys] 关闭模式热键 → 转发外部 SMTC 会话: {action}");
                    crate::external_player::forward_media_key(action);
                }
                return 0;
            }
        }
        DefWindowProcW(_hwnd, msg, wparam, lparam)
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
        if !ENABLED.load(Ordering::SeqCst) {
            return;
        }
        if let Some(app) = APP.get() {
            let _ = app.emit("smtc:control", serde_json::json!({ "action": action }));
        }
    }
}

/// 启动键盘媒体键接管（在 setup 中调用）。
/// 读取前端保存的设置 `nexbox_media_keys_enabled`（默认开启），决定路由模式。
pub fn start(app: tauri::AppHandle) {
    #[cfg(target_os = "windows")]
    {
        let enabled = crate::hotkey::read_settings_value(&app, "nexbox_media_keys_enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        log::info!("[MediaKeys] 启动设置 nexbox_media_keys_enabled = {enabled}");
        imp::apply(app, enabled);
    }
    #[cfg(not(target_os = "windows"))]
    imp::start(app);
}

/// 媒体键控制开关当前状态（跨模块读取，供 smtc.rs 决定是否保持本应用媒体会话启用）
#[cfg(target_os = "windows")]
pub fn control_enabled() -> bool {
    imp::is_enabled()
}

#[cfg(not(target_os = "windows"))]
pub fn control_enabled() -> bool {
    true
}

/// 运行时开关：启用/停用键盘媒体键控制（前端「高级」设置切换时调用）。
/// 关闭 = 停用本应用 SMTC 会话 + 热键改为直控外部音乐客户端。
#[tauri::command]
pub fn set_media_keys_enabled(app: tauri::AppHandle, enabled: bool) {
    log::info!("[MediaKeys] 收到开关命令 set_media_keys_enabled({enabled})");
    #[cfg(target_os = "windows")]
    {
        imp::apply(app, enabled);
        if !enabled {
            // 同步停用本应用的 SMTC 媒体会话：否则系统仍会把媒体控制路由给
            // 「新境盒」的会话（前端又因开关关闭不响应）
            crate::smtc::smtc_clear();
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app;
        let _ = enabled;
    }
}
