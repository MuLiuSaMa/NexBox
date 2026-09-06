//! 竖排悬浮框模块 — 基于 Tauri Webview 窗口
//!
//! 与 Win32 GDI+ overlay 并存，当 style == "vertical_panel" 时使用此模块。

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use tauri::{Emitter, Manager};

use crate::overlay_panel::{OverlayResult, OverlaySettings, collect_hardware_data, CURRENT_HARDWARE_DATA, CURRENT_SETTINGS, get_or_init_settings};

static VERTICAL_OVERLAY_ACTIVE: AtomicBool = AtomicBool::new(false);
static DATA_THREAD_STARTED: AtomicBool = AtomicBool::new(false);

/// 启动竖排悬浮框
#[tauri::command]
pub async fn start_vertical_overlay(
    app_handle: tauri::AppHandle,
    settings: Option<OverlaySettings>,
) -> Result<OverlayResult, String> {
    if VERTICAL_OVERLAY_ACTIVE.load(Ordering::SeqCst) {
        return Ok(OverlayResult {
            success: true,
            message: "竖排悬浮框已处于启用状态".to_string(),
        });
    }

    // 如果 Win32 overlay 正在运行，先停止
    if crate::overlay_panel::is_overlay_active() {
        crate::overlay_panel::stop_overlay()?;
        std::thread::sleep(Duration::from_millis(200));
    }

    // 保存设置；保留 CURRENT_SETTINGS 中已保存的竖排位置（前端传入的设置可能来自旧缓存，不含最新位置）
    if let Some(s) = settings {
        let mut settings_lock = CURRENT_SETTINGS.lock().unwrap();
        let preserved = settings_lock
            .as_ref()
            .map(|cur| (cur.vertical_position_x, cur.vertical_position_y));
        let mut merged = s;
        if let Some((vx, vy)) = preserved {
            if merged.vertical_position_x.is_none() {
                merged.vertical_position_x = vx;
            }
            if merged.vertical_position_y.is_none() {
                merged.vertical_position_y = vy;
            }
        }
        *settings_lock = Some(merged);
    }

    let settings = get_or_init_settings();
    VERTICAL_OVERLAY_ACTIVE.store(true, Ordering::SeqCst);

    // 按需创建竖排悬浮框窗口（不常驻，启用时创建、关闭时销毁）
    let Some(window) = crate::ensure_vertical_overlay(&app_handle) else {
        return Err("创建 vertical-overlay 窗口失败".to_string());
    };

    // 先设置初始窗口大小，再显示，避免窗口以默认大尺寸闪一下
    let init_width = settings.item_width as f64;
    let init_height = 60.0_f64;
    let _ = window.set_size(tauri::LogicalSize { width: init_width, height: init_height });

    // 恢复保存的位置或使用默认位置（屏幕右上角）
    // 竖排悬浮框位置独立于 Win32 悬浮框，优先从持久化文件读取，避免前端传入的设置覆盖已保存的位置
    let restored_pos = read_persisted_vertical_overlay_position(&app_handle)
        .or_else(|| settings.vertical_position_x.zip(settings.vertical_position_y));
    log::info!("[vertical-overlay] restore position: {:?}", restored_pos);
    if let Some((x, y)) = restored_pos {
        let _ = window.set_position(tauri::PhysicalPosition { x, y });
    } else {
        // 默认：屏幕右上角
        if let Ok(monitor) = window.current_monitor() {
            if let Some(monitor) = monitor {
                let screen_size = monitor.size();
                let scale = monitor.scale_factor();
                let win_w = init_width * scale;
                let x = (screen_size.width as f64 - win_w - 20.0 * scale) as i32;
                let y = (20.0 * scale) as i32;
                let _ = window.set_position(tauri::PhysicalPosition { x, y });
            }
        }
    }

    let _ = window.set_always_on_top(true);

    // 启动 FPS / 延迟 / 网络时间服务（与 Win32 start_overlay 对齐；均有幂等保护）。
    // 此前竖排面板只启动了数据推送线程，collect_hardware_data 读到的
    // fps / game_ping / net_time_offset_ms 永远为 None。
    crate::game_ping::start_ping_thread();
    crate::game_fps::start_fps_monitor();
    crate::overlay_panel::start_net_time_sync();

    // 排除竖排窗口自身成为前台目标（用户关闭鼠标穿透点击悬浮框时，FPS 目标不应切走）
    if let Ok(hwnd) = window.hwnd() {
        crate::game_fps::set_overlay_hwnd(hwnd.0 as u64);
    }

    // 窗口已创建但 visible=false，等前端 mount 后调用 vertical_overlay_ready 命令再 show，避免加载时白屏闪烁

    // 启动数据推送线程（如果尚未启动）
    if !DATA_THREAD_STARTED.swap(true, Ordering::SeqCst) {
        let handle_clone = app_handle.clone();
        thread::spawn(move || {
            while DATA_THREAD_STARTED.load(Ordering::SeqCst) {
                let data = collect_hardware_data();
                // 更新 CURRENT_HARDWARE_DATA 供硬件报告使用
                *CURRENT_HARDWARE_DATA.lock().unwrap() = Some(data.clone());

                // 推送给竖排悬浮框窗口
                if VERTICAL_OVERLAY_ACTIVE.load(Ordering::SeqCst) {
                    let _ = handle_clone.emit("vertical-overlay-data", &data);
                }

                thread::sleep(Duration::from_millis(1000));
            }
        });
    }

    // 推送当前设置到前端
    let _ = app_handle.emit("vertical-overlay-settings", &settings);

    let _ = app_handle.emit("overlay-status-changed", ());
    Ok(OverlayResult {
        success: true,
        message: "竖排悬浮框已启动".to_string(),
    })
}

