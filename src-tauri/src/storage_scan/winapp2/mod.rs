// ============================================================================
// Winapp2 社区规则库(Rust 移植自 TubaTools JunkCleaner, 原始规则库遵循
// CC-BY-SA-4.0, 见 resources/winapp2.ini 头部授权说明)
//
// 模块职责:
//   parser        - 解析 Winapp2.ini 为 CleanerEntry
//   path_expander - %ENVVAR% 展开 + 通配符路径递归展开
//   detection     - Detect/DetectFile/SpecialDetect 安装检测
//   deep_scan     - 深度扫描引擎(文件 + 注册表残留)
//   update        - 规则库信息与在线更新(仿照图吧工具箱)
// ============================================================================

pub mod deep_scan;
pub mod detection;
pub mod parser;
pub mod path_expander;
pub mod update;

pub use deep_scan::DeepScanEngine;

use parser::CleanerEntry;
use std::sync::OnceLock;

/// 编译期内嵌的原版规则库(保留完整授权头,满足 CC-BY-SA-4.0 署名要求)
const EMBEDDED_INI: &str = include_str!("../../../resources/winapp2.ini");

static EMBEDDED_ENTRIES: OnceLock<Vec<CleanerEntry>> = OnceLock::new();

/// 加载规则条目。
///
/// 优先级:
/// 1. `<AppData>\NexBox\winapp2.ini` 覆盖文件(在线更新写入的位置,默认不存在)
/// 2. 编译期内嵌的原版 Winapp2.ini
pub fn load_entries() -> Vec<CleanerEntry> {
    let override_path = update::rule_data_path();
    if override_path.is_file() {
        if let Ok(content) = std::fs::read_to_string(&override_path) {
            return parser::parse(&content);
        }
    }

    EMBEDDED_ENTRIES
        .get_or_init(|| parser::parse(EMBEDDED_INI))
        .clone()
}