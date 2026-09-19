use serde::{Deserialize, Serialize};
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use winreg::enums::*;
use winreg::RegKey;
use winreg::types::FromRegValue;
use base64::Engine as _;

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupItem {
    pub name: String,
    pub file_location: String,
    pub location_type: String,
    pub item_type: String,
    pub reg_key_path: Option<String>,
    pub reg_value_name: Option<String>,
    pub folder_path: Option<String>,
    pub raw_registry_value: Option<String>,
    /// 是否为已禁用项（可恢复，不会被删除）
    pub is_disabled: bool,
}

pub(crate) fn expand_env_vars(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let mut var_name = String::new();
            loop {
                match chars.next() {
                    Some('%') => break,
                    Some(ch) => var_name.push(ch),
                    None => {
                        result.push('%');
                        result.push_str(&var_name);
                        return result;
                    }
                }
            }
            if var_name.is_empty() {
                result.push('%');
            } else if let Ok(val) = std::env::var(&var_name) {
                result.push_str(&val);
            } else {
                result.push('%');
                result.push_str(&var_name);
                result.push('%');
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn sanitize_path(s: &str) -> String {
    let mut s = s.to_string();
    s = s.replace('\"', "");
    let mut i;

    while s.contains('/') {
        i = s.rfind('/').unwrap();
        s = s[..i].to_string();
    }

    i = s.find(".exe").unwrap_or(s.len().wrapping_sub(4));
    if i < s.len() {
        s = s[..=i + 3].to_string();
    }

    s.trim().to_string()
}

/// 原生解析 .lnk 快捷方式的目标路径（不依赖 PowerShell）。
/// 按 ShellLink 格式读取 LinkInfo 中的 LocalBasePath（优先 Unicode，回退 ANSI），失败返回 None。
pub fn resolve_shortcut_target(lnk_path: &str) -> Option<String> {
    let bytes = fs::read(lnk_path).ok()?;
    if bytes.len() < 0x4C {
        return None;
    }
    // 校验文件头处的 Shell Link CLSID（00021401-0000-0000-C000-000000000046，LE 存储）
    let expected_clsid: [u8; 16] = [
        0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x46,
    ];
    if &bytes[0x04..0x14] != expected_clsid.as_slice() {
        return None;
    }

    let link_flags = u32::from_le_bytes([bytes[0x14], bytes[0x15], bytes[0x16], bytes[0x17]]);
    if link_flags & 0x02 == 0 {
        // 无 LinkInfo，无法取得本地路径
        return None;
    }

    // 跳过 Header + 可选的 LinkTargetIDList，定位 LinkInfo 起始
    let mut offset = 0x4Cusize;
    if link_flags & 0x01 != 0 {
        if bytes.len() < offset + 2 {
            return None;
        }
        let idlist_size = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]) as usize;
        offset += 2 + idlist_size;
    }
    if bytes.len() < offset + 4 {
        return None;
    }
    let link_info_size =
        u32::from_le_bytes([bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3]])
            as usize;
    if bytes.len() < offset + link_info_size {
        return None;
    }
    let li = &bytes[offset..offset + link_info_size];
    if li.len() < 0x1C {
        return None;
    }

    let header_size = u32::from_le_bytes([li[4], li[5], li[6], li[7]]) as usize;
    // LinkInfo 内偏移基准为 li 起始
    let local_base_path_off = u32::from_le_bytes([li[16], li[17], li[18], li[19]]) as usize;
    let unicode_off = if header_size >= 0x24 && li.len() >= 0x28 {
        Some(u32::from_le_bytes([li[0x1C], li[0x1D], li[0x1E], li[0x1F]]) as usize)
    } else {
        None
    };

    // 优先 Unicode 本地路径
    if let Some(uoff) = unicode_off {
        if uoff + 2 <= li.len() {
            let mut p = uoff;
            let mut utf16 = Vec::new();
            while p + 1 < li.len() {
                let ch = u16::from_le_bytes([li[p], li[p + 1]]);
                if ch == 0 {
                    break;
                }
                utf16.push(ch);
                p += 2;
            }
            if !utf16.is_empty() {
                if let Ok(s) = String::from_utf16(&utf16) {
                    if !s.is_empty() {
                        return Some(s);
                    }
                }
            }
        }
    }

    // 回退 ANSI 本地路径
    if local_base_path_off + 1 <= li.len() {
        let mut p = local_base_path_off;
        let mut ansi = Vec::new();
        while p < li.len() {
            let b = li[p];
            if b == 0 {
                break;
            }
            ansi.push(b);
            p += 1;
        }
        if !ansi.is_empty() {
            return Some(String::from_utf8_lossy(&ansi).to_string());
        }
    }

    None
}

