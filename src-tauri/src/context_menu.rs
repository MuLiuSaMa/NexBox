//! 右键菜单 & 此电脑管理器。
//!
//! 提供「右键菜单项」与「此电脑 NameSpace 项」的扫描、隐藏、恢复功能。
//! 隐藏采用可逆方案：把条目子树复制到旁路备份键（`shell_hidden` / `NameSpaceHidden`），
//! 再删除原键；恢复时反向复制回来。全程不删除数据，可无损恢复。
//!
//! 说明：库版 `RegCopyTreeW`（winreg `copy_tree`）对目标键的 ACL 要求较严，
//! 在非管理员或系统键上容易抛 `os error 5`，故这里改用自研的递归复制
//! `copy_key_recursive`，仅需对源键可读、目标键可写。
use serde::{Deserialize, Serialize};
use winreg::enums::*;
use winreg::reg_value::RegValue;
use winreg::RegKey;
use windows_sys::Win32::System::Registry::REG_SAM_FLAGS as RegAccess;

#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::Shell::SHDefExtractIconW;
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon;

use base64::Engine;

/// 右键菜单项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMenuItem {
    /// 显示名：(Default) 优先，空则取 MUIVerb，再空则退回子键名
    pub name: String,
    /// 子键名（如 "CCleaner"）
    pub verb: String,
    /// command 子键的 (Default) 值，可空
    pub command: String,
    /// 根 hive 标识："HKCR"
    pub hive: String,
    /// 来源分类：file / folder / desktop / drive / allFiles
    pub category: String,
    /// 指向 shell 键的完整相对路径，如 r"*\shell"
    pub reg_path: String,
    /// 图标 data URI（PNG），空字符串表示无图标
    pub icon: String,
    pub is_hidden: bool,
}

/// 此电脑 NameSpace 项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThisPcItem {
    /// NameSpace 键 (Default)；空则查 CLSID (Default)；再空退回 clsid
    pub name: String,
    /// {GUID}
    pub clsid: String,
    /// "HKCU" / "HKLM"
    pub hive: String,
    /// 指向 NameSpace 键的完整路径，如 r"Software\...\Explorer\MyComputer\NameSpace"
    pub reg_path: String,
    pub is_hidden: bool,
}

// ---------- 字符串工具 ----------

fn translate_to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn translate_from_wide(v: &[u16]) -> String {
    String::from_utf16_lossy(v)
}

