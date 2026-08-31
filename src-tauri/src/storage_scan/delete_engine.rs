// ============================================================================
// 删除引擎(从 light-c-main 移植)
// 多层安全保护,确保不会误删重要文件
// ============================================================================

use log::{debug, error, info, warn};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use super::safety_constants::{
    is_rebuildable_system_cache_path, PROTECTED_EXTENSIONS_IN_WINDOWS, PROTECTED_FILES,
    PROTECTED_PATH_PREFIXES,
};
use super::DeleteResult;
use super::DeleteTarget;

// ============================================================================
// Windows API 绑定(使用 extern 声明,避免引入额外 winapi feature)
// ============================================================================

#[cfg(windows)]
mod windows_api {
    use std::ptr;

    pub const MOVEFILE_DELAY_UNTIL_REBOOT: u32 = 0x00000004;

    pub const FILE_ATTRIBUTE_READONLY: u32 = 0x00000001;
    pub const FILE_ATTRIBUTE_HIDDEN: u32 = 0x00000002;
    pub const FILE_ATTRIBUTE_SYSTEM: u32 = 0x00000004;

    // SHEmptyRecycleBin 标志
    pub const SHERB_NOCONFIRMATION: u32 = 0x00000001;
    pub const SHERB_NOPROGRESSUI: u32 = 0x00000002;
    pub const SHERB_NOSOUND: u32 = 0x00000004;

    #[link(name = "kernel32")]
    extern "system" {
        /// 标记文件在重启时删除
        pub fn MoveFileExW(
            lpExistingFileName: *const u16,
            lpNewFileName: *const u16,
            dwFlags: u32,
        ) -> i32;
        pub fn GetFileAttributesW(lpFileName: *const u16) -> u32;
        pub fn SetFileAttributesW(lpFileName: *const u16, dwFileAttributes: u32) -> i32;
        pub fn GetLastError() -> u32;
    }

    #[link(name = "shell32")]
    extern "system" {
        /// 清空回收站(Windows Shell API)
        pub fn SHEmptyRecycleBinW(hwnd: *const u16, pszRootPath: *const u16, dwFlags: u32) -> i32;
    }

    pub fn to_wide_string(s: &str) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        std::ffi::OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    /// 标记文件在重启时删除(仅用于已通过安全检查的文件)
    pub fn mark_for_delete_on_reboot(path: &str) -> Result<(), String> {
        let wide_path = to_wide_string(path);
        unsafe {
            let result = MoveFileExW(
                wide_path.as_ptr(),
                ptr::null(), // NULL 表示删除而非移动
                MOVEFILE_DELAY_UNTIL_REBOOT,
            );
            if result != 0 {
                Ok(())
            } else {
                Err(format!(
                    "标记重启删除失败，错误代码: {}",
                    GetLastError()
                ))
            }
        }
    }

    /// 移除文件的只读、隐藏、系统属性
    pub fn remove_protection_attributes(path: &str) -> Result<(), String> {
        let wide_path = to_wide_string(path);
        unsafe {
            let attrs = GetFileAttributesW(wide_path.as_ptr());
            if attrs == u32::MAX {
                return Err("无法获取文件属性".to_string());
            }
            let new_attrs =
                attrs & !(FILE_ATTRIBUTE_READONLY | FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM);
            if new_attrs != attrs {
                let result = SetFileAttributesW(wide_path.as_ptr(), new_attrs);
                if result == 0 {
                    return Err("无法修改文件属性".to_string());
                }
            }
        }
        Ok(())
    }

    /// 使用 Windows Shell API 清空指定驱动器的回收站。
    ///
    /// 直接删除 C:\$Recycle.Bin 下的文件会被系统拒绝(需要 SYSTEM 权限),
    /// 也会留下元数据残留。而 SHEmptyRecycleBinW 走的是 Shell 标准流程。
    pub fn empty_recycle_bin(drive_root: Option<&str>) -> Result<(), String> {
        let root_wide: Vec<u16>;
        let root_ptr: *const u16;

        if let Some(root) = drive_root {
            root_wide = to_wide_string(root);
            root_ptr = root_wide.as_ptr();
        } else {
            root_ptr = std::ptr::null();
        }

        let flags = SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND;

        // HRESULT 白名单:
        // S_OK (0) — 清空成功
        // E_INVALIDARG (0x80070057) — 回收站原本就为空,也视为成功
        const S_OK: i32 = 0;
        const E_INVALIDARG: i32 = -2147024809i32;

        unsafe {
            let hresult = SHEmptyRecycleBinW(std::ptr::null(), root_ptr, flags);
            if hresult == S_OK || hresult == E_INVALIDARG {
                Ok(())
            } else {
                Err(format!("清空回收站失败，HRESULT: 0x{:08X}", hresult as u32))
            }
        }
    }
}

/// 清空所有磁盘的回收站(复用 Shell 标准流程,不依赖 PowerShell)。
#[cfg(windows)]
pub fn empty_all_recycle_bins() -> Result<(), String> {
    windows_api::empty_recycle_bin(None)
}

