#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 静默安装模式（/S）：无界面完成安装后直接退出，供无人值守/商店分发使用
    if let Some(code) = nexbox_installer_lib::detect_silent_install() {
        std::process::exit(code);
    }
    // GUI 模式依赖 WebView2 渲染，缺失时无法创建 Tauri 窗口，需在启动前原生提示并引导下载
    nexbox_installer_lib::webview2::ensure_runtime_or_exit();
    nexbox_installer_lib::run()
}
