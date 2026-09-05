// ============================================================================
// 注册表残留清理(深度清理专用)
//
// 原则与文件删除引擎一致:多层安全保护,任何失败只记 failure,不重试不级联。
// 仅放行 winapp2 规则明确给出的叶子值/键;系统关键子树一律拦截。
// ============================================================================

use log::{info, warn};
use winreg::enums::*;
use winreg::{HKEY, RegKey};

use super::file_info::{DeleteError, RegistryDeleteTarget};

/// 注册表删除汇总结果
#[derive(Debug, Default)]
pub struct RegistryDeleteOutcome {
    pub success_count: usize,
    pub failed: Vec<DeleteError>,
}

/// 受保护的系统关键段(大小写不敏感,子串匹配)。
/// 命中该列表的子树内,值删除与整键删除一律拒绝。
const BLOCKED_SEGMENTS: &[&str] = &[
    "\\microsoft\\windows nt\\",
    "\\microsoft\\windows nt",
    "\\policies\\",
    "\\microsoft\\windows\\currentversion\\policies",
    "\\microsoft\\windows\\currentversion\\runonce",
    "\\microsoft\\office\\recent",
];

/// 删除一组注册表目标。仅允许 HKCU 与 HKLM\SOFTWARE 子树;键不存在视为成功跳过。
pub fn delete_registry_items(items: &[RegistryDeleteTarget]) -> RegistryDeleteOutcome {
    let mut outcome = RegistryDeleteOutcome::default();
    for item in items {
        let key_path = item.key_path.trim().trim_end_matches('\\');
        if key_path.is_empty() {
            continue;
        }
        match delete_one(&item.key_path, item.value_name.as_deref()) {
            Ok(()) => {
                outcome.success_count += 1;
                info!("注册表清理成功: {}", &item.key_path);
            }
            Err(reason) => {
                warn!("注册表清理失败: {} - {}", item.key_path, reason);
                outcome.failed.push(DeleteError {
                    path: item.key_path.clone(),
                    reason,
                });
            }
        }
    }
    outcome
}

/// 删除单个注册表目标(值或整个键)
fn delete_one(key_path: &str, value_name: Option<&str>) -> Result<(), String> {
    let (hive, sub_key) = split_registry_path(key_path)?;
    if sub_key.is_empty() {
        return Err("注册表路径缺少子键".to_string());
    }
    let sub_lower = sub_key.to_ascii_lowercase();

    // ---- HIVE 白名单:仅 HKCU 与 HKLM\SOFTWARE ----
    if hive == "HKLM" && !sub_lower.starts_with("software\\") {
        return Err("仅允许清理 HKLM\\SOFTWARE 子树,已拦截".to_string());
    }

    // ---- 关键子树拦截(值删除与整键删除均拒绝) ----
    if BLOCKED_SEGMENTS.iter().any(|seg| sub_lower.contains(seg)) {
        return Err(format!("系统关键注册表子树,已拦截: {}", key_path));
    }

    match value_name {
        // ---- 删除单个值 ----
        Some(name) => {
            let predef = hive_predef(&hive)?;
            if name.is_empty() {
                return Err("值名为空".to_string());
            }
            let key = RegKey::predef(predef)
                .open_subkey_with_flags(&sub_key, KEY_READ | KEY_WRITE)
                .map_err(|e| format!("打开注册表键失败({}): {}", key_path, e))?;
            if key.get_raw_value(name).is_err() {
                // 值已不存在,视为成功
                return Ok(());
            }
            key.delete_value(name)
                .map_err(|e| format!("删除值失败({}): {}", key_path, e))?;
            Ok(())
        }
        // ---- 删除整个键(含其下所有子键) ----
        None => {
            let Some((parent_sub, key_name)) = split_parent(&sub_key) else {
                return Err("不支持删除注册表根键".to_string());
            };
            // 整键删除额外红线:必须位于 SOFTWARE 之下且名称不是系统启动键
            let key_name_lower = key_name.to_ascii_lowercase();
            if key_name_lower == "run" || key_name_lower == "runonce" {
                return Err(format!("不允许整体删除系统启动键,已拦截: {}", key_path));
            }
            let predef = hive_predef(&hive)?;
            // 打开父键前先确认目标键存在
            let exists = RegKey::predef(predef)
                .open_subkey(&sub_key)
                .map(|_| true)
                .unwrap_or(false);
            if !exists {
                // 键已不存在,视为成功跳过
                return Ok(());
            }
            let parent_key = if parent_sub.is_empty() {
                RegKey::predef(predef)
            } else {
                RegKey::predef(predef)
                    .open_subkey_with_flags(parent_sub, KEY_READ | KEY_WRITE)
                    .map_err(|e| format!("打开父键失败({}): {}", key_path, e))?
            };
            parent_key
                .delete_subkey_all(key_name)
                .map_err(|e| format!("删除键失败({}): {}", key_path, e))?;
            Ok(())
        }
    }
}

/// 拆分 "HIVE\子键" → (HIVE 大写, 子键)
fn split_registry_path(key_path: &str) -> Result<(String, String), String> {
    let key_path = key_path.trim();
    let Some(slash) = key_path.find('\\') else {
        return Ok((key_path.to_ascii_uppercase(), String::new()));
    };
    Ok((
        key_path[..slash].to_ascii_uppercase(),
        key_path[slash + 1..].to_string(),
    ))
}

/// 把 "A\B\C" 拆成父键 "A\B" 与键名 "C"
fn split_parent(sub_key: &str) -> Option<(String, String)> {
    let rslash = sub_key.rfind('\\')?;
    Some((
        sub_key[..rslash].to_string(),
        sub_key[rslash + 1..].to_string(),
    ))
}

/// 仅放行 HKCU 与 HKLM(白名单在上层校验子树)
fn hive_predef(hive: &str) -> Result<HKEY, String> {
    match hive {
        "HKCU" | "HKEY_CURRENT_USER" => Ok(HKEY_CURRENT_USER),
        "HKLM" | "HKEY_LOCAL_MACHINE" => Ok(HKEY_LOCAL_MACHINE),
        "HKU" | "HKEY_USERS" | "HKCR" | "HKEY_CLASSES_ROOT" | "HKCC" | "HKEY_CURRENT_CONFIG" => {
            Err(format!("不允许清理注册表单元: {}", hive))
        }
        _ => Err(format!("未知注册表单元: {}", hive)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_registry_key_path() {
        let (hive, sub) = split_registry_path(r"HKCU\Software\Foo\Bar").unwrap();
        assert_eq!(hive, "HKCU");
        assert_eq!(sub, r"Software\Foo\Bar");

        let (parent, name) = split_parent(r"Software\Foo\Bar").unwrap();
        assert_eq!(parent, r"Software\Foo");
        assert_eq!(name, "Bar");
    }

    #[test]
    fn hklm_whitelist() {
        let evil = delete_one(r"HKLM\SYSTEM\CurrentControlSet\Services\Foo", None);
        assert!(evil.is_err());

        let evil = delete_one(r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Foo", None);
        assert!(evil.is_err(), "Policies 子树必须拦截");

        let evil = delete_one(r"HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce\Foo", None);
        assert!(evil.is_err(), "RunOnce 子树必须拦截");

        let evil = delete_one(r"HKCU\Software\Foo\Run", None);
        assert!(evil.is_err(), "键名为 Run 的整体删除必须拦截");
    }

    #[test]
    fn unknown_hive_rejected() {
        let r = delete_one(r"HKCR\Software\Foo", None);
        assert!(r.is_err());
        let r = delete_one(r"HKU\Software\Foo", Some("v"));
        assert!(r.is_err());
    }
}