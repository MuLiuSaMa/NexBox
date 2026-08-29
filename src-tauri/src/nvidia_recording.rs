// ============================================================================
// N卡录制管理
//
// 自动扫描 N 卡录制视频目录（GeForce Experience / NVIDIA App 默认位置 + 用户
// 自定义目录），按时间列出视频，支持永久删除 / 打开 / 定位。
// 自定义目录列表持久化到 settings.json（键 nexbox_nvidia_recording_folders）。
// ============================================================================

use log::{debug, info, warn};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// 录制视频扩展名白名单（扫描过滤 + 删除安全网）
const VIDEO_EXTS: &[&str] = &["mp4", "mov", "avi", "mkv", "wmv", "flv", "webm", "m4v"];

/// settings.json 中自定义录制目录列表的键（JSON 字符串数组）
const SETTINGS_KEY: &str = "nexbox_nvidia_recording_folders";

/// 单个目录扫描的视频上限，防止超大目录卡死 UI
const MAX_VIDEOS_PER_FOLDER: usize = 3000;

/// 扫描目录的最大递归深度
const MAX_DEPTH: usize = 5;

/// 录制文件夹信息
#[derive(Debug, Serialize)]
pub struct RecordingFolder {
    pub path: String,
    pub name: String,
    /// 是否用户手动添加（默认位置为 false）
    pub custom: bool,
    pub video_count: usize,
    pub total_size: u64,
}

/// 单个录制视频
#[derive(Debug, Serialize)]
pub struct RecordingVideo {
    pub path: String,
    pub name: String,
    pub size: u64,
    /// 修改时间（毫秒时间戳，用于按天分组与排序）
    pub modified_ms: u64,
    pub created_ms: u64,
    pub ext: String,
}

/// 扫描结果
#[derive(Debug, Serialize)]
pub struct RecordingScanResult {
    /// 按视频数降序（有内容的在前）
    pub folders: Vec<RecordingFolder>,
    /// 全部视频，按修改时间倒序
    pub videos: Vec<RecordingVideo>,
}

/// 删除结果
#[derive(Debug, Serialize)]
pub struct NvidiaDeleteResult {
    pub deleted: Vec<String>,
    pub errors: Vec<(String, String)>,
}

/// 批量复制结果
#[derive(Debug, Serialize)]
pub struct NvidiaCopyResult {
    pub copied: Vec<String>,
    pub errors: Vec<(String, String)>,
    /// 用户取消了目标文件夹选择
    pub cancelled: bool,
}

/// 判断是否为支持的视频扩展名
fn is_video_ext(path: &std::path::Path) -> Option<String> {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .filter(|ext| VIDEO_EXTS.contains(&ext.as_str()))
}

/// 规范化路径用于去重比较：统一分隔符、去除大小写差异与尾部反斜杠
fn normalize_path(path: &str) -> String {
    let p = path.replace('/', "\\");
    let p = p.trim_end_matches('\\').to_string();
    p.to_lowercase()
}

/// 读取持久化的自定义目录列表
fn read_custom_folders(app: &tauri::AppHandle) -> Vec<String> {
    crate::hotkey::read_settings_value(app, SETTINGS_KEY)
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

/// 持久化自定义目录列表
fn write_custom_folders(app: &tauri::AppHandle, folders: &[String]) {
    let value = serde_json::json!(folders);
    crate::hotkey::save_settings_value(app, SETTINGS_KEY, value);
}

/// 默认候选录制目录（都会去重，存在才扫描）
fn default_candidate_folders() -> Vec<String> {
    let mut list = Vec::new();
    if let Some(video_dir) = dirs::video_dir() {
        list.push(video_dir.to_string_lossy().into_owned());
        list.push(video_dir.join("NVIDIA App").to_string_lossy().into_owned());
        list.push(video_dir.join("NVIDIA").to_string_lossy().into_owned());
    }
    list
}

/// 扫描单个目录，返回 (视频数, 总字节数)
fn scan_folder_videos(folder: &str) -> (Vec<RecordingVideo>, usize, u64) {
    let mut videos = Vec::new();
    let mut count = 0usize;
    let mut total = 0u64;

    let walker = walkdir::WalkDir::new(folder)
        .max_depth(MAX_DEPTH)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            // 跳过隐藏/系统目录（递归遍历时比深挖更有价值）
            !is_hidden_path(e.path())
        });

    for entry in walker.filter_map(|e| e.ok()) {
        if count >= MAX_VIDEOS_PER_FOLDER {
            break;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let Some(ext) = is_video_ext(path) else {
            continue;
        };
        if let Ok(meta) = entry.metadata() {
            let size = meta.len();
            let modified_ms = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let created_ms = meta
                .created()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            videos.push(RecordingVideo {
                path: path.to_string_lossy().into_owned(),
                name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                size,
                modified_ms,
                created_ms,
                ext,
            });
            count += 1;
            total += size;
        }
    }

    (videos, count, total)
}

