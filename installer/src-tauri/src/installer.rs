use mslnk::ShellLink;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStrExt;
use winreg::enums::*;
use winreg::RegKey;

const CREATE_NO_WINDOW: u32 = 0x08000000;

/// 标记是否为更新安装（非首次安装）
static IS_UPDATE_INSTALL: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "windows")]
extern "system" {
    fn GetDiskFreeSpaceExW(
        lpDirectoryName: *const u16,
        lpFreeBytesAvailableToCaller: *mut u64,
        lpTotalNumberOfBytes: *mut u64,
        lpTotalNumberOfFreeBytes: *mut u64,
    ) -> i32;
}

/// UTF-16LE + Base64 编码 PowerShell 脚本
/// 这是 Windows 上传递含非 ASCII 字符脚本的唯一可靠方式。
/// `-EncodedCommand` 参数支持 UTF-16LE Base64，完全绕过系统代码页问题。
fn encode_ps_command(script: &str) -> String {
    let utf16: Vec<u8> = script
        .encode_utf16()
        .flat_map(|c| c.to_le_bytes())
        .collect();
    base64_encode(&utf16)
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0] as u32, *chunk.get(1).unwrap_or(&0) as u32, *chunk.get(2).unwrap_or(&0) as u32];
        let n = (b[0] << 16) | (b[1] << 8) | b[2];
        out.push(CHARS[((n >> 18) & 0x3F) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 { CHARS[((n >> 6) & 0x3F) as usize] } else { b'=' } as char);
        out.push(if chunk.len() > 2 { CHARS[(n & 0x3F) as usize] } else { b'=' } as char);
    }
    out
}

/// 通过 Base64 编码执行 PowerShell 脚本，避免系统代码页导致的乱码。
/// 脚本以 UTF-8 传入，内部自动转为 UTF-16LE Base64。
/// stdout 通过 `[Console]::OutputEncoding` 强制为 UTF-8 返回。
fn run_powershell(script: &str) -> Result<String, String> {
    let full_script = format!(
        "[Console]::OutputEncoding = [Text.Encoding]::UTF8; {}",
        script
    );
    let encoded = encode_ps_command(&full_script);

    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("无法执行 PowerShell: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("PowerShell 执行失败: {}", stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[derive(Serialize)]
pub struct FileEntry {
    relative_path: String,
    size: u64,
}

/// Embedded payload ZIP (created by build.ps1 staging step)
const PAYLOAD_ZIP: &[u8] = include_bytes!("../payload.zip");

/// 从注册表读取已安装 NexBox 的路径（用于更新场景）
fn get_existing_install_path() -> Option<String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let uninstall_key_path = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\NexBox";

    // 检查 64 位注册表视图
    if let Ok(key) = hklm.open_subkey_with_flags(uninstall_key_path, KEY_READ) {
        if let Ok(install_location) = key.get_value::<String, _>("InstallLocation") {
            let path = Path::new(&install_location);
            if path.exists() && path.join("nexbox.exe").exists() {
                return Some(install_location);
            }
        }
    }

    // 检查 32 位注册表视图 (WOW6432Node)，兼容旧版安装
    let wow64_path = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\NexBox";
    if let Ok(key) = hklm.open_subkey_with_flags(wow64_path, KEY_READ) {
        if let Ok(install_location) = key.get_value::<String, _>("InstallLocation") {
            let path = Path::new(&install_location);
            if path.exists() && path.join("nexbox.exe").exists() {
                return Some(install_location);
            }
        }
    }

    None
}

/// 检测电脑上是否已安装 NexBox（注册表存在且 nexbox.exe 有效）
#[tauri::command]
pub fn is_existing_install() -> bool {
    get_existing_install_path().is_some()
}

#[tauri::command]
pub fn get_default_install_path() -> String {
    // 更新场景：优先使用注册表中已记录的安装目录
    if let Some(existing_path) = get_existing_install_path() {
        IS_UPDATE_INSTALL.store(true, Ordering::SeqCst);
        return existing_path;
    }
    // 全新安装：默认 Program Files\NexBox
    let program_files = std::env::var("ProgramFiles")
        .unwrap_or_else(|_| "C:\\Program Files".to_string());
    Path::new(&program_files)
        .join("NexBox")
        .display()
        .to_string()
}

/// 安装完成后调度安装程序自删除（仅更新场景）
/// 创建后台 PowerShell 脚本，完全无窗口，等待安装程序退出后删除其自身文件。
#[tauri::command]
pub fn schedule_installer_cleanup() -> Result<(), String> {
    // 首次安装不删除安装程序
    if !IS_UPDATE_INSTALL.load(Ordering::SeqCst) {
        return Ok(());
    }

    let exe_path = std::env::current_exe()
        .map_err(|e| format!("无法获取安装程序路径: {}", e))?;

    let temp_dir = std::env::temp_dir();
    let script_path = temp_dir.join("nxb_cleanup.ps1");
    let exe_str = exe_path.display().to_string();

    // PowerShell 脚本：循环等待安装程序退出，删除安装程序，最后自删除
    // 使用单引号包裹路径以处理空格/特殊字符
    let ps_script = format!(
        "$exe = '{}'\r\n\
         while (Test-Path -LiteralPath $exe) {{\r\n\
             try {{\r\n\
                 Remove-Item -Force -LiteralPath $exe -ErrorAction Stop\r\n\
             }} catch {{\r\n\
                 Start-Sleep -Seconds 2\r\n\
             }}\r\n\
         }}\r\n\
         Remove-Item -Force -LiteralPath $MyInvocation.MyCommand.Path -ErrorAction SilentlyContinue\r\n",
        exe_str.replace('\'', "''") // 转义 PowerShell 单引号
    );

    fs::write(&script_path, ps_script)
        .map_err(|e| format!("无法创建清理脚本: {}", e))?;

    // 完全隐藏启动：PowerShell + WindowStyle Hidden + NoProfile + Bypass 执行策略
    Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle", "Hidden",
            "-ExecutionPolicy", "Bypass",
            "-File",
            script_path.to_str().unwrap(),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("无法启动清理脚本: {}", e))?;

    Ok(())
}

