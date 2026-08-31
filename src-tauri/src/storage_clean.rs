use crate::storage_scan::{current_user_visible_size, empty_all_recycle_bins};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanItem {
    pub id: String,
    pub name: String,
    pub path: String,
    pub exists: bool,
    pub size_bytes: u64,
    pub requires_admin: bool,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub items: Vec<CleanItem>,
    pub total_size: u64,
    pub total_items: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanResult {
    pub success: bool,
    pub message: String,
    pub freed_bytes: u64,
    pub skipped_files: Vec<String>,
}

fn get_dir_size(path: &std::path::Path) -> u64 {
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

fn get_file_size(path: &std::path::Path) -> u64 {
    if path.exists() && path.is_file() {
        fs::metadata(path).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    }
}

fn scan_clean_item(id: &str, name: &str, path: PathBuf, requires_admin: bool, description: &str) -> CleanItem {
    let exists = path.exists();
    let size_bytes = if exists {
        if path.is_dir() {
            get_dir_size(&path)
        } else {
            get_file_size(&path)
        }
    } else {
        0
    };
    CleanItem {
        id: id.to_string(),
        name: name.to_string(),
        path: path.to_string_lossy().to_string(),
        exists,
        size_bytes,
        requires_admin,
        description: description.to_string(),
    }
}

fn get_temp_path() -> Option<PathBuf> {
    std::env::var("TEMP").ok().map(PathBuf::from)
}

fn get_windows_temp() -> PathBuf {
    PathBuf::from("C:\\Windows\\Temp")
}

fn get_local_app_data() -> Option<PathBuf> {
    dirs::data_local_dir()
}

fn get_windows_dir() -> PathBuf {
    PathBuf::from("C:\\Windows")
}

#[allow(dead_code)]
fn get_thumbs_db_size(drive: &str) -> u64 {
    let mut total = 0;
    let drive_path = PathBuf::from(drive);
    if drive_path.exists() {
        if let Ok(entries) = fs::read_dir(&drive_path) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    total += scan_thumbs_db_in_dir(&p);
                }
            }
        }
    }
    total
}

#[allow(dead_code)]
fn scan_thumbs_db_in_dir(dir: &PathBuf) -> u64 {
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += scan_thumbs_db_in_dir(&p);
            } else if p.file_name().map(|n| n.to_string_lossy().to_lowercase() == "thumbs.db").unwrap_or(false) {
                total += get_file_size(&p);
            }
        }
    }
    total
}

#[tauri::command]
pub async fn scan_storage_items() -> Result<ScanResult, String> {
    let temp_path = get_temp_path();
    let local_app_data = get_local_app_data();
    let windows_dir = get_windows_dir();

    let mut items: Vec<CleanItem> = Vec::new();

    if let Some(temp) = temp_path {
        items.push(scan_clean_item(
            "temp_user",
            "用户临时文件",
            temp,
            false,
            "程序运行时产生的中间文件，可安全删除"
        ));
    }

    items.push(scan_clean_item(
        "temp_system",
        "系统临时文件",
        get_windows_temp(),
        true,
        "系统级临时文件，需要管理员权限"
    ));

    items.push(CleanItem {
        id: "recycle_bin".to_string(),
        name: "回收站".to_string(),
        path: "各磁盘 $Recycle.Bin".to_string(),
        exists: true,
        // 只统计当前用户可见条目($I/$R 配对),与清理口径一致:
        // desktop.ini、其他 SID 与孤儿元数据清不掉,也不展示。
        size_bytes: current_user_visible_size(),
        requires_admin: false,
        description: "已删除文件的暂存区，清空后释放磁盘空间".to_string(),
    });

    if let Some(local) = &local_app_data {
        items.push(scan_clean_item(
            "thumbnail_cache",
            "缩略图缓存",
            local.join("Microsoft").join("Windows").join("Explorer"),
            false,
            "图片、视频预览图缓存，删除后自动重建"
        ));

        items.push(scan_clean_item(
            "wer_archive",
            "错误报告存档",
            local.join("Microsoft").join("Windows").join("WER").join("ReportArchive"),
            false,
            "程序崩溃错误报告存档"
        ));

        items.push(scan_clean_item(
            "wer_queue",
            "错误报告队列",
            local.join("Microsoft").join("Windows").join("WER").join("ReportQueue"),
            false,
            "待上报的错误报告"
        ));

        items.push(scan_clean_item(
            "crash_dumps",
            "崩溃转储",
            local.join("CrashDumps"),
            false,
            "应用程序崩溃转储文件"
        ));

    }

    items.push(scan_clean_item(
        "prefetch",
        "预读文件",
        windows_dir.join("Prefetch"),
        true,
        "程序启动预读数据，删除后首次启动略慢"
    ));

    items.push(scan_clean_item(
        "memory_dmp",
        "完整内存转储",
        windows_dir.join("MEMORY.DMP"),
        true,
        "蓝屏完整内存转储文件"
    ));

    items.push(scan_clean_item(
        "minidump",
        "蓝屏精简转储",
        windows_dir.join("Minidump"),
        true,
        "蓝屏精简转储文件，排查问题后可删除"
    ));

    items.push(scan_clean_item(
        "windows_logs",
        "系统日志",
        windows_dir.join("Logs"),
        true,
        "Windows各组件运行日志"
    ));

    if let Some(_local) = &local_app_data {
        items.push(CleanItem {
            id: "thumbs_db".to_string(),
            name: "thumbs.db残留文件".to_string(),
            path: "各磁盘分布".to_string(),
            exists: false,
            size_bytes: 0,
            requires_admin: false,
            description: "早期Windows遗留的缩略图缓存文件（扫描较慢，暂不支持）".to_string(),
        });
    }

    let total_size = items.iter().map(|i| i.size_bytes).sum();
    let total_items = items.iter().filter(|i| i.exists && i.size_bytes > 0).count() as u64;

    Ok(ScanResult {
        items,
        total_size,
        total_items,
    })
}

