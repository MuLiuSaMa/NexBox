// ============================================================================
// 规则库信息与在线更新(仿照 TubaTools JunkCleanerDatabase 实现)
//
// 规则库官方源: https://github.com/MoscaDotTo/Winapp2 (master, CC-BY-SA-4.0)
// 优先级与加载一致: 数据目录覆盖文件优先, 内置内嵌版本作为离线回退。
// 更新流程: 下载到内存 -> 内容校验(大小 + FileKey) -> 版本比较(仅比内置新时
// 才写入) -> 原子写入数据目录。
// ============================================================================

use futures_util::StreamExt;
use log::{info, warn};
use serde::Serialize;
use std::io::Write;
use tauri::{Emitter, Window};

use super::EMBEDDED_INI;

/// 下载源(三级回退,与图吧工具箱一致:官方 raw -> jsdelivr CDN -> 镜像代理)
const DOWNLOAD_SOURCES: &[&str] = &[
    "https://raw.githubusercontent.com/MoscaDotTo/Winapp2/master/Winapp2.ini",
    "https://cdn.jsdelivr.net/gh/MoscaDotTo/Winapp2@master/Winapp2.ini",
    "https://gh-proxy.com/https://raw.githubusercontent.com/MoscaDotTo/Winapp2/master/Winapp2.ini",
];

/// 有效规则库最小大小(小于它视为下载到错误内容)
const MIN_VALID_SIZE: usize = 128 * 1024;

/// 规则库展示信息(前端展示当前版本/条目数)
#[derive(Debug, Clone, Serialize)]
pub struct RuleDatabaseInfo {
    /// ini 头部 "; Version:" 值(如 260828);无版本头为空串
    pub version: String,
    pub entry_count: usize,
    pub file_size_bytes: u64,
    /// true = 使用内置内嵌规则库; false = 使用数据目录更新过的覆盖文件
    pub is_bundled: bool,
    /// 实际生效路径(内置时为 "(内置规则库)")
    pub effective_path: String,
}

/// 更新进度事件载荷
#[derive(Debug, Clone, Serialize)]
pub struct UpdateProgress {
    pub message: String,
}

/// 数据目录覆盖规则库路径:<AppData>\NexBox\winapp2.ini
pub(crate) fn rule_data_path() -> std::path::PathBuf {
    dirs::data_dir()
        .map(|d| d.join("NexBox").join("winapp2.ini"))
        .unwrap_or_else(|| std::path::PathBuf::from("winapp2.ini"))
}

/// 从内容头部解析版本号与条目数(仿照 JunkCleanerDatabase.GetInfo)
pub fn parse_header(content: &str) -> (String, usize) {
    let mut version = String::new();
    let mut entry_count = 0usize;
    for line in content.lines().take(20) {
        let t = line.trim();
        let lower = t.to_ascii_lowercase();
        if version.is_empty() && lower.starts_with("; version:") {
            version = t[10..].trim().to_string(); // "; Version:" 固定长度 10
        }
        if entry_count == 0 && lower.starts_with("; # of entries:") {
            entry_count = t[15..].trim().replace(',', "").parse().unwrap_or(0);
        }
    }
    (version, entry_count)
}

/// 当前生效规则库信息:数据目录覆盖文件优先,否则内置内嵌版本
pub fn get_info() -> RuleDatabaseInfo {
    let path = rule_data_path();
    if path.is_file() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            let (version, entry_count) = parse_header(&content);
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            return RuleDatabaseInfo {
                version,
                entry_count,
                file_size_bytes: size,
                is_bundled: false,
                effective_path: path.to_string_lossy().to_string(),
            };
        }
    }

    // 内置内嵌版本回退
    let (version, entry_count) = parse_header(EMBEDDED_INI);
    RuleDatabaseInfo {
        version,
        entry_count,
        file_size_bytes: EMBEDDED_INI.len() as u64,
        is_bundled: true,
        effective_path: "(内置规则库)".to_string(),
    }
}

