// ============================================================================
// 安装检测(Rust 移植自 TubaTools DetectionService.cs, 源自
// builtbybel/FluentCleaner, MIT License)
//
// 回答"这个应用/规则是否适用本机":
// 多条 Detect / DetectFile 使用 OR 逻辑,命中一条即可。
// SpecialDetect 是 winapp2 对知名应用的缩写码。
// ============================================================================

use super::parser::CleanerEntry;
use super::path_expander::PathExpander;
use winreg::enums::*;
use winreg::{HKEY, RegKey};

pub struct DetectionService {
    expander: PathExpander,
}

impl DetectionService {
    pub fn new() -> Self {
        DetectionService {
            expander: PathExpander::new(),
        }
    }

    pub fn is_installed(&self, entry: &CleanerEntry) -> bool {
        if let Some(code) = &entry.special_detect {
            // 已知码直接信任其结果;未知码回退到 Detect/DetectFile
            if let Some(result) = self.try_check_special_detect(code) {
                return result;
            }
        }

        for reg in &entry.detect_keys {
            if check_registry(reg) {
                return true;
            }
        }
        for file in &entry.detect_files {
            if self.check_file(file) {
                return true;
            }
        }
        false
    }

    fn check_file(&self, raw_path: &str) -> bool {
        let expanded = self.expander.expand_variables(raw_path);
        if expanded.contains('*') || expanded.contains('?') {
            return !self.expander.resolve_paths(raw_path).is_empty();
        }
        std::path::Path::new(&expanded).exists()
    }

    /// 处理 SpecialDetect 缩写码:已知码返回 Some(结果),未知码返回 None(交给下方检测)。
    fn try_check_special_detect(&self, code: &str) -> Option<bool> {
        match code.to_ascii_uppercase().as_str() {
            "DET_CHROME" => Some(self.check_file(r"%LocalAppData%\Google\Chrome\User Data")),
            "DET_FIREFOX" => Some(self.check_file(r"%AppData%\Mozilla\Firefox")),
            "DET_IE" => Some(check_registry(
                r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\IEXPLORE.EXE",
            )),
            "DET_THUNDERBIRD" => Some(self.check_file(r"%AppData%\Thunderbird")),
            "DET_OPERA" => Some(self.check_file(r"%AppData%\Opera Software\Opera Stable")),
            "DET_EDGE" => Some(self.check_file(r"%LocalAppData%\Microsoft\Edge\User Data")),
            "DET_WINSTORE" => {
                // 每个 Win10+ 机器都有 Packages 目录,Store 可用
                Some(self.check_file(r"%LocalAppData%\Packages"))
            }
            _ => None,
        }
    }
}

impl Default for DetectionService {
    fn default() -> Self {
        Self::new()
    }
}

/// 拆分注册表路径 "HIVE\子键|值名" → (hive, 子键, 值名)
fn split_reg_path(path: &str) -> (String, String, Option<String>) {
    let mut reg_path = path;
    let mut value_name = None;

    if let Some(pipe) = path.rfind('|') {
        reg_path = &path[..pipe];
        value_name = Some(path[pipe + 1..].to_string());
    }

    match reg_path.find('\\') {
        Some(slash) => (
            reg_path[..slash].to_ascii_uppercase(),
            reg_path[slash + 1..].to_string(),
            value_name,
        ),
        None => (reg_path.to_ascii_uppercase(), String::new(), value_name),
    }
}

fn hive_to_predef(hive: &str) -> Option<HKEY> {
    Some(match hive {
        "HKLM" | "HKEY_LOCAL_MACHINE" => HKEY_LOCAL_MACHINE,
        "HKCU" | "HKEY_CURRENT_USER" => HKEY_CURRENT_USER,
        "HKU" | "HKEY_USERS" => HKEY_USERS,
        "HKCR" | "HKEY_CLASSES_ROOT" => HKEY_CLASSES_ROOT,
        "HKCC" | "HKEY_CURRENT_CONFIG" => HKEY_CURRENT_CONFIG,
        _ => return None,
    })
}

/// 判断注册表键/值是否存在(键不存在或值不存在均返回 false)
fn check_registry(reg_path: &str) -> bool {
    let (hive, sub_key, value_name) = split_reg_path(reg_path);
    reg_key_present(&hive, &sub_key, value_name.as_deref())
}

fn reg_key_present(hive: &str, sub_key: &str, value_name: Option<&str>) -> bool {
    let Some(predef) = hive_to_predef(hive) else {
        return false;
    };
    let Ok(key) = RegKey::predef(predef).open_subkey(sub_key) else {
        return false;
    };
    match value_name {
        None => true, // 键存在即可
        Some(name) => key.get_raw_value(name).is_ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_registry_path() {
        let (hive, sub, value) = split_reg_path(r"HKCU\Software\Foo");
        assert_eq!(hive, "HKCU");
        assert_eq!(sub, r"Software\Foo");
        assert_eq!(value, None);

        let (hive, sub, value) = split_reg_path(r"hklm\Software\Foo|my_value");
        assert_eq!(hive, "HKLM");
        assert_eq!(sub, r"Software\Foo");
        assert_eq!(value.as_deref(), Some("my_value"));

        let (hive, sub, _) = split_reg_path("HKLM");
        assert_eq!(hive, "HKLM");
        assert_eq!(sub, "");
    }

    #[test]
    fn detects_existing_temp_file() {
        let svc = DetectionService::new();
        let probe = std::env::temp_dir().join("nexbox_detection_probe");
        let _ = std::fs::File::create(&probe);
        assert!(svc.check_file(&probe.to_string_lossy()));

        let entry = crate::storage_scan::winapp2::parser::CleanerEntry {
            name: "Test".into(),
            section: None,
            lang_sec_ref: None,
            detect_keys: vec![],
            detect_files: vec![probe.to_string_lossy().to_string()],
            special_detect: None,
            file_keys: vec![],
            reg_keys: vec![],
            exclude_keys: vec![],
            warning: None,
            default_select: true,
        };
        assert!(svc.is_installed(&entry));
        let _ = std::fs::remove_file(&probe);
    }

    #[test]
    fn special_detect_mapping() {
        let svc = DetectionService::new();
        assert_eq!(svc.try_check_special_detect("DET_UNKNOWN_CODE"), None);
        assert_eq!(svc.try_check_special_detect("det_ie"), Some(true)); // IE 注册表路径通常存在
        // 大小写不敏感
        assert!(svc.try_check_special_detect("det_chrome").is_some());
    }
}