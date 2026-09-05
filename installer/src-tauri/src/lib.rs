mod installer;

use installer::*;

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