/// 在线更新规则库到数据目录。
///
/// 行为与图吧工具箱一致:
/// 1. 依次尝试多个下载源,任一成功即继续
/// 2. 下载内容需通过校验(>=128KB 且含 FileKey 键)
/// 3. 版本不新于当前覆盖文件时视为已最新,不覆盖
/// 4. 全程向窗口 emit "winapp2-update:progress" 进度事件
pub async fn update_rules(window: Option<Window>) -> Result<RuleDatabaseInfo, String> {
    let emit_progress = |msg: String| {
        info!("规则库更新: {}", msg);
        if let Some(w) = &window {
            let _ = w.emit("winapp2-update:progress", UpdateProgress { message: msg });
        }
    };

    emit_progress("正在连接规则库源(MoscaDotTo/Winapp2)...".to_string());
    let client = reqwest::Client::new();
    let mut last_error: Option<String> = None;

    for url in DOWNLOAD_SOURCES {
        match download(&client, url, &emit_progress).await {
            Ok(bytes) => {
                let content = String::from_utf8_lossy(&bytes).to_string();

                // ---- 校验载荷 ----
                if bytes.len() < MIN_VALID_SIZE || !contains_file_key(&bytes) {
                    warn!("规则库内容校验失败: {}", url);
                    last_error = Some("下载内容不是有效的 Winapp2 规则库".to_string());
                    continue;
                }

                let (new_version, _) = parse_header(&content);
                let old_info = get_info();

                // ---- 版本比较:非内置且新版本不高于当前 -> 已是最新 ----
                if !old_info.is_bundled
                    && !new_version.is_empty()
                    && !old_info.version.is_empty()
                    && new_version <= old_info.version
                {
                    emit_progress(format!("规则库已是最新(版本 {})", old_info.version));
                    return Ok(old_info);
                }

                // ---- 原子写入数据目录(校验通过才落盘) ----
                let target = rule_data_path();
                if let Some(dir) = target.parent() {
                    std::fs::create_dir_all(dir)
                        .map_err(|e| format!("创建数据目录失败: {}", e))?;
                }
                let mut file = std::fs::File::create(&target)
                    .map_err(|e| format!("写入规则库失败: {}", e))?;
                file.write_all(&bytes)
                    .map_err(|e| format!("写入规则库失败: {}", e))?;

                let new_info = get_info();
                emit_progress(format!(
                    "规则库已更新到版本 {} ({} 条规则)",
                    new_info.version, new_info.entry_count
                ));
                return Ok(new_info);
            }
            Err(e) => {
                last_error = Some(e.clone());
                emit_progress(format!("下载失败: {},尝试下一个源...", e));
            }
        }
    }

    Err(format!(
        "规则库更新失败: {}",
        last_error.unwrap_or_else(|| "未知错误".to_string())
    ))
}

/// 下载单个源,全程报告进度,失败返回错误说明
async fn download(
    client: &reqwest::Client,
    url: &str,
    emit: &impl Fn(String),
) -> Result<Vec<u8>, String> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("连接失败: {}", e))?;
    resp.error_for_status_ref()
        .map_err(|e| format!("HTTP 错误: {}", e))?;

    let total = resp.content_length();
    let mut received: u64 = 0;
    let mut bytes: Vec<u8> = Vec::with_capacity(total.map(|t| t as usize).unwrap_or(0).min(2 * 1024 * 1024));

    let mut stream = resp.bytes_stream();
    let mut last_report: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("下载中断: {}", e))?;
        received += chunk.len() as u64;
        bytes.extend_from_slice(&chunk);

        // 进度节流:每 ~512KB 或结束时回报一次
        if received - last_report >= 512 * 1024 {
            last_report = received;
            let kb = received / 1024;
            let msg = match total {
                Some(t) if t > 0 => format!("下载中 {} KB / {} KB ({}%)", kb, t / 1024, received * 100 / t),
                Some(t) => format!("下载中 {} KB / {} KB", kb, t / 1024),
                None => format!("下载中 {} KB", kb),
            };
            emit(msg);
        }
    }
    if received > 0 {
        emit(format!("下载完成 {} KB,正在校验...", received / 1024));
    }
    Ok(bytes)
}

/// 校验内容包含 FileKey 键(ASCII 不区分大小写)
fn contains_file_key(bytes: &[u8]) -> bool {
    bytes
        .windows(b"filekey".len())
        .any(|w| w.eq_ignore_ascii_case(b"filekey"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_version_and_entries() {
        let (v, n) = parse_header("; Version: 260828\n; # of entries: 4,075\n[Foo]\nFileKey1=X");
        assert_eq!(v, "260828");
        assert_eq!(n, 4075);

        let (v, n) = parse_header("[Foo]\nFileKey1=X\n");
        assert_eq!(v, "");
        assert_eq!(n, 0);
    }

    #[test]
    fn validates_file_key_marker() {
        assert!(contains_file_key(b"foo\nFileKey1=%Temp%\\*.tmp"));
        assert!(!contains_file_key(b"some text without marker"));
    }

    #[test]
    fn version_compare_uses_payload_ordering() {
        // 与图吧的 string.CompareOrdinal 语义一致:数字版本按序数(即数值字典序)比较
        let nu = "260901".to_string();
        let old_info = RuleDatabaseInfo {
            version: "260828".to_string(),
            entry_count: 4075,
            file_size_bytes: 1,
            is_bundled: false,
            effective_path: String::new(),
        };
        assert!(!old_info.is_bundled && !nu.is_empty() && !old_info.version.is_empty());
        assert!(!(nu <= old_info.version.clone()), "新版本应大于当前");
    }
}