/// 前端页面渲染完成后调用，此时才 show 窗口（避免 WebView2 初次加载时闪烁空白页）
#[tauri::command]
pub fn vertical_overlay_ready(app_handle: tauri::AppHandle) -> Result<(), String> {
    if let Some(win) = app_handle.get_webview_window("vertical-overlay") {
        let _ = win.show();
        let _ = app_handle.emit("overlay-status-changed", ());
    }
    Ok(())
}

/// 停止竖排悬浮框
#[tauri::command]
pub async fn stop_vertical_overlay(
    app_handle: tauri::AppHandle,
) -> Result<OverlayResult, String> {
    if !VERTICAL_OVERLAY_ACTIVE.load(Ordering::SeqCst) {
        return Ok(OverlayResult {
            success: true,
            message: "竖排悬浮框已处于关闭状态".to_string(),
        });
    }

    // 关闭前以窗口实际位置为准兜底保存：拖动结束到立即关闭之间可能不足 300ms
    // （前端 onMoved 节流/鼠标事件在系统拖动中被吞掉），最后一次位置来不及保存，
    // 这里直接读取窗口当前位置保存，保证"拖完立刻关闭"不丢失。
    if let Some(window) = app_handle.get_webview_window("vertical-overlay") {
        if let Ok(pos) = window.outer_position() {
            let x = pos.x;
            let y = pos.y;
            {
                let mut settings_lock = CURRENT_SETTINGS.lock().unwrap();
                if let Some(ref mut settings) = *settings_lock {
                    settings.vertical_position_x = Some(x);
                    settings.vertical_position_y = Some(y);
                }
            }
            persist_vertical_overlay_position(&app_handle, x, y);
            log::info!("[vertical-overlay] save position on stop x={x} y={y}");
            let _ = app_handle.emit("overlay-position-saved", serde_json::json!({ "x": x, "y": y }));
        }
    }

    VERTICAL_OVERLAY_ACTIVE.store(false, Ordering::SeqCst);

    if let Some(window) = app_handle.get_webview_window("vertical-overlay") {
        let _ = window.destroy();
    }

    // 与 Win32 stop_overlay 对称：停止 FPS 监控并清除自身窗口排除
    crate::game_fps::clear_overlay_hwnd();
    crate::game_fps::stop_fps_monitor();

    let _ = app_handle.emit("overlay-status-changed", ());
    Ok(OverlayResult {
        success: true,
        message: "竖排悬浮框已关闭".to_string(),
    })
}

