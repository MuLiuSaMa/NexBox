use std::fs;
use std::fs::File;
use std::io::Write;
use std::process::Command;
use std::os::windows::process::CommandExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use futures_util::StreamExt;
use reqwest::Client;
use tauri::{Window, Emitter, AppHandle};

#[derive(Clone, serde::Serialize)]
struct DownloadProgress {
    progress: u64,
    total: u64,
}

/// 已下载待安装的更新包路径（进程内标记，退出时自动启动安装向导）
static PENDING_INSTALL: Mutex<Option<String>> = Mutex::new(None);

/// 取消下载标志：关闭静默更新时置位，下载循环据此中止
static CANCEL_DOWNLOAD: AtomicBool = AtomicBool::new(false);

/// 读取静默更新开关：读取 settings.json 中 nexbox_auto_update，默认开启
pub fn auto_update_enabled() -> bool {
    let Some(config_dir) = dirs::config_dir() else {
        return true;
    };
    let settings_path = config_dir.join("NexBox").join("settings.json");
    let Ok(content) = std::fs::read_to_string(&settings_path) else {
        return true;
    };
    // 去空格后匹配，兼容 compact/pretty 两种 JSON 写法
    let normalized: String = content.chars().filter(|c| !c.is_whitespace()).collect();
    if normalized.contains("\"nexbox_auto_update\":false") {
        return false;
    }
    true
}

/// 使用系统 ShellExecuteW 在系统默认浏览器中打开指定 URL
#[tauri::command]
pub fn open_system_browser(url: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let url_wide: Vec<u16> = url
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let verb: Vec<u16> = "open\0".encode_utf16().collect();

        // 使用 ShellExecuteW 调用系统默认浏览器，无需 cmd 中介
        let hinst = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                url_wide.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };

        // ShellExecuteW 返回值 <= 32 表示错误
        if hinst as isize <= 32 {
            return Err(format!("Failed to open url (error: {})", hinst as isize));
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        // 非 Windows 平台回退到 xdg-open / open
        let _ = Command::new("xdg-open").arg(&url).spawn();
        let _ = Command::new("open").arg(&url).spawn();
    }
    Ok(())
}

/// 等待文件完全落盘且可访问后再启动安装向导。
/// 刚下载完成后立刻打开可能被安全软件扫描占用或尚未完全刷入磁盘，
/// 这里轮询等待文件可被打开（闲放多轮重试），确保"点击重启"时一定按在真实安装包上。
pub fn wait_until_file_ready(file_path: &str) {
    const MAX_ATTEMPTS: u32 = 10;
    const RETRY_DELAY_MS: u64 = 300;
    for _ in 0..MAX_ATTEMPTS {
        // 尝试以读写方式独占打开以探测句柄是否已释放；成功后立即关闭
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(file_path)
        {
            Ok(f) => {
                drop(f);
                return;
            }
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS)),
        }
    }
}

/// 以 SW_SHOWNORMAL 方式异步启动安装向导，立即返回（不等待安装完成）
pub fn launch_installer_sync(file_path: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        // 先确保文件已落盘且句柄释放，避免启动时文件仍在被占用导致失败
        wait_until_file_ready(file_path);

        let file_path_wide: Vec<u16> = file_path
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let verb: Vec<u16> = "open\0".encode_utf16().collect();

        // 使用 ShellExecuteW 直接启动安装包，避免 cmd 中介弹出终端窗口
        let hinst = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                file_path_wide.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };

        // ShellExecuteW 返回值 <= 32 表示错误
        if hinst as isize <= 32 {
            return Err(format!("Failed to launch installer (error: {})", hinst as isize));
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        return Err("Only Windows is supported".to_string());
    }
    Ok(())
}

/// GitCode API 返回的 release 资产 URL 可能落在 test.gitcode.net 等
/// 在当前网络环境不可达的 CDN 域名上。统一替换为主站域名 gitcode.com，
/// 由其 302 重定向到可达的 file-cdn.gitcode.com 并生成签名链接。
/// reqwest 默认自动跟随重定向，因此替换后即可正常下载。
pub(crate) fn normalize_gitcode_url(url: &str) -> String {
    const GITCODE_CDN_HOSTS: [&str; 2] = ["test.gitcode.net", "download.gitcode.net"];
    let mut normalized = url.to_string();
    for host in GITCODE_CDN_HOSTS {
        normalized = normalized.replace(
            &format!("https://{host}/"),
            "https://gitcode.com/",
        );
        normalized = normalized.replace(
            &format!("http://{host}/"),
            "https://gitcode.com/",
        );
    }
    normalized
}