fn scan_registry_key(
    root: RegKey,
    subkey_path: &str,
    location_type: &str,
    items: &mut Vec<StartupItem>,
    is_disabled: bool,
) {
    if let Ok(key) = root.open_subkey_with_flags(subkey_path, KEY_READ) {
        for value_result in key.enum_values() {
            if let Ok((name, value)) = value_result {
                if name.is_empty() || name == "(默认)" || name == "(Default)" {
                    continue;
                }
                let raw_path = match String::from_reg_value(&value) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                if raw_path.trim().is_empty() {
                    continue;
                }
                let item_name = name;
                let expanded = expand_env_vars(&raw_path);
                let display_path = sanitize_path(&expanded);
                items.push(StartupItem {
                    name: item_name.clone(),
                    file_location: display_path,
                    location_type: location_type.to_string(),
                    item_type: "Registry".to_string(),
                    reg_key_path: Some(format!("{}", subkey_path)),
                    reg_value_name: Some(item_name),
                    folder_path: None,
                    raw_registry_value: Some(raw_path),
                    is_disabled,
                });
            }
        }
    }
}

fn scan_startup_folder(
    folder: PathBuf,
    location_type: &str,
    items: &mut Vec<StartupItem>,
    is_disabled: bool,
) {
    if !folder.exists() {
        return;
    }
    if let Ok(entries) = fs::read_dir(&folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = match path.file_name() {
                Some(n) => n.to_string_lossy().to_string(),
                None => continue,
            };
            let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase());
            let is_exe = ext.as_deref() == Some("exe");
            let is_bat = ext.as_deref() == Some("bat") || ext.as_deref() == Some("cmd");
            let is_lnk = ext.as_deref() == Some("lnk");
            if !is_exe && !is_bat && !is_lnk {
                continue;
            }
            let name = if let Some(stem) = path.file_stem() {
                stem.to_string_lossy().to_string()
            } else {
                file_name.clone()
            };
            let target_path = if is_lnk {
                resolve_shortcut_target(&path.to_string_lossy())
                    .unwrap_or_else(|| path.to_string_lossy().to_string())
            } else {
                path.to_string_lossy().to_string()
            };
            items.push(StartupItem {
                name,
                file_location: target_path,
                location_type: location_type.to_string(),
                item_type: "Folder".to_string(),
                reg_key_path: None,
                reg_value_name: None,
                folder_path: Some(path.to_string_lossy().to_string()),
                raw_registry_value: None,
                is_disabled,
            });
        }
    }
}

fn get_user_startup_folder() -> Option<PathBuf> {
    dirs::config_dir().map(|p| {
        p.join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Startup")
    })
}

fn get_common_startup_folder() -> Option<PathBuf> {
    dirs::data_dir().map(|p| {
        p.join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Startup")
    })
}

