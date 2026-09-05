// ============================================================================
// Winapp2 深度扫描引擎
//
// 对每条规则条目:
//   1. 安装检测(Detect/DetectFile/SpecialDetect)不通过则跳过
//   2. FileKey: None/Recurse 逐文件统计;REMOVESELF 直接把匹配目录作为删除目标
//   3. RegKey: 目标当前存在才计入注册表残留
// 文件扫描复用 ScanEngine 的系统保护与 Chromium 持久化数据拦截。
// ============================================================================

use log::info;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::SystemTime;

use super::detection::DetectionService;
use super::parser::{CleanerEntry, ExcludeType, FileKeyEntry, FileKeyFlag};
use super::path_expander::{matches_wildcard, PathExpander};
use crate::storage_scan::file_info::{CategoryScanResult, FileInfo, RegistryItemInfo};
use crate::storage_scan::scan_engine::ScanEngine;
use crate::storage_scan::JunkCategory;

/// 扫描并发线程数(与 ScanEngine 风格一致的有界并行)
const WORKERS: usize = 8;

/// 深度扫描引擎(无状态,方法级共享检测器)
pub struct DeepScanEngine;

impl DeepScanEngine {
    /// 并行扫描全部 Winapp2 条目,返回命中(有文件或注册表残留)的分类结果
    pub fn scan(entries: &[CleanerEntry]) -> Vec<CategoryScanResult> {
        if entries.is_empty() {
            return Vec::new();
        }

        let count = entries.len();
        let shared_entries: Arc<Vec<CleanerEntry>> = Arc::new(entries.to_vec());
        let results: Arc<Mutex<Vec<CategoryScanResult>>> = Arc::new(Mutex::new(Vec::new()));
        let counter = Arc::new(AtomicUsize::new(0));
        let worker_count = WORKERS.min(count);

        info!("深度扫描开始: {} 条规则, {} 个线程", count, worker_count);

        let mut handles = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let counter = Arc::clone(&counter);
            let shared_entries = Arc::clone(&shared_entries);
            let results = Arc::clone(&results);
            handles.push(thread::spawn(move || {
                let detector = DetectionService::new();
                let expander = PathExpander::new();
                loop {
                    let idx = counter.fetch_add(1, Ordering::SeqCst);
                    if idx >= shared_entries.len() {
                        break;
                    }
                    let entry = &shared_entries[idx];
                    if let Some(category_result) =
                        scan_entry(entry, &detector, &expander)
                    {
                        results.lock().unwrap().push(category_result);
                    }
                }
            }));
        }

        for handle in handles {
            let _ = handle.join();
        }

        let found = results.lock().unwrap().clone();
        info!("深度扫描完成: {} 条规则命中", found.len());
        found
    }

    /// 扫描单个条目(scan_junk_category 单分类命令使用)
    pub fn scan_single(entries: &[CleanerEntry], name: &str) -> Option<CategoryScanResult> {
        let entry = entries.iter().find(|e| e.name == name)?;
        let detector = DetectionService::new();
        let expander = PathExpander::new();
        scan_entry(entry, &detector, &expander)
    }
}

/// 扫描单个规则条目
fn scan_entry(
    entry: &CleanerEntry,
    detector: &DetectionService,
    expander: &PathExpander,
) -> Option<CategoryScanResult> {
    if !detector.is_installed(entry) {
        return None;
    }

    let mut result = CategoryScanResult::new(JunkCategory::Winapp2(entry.name.clone()));
    result.default_select = entry.default_select;
    // Edge 浏览器的数据条目一律默认取消勾选(用户要求),可手动勾选
    if is_edge_entry(entry) {
        result.default_select = false;
    }
    // 风险档:默认不勾选或带警告的条目升为 3 档
    if !result.default_select || entry.warning.is_some() {
        result.risk_level = 3;
    }

    // 预处理 FileKey 路径;只保留 FILE/PATH 类型的排除规则
    let exclusions: Vec<ExclusionRule> = entry
        .exclude_keys
        .iter()
        .filter(|e| e.ex_type != ExcludeType::Reg)
        .map(|e| ExclusionRule {
            dir_prefix: format!(
                "{}\\",
                expander.expand_variables(&e.path).trim_end_matches(['\\', '/'])
            ),
            pattern: e.pattern.clone(),
        })
        .collect();

    for file_key in &entry.file_keys {
        scan_file_key(file_key, expander, &exclusions, &mut result);
    }

    for reg_key in &entry.reg_keys {
        if super::detection::reg_key_exists(&reg_key.key_path, reg_key.value_name.as_deref()) {
            result.registry_items.push(RegistryItemInfo {
                key_path: reg_key.key_path.clone(),
                value_name: reg_key.value_name.clone(),
                description: match &reg_key.value_name {
                    Some(v) => format!("值名: {}", v),
                    None => "整个键".to_string(),
                },
            });
        }
    }

    if result.is_empty_result() {
        return None;
    }
    Some(result)
}

