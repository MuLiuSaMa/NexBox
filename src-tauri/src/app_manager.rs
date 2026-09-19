use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use winreg::enums::*;
use winreg::RegKey;

/// 已安装的桌面应用（数据源：注册表 Uninstall 键，Geek Uninstaller 同款）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledApp {
    /// 显示名称（DisplayName，缺失则整项跳过）
    pub name: String,
    pub display_version: Option<String>,
    pub publisher: Option<String>,
    pub install_location: Option<String>,
    pub install_date: Option<String>,
    /// 估计占用大小（注册表 DWORD，单位 KB）
    pub estimated_size_kb: Option<u64>,
    /// DisplayIcon 原始值，可能带 ",N" 图标索引
    pub display_icon: Option<String>,
    /// 解析出的可执行路径（供「打开」使用）
    pub app_path: Option<String>,
    pub uninstall_string: Option<String>,
    pub quiet_uninstall_string: Option<String>,
    /// 来源注册表视图：HKLM64 / HKLM32 / HKCU
    pub registry_source: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppIconInput {
    pub display_icon: Option<String>,
    pub app_path: Option<String>,
}

const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
const UNINSTALL_KEY_32: &str =
    r"Software\Wow6432Node\Microsoft\Windows\CurrentVersion\Uninstall";

/// 文本自然度打分：正常字符加分，控制字符 / 替换符 / 未定义区减分。
/// 用于在多种编码候选（UTF-16LE / UTF-8 / GBK）中选出最像真实文本的解码结果。
fn score_text(s: &str) -> i32 {
    s.chars()
        .map(|c| match c {
            '\u{0}' => -20,
            c if c.is_control() => -8,
            '\u{FFFD}' => -12,
            ' '..='~' => 1,
            c if ('\u{00A0}'..='\u{024F}').contains(&c) => 1, // Latin-1 / Latin-1 扩展
            c if ('\u{2E80}'..='\u{9FFF}').contains(&c) => 1, // CJK 部首 / 汉字
            c if ('\u{3040}'..='\u{30FF}').contains(&c) => 1, // 假名
            c if ('\u{AC00}'..='\u{D7AF}').contains(&c) => 1, // 谚文
            c if ('\u{F900}'..='\u{FAFF}').contains(&c) => 1, // CJK 兼容表意
            c if c.is_alphanumeric() || c.is_whitespace() => 1,
            _ => 0,
        })
        .sum()
}

/// 对注册表字符串的原始字节做智能解码：
/// 标准 REG_SZ 为 UTF-16LE，但部分国产软件写入的 DisplayName 实际是 UTF-8 / GBK 字节
/// （或双重编码），直接按 UTF-16 读取会出现乱码（如"游戏加加"）。
/// 分别按 UTF-16LE / UTF-8 / GBK 解码后打分，取最合理的文本；含 NUL 分隔的
/// REG_MULTI_SZ 只取第一段。返回 None 表示值为空。
fn read_reg_str_raw_decode(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }

    // 候选：(得分, 解码错误数, 偏好序, 文本)
    let mut candidates: Vec<(i32, usize, usize, String)> = Vec::new();

    // UTF-16LE（常规 REG_SZ 形态）
    let mut utf16 = String::new();
    let utf16_err;
    if bytes.len() % 2 == 0 {
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        utf16 = String::from_utf16_lossy(&units);
        utf16_err = utf16.chars().filter(|&c| c == '\u{FFFD}').count();
    } else {
        utf16_err = usize::MAX;
    }
    candidates.push((score_text(&utf16), utf16_err, 2, utf16));

    // UTF-8
    let utf8_lossy = String::from_utf8_lossy(bytes).into_owned();
    let utf8_err = utf8_lossy.chars().filter(|&c| c == '\u{FFFD}').count();
    candidates.push((score_text(&utf8_lossy), utf8_err, 0, utf8_lossy));

    // GBK（cp936，国产老软件常用）
    let (gbk_text, _, gbk_err) = encoding_rs::GBK.decode(bytes);
    let gbk_err = if gbk_err { 1 } else { 0 };
    candidates.push((score_text(&gbk_text), gbk_err, 1, gbk_text.into_owned()));

    candidates.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));

    let mut text = candidates.into_iter().next().map(|(_, _, _, t)| t)?;
    // REG_MULTI_SZ 等以 NUL 分隔的多段值只取第一段
    if let Some(nul) = text.find('\u{0}') {
        text.truncate(nul);
    }
    let text = text.trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

