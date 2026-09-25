// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // ═══════════════════════════════════════════════════════════════════
    // 第零优先：权限保障。
    // manifest 已从 requireAdministrator 改为 asInvoker（MSIX 包内禁止入口提权），
    // 因此在运行时检测：未提权则通过 UAC（runas）重启自身。
    // 对普通 exe：效果与原来双击弹 UAC 一致；
    // 对 MSIX：解决"不支持该请求"启动失败，提权进程继承包身份。
    // ═══════════════════════════════════════════════════════════════════
    #[cfg(windows)]
    ensure_elevation();

    // ═══════════════════════════════════════════════════════════════════
    // 第一优先：初始化日志（在任何 Tauri 代码之前）
    // 这样即使 .build() 崩溃或 single-instance 插件退出，
    // 也能在日志文件中留下记录，便于排查。
    // 日志路径：%LOCALAPPDATA%/NexBox/nexbox.log
    // ═══════════════════════════════════════════════════════════════════
    init_early_logging();

    log::info!(
        "═══════════════════════════════════════════════════════════════"
    );
    log::info!(
        "[BOOT] nexbox.exe 启动 | pid={} exe={:?} cwd={:?} args={:?}",
        std::process::id(),
        std::env::current_exe().ok(),
        std::env::current_dir().ok(),
        std::env::args().collect::<Vec<_>>()
    );

    // 开机自启修复：Windows 通过注册表 Run 键 / 计划任务启动程序时
    // 工作目录可能是 System32，导致 Tauri 加载资源和依赖失败。
    // 主动切换到 exe 所在目录。
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let _ = std::env::set_current_dir(exe_dir);
            log::info!("[BOOT] 工作目录已切换到: {}", exe_dir.display());
        }
    }

    // ═══════════════════════════════════════════════════════════════════
    // 第二优先：MSIX 包内运行时显式设置进程 AUMID 为包 AUMID。
    // exe 位于 WindowsApps 时窗口默认按 exe 路径生成 AUMID，Shell 对这种
    // "有包身份但 AUMID 非包 AUMID"的窗口按 packaged 语义渲染图标，
    // 给任务栏/跳表图标垫系统强调色底板（用户看到的红底）。
    // 显式设为包 AUMID 后 Shell 解析 manifest 的 altform-unplated 资产，
    // 任务栏图标恢复无底板渲染（必须在任何窗口创建前调用）。
    // 非打包运行（exe 不在 WindowsApps）不设置，保持与手动快捷方式一致。
    // ═══════════════════════════════════════════════════════════════════
    #[cfg(windows)]
    set_packaged_aumid();

    log::info!("[BOOT] 即将进入 nexbox_lib::run()");
    nexbox_lib::run();
}

/// 检测当前进程是否已提权；未提权则通过 ShellExecuteW("runas") 以管理员重启自身。
/// UAC 被拒绝或启动失败时直接退出（NexBox 的核心功能依赖管理员权限）。
#[cfg(windows)]
fn ensure_elevation() {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    unsafe {
        let mut token: windows_sys::Win32::Foundation::HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            // 打不开 token（极罕见）：放行，让后续逻辑按原有方式运行/报错
            return;
        }
        let mut elevation: TOKEN_ELEVATION = std::mem::zeroed();
        let mut ret_len: u32 = 0;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut TOKEN_ELEVATION as *mut core::ffi::c_void,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        );
        CloseHandle(token);
        if ok == 0 || elevation.TokenIsElevated != 0 {
            // 已提权（或检测失败）：正常继续
            return;
        }

        // 未提权：以管理员身份重启自身
        log::info!("[BOOT] 当前未提权，尝试通过 UAC 以管理员重启自身");
        let exe: Vec<u16> = std::env::current_exe()
            .unwrap_or_default()
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let verb: Vec<u16> = "runas\0".encode_utf16().collect();
        let result = ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            exe.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL as i32,
        );
        if result as usize > 32 {
            // 提权实例已拉起，当前实例退出
            std::process::exit(0);
        } else {
            // 用户拒绝 UAC 或启动失败：退出（无权限跑不下去）
            log::warn!(
                "[BOOT] UAC 提权未成功 (ShellExecuteW code={})，退出",
                result as usize
            );
            std::process::exit(0);
        }
    }
}

/// 仅当 exe 位于 WindowsApps（MSIX 包内运行）时，把进程 AUMID 设为包 AUMID。
/// 包 AUMID = PackageFamilyName!ApplicationId（如 MuLiuSaMa.NexBox_rkw8a3fpq7zcw!NexBox）。
/// PackageFamilyName 从包安装目录名解析（格式固定：<FamilyName>_<Version>_<Arch>__<Hash>，
/// MSIX Identity Name 不允许下划线，故取首尾段拼接即可；哈希固定 13 字符无下划线）。
/// ApplicationId 必须与 msix/AppxManifest.xml 的 <Application Id> 完全一致。
#[cfg(windows)]
fn set_packaged_aumid() {
    use windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

    const APP_ID: &str = "NexBox"; // msix/build-msix.ps1 生成的 <Application Id="NexBox">

    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return,
    };
    if !exe.to_string_lossy().to_lowercase().contains("\\windowsapps\\") {
        return; // 非 MSIX 运行：不设置，保持默认 exe 路径 AUMID
    }
    let dir_name = match exe
        .parent()
        .and_then(|d| d.file_name())
        .and_then(|n| n.to_str())
    {
        Some(n) => n,
        None => return,
    };
    let segs: Vec<&str> = dir_name.split('_').filter(|s| !s.is_empty()).collect();
    if segs.len() < 2 {
        log::warn!("[BOOT] 包目录名不符合预期，跳过 AUMID 设置: {dir_name}");
        return;
    }
    let family = format!("{}_{}", segs[0], segs[segs.len() - 1]);
    let aumid = format!("{}!{}", family, APP_ID);

    let wide: Vec<u16> = aumid.encode_utf16().chain(std::iter::once(0)).collect();
    let hr = unsafe { SetCurrentProcessExplicitAppUserModelID(wide.as_ptr()) };
    if hr == 0 {
        log::info!("[BOOT] 已设置进程 AUMID = {aumid}");
    } else {
        log::warn!("[BOOT] 设置进程 AUMID 失败 hr=0x{:08X}", hr);
    }
}

/// 在 main() 最开始初始化日志，确保 .build() 之前的崩溃也能被记录。
///
/// Release 模式：日志写入文件 `%LOCALAPPDATA%/NexBox/nexbox.log`
/// Debug 模式：日志同时输出到控制台和文件
fn init_early_logging() {
    // Debug 模式：由 tauri_plugin_log 处理日志（控制台输出）
    // Release 模式：初始化文件日志，因为开机自启时没有控制台
    if cfg!(debug_assertions) {
        return;
    }

    let log_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("NexBox");
    let _ = std::fs::create_dir_all(&log_dir);
    let log_path = log_dir.join("nexbox.log");

    // 限制日志文件大小，避免无限增长（超过 5MB 时截断）
    if let Ok(metadata) = std::fs::metadata(&log_path) {
        if metadata.len() > 5 * 1024 * 1024 {
            let _ = std::fs::remove_file(&log_path);
        }
    }

    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path);

    let mut builder = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    );

    if let Ok(file) = log_file {
        builder.target(env_logger::Target::Pipe(Box::new(file)));
    }

    let _ = builder.try_init();
}