/// 把 HICON 画到指定尺寸的 32 位 DIB 位图并编码为 PNG 字节（BGRA -> RGBA）
fn draw_hicon_png(hicon: *mut std::ffi::c_void, size: i32) -> Option<Vec<u8>> {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::Graphics::Gdi::{
            BITMAPINFO, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject,
            SelectObject, ReleaseDC, GetDC,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::DrawIconEx;

        unsafe {
            let hdc_screen = GetDC(std::ptr::null_mut());
            if hdc_screen.is_null() {
                return None;
            }
            let hdc = CreateCompatibleDC(hdc_screen);
            if hdc.is_null() {
                ReleaseDC(std::ptr::null_mut(), hdc_screen);
                return None;
            }

            let mut bmi: BITMAPINFO = std::mem::zeroed();
            bmi.bmiHeader.biSize =
                std::mem::size_of::<windows_sys::Win32::Graphics::Gdi::BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = size;
            bmi.bmiHeader.biHeight = -size; // top-down
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = 0; // BI_RGB

            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let hbmp = CreateDIBSection(hdc, &bmi, 0, &mut bits, std::ptr::null_mut(), 0);
            if hbmp.is_null() || bits.is_null() {
                DeleteDC(hdc);
                ReleaseDC(std::ptr::null_mut(), hdc_screen);
                return None;
            }

            let old = SelectObject(hdc, hbmp as *mut std::ffi::c_void);
            DrawIconEx(
                hdc, 0, 0, hicon, size, size, 0, std::ptr::null_mut(), 0x0003, // DI_NORMAL
            );

            // 位图为 BGRA，转为 RGBA
            let total = (size * size * 4) as usize;
            let src = std::slice::from_raw_parts(bits as *const u8, total);
            let mut rgba = Vec::with_capacity(total);
            for i in 0..(size * size) as usize {
                rgba.push(src[i * 4 + 2]); // R
                rgba.push(src[i * 4 + 1]); // G
                rgba.push(src[i * 4]);     // B
                rgba.push(src[i * 4 + 3]); // A
            }

            SelectObject(hdc, old);
            DeleteObject(hbmp);
            DeleteDC(hdc);
            ReleaseDC(std::ptr::null_mut(), hdc_screen);

            let img = image::RgbaImage::from_raw(size as u32, size as u32, rgba)?;
            let mut out = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(img)
                .write_to(&mut out, image::ImageFormat::Png)
                .ok()?;
            Some(out.into_inner())
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (hicon, size);
        None
    }
}

/// 从 exe 文件提取图标并编码为 PNG data URI（不使用 PowerShell）。
/// 优先用 SHDefExtractIconW 按 256px 提取高清图标（在 32px 框内缩小显示更清晰），
/// 失败时回退到 SHGetFileInfoW 的默认图标。
pub fn extract_icon_data_uri(file_path: &str) -> Option<String> {
    extract_icon_data_uri_with_index(file_path, 0)
}

/// 从 exe/ico 文件提取指定资源索引的图标（DisplayIcon 形如 `xxx.exe,1` 时使用），
/// 编码为 PNG data URI；索引无效时默认取第一个图标资源。
pub fn extract_icon_data_uri_with_index(file_path: &str, icon_index: i32) -> Option<String> {
    extract_icon_data_uri_at_size(file_path, 256, icon_index)
}

/// 按指定尺寸提取图标 data URI（批量进程图标等场景使用小尺寸以提升效率）
fn extract_icon_data_uri_at_size(file_path: &str, size: i32, icon_index: i32) -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::Shell::SHDefExtractIconW;
        use windows_sys::Win32::UI::Shell::SHGetFileInfoW;
        use windows_sys::Win32::UI::Shell::SHFILEINFOW;
        use windows_sys::Win32::UI::Shell::SHGFI_ICON;
        use windows_sys::Win32::UI::Shell::SHGFI_LARGEICON;
        use windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon;

        unsafe {
            let wide: Vec<u16> = file_path
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            // 优先按指定尺寸提取高清图标（Windows 会从 EXE 资源中取最大可用尺寸），
            // 前端在 32px 框内缩小显示，在高 DPI 屏上也不会发糊。
            let mut hicon: *mut std::ffi::c_void = std::ptr::null_mut();
            let mut bytes: Option<Vec<u8>> = None;
            let hr = SHDefExtractIconW(
                wide.as_ptr(),
                icon_index,
                0,
                &mut hicon,
                std::ptr::null_mut(),
                size as u32,
            );
            if hr == 0 && !hicon.is_null() {
                bytes = draw_hicon_png(hicon, size);
                DestroyIcon(hicon);
            }

            // 回退：SHGetFileInfoW 默认图标（32px）
            if bytes.is_none() {
                let mut sfi: SHFILEINFOW = std::mem::zeroed();
                let got = SHGetFileInfoW(
                    wide.as_ptr(),
                    0,
                    &mut sfi,
                    std::mem::size_of::<SHFILEINFOW>() as u32,
                    SHGFI_ICON | SHGFI_LARGEICON,
                );
                if got == 0 || sfi.hIcon.is_null() {
                    return None;
                }
                bytes = draw_hicon_png(sfi.hIcon, 32);
                if bytes.is_none() {
                    DestroyIcon(sfi.hIcon);
                }
            }

            let png = bytes?;
            let b64 = base64::engine::general_purpose::STANDARD.encode(png);
            Some(format!("data:image/png;base64,{b64}"))
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (file_path, size);
        None
    }
}