#[tauri::command]
pub fn check_disk_space(path: String) -> Result<u64, String> {
    let path = Path::new(&path);
    let available = fs2_available_space(path).map_err(|e| format!("无法检查磁盘空间: {}", e))?;
    Ok(available)
}

#[tauri::command]
pub fn get_app_version() -> String {
    env!("NEXBOX_APP_VERSION").to_string()
}

#[tauri::command]
pub fn get_resource_files() -> Result<Vec<FileEntry>, String> {
    let cursor = std::io::Cursor::new(PAYLOAD_ZIP);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("无法读取安装包: {}", e))?;

    let mut files = Vec::new();
    for i in 0..archive.len() {
        let file = archive.by_index(i).map_err(|e| format!("读取文件失败: {}", e))?;
        if !file.name().ends_with('/') {
            files.push(FileEntry {
                relative_path: file.name().to_string(),
                size: file.size(),
            });
        }
    }
    Ok(files)
}

/// 强制结束正在运行的 NexBox 本体（nexbox.exe）及其子进程。
/// 若本体正在运行，其 exe/dll 文件被占用会导致解压失败、安装卡住，
/// 因此安装开始前先强制结束本体，并等待其完全退出、文件句柄释放。
fn kill_running_app() {
    // taskkill /T 会连带结束本体的子进程树（如 NexBoxMonitor.exe）
    let _ = Command::new("taskkill")
        .args(["/F", "/T", "/IM", "nexbox.exe"])
        .creation_flags(CREATE_NO_WINDOW)
        .status();

    // 兜底清理可能残留的监控子进程，避免其独占文件
    let _ = Command::new("taskkill")
        .args(["/F", "/IM", "NexBoxMonitor.exe"])
        .creation_flags(CREATE_NO_WINDOW)
        .status();

    // 等待进程完全退出、文件句柄释放
    std::thread::sleep(std::time::Duration::from_millis(500));
}

