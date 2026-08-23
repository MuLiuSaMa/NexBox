use std::path::PathBuf;
use std::process::Command;

const APP_NAME: &str = "NexBox";
const TASK_NAME: &str = "NexBox";

/// 执行命令（隐藏窗口）
#[cfg(windows)]
fn exec_hidden(cmd: &str, args: &[&str]) -> Result<std::process::Output, String> {
    use std::os::windows::process::CommandExt;
    Command::new(cmd)
        .args(args)
        .creation_flags(0x08000000)
        .output()
        .map_err(|e| format!("执行 {} 失败: {}", cmd, e))
}

/// 获取用户启动文件夹路径
#[cfg(windows)]
fn get_startup_folder() -> Result<PathBuf, String> {
    dirs::config_dir()
        .map(|p| {
            p.join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("Startup")
        })
        .ok_or("无法获取启动文件夹路径".to_string())
}

// ========== 主方案：任务计划程序（onlogon + /rl highest，管理员静默启动） ==========

/// 创建任务计划：用户登录时以最高权限启动 NexBox（不弹 UAC）
/// schtasks /create /tn "NexBox" /tr "\"exe\" [--autostart]" /sc onlogon /rl highest /f
/// 必须指定 /rl highest：NexBox 的 manifest 是 requireAdministrator，
/// 计划任务以最高权限静默启动，避免开机弹 UAC 导致不自启。
/// minimized_start 为 true 时追加 --autostart，主程序据此隐藏窗口静默启动。
#[cfg(windows)]
fn create_scheduled_task(exe_path: &str, minimized_start: bool) -> Result<(), String> {
    let run_cmd = if minimized_start {
        format!("\"{}\" --autostart", exe_path)
    } else {
        format!("\"{}\"", exe_path)
    };

    // /delay 增加约 5 秒启动延迟：登录瞬间桌面合成器/GPU 驱动尚未就绪时，
    // 若立刻以最高权限拉起 WebView2 主窗口，个别机器会出现前端初始化失败、
    // 界面打不开（只启动后端）、托盘无响应的问题。延迟 5 秒待桌面就绪后再启动，
    // 从根源规避该时序脆弱点，同时基本不影响开机可用感。
    let output = exec_hidden("schtasks", &[
        "/create",
        "/tn", TASK_NAME,
        "/tr", &run_cmd,
        "/sc", "onlogon",
        "/rl", "highest",
        "/delay", "0000:00:05",
        "/f",
    ])?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("创建计划任务失败: {}", stderr.trim()));
    }

    log::info!("计划任务已创建: {} -> {}", TASK_NAME, run_cmd);
    Ok(())
}

/// 检查注册表 Run 键中的自启项是否存在（用于检测旧版残留）
#[cfg(windows)]
fn check_registry_run() -> bool {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    if let Ok(key) = hkcu.open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run") {
        return key.get_value::<String, _>(APP_NAME).is_ok();
    }
    false
}

// ========== 旧方案清理（注册表 Run 键 / 启动快捷方式，历史遗留） ==========

/// 删除任务计划（用于清理旧版方案残留）
#[cfg(windows)]
fn remove_scheduled_task() -> Result<(), String> {
    let output = exec_hidden("schtasks", &[
        "/delete",
        "/tn", TASK_NAME,
        "/f",
    ])?;

    if output.status.success() {
        log::info!("计划任务已删除: {}", TASK_NAME);
        return Ok(());
    }

    // 删除失败：可能是任务不存在（不同系统错误文本不一，甚至 stderr 为空）
    // 用 query 二次确认，任务确实不存在则视为成功跳过，避免误报失败
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{} {}", stderr, stdout);
    let lower = combined.to_lowercase();

    let not_found = lower.contains("cannot find")
        || lower.contains("does not exist")
        || lower.contains("system cannot find")
        || lower.contains("不存在")
        || lower.contains("找不到")
        || lower.contains("没有找到");
    if not_found || !check_scheduled_task() {
        log::info!("计划任务不存在，跳过删除");
        return Ok(());
    }

    if lower.contains("denied") || lower.contains("access denied") || lower.contains("拒绝访问") || lower.contains("权限") {
        return Err("需要管理员权限，请以管理员身份运行后重试".to_string());
    }

    Err(format!("删除计划任务失败: {}", combined.trim()))
}

