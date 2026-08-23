//! 持久化适配层 —— 上游 SQLite(`sqlx`) `Db` 的 JSON 文件后端等价实现。
//!
//! 与上游的接口契约逐字对齐（方法签名、守卫语义、错误类型），使协调器源码
//! 零改动运行。存储模型：单文件 `accel_state.json`（`config` KV + 每任务的
//! 分段行/validator/range 标记/epoch），每次变更原子落盘（tmp + rename）。
//!
//! 守卫语义对齐上游 SQL：
//! - [`Db::update_segment_progress_bounded`]：epoch 存在性 + `start_byte`
//!   匹配双守卫 + 段长钳制（防旧 spawn 迟到写污染新布局）；
//! - [`Db::update_segments_progress_batch`]：同款守卫 + 单调不回退
//!   （`max(已记录, min(写入值, 段长))`）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

/// 单个分段行 —— 字段与上游 `db::SegmentInfo` 一致。
pub struct SegmentInfo {
    pub index: i32,
    pub start_byte: i64,
    pub end_byte: i64,
    pub downloaded_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TaskState {
    #[serde(default)]
    total_bytes: i64,
    #[serde(default)]
    etag: String,
    #[serde(default)]
    last_modified: String,
    /// 缺省 true：与上游「任务不存在/旧库默认视为已验证」一致。
    #[serde(default = "default_true")]
    range_verified: bool,
    #[serde(default)]
    segments_epoch: i64,
    #[serde(default)]
    segments: Vec<SegmentRow>,
    // ── NexBox 扩展：重启后恢复任务所需的元数据（上游存 tasks 表同款字段）──
    #[serde(default)]
    url: String,
    #[serde(default)]
    save_dir: String,
    #[serde(default)]
    file_name: String,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SegmentRow {
    index: i32,
    start_byte: i64,
    end_byte: i64,
    downloaded_bytes: i64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct State {
    #[serde(default)]
    config: HashMap<String, String>,
    #[serde(default)]
    tasks: HashMap<String, TaskState>,
}

/// JSON 后端持久化句柄。Clone 廉价（共享同一内部状态，与上游连接池克隆语义对齐）。
#[derive(Clone)]
pub struct Db {
    inner: Arc<Mutex<DbInner>>,
}

struct DbInner {
    path: PathBuf,
    state: State,
}

impl Db {
    /// 打开（或创建）位于 `dir` 下的状态库。与上游 `Db::open` 同名同形。
    pub async fn open(dir: &Path) -> Result<Self, DbError> {
        let dir = dir.to_path_buf();
        tokio::task::spawn_blocking(move || Self::open_sync(&dir))
            .await
            .map_err(|e| DbError::Other(format!("join error: {e}")))?
    }

    fn open_sync(dir: &Path) -> Result<Self, DbError> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join("accel_state.json");
        let state = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            serde_json::from_str(&raw)?
        } else {
            State::default()
        };
        Ok(Self {
            inner: Arc::new(Mutex::new(DbInner { path, state })),
        })
    }

    // -----------------------------------------------------------------------
    // Config KV store
    // -----------------------------------------------------------------------

    /// Get a single config value by key.
    pub async fn get_config(&self, key: &str) -> Result<Option<String>, DbError> {
        let db = self.lock();
        Ok(db.state.config.get(key).cloned())
    }

    /// Set a config value (insert or update).
    pub async fn set_config(&self, key: &str, value: &str) -> Result<(), DbError> {
        let mut db = self.lock();
        db.state
            .config
            .insert(key.to_string(), value.to_string());
        db.persist()
    }

    /// Delete a config entry by key.
    #[allow(dead_code)]
    pub async fn delete_config(&self, key: &str) -> Result<(), DbError> {
        let mut db = self.lock();
        db.state.config.remove(key);
        db.persist()
    }

    // -----------------------------------------------------------------------
    // Segments
    // -----------------------------------------------------------------------

    /// 全量替换任务分段行（上游为事务内逐条 INSERT；协调器仅在空表新建或
    /// 先 delete 再重建时调用，replace 语义对两种调用模式均安全幂等）。
    pub async fn insert_segments(
        &self,
        task_id: &str,
        segments: &[(i32, i64, i64)],
    ) -> Result<(), DbError> {
        let mut db = self.lock();
        let task = db.task_mut(task_id);
        task.segments = segments
            .iter()
            .map(|(index, start, end)| SegmentRow {
                index: *index,
                start_byte: *start,
                end_byte: *end,
                downloaded_bytes: 0,
            })
            .collect();
        db.persist()
    }

    pub async fn load_segments(&self, task_id: &str) -> Result<Vec<SegmentInfo>, DbError> {
        let db = self.lock();
        Ok(db
            .state
            .tasks
            .get(task_id)
            .map(|t| {
                t.segments
                    .iter()
                    .map(|s| SegmentInfo {
                        index: s.index,
                        start_byte: s.start_byte,
                        end_byte: s.end_byte,
                        downloaded_bytes: s.downloaded_bytes,
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// 写入当前 spawn 的段行布局属主令牌。
    pub async fn set_segments_epoch(&self, task_id: &str, epoch: i64) -> Result<(), DbError> {
        let mut db = self.lock();
        db.task_mut(task_id).segments_epoch = epoch;
        db.persist()
    }

    /// worker 侧段进度写入：epoch 存在性 + `start_byte` 匹配双守卫 + 段长钳制。
    /// 任一守卫不过即为 0 行受影响的静默 no-op（对齐上游 UPDATE ... WHERE）。
    pub async fn update_segment_progress_bounded(
        &self,
        task_id: &str,
        segment_index: i32,
        downloaded_bytes: i64,
        start_byte: i64,
        epoch: i64,
    ) -> Result<(), DbError> {
        let mut db = self.lock();
        let Some(task) = db.state.tasks.get_mut(task_id) else {
            return Ok(());
        };
        if task.segments_epoch != epoch {
            return Ok(());
        }
        if let Some(seg) = task.segments.iter_mut().find(|s| s.index == segment_index) {
            if seg.start_byte != start_byte {
                return Ok(());
            }
            let cap = seg.end_byte - seg.start_byte + 1;
            seg.downloaded_bytes = downloaded_bytes.min(cap);
        }
        db.persist()
    }

    /// 批量段进度写入：逐行复用 bounded 守卫 + 单调不回退。
    /// `rows` 为 `(segment_index, downloaded_bytes, start_byte)`；空切片 no-op。
    pub(crate) async fn update_segments_progress_batch(
        &self,
        task_id: &str,
        epoch: i64,
        rows: &[(i32, i64, i64)],
    ) -> Result<(), DbError> {
        if rows.is_empty() {
            return Ok(());
        }
        let mut db = self.lock();
        let Some(task) = db.state.tasks.get_mut(task_id) else {
            return Ok(());
        };
        if task.segments_epoch != epoch {
            return Ok(());
        }
        for (seg_idx, dl_bytes, start_byte) in rows {
            if let Some(seg) = task.segments.iter_mut().find(|s| s.index == *seg_idx) {
                if seg.start_byte != *start_byte {
                    continue;
                }
                let capped = (*dl_bytes).min(seg.end_byte - seg.start_byte + 1);
                seg.downloaded_bytes = seg.downloaded_bytes.max(capped);
            }
        }
        db.persist()
    }

    /// Flush final downloaded_bytes for all segments（权威终态覆写，钳制段长）。
    pub async fn flush_segments_progress(
        &self,
        task_id: &str,
        updates: Vec<(i32, i64)>,
    ) -> Result<(), DbError> {
        let mut db = self.lock();
        if let Some(task) = db.state.tasks.get_mut(task_id) {
            for (seg_idx, dl_bytes) in updates {
                if let Some(seg) = task.segments.iter_mut().find(|s| s.index == seg_idx) {
                    seg.downloaded_bytes =
                        dl_bytes.min(seg.end_byte - seg.start_byte + 1);
                }
            }
        }
        db.persist()
    }

    pub async fn delete_segments(&self, task_id: &str) -> Result<(), DbError> {
        let mut db = self.lock();
        if let Some(task) = db.state.tasks.get_mut(task_id) {
            task.segments.clear();
        } else {
            db.state.tasks.insert(task_id.to_string(), TaskState::default());
        }
        db.persist()
    }

    pub async fn reset_segments_progress(&self, task_id: &str) -> Result<(), DbError> {
        let mut db = self.lock();
        if let Some(task) = db.state.tasks.get_mut(task_id) {
            for seg in &mut task.segments {
                seg.downloaded_bytes = 0;
            }
            db.persist()?;
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Task metadata
    // -----------------------------------------------------------------------

    /// 原子 upsert 单段（DELETE + INSERT 等价）。
    pub async fn upsert_segment(
        &self,
        task_id: &str,
        segment_index: i32,
        start_byte: i64,
        end_byte: i64,
        downloaded_bytes: i64,
    ) -> Result<(), DbError> {
        let mut db = self.lock();
        let task = db.task_mut(task_id);
        task.segments.retain(|s| s.index != segment_index);
        task.segments.push(SegmentRow {
            index: segment_index,
            start_byte,
            end_byte,
            downloaded_bytes,
        });
        task.segments.sort_by_key(|s| s.index);
        db.persist()
    }

    /// Atomically persist a segment split: upsert the new child segment **and**
    /// shrink the parent's `end_byte` in a single transaction.
    ///
    /// This prevents the scenario where the process crashes between the two
    /// operations, leaving overlapping byte ranges that `validate_coverage`
    /// would have to reset.
    #[allow(clippy::too_many_arguments)]
    pub async fn persist_split(
        &self,
        task_id: &str,
        child_index: i32,
        child_start: i64,
        child_end: i64,
        child_downloaded: i64,
        parent_index: i32,
        parent_new_end: i64,
    ) -> Result<(), DbError> {
        let mut db = self.lock();
        let Some(task) = db.state.tasks.get_mut(task_id) else {
            return Ok(());
        };
        // 1. Upsert child segment (DELETE + INSERT).
        task.segments.retain(|s| s.index != child_index);
        task.segments.push(SegmentRow {
            index: child_index,
            start_byte: child_start,
            end_byte: child_end,
            downloaded_bytes: child_downloaded,
        });
        // 2. Shrink parent's end_byte.
        if let Some(parent) = task.segments.iter_mut().find(|s| s.index == parent_index) {
            parent.end_byte = parent_new_end;
        }
        task.segments.sort_by_key(|s| s.index);
        db.persist()
    }

    /// 原子持久化【开放式首段合并】：延长父段 `end_byte` 并删除全部被吸收的
    /// Pending 段行，单事务提交（与 `persist_split` 对称——防止崩溃残留
    /// 重叠/缺口区间，否则 resume 时 `validate_coverage` 会整体重置进度）。
    pub async fn persist_merge(
        &self,
        task_id: &str,
        parent_index: i32,
        parent_new_end: i64,
        absorbed: &[i32],
    ) -> Result<(), DbError> {
        let mut db = self.lock();
        let Some(task) = db.state.tasks.get_mut(task_id) else {
            return Ok(());
        };
        if let Some(parent) = task.segments.iter_mut().find(|s| s.index == parent_index) {
            parent.end_byte = parent_new_end;
        }
        task.segments.retain(|s| !absorbed.contains(&s.index));
        db.persist()
    }

    /// Update the total_bytes for a task.
    pub async fn update_task_total_bytes(&self, id: &str, total_bytes: i64) -> Result<(), DbError> {
        let mut db = self.lock();
        db.task_mut(id).total_bytes = total_bytes;
        db.persist()
    }

    /// 记录首次下载 probe 看到的原始版本标识（ETag / Last-Modified）。
    pub async fn set_task_validator(
        &self,
        id: &str,
        etag: &str,
        last_modified: &str,
    ) -> Result<(), DbError> {
        let mut db = self.lock();
        let task = db.task_mut(id);
        task.etag = etag.to_string();
        task.last_modified = last_modified.to_string();
        db.persist()
    }

    /// 读取首次下载记录的原始版本标识，返回 `(orig_etag, orig_last_modified)`。
    #[allow(dead_code)]
    pub async fn get_task_validator(&self, id: &str) -> Result<(String, String), DbError> {
        let db = self.lock();
        match db.state.tasks.get(id) {
            Some(t) => Ok((t.etag.clone(), t.last_modified.clone())),
            None => Ok((String::new(), String::new())),
        }
    }

    /// 设置任务的 Range 能力验证标记。
    pub async fn set_task_range_verified(&self, id: &str, verified: bool) -> Result<(), DbError> {
        let mut db = self.lock();
        db.task_mut(id).range_verified = verified;
        db.persist()
    }

    /// 保存任务元数据（URL/目录/文件名）——断电续传的恢复依据。
    pub async fn set_task_meta(
        &self,
        id: &str,
        url: &str,
        save_dir: &str,
        file_name: &str,
    ) -> Result<(), DbError> {
        let mut db = self.lock();
        let task = db.task_mut(id);
        task.url = url.to_string();
        task.save_dir = save_dir.to_string();
        task.file_name = file_name.to_string();
        db.persist()
    }

    /// 读取任务元数据。任务不存在返回 None。
    #[allow(dead_code)]
    pub async fn get_task_meta(
        &self,
        id: &str,
    ) -> Result<Option<(String, String, String)>, DbError> {
        let db = self.lock();
        Ok(db.state.tasks.get(id).map(|t| {
            (
                t.url.clone(),
                t.save_dir.clone(),
                t.file_name.clone(),
            )
        }))
    }

    /// 列出所有含分段行且未完成的任务（供重启后扫描可续传任务）。
    /// 返回 `(task_id, url, save_dir, file_name, total_bytes, downloaded_bytes)`。
    pub async fn list_unfinished_tasks(
        &self,
    ) -> Result<Vec<(String, String, String, String, i64, i64)>, DbError> {
        let db = self.lock();
        let mut out = Vec::new();
        for (id, t) in &db.state.tasks {
            if t.segments.is_empty() || t.url.is_empty() {
                continue;
            }
            let total_len = |s: &SegmentRow| s.end_byte - s.start_byte + 1;
            let done: i64 = t
                .segments
                .iter()
                .map(|s| s.downloaded_bytes.min(total_len(s)).max(0))
                .sum();
            let complete = t
                .segments
                .iter()
                .all(|s| s.downloaded_bytes >= total_len(s));
            if complete {
                continue;
            }
            out.push((
                id.clone(),
                t.url.clone(),
                t.save_dir.clone(),
                t.file_name.clone(),
                t.total_bytes,
                done,
            ));
        }
        Ok(out)
    }

    /// 读取任务的 Range 能力验证标记。任务不存在视为已验证（true）。
    #[allow(dead_code)]
    pub async fn get_task_range_verified(&self, id: &str) -> Result<bool, DbError> {
        let db = self.lock();
        Ok(db
            .state
            .tasks
            .get(id)
            .map(|t| t.range_verified)
            .unwrap_or(true))
    }

    // -----------------------------------------------------------------------

    fn lock(&self) -> std::sync::MutexGuard<'_, DbInner> {
        match self.inner.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

impl DbInner {
    fn task_mut(&mut self, task_id: &str) -> &mut TaskState {
        self.state
            .tasks
            .entry(task_id.to_string())
            .or_default()
    }

    /// 原子落盘：tmp 写入 + rename 覆盖。持锁执行（文件极小，微秒级），
    /// 保证读改写序列化、杜绝整文件级 last-writer-wins 丢更新。
    fn persist(&self) -> Result<(), DbError> {
        let json = serde_json::to_vec_pretty(&self.state)?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}