#[tauri::command]
pub fn install(
    target_dir: String,
    create_desktop_shortcut: bool,
) -> Result<(), String> {
    let target = PathBuf::from(&target_dir);

    // 若本体正在运行则强制结束，避免文件被占用导致安装失败/卡住
    kill_running_app();

    // Cleanup old Inno Setup artifacts before installing
    cleanup_old_innosetup(&target);

    // Create target directory
    fs::create_dir_all(&target)
        .map_err(|e| format!("无法创建目标目录: {}", e))?;

    // Extract payload ZIP to target directory
    extract_payload(&target)?;

    // Register uninstaller first (before shortcuts, so registry is always written)
    register_uninstall(&target_dir, env!("NEXBOX_APP_VERSION"))
        .map_err(|e| format!("无法注册卸载信息: {}", e))?;

    // Remove any existing shortcuts to avoid duplicates from old version
    delete_existing_shortcuts("新境盒");

    // Create Start Menu shortcut (non-fatal — installation succeeds regardless)
    let exe_path = target.join("nexbox.exe");
    if let Err(e) = create_lnk_shortcut("新境盒", &exe_path, "StartMenu") {
        eprintln!("创建开始菜单快捷方式失败 (非致命): {}", e);
    }

    // Create Desktop shortcut if requested (non-fatal)
    if create_desktop_shortcut {
        if let Err(e) = create_lnk_shortcut("新境盒", &exe_path, "Desktop") {
            eprintln!("创建桌面快捷方式失败 (非致命): {}", e);
        }
    }

    Ok(())
}

#[tauri::command]
pub fn cancel_install(target_dir: String) -> Result<(), String> {
    let path = Path::new(&target_dir);
    if path.exists() {
        fs::remove_dir_all(path)
            .map_err(|e| format!("无法清理安装目录: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
pub fn launch_installed_app(target_dir: String) -> Result<(), String> {
    let exe_path = Path::new(&target_dir).join("nexbox.exe");
    if !exe_path.exists() {
        return Err("未找到 nexbox.exe".to_string());
    }

    Command::new(&exe_path)
        .spawn()
        .map_err(|e| format!("无法启动应用: {}", e))?;

    Ok(())
}

// === Payload extraction ===

fn extract_payload(target_dir: &Path) -> Result<(), String> {
    let cursor = std::io::Cursor::new(PAYLOAD_ZIP);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| format!("无法读取安装包: {}", e))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("读取文件失败: {}", e))?;

        let name = file.name().to_string();
        let outpath = target_dir.join(&name);

        if name.ends_with('/') {
            fs::create_dir_all(&outpath)
                .map_err(|e| format!("无法创建目录 {}: {}", outpath.display(), e))?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("无法创建目录 {}: {}", parent.display(), e))?;
            }
            let mut outfile = fs::File::create(&outpath)
                .map_err(|e| format!("无法创建文件 {}: {}", outpath.display(), e))?;
            std::io::copy(&mut file, &mut outfile)
                .map_err(|e| format!("无法写入文件 {}: {}", outpath.display(), e))?;
        }
    }

    Ok(())
}

// === Shortcut creation via native Windows IShellLink API ===

fn create_lnk_shortcut(name: &str, target_exe: &Path, location: &str) -> Result<(), String> {
    let folder = get_special_folder_path(location)
        .ok_or_else(|| "无法获取系统目录路径".to_string())?;

    let shortcut_path = Path::new(&folder).join(format!("{}.lnk", name));
    let workdir = target_exe.parent().unwrap_or(Path::new(""));

    let mut sl = ShellLink::new(target_exe)
        .map_err(|e| format!("创建快捷方式失败: {}", e))?;
    sl.set_working_dir(Some(workdir.to_string_lossy().to_string()));
    sl.create_lnk(&shortcut_path)
        .map_err(|e| format!("保存快捷方式失败: {}", e))?;

    Ok(())
}