/// 跳过隐藏/系统文件，避免把音频、垃圾目录等非录制内容全量扫进来
fn is_hidden_path(path: &std::path::Path) -> bool {
    path.file_name()
        .map(|n| n.to_string_lossy().starts_with('$') || n.to_string_lossy().starts_with('.'))
        .unwrap_or(false)
}

/// 扫描 N 卡录制目录（默认位置 + 用户自定义目录）。
#[tauri::command]
pub async fn scan_nvidia_recordings(app: tauri::AppHandle) -> Result<RecordingScanResult, String> {
    let custom_list = read_custom_folders(&app);
    let default_list = default_candidate_folders();

    // 汇总候选目录（默认在前、自定义在后），按规范化路径去重
    let custom_set: std::collections::HashSet<String> = custom_list
        .iter()
        .map(|p| normalize_path(p))
        .collect();
    // 记录候选目录的原始顺序：默认目录按 候选定义顺序（视频 → NVIDIA App → NVIDIA），
    // 自定义目录按添加顺序排在最后
    let mut orig_index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (i, p) in default_list.iter().chain(custom_list.iter()).enumerate() {
        orig_index.entry(normalize_path(p)).or_insert(i);
    }
    let mut seen = std::collections::HashSet::new();
    let mut candidates: Vec<String> = Vec::new();
    for path in default_list.iter().cloned().chain(custom_list.iter().cloned()) {
        let norm = normalize_path(&path);
        if seen.insert(norm) {
            candidates.push(path);
        }
    }
    // 深目录优先扫描：视频归属到最深匹配目录，避免父子目录同时入选造成重复
    candidates.sort_by_key(|p| {
        std::cmp::Reverse(
            p.split(['\\', '/'])
                .filter(|s| !s.is_empty())
                .count(),
        )
    });

    let mut folders_final: Vec<RecordingFolder> = Vec::new();
    let mut videos: Vec<RecordingVideo> = Vec::new();
    // 全局视频路径去重：父目录递归扫描与子目录扫描会命中同一文件
    let mut seen_videos: std::collections::HashSet<String> = std::collections::HashSet::new();

    let results = tokio::task::spawn_blocking(move || {
        let mut per_folder: Vec<(String, RecordingFolder, Vec<RecordingVideo>)> = Vec::new();
        for path in &candidates {
            if !std::path::Path::new(path).is_dir() {
                debug!("[NvidiaRecording] 目录不存在，跳过: {path}");
                continue;
            }
            let (folder_videos, count, total) = scan_folder_videos(path);
            let name = std::path::Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.clone());
            per_folder.push((
                path.clone(),
                RecordingFolder {
                    path: path.clone(),
                    name,
                    custom: custom_set.contains(&normalize_path(path)),
                    video_count: count,
                    total_size: total,
                },
                folder_videos,
            ));
        }
        per_folder
    })
    .await
    .map_err(|e| format!("扫描任务异常: {e}"))?;

    for (_path, mut folder, folder_videos) in results {
        // 每个视频只保留一份（归属最深目录），目录计数同步去重
        let mut unique_count = 0usize;
        let mut unique_total = 0u64;
        for v in folder_videos {
            if seen_videos.insert(normalize_path(&v.path)) {
                unique_total += v.size;
                unique_count += 1;
                videos.push(v);
            }
        }
        folder.video_count = unique_count;
        folder.total_size = unique_total;
        // 默认位置目录只有在包含视频时才展示；自定义目录始终展示（可提示为空）
        if !folder.custom && folder.video_count == 0 {
            continue;
        }
        folders_final.push(folder);
    }

    // 展示顺序：默认目录在前（视频 → NVIDIA App → NVIDIA），自定义目录按添加顺序跟在 NVIDIA 后面
    folders_final.sort_by_key(|f| {
        let idx = orig_index
            .get(&normalize_path(&f.path))
            .copied()
            .unwrap_or(usize::MAX);
        (f.custom, idx)
    });
    videos.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms));

    info!(
        "[NvidiaRecording] 扫描完成: {} 个目录, {} 个视频",
        folders_final.len(),
        videos.len()
    );
    Ok(RecordingScanResult {
        folders: folders_final,
        videos,
    })
}

