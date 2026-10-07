use serde_json::json;
use tauri::Manager;
use tauri_plugin_os::locale;

async fn send_statistics(version: String, os: String) {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    match client
        .post("https://mc.sjtu.cn/api-sjmcl/statistics")
        .json(&json!({
            "version": format!("box-{}", version),
            "os": os,
        }))
        .send()
        .await
    {
        Ok(resp) => log::info!("Statistics sent, status: {}", resp.status()),
        Err(e) => log::error!("Failed to send statistics: {}", e),
    }
}

/// 统计标志的独立文件路径（app_data_dir/statistics-sent.json）。
/// 注意：刻意不写入 settings.json —— 插件 store 对同一文件会创建多个独立实例，
/// 任何实例的 save() 都会用自身不完整的缓存全量覆盖磁盘，导致前端数据丢失。
fn statistics_flag_path(app_handle: &tauri::AppHandle) -> std::path::PathBuf {
    app_handle
        .path()
        .app_data_dir()
        .unwrap_or_default()
        .join("statistics-sent.json")
}

pub fn check_and_send_statistics(app: &tauri::App) {
    let flag_path = statistics_flag_path(app.handle());
    if flag_path.exists() {
        log::info!("Statistics already sent, skipping");
        return;
    }

    let version = env!("CARGO_PKG_VERSION").to_string();
    let os = std::env::consts::OS.to_string();
    log::info!("Sending statistics: version={}, os={}", version, os);

    let app_handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        send_statistics(version, os).await;

        let flag_path = statistics_flag_path(&app_handle);
        if let Some(parent) = flag_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::write(&flag_path, "1").is_ok() {
            log::info!("Statistics flag saved");
        } else {
            log::error!("Failed to write statistics flag");
        }
    });
}

pub fn get_mapped_locale() -> String {
    let locale = locale().unwrap_or_else(|| "en".to_string());
    let matched_locale;

    #[cfg(target_os = "macos")]
    {
        let language_map = [
            ("de", vec!["de"]),
            ("fr", vec!["fr"]),
            ("ja", vec!["ja"]),
            ("zh-Hans", vec!["zh-Hans", "wuu-Hans", "yue-Hans"]),
            ("zh-Hant", vec!["zh-Hant", "yue-Hant"]),
        ];

        matched_locale = language_map
            .iter()
            .find(|(_, locales)| locales.iter().any(|l| locale.starts_with(l)))
            .map(|(mapped, _)| mapped.to_string());
    }

    #[cfg(not(target_os = "macos"))]
    {
        let language_map = [
            ("de", vec!["de"]),
            ("fr", vec!["fr"]),
            ("ja", vec!["ja"]),
            ("zh-Hans", vec!["zh-CN", "zh-SG"]),
            ("zh-Hant", vec!["zh-TW", "zh-HK", "zh-MO"]),
        ];

        matched_locale = language_map
            .iter()
            .find(|(_, locales)| locales.iter().any(|l| locale.starts_with(l)))
            .map(|(mapped, _)| mapped.to_string());
    }

    matched_locale.unwrap_or_else(|| "en".to_string())
}

#[tauri::command]
pub fn get_system_locale() -> String {
    get_mapped_locale()
}

#[tauri::command]
pub fn get_system_username() -> String {
    // 用 var_os + to_string_lossy：Windows 环境变量为 UTF-16，
    // 中文用户名经 Rust 转 UTF-8 不会乱码；非 UTF-8 时也安全降级
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("USERNAME")
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    }

    #[cfg(not(target_os = "windows"))]
    {
        std::env::var_os("USER")
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    }
}