/// Get system folder paths without spawning PowerShell.
/// Uses dirs crate for Desktop, SHGetFolderPathW for Common Programs.
fn get_special_folder_path(folder: &str) -> Option<String> {
    if folder == "Desktop" {
        dirs::desktop_dir().map(|p| p.to_string_lossy().to_string())
    } else {
        get_common_programs_path()
    }
}

#[cfg(target_os = "windows")]
fn get_common_programs_path() -> Option<String> {
    use std::os::windows::ffi::OsStringExt;

    extern "system" {
        fn SHGetFolderPathW(
            hwnd: *mut std::ffi::c_void,
            csidl: i32,
            h_token: *mut std::ffi::c_void,
            dw_flags: u32,
            psz_path: *mut u16,
        ) -> i32;
    }

    const CSIDL_COMMON_PROGRAMS: i32 = 0x0017;
    let mut buf = vec![0u16; 260]; // MAX_PATH

    unsafe {
        let result = SHGetFolderPathW(
            std::ptr::null_mut(),
            CSIDL_COMMON_PROGRAMS,
            std::ptr::null_mut(),
            0,
            buf.as_mut_ptr(),
        );
        if result == 0 {
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            return Some(std::ffi::OsString::from_wide(&buf[..len]).to_string_lossy().to_string());
        }
    }
    None
}

// === Registry operations ===

fn register_uninstall(install_dir: &str, version: &str) -> Result<(), String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let uninstall_path = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\NexBox";

    let (key, _) = hklm
        .create_subkey(uninstall_path)
        .map_err(|e| format!("无法创建注册表键: {}", e))?;

    let icon_path = format!("{}\\nexbox.exe", install_dir);
    let uninstaller_path = format!("{}\\Uninstnexbox.exe", install_dir);

    key.set_value("DisplayName", &"新境盒")
        .map_err(|e| format!("写入注册表失败: {}", e))?;
    key.set_value("DisplayVersion", &version)
        .map_err(|e| format!("写入注册表失败: {}", e))?;
    key.set_value("DisplayIcon", &icon_path)
        .map_err(|e| format!("写入注册表失败: {}", e))?;
    key.set_value("Publisher", &"MuLiu")
        .map_err(|e| format!("写入注册表失败: {}", e))?;
    key.set_value("InstallLocation", &install_dir)
        .map_err(|e| format!("写入注册表失败: {}", e))?;
    key.set_value("UninstallString", &uninstaller_path)
        .map_err(|e| format!("写入注册表失败: {}", e))?;
    key.set_value("NoModify", &1u32)
        .map_err(|e| format!("写入注册表失败: {}", e))?;
    key.set_value("NoRepair", &1u32)
        .map_err(|e| format!("写入注册表失败: {}", e))?;
    key.set_value("EstimatedSize", &250_000u32)
        .map_err(|e| format!("写入注册表失败: {}", e))?;
    key.set_value("URLInfoAbout", &"https://www.nexbox.top/")
        .map_err(|e| format!("写入注册表失败: {}", e))?;

    Ok(())
}

// === Disk space check ===

/// Remove old Inno Setup uninstaller files (unins000.exe / unins000.dat)
/// and clean up leftover Inno Setup registry entries from the old installer
fn cleanup_old_innosetup(target: &Path) {
    // 1. Delete old Inno uninstaller files
    for name in &["unins000.exe", "unins000.dat"] {
        let path = target.join(name);
        if path.exists() {
            let _ = fs::remove_file(&path);
        }
    }

    // 2. Delete old Inno Setup registry entries
    cleanup_old_innosetup_registry();
}

