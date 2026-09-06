// ============================================================================
// 存储扫描模块(移植自 light-c-main)
//
// 包含:
// 1. 垃圾清理 - 按分类扫描已知缓存/临时目录,多线程并行,带多重安全保护
// 2. 大文件扫描 - 遍历用户选择的磁盘,收集 Top N 最大文件
// ============================================================================

mod big_files;
pub(crate) mod big_files_engine;
mod categories;
mod delete_engine;
mod file_info;
mod recycle_bin;
mod safety_constants;
mod scan_engine;
mod winapp2;

pub use big_files::LargeFileEntry;
pub use categories::JunkCategory;
pub use delete_engine::{DeleteEngine, empty_all_recycle_bins};
pub use file_info::{CategoryScanResult, DeleteResult, DeleteTarget, FileInfo, JunkScanResult};
pub use recycle_bin::current_user_visible_size;
pub use scan_engine::ScanEngine;
pub use winapp2::DeepScanEngine;

use log::info;
use serde::{Deserialize, Serialize};
use tauri::Window;

/// 扫描请求参数
#[derive(Debug, Deserialize)]
pub struct ScanRequest {
    pub categories: Option<Vec<String>>,
}

/// 分类信息(用于前端展示)
#[derive(Debug, Serialize)]
pub struct CategoryInfo {
    pub name: String,
    pub description: String,
    pub risk_level: u8,
    /// 是否默认勾选(内置分类恒 true)
    pub default_select: bool,
}

/// 执行垃圾文件扫描(深度清理 = 内置分类线 + Winapp2 规则线)
#[tauri::command]
pub async fn scan_junk_categories(request: Option<ScanRequest>) -> Result<JunkScanResult, String> {
    info!("开始扫描垃圾文件(深度清理)");

    let result = tokio::task::spawn_blocking(move || {
        // ---- 1. 内置分类线(request.categories 过滤仅作用于内置 17 分类) ----
        let engine = if let Some(req) = request {
            if let Some(category_names) = req.categories {
                let categories: Vec<JunkCategory> = JunkCategory::all()
                    .into_iter()
                    .filter(|c| category_names.contains(&c.display_name().to_string()))
                    .collect();

                if categories.is_empty() {
                    ScanEngine::new()
                } else {
                    ScanEngine::new().with_categories(categories)
                }
            } else {
                ScanEngine::new()
            }
        } else {
            ScanEngine::new()
        };
        let mut result = engine.scan();

        // ---- 2. Winapp2 规则线(深度清理核心) ----
        let entries = winapp2::load_entries();
        for category_result in DeepScanEngine::scan(&entries) {
            result.add_category_result(category_result);
        }

        result
    })
    .await
    .map_err(|e| format!("扫描任务异常: {}", e))?;

    info!(
        "扫描完成: {} 个分类, {} 个文件, {} 字节",
        result.categories.len(),
        result.total_file_count,
        result.total_size
    );

    Ok(result)
}

/// 扫描单个分类(内置分类或 Winapp2 条目)
#[tauri::command]
pub async fn scan_junk_category(category_name: String) -> Result<CategoryScanResult, String> {
    info!("扫描分类: {}", category_name);

    let result = tokio::task::spawn_blocking(move || -> Result<CategoryScanResult, String> {
        // 优先匹配内置分类
        if let Some(category) = JunkCategory::all()
            .into_iter()
            .find(|c| c.display_name() == category_name)
        {
            let engine = ScanEngine::new();
            return Ok(engine.scan_category(&category));
        }

        // 回退到 Winapp2 单条目扫描
        let entries = winapp2::load_entries();
        DeepScanEngine::scan_single(&entries, &category_name)
            .ok_or_else(|| format!("未知分类: {}", category_name))
    })
    .await
    .map_err(|e| format!("扫描任务异常: {}", e))??;

    Ok(result)
}

/// 获取所有可用的内置清理分类
#[tauri::command]
pub fn get_junk_categories() -> Vec<CategoryInfo> {
    JunkCategory::all()
        .into_iter()
        .map(|c| CategoryInfo {
            name: c.display_name().to_string(),
            description: c.description().to_string(),
            risk_level: c.risk_level(),
            default_select: true,
        })
        .collect()
}

/// 获取当前生效的 Winapp2 规则库信息(版本/条目数/是否内置)
#[tauri::command]
pub fn get_winapp2_rule_info() -> winapp2::update::RuleDatabaseInfo {
    winapp2::update::get_info()
}

/// 在线更新 Winapp2 规则库到数据目录(仿照图吧工具箱;进度通过
/// "winapp2-update:progress" 事件推送)
#[tauri::command]
pub async fn update_winapp2_rules(
    window: Window,
) -> Result<winapp2::update::RuleDatabaseInfo, String> {
    winapp2::update::update_rules(Some(window)).await
}