/// 解析 `%VAR%` 环境变量
fn expand_env_vars(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let mut name = String::new();
            loop {
                match chars.next() {
                    Some('%') => break,
                    Some(ch) => name.push(ch),
                    None => {
                        out.push('%');
                        out.push_str(&name);
                        return out;
                    }
                }
            }
            if name.is_empty() {
                out.push('%');
            } else if let Ok(v) = std::env::var(&name) {
                out.push_str(&v);
            } else {
                out.push('%');
                out.push_str(&name);
                out.push('%');
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// 用 `SHLoadIndirectString` 解析 `@dll,-id` / `@%SystemRoot%\...dll,-id` 这类间接字符串。
#[cfg(target_os = "windows")]
fn sh_load_indirect(src: &str) -> Option<String> {
    unsafe {
        let mut buf = vec![0u16; 4096];
        let hr = windows_sys::Win32::UI::Shell::SHLoadIndirectString(
            translate_to_wide(src).as_ptr(),
            buf.as_mut_ptr(),
            buf.len() as u32,
            std::ptr::null_mut(),
        );
        if (hr as i32) >= 0 {
            let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            let s = translate_from_wide(&buf[..end]);
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        None
    }
}
#[cfg(not(target_os = "windows"))]
fn sh_load_indirect(_src: &str) -> Option<String> {
    None
}

/// 把注册表字符串规整为面向用户的可读名：
/// 先展开环境变量，若为 `@` 间接字符串则尝试加载资源，失败则保留原文。
fn friendly_name(raw: &str) -> String {
    let expanded = expand_env_vars(raw).trim().to_string();
    if !expanded.starts_with('@') {
        return expanded;
    }
    sh_load_indirect(&expanded).unwrap_or(expanded)
}

fn read_string(key: &RegKey, name: &str) -> String {
    key.get_value(name).unwrap_or_default()
}

// ---------- 递归复制（替代 RegCopyTreeW） ----------

/// 打开键，不存在则创建（`RegCreateKeyEx` 会自动创建缺失的父级）
fn ensure_subkey(root: &RegKey, path: &str, access: RegAccess) -> Result<RegKey, String> {
    root.open_subkey_with_flags(path, access)
        .or_else(|_| root.create_subkey(path).map(|(k, _)| k))
        .map_err(|e| format!("Failed to open/create '{}': {e}", path))
}

/// 打开源键：先尝试完整权限，失败回退只读；两者都失败才报错。
/// （部分系统 NameSpace 的 CLSID 是特殊/链接键，仅 KEY_READ 可能被拒）
fn open_source_key(root: &RegKey, path: &str) -> Result<RegKey, String> {
    root.open_subkey_with_flags(path, KEY_ALL_ACCESS)
        .or_else(|_| root.open_subkey_with_flags(path, KEY_READ))
        .map_err(|e| format!("Failed to open source '{path}': {e}"))
}

/// 把 `src_root\src_path` 子树复制到 `dst_root\dst_path`（含所有值与嵌套子键）。
/// 源只需可读，目标只需可写，避免 RegCopyTreeW 的 ACL 限制。
fn copy_key_recursive(
    src_root: &RegKey,
    src_path: &str,
    dst_root: &RegKey,
    dst_path: &str,
) -> Result<(), String> {
    let src = open_source_key(src_root, src_path)?;
    let dst = ensure_subkey(dst_root, dst_path, KEY_SET_VALUE | KEY_CREATE_SUB_KEY | KEY_READ)?;

    // 复制所有值（含 (Default)，类型保持一致）
    for val in src.enum_values() {
        let (name, data): (String, RegValue) = val
            .map_err(|e| format!("Failed to enumerate values of '{src_path}': {e}"))?;
        dst.set_raw_value(&name, &data)
            .map_err(|e| format!("Failed to write value '{:?}' of '{dst_path}': {e}", name))?;
    }

    // 递归复制子键
    for sub in src.enum_keys() {
        let sub_name = sub.map_err(|e| format!("Failed to enumerate subkeys of '{src_path}': {e}"))?;
        copy_key_recursive(
            src_root,
            &format!("{src_path}\\{sub_name}"),
            dst_root,
            &format!("{dst_path}\\{sub_name}"),
        )?;
    }
    Ok(())
}

fn delete_key(root: &RegKey, parent_path: &str, name: &str) -> Result<(), String> {
    root.open_subkey_with_flags(parent_path, KEY_ALL_ACCESS)
        .map_err(|e| format!("Failed to open '{parent_path}': {e}"))?
        .delete_subkey_all(name)
        .map_err(|e| format!("Failed to delete '{parent_path}\\{name}': {e}"))
}

// ---------- 图标提取 ----------

/// 从 HICON 绘制 PNG 字节（复用 startup_manager 的实现逻辑）
#[cfg(target_os = "windows")]
fn draw_hicon_to_png(hicon: *mut std::ffi::c_void, size: i32) -> Option<Vec<u8>> {
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
        bmi.bmiHeader.biHeight = -size;
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = 0;

        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let hbmp = CreateDIBSection(hdc, &bmi, 0, &mut bits, std::ptr::null_mut(), 0);
        if hbmp.is_null() || bits.is_null() {
            DeleteDC(hdc);
            ReleaseDC(std::ptr::null_mut(), hdc_screen);
            return None;
        }

        let old = SelectObject(hdc, hbmp as *mut std::ffi::c_void);
        DrawIconEx(
            hdc, 0, 0, hicon, size, size, 0, std::ptr::null_mut(), 0x0003,
        );

        let total = (size * size * 4) as usize;
        let src = std::slice::from_raw_parts(bits as *const u8, total);
        let mut rgba = Vec::with_capacity(total);
        for i in 0..(size * size) as usize {
            rgba.push(src[i * 4 + 2]);
            rgba.push(src[i * 4 + 1]);
            rgba.push(src[i * 4]);
            rgba.push(src[i * 4 + 3]);
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

/// 从 exe/dll 指定索引提取图标并返回 data URI
fn extract_icon_by_index(file_path: &str, icon_index: i32) -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        let wide: Vec<u16> = file_path
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        let size = 64i32;
        let mut hicon: *mut std::ffi::c_void = std::ptr::null_mut();
        let bytes = unsafe {
            let hr = SHDefExtractIconW(
                wide.as_ptr(),
                icon_index,
                0,
                &mut hicon,
                std::ptr::null_mut(),
                size as u32,
            );
            if hr == 0 && !hicon.is_null() {
                let result = draw_hicon_to_png(hicon, size);
                DestroyIcon(hicon);
                result
            } else {
                None
            }
        };

        if let Some(png_bytes) = bytes {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
            return Some(format!("data:image/png;base64,{b64}"));
        }
        None
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (file_path, icon_index);
        None
    }
}

// ---------- 右键菜单 ----------

/// 从注册表 Icon 值解析出文件路径和图标索引
/// 格式: "C:\path\to\file.exe,0" 或 "C:\path\to\file.dll,-100" 或 "@dll,-id"
fn parse_icon_value(raw: &str) -> Option<(String, i32)> {
    let expanded = expand_env_vars(raw).trim().to_string();
    if expanded.is_empty() {
        return None;
    }
    // 处理 @dll,-id 间接字符串格式
    if expanded.starts_with('@') {
        if let Some(resolved) = sh_load_indirect(&expanded) {
            return parse_icon_value(&resolved);
        }
        return None;
    }
    // 分离路径和索引（最后一个逗号分隔）
    if let Some(pos) = expanded.rfind(',') {
        let path = expanded[..pos].trim().to_string();
        let index_str = expanded[pos + 1..].trim().to_string();
        if let Ok(index) = index_str.parse::<i32>() {
            return Some((path, index));
        }
        // 索引解析失败，尝试只用路径
        return Some((expanded, 0));
    }
    // 无逗号，整个字符串就是路径
    Some((expanded, 0))
}

/// 读取一个 shell verb 子键的显示名、命令和图标
fn resolve_verb_info(root: &RegKey, verb: &str) -> (String, String, String) {
    let mut name = String::new();
    let mut command = String::new();
    let mut icon = String::new();
    if let Ok(key) = root.open_subkey_with_flags(verb, KEY_READ) {
        let def = read_string(&key, "").trim().to_string();
        if !def.is_empty() && !def.eq_ignore_ascii_case("(default)") {
            name = friendly_name(&def);
        } else {
            let mui = read_string(&key, "MUIVerb").trim().to_string();
            if !mui.is_empty() {
                name = friendly_name(&mui);
            }
        }
        if let Ok(cmd) = key.open_subkey_with_flags("command", KEY_READ) {
            command = read_string(&cmd, "").trim().to_string();
        }
        // 读取 Icon 值
        let icon_raw = read_string(&key, "Icon").trim().to_string();
        if !icon_raw.is_empty() {
            if let Some((path, index)) = parse_icon_value(&icon_raw) {
                if let Some(data_uri) = extract_icon_by_index(&path, index) {
                    icon = data_uri;
                }
            }
        }
    }
    (name, command, icon)
}

/// 扫描一个关联类下的 shell verb（可见 + 隐藏）
fn scan_shell_assoc(
    root: &RegKey,
    assoc_path: &str,
    category: &str,
    items: &mut Vec<ContextMenuItem>,
) {
    let shell_path = format!("{assoc_path}\\shell");
    let hidden_path = format!("{assoc_path}\\shell_hidden");

    if let Ok(shell) = root.open_subkey_with_flags(&shell_path, KEY_READ) {
        for k in shell.enum_keys().flatten() {
            let (name, command, icon) = resolve_verb_info(&shell, &k);
            items.push(ContextMenuItem {
                name: if name.is_empty() { k.clone() } else { name },
                verb: k,
                command,
                hive: "HKCR".to_string(),
                category: category.to_string(),
                reg_path: shell_path.clone(),
                icon,
                is_hidden: false,
            });
        }
    }
    if let Ok(hidden) = root.open_subkey_with_flags(&hidden_path, KEY_READ) {
        for k in hidden.enum_keys().flatten() {
            let (name, command, icon) = resolve_verb_info(&hidden, &k);
            items.push(ContextMenuItem {
                name: if name.is_empty() { k.clone() } else { name },
                verb: k,
                command,
                hive: "HKCR".to_string(),
                category: category.to_string(),
                reg_path: shell_path.clone(),
                icon,
                is_hidden: true,
            });
        }
    }
}

/// 扫描系统右键菜单的所有静态 verb 项
#[tauri::command]
pub async fn scan_context_menu_items() -> Result<Vec<ContextMenuItem>, String> {
    let root = RegKey::predef(HKEY_CLASSES_ROOT);
    let mut items: Vec<ContextMenuItem> = Vec::new();
    scan_shell_assoc(&root, "*", "file", &mut items);
    scan_shell_assoc(&root, "Directory", "folder", &mut items);
    scan_shell_assoc(&root, r"Directory\Background", "desktop", &mut items);
    scan_shell_assoc(&root, "Drive", "drive", &mut items);
    scan_shell_assoc(&root, "AllFilesystemObjects", "allFiles", &mut items);
    Ok(items)
}

/// 隐藏（移动到 shell_hidden）：先复制到备份，成功后删除原键
#[tauri::command]
pub async fn hide_context_menu_item(item: ContextMenuItem) -> Result<bool, String> {
    let assoc_path = item
        .reg_path
        .trim_end_matches('\\')
        .trim_end_matches("shell")
        .trim_end_matches('\\');
    let shell_path = item.reg_path.trim_end_matches('\\');
    let hidden_path = format!("{assoc_path}\\shell_hidden");

    let root = RegKey::predef(HKEY_CLASSES_ROOT);
    // 确保备份父键存在
    ensure_subkey(&root, &hidden_path, KEY_SET_VALUE | KEY_CREATE_SUB_KEY | KEY_READ)?;

    // 复制 verb 子树到备份
    copy_key_recursive(
        &root,
        &format!("{shell_path}\\{}", item.verb),
        &root,
        &format!("{hidden_path}\\{}", item.verb),
    )?;
    // 删除原键
    delete_key(&root, shell_path, &item.verb)?;
    Ok(true)
}

/// 恢复（从 shell_hidden 移回 shell）
#[tauri::command]
pub async fn restore_context_menu_item(item: ContextMenuItem) -> Result<bool, String> {
    let assoc_path = item
        .reg_path
        .trim_end_matches('\\')
        .trim_end_matches("shell")
        .trim_end_matches('\\');
    let shell_path = item.reg_path.trim_end_matches('\\');
    let hidden_path = format!("{assoc_path}\\shell_hidden");

    let root = RegKey::predef(HKEY_CLASSES_ROOT);
    // 确保目标 shell 键存在
    ensure_subkey(&root, shell_path, KEY_SET_VALUE | KEY_CREATE_SUB_KEY | KEY_READ)?;

    // 若目标已有同名 verb，清掉
    if root
        .open_subkey_with_flags(&format!("{shell_path}\\{}", item.verb), KEY_READ)
        .is_ok()
    {
        delete_key(&root, shell_path, &item.verb)?;
    }

    // 从备份复制回来
    copy_key_recursive(
        &root,
        &format!("{hidden_path}\\{}", item.verb),
        &root,
        &format!("{shell_path}\\{}", item.verb),
    )?;
    // 删除备份
    delete_key(&root, &hidden_path, &item.verb)?;
    Ok(true)
}

// ---------- 此电脑 NameSpace ----------

fn resolve_pc_root(hive: &str) -> RegKey {
    // "HKLM" / "HKLM-WOW" 都走 HKEY_LOCAL_MACHINE，其余（HKCU）走 HKEY_CURRENT_USER
    if hive.starts_with("HKLM") {
        RegKey::predef(HKEY_LOCAL_MACHINE)
    } else {
        RegKey::predef(HKEY_CURRENT_USER)
    }
}

/// 是否为内部委托名（如 `CLSID_ThisPCLocalDownloadsRegFolder`），跳过不当作显示名。
fn is_internal_name(s: &str) -> bool {
    if s.trim_start().starts_with("CLSID_") {
        return true;
    }
    // 含路径/波浪号也视为非可读名
    s.contains('\\') || s.contains('~')
}

/// 已知的系统「此电脑」文件夹 CLSID → 中文名（不依赖系统 UI 语言）。
/// 键为不含大小写的大括号 GUID（比较时统一转小写）。
/// 同时覆盖 My*（个人）与 Local*（本地）两套 Known Folder，统一映射为同一中文名，
/// 避免出现 "Downloads" 与 "Local Downloads" 等重复英文名。
const KNOWN_PC_FOLDERS: &[(&str, &str)] = &[
    // 3D 对象
    ("{0db7e03f-fc29-4dc6-9020-ff41b59e513a}", "3D 对象"), // 3D Objects
    // 下载（Downloads / Local Downloads）
    ("{374de290-123f-4565-9164-39c4925e467b}", "下载"),
    ("{088e3905-0323-4b02-9826-5d99428e115f}", "下载"), // Local Downloads
    // 图片（My Pictures / Local Pictures）
    ("{3add1653-eb32-4cb0-bbd7-dfa0abb5acca}", "图片"),
    ("{24ad3ad4-a569-4530-98e1-ab02f9417aa8}", "图片"), // Local Pictures
    // 音乐（My Music / Local Music）
    ("{1cf1260c-4dd0-4ebb-811f-33c572699fde}", "音乐"),
    ("{3dfdf296-dbec-4fb4-81d1-6a3438bcf4de}", "音乐"), // Local Music
    // 视频（My Video / Local Videos）
    ("{a0953c92-50dc-43bf-be83-3742fed03c9c}", "视频"),
    ("{f86fa3ab-70d2-4fc7-9c99-fcbf05467f3a}", "视频"), // Local Videos
    // 文档（Personal / Local Documents）
    ("{a8cdff1c-4878-43be-b5fd-f8091c1c60d0}", "文档"),
    ("{d3162b92-9365-467a-956b-92703aca08af}", "文档"), // Local Documents
    // 桌面
    ("{b4bfcc3a-db2c-424c-b029-7fe99a87c641}", "桌面"), // Desktop
];

/// 把解析出的英文名（如 "Downloads"、"Pictures"）汉化为中文，命中失败则原样返回。
fn zh_display_name(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    let map: &[(&str, &str)] = &[
        ("3d objects", "3D 对象"),
        ("3d object", "3D 对象"),
        ("desktop", "桌面"),
        ("desktop folder", "桌面"),
        ("documents", "文档"),
        ("document", "文档"),
        ("downloads", "下载"),
        ("download", "下载"),
        ("music", "音乐"),
        ("pictures", "图片"),
        ("picture", "图片"),
        ("videos", "视频"),
        ("video", "视频"),
        ("recent", "最近使用"),
        ("favorites", "收藏夹"),
        ("network", "网络"),
        ("delegatefolders", "委派文件夹"),
        ("delegate folder", "委派文件夹"),
        ("delegate folders", "委派文件夹"),
    ];
    for (k, v) in map {
        if lower == *k {
            return v.to_string();
        }
    }
    name.to_string()
}

/// 读取 NameSpace 或 NameSpaceHidden 下的一个 CLSID 子键的显示名并汉化。
/// 命名优先级：已知CLSID表 > CLSID.LocalizedString > CLSID.(Default) > NameSpace.(Default)。
fn resolve_clsid_name(root: &RegKey, ns_path: &str, clsid: &str) -> String {
    // 1) 已知 CLSID 直接给中文名
    let lc = clsid.to_lowercase();
    if let Some((_, cn)) = KNOWN_PC_FOLDERS.iter().find(|(c, _)| *c == lc) {
        return cn.to_string();
    }

    let hkcr = RegKey::predef(HKEY_CLASSES_ROOT);
    let clsid_path = format!(r"CLSID\{clsid}");
    if let Ok(k) = hkcr.open_subkey_with_flags(&clsid_path, KEY_READ) {
        // 2) LocalizedString 是真正本地化显示名（多为 @shell32.dll,-xxxx）
        let loc = read_string(&k, "LocalizedString").trim().to_string();
        if !loc.is_empty() {
            let resolved = friendly_name(&loc);
            let resolved = resolved.trim();
            if !resolved.is_empty() && !resolved.starts_with('@') {
                return zh_display_name(resolved);
            }
        }
        // 3) CLSID 的 (Default)
        let def = read_string(&k, "").trim().to_string();
        if !def.is_empty() && !is_internal_name(&def) {
            return zh_display_name(&friendly_name(&def));
        }
    }
    // 4) NameSpace 子键 (Default)
    if let Ok(k) = root.open_subkey_with_flags(&format!("{ns_path}\\{clsid}"), KEY_READ) {
        let def = read_string(&k, "").trim().to_string();
        if !def.is_empty() && !is_internal_name(&def) {
            return zh_display_name(&friendly_name(&def));
        }
    }
    clsid.to_string()
}

/// 是否为系统内置的「此电脑」文件夹（显示在"文件夹"区，而非"设备和驱动器"区）。
/// 用户关心的是"设备和驱动器"区里的第三方项（网盘/虚拟设备等），因此这些系统文件夹应被过滤。
fn is_system_pc_folder(clsid: &str) -> bool {
    let lc = clsid.to_lowercase();
    KNOWN_PC_FOLDERS.iter().any(|(c, _)| *c == lc)
}

/// 是否为内部占位名（解析不出真实名字的壳子项，如 `DelegateFolders`、`CLSID_xx` …）。
/// 这类项不是真实设备或网盘，直接从列表中过滤。
fn is_placeholder_name(name: &str) -> bool {
    let lower = name.trim().to_lowercase();
    matches!(
        lower.as_str(),
        "delegatefolders" | "delegate folder" | "delegate folders" | "clsid"
    ) || lower.starts_with("clsid_")
}

fn scan_ns(root: &RegKey, ns_path: &str, hidden_path: &str, hive: &str, items: &mut Vec<ThisPcItem>) {
    if let Ok(ns) = root.open_subkey_with_flags(ns_path, KEY_READ) {
        for clsid in ns.enum_keys().flatten() {
            if is_system_pc_folder(&clsid) {
                continue; // 跳过系统内置文件夹，只保留"设备和驱动器"区的项
            }
            let name = resolve_clsid_name(root, ns_path, &clsid);
            if is_placeholder_name(&name) {
                continue; // 跳过内部占位名（DelegateFolders 等）
            }
            items.push(ThisPcItem {
                name: if name.is_empty() { clsid.clone() } else { name },
                clsid: clsid.clone(),
                hive: hive.to_string(),
                reg_path: ns_path.to_string(),
                is_hidden: false,
            });
        }
    }
    if let Ok(hidden) = root.open_subkey_with_flags(hidden_path, KEY_READ) {
        for clsid in hidden.enum_keys().flatten() {
            if is_system_pc_folder(&clsid) {
                continue;
            }
            let name = resolve_clsid_name(root, hidden_path, &clsid);
            if is_placeholder_name(&name) {
                continue;
            }
            items.push(ThisPcItem {
                name: if name.is_empty() { clsid.clone() } else { name },
                clsid: clsid.clone(),
                hive: hive.to_string(),
                reg_path: ns_path.to_string(),
                is_hidden: true,
            });
        }
    }
}

const NS_PATH: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\MyComputer\NameSpace";
const NS_HIDDEN: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\MyComputer\NameSpaceHidden";

/// 扫描「此电脑」的 NameSpace 项（HKCU + HKLM，含 Wow6432Node）
#[tauri::command]
pub async fn scan_this_pc_items() -> Result<Vec<ThisPcItem>, String> {
    let mut items: Vec<ThisPcItem> = Vec::new();

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    scan_ns(&hkcu, NS_PATH, NS_HIDDEN, "HKCU", &mut items);

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    scan_ns(&hklm, NS_PATH, NS_HIDDEN, "HKLM", &mut items);

    let wow_path32 = r"Software\Wow6432Node\Microsoft\Windows\CurrentVersion\Explorer\MyComputer\NameSpace";
    let wow_hidden32 = r"Software\Wow6432Node\Microsoft\Windows\CurrentVersion\Explorer\MyComputer\NameSpaceHidden";
    scan_ns(&hklm, wow_path32, wow_hidden32, "HKLM-WOW", &mut items);

    Ok(items)
}

fn move_clsid(hive: &str, current_ns: &str, target_ns: &str, clsid: &str) -> Result<bool, String> {
    let root = resolve_pc_root(hive);
    // 确保目标父键存在
    ensure_subkey(&root, target_ns, KEY_SET_VALUE | KEY_CREATE_SUB_KEY | KEY_READ)?;

    // 若目标已有同名 clsid，清掉
    if root
        .open_subkey_with_flags(&format!("{target_ns}\\{clsid}"), KEY_READ)
        .is_ok()
    {
        delete_key(&root, target_ns, clsid)?;
    }

    copy_key_recursive(
        &root,
        &format!("{current_ns}\\{clsid}"),
        &root,
        &format!("{target_ns}\\{clsid}"),
    )?;
    delete_key(&root, current_ns, clsid)?;
    Ok(true)
}

/// 隐藏「此电脑」项
#[tauri::command]
pub async fn hide_this_pc_item(item: ThisPcItem) -> Result<bool, String> {
    let hidden_ns = item.reg_path.replace("NameSpace", "NameSpaceHidden");
    move_clsid(&item.hive, &item.reg_path, &hidden_ns, &item.clsid)
}

/// 恢复「此电脑」项
#[tauri::command]
pub async fn restore_this_pc_item(item: ThisPcItem) -> Result<bool, String> {
    let hidden_ns = item.reg_path.replace("NameSpace", "NameSpaceHidden");
    move_clsid(&item.hive, &hidden_ns, &item.reg_path, &item.clsid)
}

// ---------- 盘符（设备和驱动器里的磁盘） ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveItem {
    /// 盘符，如 "C:"、"D:"
    pub letter: String,
    /// 卷标（可空）
    pub label: String,
    /// 类型：fixed / removable / cdrom / remote / ram / unknown
    pub drive_type: String,
    /// 是否已通过 NoDrives 策略隐藏
    pub is_hidden: bool,
}

const EXPLORER_POLICY: &str = r"Software\Microsoft\Windows\CurrentVersion\Policies\Explorer";
const NO_DRIVES: &str = "NoDrives";

/// 读取 NoDrives 掩码（bit0=A … bit25=Z，值为 1 表示该盘隐藏）
fn read_no_drives_mask() -> u32 {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    match hkcu.open_subkey_with_flags(EXPLORER_POLICY, KEY_READ) {
        Ok(k) => k.get_value(NO_DRIVES).unwrap_or(0),
        Err(_) => 0,
    }
}

fn write_no_drives_mask(mask: u32) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = ensure_subkey(&hkcu, EXPLORER_POLICY, KEY_SET_VALUE)?;
    if mask == 0 {
        // 全无隐藏则删除该值
        let _ = key.delete_value(NO_DRIVES);
    } else {
        key.set_value(NO_DRIVES, &mask)
            .map_err(|e| format!("Failed to set NoDrives mask: {e}"))?;
    }
    Ok(())
}

fn drive_type_name(t: u32) -> String {
    match t {
        2 => "removable".to_string(), // DRIVE_REMOVABLE
        3 => "fixed".to_string(),     // DRIVE_FIXED
        4 => "remote".to_string(),    // DRIVE_REMOTE
        5 => "cdrom".to_string(),     // DRIVE_CDROM
        6 => "ram".to_string(),       // DRIVE_RAMDISK
        _ => "unknown".to_string(),
    }
}

fn letter_bit(letter: &str) -> Option<u32> {
    let c = letter.trim().trim_end_matches(':').chars().next()?.to_ascii_uppercase();
    if c.is_ascii_alphabetic() {
        Some((c as u32) - (b'A' as u32))
    } else {
        None
    }
}

/// 扫描所有存在的盘符（A:-Z:），并报告是否已隐藏
#[tauri::command]
pub async fn scan_drives() -> Vec<DriveItem> {
    use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;
    let mask = read_no_drives_mask();
    let bits = unsafe { GetLogicalDrives() };
    let mut out: Vec<DriveItem> = Vec::new();
    for i in 0..26u32 {
        if bits & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i as u8) as char;
        let root = format!("{letter}:\\");
        let drive_type = get_drive_type(&root);
        let label = get_volume_label(&root);
        out.push(DriveItem {
            letter: format!("{letter}:"),
            label,
            drive_type: drive_type_name(drive_type),
            is_hidden: (mask >> i) & 1 == 1,
        });
    }
    out
}

