//! 准星「按住打开（热键）」模式
//!
//! 开启后，按住绑定的按键（支持鼠标左键/右键/中键/侧键及键盘按键）
//! 时显示准星，松开后自动隐藏。使用低层钩子（WH_MOUSE_LL + WH_KEYBOARD_LL）
//! 精确监听按下/松开边沿，避免 GetAsyncKeyState 轮询在游戏中漏报。

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tauri::Emitter;

/// 按住模式总开关（持久化键：crosshair-hold-enabled）
static HOLD_ENABLED: AtomicBool = AtomicBool::new(false);
/// 当前绑定的按键（持久化键：crosshair-hold-key）
static HOLD_KEY: Mutex<Option<String>> = Mutex::new(None);
/// 当前生效的解析后绑定
static CURRENT_BINDING: Mutex<Option<Binding>> = Mutex::new(None);
/// 松手时是否由按住模式接管（ownership：仅接管自己按下的准星）
static HELD_BY_HOLD: AtomicBool = AtomicBool::new(false);
/// 主键当前是否处于按下状态（用于边沿去抖）
static BINDING_DOWN: AtomicBool = AtomicBool::new(false);
/// 按下后延迟显示的毫秒数（0 = 立即显示；持久化键：crosshair-hold-delay）
static HOLD_DELAY_MS: AtomicU32 = AtomicU32::new(0);
/// 延迟代次：每次按下/松开/换绑定都递增，用于让过期的延迟线程自动作废
static DELAY_GEN: AtomicU32 = AtomicU32::new(0);

static APP_HANDLE: Mutex<Option<tauri::AppHandle>> = Mutex::new(None);
static MONITOR_RUNNING: AtomicBool = AtomicBool::new(false);
static MONITOR_STOP: AtomicBool = AtomicBool::new(false);
static MONITOR_THREAD: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);
/// 钩子线程 id（线程内登记，用于投递 WM_QUIT 唤醒阻塞的消息循环）
static MONITOR_THREAD_ID: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, Debug)]
struct Binding {
    vk: i32,
    is_mouse: bool,
    ctrl: bool,
    shift: bool,
    alt: bool,
    win: bool,
}

pub fn is_hold_enabled() -> bool {
    HOLD_ENABLED.load(Ordering::SeqCst)
}

pub fn get_hold_key() -> String {
    HOLD_KEY.lock().unwrap().clone().unwrap_or_default()
}

fn set_hold_key_internal(key: &str) {
    *HOLD_KEY.lock().unwrap() = Some(key.to_string());
}

/// 键盘 token → Windows 虚拟键码（与 hotkey-recorder.tsx 的 keyToHotkeyFormat 输出对齐）
fn key_token_to_vk(token: &str) -> Option<i32> {
    if let Some(c) = token.chars().next() {
        if token.len() == 1 {
            if c.is_ascii_alphabetic() {
                return Some(0x41 + (c.to_ascii_uppercase() as u8 - b'A') as i32);
            }
            if c.is_ascii_digit() {
                return Some(0x30 + (c as u8 - b'0') as i32);
            }
        }
    }
    if let Some(rest) = token.strip_prefix('F') {
        if let Ok(n) = rest.parse::<i32>() {
            if (1..=24).contains(&n) {
                return Some(0x70 + n - 1);
            }
        }
    }
    match token {
        "Up" => Some(0x26),
        "Down" => Some(0x28),
        "Left" => Some(0x25),
        "Right" => Some(0x27),
        "Space" => Some(0x20),
        "Escape" => Some(0x1B),
        "Tab" => Some(0x09),
        "Enter" => Some(0x0D),
        "Backspace" => Some(0x08),
        "Delete" => Some(0x2E),
        "Home" => Some(0x24),
        "End" => Some(0x23),
        "PageUp" => Some(0x21),
        "PageDown" => Some(0x22),
        "Insert" => Some(0x2D),
        "Pause" => Some(0x13),
        "ScrollLock" => Some(0x91),
        "CapsLock" => Some(0x14),
        "NumLock" => Some(0x90),
        "PrintScreen" => Some(0x2C),
        "Minus" => Some(0xBD),
        "Equal" => Some(0xBB),
        "[" => Some(0xDB),
        "]" => Some(0xDD),
        "Backslash" => Some(0xDC),
        "Semicolon" => Some(0xBA),
        "Quote" => Some(0xDE),
        "Comma" => Some(0xBC),
        "Period" => Some(0xBE),
        "Slash" => Some(0xBF),
        "Backquote" => Some(0xC0),
        _ => None,
    }
}

