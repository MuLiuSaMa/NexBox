//! 桌面歌词解锁按钮鼠标钩子管理
//!
//! 解锁按钮已从独立小窗口(`lyrics-unlock-btn`)合并进桌面歌词窗口,
//! 以移除一个 WebView2 webview,降低内存占用。
//!
//! 锁定时歌词窗口整窗鼠标穿透,页面无法接收鼠标事件;
//! 本模块通过 WH_MOUSE_LL 全局低级鼠标钩子监听左键点击:
//! - 命中歌词窗口顶部中央的解锁按钮区域时,吞掉该消息(返回 1,
//!   不让点击穿透到底层应用,与原先独立按钮窗口挡住点击的行为一致),
//!   并 emit `lyrics:unlock-triggered` 事件,由歌词页面完成解锁流程。
//! - 按钮的显隐/悬停态由歌词页面的 200ms 光标轮询驱动,
//!   通过 `set_lyrics_unlock_hook_armed` 控制钩子是否拦截点击。

use tauri::AppHandle;

#[cfg(windows)]
mod imp {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, OnceLock};

    use tauri::{AppHandle, Emitter, Manager};
    use windows_sys::Win32::Foundation::{POINT, RECT};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, GetWindowRect, PostThreadMessageW,
        SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HHOOK, MSG, MSLLHOOKSTRUCT,
        WH_MOUSE_LL, WM_LBUTTONDOWN, WM_QUIT,
    };

    /// 钩子是否已安装
    static HOOK_ACTIVE: AtomicBool = AtomicBool::new(false);
    /// 钩子是否处于"拦截"状态(仅当内嵌解锁按钮可见时需要拦截点击)
    static HOOK_ARMED: AtomicBool = AtomicBool::new(false);
    /// 供钩子回调使用的 AppHandle
    static APP: OnceLock<Mutex<Option<AppHandle>>> = OnceLock::new();
    /// 钩子句柄(以 usize 存储指针,避免 HHOOK 非 Send 无法放入 static Mutex)
    static HOOK_HANDLE: OnceLock<Mutex<Option<usize>>> = OnceLock::new();
    /// 钩子线程 ID(用于发送 WM_QUIT 结束消息循环)
    static HOOK_THREAD_ID: OnceLock<Mutex<Option<u32>>> = OnceLock::new();

    fn app_slot() -> &'static Mutex<Option<AppHandle>> {
        APP.get_or_init(|| Mutex::new(None))
    }

    fn hook_slot() -> &'static Mutex<Option<usize>> {
        HOOK_HANDLE.get_or_init(|| Mutex::new(None))
    }

    fn thread_slot() -> &'static Mutex<Option<u32>> {
        HOOK_THREAD_ID.get_or_init(|| Mutex::new(None))
    }

    fn app_handle() -> Result<AppHandle, String> {
        app_slot()
            .lock()
            .map_err(|_| "hook app handle lock poisoned".to_string())?
            .clone()
            .ok_or_else(|| "hook app handle not initialized".to_string())
    }

    /// 解锁按钮区域几何(与前端 `isCursorInUnlockArea` 保持一致):
    /// 位于歌词窗口顶部中央,宽 min(120, 窗口宽),高 50,物理像素。
    fn unlock_rect_contains(win_rect: &RECT, pt: &POINT) -> bool {
        let win_w = win_rect.right - win_rect.left;
        let w = win_w.min(120);
        let left = win_rect.left + (win_w - w) / 2;
        pt.x >= left && pt.x <= left + w && pt.y >= win_rect.top && pt.y <= win_rect.top + 50
    }

    /// WH_MOUSE_LL 回调:检测解锁区域内的左键按下,命中则吞掉消息并触发解锁事件
    unsafe extern "system" fn low_level_mouse_proc(code: i32, wparam: usize, lparam: isize) -> isize {
        if code >= 0
            && wparam as u32 == WM_LBUTTONDOWN
            && HOOK_ACTIVE.load(Ordering::SeqCst)
            && HOOK_ARMED.load(Ordering::SeqCst)
        {
            let msll = lparam as *const MSLLHOOKSTRUCT;
            if !msll.is_null() {
                let pt = (*msll).pt;
                let hit = app_handle()
                    .ok()
                    .and_then(|app| app.get_webview_window("desktop-lyrics"))
                    // tauri 的 hwnd() 返回 windows crate 的 HWND,取其 .0 原始指针
                    .and_then(|win| win.hwnd().ok().map(|h| h.0))
                    .map(|hwnd| {
                        let mut rect: RECT = std::mem::zeroed();
                        GetWindowRect(hwnd, &mut rect) != 0 && unlock_rect_contains(&rect, &pt)
                    })
                    .unwrap_or(false);
                if hit {
                    if let Ok(app) = app_handle() {
                        let _ = app.emit("lyrics:unlock-triggered", ());
                    }
                    // 吞掉消息,不让点击穿透到底层应用
                    return 1;
                }
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
    }

    pub fn enable(app_handle: AppHandle) -> Result<(), String> {
        if thread_slot()
            .lock()
            .map_err(|_| "hook thread slot lock poisoned".to_string())?
            .is_some()
        {
            return Ok(()); // 钩子已在运行
        }

        if let Ok(mut slot) = app_slot().lock() {
            *slot = Some(app_handle);
        }

        // 专用线程:安装全局钩子并泵消息(WH_MOUSE_LL 回调需要线程持续处理消息)
        let (tx, rx) = std::sync::mpsc::channel::<(u32, bool)>();
        std::thread::spawn(move || {
            unsafe {
                let hook = SetWindowsHookExW(
                    WH_MOUSE_LL,
                    Some(low_level_mouse_proc),
                    std::ptr::null_mut(),
                    0,
                );
                if hook.is_null() {
                    let _ = tx.send((0, false)); // 安装失败:通知调用方
                    return;
                }
                if let Ok(mut slot) = hook_slot().lock() {
                    *slot = Some(hook as usize);
                }
                HOOK_ACTIVE.store(true, Ordering::SeqCst);
                let _ = tx.send((GetCurrentThreadId(), true));
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
                // 消息循环结束(收到 WM_QUIT),卸钩并清理句柄
                UnhookWindowsHookEx(hook);
            }
            HOOK_ACTIVE.store(false, Ordering::SeqCst);
            if let Ok(mut slot) = hook_slot().lock() {
                *slot = None;
            }
        });

        let (tid, installed) = rx
            .recv_timeout(std::time::Duration::from_millis(2000))
            .map_err(|_| "failed to start lyrics unlock hook thread".to_string())?;
        if !installed {
            return Err("failed to install lyrics unlock mouse hook".to_string());
        }
        if let Ok(mut slot) = thread_slot().lock() {
            *slot = Some(tid);
        }
        Ok(())
    }

    pub fn disable() -> Result<(), String> {
        HOOK_ARMED.store(false, Ordering::SeqCst);

        // 主动卸钩(钩子线程退出时也会再次 Unhook,幂等)
        if let Ok(mut slot) = hook_slot().lock() {
            if let Some(hook) = slot.take() {
                unsafe {
                    UnhookWindowsHookEx(hook as HHOOK);
                }
            }
        }

        // 通知钩子线程退出消息循环
        if let Ok(mut slot) = thread_slot().lock() {
            if let Some(tid) = slot.take() {
                unsafe {
                    PostThreadMessageW(tid, WM_QUIT, 0, 0);
                }
            }
        }

        Ok(())
    }

    pub fn set_armed(armed: bool) {
        HOOK_ARMED.store(armed, Ordering::SeqCst);
    }
}

/// 启用解锁检测钩子(进入锁定状态时调用,幂等)
#[cfg(windows)]
#[tauri::command]
pub fn enable_lyrics_unlock_hook(app_handle: AppHandle) -> Result<(), String> {
    imp::enable(app_handle)
}

/// 停用解锁检测钩子(解锁/关闭歌词时调用,幂等)
#[cfg(windows)]
#[tauri::command]
pub fn disable_lyrics_unlock_hook() -> Result<(), String> {
    imp::disable()
}

/// 切换钩子拦截状态:仅当内嵌解锁按钮可见时置 true,钩子才拦截点击
#[cfg(windows)]
#[tauri::command]
pub fn set_lyrics_unlock_hook_armed(armed: bool) -> Result<(), String> {
    imp::set_armed(armed);
    Ok(())
}

/// 退出清理:确保钩子卸载、线程结束
pub fn cleanup() {
    #[cfg(windows)]
    let _ = imp::disable();
}

#[cfg(not(windows))]
#[tauri::command]
pub fn enable_lyrics_unlock_hook(_app_handle: AppHandle) -> Result<(), String> {
    Err("enable_lyrics_unlock_hook is only supported on Windows".to_string())
}

#[cfg(not(windows))]
#[tauri::command]
pub fn disable_lyrics_unlock_hook() -> Result<(), String> {
    Err("disable_lyrics_unlock_hook is only supported on Windows".to_string())
}

#[cfg(not(windows))]
#[tauri::command]
pub fn set_lyrics_unlock_hook_armed(_armed: bool) -> Result<(), String> {
    Err("set_lyrics_unlock_hook_armed is only supported on Windows".to_string())
}