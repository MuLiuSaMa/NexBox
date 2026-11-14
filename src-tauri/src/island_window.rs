//! 桌面灵动岛窗口 —— 把原本内嵌在主窗口标题栏的灵动岛搬到桌面上显示。
//!
//! 设计要点：状态真源仍然留在主窗口的灵动岛 store（提示队列、自动关闭计时、
//! 持久基线等逻辑一行都不搬），本窗口只是「纯渲染器」：
//!   主窗口 store 变更 → 前端 emit("island:snapshot") → 本窗口页面应用快照跑同一套动画；
//!   本窗口的悬停/点击 → 前端 emit("island:cmd") → 主窗口 store 执行 hold/extend/close。
//!
//! 窗口常驻可见 + 空闲鼠标穿透（不做 show/hide，避免抢走游戏/前台窗口焦点，
//! 也避免 WebView 首帧丢动画）；关闭开关时直接 destroy 释放 WebView2 内存。

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

/// 窗口标签，同时也是前端路由 `/dynamic-island`
pub const ISLAND_WINDOW_LABEL: &str = "dynamic-island";

/// 窗口标题（同时用于识别残留窗口，改动时与 `ensure_island_window` 保持一致）
const ISLAND_WINDOW_TITLE: &str = "NexBox Dynamic Island";

/// 建窗串行锁：
/// `set_dynamic_island_enabled` 是 async 命令，启动对账与设置页开关可能并发打进来。
/// 两个任务同时走到 `get_webview_window` 都拿到 None，就会各自 build 出窗口 ——
/// 多出来的那个 Tauri 只认一半，前端 tick 也不会给它做穿透，最终变成一块
/// 「点不动」的死区。这里把建窗流程串起来。
static ISLAND_CREATE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 逻辑尺寸：需容纳展开态宽度 EXPANDED_WIDTH=320（+边框）与音乐岛展开高度 150（+顶部 4 偏移）
const ISLAND_WIDTH: f64 = 348.0;
const ISLAND_HEIGHT: f64 = 176.0;

/// 与其它窗口逐字一致的 WebView2 参数。
/// 参数不一致会让 WebView2 环境冲突，直接导致新窗口创建失败（全窗口必须一致）。
/// 抗闪烁四件套（悬浮窗/透明窗必备）：
///   CalculateNativeWinOcclusion —— Chromium 原生窗口遮挡计算，对 always-on-top
///     透明小窗口有误判 bug：被误判"被遮挡"→ 渲染挂起 → 恢复 → 反复横跳，
///     表现为岛体文字/圆圈无规律闪烁（rAF 驱动的动画在停摆/恢复间抽搐）；
///   其余三个禁用后台渲染降频/timer 节流，保证失焦后心跳、轮询与动画帧率稳定。
/// 与主窗口完全一致的 WebView2 参数（逐项说明见 lib.rs `MAIN_BROWSER_ARGS`）。
const BROWSER_ARGS: &str = crate::MAIN_BROWSER_ARGS;

/// 按需创建桌面灵动岛窗口（已存在则直接复用）。
/// 创建即 visible(true)：空闲时整窗透明且鼠标穿透，视觉上等同于不存在。
fn ensure_island_window(app: &AppHandle) -> Option<tauri::WebviewWindow> {
    let _guard = ISLAND_CREATE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if let Some(win) = app.get_webview_window(ISLAND_WINDOW_LABEL) {
        return Some(win);
    }

    // 走到这里说明 Tauri 没有托管任何灵动岛窗口 —— 此时任何同名窗口都是
    // 「已解除管理但 Win32 还活着」的残留，先清掉再建，否则它会一直挡点击
    #[cfg(windows)]
    purge_stray_island_windows();

    let builder = WebviewWindowBuilder::new(
        app,
        ISLAND_WINDOW_LABEL,
        WebviewUrl::App(ISLAND_WINDOW_LABEL.into()),
    )
    .title(ISLAND_WINDOW_TITLE)
    .additional_browser_args(BROWSER_ARGS)
    .inner_size(ISLAND_WIDTH, ISLAND_HEIGHT)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .maximizable(false)
    // 注意：不要设 closable(false)/minimizable(false)。destroy() 走的是
    // WM_CLOSE → CloseRequested 流程，不可关闭标志会阻止窗口真正销毁（桌面岛关不掉）。
    // 本窗口无标题栏、无任务栏图标，用户根本碰不到关闭按钮，无需额外限制。
    .skip_taskbar(true)
    .shadow(false)
    // 不抢焦点：岛弹出时绝不能把游戏/前台窗口的焦点抢走
    .focused(false)
    .visible(true);

    match builder.build() {
        Ok(win) => {
            // 创建即穿透 + 顶部居中：页面加载完成前窗口已可见，
            // 不先置穿透会在左上角（系统默认落点）挡出一块点不到的区域
            #[cfg(windows)]
            let _ = apply_click_through(&win, true);
            place_top_center(&win);
            Some(win)
        }
        Err(e) => {
            log::error!("[Island] 创建 dynamic-island 窗口失败: {e}");
            None
        }
    }
}

