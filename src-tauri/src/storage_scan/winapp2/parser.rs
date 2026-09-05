// ============================================================================
// Winapp2.ini 解析器(Rust 移植自 TubaTools Winapp2Parser.cs, 后者源自
// builtbybel/FluentCleaner, MIT License)
//
// Winapp2.ini 是 CCleaner 社区维护的垃圾清理规则库(CC-BY-SA-4.0)。
// 格式要点:
//   [条目名 *]           社区条目标记 "*" 解析时去除
//   FileKeyN=路径|模式|标志   标志可为 RECURSE / REMOVESELF
//   RegKeyN=HIVE\子键|值名    值名省略表示删除整个键
//   ExcludeKeyN=TYPE|路径|模式 TYPE ∈ FILE/PATH/REG
//   DetectN / DetectFileN    安装检测(OR 逻辑)
// ============================================================================

/// FileKey 目录扫描方式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKeyFlag {
    /// 仅顶层文件
    None,
    /// 递归扫描子目录
    Recurse,
    /// 递归扫描,删除后修剪空目录(= 把匹配目录整体作为删除目标)
    RemoveSelf,
}

/// 一条 FileKeyN= 解析结果(路径|模式|[标志])
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileKeyEntry {
    /// 目录路径,可含 %ENVVAR% 与路径段通配符
    pub path: String,
    /// 分号分隔的文件过滤模式,缺省 "*.*"
    pub pattern: String,
    pub flag: FileKeyFlag,
}

impl FileKeyEntry {
    /// 解析 "路径|模式|标志" 三段式,兼容 C# 原实现的三种写法:
    ///   path|pattern
    ///   path|FLAG
    ///   path|pattern|FLAG
    pub fn parse(value: &str) -> Self {
        let parts: Vec<&str> = value.split('|').collect();
        let mut entry = FileKeyEntry {
            path: parts[0].trim().to_string(),
            pattern: "*.*".to_string(),
            flag: FileKeyFlag::None,
        };

        match parts.len() {
            2 => {
                let p = parts[1].trim().to_uppercase();
                if p == "RECURSE" || p == "REMOVESELF" {
                    entry.flag = if p == "RECURSE" {
                        FileKeyFlag::Recurse
                    } else {
                        FileKeyFlag::RemoveSelf
                    };
                } else {
                    entry.pattern = parts[1].trim().to_string();
                }
            }
            n if n > 2 => {
                entry.pattern = parts[1].trim().to_string();
                entry.flag = match parts[2].trim().to_uppercase().as_str() {
                    "RECURSE" => FileKeyFlag::Recurse,
                    "REMOVESELF" => FileKeyFlag::RemoveSelf,
                    _ => FileKeyFlag::None,
                };
            }
            _ => {}
        }

        entry
    }
}

/// 一条 RegKeyN= 解析结果(HIVE\子键|[值名])
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegKeyEntry {
    pub key_path: String,
    pub value_name: Option<String>,
}

impl RegKeyEntry {
    pub fn parse(value: &str) -> Self {
        let Some(pipe) = value.rfind('|') else {
            return RegKeyEntry {
                key_path: value.trim().trim_end_matches('\\').to_string(),
                value_name: None,
            };
        };
        RegKeyEntry {
            key_path: value[..pipe].trim().trim_end_matches('\\').to_string(),
            value_name: Some(value[pipe + 1..].trim().to_string()),
        }
    }
}

/// ExcludeKey 保护的资源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExcludeType {
    /// 目录内特定文件或文件名模式
    File,
    /// 整个目录子树
    Path,
    /// 注册表键/值(文件扫描阶段忽略)
    Reg,
}

/// 一条 ExcludeKeyN= 解析结果(TYPE|路径|[文件名模式])
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExcludeKeyEntry {
    pub ex_type: ExcludeType,
    pub path: String,
    pub pattern: Option<String>,
}

impl ExcludeKeyEntry {
    pub fn parse(value: &str) -> Self {
        let parts: Vec<&str> = value.split('|').collect();
        let mut entry = ExcludeKeyEntry {
            ex_type: ExcludeType::File,
            path: String::new(),
            pattern: None,
        };
        if !parts.is_empty() {
            entry.ex_type = match parts[0].trim().to_uppercase().as_str() {
                "FILE" => ExcludeType::File,
                "PATH" => ExcludeType::Path,
                "REG" => ExcludeType::Reg,
                _ => ExcludeType::File,
            };
        }
        if parts.len() > 1 {
            entry.path = parts[1].trim().to_string();
        }
        if parts.len() > 2 {
            entry.pattern = Some(parts[2].trim().to_string());
        }
        entry
    }
}

