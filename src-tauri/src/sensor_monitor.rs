//! 传感器监控窗口 — 独立 Tauri 窗口，展示 LHML 所有传感器数据

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::sensor::{read_lhm_sensors, SensorsResponse};

/// 按需创建传感器监控窗口（不常驻，打开时创建、关闭时销毁）。
/// 如果窗口已存在，直接返回。
pub fn ensure_sensor_monitor_window(
    app: &AppHandle,
) -> Option<tauri::WebviewWindow> {
    let label = "sensor-monitor";
    if let Some(win) = app.get_webview_window(label) {
        return Some(win);
    }

    let builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App("sensor-monitor".into()))
        .title("NexBox 传感器监控")
        // 与其它窗口保持一致的 WebView2 参数（禁用 Chromium 自动媒体会话，避免与 smtc.rs 会话重复）
        .additional_browser_args("--disable-features=MediaSessionService,HardwareMediaKeyHandling --autoplay-policy=no-user-gesture-required")
        .inner_size(1000.0, 700.0)
        .min_inner_size(600.0, 400.0)
        .resizable(true)
        .decorations(true)
        .visible(false) // 先隐藏，等前端就绪再 show
        .center(); // 屏幕居中

    // 应用层窗口
    #[cfg(target_os = "windows")]
    let builder = builder;
    #[cfg(target_os = "macos")]
    let builder = builder;

    match builder.build() {
        Ok(win) => Some(win),
        Err(e) => {
            log::error!("[SensorMonitor] 创建 sensor-monitor 窗口失败: {e}");
            None
        }
    }
}

/// 打开（或聚焦）传感器监控窗口
#[tauri::command]
pub async fn open_sensor_monitor(app_handle: AppHandle) -> Result<(), String> {
    let window = match ensure_sensor_monitor_window(&app_handle) {
        Some(win) => win,
        None => return Err("创建传感器监控窗口失败".to_string()),
    };

    // 如果窗口已存在且可见，直接聚焦
    if window.is_visible().unwrap_or(false) {
        let _ = window.set_focus();
        return Ok(());
    }

    // 首次显示
    let _ = window.show();
    let _ = window.set_focus();
    Ok(())
}

/// 获取所有 LHML 传感器数据（实时）
/// 使用 spawn_blocking 避免阻塞 Tauri 异步运行时
#[tauri::command]
pub async fn get_all_sensors() -> Result<SensorsResponse, String> {
    match tauri::async_runtime::spawn_blocking(|| read_lhm_sensors()).await {
        Ok(result) => result,
        Err(e) => Err(format!("传感器查询任务失败: {}", e)),
    }
}