fn read_str(key: &RegKey, name: &str) -> Option<String> {
    let raw = key.get_raw_value(name).ok()?;
    read_reg_str_raw_decode(&raw.bytes)
}

fn read_dword(key: &RegKey, name: &str) -> Option<u32> {
    key.get_value::<u32, _>(name).ok()
}

/// 是否为系统组件（SystemComponent=1 的项不展示，避免误卸载）
fn is_system_component(key: &RegKey) -> bool {
    read_dword(key, "SystemComponent").unwrap_or(0) != 0
}

/// 解析 DisplayIcon：可能形如 `"C:\path\app.exe",0`、`C:\path\app.ico`、`%ProgramFiles%\x\a.exe,-101`。
/// 返回（展开环境变量并去引号后的文件路径, 图标索引）。
fn parse_display_icon(raw: &str) -> (Option<String>, i32) {
    let mut s = raw.trim().trim_matches('"').to_string();
    let mut index = 0;
    if let Some(comma) = s.rfind(',') {
        if let Ok(n) = s[comma + 1..].trim().parse::<i32>() {
            index = n;
            s = s[..comma].to_string();
        }
    }
    s = crate::startup_manager::expand_env_vars(&s);
    s = s.trim().trim_matches('"').to_string();
    (if s.is_empty() { None } else { Some(s) }, index)
}

/// 解析应用可执行路径：优先 DisplayIcon 指向的 exe，其次安装目录下按名称匹配的 exe
fn resolve_app_path(
    display_icon: Option<&str>,
    install_location: Option<&str>,
    name: &str,
) -> Option<String> {
    if let Some(icon) = display_icon {
        if let (Some(file), _) = parse_display_icon(icon) {
            if file.to_lowercase().ends_with(".exe") && PathBuf::from(&file).is_file() {
                return Some(file);
            }
        }
    }
    if let Some(loc) = install_location {
        let dir = PathBuf::from(loc);
        if dir.is_dir() {
            let name_lower = name.to_lowercase();
            if let Ok(entries) = std::fs::read_dir(&dir) {
                let mut exes: Vec<PathBuf> = entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        p.extension()
                            .map(|e| e.to_string_lossy().eq_ignore_ascii_case("exe"))
                            .unwrap_or(false)
                    })
                    .collect();
                if !exes.is_empty() {
                    // 优先文件名包含应用名的 exe，避免选中卸载器/帮助程序
                    exes.sort_by_key(|p| {
                        let stem = p
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_lowercase())
                            .unwrap_or_default();
                        if stem.contains(&name_lower) || name_lower.contains(&stem) {
                            0
                        } else {
                            1
                        }
                    });
                    return exes
                        .into_iter()
                        .next()
                        .map(|p| p.to_string_lossy().to_string());
                }
            }
        }
    }
    None
}

/// 去重键：优先按卸载命令（忽略大小写），否则按（名称,版本）
fn dedupe_key(app: &InstalledApp) -> String {
    match &app.uninstall_string {
        Some(s) => format!(
            "u:{}",
            s.trim().replace(' ', "").to_lowercase()
        ),
        None => format!(
            "n:{}|v:{}",
            app.name.trim().to_lowercase(),
            app.display_version.as_deref().unwrap_or("").trim().to_lowercase()
        ),
    }
}

