//! 主窗口位置持久化模块
//!
//! 复用竖排悬浮窗的持久化模式：将主窗口的物理像素坐标写入 settings.json 的
//! `main-window` 键（保留其他键），启动时恢复，关闭/退出时保存兜底。

use tauri::{AppHandle, Manager, Runtime};

/// 从持久化 settings.json 读取主窗口保存的位置（物理坐标）
pub fn read_saved_position<R: Runtime>(app: &AppHandle<R>) -> Option<(i32, i32)> {
    let dir = app.path().app_data_dir().ok()?;
    let path = dir.join("settings.json");
    let content = std::fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    let main = json.get("main-window")?;
    let x = main.get("position_x")?.as_i64()? as i32;
    let y = main.get("position_y")?.as_i64()? as i32;
    Some((x, y))
}

/// 将主窗口位置写入 settings.json 的 `main-window` 键（仅更新位置，保留其他 key）
pub fn persist_position<R: Runtime>(app: &AppHandle<R>, x: i32, y: i32) {
    let Ok(dir) = app.path().app_data_dir() else {
        return;
    };
    let path = dir.join("settings.json");
    let mut json: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = json.as_object_mut() {
        let main = obj.entry("main-window").or_insert(serde_json::json!({}));
        if let Some(m) = main.as_object_mut() {
            m.insert("position_x".to_string(), serde_json::json!(x));
            m.insert("position_y".to_string(), serde_json::json!(y));
        }
    }
    if let Ok(content) = serde_json::to_string_pretty(&json) {
        let _ = std::fs::write(&path, content);
        log::info!("[main-window] save position x={x} y={y}");
    }
}

/// 判断指定物理坐标是否落在任一显示器窗口区域内（含 10px 容差，复用 tray 归位判据）
pub fn is_on_any_monitor<R: Runtime>(app: &AppHandle<R>, pos: (i32, i32)) -> bool {
    let Some(monitors) = app.available_monitors().ok() else {
        return false;
    };
    monitors.iter().any(|m| {
        let r = m.position();
        let s = m.size();
        let x = pos.0;
        let y = pos.1;
        x + 10 >= r.x && x - 10 <= r.x + s.width as i32
            && y + 10 >= r.y && y - 10 <= r.y + s.height as i32
    })
}

/// 恢复主窗口位置：有保存记录且仍在屏幕内 → 恢复到保存位置；
/// 否则（无记录或离屏）→ 显式居中到主显示器。
pub fn restore_position<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if let Some(pos) = read_saved_position(app) {
        if is_on_any_monitor(app, pos) {
            let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
                x: pos.0,
                y: pos.1,
            }));
            log::info!("[main-window] restore saved position {:?}", pos);
            return;
        }
    }
    // 无记录或保存位置已离屏 → 居中到主显示器
    if let Some(monitor) = app.primary_monitor().ok().flatten() {
        let p = monitor.position();
        let s = monitor.size();
        if let Ok(ws) = window.outer_size() {
            let x = p.x + (s.width as i32 - ws.width as i32) / 2;
            let y = p.y + (s.height as i32 - ws.height as i32) / 2;
            let _ = window.set_position(tauri::Position::Physical(
                tauri::PhysicalPosition { x, y },
            ));
        }
    }
}

/// 保存主窗口当前位置：读取实际 outer_position，跳过最大化/最小化等异常状态。
pub fn save_current_position<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    // 最大化/最小化时 outer_position 不可靠，跳过保存避免恢复到异常状态
    if window.is_maximized().unwrap_or(false) || window.is_minimized().unwrap_or(false) {
        log::info!("[main-window] skip save position (window maximized/minimized)");
        return;
    }
    if let Ok(pos) = window.outer_position() {
        persist_position(app, pos.x, pos.y);
    }
}