/// 开启/关闭桌面灵动岛窗口（幂等，供设置「高级」开关与启动恢复调用）。
#[tauri::command]
pub async fn set_dynamic_island_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    if enabled {
        let Some(win) = ensure_island_window(&app) else {
            return Err("创建桌面灵动岛窗口失败".to_string());
        };
        // 复用的隐藏窗口可能残留「非穿透」状态（上次悬停时关闭开关）：先置穿透再 show，
        // 否则从重新出现到前端 tick 纠正之间的这段时间会挡点击
        #[cfg(windows)]
        let _ = apply_click_through(&win, true);
        // 关掉开关时窗口会自己 hide() 退场（不等销毁），重新开启必须显式 show，
        // 否则复用同一个隐藏窗口 → 桌面上永远不再出现岛
        let _ = win.show();
        // show() 会重新下发窗口样式，可能把上面那次穿透吃掉；show 之后再补一次，
        // 保证「出现即穿透」，不必等前端 60ms 的 tick 起来
        #[cfg(windows)]
        let _ = apply_click_through(&win, true);
        return Ok(());
    }

    if let Some(win) = app.get_webview_window(ISLAND_WINDOW_LABEL) {
        // 兜底穿透：hide/destroy 可能被系统、杀软或 WebView2 拖慢甚至失败，
        // 任何情况下都不能留下一个仍参与鼠标命中的隐形置顶窗口
        #[cfg(windows)]
        let _ = apply_click_through(&win, true);
        // 先 hide 再销毁：即使销毁被系统/杀软拖慢，桌面岛也会立即从屏幕上消失
        let _ = win.hide();
        // 通知主窗口退回内嵌渲染（前端也会自己处理，这里是销毁方的权威信号）
        let _ = app.emit("island:closed", ());
        if let Err(e) = win.destroy() {
            // 不向上报错：窗口已隐藏且置穿透，而报错会让前端把开关翻回“开”，
            // 与持久化的 false 分裂，用户反而无法通过开关自助恢复
            log::error!("[Island] 销毁桌面灵动岛窗口失败: {e}（已隐藏并置穿透）");
        }
        // destroy 偶尔只走到一半（WebView2 关闭被拖住/被拦截），而 Tauri 已经
        // 解除管理 —— 于是残留一个可点击、永不穿透的置顶窗口盖住一截屏幕。
        // 这里兜底再清一遍同名残留窗口。
        #[cfg(windows)]
        purge_stray_island_windows();
    }
    Ok(())
}

/// 设置鼠标穿透：空闲（光标不在岛体上）时整窗穿透，避免常驻窗口挡住顶部一条区域。
#[tauri::command]
pub fn dynamic_island_set_click_through(app: AppHandle, ignore: bool) -> Result<(), String> {
    let win = app
        .get_webview_window(ISLAND_WINDOW_LABEL)
        .ok_or_else(|| "dynamic-island window not found".to_string())?;
    #[cfg(windows)]
    {
        apply_click_through(&win, ignore)
    }
    #[cfg(not(windows))]
    {
        win.set_ignore_cursor_events(ignore)
            .map_err(|e| format!("set_ignore_cursor_events failed: {e}"))
    }
}

/// Windows 下真正生效的整窗穿透。Tauri 的 set_ignore_cursor_events 只在顶层窗口置
/// WS_EX_TRANSPARENT，WebView2 子窗口不继承该标志、仍会吞掉点击（桌面歌词同款坑，
/// 详见 utils/cursor.rs 顶部说明）。沿用歌词窗口的 EnableWindow 组合方案：
/// 顶层窗口切换 WS_EX_TRANSPARENT + 全量子窗口 EnableWindow(FALSE/TRUE)。
#[cfg(windows)]
fn apply_click_through(win: &tauri::WebviewWindow, ignore: bool) -> Result<(), String> {
    let hwnd = win
        .hwnd()
        .map_err(|e| format!("dynamic-island hwnd failed: {e}"))?;
    apply_click_through_hwnd(hwnd.0 as windows_sys::Win32::Foundation::HWND, ignore);
    Ok(())
}

