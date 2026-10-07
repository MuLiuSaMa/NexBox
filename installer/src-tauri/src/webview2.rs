//! WebView2 运行时检测与缺失提示。
//! 安装器界面由 WebView2 渲染，缺失时 Tauri 窗口无法创建，
//! 必须在启动 Tauri 之前用原生 MessageBox 引导用户下载安装。

use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use winreg::RegKey;

/// WebView2 Evergreen Runtime 在 EdgeUpdate\Clients 下的固定 GUID
const WEBVIEW2_CLIENT_GUID: &str = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
/// 运行时可能注册的位置：HKLM 64 位视图（x64 安装包）、HKLM WOW6432Node（x86 安装包）、HKCU（每用户安装）
const WEBVIEW2_CLIENT_KEYS: [(&winreg::HKEY, &str); 3] = [
    (&HKEY_LOCAL_MACHINE, r"SOFTWARE"),
    (&HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node"),
    (&HKEY_CURRENT_USER, r"Software"),
];
const WEBVIEW2_DOWNLOAD_URL: &str = "https://developer.microsoft.com/zh-cn/microsoft-edge/webview2";

const MB_YESNO: u32 = 0x4;
const MB_ICONWARNING: u32 = 0x30;
const MB_SETFOREGROUND: u32 = 0x0001_0000;
const MB_TOPMOST: u32 = 0x0004_0000;
const IDYES: i32 = 6;
const SW_SHOWNORMAL: i32 = 1;
const LANG_PRIMARY_ZH: u32 = 0x0004;

#[link(name = "user32")]
extern "system" {
    fn MessageBoxW(
        hwnd: isize,
        lptext: *const u16,
        lpcaption: *const u16,
        utype: u32,
    ) -> i32;
}

#[link(name = "shell32")]
extern "system" {
    fn ShellExecuteW(
        hwnd: isize,
        lpoperation: *const u16,
        lpfile: *const u16,
        lpparameters: *const u16,
        lpdirectory: *const u16,
        nshowcmd: i32,
    ) -> isize;
}

extern "system" {
    fn GetUserDefaultUILanguage() -> u16;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 检测系统是否已安装 WebView2 运行时（与微软官方检测口径一致：pv 值存在且不为占位版本）
pub fn is_available() -> bool {
    for (hkey, root) in WEBVIEW2_CLIENT_KEYS {
        let path = format!("{}\\Microsoft\\EdgeUpdate\\Clients\\{}", root, WEBVIEW2_CLIENT_GUID);
        let Ok(key) = RegKey::predef(*hkey).open_subkey(&path) else {
            continue;
        };
        let Ok(pv) = key.get_value::<String, _>("pv") else {
            continue;
        };
        let pv = pv.trim();
        if !pv.is_empty() && pv != "0.0.0.0" {
            return true;
        }
    }
    false
}

fn is_system_language_chinese() -> bool {
    (unsafe { GetUserDefaultUILanguage() } as u32 & 0x3FF) == LANG_PRIMARY_ZH
}

/// GUI 启动前调用：缺少 WebView2 时弹原生提示，用户确认后跳转官网下载页，随后退出进程。
/// 静默安装（/S）不经过此路径——纯 Rust 安装逻辑不依赖 WebView2。
pub fn ensure_runtime_or_exit() {
    if is_available() {
        return;
    }

    let (text, caption) = if is_system_language_chinese() {
        (
            "检测到系统缺少 Microsoft Edge WebView2 运行时，安装程序无法显示界面。\n\n点击“是”前往官网下载并安装 WebView2，安装完成后请重新运行本安装程序。\n点击“否”退出。",
            "新境盒 安装程序",
        )
    } else {
        (
            "The Microsoft Edge WebView2 Runtime is missing on this system, so the installer cannot display its interface.\n\nClick Yes to download and install WebView2 from the official website, then run this installer again.\nClick No to exit.",
            "NexBox Installer",
        )
    };

    let text_w = wide(text);
    let caption_w = wide(caption);
    let clicked_yes = unsafe {
        MessageBoxW(
            0,
            text_w.as_ptr(),
            caption_w.as_ptr(),
            MB_YESNO | MB_ICONWARNING | MB_SETFOREGROUND | MB_TOPMOST,
        )
    } == IDYES;

    if clicked_yes {
        let url_w = wide(WEBVIEW2_DOWNLOAD_URL);
        let open_w = wide("open");
        unsafe {
            ShellExecuteW(
                0,
                open_w.as_ptr(),
                url_w.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            );
        }
    }

    std::process::exit(1);
}