/// 弹出目录选择框，添加自定义录制目录并持久化，返回最新目录列表。
#[tauri::command]
pub fn add_nvidia_recording_folder(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let selected = rfd::FileDialog::new()
        .set_title("选择 N 卡录制文件夹")
        .pick_folder()
        .map(|f| f.to_string_lossy().into_owned());
    let Some(path) = selected else {
        return Ok(read_custom_folders(&app)); // 用户取消，返回当前列表
    };
    if path.trim().is_empty() || !std::path::Path::new(&path).is_dir() {
        return Err("所选目录不存在或不可访问".to_string());
    }
    let mut folders = read_custom_folders(&app);
    let norm = normalize_path(&path);
    if !folders.iter().any(|p| normalize_path(p) == norm) {
        folders.push(path.clone());
        write_custom_folders(&app, &folders);
        info!("[NvidiaRecording] 已添加自定义录制目录: {path}");
    }
    Ok(folders)
}

/// 从自定义目录列表中移除指定目录，返回最新列表。
#[tauri::command]
pub fn remove_nvidia_recording_folder(app: tauri::AppHandle, path: String) -> Result<Vec<String>, String> {
    let mut folders = read_custom_folders(&app);
    let target = normalize_path(&path);
    folders.retain(|p| normalize_path(p) != target);
    write_custom_folders(&app, &folders);
    info!("[NvidiaRecording] 已移除自定义录制目录: {path}");
    Ok(folders)
}

/// 使用系统默认播放器打开视频。
/// 走后端 opener API（不经前端 capability 桥），避免权限/作用域差异导致打不开。
#[tauri::command]
pub fn open_video_with_system_player(path: String) -> Result<(), String> {
    if !std::path::Path::new(&path).is_file() {
        return Err("文件不存在或不可访问".to_string());
    }
    tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|e| format!("打开失败: {e}"))
}

/// 在资源管理器中定位视频文件
#[tauri::command]
pub fn reveal_video_in_explorer(path: String) -> Result<(), String> {
    if !std::path::Path::new(&path).exists() {
        return Err("文件不存在或不可访问".to_string());
    }
    tauri_plugin_opener::reveal_item_in_dir(&path).map_err(|e| format!("定位失败: {e}"))
}