// ==================== 节约内存（实验性）：缩小到托盘销毁主窗口 ====================
//
// 启用后，缩小到托盘（minimize_to_tray 命令 / Alt+F4 的 CloseRequested）不再仅 hide，
// 而是销毁主窗口（释放其 WebView2 进程树，约 100~200MB 提交内存，参照托盘菜单原生化的先例）；
// 从托盘恢复时（show_main_window 统一入口）重建窗口。重建为冷加载（1~2s），前端回到默认启动页。
//
// 守卫：音乐播放中（smtc 状态 10s 内 playing）回退为仅隐藏——销毁会杀死前端 audio 元素，后台听歌必断。
//
// 重建后必须补挂的东西（详见 rebuild_main_window / finish_rebuild_show）：
//   窗口事件闭包、媒体键 WM_APPCOMMAND 子类化、SMTC 会话重绑、桌面歌词数据回填；
//   灵动岛由 island:ready/心跳机制自动恢复，无需额外处理。

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// 节约内存（实验性）开关
static MEMORY_SAVING_MODE: AtomicBool = AtomicBool::new(false);
/// 主窗口正在执行销毁流程：destroy() 在 Windows 走 WM_CLOSE → CloseRequested 流程，
/// 事件闭包必须放行（勿 prevent_close），否则销毁被卡住（同 island_window 的坑）
static MAIN_BEING_DESTROYED: AtomicBool = AtomicBool::new(false);
/// 主窗口已重建、等待前端挂载后调用 main_window_ready 完成显示握手
static REBUILD_PENDING: AtomicBool = AtomicBool::new(false);

pub(crate) fn memory_saving_enabled() -> bool {
    MEMORY_SAVING_MODE.load(Ordering::SeqCst)
}

pub(crate) fn is_main_being_destroyed() -> bool {
    MAIN_BEING_DESTROYED.load(Ordering::SeqCst)
}

/// 启动时从 settings.json 恢复开关（键 `memory-saving-mode`，默认关闭）
pub fn load_memory_saving_mode(app: &tauri::AppHandle) {
    let enabled = crate::hotkey::read_settings_value(app, "memory-saving-mode")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    MEMORY_SAVING_MODE.store(enabled, Ordering::SeqCst);
    if enabled {
        log::info!("[main-window] 节约内存模式已启用（实验性）：缩小到托盘将销毁主窗口");
    }
}

#[tauri::command]
pub fn set_memory_saving_mode(app: tauri::AppHandle, enabled: bool) {
    MEMORY_SAVING_MODE.store(enabled, Ordering::SeqCst);
    crate::hotkey::save_settings_value(&app, "memory-saving-mode", serde_json::Value::Bool(enabled));
    log::info!("[main-window] 节约内存模式 = {enabled}");
}

#[tauri::command]
pub fn get_memory_saving_mode() -> bool {
    memory_saving_enabled()
}

/// 销毁主窗口以释放内存（节约内存模式）。
/// 返回 true = 已销毁；false = 被守卫拦下（音乐播放中）或窗口不存在，调用方应回退为仅隐藏。
pub fn destroy_main_window<R: Runtime>(app: &AppHandle<R>) -> bool {
    let Some(win) = app.get_webview_window("main") else {
        return false;
    };
    // 守卫：音乐播放中不销毁（销毁会杀死前端 audio 元素，后台听歌必断）
    if let Some((state, age)) = crate::smtc::current_with_age() {
        if state.playing && age < 10_000 {
            log::info!("[main-window] 音乐播放中，缩小到托盘回退为仅隐藏");
            return false;
        }
    }
    save_current_position(app);
    // 媒体键子类化绑定主窗口 HWND，句柄死后静态量不清会拒绝新窗口挂载，销毁前必须恢复
    crate::media_keys::restore_main_subclass();
    // SMTC 会话同样绑定主窗口 HWND，清空状态避免系统浮层残留旧歌曲
    crate::smtc::smtc_clear();

    MAIN_BEING_DESTROYED.store(true, Ordering::SeqCst);
    crate::emit_main_visibility(app, false);
    let destroyed = win.destroy().is_ok();
    // 销毁失败时复位标志（CloseRequested 闭包需按普通关闭处理）；成功则保持 true，
    // 由重建路径（rebuild_main_window）复位——destroy 触发的 CloseRequested 可能异步到达
    if !destroyed {
        MAIN_BEING_DESTROYED.store(false, Ordering::SeqCst);
    }
    log::info!("[main-window] 主窗口已销毁以释放内存 (ok={destroyed})，托盘可恢复");
    destroyed
}