/// Scan and remove old Inno Setup uninstall registry entries.
/// Inno Setup convention: {AppId}_is1 under HKLM\...\Uninstall
/// These leftover entries cause "ghost" entries in Apps & Features.
fn cleanup_old_innosetup_registry() {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    // Check both 64-bit and 32-bit (WOW6432Node) registry views
    let hive_paths = [
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    ];

    for hive in &hive_paths {
        if let Ok(uninstall_key) = hklm.open_subkey_with_flags(hive, KEY_ALL_ACCESS) {
            // Collect INNO _is1 keys whose DisplayName matches NexBox
            let to_delete: Vec<String> = uninstall_key
                .enum_keys()
                .filter_map(|k| k.ok())
                .filter(|k| k.ends_with("_is1"))
                .filter(|key_name| {
                    if let Ok(subkey) =
                        uninstall_key.open_subkey_with_flags(key_name, KEY_READ)
                    {
                        if let Ok(name) = subkey.get_value::<String, _>("DisplayName") {
                            return name.contains("新境盒")
                                || name.to_lowercase().contains("nexbox");
                        }
                    }
                    false
                })
                .collect();

            for key_name in &to_delete {
                let _ = uninstall_key.delete_subkey_all(key_name);
            }
        }
    }
}

/// Delete existing shortcuts before creating new ones to avoid duplicates
fn delete_existing_shortcuts(name: &str) {
    // Delete from Desktop
    if let Some(desktop) = get_special_folder_path("Desktop") {
        let path = format!("{}\\{}.lnk", desktop, name);
        if Path::new(&path).exists() {
            let _ = fs::remove_file(&path);
        }
    }
    // Delete from Start Menu
    if let Some(start_menu) = get_special_folder_path("StartMenu") {
        let path = format!("{}\\{}.lnk", start_menu, name);
        if Path::new(&path).exists() {
            let _ = fs::remove_file(&path);
        }
    }
}

fn fs2_available_space(path: &Path) -> Result<u64, std::io::Error> {
    let path_str = path.display().to_string();
    // 提取盘符（从路径开头取第一个字符 + ':'）
    let drive_letter = path_str
        .chars()
        .next()
        .filter(|c| c.is_ascii_alphabetic())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "无效的路径"))?;

    // 方案1: Win32 API GetDiskFreeSpaceExW —— 不依赖 PowerShell，不受安全软件/执行策略影响
    #[cfg(target_os = "windows")]
    {
        let root_path = format!("{}:\\", drive_letter);
        let wide_path: Vec<u16> = std::ffi::OsStr::new(&root_path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut free_bytes: u64 = 0;
        let mut total_bytes: u64 = 0;
        let mut total_free_bytes: u64 = 0;

        unsafe {
            let result = GetDiskFreeSpaceExW(
                wide_path.as_ptr(),
                &mut free_bytes as *mut u64,
                &mut total_bytes as *mut u64,
                &mut total_free_bytes as *mut u64,
            );
            if result != 0 {
                return Ok(free_bytes);
            }
            // API 调用失败，记录错误信息供排查
            let api_error = std::io::Error::last_os_error();
            eprintln!(
                "GetDiskFreeSpaceExW({}) 失败: {}，回退到 PowerShell",
                root_path, api_error
            );
        }
    }

    // 方案2: PowerShell Get-CimInstance Win32_LogicalDisk（降级方案）
    let ps_cmd = format!(
        "(Get-CimInstance Win32_LogicalDisk -Filter \"DeviceID='{}:'\").FreeSpace",
        drive_letter
    );
    match run_powershell(&ps_cmd) {
        Ok(stdout) => {
            // 清理可能的 BOM 字符（U+FEFF）
            let cleaned = stdout.trim().trim_start_matches('\u{feff}');
            if let Ok(bytes) = cleaned.parse::<u64>() {
                return Ok(bytes);
            }
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!(
                    "无法解析磁盘空间数值 (盘符 {}:): '{}'",
                    drive_letter,
                    stdout.trim()
                ),
            ))
        }
        Err(e) => Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("无法获取磁盘空间 (盘符 {}:): {}", drive_letter, e),
        )),
    }
}
