// ============================================================================
// 文件信息结构定义(从 light-c-main 移植)
// 用于存储扫描到的文件详细信息
// ============================================================================

use super::JunkCategory;
use serde::{Deserialize, Serialize};

/// 单个文件的详细信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    /// 文件完整路径
    pub path: String,
    /// 文件名
    pub name: String,
    /// 文件大小(字节)
    pub size: u64,
    /// 文件被删除前的原始路径(仅回收站条目有值)
    pub original_path: Option<String>,
    /// 最后修改时间(Unix时间戳)
    pub modified_time: i64,
    /// 是否为目录
    pub is_dir: bool,
    /// 所属分类
    pub category: JunkCategory,
}

impl FileInfo {
    /// 创建新的文件信息
    pub fn new(
        path: String,
        name: String,
        size: u64,
        modified_time: i64,
        is_dir: bool,
        category: JunkCategory,
    ) -> Self {
        FileInfo {
            path,
            name,
            size,
            original_path: None,
            modified_time,
            is_dir,
            category,
        }
    }

    /// 记录回收站元数据中的原始路径,供界面提示用户文件来源。
    pub fn with_original_path(mut self, original_path: String) -> Self {
        self.original_path = Some(original_path);
        self
    }
}

/// 分类扫描结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryScanResult {
    /// 分类
    pub category: JunkCategory,
    /// 分类显示名称
    pub display_name: String,
    /// 分类描述
    pub description: String,
    /// 风险等级
    pub risk_level: u8,
    /// 该分类下的所有文件
    pub files: Vec<FileInfo>,
    /// 该分类下的注册表残留项(Winapp2 规则)
    pub registry_items: Vec<RegistryItemInfo>,
    /// 该分类是否默认勾选(内置分类恒 true;Winapp2 条目按 Default 标志)
    pub default_select: bool,
    /// 总大小(字节)
    pub total_size: u64,
    /// 文件数量
    pub file_count: usize,
}

impl CategoryScanResult {
    /// 创建新的分类扫描结果
    pub fn new(category: JunkCategory) -> Self {
        CategoryScanResult {
            display_name: category.display_name().to_string(),
            description: category.description().to_string(),
            risk_level: category.risk_level(),
            category,
            files: Vec::new(),
            registry_items: Vec::new(),
            default_select: true,
            total_size: 0,
            file_count: 0,
        }
    }

    /// 添加文件到结果中
    pub fn add_file(&mut self, file: FileInfo) {
        self.total_size += file.size;
        self.file_count += 1;
        self.files.push(file);
    }

    /// 是否有可清理内容
    pub fn is_empty_result(&self) -> bool {
        self.file_count == 0 && self.registry_items.is_empty()
    }

    /// 获取人类可读的总大小
    pub fn human_readable_total_size(&self) -> String {
        format_size(self.total_size)
    }
}

/// 完整扫描结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JunkScanResult {
    /// 各分类的扫描结果
    pub categories: Vec<CategoryScanResult>,
    /// 总大小(字节)
    pub total_size: u64,
    /// 总文件数量
    pub total_file_count: usize,
    /// 扫描耗时(毫秒)
    pub scan_duration_ms: u64,
    /// 扫描时间戳
    pub scan_timestamp: i64,
}

impl JunkScanResult {
    /// 创建新的扫描结果
    pub fn new() -> Self {
        JunkScanResult {
            categories: Vec::new(),
            total_size: 0,
            total_file_count: 0,
            scan_duration_ms: 0,
            scan_timestamp: chrono::Utc::now().timestamp(),
        }
    }

    /// 添加分类结果
    pub fn add_category_result(&mut self, result: CategoryScanResult) {
        self.total_size += result.total_size;
        self.total_file_count += result.file_count;
        self.categories.push(result);
    }

    /// 设置扫描耗时
    pub fn set_duration(&mut self, duration_ms: u64) {
        self.scan_duration_ms = duration_ms;
    }

    /// 获取人类可读的总大小
    pub fn human_readable_total_size(&self) -> String {
        format_size(self.total_size)
    }
}

impl Default for JunkScanResult {
    fn default() -> Self {
        Self::new()
    }
}

/// 格式化文件大小为人类可读格式
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// 删除目标:携带扫描阶段已获取的文件大小,避免删除阶段对每个文件重复 stat
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteTarget {
    pub path: String,
    /// 已知的文件大小(字节);为 None 时删除引擎会自行查询
    #[serde(default)]
    pub size: Option<u64>,
    /// 是否为注册表目标(Winapp2 规则);为 true 时 path 是 "HIVE\子键" 路径
    #[serde(default)]
    pub is_registry: Option<bool>,
    /// 注册表目标要删除的值名;省略表示删除整个键
    #[serde(default)]
    pub value_name: Option<String>,
}

/// 注册表删除目标(供命令层解析前端提交后交给 registry_delete)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryDeleteTarget {
    /// "HIVE\子键" 格式的注册表路径
    pub key_path: String,
    /// 值名;None 表示删除整个键
    pub value_name: Option<String>,
}

/// 展示给用户的注册表残留项信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryItemInfo {
    /// "HIVE\子键" 格式的注册表路径
    pub key_path: String,
    pub value_name: Option<String>,
    /// 展示文本(如 "值名: xxx" 或 "整个键")
    pub description: String,
}

/// 删除操作结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteResult {
    /// 成功删除的文件数
    pub success_count: usize,
    /// 删除失败的文件数
    pub failed_count: usize,
    /// 标记为重启后删除的文件数
    pub reboot_pending_count: usize,
    /// 释放的空间大小(字节)
    pub freed_size: u64,
    /// 是否需要重启完成清理
    pub needs_reboot: bool,
    /// 失败的文件列表及原因
    pub failed_files: Vec<DeleteError>,
}

impl DeleteResult {
    /// 创建新的删除结果
    pub fn new() -> Self {
        DeleteResult {
            success_count: 0,
            failed_count: 0,
            reboot_pending_count: 0,
            freed_size: 0,
            needs_reboot: false,
            failed_files: Vec::new(),
        }
    }

    /// 记录成功删除
    pub fn add_success(&mut self, size: u64) {
        self.success_count += 1;
        self.freed_size += size;
    }

    /// 记录重启后删除
    pub fn add_reboot_pending(&mut self, size: u64) {
        self.reboot_pending_count += 1;
        self.needs_reboot = true;
        self.freed_size += size; // 文件将在重启后删除,计入释放空间
    }

    /// 记录删除失败
    pub fn add_failure(&mut self, path: String, reason: String) {
        self.failed_count += 1;
        self.failed_files.push(DeleteError { path, reason });
    }
}

impl Default for DeleteResult {
    fn default() -> Self {
        Self::new()
    }
}

/// 删除错误信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteError {
    /// 文件路径
    pub path: String,
    /// 错误原因
    pub reason: String,
}
