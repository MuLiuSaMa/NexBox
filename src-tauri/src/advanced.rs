//! 高级设置：缓存/数据目录大小查询与清除
//!
//! - 缓存目录：`%LOCALAPPDATA%\NexBox\`（图标缓存、公告缓存、RuntimeRepair 下载缓存等），可自动重建
//!   - 注意：排除 WebView2 运行时数据目录 `EBWebView`（由 WebView2 自动生成，不属于应用缓存）
//! - 数据目录：应用数据目录（`app_data_dir`，含 settings.json 与所有 store 配置），清除即重置全部设置
//!   - 同时尽力清除 WebView2 运行时数据（`EBWebView`），该目录运行中可能被占用，需重启应用完全生效

use std::fs;
use std::path::Path;
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use tauri::Manager;

/// 缓存根目录：`%LOCALAPPDATA%\NexBox`
fn cache_root_dir() -> Option<std::path::PathBuf> {
    dirs::data_local_dir().map(|p| p.join("NexBox"))
}

/// 是否为 WebView2 运行时数据目录（EBWebView）
fn is_webview_dir(path: &Path) -> bool {
    path.file_name()
        .map(|n| n.to_string_lossy().eq_ignore_ascii_case("EBWebView"))
        .unwrap_or(false)
}

/// 递归累加目录大小（字节）
fn get_dir_size(path: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += get_dir_size(&p);
            } else if let Ok(metadata) = fs::metadata(&p) {
                total += metadata.len();
            }
        }
    }
    total
}

/// 缓存根目录下除 WebView2 数据（EBWebView）外的实际大小
fn get_cache_size() -> u64 {
    let Some(root) = cache_root_dir() else {
        return 0;
    };
    if !root.exists() {
        return 0;
    }
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(&root) {
        for entry in entries.flatten() {
            let p = entry.path();
            if is_webview_dir(&p) {
                continue;
            }
            if p.is_dir() {
                total += get_dir_size(&p);
            } else if let Ok(metadata) = fs::metadata(&p) {
                total += metadata.len();
            }
        }
    }
    total
}

/// Local 数据目录（`%LOCALAPPDATA%\NexBox`，含 WebView2 数据 EBWebView）的总大小
fn get_local_size() -> u64 {
    cache_root_dir()
        .map(|p| if p.exists() { get_dir_size(&p) } else { 0 })
        .unwrap_or(0)
}

/// 删除目录下的所有子项（保留目录本身，返回实际释放字节数）。
/// 某子项因被占用删除失败时跳过，不中断其余清理。
fn clear_dir_contents(path: &Path) -> Result<u64, String> {
    if !path.exists() {
        return Ok(0);
    }
    let read_dir = fs::read_dir(path).map_err(|e| format!("无法读取目录 {}: {}", path.display(), e))?;
    let mut freed: u64 = 0;
    for entry in read_dir.flatten() {
        let p = entry.path();
        let size = if p.is_dir() { get_dir_size(&p) } else { 0 };
        let result = if p.is_dir() {
            fs::remove_dir_all(&p)
        } else {
            fs::remove_file(&p)
        };
        match result {
            Ok(_) => freed += size,
            Err(e) => log::warn!("跳过无法删除 {}: {}", p.display(), e),
        }
    }
    Ok(freed)
}

#[derive(serde::Serialize)]
pub struct StorageSizes {
    pub cache_size: u64,
    pub data_size: u64,
}

/// 查询缓存与数据目录大小（字节）。
/// 缓存（可单独清除，不含被占用的 EBWebView）；数据（清除时为彻底重置，含 Local 与 Roaming 全部）。
#[tauri::command]
pub async fn get_storage_sizes(app: tauri::AppHandle) -> Result<StorageSizes, String> {
    let roaming_size = app
        .path()
        .app_data_dir()
        .map(|p| if p.exists() { get_dir_size(&p) } else { 0 })
        .map_err(|e| format!("无法获取应用数据目录: {}", e))?;
    Ok(StorageSizes {
        cache_size: get_cache_size(),
        data_size: roaming_size + get_local_size(),
    })
}

