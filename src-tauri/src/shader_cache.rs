use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShaderCacheDir {
    pub name: String,
    pub path: String,
    pub exists: bool,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendorScanResult {
    pub vendor: String,
    pub dirs: Vec<ShaderCacheDir>,
    pub total_dirs: u64,
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub groups: Vec<VendorScanResult>,
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanResult {
    pub success: bool,
    pub message: String,
    pub freed_bytes: u64,
    pub reboot_pending_count: u64,
}

// ── Windows 宽字符转换 ──
fn to_wide(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

// ── 目录大小扫描 ──
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

fn scan_cache_dir(name: &str, path: PathBuf) -> ShaderCacheDir {
    let exists = path.exists() && path.is_dir();
    let size_bytes = if exists { get_dir_size(&path) } else { 0 };
    ShaderCacheDir {
        name: name.to_string(),
        path: path.to_string_lossy().to_string(),
        exists,
        size_bytes,
    }
}

fn get_local_app_data() -> Option<PathBuf> {
    dirs::data_local_dir()
}

fn get_local_low_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join("AppData").join("LocalLow"))
}

// ── 单一数据源：厂商分组与目录清单 ──
// 扫描与清理共用此表，避免两处硬编码不同步。

#[derive(Clone, Copy)]
enum Base {
    Local,
    LocalLow,
}

struct DirDef {
    name: &'static str,
    rel: &'static str,
    base: Base,
}

struct VendorDef {
    vendor: &'static str,
    dirs: &'static [DirDef],
}

const VENDORS: &[VendorDef] = &[
    VendorDef {
        vendor: "nvidia",
        dirs: &[
            DirDef { name: "NVIDIA DXCache", rel: r"NVIDIA\DXCache", base: Base::Local },
            DirDef { name: "NVIDIA GLCache", rel: r"NVIDIA\GLCache", base: Base::Local },
            DirDef { name: "NVIDIA ComputeCache", rel: r"NVIDIA\ComputeCache", base: Base::Local },
            DirDef {
                name: "NVIDIA Corporation NV_Cache",
                rel: r"NVIDIA Corporation\NV_Cache",
                base: Base::Local,
            },
        ],
    },
    VendorDef {
        vendor: "amd",
        dirs: &[
            DirDef { name: "AMD DxCache", rel: r"AMD\DxCache", base: Base::Local },
            DirDef { name: "AMD GLCache", rel: r"AMD\GLCache", base: Base::Local },
            DirDef { name: "AMD VkCache", rel: r"AMD\VkCache", base: Base::Local },
            DirDef { name: "AMD DxcCache", rel: r"AMD\DxcCache", base: Base::Local },
            DirDef { name: "AMD LocalLow DxCache", rel: r"AMD\DxCache", base: Base::LocalLow },
            DirDef { name: "AMD LocalLow GLCache", rel: r"AMD\GLCache", base: Base::LocalLow },
            DirDef { name: "AMD LocalLow VkCache", rel: r"AMD\VkCache", base: Base::LocalLow },
        ],
    },
    VendorDef {
        vendor: "intel",
        dirs: &[DirDef {
            name: "Intel ShaderCache",
            rel: r"Intel\ShaderCache",
            base: Base::Local,
        }],
    },
    VendorDef {
        vendor: "directx",
        dirs: &[
            DirDef { name: "D3DSCache", rel: r"D3DSCache", base: Base::Local },
            DirDef { name: "DirectXShaderCache", rel: r"DirectXShaderCache", base: Base::Local },
        ],
    },
];

/// 将目录定义解析为绝对路径。
fn resolve(def: &DirDef, local: &Path, local_low: &Path) -> PathBuf {
    match def.base {
        Base::Local => local.join(def.rel),
        Base::LocalLow => local_low.join(def.rel),
    }
}

fn build_vendor_result(vendor: &str, dirs: Vec<ShaderCacheDir>) -> VendorScanResult {
    let total_dirs = dirs.iter().filter(|d| d.exists).count() as u64;
    let total_size = dirs.iter().map(|d| d.size_bytes).sum();
    VendorScanResult {
        vendor: vendor.to_string(),
        dirs,
        total_dirs,
        total_size,
    }
}

// ── 强制删除 Windows API 封装 ──
/// 尝试删除单个文件，失败返回 false
fn force_delete_file(path: &Path) -> bool {
    use windows_sys::Win32::Storage::FileSystem::DeleteFileW;
    let wide = to_wide(&path.to_string_lossy());
    unsafe { DeleteFileW(wide.as_ptr()) != 0 }
}

/// 尝试删除空目录
fn force_remove_dir(path: &Path) -> bool {
    use windows_sys::Win32::Storage::FileSystem::RemoveDirectoryW;
    let wide = to_wide(&path.to_string_lossy());
    unsafe { RemoveDirectoryW(wide.as_ptr()) != 0 }
}

/// 将文件/目录标记为重启后删除（MoveFileEx + MOVEFILE_DELAY_UNTIL_REBOOT）
fn schedule_reboot_delete(path: &Path) -> bool {
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_DELAY_UNTIL_REBOOT};
    let wide = to_wide(&path.to_string_lossy());
    unsafe { MoveFileExW(wide.as_ptr(), std::ptr::null(), MOVEFILE_DELAY_UNTIL_REBOOT) != 0 }
}