#[cfg(not(windows))]
pub fn empty_all_recycle_bins() -> Result<(), String> {
    Err("清空回收站仅支持 Windows".to_string())
}

/// 删除引擎
pub struct DeleteEngine {
    /// 是否跳过正在使用的文件
    skip_in_use: bool,
}

impl DeleteEngine {
    /// 创建新的删除引擎
    pub fn new() -> Self {
        DeleteEngine {
            skip_in_use: true, // 默认跳过正在使用的文件
        }
    }

    /// 删除指定路径列表
    pub fn delete_paths(&self, targets: &[DeleteTarget]) -> DeleteResult {
        let mut result = DeleteResult::new();
        if targets.is_empty() {
            return result;
        }

        info!("开始删除 {} 个路径", targets.len());

        // 分离回收站路径:回收站文件应通过 Shell API 清空,而非逐文件删除
        // 直接删除 $Recycle.Bin 下的文件需要 SYSTEM 权限,SHEmptyRecycleBinW 是标准方式
        let (recycle_paths, normal_paths): (Vec<&DeleteTarget>, Vec<&DeleteTarget>) = targets
            .iter()
            .partition(|t| t.path.to_lowercase().contains("\\$recycle.bin"));

        if !recycle_paths.is_empty() {
            self.delete_recycle_paths(&recycle_paths, &mut result);
        }

        for target in normal_paths {
            let file_path = Path::new(&target.path);
            // 优先复用扫描阶段已知的大小,避免删除阶段对每个文件再 stat 一次
            let size = target.size.unwrap_or_else(|| self.get_path_size(file_path));

            match self.delete_single_path(file_path, size) {
                Ok((freed, marked_for_reboot)) => {
                    if marked_for_reboot {
                        result.add_reboot_pending(freed);
                        debug!("已标记重启删除: {}", target.path);
                    } else {
                        result.add_success(freed);
                        debug!("成功删除: {}", target.path);
                    }
                }
                Err(e) => {
                    result.add_failure(target.path.clone(), e);
                    warn!("删除失败: {}", target.path);
                }
            }
        }

        info!(
            "删除完成: 成功 {} 个, 失败 {} 个, 待重启 {} 个, 释放空间 {} 字节",
            result.success_count,
            result.failed_count,
            result.reboot_pending_count,
            result.freed_size
        );

        result
    }

    /// 回收站路径按盘符调用 Shell API 清空
    fn delete_recycle_paths(&self, targets: &[&DeleteTarget], result: &mut DeleteResult) {
        #[cfg(windows)]
        {
            info!("检测到 {} 个回收站条目，按盘符调用 Shell API 清空", targets.len());

            // 必须在 Shell API 运行前读取大小,否则成功清空后路径已不存在,只能得到 0 字节。
            let mut recycle_by_drive: BTreeMap<String, Vec<(String, u64)>> = BTreeMap::new();
            for target in targets {
                let file_path = Path::new(&target.path);
                let logical_size = target.size.unwrap_or_else(|| self.get_path_size(file_path));
                let Some(drive_root) = recycle_drive_root(&target.path) else {
                    result.add_failure(target.path.clone(), "回收站路径缺少有效盘符".to_string());
                    continue;
                };
                recycle_by_drive
                    .entry(drive_root)
                    .or_default()
                    .push((target.path.clone(), logical_size));
            }

            for (drive_root, entries) in recycle_by_drive {
                match windows_api::empty_recycle_bin(Some(&drive_root)) {
                    Ok(_) => {
                        info!("Shell API 清空回收站成功: {}", drive_root);
                        for (_path, logical_size) in entries {
                            result.add_success(logical_size);
                        }
                    }
                    Err(error) => {
                        warn!("Shell API 清空回收站失败 ({}): {}", drive_root, error);
                        for (path, logical_size) in entries {
                            let _ = logical_size;
                            result.add_failure(path, format!("清空回收站失败 ({}): {}", drive_root, error));
                        }
                    }
                }
            }
        }

        #[cfg(not(windows))]
        {
            let _ = (paths, result);
        }
    }

    /// 删除单个文件或目录(多层安全检查)
    /// 返回 (释放大小, 是否标记为重启删除)
    fn delete_single_path(&self, path: &Path, size: u64) -> Result<(u64, bool), String> {
        // 检查路径是否存在
        if !path.exists() {
            return Err("文件不存在".to_string());
        }

        // 安全检查第1层:检查是否为受保护路径
        if self.is_protected_path(path) {
            return Err("系统保护路径，禁止删除".to_string());
        }

        // 尝试删除
        if path.is_dir() {
            self.delete_directory(path, size)
        } else {
            self.delete_file(path, size)
        }
    }