fn clean_dir_contents(path: &PathBuf) -> (u64, Vec<String>) {
    let mut freed = 0u64;
    let mut skipped = Vec::new();

    if !path.exists() || !path.is_dir() {
        return (freed, skipped);
    }

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let (sub_freed, sub_skipped) = clean_dir_contents(&p);
                freed += sub_freed;
                skipped.extend(sub_skipped);
                if fs::remove_dir(&p).is_ok() {
                } else {
                    skipped.push(p.to_string_lossy().to_string());
                }
            } else {
                let size = get_file_size(&p);
                match fs::remove_file(&p) {
                    Ok(_) => freed += size,
                    Err(_) => skipped.push(p.to_string_lossy().to_string()),
                }
            }
        }
    }

    (freed, skipped)
}

fn clean_single_file(path: &PathBuf) -> (u64, Vec<String>) {
    if !path.exists() {
        return (0, Vec::new());
    }

    let size = get_file_size(path);
    match fs::remove_file(path) {
        Ok(_) => (size, Vec::new()),
        Err(_) => (0, vec![path.to_string_lossy().to_string()]),
    }
}

fn clean_thumbs_db_files(drive: &str) -> (u64, Vec<String>) {
    let mut freed = 0u64;
    let mut skipped = Vec::new();
    let drive_path = PathBuf::from(drive);

    if drive_path.exists() {
        if let Ok(entries) = fs::read_dir(&drive_path) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    let (sub_freed, sub_skipped) = clean_thumbs_db_in_dir(&p);
                    freed += sub_freed;
                    skipped.extend(sub_skipped);
                }
            }
        }
    }

    (freed, skipped)
}

fn clean_thumbs_db_in_dir(dir: &PathBuf) -> (u64, Vec<String>) {
    let mut freed = 0u64;
    let mut skipped = Vec::new();

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let (sub_freed, sub_skipped) = clean_thumbs_db_in_dir(&p);
                freed += sub_freed;
                skipped.extend(sub_skipped);
            } else if p.file_name().map(|n| n.to_string_lossy().to_lowercase() == "thumbs.db").unwrap_or(false) {
                let size = get_file_size(&p);
                match fs::remove_file(&p) {
                    Ok(_) => freed += size,
                    Err(_) => skipped.push(p.to_string_lossy().to_string()),
                }
            }
        }
    }

    (freed, skipped)
}