/// 鼠标按键 token → 虚拟键码
/// MouseLeft=0x01, MouseRight=0x02, MouseMiddle=0x04, MouseX1=0x05, MouseX2=0x06
fn mouse_token_to_vk(token: &str) -> Option<i32> {
    match token {
        "MouseLeft" => Some(0x01),
        "MouseRight" => Some(0x02),
        "MouseMiddle" => Some(0x04),
        "MouseX1" => Some(0x05),
        "MouseX2" => Some(0x06),
        _ => None,
    }
}

/// 解析快捷键字符串（如 "MouseRight"、"Ctrl+Alt+C"、"F8"）为绑定信息
fn parse_binding(s: &str) -> Option<Binding> {
    if s.trim().is_empty() {
        return None;
    }
    let mut ctrl = false;
    let mut shift = false;
    let mut alt = false;
    let mut win = false;
    let mut main: Option<&str> = None;
    for token in s.split('+') {
        let t = token.trim();
        if t.is_empty() {
            continue;
        }
        match t {
            "Ctrl" => ctrl = true,
            "Shift" => shift = true,
            "Alt" => alt = true,
            "Command" => win = true,
            _ => main = Some(t),
        }
    }
    let main = main?;
    if let Some(vk) = mouse_token_to_vk(main) {
        return Some(Binding {
            vk,
            is_mouse: true,
            ctrl,
            shift,
            alt,
            win,
        });
    }
    Some(Binding {
        vk: key_token_to_vk(main)?,
        is_mouse: false,
        ctrl,
        shift,
        alt,
        win,
    })
}

/// 查询准星当前是否激活（委托给 crosshair 模块）
fn crosshair_active() -> bool {
    #[cfg(target_os = "windows")]
    {
        crate::crosshair::is_active()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// 按住模式显示准星：仅当准星当前未激活时启动并接管
fn show_crosshair(app: &tauri::AppHandle) {
    if crosshair_active() {
        // 准星已处于显示状态（例如手动开关已启用），不接管，松开时不关闭
        HELD_BY_HOLD.store(false, Ordering::SeqCst);
        return;
    }
    #[cfg(target_os = "windows")]
    {
        if crate::crosshair::start(crate::crosshair::get_settings()).is_ok() {
            HELD_BY_HOLD.store(true, Ordering::SeqCst);
            let _ = app.emit("crosshair-status-changed", ());
        }
    }
}

/// 按住模式隐藏准星：仅当由按住模式接管时才关闭（关闭不延迟，立即生效）
fn hide_crosshair(app: &tauri::AppHandle) {
    // 作废仍在等待的延迟显示线程
    DELAY_GEN.fetch_add(1, Ordering::SeqCst);
    if HELD_BY_HOLD.swap(false, Ordering::SeqCst) {
        #[cfg(target_os = "windows")]
        {
            let _ = crate::crosshair::stop();
            let _ = app.emit("crosshair-status-changed", ());
        }
    }
}

/// 按下边沿处理：根据延迟配置立即显示，或起线程等待后再显示
fn handle_binding_down(app: &tauri::AppHandle) {
    let delay = HOLD_DELAY_MS.load(Ordering::SeqCst);
    if delay == 0 {
        show_crosshair(app);
        return;
    }
    // 延迟模式：登记本次按下的代次，松开或再次按下都会使旧线程作废
    let gen = DELAY_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(delay as u64));
        // 代次已变（期间松开/重新按下/停止监听）则放弃显示
        if DELAY_GEN.load(Ordering::SeqCst) != gen || !BINDING_DOWN.load(Ordering::SeqCst) {
            return;
        }
        if MONITOR_STOP.load(Ordering::SeqCst) {
            return;
        }
        show_crosshair(&app);
    });
}

/// 根据门控（热键总开关 && 准星热键独立开关 && 按住模式开关）启动/停止监听
pub fn apply(app: &tauri::AppHandle) {
    let gate =
        is_hold_enabled() && crate::hotkey::is_hotkeys_enabled() && crate::hotkey::is_crosshair_enabled();
    #[cfg(target_os = "windows")]
    {
        if gate {
            win_hooks::set_monitor(app.clone(), true);
        } else {
            win_hooks::set_monitor(app.clone(), false);
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, gate);
    }
}