    /// 删除文件,返回 (大小, 是否标记为重启删除)
    fn delete_file(&self, path: &Path, size: u64) -> Result<(u64, bool), String> {
        match fs::remove_file(path) {
            Ok(_) => Ok((size, false)),
            Err(e) => {
                // 尝试移除只读属性后再删除
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    #[cfg(windows)]
                    {
                        let path_str = path.to_string_lossy();
                        if windows_api::remove_protection_attributes(&path_str).is_ok() {
                            if fs::remove_file(path).is_ok() {
                                return Ok((size, false));
                            }
                        }
                    }
                    #[cfg(not(windows))]
                    {
                        let _ = e;
                        if let Ok(metadata) = fs::metadata(path) {
                            let mut permissions = metadata.permissions();
                            permissions.set_readonly(false);
                            if fs::set_permissions(path, permissions).is_ok() {
                                if fs::remove_file(path).is_ok() {
                                    return Ok((size, false));
                                }
                            }
                        }
                    }
                    Err(format!("权限不足: {}", e))
                } else {
                    // 检测共享冲突(错误码 32, ERROR_SHARING_VIOLATION),
                    // 文件正被其他进程使用时无法直接删除,标记为重启后删除
                    #[cfg(windows)]
                    let is_sharing_violation = e.raw_os_error() == Some(32);
                    #[cfg(not(windows))]
                    let is_sharing_violation = false;

                    if is_sharing_violation && self.skip_in_use {
                        #[cfg(windows)]
                        {
                            let path_str = path.to_string_lossy();
                            match windows_api::mark_for_delete_on_reboot(&path_str) {
                                Ok(_) => {
                                    info!("文件已标记为重启删除: {}", path_str);
                                    return Ok((size, true));
                                }
                                Err(mark_err) => {
                                    warn!("标记重启删除失败: {} - {}", path_str, mark_err);
                                }
                            }
                        }
                        Err(format!("文件被系统占用: {}", e))
                    } else {
                        Err(format!("删除失败: {}", e))
                    }
                }
            }
        }
    }

    /// 删除目录,返回 (大小, 是否标记为重启删除)
    fn delete_directory(&self, path: &Path, size: u64) -> Result<(u64, bool), String> {
        match fs::remove_dir_all(path) {
            Ok(_) => Ok((size, false)),
            Err(e) => {
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    Err(format!("权限不足: {}", e))
                } else {
                    Err(format!("删除目录失败: {}", e))
                }
            }
        }
    }

    /// 获取路径大小
    fn get_path_size(&self, path: &Path) -> u64 {
        if path.is_file() {
            fs::metadata(path).map(|m| m.len()).unwrap_or(0)
        } else if path.is_dir() {
            walkdir::WalkDir::new(path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
                .filter_map(|e| e.metadata().ok())
                .map(|m| m.len())
                .sum()
        } else {
            0
        }
    }

    /// 检查是否为受保护的路径(多层安全检查)
    fn is_protected_path(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy().to_lowercase();

        // 第1层:检查路径前缀
        for protected in PROTECTED_PATH_PREFIXES {
            if path_str.starts_with(protected) && !is_rebuildable_system_cache_path(&path_str) {
                error!("安全拦截: 尝试删除受保护路径 {}", path_str);
                return true;
            }
        }

        // 第2层:检查文件名
        if let Some(file_name) = path.file_name() {
            let name = file_name.to_string_lossy().to_lowercase();
            for protected in PROTECTED_FILES {
                if name == *protected {
                    error!("安全拦截: 尝试删除系统关键文件 {}", name);
                    return true;
                }
            }
        }

        // 第3层:在Windows目录下保护特定扩展名
        if path_str.contains("\\windows\\") {
            if let Some(ext) = path.extension() {
                let ext_str = ext.to_string_lossy().to_lowercase();
                if PROTECTED_EXTENSIONS_IN_WINDOWS.contains(&ext_str.as_str()) {
                    error!("安全拦截: 尝试删除Windows目录下的系统文件 {}", path_str);
                    return true;
                }
            }
        }

        // 第4层:检查是否是用户配置文件夹的根目录
        let user_critical_paths = [
            "\\appdata\\local",
            "\\appdata\\roaming",
            "\\documents",
            "\\desktop",
            "\\downloads",
        ];
        for critical in &user_critical_paths {
            // 只保护根目录,不保护子目录
            if path_str.ends_with(critical) {
                error!("安全拦截: 尝试删除用户关键目录 {}", path_str);
                return true;
            }
        }

        // 第5层:检查是否是驱动器根目录
        if path_str.len() <= 3 && path_str.ends_with("\\") {
            error!("安全拦截: 尝试删除驱动器根目录 {}", path_str);
            return true;
        }

        false
    }
}

impl Default for DeleteEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// 从回收站数据路径提取 Shell API 所需的驱动器根路径。
fn recycle_drive_root(path: &str) -> Option<String> {
    let bytes = path.as_bytes();
    if bytes.len() < 2 || bytes[1] != b':' || !bytes[0].is_ascii_alphabetic() {
        return None;
    }
    Some(format!("{}:\\", (bytes[0] as char).to_ascii_uppercase()))
}