/// Winapp2.ini 单条规则
#[derive(Debug, Clone)]
pub struct CleanerEntry {
    /// 界面显示名(如 "Microsoft Edge")
    pub name: String,
    /// 可选自由格式分类名
    pub section: Option<String>,
    /// 分类代码(如 3025 = Windows),当前仅保留
    pub lang_sec_ref: Option<i32>,
    pub detect_keys: Vec<String>,
    pub detect_files: Vec<String>,
    /// 知名应用缩写码(如 "DET_CHROME")
    pub special_detect: Option<String>,
    pub file_keys: Vec<FileKeyEntry>,
    pub reg_keys: Vec<RegKeyEntry>,
    pub exclude_keys: Vec<ExcludeKeyEntry>,
    pub warning: Option<String>,
    /// 是否默认勾选(ini 中显式 "Default=False" 才为 false)
    pub default_select: bool,
}

impl CleanerEntry {
    /// 仅当能检测且存在可清理内容时才有用
    pub fn is_valid(&self) -> bool {
        (!self.detect_keys.is_empty()
            || !self.detect_files.is_empty()
            || self.special_detect.is_some())
            && (!self.file_keys.is_empty() || !self.reg_keys.is_empty())
    }
}

/// 键名匹配:FileKeyN / RegKeyN / ExcludeKeyN / DetectN / DetectFileN(N 可省略)
fn key_matches_prefix(line: &str, prefix: &str) -> bool {
    let rest = match line.strip_prefix(prefix) {
        Some(r) => r,
        None => return false,
    };
    rest.is_empty() || rest.chars().all(|c| c.is_ascii_digit())
}

/// 解析 Winapp2.ini / Winappx.ini 风格内容为 CleanerEntry 列表
pub fn parse(content: &str) -> Vec<CleanerEntry> {
    let mut entries: Vec<CleanerEntry> = Vec::new();
    let mut current: Option<CleanerEntry> = None;

    for line in content.split(|c| c == '\r' || c == '\n') {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            if let Some(entry) = current.take() {
                if entry.is_valid() {
                    entries.push(entry);
                }
            }

            let name = line[1..line.len() - 1].trim();
            // 跳过文件自身的头部块
            if name.to_ascii_lowercase().starts_with("winapp2")
                || name.to_ascii_lowercase().starts_with("version")
            {
                continue;
            }
            // 去掉社区条目的尾部 "*"
            let name = name.trim_end_matches('*').trim_end().to_string();
            current = Some(CleanerEntry {
                name,
                section: None,
                lang_sec_ref: None,
                detect_keys: Vec::new(),
                detect_files: Vec::new(),
                special_detect: None,
                file_keys: Vec::new(),
                reg_keys: Vec::new(),
                exclude_keys: Vec::new(),
                warning: None,
                default_select: true,
            });
            continue;
        }

        let Some(entry) = current.as_mut() else {
            continue;
        };

        let Some(eq_idx) = line.find('=') else {
            continue;
        };
        let key = line[..eq_idx].trim();
        let value = line[eq_idx + 1..].trim();
        if value.is_empty() {
            continue;
        }

        let upper_key = key.to_ascii_uppercase();
        match upper_key.as_str() {
            "LANGSECREF" => {
                if let Ok(n) = value.parse::<i32>() {
                    entry.lang_sec_ref = Some(n);
                }
            }
            "SECTION" => entry.section = Some(value.to_string()),
            "SPECIALDETECT" => entry.special_detect = Some(value.to_string()),
            "WARNING" => entry.warning = Some(value.to_string()),
            "DEFAULT" => {
                entry.default_select = !value.eq_ignore_ascii_case("False");
            }
            _ => {
                if key_matches_prefix(upper_key.as_str(), "DETECTFILE") {
                    entry.detect_files.push(value.to_string());
                } else if key_matches_prefix(upper_key.as_str(), "DETECT") {
                    entry.detect_keys.push(value.to_string());
                } else if key_matches_prefix(upper_key.as_str(), "FILEKEY") {
                    entry.file_keys.push(FileKeyEntry::parse(value));
                } else if key_matches_prefix(upper_key.as_str(), "REGKEY") {
                    entry.reg_keys.push(RegKeyEntry::parse(value));
                } else if key_matches_prefix(upper_key.as_str(), "EXCLUDEKEY") {
                    entry.exclude_keys.push(ExcludeKeyEntry::parse(value));
                }
            }
        }
    }

    if let Some(entry) = current {
        if entry.is_valid() {
            entries.push(entry);
        }
    }

    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