#[tauri::command]
pub async fn clean_storage_items(item_ids: Vec<String>) -> Result<CleanResult, String> {
    let temp_path = get_temp_path();
    let local_app_data = get_local_app_data();
    let windows_dir = get_windows_dir();

    let mut total_freed = 0u64;
    let mut all_skipped: Vec<String> = Vec::new();

    for id in item_ids {
        let (freed, skipped) = match id.as_str() {
            "temp_user" => {
                if let Some(temp) = &temp_path {
                    clean_dir_contents(temp)
                } else {
                    (0, Vec::new())
                }
            }
            "temp_system" => {
                clean_dir_contents(&get_windows_temp())
            }
            "recycle_bin" => {
                // Shell 标准流程清空全部磁盘回收站;释放量按清理前的可见条目统计,
                // 与扫描口径一致。desktop.ini 等不可清理项本来就不计入。
                let size = current_user_visible_size();
                match empty_all_recycle_bins() {
                    Ok(_) => (size, Vec::new()),
                    Err(error) => (0, vec![format!("清空回收站失败: {}", error)]),
                }
            }
            "thumbnail_cache" => {
                if let Some(local) = &local_app_data {
                    let cache_dir = local.join("Microsoft").join("Windows").join("Explorer");
                    let mut freed = 0u64;
                    let mut skipped = Vec::new();

                    if let Ok(entries) = fs::read_dir(&cache_dir) {
                        for entry in entries.flatten() {
                            let p = entry.path();
                            if p.file_name().map(|n| n.to_string_lossy().starts_with("thumbcache")).unwrap_or(false) {
                                let size = get_file_size(&p);
                                match fs::remove_file(&p) {
                                    Ok(_) => freed += size,
                                    Err(_) => skipped.push(p.to_string_lossy().to_string()),
                                }
                            }
                        }
                    }
                    (freed, skipped)
                } else {
                    (0, Vec::new())
                }
            }
            "prefetch" => {
                let prefetch_dir = windows_dir.join("Prefetch");
                let mut freed = 0u64;
                let mut skipped = Vec::new();

                if let Ok(entries) = fs::read_dir(&prefetch_dir) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if p.extension().map(|e| e.to_string_lossy() == "pf").unwrap_or(false) {
                            let size = get_file_size(&p);
                            match fs::remove_file(&p) {
                                Ok(_) => freed += size,
                                Err(_) => skipped.push(p.to_string_lossy().to_string()),
                            }
                        }
                    }
                }
                (freed, skipped)
            }
            "wer_archive" => {
                if let Some(local) = &local_app_data {
                    clean_dir_contents(&local.join("Microsoft").join("Windows").join("WER").join("ReportArchive"))
                } else {
                    (0, Vec::new())
                }
            }
            "wer_queue" => {
                if let Some(local) = &local_app_data {
                    clean_dir_contents(&local.join("Microsoft").join("Windows").join("WER").join("ReportQueue"))
                } else {
                    (0, Vec::new())
                }
            }
            "crash_dumps" => {
                if let Some(local) = &local_app_data {
                    clean_dir_contents(&local.join("CrashDumps"))
                } else {
                    (0, Vec::new())
                }
            }
            "memory_dmp" => {
                clean_single_file(&windows_dir.join("MEMORY.DMP"))
            }
            "minidump" => {
                clean_dir_contents(&windows_dir.join("Minidump"))
            }
            "windows_logs" => {
                clean_dir_contents(&windows_dir.join("Logs"))
            }
            "thumbs_db" => {
                clean_thumbs_db_files("C:")
            }
            _ => (0, Vec::new()),
        };

        total_freed += freed;
        all_skipped.extend(skipped);
    }

    let message = if all_skipped.is_empty() {
        format_size_message(total_freed)
    } else {
        format!(
            "清理完成，释放 {}。跳过 {} 个被占用文件",
            format_size(total_freed),
            all_skipped.len()
        )
    };

    Ok(CleanResult {
        success: true,
        message,
        freed_bytes: total_freed,
        skipped_files: all_skipped,
    })
}

#[tauri::command]
pub async fn empty_recycle_bin_cmd() -> Result<CleanResult, String> {
    let size = current_user_visible_size();
    match empty_all_recycle_bins() {
        Ok(_) => Ok(CleanResult {
            success: true,
            message: "回收站已清空".to_string(),
            freed_bytes: size,
            skipped_files: Vec::new(),
        }),
        Err(error) => Err(format!("清空回收站失败: {}", error)),
    }
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn format_size_message(bytes: u64) -> String {
    if bytes == 0 {
        "没有需要清理的文件".to_string()
    } else {
        format!("清理完成，释放 {}", format_size(bytes))
    }
}