/// 保存竖排悬浮框位置（更新内存并持久化到 settings.json 的 overlay-settings 键，与 Win32 悬浮框位置独立）
#[tauri::command]
pub async fn save_vertical_overlay_position(
    app_handle: tauri::AppHandle,
    x: i32,
    y: i32,
) -> Result<OverlayResult, String> {
    {
        let mut settings_lock = CURRENT_SETTINGS.lock().unwrap();
        if let Some(ref mut settings) = *settings_lock {
            settings.vertical_position_x = Some(x);
            settings.vertical_position_y = Some(y);
        }
    }
    log::info!("[vertical-overlay] save position x={x} y={y}");
    persist_vertical_overlay_position(&app_handle, x, y);
    // 通知主应用同步位置状态，避免其持有旧缓存后续保存时覆盖
    let _ = app_handle.emit("overlay-position-saved", serde_json::json!({ "x": x, "y": y }));
    Ok(OverlayResult {
        success: true,
        message: "位置已保存".to_string(),
    })
}

/// 将竖排悬浮框位置写入 settings.json 的 overlay-settings 键（仅更新 vertical_position，保留其他 key，兼容前端 LazyStore）
fn persist_vertical_overlay_position(app_handle: &tauri::AppHandle, x: i32, y: i32) {
    let Ok(dir) = app_handle.path().app_data_dir() else {
        return;
    };
    let path = dir.join("settings.json");
    let mut json: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = json.as_object_mut() {
        let overlay = obj.entry("overlay-settings").or_insert(serde_json::json!({}));
        if let Some(o) = overlay.as_object_mut() {
            o.insert("vertical_position_x".to_string(), serde_json::json!(x));
            o.insert("vertical_position_y".to_string(), serde_json::json!(y));
        }
    }
    if let Ok(content) = serde_json::to_string_pretty(&json) {
        let _ = std::fs::write(&path, content);
    }
}

/// 从持久化 settings.json 读取竖排悬浮框保存的位置（物理坐标），作为恢复位置的权威来源
fn read_persisted_vertical_overlay_position(app_handle: &tauri::AppHandle) -> Option<(i32, i32)> {
    let dir = app_handle.path().app_data_dir().ok()?;
    let path = dir.join("settings.json");
    let content = std::fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    let overlay = json.get("overlay-settings")?;
    let x = overlay.get("vertical_position_x")?.as_i64()? as i32;
    let y = overlay.get("vertical_position_y")?.as_i64()? as i32;
    Some((x, y))
}

/// 从持久化 settings.json 清除竖排悬浮框保存的位置
fn clear_persisted_vertical_overlay_position(app_handle: &tauri::AppHandle) {
    let Ok(dir) = app_handle.path().app_data_dir() else {
        return;
    };
    let path = dir.join("settings.json");
    let mut json: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = json.as_object_mut() {
        if let Some(overlay) = obj.get_mut("overlay-settings").and_then(|o| o.as_object_mut()) {
            overlay.remove("vertical_position_x");
            overlay.remove("vertical_position_y");
        }
    }
    if let Ok(content) = serde_json::to_string_pretty(&json) {
        let _ = std::fs::write(&path, content);
    }
}

/// 设置鼠标穿透
#[tauri::command]
pub async fn set_vertical_overlay_click_through(
    app_handle: tauri::AppHandle,
    enabled: bool,
) -> Result<OverlayResult, String> {
    if let Some(window) = app_handle.get_webview_window("vertical-overlay") {
        let _ = window.set_ignore_cursor_events(enabled);
    }
    Ok(OverlayResult {
        success: true,
        message: if enabled {
            "已开启鼠标穿透".to_string()
        } else {
            "已关闭鼠标穿透".to_string()
        },
    })
}