; comment line
[Winapp2 Example]
LangSecRef=3029
[Google Chrome Caches *]
LangSecRef=3029
DetectFile=%LocalAppData%\\Google\\Chrome*
FileKey1=%LocalAppData%\\Google\\Chrome*\\User Data|*-journal|RECURSE
FileKey2=%LocalAppData%\\Google\\Chrome*\\User Data\\*\\*Cache*|*|REMOVESELF
RegKey1=HKCU\\Software\\Google\\Chrome\\Something|value1
[Google Chrome Pinned Tabs *]
Default=False
DetectFile=%LocalAppData%\\Google\\Chrome*
FileKey1=%LocalAppData%\\Google\\Chrome*\\User Data\\*|Bookmarks.bak
[Empty Entry *]
DetectFile=%LocalAppData%\\NothingHere*
";

    #[test]
    fn parses_sections_and_keys() {
        let entries = parse(SAMPLE);
        assert_eq!(entries.len(), 2, "头部块与无效条目应被跳过");

        let chrome = &entries[0];
        assert_eq!(chrome.name, "Google Chrome Caches");
        assert_eq!(chrome.lang_sec_ref, Some(3029));
        assert_eq!(chrome.detect_files.len(), 1);
        assert_eq!(chrome.file_keys.len(), 2);
        assert_eq!(chrome.reg_keys.len(), 1);
        assert_eq!(chrome.default_select, true);
        assert_eq!(chrome.detect_keys.len(), 0);

        let pinned = &entries[1];
        assert_eq!(pinned.name, "Google Chrome Pinned Tabs");
        assert_eq!(pinned.default_select, false);
    }

    #[test]
    fn parses_file_key_three_part_formats() {
        // path|pattern
        let a = FileKeyEntry::parse("%LocalAppData%\\X|*.tmp;*.log");
        assert_eq!(a.pattern, "*.tmp;*.log");
        assert_eq!(a.flag, FileKeyFlag::None);

        // path|FLAG (无模式)
        let b = FileKeyEntry::parse("%LocalAppData%\\X|RECURSE");
        assert_eq!(b.pattern, "*.*");
        assert_eq!(b.flag, FileKeyFlag::Recurse);

        // path|pattern|FLAG
        let c = FileKeyEntry::parse("%LocalAppData%\\X|*|REMOVESELF");
        assert_eq!(c.pattern, "*");
        assert_eq!(c.flag, FileKeyFlag::RemoveSelf);

        // 默认模式
        let d = FileKeyEntry::parse("%LocalAppData%\\X");
        assert_eq!(d.pattern, "*.*");
        assert_eq!(d.flag, FileKeyFlag::None);
    }

    #[test]
    fn parses_reg_key_and_exclude_key() {
        let reg = RegKeyEntry::parse(r"HKCU\Software\Foo");
        assert_eq!(reg.key_path, r"HKCU\Software\Foo");
        assert_eq!(reg.value_name, None);

        let reg2 = RegKeyEntry::parse(r"HKCU\Software\Foo|value_name");
        assert_eq!(reg2.key_path, r"HKCU\Software\Foo");
        assert_eq!(reg2.value_name.as_deref(), Some("value_name"));

        let ex1 = ExcludeKeyEntry::parse(r"FILE|%AppData%\Mozilla\Firefox\Profiles\|places.sqlite");
        assert_eq!(ex1.ex_type, ExcludeType::File);
        assert!(ex1.path.contains("Firefox\\Profiles\\"));
        assert_eq!(ex1.pattern.as_deref(), Some("places.sqlite"));

        let ex2 = ExcludeKeyEntry::parse(r"PATH|%AppData%\Foo");
        assert_eq!(ex2.ex_type, ExcludeType::Path);
        assert_eq!(ex2.pattern, None);

        let ex3 = ExcludeKeyEntry::parse(r"REG|HKCU\Software\Foo");
        assert_eq!(ex3.ex_type, ExcludeType::Reg);
    }

    #[test]
    fn skips_invalid_and_header_blocks() {
        let entries = parse("[Winapp2 data]\nSomeKey=1\n\n[No Detect]\nFileKey1=X|*.*\n[No Keys]\nDetect=HKLM\\Software\\Microsoft\\Windows\n[Valid]\nDetect=HKLM\\Software\\Microsoft\\Windows\nFileKey1=%Temp%\\*.tmp");
        // 头部块跳过;缺检测的条目无效;缺可清理键的条目无效
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Valid");
    }
}