/// 判断规则条目是否属于 Edge 浏览器:
/// 名称含独立单词 "edge"(如 "Microsoft Edge Caches"),或 FileKey 路径命中
/// Microsoft\Edge / \Edge\ 目录(避免 "Edgeup"、"Knowledge" 等子串误伤)
fn is_edge_entry(entry: &CleanerEntry) -> bool {
    let name = entry.name.to_ascii_lowercase();
    if name.split([' ', '(', ')', '&', '/']).any(|w| w == "edge") {
        return true;
    }
    entry.file_keys.iter().any(|fk| {
        let p = fk.path.to_ascii_lowercase();
        p.contains(r"microsoft\edge") || p.contains(r"\edge\")
    })
}

/// 扫描单条 FileKey 规则
fn scan_file_key(
    file_key: &FileKeyEntry,
    expander: &PathExpander,
    exclusions: &[ExclusionRule],
    result: &mut CategoryScanResult,
) {
    let category = result.category.clone();

    if file_key.flag == FileKeyFlag::RemoveSelf {
        // REMOVESELF:匹配目录整体作为删除目标,删除阶段 remove_dir_all 自带空目录修剪
        for dir in expander.resolve_paths(&file_key.path) {
            let path = Path::new(&dir);
            if !path.is_dir() || root_excluded(&dir, exclusions) || protected_by_scan(path) {
                continue;
            }
            let size = dir_size(path);
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| dir.clone());
            let modified = modified_time(path).unwrap_or(0);
            result.add_file(FileInfo::new(
                dir,
                name,
                size,
                modified,
                true, // 目录目标
                category.clone(),
            ));
        }
        return;
    }

    // None: 仅顶层;Recurse: 递归子目录
    let recurse = file_key.flag == FileKeyFlag::Recurse;
    let mut patterns: Vec<String> = file_key
        .pattern
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    // Windows 语义:"*.*" 匹配所有文件(含无扩展名文件),统一成 "*"
    for p in patterns.iter_mut() {
        if p == "*.*" {
            p.clear();
            p.push('*');
        }
    }

    for dir in expander.resolve_paths(&file_key.path) {
        let root = Path::new(&dir);
        if !root.is_dir() || root_excluded(&dir, exclusions) {
            continue;
        }
        walk_files(root, &patterns, recurse, exclusions, &category, result);
    }
}

/// 遍历目录,按模式收集文件(None 仅顶层,Recurse 递归,跳过 reparse point)
fn walk_files(
    dir: &Path,
    patterns: &[String],
    recurse: bool,
    exclusions: &[ExclusionRule],
    category: &JunkCategory,
    result: &mut CategoryScanResult,
) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        if file_type.is_file() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if !patterns.iter().any(|p| p == "*" || matches_wildcard(p, &name)) {
                continue;
            }
            if protected_by_scan(&path) || is_excluded(&path, exclusions) {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            result.add_file(FileInfo::new(
                path.to_string_lossy().to_string(),
                name,
                metadata.len(),
                modified_from_meta(&metadata).unwrap_or(0),
                false,
                category.clone(),
            ));
        }
    }

    if !recurse {
        return;
    }

    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        // 跳过符号链接/重解析点,避免 junction 死循环
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        let sub = entry.path();
        if protected_by_scan(&sub) {
            continue;
        }
        walk_files(&sub, patterns, recurse, exclusions, category, result);
    }
}

/// 文件是否命中排除规则(规则内的直接子文件才被字面模式排除)
fn is_excluded(path: &Path, exclusions: &[ExclusionRule]) -> bool {
    let path_str = path.to_string_lossy();
    exclusions
        .iter()
        .any(|rule| rule.matches(path_str.as_ref()))
}

/// 整目录排除检查:排除规则无模式时表示整个子树都被排除
fn root_excluded(dir: &str, exclusions: &[ExclusionRule]) -> bool {
    let normalized = format!("{}\\", dir.trim_end_matches(['\\', '/'])).to_ascii_lowercase();
    exclusions
        .iter()
        .any(|rule| rule.pattern.is_none() && normalized.starts_with(&rule.dir_prefix_lower()))
}