/// 批量复制录制视频：弹出目标文件夹选择框后逐一复制（重名自动追加序号）。
#[tauri::command]
pub async fn copy_nvidia_recording_videos(paths: Vec<String>) -> Result<NvidiaCopyResult, String> {
    use std::path::PathBuf;
    let Some(dest) = rfd::FileDialog::new()
        .set_title("选择复制目标文件夹")
        .pick_folder()
        .map(|f| f.to_path_buf())
    else {
        return Ok(NvidiaCopyResult {
            copied: vec![],
            errors: vec![],
            cancelled: true,
        });
    };
    tokio::task::spawn_blocking(move || {
        let mut copied = Vec::new();
        let mut errors = Vec::new();
        for raw in paths {
            let src = PathBuf::from(&raw);
            if !src.is_file() {
                errors.push((raw.clone(), "文件不存在或不可访问".to_string()));
                continue;
            }
            // 目标重名时追加 " (n)"，不覆盖已有文件
            let mut target = dest.join(src.file_name().map(|n| n.to_os_string()).unwrap_or_default());
            if target.exists() {
                let stem = src
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let ext = src
                    .extension()
                    .map(|e| format!(".{}", e.to_string_lossy()))
                    .unwrap_or_default();
                let mut i = 1i64;
                loop {
                    target = dest.join(format!("{} ({}){}", stem, i, ext));
                    if !target.exists() {
                        break;
                    }
                    i += 1;
                }
            }
            match std::fs::copy(&src, &target) {
                Ok(_) => {
                    debug!("[NvidiaRecording] 已复制: {} -> {}", raw, target.display());
                    copied.push(raw);
                }
                Err(e) => {
                    warn!("[NvidiaRecording] 复制失败: {} - {}", raw, e);
                    errors.push((raw, format!("复制失败: {e}")));
                }
            }
        }
        NvidiaCopyResult {
            copied,
            errors,
            cancelled: false,
        }
    })
    .await
    .map_err(|e| format!("复制任务异常: {e}"))
    .map(Ok)?
}

/// 永久删除录制视频（前端已有二次确认）。
/// 后端兜底：仅允许删除视频扩展名白名单内的文件，防止误删其它数据。
#[tauri::command]
pub async fn delete_nvidia_recording_video(paths: Vec<String>) -> Result<NvidiaDeleteResult, String> {
    let result = tokio::task::spawn_blocking(move || {
        let mut deleted = Vec::new();
        let mut errors = Vec::new();
        for raw in paths {
            let path = PathBuf::from(&raw);
            if is_video_ext(&path).is_none() {
                errors.push((raw.clone(), "非视频文件，已拒绝删除".to_string()));
                continue;
            }
            match delete_file_force(&path) {
                Ok(()) => {
                    debug!("[NvidiaRecording] 已永久删除: {raw}");
                    deleted.push(raw);
                }
                Err(e) => {
                    warn!("[NvidiaRecording] 删除失败: {raw} - {e}");
                    errors.push((raw, e));
                }
            }
        }
        NvidiaDeleteResult { deleted, errors }
    })
    .await
    .map_err(|e| format!("删除任务异常: {e}"))?;

    info!(
        "[NvidiaRecording] 删除完成: 成功 {}, 失败 {}",
        result.deleted.len(),
        result.errors.len()
    );
    Ok(result)
}

/// 删除单个文件，权限不足时先去除只读属性再删
fn delete_file_force(path: &std::path::Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) => {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                remove_readonly(path)?;
                std::fs::remove_file(path).map_err(|e2| format!("删除失败: {e2}"))
            } else {
                Err(format!("删除失败: {e}"))
            }
        }
    }
}

/// 移除文件的只读属性（Windows）
#[cfg(windows)]
fn remove_readonly(path: &std::path::Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileAttributesW, SetFileAttributesW, FILE_ATTRIBUTE_READONLY,
    };

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let attrs = GetFileAttributesW(wide.as_ptr());
        if attrs == u32::MAX {
            return Err("无法获取文件属性".to_string());
        }
        let new_attrs = attrs & !FILE_ATTRIBUTE_READONLY;
        if new_attrs != attrs && SetFileAttributesW(wide.as_ptr(), new_attrs) == 0 {
            return Err("无法移除只读属性".to_string());
        }
    }
    Ok(())
}

/// 非 Windows 兜底（理论上仅用于非目标平台编译）
#[cfg(not(windows))]
fn remove_readonly(path: &std::path::Path) -> Result<(), String> {
    let mut perms = std::fs::metadata(path)
        .map_err(|e| format!("读取文件属性失败: {e}"))?
        .permissions();
    perms.set_readonly(false);
    std::fs::set_permissions(path, perms).map_err(|e| format!("移除只读属性失败: {e}"))
}