/// 启动时载入持久化配置（供 lib.rs setup 调用）
pub fn init(app: &tauri::AppHandle, enabled: bool, key: &str, delay_ms: u32) {
    HOLD_ENABLED.store(enabled, Ordering::SeqCst);
    HOLD_DELAY_MS.store(delay_ms, Ordering::SeqCst);
    let key = if key.trim().is_empty() {
        "MouseRight".to_string()
    } else {
        key.to_string()
    };
    set_hold_key_internal(&key);
    apply(app);
}

/// 退出清理
pub fn cleanup() {
    #[cfg(target_os = "windows")]
    win_hooks::stop_hook_thread();
    #[cfg(not(target_os = "windows"))]
    {
        MONITOR_STOP.store(true, Ordering::SeqCst);
        MONITOR_RUNNING.store(false, Ordering::SeqCst);
        if let Some(handle) = MONITOR_THREAD.lock().unwrap().take() {
            let _ = handle.join();
        }
    }
    *CURRENT_BINDING.lock().unwrap() = None;
    BINDING_DOWN.store(false, Ordering::SeqCst);
    *APP_HANDLE.lock().unwrap() = None;
    HELD_BY_HOLD.store(false, Ordering::SeqCst);
}

#[tauri::command]
pub async fn get_crosshair_hold_enabled() -> bool {
    is_hold_enabled()
}

#[tauri::command]
pub async fn set_crosshair_hold_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    HOLD_ENABLED.store(enabled, Ordering::SeqCst);
    crate::hotkey::save_settings_value(
        &app,
        "crosshair-hold-enabled",
        serde_json::Value::Bool(enabled),
    );
    apply(&app);
    log::info!(
        "准星按住模式开关: {}",
        if enabled { "开启" } else { "关闭" }
    );
    Ok(())
}

#[tauri::command]
pub async fn get_crosshair_hold_key() -> String {
    get_hold_key()
}

#[tauri::command]
pub async fn set_crosshair_hold_key(app: tauri::AppHandle, key: String) -> Result<(), String> {
    set_hold_key_internal(&key);
    crate::hotkey::save_settings_value(
        &app,
        "crosshair-hold-key",
        serde_json::Value::String(key),
    );
    apply(&app);
    log::info!("准星按住键位已更新: {}", get_hold_key());
    Ok(())
}

#[tauri::command]
pub async fn get_crosshair_hold_delay() -> u32 {
    HOLD_DELAY_MS.load(Ordering::SeqCst)
}

#[tauri::command]
pub async fn set_crosshair_hold_delay(app: tauri::AppHandle, delay_ms: u32) -> Result<(), String> {
    let delay_ms = delay_ms.min(10_000);
    HOLD_DELAY_MS.store(delay_ms, Ordering::SeqCst);
    // 配置变更时作废所有等待中的延迟线程
    DELAY_GEN.fetch_add(1, Ordering::SeqCst);
    crate::hotkey::save_settings_value(
        &app,
        "crosshair-hold-delay",
        serde_json::Value::from(delay_ms),
    );
    log::info!("准星按住显示延迟已更新: {}ms", delay_ms);
    Ok(())
}

/// Windows 低层钩子实现
#[cfg(target_os = "windows")]
mod win_hooks {
    use super::*;
    use windows_sys::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    /// LLKHF_UP：按键已松开
    const LLKHF_UP: u32 = 0x80;

    fn is_down(vk: i32) -> bool {
        (unsafe { GetAsyncKeyState(vk) } as i32 & 0x8000) != 0
    }

    /// 校验修饰键（ctrl/shift/alt/win 左右变体都算）与绑定一致
    fn modifiers_match(b: &Binding) -> bool {
        let ctrl = is_down(0x11) || is_down(0xA2) || is_down(0xA3);
        let shift = is_down(0x10) || is_down(0xA0) || is_down(0xA1);
        let alt = is_down(0x12) || is_down(0xA4) || is_down(0xA5);
        let win = is_down(0x5B) || is_down(0x5C);
        ctrl == b.ctrl && shift == b.shift && alt == b.alt && win == b.win
    }