fn scan_uninstall_key(
    root: RegKey,
    subkey: &str,
    source: &str,
    out: &mut Vec<InstalledApp>,
    seen: &mut HashSet<String>,
) {
    let Ok(parent) = root.open_subkey_with_flags(subkey, KEY_READ) else {
        return;
    };
    for entry in parent.enum_keys() {
        let Ok(sub_name) = entry else { continue };
        let Ok(key) = parent.open_subkey_with_flags(&sub_name, KEY_READ) else {
            continue;
        };
        if is_system_component(&key) {
            continue;
        }
        let Some(name) = read_str(&key, "DisplayName") else {
            continue;
        };
        let display_icon = read_str(&key, "DisplayIcon");
        let install_location = read_str(&key, "InstallLocation");
        let created = InstalledApp {
            name,
            display_version: read_str(&key, "DisplayVersion"),
            publisher: read_str(&key, "Publisher"),
            install_location,
            install_date: read_str(&key, "InstallDate"),
            estimated_size_kb: read_dword(&key, "EstimatedSize").map(|v| v as u64),
            display_icon: display_icon.clone(),
            app_path: None,
            uninstall_string: read_str(&key, "UninstallString"),
            quiet_uninstall_string: read_str(&key, "QuietUninstallString"),
            registry_source: source.to_string(),
        };
        let mut app = created;
        app.app_path = resolve_app_path(
            display_icon.as_deref(),
            app.install_location.as_deref(),
            &app.name,
        );
        if seen.insert(dedupe_key(&app)) {
            out.push(app);
        }
    }
}

/// 扫描本机已安装的传统桌面应用（注册表 Uninstall 键：HKLM 64/32 视图 + HKCU）
#[tauri::command]
pub async fn list_installed_apps() -> Result<Vec<InstalledApp>, String> {
    let mut out: Vec<InstalledApp> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    scan_uninstall_key(
        RegKey::predef(HKEY_LOCAL_MACHINE),
        UNINSTALL_KEY,
        "HKLM64",
        &mut out,
        &mut seen,
    );
    // 64 位系统才存在 Wow6432Node 视图
    #[cfg(target_pointer_width = "64")]
    scan_uninstall_key(
        RegKey::predef(HKEY_LOCAL_MACHINE),
        UNINSTALL_KEY_32,
        "HKLM32",
        &mut out,
        &mut seen,
    );
    scan_uninstall_key(
        RegKey::predef(HKEY_CURRENT_USER),
        UNINSTALL_KEY,
        "HKCU",
        &mut out,
        &mut seen,
    );

    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

/// 批量提取应用图标 data URI（懒加载，失败返回空字符串，前端显示占位）
#[tauri::command]
pub async fn get_app_icons(items: Vec<AppIconInput>) -> Vec<String> {
    items
        .iter()
        .map(|item| {
            let mut icon_path: Option<String> = None;
            let mut icon_index = 0;
            if let Some(icon) = &item.display_icon {
                let (file, idx) = parse_display_icon(icon);
                if file.is_some() {
                    icon_path = file;
                    icon_index = idx;
                }
            }
            let path = icon_path.or_else(|| item.app_path.clone());
            match path {
                Some(p) => crate::startup_manager::extract_icon_data_uri_with_index(&p, icon_index)
                    .unwrap_or_default(),
                None => String::new(),
            }
        })
        .collect()
}

/// 打开应用：优先启动可执行文件；找不到时打开安装目录
#[tauri::command]
pub async fn open_app(app: InstalledApp) -> Result<(), String> {
    if let Some(path) = app.app_path {
        if PathBuf::from(&path).is_file() {
            Command::new(&path)
                .spawn()
                .map_err(|e| format!("启动应用失败: {e}"))?;
            return Ok(());
        }
    }
    if let Some(loc) = app.install_location {
        if PathBuf::from(&loc).is_dir() {
            Command::new("explorer")
                .arg(&loc)
                .spawn()
                .map_err(|e| format!("打开安装目录失败: {e}"))?;
            return Ok(());
        }
    }
    Err("未找到可执行文件或安装目录".to_string())
}

/// 忽略大小写查找子串（返回字节下标；ASCII 大小写转换不改变字节位置）
fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    let lower = haystack.to_lowercase();
    lower.find(&needle.to_lowercase())
}