// ============================================================================
// 视频缩略图
//
// 通过 Windows Shell (IShellItemImageFactory) 生成与资源管理器一致的视频缩略图，
// 结果 PNG 缓存到 app_data_dir/nvidia_thumbnails/，前端用 convertFileSrc 显示。
// ============================================================================

/// 缩略图缓存目录名（位于 app_data_dir 下）
const THUMB_DIR_NAME: &str = "nvidia_thumbnails";
/// 缩略图边长上限
const THUMB_MAX_EDGE: i32 = 320;

/// 为指定视频批量生成缩略图（带磁盘缓存）。
/// 返回 path -> 缓存图片绝对路径；生成失败的项为 None（前端回退到图标占位）。
#[tauri::command]
pub async fn get_video_thumbnails(
    app: tauri::AppHandle,
    paths: Vec<String>,
) -> Result<std::collections::HashMap<String, Option<String>>, String> {
    use tauri::Manager;
    if paths.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let cache_dir = app
        .path()
        .app_data_dir()
        .map(|d| d.join(THUMB_DIR_NAME))
        .map_err(|e| format!("获取缓存目录失败: {e}"))?;
    tokio::task::spawn_blocking(move || build_thumbnails(&cache_dir, &paths))
        .await
        .map_err(|e| format!("缩略图任务异常: {e}"))?
}

/// 多线程生成缩略图（并发上限 8），返回 path -> 缩略图路径映射
fn build_thumbnails(
    cache_dir: &std::path::Path,
    paths: &[String],
) -> Result<std::collections::HashMap<String, Option<String>>, String> {
    if let Err(e) = std::fs::create_dir_all(cache_dir) {
        return Err(format!("创建缩略图缓存目录失败: {e}"));
    }

    let all: Arc<Vec<(String, PathBuf)>> = Arc::new(
        paths
            .iter()
            .map(|p| (p.clone(), thumbnail_cache_path(cache_dir, p)))
            .collect(),
    );
    let index = Arc::new(AtomicUsize::new(0));
    let results: Arc<Mutex<std::collections::HashMap<String, Option<String>>>> =
        Arc::new(Mutex::new(std::collections::HashMap::new()));

    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(8)
        .min(paths.len())
        .max(1);

    std::thread::scope(|scope| {
        for _ in 0..workers {
            let all = all.clone();
            let index = index.clone();
            let results = results.clone();
            scope.spawn(move || {
                // Shell 缩略图提供程序多在 STA 下工作，每个工作线程初始化一次 COM
                let _com = ComGuard::new();
                loop {
                    let i = index.fetch_add(1, Ordering::SeqCst);
                    let Some((path, cache)) = all.get(i) else {
                        break;
                    };
                    let thumb = resolve_thumbnail(cache_dir, path, cache);
                    results.lock().unwrap().insert(path.clone(), thumb);
                }
            });
        }
    });

    let out = results.lock().unwrap().clone();
    Ok(out)
}

/// 缓存文件路径：按规范化路径哈希命名
fn thumbnail_cache_path(cache_dir: &std::path::Path, src: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    normalize_path(src).hash(&mut hasher);
    cache_dir.join(format!("{:016x}.png", hasher.finish()))
}

/// 命中缓存则直接返回；否则生成，失败返回 None
fn resolve_thumbnail(
    cache_dir: &std::path::Path,
    src: &str,
    cache: &std::path::Path,
) -> Option<String> {
    if cache.exists() {
        return Some(cache.to_string_lossy().into_owned());
    }
    let src_path = std::path::Path::new(src);
    if !src_path.is_file() || is_video_ext(src_path).is_none() {
        return None;
    }
    match generate_thumbnail(cache_dir, src, cache) {
        Ok(()) => Some(cache.to_string_lossy().into_owned()),
        Err(e) => {
            debug!("[NvidiaRecording] 缩略图生成失败 {src}: {e}");
            None
        }
    }
}