/// 批量提取进程图标 data URI（不使用 PowerShell）；失败项为空字符串
#[tauri::command]
pub async fn get_process_icons(exe_paths: Vec<String>) -> Vec<String> {
    exe_paths
        .iter()
        .map(|p| extract_icon_data_uri_at_size(p, 64, 0).unwrap_or_default())
        .collect()
}

/// 获取启动项对应软件的图标（data URI），失败返回空字符串
#[tauri::command]
pub async fn get_startup_item_icon(file_location: String) -> String {
    extract_icon_data_uri(&file_location).unwrap_or_default()
}

/// Registry 键路径：Run -> RunDisabled，RunOnce -> RunOnceDisabled
fn registry_disabled_key_path(reg_key_path: &str) -> String {
    if reg_key_path.contains("RunOnce") {
        reg_key_path.replace("RunOnce", "RunOnceDisabled")
    } else {
        reg_key_path.replace("Run", "RunDisabled")
    }
}

/// 反向：RunDisabled -> Run，RunOnceDisabled -> RunOnce
fn registry_enabled_key_path(reg_key_path: &str) -> String {
    if reg_key_path.contains("RunOnceDisabled") {
        reg_key_path.replace("RunOnceDisabled", "RunOnce")
    } else {
        reg_key_path.replace("RunDisabled", "Run")
    }
}

fn resolve_registry_root(location_type: &str) -> RegKey {
    if location_type.starts_with("HKLM") {
        RegKey::predef(HKEY_LOCAL_MACHINE)
    } else {
        RegKey::predef(HKEY_CURRENT_USER)
    }
}

/// 打开键（不存在则创建）
fn open_or_create_key(root: &RegKey, path: &str) -> Result<RegKey, String> {
    match root.open_subkey_with_flags(path, KEY_SET_VALUE | KEY_READ) {
        Ok(k) => Ok(k),
        Err(_) => root
            .create_subkey(path)
            .map(|(k, _)| k)
            .map_err(|e| format!("Failed to create registry key: {}", e)),
    }
}

