// ============================================================================
// Winapp2 路径展开器(Rust 移植自 TubaTools PathExpander.cs, 源自
// builtbybel/FluentCleaner, MIT License)
//
// 处理 FileKey 路径的两大难点:
// 1. 展开 %ENVVAR% 标记(winapp2 使用自己的变量子集,不全用系统变量)
// 2. 遍历路径段含 * 通配符的目录树
// ============================================================================

use std::collections::HashSet;
use std::path::Path;

#[derive(Default)]
pub struct PathExpander {
    vars: Vec<(String, String)>,
}

/// 大小写不敏感替换(等价 C# 的 IndexOf+拼接 实现)
fn replace_ignore_case(source: &str, old: &str, new: &str) -> String {
    let mut result = String::new();
    let lower_source = source.to_ascii_lowercase();
    let lower_old = old.to_ascii_lowercase();
    let mut pos = 0;
    while let Some(rel) = lower_source[pos..].find(&lower_old) {
        let idx = pos + rel;
        result.push_str(&source[pos..idx]);
        result.push_str(new);
        pos = idx + old.len();
    }
    result.push_str(&source[pos..]);
    result
}

/// 简单通配符匹配(* / ?),大小写不敏感(Windows 文件名语义)
pub(crate) fn matches_wildcard(pattern: &str, name: &str) -> bool {
    let p = pattern.as_bytes();
    let t = name.as_bytes();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star_pi, mut star_ti) = (usize::MAX, 0usize);

    while ti < t.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi].eq_ignore_ascii_case(&t[ti])) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star_pi = pi;
            star_ti = ti;
            pi += 1;
        } else if star_pi != usize::MAX {
            pi = star_pi + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

fn push_var(vars: &mut Vec<(String, String)>, name: &str, value: Option<String>) {
    if let Some(mut v) = value {
        if !v.is_empty() {
            while v.ends_with('\\') || v.ends_with('/') {
                v.pop();
            }
            vars.push((format!("%{name}%"), v));
        }
    }
}

impl PathExpander {
    pub fn new() -> Self {
        // 系统环境变量优先取 env,保证与 Shell 看到的实际安装位置一致
        let env = |name: &str| std::env::var(name).ok();

        let mut vars: Vec<(String, String)> = Vec::new();
        push_var(&mut vars, "AppData", dirs::data_dir().map(|d| d.to_string_lossy().to_string()));
        push_var(
            &mut vars,
            "LocalAppData",
            dirs::data_local_dir().map(|d| d.to_string_lossy().to_string()),
        );
        push_var(
            &mut vars,
            "LocalLowAppData",
            dirs::data_local_dir().map(|d| {
                d.parent()
                    .map(|p| p.join("LocalLow"))
                    .unwrap_or(d)
                    .to_string_lossy()
                    .to_string()
            }),
        );
        push_var(&mut vars, "ProgramFiles", env("ProgramFiles"));
        push_var(&mut vars, "ProgramFiles(x86)", env("ProgramFiles(x86)"));
        // Winapp2 别名(无括号写法)
        push_var(&mut vars, "ProgramFilesX86", env("ProgramFiles(x86)"));
        push_var(&mut vars, "ProgramData", env("ProgramData"));
        push_var(&mut vars, "CommonAppData", env("ProgramData"));
        push_var(&mut vars, "UserProfile", env("UserProfile"));
        push_var(&mut vars, "Documents", dirs::document_dir().map(|d| d.to_string_lossy().to_string()));
        push_var(&mut vars, "Desktop", dirs::desktop_dir().map(|d| d.to_string_lossy().to_string()));
        push_var(&mut vars, "Music", dirs::audio_dir().map(|d| d.to_string_lossy().to_string()));
        push_var(&mut vars, "Pictures", dirs::picture_dir().map(|d| d.to_string_lossy().to_string()));
        push_var(&mut vars, "Videos", dirs::video_dir().map(|d| d.to_string_lossy().to_string()));
        push_var(&mut vars, "SystemRoot", env("SystemRoot"));
        push_var(&mut vars, "WinDir", env("SystemRoot"));
        push_var(
            &mut vars,
            "System",
            env("SystemRoot").map(|r| format!(r#"{}\System32"#, r.trim_end_matches('\\'))),
        );
        push_var(
            &mut vars,
            "SystemX86",
            env("SystemRoot").map(|r| format!(r#"{}\SysWOW64"#, r.trim_end_matches('\\'))),
        );
        let temp = std::env::temp_dir().to_string_lossy().to_string();
        push_var(&mut vars, "Temp", Some(temp.clone()));
        push_var(&mut vars, "Tmp", Some(temp));
        push_var(
            &mut vars,
            "SystemDrive",
            env("SystemRoot").map(|r| {
                Path::new(&r)
                    .ancestors()
                    .last()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| "C:".to_string())
            }),
        );

        PathExpander { vars }
    }

    /// 替换已知变量,剩余 %VAR% 交由系统环境变量展开;裸盘符 "C:" 补 "\"
    pub fn expand_variables(&self, path: &str) -> String {
        let mut result = path.to_string();
        for (name, value) in &self.vars {
            result = replace_ignore_case(&result, name, value);
        }

        // 让 OS 处理其余未知 %VAR%(等价 Environment.ExpandEnvironmentVariables)
        if result.contains('%') {
            result = expand_remaining_env(&result);
        }

        // 裸盘符("C:" 只有字母+冒号)在 Windows 上表示该盘的当前目录而非根目录,
        // 补上分隔符避免 "C:|..." 这类规则只扫到应用的工作目录。
        let bytes = result.as_bytes();
        if bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            result.push('\\');
        }

        result
    }

    /// 返回模式如 "%LocalAppData%\Google\Chrome*\User Data\*\Cache" 命中的所有实际路径。
    /// Winapp2 用 %ProgramFiles% 同时指代 32/64 位位置,因此自动补扫 x86 变体。
    pub fn resolve_paths(&self, raw_path: &str) -> Vec<String> {
        let mut results: HashSet<String> = HashSet::new();
        resolve_recursive(&self.expand_variables(raw_path), &mut results);

        if raw_path.to_ascii_lowercase().contains("%programfiles%") {
            let x86 = replace_ignore_case(raw_path, "%ProgramFiles%", "%ProgramFiles(x86)%");
            resolve_recursive(&self.expand_variables(&x86), &mut results);
        }

        results.into_iter().collect()
    }
}

/// 递归展开路径中的通配符段
fn resolve_recursive(path: &str, results: &mut HashSet<String>) {
    let parts: Vec<&str> = path.split(['\\', '/']).collect();
    let wc_idx = parts.iter().position(|p| p.contains('*') || p.contains('?'));
    let Some(wc_idx) = wc_idx else {
        // 无通配符:字面路径,原样加入(调用方负责存在性检查)
        results.insert(path.to_string());
        return;
    };

    let base_path = if wc_idx == 0 {
        drive_root(path).unwrap_or_default()
    } else {
        parts[..wc_idx].join("\\")
    };

    if base_path.is_empty() || !Path::new(&base_path).is_dir() {
        return;
    }

    let wildcard = parts[wc_idx];
    let remaining = &parts[wc_idx + 1..];
    let remaining_is_empty = remaining.is_empty();

    let Ok(read_dir) = std::fs::read_dir(&base_path) else {
        return;
    };

    // 只筛选第一条通配符段(后续段交给递归处理)
    let mut matches: Vec<std::path::PathBuf> = Vec::new();
    for entry in read_dir.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !matches_wildcard(wildcard, &name) {
            continue;
        }
        let path_buf = entry.path();
        if remaining_is_empty {
            matches.push(path_buf);
        } else {
            // 后面还有段时只关心目录,并跳过符号链接防环
            if let Ok(ft) = entry.file_type() {
                if ft.is_symlink() {
                    continue;
                }
            }
            if path_buf.is_dir() {
                matches.push(path_buf);
            }
        }
    }

    for m in matches {
        if remaining_is_empty {
            results.insert(m.to_string_lossy().to_string());
        } else {
            let next = m.join(remaining.join("\\"));
            resolve_recursive(&next.to_string_lossy(), results);
        }
    }
}

/// 提取路径中的盘符根("C:\Users\*" -> "C:\")
fn drive_root(path: &str) -> Option<String> {
    let bytes = path.as_bytes();
    if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\' {
        return Some(path[..3].to_string());
    }
    // UNC 根
    if path.starts_with("\\\\") {
        let parts: Vec<&str> = path.split(['\\', '/']).collect();
        if parts.len() >= 4 {
            return Some(format!("\\\\{}\\{}", parts[2], parts[3]));
        }
    }
    None
}

/// 把路径中的 %NAME% 用系统环境变量替换(未知保留)
fn expand_remaining_env(path: &str) -> String {
    let mut result = String::new();
    let bytes = path.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            // 找下一个 %
            if let Some(rel) = path[i + 1..].find('%') {
                let name = &path[i + 1..i + 1 + rel];
                if !name.is_empty() && name.chars().all(|c| !c.is_whitespace()) {
                    if let Ok(value) = std::env::var(name) {
                        result.push_str(&value);
                        i += rel + 2;
                        continue;
                    }
                }
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_structured() -> PathBuf {
        let base = std::env::temp_dir().join(format!("nexbox_winapp2_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("App1\\User Data\\Default\\Cache")).unwrap();
        fs::create_dir_all(base.join("App2\\User Data\\Default\\Cache")).unwrap();
        fs::create_dir_all(base.join("App1\\User Data\\Profile 1\\Cache")).unwrap();
        fs::write(base.join("App1\\User Data\\Default\\Cache\\f1.dat"), b"x").unwrap();
        base
    }

    #[test]
    fn expands_known_vars_case_insensitively() {
        let x = PathExpander::new();
        let temp = std::env::temp_dir();
        assert_eq!(
            Path::new(&x.expand_variables("%temp%\\foo")),
            temp.join("foo")
        );
        // 已知变量未命中时保留字面量
        assert!(x.expand_variables("%NoSuchVar_12345%").contains("%NoSuchVar_12345%"));
    }

    #[test]
    fn expands_bare_drive_root() {
        let x = PathExpander::new();
        let expanded = x.expand_variables("%SystemDrive%");
        // 变量值为 "C"(无尾分隔符)时应补成 "C:\"
        assert!(expanded.len() == 3 && expanded.ends_with('\\'));
    }

    #[test]
    fn resolves_wildcard_paths() {
        let x = PathExpander::new();
        let base = temp_structured();
        let pattern = format!(
            "{}\\App*\\User Data\\Profile 1\\Cache",
            base.to_string_lossy()
        );
        // 通配符只扩展 App1;Profile 1 是字面段(不存在时按 C# 语义原样保留,由扫描端过滤)
        let resolved = x.resolve_paths(&pattern);
        assert!(
            resolved.iter().any(|p| p.ends_with("App1\\User Data\\Profile 1\\Cache")),
            "应命中 App1 的真实目录"
        );

        let pattern2 = format!("{}\\App1\\User Data\\*\\Cache", base.to_string_lossy());
        let resolved2 = x.resolve_paths(&pattern2);
        assert_eq!(resolved2.len(), 2, "Default 与 Profile 1 两个目录都应命中");

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn literal_without_wildcard_returns_as_is() {
        let x = PathExpander::new();
        let temp = std::env::temp_dir().join("nexbox_literal_check");
        let resolved = x.resolve_paths(&temp.to_string_lossy());
        assert_eq!(resolved.len(), 1);
        assert_eq!(Path::new(&resolved[0]), temp);
    }

    #[test]
    fn programfiles_duo_scan() {
        let x = PathExpander::new();
        let stray = std::env::var("ProgramFiles").unwrap_or_default();
        let resolved = x.resolve_paths(&format!("%ProgramFiles%\\__nexbox_probe_{}__", std::process::id()));
        if !stray.is_empty() {
            assert!(!resolved.is_empty());
        }
    }
}