//! CPU 改名（仅供娱乐）
//!
//! 原理：Windows 任务管理器 / 系统属性 / 部分软件显示 CPU 名称时，读的是
//! `HKLM\HARDWARE\DESCRIPTION\System\CentralProcessor\N\ProcessorNameString`。
//! 这里改写该字符串即可让系统里显示成任意名字。
//!
//! 注意：
//! - 不改动任何真实硬件信息，也不影响 CPU 的实际性能；
//! - `HKLM\HARDWARE` 是**易失性**键，重启后由内核重新枚举生成，名称会自动还原；
//! - 写入需要管理员权限，非管理员会返回明确错误；
//! - 改写前会把原始名称备份到 `%APPDATA%\NexBox\cpu_rename_backup.json`，支持一键还原。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[cfg(target_os = "windows")]
use winreg::enums::*;
#[cfg(target_os = "windows")]
use winreg::RegKey;

#[cfg(target_os = "windows")]
const CPU_ROOT: &str = r"HARDWARE\DESCRIPTION\System\CentralProcessor";
const VALUE_NAME: &str = "ProcessorNameString";
/// 名称长度上限：注册表该值是 REG_SZ，任务管理器显示区域也有限，过长会被截断
const MAX_NAME_LEN: usize = 64;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CpuInfo {
    /// 当前显示的 CPU 名称
    pub name: String,
    /// 备份的原始名称（已备份时才有）
    pub original_name: Option<String>,
    /// 是否存在可还原的备份
    pub is_backed_up: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CpuRenameResult {
    pub success: bool,
    pub message: String,
}

fn backup_path() -> Result<PathBuf, String> {
    let dir = dirs::config_dir()
        .ok_or("无法获取 APPDATA 目录")?
        .join("NexBox");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建数据目录失败: {}", e))?;
    Ok(dir.join("cpu_rename_backup.json"))
}

/// 备份结构：注册表子键序号 -> 原始名称
fn read_backup() -> BTreeMap<String, String> {
    let Ok(path) = backup_path() else {
        return BTreeMap::new();
    };
    let Ok(content) = std::fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    serde_json::from_str::<BTreeMap<String, String>>(&content).unwrap_or_default()
}

fn write_backup(map: &BTreeMap<String, String>) -> Result<(), String> {
    let path = backup_path()?;
    let json = serde_json::to_string_pretty(map).map_err(|e| format!("序列化备份失败: {}", e))?;
    std::fs::write(path, json).map_err(|e| format!("写入备份失败: {}", e))
}

/// 枚举 `CentralProcessor` 下的所有子键（0、1、2… 每个逻辑处理器一个）
#[cfg(target_os = "windows")]
fn cpu_subkeys() -> Result<Vec<String>, String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let root = hklm
        .open_subkey_with_flags(CPU_ROOT, KEY_READ)
        .map_err(|e| format!("打开 CPU 注册表键失败: {}", e))?;
    let mut keys: Vec<String> = root
        .enum_keys()
        .filter_map(|k| k.ok())
        .collect();
    keys.sort_by_key(|k| k.parse::<u32>().unwrap_or(u32::MAX));
    if keys.is_empty() {
        return Err("未找到 CPU 注册表子键".to_string());
    }
    Ok(keys)
}

#[cfg(target_os = "windows")]
fn read_name_of(hklm: &RegKey, sub: &str) -> Option<String> {
    hklm.open_subkey_with_flags(format!("{}\\{}", CPU_ROOT, sub), KEY_READ)
        .ok()?
        .get_value::<String, _>(VALUE_NAME)
        .ok()
}

#[tauri::command]
pub fn get_cpu_rename_info() -> Result<CpuInfo, String> {
    #[cfg(target_os = "windows")]
    {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let keys = cpu_subkeys()?;
        let current = read_name_of(&hklm, &keys[0]).unwrap_or_else(|| "未知 CPU".to_string());
        let backup = read_backup();
        let original = backup.get(&keys[0]).cloned();
        Ok(CpuInfo {
            name: current,
            is_backed_up: !backup.is_empty(),
            original_name: original,
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("仅支持 Windows".to_string())
    }
}

/// 应用新名称：写入所有逻辑处理器子键，首次写入前记录原始名称
#[tauri::command]
pub fn apply_cpu_rename(new_name: String) -> Result<CpuRenameResult, String> {
    let trimmed = new_name.trim().to_string();
    if trimmed.is_empty() {
        return Err("名称不能为空".to_string());
    }
    if trimmed.chars().count() > MAX_NAME_LEN {
        return Err(format!("名称过长（最多 {} 个字符）", MAX_NAME_LEN));
    }

    #[cfg(target_os = "windows")]
    {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let keys = cpu_subkeys()?;
        let mut backup = read_backup();
        let mut written = 0usize;

        for sub in &keys {
            let Ok(key) = hklm.open_subkey_with_flags(
                format!("{}\\{}", CPU_ROOT, sub),
                KEY_READ | KEY_WRITE,
            ) else {
                continue;
            };
            // 只在首次改写时记录原始名称，避免把「改过的名字」当成原始名称备份
            if !backup.contains_key(sub) {
                if let Ok(origin) = key.get_value::<String, _>(VALUE_NAME) {
                    backup.insert(sub.clone(), origin);
                }
            }
            if key.set_value(VALUE_NAME, &trimmed).is_ok() {
                written += 1;
            }
        }

        if written == 0 {
            return Err("写入失败：需要管理员权限，请以管理员身份运行 NexBox".to_string());
        }
        write_backup(&backup)?;
        Ok(CpuRenameResult {
            success: true,
            message: format!("已改名为「{}」（{} 个处理器）", trimmed, written),
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("仅支持 Windows".to_string())
    }
}

/// 还原为备份中的原始名称
#[tauri::command]
pub fn restore_cpu_name() -> Result<CpuRenameResult, String> {
    #[cfg(target_os = "windows")]
    {
        let backup = read_backup();
        if backup.is_empty() {
            return Err("没有可还原的备份".to_string());
        }
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let mut restored = 0usize;

        for (sub, original) in &backup {
            let Ok(key) = hklm.open_subkey_with_flags(
                format!("{}\\{}", CPU_ROOT, sub),
                KEY_READ | KEY_WRITE,
            ) else {
                continue;
            };
            if key.set_value(VALUE_NAME, original).is_ok() {
                restored += 1;
            }
        }

        if restored == 0 {
            return Err("还原失败：需要管理员权限，请以管理员身份运行 NexBox".to_string());
        }
        let _ = std::fs::remove_file(backup_path()?);
        Ok(CpuRenameResult {
            success: true,
            message: "已还原为原始 CPU 名称".to_string(),
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("仅支持 Windows".to_string())
    }
}