/// 强制删除目录内容：
/// - 文件：先尝试 DeleteFileW，失败则 MoveFileExW 调度重启删除
/// - 目录：递归处理后 RemoveDirectoryW，仍存在则调度重启删除
/// 返回 (已删除字节数, 调度重启删除的文件数)
fn force_delete_contents(path: &Path, deleted_bytes: &mut u64, reboot_count: &mut u64) {
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                force_delete_contents(&p, deleted_bytes, reboot_count);
                // 尝试删除空子目录
                if p.exists() {
                    force_remove_dir(&p);
                }
                // 如果目录仍然存在，标记重启删除
                if p.exists() {
                    schedule_reboot_delete(&p);
                    *reboot_count += 1;
                }
            } else {
                let file_size = fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                if force_delete_file(&p) {
                    *deleted_bytes += file_size;
                } else {
                    // 文件被锁定，调度重启后删除
                    if schedule_reboot_delete(&p) {
                        *deleted_bytes += file_size;
                        *reboot_count += 1;
                    }
                }
            }
        }
    }
}

/// 强制清理单个缓存目录：
/// 1. 先尝试常规 remove_dir_all（最快）
/// 2. 失败则逐文件强制删除 + MoveFileEx 调度重启删除
/// 返回 (已删除字节数, 重启删除项数)
fn force_clean_one_dir(dir_path: &Path) -> (u64, u64) {
    if !dir_path.exists() {
        return (0, 0);
    }

    let before_size = get_dir_size(dir_path);

    // 第一步：尝试常规删除
    if fs::remove_dir_all(dir_path).is_ok() {
        return (before_size, 0);
    }

    // 第二步：逐文件强制删除 + 调度重启删除
    let mut deleted_bytes: u64 = 0;
    let mut reboot_count: u64 = 0;
    force_delete_contents(dir_path, &mut deleted_bytes, &mut reboot_count);

    // 第三步：尝试删除根目录本身
    if dir_path.exists() {
        force_remove_dir(dir_path);
    }
    if dir_path.exists() {
        schedule_reboot_delete(dir_path);
        reboot_count += 1;
    }

    (deleted_bytes, reboot_count)
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

// ── Tauri 命令 ──

#[tauri::command]
pub async fn scan_shader_caches() -> Result<ScanResult, String> {
    let local_app_data = get_local_app_data().ok_or("无法获取 LocalAppData 目录")?;
    let local_low = get_local_low_dir().ok_or("无法获取 LocalLow 目录")?;

    let groups: Vec<VendorScanResult> = VENDORS
        .iter()
        .map(|v| {
            let dirs = v
                .dirs
                .iter()
                .map(|d| scan_cache_dir(d.name, resolve(d, &local_app_data, &local_low)))
                .collect();
            build_vendor_result(v.vendor, dirs)
        })
        .collect();

    let total_size = groups.iter().map(|g| g.total_size).sum();

    Ok(ScanResult { groups, total_size })
}

#[tauri::command]
pub async fn clean_shader_cache(vendor: String) -> Result<CleanResult, String> {
    let local_app_data = get_local_app_data().ok_or("无法获取 LocalAppData 目录")?;
    let local_low = get_local_low_dir().ok_or("无法获取 LocalLow 目录")?;

    let def = VENDORS
        .iter()
        .find(|v| v.vendor == vendor)
        .ok_or_else(|| format!("不支持的显卡厂商: {}", vendor))?;

    let mut total_freed: u64 = 0;
    let mut total_reboot: u64 = 0;
    let mut cleaned_count = 0;

    for d in def.dirs {
        let dir_path = resolve(d, &local_app_data, &local_low);
        if !dir_path.exists() {
            continue;
        }

        let (freed, reboot) = force_clean_one_dir(&dir_path);
        total_freed += freed;
        total_reboot += reboot;
        cleaned_count += 1;
    }

    if cleaned_count == 0 && total_freed == 0 {
        Ok(CleanResult {
            success: true,
            message: "没有找到需要清理的缓存目录".to_string(),
            freed_bytes: 0,
            reboot_pending_count: 0,
        })
    } else {
        let mut msg = format!(
            "强制清理完成，已处理 {} 个目录，释放 {}",
            cleaned_count,
            format_size(total_freed)
        );
        if total_reboot > 0 {
            msg.push_str(&format!(
                "，{} 个被占用的文件将在重启后删除",
                total_reboot
            ));
        }

        Ok(CleanResult {
            success: true,
            message: msg,
            freed_bytes: total_freed,
            reboot_pending_count: total_reboot,
        })
    }
}