#[tauri::command]
pub async fn download_file(
    url: String,
    file_name: String,
    window: Window,
) -> Result<String, String> {
    let client = Client::new();
    let url = normalize_gitcode_url(&url);
    let response = client.get(&url).send().await.map_err(|e| e.to_string())?;

    let total_size = response.content_length().unwrap_or(0);

    let download_path = match dirs::download_dir() {
        Some(mut path) => {
            path.push(file_name);
            path
        }
        None => {
            let mut path = std::env::current_dir().map_err(|e| e.to_string())?;
            path.push(file_name);
            path
        }
    };

    // 先检查 HTTP 状态码：非 2xx（限流 429 / 认证 401 / 5xx 等）直接失败，
    // 避免把 GitCode 返回的错误页/提示页当作安装包写盘导致 1KB 坏文件
    if !response.status().is_success() {
        return Err(format!(
            "Download failed (HTTP {}): {}",
            response.status().as_u16(),
            url
        ));
    }

    // 作用域内写文件：结束即 drop，确保返回给前端前句柄已关闭
    {
        let mut file = File::create(&download_path).map_err(|e| e.to_string())?;

        let mut stream = response.bytes_stream();
        let mut downloaded: u64 = 0;
        let mut last_emit = std::time::Instant::now();
        let mut last_emitted: u64 = 0;

        while let Some(chunk) = stream.next().await {
            // 检查取消标志：关闭静默更新后中止下载
            if CANCEL_DOWNLOAD.load(Ordering::SeqCst) {
                return Err("Download cancelled".to_string());
            }
            let chunk = chunk.map_err(|e| e.to_string())?;
            file.write_all(&chunk).map_err(|e| e.to_string())?;
            downloaded += chunk.len() as u64;

            let progress = if total_size > 0 {
                (downloaded * 100) / total_size
            } else {
                0
            };

            // 节流：至少间隔 200ms 且进度前进时才向前端推送，避免高频事件导致 UI 数字跳闪
            if progress != last_emitted && last_emit.elapsed().as_millis() >= 200 {
                last_emit = std::time::Instant::now();
                last_emitted = progress;
                match window.emit(
                    "download-progress",
                    DownloadProgress {
                        progress,
                        total: total_size,
                    },
                ) {
                    Ok(_) => {},
                    Err(e) => {
                        eprintln!("Failed to emit progress: {}", e);
                    }
                }
            }
        }

        // sync_all 强制刷入磁盘；flush 只写内核缓冲，无法保证真正落盘
        file.sync_all().map_err(|e| e.to_string())?;

        // 完整性校验：若服务端声明了 Content-Length，实际写入字节必须一致，
        // 不一致说明下载中断/被截断，删除坏文件并报错，避免生成残缺安装包
        if total_size > 0 && downloaded != total_size {
            drop(file);
            let _ = std::fs::remove_file(&download_path);
            return Err(format!(
                "Download incomplete: expected {total_size} bytes, got {downloaded}"
            ));
        }
        // 作用域结束，file 在此 drop，句柄关闭、完全落盘后才会返回路径
    }

    Ok(download_path.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn open_installer(file_path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        Command::new("cmd")
            .args(["/c", "start", "", &file_path])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        return Err("Only Windows is supported".to_string());
    }
    Ok(())
}

#[tauri::command]
pub async fn download_update(
    url: String,
    file_name: String,
    silent: bool,
    window: Window,
) -> Result<String, String> {
    // 静默自动下载时检查开关；用户手动点击下载始终允许
    if silent && !auto_update_enabled() {
        return Err("Silent update disabled".to_string());
    }
    download_file(url, file_name, window).await
}

#[tauri::command]
pub async fn install_update(
    file_path: String,
    app_handle: AppHandle,
) -> Result<(), String> {
    launch_installer_sync(&file_path)?;

    std::thread::sleep(std::time::Duration::from_millis(500));

    app_handle.exit(0);

    Ok(())
}

/// 关闭静默更新时调用：置位取消标志，中止进行中的下载
#[tauri::command]
pub fn cancel_download() -> Result<(), String> {
    CANCEL_DOWNLOAD.store(true, Ordering::SeqCst);
    Ok(())
}

/// 开始新下载前调用：清除取消标志
#[tauri::command]
pub fn reset_download_cancel() -> Result<(), String> {
    CANCEL_DOWNLOAD.store(false, Ordering::SeqCst);
    Ok(())
}

/// 下载完成后由前端调用，登记待安装包路径（供关闭软件时自动启动安装向导）
#[tauri::command]
pub fn mark_pending_install(file_path: String) -> Result<(), String> {
    // 后端兜底：静默更新已关闭时不登记待安装，退出时不会自动启动安装向导
    if !auto_update_enabled() {
        return Err("Silent update disabled".to_string());
    }
    if !std::path::Path::new(&file_path).exists() {
        return Err(format!("Install file not found: {file_path}"));
    }
    *PENDING_INSTALL.lock().map_err(|e| e.to_string())? = Some(file_path);
    Ok(())
}

/// 用户点击"重启安装"、跳过或删除文件时清除待安装标记
#[tauri::command]
pub fn clear_pending_install() -> Result<(), String> {
    *PENDING_INSTALL.lock().map_err(|e| e.to_string())? = None;
    Ok(())
}

/// 供 RunEvent::ExitRequested 读取；取出并清除待安装包路径（进程即将退出）
pub fn take_pending_install() -> Option<String> {
    PENDING_INSTALL.lock().ok().and_then(|mut g| g.take())
}

#[tauri::command]
pub async fn delete_download_file(file_path: String) -> Result<(), String> {
    if std::path::Path::new(&file_path).exists() {
        fs::remove_file(&file_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}
