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

/// 逻辑尺寸：需容纳展开态宽度 EXPANDED_WIDTH=320（+边框）与音乐岛展开高度 150（+顶部 4 偏移）
const ISLAND_WIDTH: f64 = 348.0;
const ISLAND_HEIGHT: f64 = 176.0;

/// 与其它窗口逐字一致的 WebView2 参数。
/// 参数不一致会让 WebView2 环境冲突，直接导致新窗口创建失败（全窗口必须一致）。
const BROWSER_ARGS: &str = "--disable-features=MediaSessionService,HardwareMediaKeyHandling,msWebOOUI,msPdfOOUI,msSmartScreenProtection,msEdgeAutofill,msEdgeShopping,msEdgeWallet --autoplay-policy=no-user-gesture-required --disable-background-networking --disable-client-side-phishing-detection --disable-component-update --disable-default-apps --disable-extensions --disable-sync";

/// 按需创建桌面灵动岛窗口（已存在则直接复用）。
/// 创建即 visible(true)：空闲时整窗透明且鼠标穿透，视觉上等同于不存在。
fn ensure_island_window(app: &AppHandle) -> Option<tauri::WebviewWindow> {
    if let Some(win) = app.get_webview_window(ISLAND_WINDOW_LABEL) {
        return Some(win);
    }

    let builder = WebviewWindowBuilder::new(
        app,
        ISLAND_WINDOW_LABEL,
        WebviewUrl::App(ISLAND_WINDOW_LABEL.into()),
    )
    .title("NexBox Dynamic Island")
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
        Ok(win) => Some(win),
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
        // 关掉开关时窗口会自己 hide() 退场（不等销毁），重新开启必须显式 show，
        // 否则复用同一个隐藏窗口 → 桌面上永远不再出现岛
        let _ = win.show();
        return Ok(());
    }

    if let Some(win) = app.get_webview_window(ISLAND_WINDOW_LABEL) {
        // 先 hide 再销毁：即使销毁被系统/杀软拖慢，桌面岛也会立即从屏幕上消失
        let _ = win.hide();
        // 通知主窗口退回内嵌渲染（前端也会自己处理，这里是销毁方的权威信号）
        let _ = app.emit("island:closed", ());
        win.destroy().map_err(|e| format!("销毁桌面灵动岛窗口失败: {e}"))?;
    }
    Ok(())
}

/// 设置鼠标穿透：空闲（光标不在岛体上）时整窗穿透，避免常驻窗口挡住顶部一条区域。
#[tauri::command]
pub fn dynamic_island_set_click_through(app: AppHandle, ignore: bool) -> Result<(), String> {
    let win = app
        .get_webview_window(ISLAND_WINDOW_LABEL)
        .ok_or_else(|| "dynamic-island window not found".to_string())?;
    win.set_ignore_cursor_events(ignore)
        .map_err(|e| format!("set_ignore_cursor_events failed: {e}"))?;
    Ok(())
}