#[tauri::command]
pub async fn scan_startup_items() -> Result<Vec<StartupItem>, String> {
    let mut items: Vec<StartupItem> = Vec::new();

    let hklm = || RegKey::predef(HKEY_LOCAL_MACHINE);
    let hkcu = || RegKey::predef(HKEY_CURRENT_USER);

    // 正常启动项
    scan_registry_key(hklm(), r"Software\Microsoft\Windows\CurrentVersion\Run", "HKLM:Run", &mut items, false);
    scan_registry_key(hklm(), r"Software\Microsoft\Windows\CurrentVersion\RunOnce", "HKLM:RunOnce", &mut items, false);

    #[cfg(target_pointer_width = "64")]
    {
        scan_registry_key(hklm(), r"Software\Wow6432Node\Microsoft\Windows\CurrentVersion\Run", "HKLM:Run (WOW64)", &mut items, false);
        scan_registry_key(hklm(), r"Software\Wow6432Node\Microsoft\Windows\CurrentVersion\RunOnce", "HKLM:RunOnce (WOW64)", &mut items, false);
    }

    scan_registry_key(hkcu(), r"Software\Microsoft\Windows\CurrentVersion\Run", "HKCU:Run", &mut items, false);
    scan_registry_key(hkcu(), r"Software\Microsoft\Windows\CurrentVersion\RunOnce", "HKCU:RunOnce", &mut items, false);

    // 已禁用的启动项（可恢复）
    scan_registry_key(hklm(), r"Software\Microsoft\Windows\CurrentVersion\RunDisabled", "HKLM:RunDisabled", &mut items, true);
    scan_registry_key(hklm(), r"Software\Microsoft\Windows\CurrentVersion\RunOnceDisabled", "HKLM:RunOnceDisabled", &mut items, true);

    #[cfg(target_pointer_width = "64")]
    {
        scan_registry_key(hklm(), r"Software\Wow6432Node\Microsoft\Windows\CurrentVersion\RunDisabled", "HKLM:RunDisabled (WOW64)", &mut items, true);
        scan_registry_key(hklm(), r"Software\Wow6432Node\Microsoft\Windows\CurrentVersion\RunOnceDisabled", "HKLM:RunOnceDisabled (WOW64)", &mut items, true);
    }

    scan_registry_key(hkcu(), r"Software\Microsoft\Windows\CurrentVersion\RunDisabled", "HKCU:RunDisabled", &mut items, true);
    scan_registry_key(hkcu(), r"Software\Microsoft\Windows\CurrentVersion\RunOnceDisabled", "HKCU:RunOnceDisabled", &mut items, true);

    if let Some(user_folder) = get_user_startup_folder() {
        scan_startup_folder(user_folder.clone(), "CUStartupFolder", &mut items, false);
        scan_startup_folder(user_folder.join("Disabled"), "CUStartupFolderDisabled", &mut items, true);
    }
    if let Some(common_folder) = get_common_startup_folder() {
        scan_startup_folder(common_folder.clone(), "LMStartupFolder", &mut items, false);
        scan_startup_folder(common_folder.join("Disabled"), "LMStartupFolderDisabled", &mut items, true);
    }

    Ok(items)
}

/// 禁用启动项（可逆）：注册表值移到 RunDisabled/RunOnceDisabled，文件移到 Startup\Disabled
#[tauri::command]
pub async fn disable_startup_item(item: StartupItem) -> Result<bool, String> {
    if item.item_type == "Registry" {
        let value_name = item.reg_value_name.as_ref().ok_or("Missing registry value name")?;
        let reg_key_path = item.reg_key_path.as_ref().ok_or("Missing registry key path")?;

        let root = resolve_registry_root(&item.location_type);
        let disabled_path = registry_disabled_key_path(reg_key_path);

        // 读原值（保留 REG_SZ / REG_EXPAND_SZ 等类型）
        let key = root
            .open_subkey_with_flags(reg_key_path, KEY_READ | KEY_SET_VALUE)
            .map_err(|e| format!("Failed to open registry key: {}", e))?;
        let raw = key
            .get_raw_value(value_name)
            .map_err(|e| format!("Failed to read registry value: {}", e))?;

        // 写入 Disabled 键
        let disabled_key = open_or_create_key(&root, &disabled_path)?;
        disabled_key
            .set_raw_value(value_name, &raw)
            .map_err(|e| format!("Failed to write disabled registry value: {}", e))?;

        // 删除原值
        key.delete_value(value_name)
            .map_err(|e| format!("Failed to remove original registry value: {}", e))?;

        Ok(true)
    } else if item.item_type == "Folder" {
        let folder_path = item.folder_path.as_ref().ok_or("Missing folder path")?;
        let src = PathBuf::from(folder_path);
        let file_name = src.file_name().ok_or("Invalid file path")?;
        let parent = src.parent().ok_or("Invalid parent directory")?;
        let disabled_dir = parent.join("Disabled");
        fs::create_dir_all(&disabled_dir)
            .map_err(|e| format!("Failed to create Disabled directory: {}", e))?;
        let target = disabled_dir.join(file_name);
        if target.exists() {
            fs::remove_file(&target).map_err(|e| format!("Failed to clean target file: {}", e))?;
        }
        fs::rename(&src, &target).map_err(|e| format!("Failed to move file: {}", e))?;
        Ok(true)
    } else {
        Err("Unknown item type".to_string())
    }
}