/// 删除指定的垃圾文件(纯文件/目录,不含注册表)
#[tauri::command]
pub async fn delete_junk_files(targets: Vec<DeleteTarget>) -> Result<DeleteResult, String> {
    info!("开始删除 {} 个目标", targets.len());

    let result = tokio::task::spawn_blocking(move || {
        DeleteEngine::new().delete_paths(&targets)
    })
    .await
    .map_err(|e| format!("删除任务异常: {}", e))?;

    info!(
        "删除完成: 成功 {}, 失败 {}, 释放 {} 字节",
        result.success_count, result.failed_count, result.freed_size
    );

    Ok(result)
}

/// 扫描系统盘大文件,并实时推送进度
#[tauri::command]
pub async fn scan_large_files(
    window: Window,
    top_n: Option<usize>,
    drive_letter: Option<String>,
) -> Result<Vec<LargeFileEntry>, String> {
    // 大文件列表会直接渲染到前端,命令层收敛数量,避免异常配置造成界面和扫描压力失控。
    // 前端不再提供数量选择,固定返回最大的 100 个文件。
    let top_n = top_n.unwrap_or(100).clamp(10, 500);
    let drive_letter = normalize_large_file_drive_letter(drive_letter.as_deref())?;
    tokio::task::spawn_blocking(move || big_files::scan(&window, top_n, drive_letter))
        .await
        .map_err(|e| format!("扫描任务异常: {}", e))?
}

/// 取消大文件扫描
#[tauri::command]
pub fn cancel_large_file_scan() {
    big_files::cancel();
}

/// 在资源管理器中定位/打开文件所在目录
#[tauri::command]
pub fn reveal_large_file(path: String) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    if path.trim().is_empty() {
        return Err("文件路径为空".to_string());
    }

    let path_buf = std::path::PathBuf::from(&path);
    // 已删除的文件,打开其所在目录
    if !path_buf.exists() {
        if let Some(parent) = path_buf.parent() {
            if parent.exists() {
                Command::new("explorer")
                    .arg(&parent.to_string_lossy().to_string())
                    .creation_flags(CREATE_NO_WINDOW)
                    .spawn()
                    .map_err(|e| format!("打开资源管理器失败: {}", e))?;
                return Ok(());
            }
        }
        return Err("文件不存在且无法确定所在目录".to_string());
    }

    // 存在则选中该文件
    Command::new("explorer")
        .arg("/select,")
        .arg(&path)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("打开资源管理器失败: {}", e))?;
    Ok(())
}

/// 强制删除大文件(调用删除引擎,含多层安全保护)
#[tauri::command]
pub async fn delete_large_file(paths: Vec<String>) -> Result<DeleteResult, String> {
    info!("大文件强制删除: 开始删除 {} 个文件", paths.len());

    // 大文件删除未携带已知大小(size 置 None),删除引擎会自行查询
    let targets: Vec<DeleteTarget> = paths
        .into_iter()
        .map(|path| DeleteTarget {
            path,
            size: None,
        })
        .collect();

    let result = tokio::task::spawn_blocking(move || {
        let engine = DeleteEngine::new();
        engine.delete_paths(&targets)
    })
    .await
    .map_err(|e| format!("删除任务异常: {}", e))?;

    info!(
        "大文件强制删除完成: 成功 {}, 失败 {}, 释放 {} 字节",
        result.success_count, result.failed_count, result.freed_size
    );
    Ok(result)
}

/// 获取本机存在的固定磁盘盘符列表
#[tauri::command]
pub fn get_drive_list() -> Vec<String> {
    ('A'..='Z')
        .filter(|letter| {
            let root = format!("{}:\\", letter);
            std::path::Path::new(&root).is_dir()
        })
        .map(|letter| format!("{}:", letter))
        .collect()
}

fn normalize_large_file_drive_letter(value: Option<&str>) -> Result<char, String> {
    // 前端只传盘符,但这里仍做兜底校验,避免手动调用命令时传入路径或特殊字符。
    let raw = value
        .and_then(|text| text.chars().find(|ch| ch.is_ascii_alphabetic()))
        .unwrap_or_else(|| {
            std::env::var("SYSTEMDRIVE")
                .ok()
                .and_then(|drive| drive.chars().find(|ch| ch.is_ascii_alphabetic()))
                .unwrap_or('C')
        })
        .to_ascii_uppercase();

    let root = format!("{}:\\", raw);
    if !std::path::Path::new(&root).is_dir() {
        return Err(format!("磁盘不存在或不可访问: {}", root));
    }

    Ok(raw)
}