/// 从命令串拆出（可执行文件, 参数）。
/// 兼容常见形态：
///   1. `"C:\Program Files\App\unins000.exe" /S`（程序带引号）
///   2. `C:\Program Files (x86)\GamePP\uninstall.exe -silent`（程序未引号但路径含空格）
///   3. `MsiExec.exe /X{GUID}` / `rundll32.exe ...`（无空格程序名）
fn split_command(cmd: &str) -> (String, String) {
    let cmd = cmd.trim();
    if cmd.is_empty() {
        return (String::new(), String::new());
    }
    // 形态 1：程序被引号包裹
    if cmd.starts_with('"') {
        if let Some(end) = cmd[1..].find('"') {
            let prog = &cmd[1..1 + end];
            let rest = &cmd[1 + end + 1..];
            return (prog.to_string(), rest.trim().to_string());
        }
    }
    // 形态 2：未引号的完整路径（含空格），按 ".exe"（忽略大小写）结尾切分
    if let Some(idx) = find_ci(cmd, ".exe") {
        let prog_end = idx + 4;
        return (
            cmd[..prog_end].to_string(),
            cmd[prog_end..].trim().to_string(),
        );
    }
    // 形态 3：无空格程序名
    match cmd.find(char::is_whitespace) {
        Some(idx) => (cmd[..idx].to_string(), cmd[idx..].trim().to_string()),
        None => (cmd.to_string(), String::new()),
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 提权启动卸载程序（runas 触发 UAC，与主流卸载工具行为一致）
fn launch_elevated(prog: &str, args: &str) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    unsafe {
        let operation = to_wide("runas");
        let file = to_wide(prog);
        let params = to_wide(args);
        let dir = to_wide("");

        let result = ShellExecuteW(
            HWND::default(),
            PCWSTR(operation.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR(params.as_ptr()),
            PCWSTR(dir.as_ptr()),
            SW_SHOWNORMAL,
        );
        let code = result.0 as isize;
        if code <= 32 {
            return Err(format!("启动卸载程序失败（ShellExecute code={code}）"));
        }
    }
    Ok(())
}

/// 卸载应用：以提权方式调用系统卸载器（quiet=true 时优先静默卸载串）
#[tauri::command]
pub async fn uninstall_app(app: InstalledApp, quiet: bool) -> Result<(), String> {
    // 优先 QuietUninstallString，其次 UninstallString；quiet 标志侧重前者
    let chosen = if quiet {
        app.quiet_uninstall_string
            .clone()
            .or_else(|| app.uninstall_string.clone())
    } else {
        app.uninstall_string
            .clone()
            .or_else(|| app.quiet_uninstall_string.clone())
    };
    let raw = chosen.unwrap_or_default();
    // 个别安装包用换行分隔多个卸载命令，只取第一条
    let first_line = raw.lines().next().unwrap_or_default().trim();
    let expanded = crate::startup_manager::expand_env_vars(first_line);
    if expanded.is_empty() {
        return Err("该应用未提供卸载程序".to_string());
    }
    let (prog, args) = split_command(&expanded);
    if prog.is_empty() {
        return Err("无法解析卸载命令".to_string());
    }
    // 相对路径/裸文件名：尝试在安装目录下解析为绝对路径，
    // 避免 ShellExecute 依赖工作目录而报"文件未找到"
    let (prog, args) = if !Path::new(&prog).is_absolute() {
        match app
            .install_location
            .as_deref()
            .map(str::trim)
            .filter(|l| !l.is_empty())
        {
            Some(loc) => {
                let candidate = Path::new(loc).join(&prog);
                if candidate.is_file() {
                    (candidate.to_string_lossy().to_string(), args)
                } else {
                    (prog, args)
                }
            }
            None => (prog, args),
        }
    } else {
        (prog, args)
    };
    // 绝对路径的卸载程序若磁盘上不存在，直接给出明确提示，避免 ShellExecute 只返回模糊的 SE_ERR_FNF
    if Path::new(&prog).is_absolute() && !Path::new(&prog).exists() {
        log::warn!("[AppManager] 卸载程序不存在: {}", prog);
        return Err(format!("未找到卸载程序：{prog}（应用可能已被卸载）"));
    }
    log::info!(
        "[AppManager] 开始卸载 {} -> {} {}",
        app.name,
        prog,
        args
    );
    launch_elevated(&prog, &args)
}