/// 检查任务计划是否存在（用于检测旧版残留）
#[cfg(windows)]
fn check_scheduled_task() -> bool {
    match exec_hidden("schtasks", &["/query", "/tn", TASK_NAME]) {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

// ========== 注册表 Run 键清理 ==========

/// 删除注册表 Run 键中的自启条目
#[cfg(windows)]
fn remove_registry_run() -> Result<(), String> {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            winreg::enums::KEY_SET_VALUE,
        )
        .map_err(|e| format!("打开注册表 Run 键失败: {}", e))?;

    match key.delete_value(APP_NAME) {
        Ok(()) => {
            log::info!("注册表 Run 键已删除");
            Ok(())
        }
        Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => {
            log::info!("注册表 Run 键不存在，跳过删除");
            Ok(())
        }
        Err(e) => Err(format!("删除注册表 Run 键失败: {}", e)),
    }
}

// ========== 启动文件夹快捷方式清理（旧版方案，历史遗留） ==========

#[cfg(windows)]
fn remove_startup_shortcut() -> Result<(), String> {
    let startup_dir = get_startup_folder()?;
    let lnk_path = startup_dir.join("NexBox.lnk");

    if lnk_path.exists() {
        std::fs::remove_file(&lnk_path)
            .map_err(|e| format!("删除快捷方式失败: {}", e))?;
        log::info!("启动文件夹快捷方式已删除");
    }
    Ok(())
}

#[cfg(windows)]
fn check_startup_shortcut() -> bool {
    if let Ok(startup_dir) = get_startup_folder() {
        return startup_dir.join("NexBox.lnk").exists();
    }
    false
}

// ========== Tauri Commands ==========

#[tauri::command]
pub async fn set_nexbox_auto_start(enable: bool, minimized_start: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        if enable {
            let app_path = std::env::current_exe()
                .map_err(|e| format!("获取程序路径失败: {}", e))?
                .to_string_lossy()
                .replace("/", "\\");

            log::info!("准备设置开机自启（计划任务），最小化启动={}", minimized_start);

            // 主方案：任务计划程序（管理员静默启动，不弹 UAC）
            create_scheduled_task(&app_path, minimized_start)?;

            // 清理旧版方案残留（注册表 Run 键、启动快捷方式），确保只保留一个启动项
            match remove_registry_run() {
                Ok(()) => log::info!("旧注册表 Run 键已清理"),
                Err(e) => log::warn!("清理旧注册表 Run 键失败: {}", e),
            }
            match remove_startup_shortcut() {
                Ok(()) => log::info!("旧启动快捷方式已清理"),
                Err(e) => log::warn!("清理旧启动快捷方式失败: {}", e),
            }

            if check_scheduled_task() {
                log::info!("开机自启设置成功（计划任务）");
                return Ok(());
            }
            return Err("开机自启设置失败：计划任务创建未生效".to_string());
        } else {
            let mut errors: Vec<String> = Vec::new();

            match remove_scheduled_task() {
                Ok(()) => log::info!("计划任务已删除"),
                Err(e) => {
                    log::warn!("删除计划任务失败: {}", e);
                    errors.push(e);
                }
            }
            match remove_registry_run() {
                Ok(()) => log::info!("注册表 Run 键已删除"),
                Err(e) => {
                    log::warn!("删除注册表 Run 键失败: {}", e);
                    errors.push(e);
                }
            }
            match remove_startup_shortcut() {
                Ok(()) => log::info!("快捷方式已删除"),
                Err(e) => {
                    log::warn!("删除快捷方式失败: {}", e);
                    errors.push(e);
                }
            }

            if errors.is_empty() {
                log::info!("开机自启已完全关闭");
                return Ok(());
            }
            return Err(format!("关闭开机自启失败：{}", errors.join("；")));
        }
    }

    #[cfg(not(windows))]
    {
        let _ = enable;
        Err("当前平台不支持开机自启动设置".to_string())
    }
}

#[tauri::command]
pub async fn check_nexbox_auto_start() -> Result<bool, String> {
    #[cfg(windows)]
    {
        let task_exists = check_scheduled_task();
        let run_exists = check_registry_run();
        let shortcut_exists = check_startup_shortcut();

        let enabled = task_exists || run_exists || shortcut_exists;
        log::debug!(
            "开机自启状态检查：计划任务={}, Run键={}, 快捷方式={}, 最终={}",
            task_exists,
            run_exists,
            shortcut_exists,
            enabled
        );
        Ok(enabled)
    }

    #[cfg(not(windows))]
    {
        Ok(false)
    }
}

/// 应用启动时调用：自动清理旧版方案残留（注册表 Run 键、启动快捷方式）
/// 用于从旧版（Run 键/快捷方式自启）升级到新版（计划任务）时，
/// 检测到残留即自动删除，确保只保留当前的计划任务启动项。
pub fn cleanup_legacy_auto_start() {
    #[cfg(windows)]
    {
        if check_registry_run() {
            match remove_registry_run() {
                Ok(()) => log::info!("已自动清理旧版注册表 Run 键启动项"),
                Err(e) => log::warn!("自动清理旧注册表 Run 键失败: {}", e),
            }
        }
        if check_startup_shortcut() {
            match remove_startup_shortcut() {
                Ok(()) => log::info!("已自动清理旧版启动快捷方式"),
                Err(e) => log::warn!("自动清理旧启动快捷方式失败: {}", e),
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = ();
    }
}