/// 重建主窗口（节约内存模式下从托盘恢复时由 show_main_window 调用，幂等）。
/// 窗口以不可见方式创建（防白屏，同竖排悬浮窗模式），前端挂载后调用 `main_window_ready`
/// 完成显示握手；10s 看门狗兜底强制显示，防 ready 信号丢失导致窗口永远不可见。
pub fn rebuild_main_window<R: Runtime>(app: &AppHandle<R>) {
    if app.get_webview_window("main").is_some() {
        return;
    }
    use tauri::{WebviewUrl, WebviewWindowBuilder};
    // 上一轮销毁流程结束，CloseRequested 闭包恢复普通语义
    MAIN_BEING_DESTROYED.store(false, Ordering::SeqCst);

    // 参数与 tauri.conf.json 主窗口配置逐字对齐；additional_browser_args 必须与
    // 其它窗口一致，否则 WebView2 环境冲突导致创建失败（island_window.rs 同款警告）
    let builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("新境盒")
        .additional_browser_args(crate::MAIN_BROWSER_ARGS)
        .inner_size(1230.0, 771.0)
        .min_inner_size(1140.0, 715.0)
        .resizable(true)
        .decorations(false)
        .devtools(true)
        .visible(false);

    match builder.build() {
        Ok(win) => {
            // 事件闭包（Close→托盘 / Resized 可见性）不会自动重挂，必须显式补上
            crate::attach_main_window_handlers(&win);
            let _ = win.set_skip_taskbar(false);
            restore_position(app);
            // 媒体键子类化按开关状态重装到新 HWND
            crate::media_keys::rebind_main_subclass_if_enabled(app);
            REBUILD_PENDING.store(true, Ordering::SeqCst);
            log::info!("[main-window] 主窗口已重建（不可见），等待前端就绪后显示");

            let app2 = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(10));
                if REBUILD_PENDING.swap(false, Ordering::SeqCst) {
                    log::warn!("[main-window] 前端 10s 未就绪，看门狗强制显示主窗口");
                    finish_rebuild_show(&app2);
                }
            });
        }
        Err(e) => log::error!("[main-window] 重建主窗口失败: {e}"),
    }
}

/// 重建窗口的显示握手收尾：显示 + 聚焦 + 可见性推送 + SMTC 重绑 + 桌面歌词数据回填。
/// 由 `main_window_ready` 命令（前端挂载后调用）与看门狗共用。
fn finish_rebuild_show<R: Runtime>(app: &AppHandle<R>) {
    use tauri::Emitter;
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.set_skip_taskbar(false);
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
    crate::emit_main_visibility(app, true);
    // SMTC 会话重绑到新 HWND 并重置元数据缓存，前端下一次每秒推送自动重新填充
    crate::smtc::rebind_after_main_window_recreate(app.clone());
    // 桌面歌词恢复：歌词窗口的 request-data 仅挂载时发一次，主窗口重建后由后端补发，
    // 主窗口 music-store 的现有监听器会自动回填歌词数据
    let _ = app.emit("desktop-lyrics:request-data", ());
}

/// 前端就绪握手：仅重建场景生效（正常启动 REBUILD_PENDING 为假，直接忽略）。
/// 前端 App 挂载时无条件调用，安全无副作用。
#[tauri::command]
pub fn main_window_ready(app: tauri::AppHandle) {
    if !REBUILD_PENDING.swap(false, Ordering::SeqCst) {
        return;
    }
    log::info!("[main-window] 前端已就绪，显示重建后的主窗口");
    finish_rebuild_show(&app);
}