/// 生成单个视频缩略图（IShellItemImageFactory -> HBITMAP -> GetDIBits -> PNG）
#[cfg(windows)]
fn generate_thumbnail(
    _cache_dir: &std::path::Path,
    src: &str,
    dst: &std::path::Path,
) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::{Interface, PCWSTR};
    use windows::Win32::Foundation::{HWND, SIZE};
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDIBits, GetDC, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::UI::Shell::{
        IShellItem, IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK,
        SIIGBF_THUMBNAILONLY,
    };

    // 1) 路径 -> IShellItem -> IShellItemImageFactory
    let wide: Vec<u16> = std::ffi::OsStr::new(src)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let item: IShellItem = unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None) }
        .map_err(|e| format!("创建 ShellItem 失败: {e:?}"))?;
    let factory: IShellItemImageFactory = item
        .cast()
        .map_err(|e| format!("获取缩略图工厂失败: {e:?}"))?;
    let hbm = unsafe {
        factory.GetImage(
            SIZE {
                cx: THUMB_MAX_EDGE,
                cy: THUMB_MAX_EDGE,
            },
            SIIGBF_THUMBNAILONLY | SIIGBF_BIGGERSIZEOK,
        )
    }
    .map_err(|e| format!("获取缩略图失败: {e:?}"))?;

    // 2) 读取位图实际尺寸
    let mut bmp: BITMAP = unsafe { std::mem::zeroed() };
    let got = unsafe {
        GetObjectW(
            hbm,
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bmp as *mut _ as *mut core::ffi::c_void),
        )
    };
    if got == 0 || bmp.bmWidth <= 0 || bmp.bmHeight <= 0 {
        let _ = unsafe { DeleteObject(hbm) };
        return Err("无法读取位图信息".to_string());
    }
    let w = bmp.bmWidth;
    let h = bmp.bmHeight;

    // 3) GetDIBits 提取 32bpp 顶向下 BGRA 像素
    let dc = unsafe { GetDC(HWND(std::ptr::null_mut())) };
    if dc.is_invalid() {
        let _ = unsafe { DeleteObject(hbm) };
        return Err("获取屏幕 DC 失败".to_string());
    }
    let mut bmi = BITMAPINFO::default();
    bmi.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: w,
        biHeight: -h,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let lines = unsafe {
        GetDIBits(
            dc,
            hbm,
            0,
            h as u32,
            Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
            &mut bmi,
            DIB_RGB_COLORS,
        )
    };
    let _ = unsafe { ReleaseDC(None, dc) };
    let _ = unsafe { DeleteObject(hbm) };
    if lines == 0 {
        return Err("提取位图像素失败".to_string());
    }

    // 4) BGRA -> RGBA 并编码 PNG
    let mut rgba = vec![0u8; buf.len()];
    for (i, px) in buf.chunks_exact(4).enumerate() {
        rgba[i * 4] = px[2];
        rgba[i * 4 + 1] = px[1];
        rgba[i * 4 + 2] = px[0];
        rgba[i * 4 + 3] = 255;
    }
    image::save_buffer(dst, &rgba, w as u32, h as u32, image::ExtendedColorType::Rgba8)
        .map_err(|e| format!("保存缩略图失败: {e}"))?;
    Ok(())
}

/// 非 Windows 平台不支持（仅用于非目标平台编译）
#[cfg(not(windows))]
fn generate_thumbnail(
    _cache_dir: &std::path::Path,
    _src: &str,
    _dst: &std::path::Path,
) -> Result<(), String> {
    Err("当前平台不支持生成缩略图".to_string())
}

/// 工作线程级 COM 环境（STA）
struct ComGuard;

impl ComGuard {
    fn new() -> Self {
        #[cfg(windows)]
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
            );
        }
        Self
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            windows::Win32::System::Com::CoUninitialize();
        }
    }
}