/// 重置竖排悬浮框位置
#[tauri::command]
pub async fn reset_vertical_overlay_position(
    app_handle: tauri::AppHandle,
) -> Result<OverlayResult, String> {
    // 清除保存的竖排位置
    {
        let mut settings_lock = CURRENT_SETTINGS.lock().unwrap();
        if let Some(ref mut settings) = *settings_lock {
            settings.vertical_position_x = None;
            settings.vertical_position_y = None;
        }
    }
    // 同步清除持久化文件中的竖排位置
    clear_persisted_vertical_overlay_position(&app_handle);

    // 通知主应用清除本地缓存的竖排位置，避免后续保存其他设置时把旧位置写回
    let _ = app_handle.emit("vertical-overlay-position-reset", ());

    // 移动窗口到默认位置（右上角）
    if let Some(window) = app_handle.get_webview_window("vertical-overlay") {
        if let Ok(monitor) = window.current_monitor() {
            if let Some(monitor) = monitor {
                let screen_size = monitor.size();
                let scale = monitor.scale_factor();
                let win_w = 220.0 * scale;
                let x = (screen_size.width as f64 - win_w - 20.0 * scale) as i32;
                let y = (20.0 * scale) as i32;
                let _ = window.set_position(tauri::PhysicalPosition { x, y });
            }
        }
    }

    Ok(OverlayResult {
        success: true,
        message: "位置已重置为默认".to_string(),
    })
}

/// 调整竖排悬浮框窗口大小
#[tauri::command]
pub async fn resize_vertical_overlay(
    app_handle: tauri::AppHandle,
    height: u32,
) -> Result<(), String> {
    if let Some(window) = app_handle.get_webview_window("vertical-overlay") {
        let settings = crate::overlay_panel::get_or_init_settings();
        let logical_width = settings.item_width as f64;
        let _ = window.set_size(tauri::LogicalSize {
            width: logical_width,
            height: height as f64,
        });
    }
    Ok(())
}

/// 切换竖排悬浮框开关（供快捷键调用）
pub fn toggle_vertical_overlay(app_handle: &tauri::AppHandle) -> Result<OverlayResult, String> {
    if VERTICAL_OVERLAY_ACTIVE.load(Ordering::SeqCst) {
        // 使用 blocking 方式调用 async 命令
        let handle = app_handle.clone();
        tauri::async_runtime::block_on(async move {
            stop_vertical_overlay(handle).await
        })
    } else {
        let handle = app_handle.clone();
        tauri::async_runtime::block_on(async move {
            start_vertical_overlay(handle, None).await
        })
    }
}

/// 查询竖排悬浮框是否活跃
pub fn is_vertical_overlay_active() -> bool {
    VERTICAL_OVERLAY_ACTIVE.load(Ordering::SeqCst)
}

/// 停止数据推送线程
pub fn stop_data_thread() {
    DATA_THREAD_STARTED.store(false, Ordering::SeqCst);
}

/// 清理（应用退出时调用）
pub fn cleanup(app_handle: &tauri::AppHandle) {
    if VERTICAL_OVERLAY_ACTIVE.load(Ordering::SeqCst) {
        // 退出前同样以窗口实际位置兜底保存
        if let Some(window) = app_handle.get_webview_window("vertical-overlay") {
            if let Ok(pos) = window.outer_position() {
                let x = pos.x;
                let y = pos.y;
                {
                    let mut settings_lock = CURRENT_SETTINGS.lock().unwrap();
                    if let Some(ref mut settings) = *settings_lock {
                        settings.vertical_position_x = Some(x);
                        settings.vertical_position_y = Some(y);
                    }
                }
                persist_vertical_overlay_position(app_handle, x, y);
                log::info!("[vertical-overlay] save position on cleanup x={x} y={y}");
            }
        }
        VERTICAL_OVERLAY_ACTIVE.store(false, Ordering::SeqCst);
        if let Some(window) = app_handle.get_webview_window("vertical-overlay") {
            let _ = window.destroy();
        }
    }
    stop_data_thread();
}