#[cfg(target_os = "windows")]
fn get_drive_type(root: &str) -> u32 {
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
    unsafe { GetDriveTypeW(translate_to_wide(root).as_ptr() as *const u16) }
}
#[cfg(not(target_os = "windows"))]
fn get_drive_type(_root: &str) -> u32 {
    0
}

#[cfg(target_os = "windows")]
fn get_volume_label(root: &str) -> String {
    use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationW;
    unsafe {
        let mut name = [0u16; 256];
        let mut fs = [0u16; 64];
        let mut serial = 0u32;
        let mut maxlen = 0u32;
        let mut flags = 0u32;
        let ok = GetVolumeInformationW(
            translate_to_wide(root).as_ptr(),
            name.as_mut_ptr(),
            name.len() as u32,
            &mut serial,
            &mut maxlen,
            &mut flags,
            fs.as_mut_ptr(),
            fs.len() as u32,
        );
        if ok != 0 {
            let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
            translate_from_wide(&name[..end]).trim().to_string()
        } else {
            String::new()
        }
    }
}
#[cfg(not(target_os = "windows"))]
fn get_volume_label(_root: &str) -> String {
    String::new()
}

/// 隐藏指定盘符（写入 NoDrives 位掩码）
#[tauri::command]
pub async fn hide_drive(letter: String) -> Result<bool, String> {
    let bit = letter_bit(&letter).ok_or("Invalid drive letter".to_string())?;
    let mut mask = read_no_drives_mask();
    mask |= 1 << bit;
    write_no_drives_mask(mask)?;
    Ok(true)
}

/// 恢复指定盘符（清除 NoDrives 对应位）
#[tauri::command]
pub async fn restore_drive(letter: String) -> Result<bool, String> {
    let bit = letter_bit(&letter).ok_or("Invalid drive letter".to_string())?;
    let mut mask = read_no_drives_mask();
    mask &= !(1 << bit);
    write_no_drives_mask(mask)?;
    Ok(true)
}