/// 恢复已禁用的启动项：注册表值从 RunDisabled 移回 Run，文件从 Startup\Disabled 移回
#[tauri::command]
pub async fn enable_startup_item(item: StartupItem) -> Result<bool, String> {
    if item.item_type == "Registry" {
        let value_name = item.reg_value_name.as_ref().ok_or("Missing registry value name")?;
        let disabled_key_path = item.reg_key_path.as_ref().ok_or("Missing registry key path")?;

        let root = resolve_registry_root(&item.location_type);
        let enabled_key_path = registry_enabled_key_path(disabled_key_path);

        // 读 Disabled 键中的值（保留类型）
        let disabled_key = root
            .open_subkey_with_flags(disabled_key_path, KEY_READ)
            .map_err(|e| format!("Failed to open disabled registry key: {}", e))?;
        let raw = disabled_key
            .get_raw_value(value_name)
            .map_err(|e| format!("Failed to read disabled registry value: {}", e))?;

        // 写回原键
        let enabled_key = open_or_create_key(&root, &enabled_key_path)?;
        enabled_key
            .set_raw_value(value_name, &raw)
            .map_err(|e| format!("Failed to restore registry value: {}", e))?;

        // 删除 Disabled 键中的值
        let disabled_key_set = root
            .open_subkey_with_flags(disabled_key_path, KEY_SET_VALUE)
            .map_err(|e| format!("Failed to open disabled registry key (write): {}", e))?;
        disabled_key_set
            .delete_value(value_name)
            .map_err(|e| format!("Failed to remove disabled registry value: {}", e))?;

        Ok(true)
    } else if item.item_type == "Folder" {
        let folder_path = item.folder_path.as_ref().ok_or("Missing folder path")?;
        let src = PathBuf::from(folder_path);
        let file_name = src.file_name().ok_or("Invalid file path")?;
        let parent = src.parent().ok_or("Invalid parent directory")?;
        let enabled_dir = parent.parent().ok_or("Invalid Disabled directory")?;
        let target = enabled_dir.join(file_name);
        if target.exists() {
            fs::remove_file(&target).map_err(|e| format!("Failed to clean target file: {}", e))?;
        }
        fs::rename(&src, &target).map_err(|e| format!("Failed to restore file: {}", e))?;
        Ok(true)
    } else {
        Err("Unknown item type".to_string())
    }
}

#[tauri::command]
pub async fn locate_startup_file(file_location: String, item_type: String, raw_registry_value: Option<String>) -> Result<(), String> {
    let path = if item_type == "Registry" {
        let raw = raw_registry_value.as_deref().unwrap_or(&file_location);
        sanitize_path(&expand_env_vars(raw))
    } else {
        file_location.trim().trim_matches('"').to_string()
    };

    if path.is_empty() {
        return Err("Empty file path".to_string());
    }

    let path_buf = PathBuf::from(&path);
    let parent = path_buf.parent().map(|p| p.to_string_lossy().to_string());

    if !path_buf.exists() || path_buf.is_dir() {
        if let Some(parent_path) = parent {
            Command::new("explorer")
                .arg(&parent_path)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(|e| format!("Failed to open Explorer: {}", e))?;
        } else {
            return Err("Cannot determine parent directory".to_string());
        }
    } else {
        Command::new("explorer")
            .arg("/select,")
            .arg(&path)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Failed to open Explorer: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn find_startup_key_in_registry(reg_key_path: String, location_type: String) -> Result<(), String> {
    let hive_prefix = if location_type.starts_with("HKLM") {
        r"HKEY_LOCAL_MACHINE"
    } else {
        r"HKEY_CURRENT_USER"
    };
    let full_path = format!("{}\\{}", hive_prefix, reg_key_path);

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let regedit_key_path = r"Software\Microsoft\Windows\CurrentVersion\Applets\Regedit";
    match hkcu.open_subkey_with_flags(regedit_key_path, KEY_SET_VALUE | KEY_READ) {
        Ok(key) => {
            let _ = key.set_value("LastKey", &full_path);
        }
        Err(_) => {
            let _ = hkcu.create_subkey(regedit_key_path);
            if let Ok(key) = hkcu.open_subkey_with_flags(regedit_key_path, KEY_SET_VALUE) {
                let _ = key.set_value("LastKey", &full_path);
            }
        }
    }

    Command::new("regedit")
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("Failed to open regedit: {}", e))?;

    Ok(())
}