/// 扫描期保护:系统路径 / WebView 持久化数据 / 浏览器扩展 IndexedDB
fn protected_by_scan(path: &Path) -> bool {
    let engine = ScanEngine::new();
    if engine.is_system_protected(path) || engine.is_persistent_app_profile_path(path) {
        return true;
    }
    // TubaTools 同款安全网:浏览器扩展数据库(1Password/Bitwarden/uBlock 等)
    let path_str = path.to_string_lossy().to_ascii_lowercase();
    path_str.contains("\\indexeddb\\chrome-extension_")
}

/// 递归统计目录大小(仅文件,不跟随链接)
fn dir_size(dir: &Path) -> u64 {
    walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

fn modified_time(path: &Path) -> Option<i64> {
    fs::metadata(path).ok().and_then(|m| modified_from_meta(&m))
}

fn modified_from_meta(meta: &fs::Metadata) -> Option<i64> {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
}

/// 一条排除规则(对应 ExcludeKeyN)
struct ExclusionRule {
    /// 排除目录前缀,恒以 '\' 结尾
    dir_prefix: String,
    /// 可选文件名模式;None 表示整个子树
    pattern: Option<String>,
}

impl ExclusionRule {
    fn dir_prefix_lower(&self) -> String {
        self.dir_prefix.to_ascii_lowercase()
    }

    /// 与 C# ExclusionRule.Matches 一致的匹配逻辑
    fn matches(&self, file_path: &str) -> bool {
        let lower_path = file_path.to_ascii_lowercase();
        let lower_prefix = self.dir_prefix_lower();
        if !lower_path.starts_with(&lower_prefix) {
            return false;
        }

        let Some(pattern) = &self.pattern else {
            return true; // 无模式:整个目录子树被排除
        };

        let file_name = Path::new(file_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if pattern.contains('*') || pattern.contains('?') {
            matches_wildcard(pattern, &file_name)
        } else {
            // 字面模式:必须是指定前缀目录的直接子文件
            lower_path[lower_prefix.len()..] == pattern.to_ascii_lowercase()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry_named(name: &str, paths: &[&str]) -> CleanerEntry {
        CleanerEntry {
            name: name.to_string(),
            section: None,
            lang_sec_ref: None,
            detect_keys: Vec::new(),
            detect_files: Vec::new(),
            special_detect: None,
            file_keys: paths
                .iter()
                .map(|p| FileKeyEntry {
                    path: p.to_string(),
                    pattern: "*".to_string(),
                    flag: FileKeyFlag::None,
                })
                .collect(),
            reg_keys: Vec::new(),
            exclude_keys: Vec::new(),
            warning: None,
            default_select: true,
        }
    }

    #[test]
    fn detects_edge_entries() {
        assert!(is_edge_entry(&entry_named("Microsoft Edge Caches", &[])));
        assert!(is_edge_entry(&entry_named("Edge WebView2 Cache", &[])));
        assert!(is_edge_entry(&entry_named("Edge", &["%LocalAppData%\\Microsoft\\Edge\\User Data\\Default"])));
        // 子串误伤防护
        assert!(!is_edge_entry(&entry_named("Chromium Knowledge Base", &[])));
        assert!(!is_edge_entry(&entry_named("Edgeup Reminder", &[])));
        // 路径特征命中(名下无语,路径是 Edge 目录)
        assert!(is_edge_entry(&entry_named(
            "Chromium 系缓存",
            &["%LocalAppData%\\Microsoft\\Edge\\User Data\\*\\Cache"]
        )));
        assert!(!is_edge_entry(&entry_named("Firefox Caches", &[])));
    }

    #[test]
    fn exclusion_rule_matching() {
        let rule = ExclusionRule {
            dir_prefix: r"c:\docs\".to_string(),
            pattern: Some("readme.pdf".to_string()),
        };
        assert!(rule.matches(r"c:\docs\readme.pdf"));
        assert!(!rule.matches(r"c:\docs\sub\readme.pdf"), "字面模式仅匹配直接子文件");

        let dir_rule = ExclusionRule {
            dir_prefix: r"c:\cache\".to_string(),
            pattern: Some("*.db".to_string()),
        };
        assert!(dir_rule.matches(r"c:\cache\sub\a.db"), "通配模式覆盖整个子树");
        assert!(!dir_rule.matches(r"c:\cache\sub\a.log"));
    }
}