/// 清除缓存目录（图标等缓存文件，不含 WebView2 数据），返回释放字节数。
/// 封面缓存（covers）不在此列：它无法自动重建，删除后需整库重新解析音频才能恢复，
/// 因此跳过保护（若被外部工具误删，启动时的封面自愈仍可从音频重新提取）。
#[tauri::command]
pub async fn clear_cache(app: tauri::AppHandle) -> Result<u64, String> {
    let Some(root) = cache_root_dir() else {
        return Err("无法定位缓存目录".to_string());
    };
    if !root.exists() {
        return Ok(0);
    }
    // 封面缓存目录：%LOCALAPPDATA%\NexBox\cache\covers
    let covers_dir = app.path().app_cache_dir().ok().map(|p| p.join("covers"));
    let read_dir = fs::read_dir(&root).map_err(|e| format!("无法读取缓存目录 {}: {}", root.display(), e))?;
    let mut freed: u64 = 0;
    for entry in read_dir.flatten() {
        let p = entry.path();
        if is_webview_dir(&p) {
            continue; // 跳过 WebView2 数据，不纳入应用缓存
        }
        if covers_dir.as_deref().map(|c| p.starts_with(c)).unwrap_or(false) {
            continue; // 跳过受保护的封面缓存
        }
        let size = if p.is_dir() { get_dir_size(&p) } else { 0 };
        let result = if p.is_dir() {
            // 目录内包含封面缓存的祖先目录（如 cache\）：递归清理并剪枝，保留 covers 子树
            let contains_protected = covers_dir
                .as_deref()
                .map(|c| c.starts_with(&p))
                .unwrap_or(false);
            if contains_protected {
                freed += clear_dir_prune(&p, covers_dir.as_deref().unwrap_or(&p));
                let _ = fs::remove_dir(&p); // 子项已清空，兜底移除空目录本身
                Ok(())
            } else {
                fs::remove_dir_all(&p)
            }
        } else {
            fs::remove_file(&p)
        };
        match result {
            Ok(_) => freed += size,
            Err(e) => log::warn!("跳过无法删除 {}: {}", p.display(), e),
        }
    }
    Ok(freed)
}

/// 递归删除目录内容，但剪枝跳过 `exclude` 子树（不删除、不计入释放量）。
fn clear_dir_prune(dir: &Path, exclude: &Path) -> u64 {
    let mut freed: u64 = 0;
    let Ok(read_dir) = fs::read_dir(dir) else {
        return 0;
    };
    for entry in read_dir.flatten() {
        let p = entry.path();
        if p.starts_with(exclude) {
            continue;
        }
        if p.is_dir() {
            freed += clear_dir_prune(&p, exclude);
            let _ = fs::remove_dir(&p); // 子项已清空，移除空目录本身
        } else {
            let size = fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
            if fs::remove_file(&p).is_ok() {
                freed += size;
            }
        }
    }
    freed
}

/// 清除数据（彻底重置）：删除 Roaming（`%APPDATA%\NexBox`）与 Local（`%LOCALAPPDATA%\NexBox`）下可删除的内容。
/// 被占用的 WebView2 数据（EBWebView）由 `restart_app` 的重启脚本在进程退出后彻底删除。
#[tauri::command]
pub async fn clear_data(app: tauri::AppHandle) -> Result<u64, String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("无法获取应用数据目录: {}", e))?;
    let mut freed = clear_dir_contents(&path)?;
    // 同时清理 Local 下可删除的缓存（图标、公告、RuntimeRepair 等）与 WebView2 数据
    if let Some(root) = cache_root_dir() {
        freed += clear_dir_contents(&root)?;
    }
    Ok(freed)
}

/// 重启应用：启动一个「清理 + 重启」脚本后退出当前进程。
/// 脚本会等待当前进程完全退出，再删除 Local 数据目录（`%LOCALAPPDATA%\NexBox`，含被占用的 EBWebView，
/// 此时句柄已释放），最后启动新进程，从而彻底清除残留数据。
#[tauri::command]
pub fn restart_app(app: tauri::AppHandle) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("获取当前程序路径失败: {}", e))?;
    let old_pid = std::process::id();
    let exe_path = exe.to_string_lossy().replace('\'', "''");
    let local_root = cache_root_dir().map(|p| p.to_string_lossy().replace('\'', "''"));

    // 等待旧进程退出 → 删除 Local 数据目录 → 启动新进程
    let wait_loop = format!(
        "$oldPid={old_pid}; $deadline=(Get-Date).AddSeconds(10); \
         do {{ Start-Sleep -Milliseconds 200; $p=Get-Process -Id $oldPid -ErrorAction SilentlyContinue }} \
         while ($p -and (Get-Date) -lt $deadline); Start-Sleep -Milliseconds 600"
    );
    let script = match local_root {
        Some(root) => format!(
            "{wait_loop}; if (Test-Path -LiteralPath '{root}') {{ Remove-Item -LiteralPath '{root}' -Recurse -Force -ErrorAction SilentlyContinue }}; Start-Process -FilePath '{exe_path}'"
        ),
        None => format!("{wait_loop}; Start-Process -FilePath '{exe_path}'"),
    };

    Command::new("powershell")
        .args(["-WindowStyle", "Hidden", "-NoProfile", "-Command", &script])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .spawn()
        .map_err(|e| format!("启动清理重启脚本失败: {}", e))?;

    log::info!("清除数据后重启：等待进程 {} 退出后清理 Local 数据目录", old_pid);
    app.exit(0);
    Ok(())
}