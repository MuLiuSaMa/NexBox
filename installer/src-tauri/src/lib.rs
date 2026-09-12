mod installer;

use installer::*;

/// 检测静默安装参数并执行无界面安装（供 Microsoft Store 等无人值守场景使用）。
/// 支持 /S、/SILENT、/VERYSILENT、/QUIET（不区分大小写），
/// 可选 /D=<路径> 指定安装目录（含空格路径需整体加引号传递）。
/// 命中静默参数时返回 Some(退出码)：成功 0，失败 1；未命中返回 None（正常进入 GUI）。
pub fn detect_silent_install() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let silent = args.iter().any(|a| {
        let a = a.trim();
        a.eq_ignore_ascii_case("/S")
            || a.eq_ignore_ascii_case("/SILENT")
            || a.eq_ignore_ascii_case("/VERYSILENT")
            || a.eq_ignore_ascii_case("/QUIET")
    });
    if !silent {
        return None;
    }

    let custom_dir = args.iter().find_map(|a| {
        let a = a.trim();
        if a.len() >= 4 && a.as_bytes()[..3].eq_ignore_ascii_case(b"/D=") {
            Some(a[3..].trim_matches('"').to_string())
        } else {
            None
        }
    });

    // 安装目录优先级：显式 /D= > 已安装目录（更新场景，沿用原路径）> 默认 Program Files\NexBox
    let target_dir = custom_dir.unwrap_or_else(get_default_install_path);

    // 与 GUI 默认行为保持一致：创建桌面快捷方式
    match install(target_dir, true) {
        Ok(()) => Some(0),
        Err(e) => {
            eprintln!("NexBox silent install failed: {}", e);
            Some(1)
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_default_install_path,
            is_existing_install,
            check_disk_space,
            get_resource_files,
            install,
            cancel_install,
            launch_installed_app,
            get_app_version,
            schedule_installer_cleanup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