/// 按 HWND 直接设置穿透（同进程跨线程安全，残留窗口清理也走这里）
///
/// 关键：**WS_EX_TRANSPARENT 必须和 WS_EX_LAYERED 一起设**才会让鼠标命中测试
/// 跳过窗口，单独设 TRANSPARENT 完全无效（实测 WindowFromPoint 仍返回该窗口）。
/// tao 的 `IGNORE_CURSOR_EVENT` 标志也是成对设置这两个位。之前只设了
/// TRANSPARENT，窗口看起来"已穿透"（样式确实写进去了），但点击照样被它吞掉 ——
/// 表现就是屏幕上方那一块点不动。
///
/// LAYERED 一旦设置就保留：反复 toggle 会让窗口在 layered / 非 layered 之间
/// 重新合成，悬停进出时闪一下。非穿透时只摘掉 TRANSPARENT 即可恢复接收点击。
#[cfg(windows)]
fn apply_click_through_hwnd(hwnd_raw: windows_sys::Win32::Foundation::HWND, ignore: bool) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumChildWindows, GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE,
        SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_EX_LAYERED,
        WS_EX_TRANSPARENT,
    };

    unsafe {
        let style = GetWindowLongPtrW(hwnd_raw, GWL_EXSTYLE);
        let style = if ignore {
            style | WS_EX_TRANSPARENT as isize | WS_EX_LAYERED as isize
        } else {
            style & !(WS_EX_TRANSPARENT as isize)
        };
        SetWindowLongPtrW(hwnd_raw, GWL_EXSTYLE, style);

        // 扩展样式改完必须刷新一次，否则命中测试可能沿用旧结果
        SetWindowPos(
            hwnd_raw,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );

        EnumChildWindows(hwnd_raw, Some(toggle_child_enabled), if ignore { 0 } else { 1 });
    }
}

/// 清理「Tauri 已不再托管、Win32 窗口却还活着」的灵动岛残留窗口。
///
/// 症状：桌面上有一块 348×176 的区域点不动（鼠标穿透失效），而且它不一定
/// 停在屏幕顶部中央 —— 位置取决于它被创建时的默认落点。这类窗口没人再给
/// 它下发穿透（前端 tick 跑在被托管的新窗口里），所以会一直吞鼠标。
///
/// 处理方式只做「置穿透 + 隐藏」，不做 DestroyWindow：
///   · DestroyWindow 只能销毁本线程创建的窗口，这里跨越消息循环线程，会失败；
///   · 隐藏 + 穿透已经足够让用户立刻恢复点击，且不会误伤正在关闭中的窗口。
#[cfg(windows)]
fn purge_stray_island_windows() {
    use windows_sys::Win32::Foundation::{HWND, LPARAM, TRUE};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, ShowWindow,
        SW_HIDE,
    };

    unsafe extern "system" fn on_window(hwnd: HWND, _lparam: LPARAM) -> i32 {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return TRUE;
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if n <= 0 {
            return TRUE;
        }
        if String::from_utf16_lossy(&buf[..n as usize]) != ISLAND_WINDOW_TITLE {
            return TRUE;
        }
        // 只动自己进程的窗口
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid != std::process::id() {
            return TRUE;
        }

        log::warn!(
            "[Island] 发现残留灵动岛窗口 hwnd={:?}，置穿透并隐藏",
            hwnd as isize
        );
        apply_click_through_hwnd(hwnd, true);
        ShowWindow(hwnd, SW_HIDE);
        TRUE
    }

    unsafe {
        EnumWindows(Some(on_window), 0);
    }
}

#[cfg(windows)]
unsafe extern "system" fn toggle_child_enabled(
    hwnd: windows_sys::Win32::Foundation::HWND,
    lparam: isize,
) -> i32 {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
    EnableWindow(hwnd, if lparam != 0 { 1 } else { 0 });
    1
}

/// 建窗即归位到主屏顶部中央（物理坐标）。不等前端 place() 才搬走，
/// 否则窗口先以系统默认落点（常在屏幕左上角）出现，加载期间挡点击。
fn place_top_center(win: &tauri::WebviewWindow) {
    // primary_monitor 在窗口刚创建、尚未关联任何显示器时会返回 Ok(None)：
    // 直接 return 会让窗口留在系统默认落点（屏幕中部），而落点一旦不是
    // 「顶部居中」，前端每 2s 的 place() 之外就没有任何校正了
    let monitor = win
        .primary_monitor()
        .ok()
        .flatten()
        .or_else(|| win.current_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let scale = monitor.scale_factor();
    let size = monitor.size();
    let pos = monitor.position();
    let w = (ISLAND_WIDTH * scale).round() as i32;
    let x = pos.x + (size.width as i32 - w).max(0) / 2;
    let _ = win.set_position(tauri::PhysicalPosition::new(x, pos.y));
}