    /// 低层键盘钩子回调：监听主键按下/松开边沿
    unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let binding = CURRENT_BINDING.lock().unwrap().clone();
            if let Some(b) = binding {
                if !b.is_mouse {
                    let kb = lparam as *const KBDLLHOOKSTRUCT;
                    if !kb.is_null() {
                        let vk = (*kb).vkCode as i32;
                        if vk == b.vk {
                            let up = (*kb).flags & LLKHF_UP != 0;
                            if up {
                                if BINDING_DOWN.swap(false, Ordering::SeqCst) {
                                    if let Some(app) = APP_HANDLE.lock().unwrap().clone() {
                                        hide_crosshair(&app);
                                    }
                                }
                            } else if !BINDING_DOWN.swap(true, Ordering::SeqCst) {
                                if let Some(app) = APP_HANDLE.lock().unwrap().clone() {
                                    // 含修饰键的组合需先按修饰键（按下边沿时刻校验）
                                    let no_mods = !b.ctrl && !b.shift && !b.alt && !b.win;
                                    if no_mods || modifiers_match(&b) {
                                        handle_binding_down(&app);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
    }

    /// 低层鼠标钩子回调：监听鼠标左/右/中/侧键按下松开边沿
    unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 {
            let binding = CURRENT_BINDING.lock().unwrap().clone();
            if let Some(b) = binding {
                if b.is_mouse {
                    let msg = wparam as u32;
                    // 低层钩子的事件数据在 lParam 指向的 MSLLHOOKSTRUCT（X 侧键编号在 mouseData 高 16 位）
                    let ms = lparam as *const MSLLHOOKSTRUCT;
                    let mut matched = false;
                    let mut is_down_msg = false;
                    match msg {
                        WM_LBUTTONDOWN if b.vk == 0x01 => {
                            matched = true;
                            is_down_msg = true;
                        }
                        WM_LBUTTONUP if b.vk == 0x01 => {
                            matched = true;
                            is_down_msg = false;
                        }
                        WM_RBUTTONDOWN if b.vk == 0x02 => {
                            matched = true;
                            is_down_msg = true;
                        }
                        WM_RBUTTONUP if b.vk == 0x02 => {
                            matched = true;
                            is_down_msg = false;
                        }
                        WM_MBUTTONDOWN if b.vk == 0x04 => {
                            matched = true;
                            is_down_msg = true;
                        }
                        WM_MBUTTONUP if b.vk == 0x04 => {
                            matched = true;
                            is_down_msg = false;
                        }
                        WM_XBUTTONDOWN | WM_XBUTTONUP => {
                            // MSLLHOOKSTRUCT.mouseData 高 16 位：1=XBUTTON1, 2=XBUTTON2
                            let x = if ms.is_null() {
                                0
                            } else {
                                (((*ms).mouseData >> 16) & 0xFFFF) as i32
                            };
                            let want = if b.vk == 0x05 {
                                1
                            } else if b.vk == 0x06 {
                                2
                            } else {
                                0
                            };
                            if x == want {
                                matched = true;
                                is_down_msg = msg == WM_XBUTTONDOWN;
                            }
                        }
                        _ => {}
                    }
                    if matched {
                        if is_down_msg {
                            if !BINDING_DOWN.swap(true, Ordering::SeqCst) {
                                if let Some(app) = APP_HANDLE.lock().unwrap().clone() {
                                    handle_binding_down(&app);
                                }
                            }
                        } else if BINDING_DOWN.swap(false, Ordering::SeqCst) {
                            if let Some(app) = APP_HANDLE.lock().unwrap().clone() {
                                hide_crosshair(&app);
                            }
                        }
                    }
                }
            }
        }
        CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
    }

    /// 钩子线程体：安装键盘+鼠标低层钩子后进入阻塞式消息循环。
    /// 低层钩子回调只在线程泵消息时被调用，且系统派发输入事件会同步等待钩子返回，
    /// 因此必须用 GetMessageW 阻塞等待（事件一到立即处理）；不能用 Peek+sleep 轮询，
    /// 否则 sleep 期间全系统输入事件都会被积压，表现为光标移动卡顿。
    fn hook_thread() {
        unsafe {
            MONITOR_THREAD_ID.store(GetCurrentThreadId(), Ordering::SeqCst);
            if MONITOR_STOP.load(Ordering::SeqCst) {
                MONITOR_RUNNING.store(false, Ordering::SeqCst);
                return;
            }
            let kb_hook = SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard_proc),
                std::ptr::null::<c_void>() as HINSTANCE,
                0,
            );
            if kb_hook.is_null() {
                MONITOR_RUNNING.store(false, Ordering::SeqCst);
                log::error!("[CrosshairHold] 安装键盘低层钩子失败");
                return;
            }
            let ms_hook = SetWindowsHookExW(
                WH_MOUSE_LL,
                Some(mouse_proc),
                std::ptr::null::<c_void>() as HINSTANCE,
                0,
            );
            if ms_hook.is_null() {
                UnhookWindowsHookEx(kb_hook);
                MONITOR_RUNNING.store(false, Ordering::SeqCst);
                log::error!("[CrosshairHold] 安装鼠标低层钩子失败");
                return;
            }
            log::info!("[CrosshairHold] 低层钩子已安装 (键盘/鼠标)");

            let mut msg: MSG = std::mem::zeroed();
            loop {
                // 返回 0 = 收到 WM_QUIT，-1 = 出错；阻塞直到有事件需要处理
                let ret = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
                if ret <= 0 {
                    break;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            UnhookWindowsHookEx(kb_hook);
            UnhookWindowsHookEx(ms_hook);
            log::info!("[CrosshairHold] 低层钩子已卸载");
            MONITOR_RUNNING.store(false, Ordering::SeqCst);
        }
    }

    /// 停止钩子线程：投递 WM_QUIT 唤醒阻塞在 GetMessageW 的循环后再回收线程
    pub fn stop_hook_thread() {
        MONITOR_STOP.store(true, Ordering::SeqCst);
        // 线程刚启动时可能尚未登记 id：短暂等待其登记或自行退出
        let mut tid = MONITOR_THREAD_ID.load(Ordering::SeqCst);
        for _ in 0..250 {
            if tid != 0 || !MONITOR_RUNNING.load(Ordering::SeqCst) {
                break;
            }
            thread::sleep(Duration::from_millis(2));
            tid = MONITOR_THREAD_ID.load(Ordering::SeqCst);
        }
        if tid != 0 {
            // 消息队列可能尚未建立，投递失败时短暂重试
            for _ in 0..250 {
                if unsafe { PostThreadMessageW(tid, WM_QUIT, 0, 0) } != 0 {
                    break;
                }
                if !MONITOR_RUNNING.load(Ordering::SeqCst) {
                    break;
                }
                thread::sleep(Duration::from_millis(2));
            }
        }
        if let Some(handle) = MONITOR_THREAD.lock().unwrap().take() {
            let _ = handle.join();
        }
        MONITOR_THREAD_ID.store(0, Ordering::SeqCst);
    }

    /// 启动/停止按住监听线程
    pub fn set_monitor(app: tauri::AppHandle, active: bool) {
        if !active {
            stop_hook_thread();
            *CURRENT_BINDING.lock().unwrap() = None;
            BINDING_DOWN.store(false, Ordering::SeqCst);
            *APP_HANDLE.lock().unwrap() = None;
            // 若正处于按住显示状态，先关闭准星，避免残留
            if HELD_BY_HOLD.swap(false, Ordering::SeqCst) {
                let _ = crate::crosshair::stop();
                let _ = app.emit("crosshair-status-changed", ());
            }
            return;
        }

        let binding = parse_binding(&get_hold_key());
        if binding.is_none() {
            log::warn!(
                "[CrosshairHold] 绑定的按键无效，按住模式未启动: {}",
                get_hold_key()
            );
            return;
        }
        *CURRENT_BINDING.lock().unwrap() = binding;
        *APP_HANDLE.lock().unwrap() = Some(app);

        if MONITOR_RUNNING.load(Ordering::SeqCst) {
            // 运行中更换绑定：重置按下状态，避免旧键松开事件错位
            BINDING_DOWN.store(false, Ordering::SeqCst);
            return;
        }
        MONITOR_STOP.store(false, Ordering::SeqCst);
        MONITOR_RUNNING.store(true, Ordering::SeqCst);
        let handle = thread::spawn(hook_thread);
        *MONITOR_THREAD.lock().unwrap() = Some(handle);
    }
}