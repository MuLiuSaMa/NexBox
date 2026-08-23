//! 内置下载加速器 —— 多线程动态分段下载（IDM 式）。
//!
//! 核心引擎为 `nx-dl-engine` crate（逐字搬运自 FluxDown，AGPL-3.0，
//! 见该 crate 内 NOTICE）。本模块是宿主驱动层：探测 → 分段规划 →
//! 协调器下载 → 单流回退 → 落盘改名，经 Tauri 命令与事件向前端暴露。
//!
//! 状态码语义（对齐上游）：0=等待 1=下载中 2=暂停 3=完成 4=错误 5=准备中。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI32, AtomicI64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use nx_dl_engine::cdn::NodePool;
use nx_dl_engine::db::Db;
use nx_dl_engine::downloader::{
    build_client, dedup_filename, resolve_file_info, DownloadError, ProgressUpdate, RequestSpec,
    TEMP_EXT,
};
use nx_dl_engine::events::{EngineEvent, EventSink};
use nx_dl_engine::proxy_config::ProxyConfig;
use nx_dl_engine::segment_advisor::{advise_static, AdvisorInput};
use nx_dl_engine::segment_coordinator::{
    load_domain_conn_caps, run_coordinated_download, ReportScope,
};
use nx_dl_engine::speed_limiter::SpeedLimiter;

/// 引擎状态目录（域名连接策略学习缓存等持久化位置）。
fn engine_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("NexBox")
        .join("accel-engine")
}

/// 状态码（对齐上游）：0=等待 1=下载中 2=暂停 3=完成 4=错误 5=准备中。
const STATUS_PAUSED: i32 = 2;
const STATUS_COMPLETED: i32 = 3;

// ---------------------------------------------------------------------------
// 任务注册表
// ---------------------------------------------------------------------------

pub struct TaskEntry {
    pub id: String,
    pub url: String,
    pub file_name: String,
    pub save_dir: PathBuf,
    /// 最终落盘路径（不含 .fdownloading 后缀）。
    pub final_path: PathBuf,
    pub temp_path: PathBuf,
    pub total_bytes: AtomicI64,
    pub downloaded_bytes: AtomicI64,
    /// 0=pending 1=downloading 2=paused 3=completed 4=error 5=preparing
    pub status: AtomicI32,
    pub error_message: Mutex<String>,
    cancel: Mutex<CancellationToken>,
}

impl TaskEntry {
    fn swap_token(&self) -> CancellationToken {
        let mut g = self.cancel.lock().unwrap();
        let t = CancellationToken::new();
        *g = t.clone();
        t
    }
    fn token(&self) -> CancellationToken {
        self.cancel.lock().unwrap().clone()
    }
}

static TASKS: LazyLock<Mutex<HashMap<String, Arc<TaskEntry>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static SPAWN_GEN: AtomicI64 = AtomicI64::new(1);
/// 全局限速器（v1 不限速；后续接设置面板 set_limit 即可生效于所有任务）。
static LIMITER: LazyLock<SpeedLimiter> = LazyLock::new(|| SpeedLimiter::new(0));

fn tasks() -> &'static Mutex<HashMap<String, Arc<TaskEntry>>> {
    &TASKS
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentDto {
    pub index: i32,
    pub start_byte: i64,
    pub end_byte: i64,
    pub downloaded_bytes: i64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccelTaskSnapshot {
    pub id: String,
    pub url: String,
    pub file_name: String,
    pub save_dir: String,
    pub total_bytes: i64,
    pub downloaded_bytes: i64,
    /// 字节/秒
    pub speed_bps: i64,
    pub status: i32,
    pub error_message: String,
    pub segments: Vec<SegmentDto>,
}

fn snapshot(entry: &TaskEntry, speed_bps: i64, segments: Vec<SegmentDto>) -> AccelTaskSnapshot {
    AccelTaskSnapshot {
        id: entry.id.clone(),
        url: entry.url.clone(),
        file_name: entry.file_name.clone(),
        save_dir: entry.save_dir.to_string_lossy().into_owned(),
        total_bytes: entry.total_bytes.load(Ordering::Relaxed),
        downloaded_bytes: entry.downloaded_bytes.load(Ordering::Relaxed),
        speed_bps,
        status: entry.status.load(Ordering::Relaxed),
        error_message: entry.error_message.lock().unwrap().clone(),
        segments,
    }
}

// ---------------------------------------------------------------------------
// 事件
// ---------------------------------------------------------------------------

struct TauriSink {
    app: AppHandle,
    task_id: String,
}

impl EventSink for TauriSink {
    fn emit(&self, event: EngineEvent) {
        if let EngineEvent::SegmentSplit {
            parent_index,
            parent_new_end,
            child_index,
            child_start,
            child_end,
            is_proactive,
            total_segments,
            ..
        } = event
        {
            let _ = self.app.emit(
                "accel-segment-split",
                serde_json::json!({
                    "taskId": self.task_id,
                    "parentIndex": parent_index,
                    "parentNewEnd": parent_new_end,
                    "childIndex": child_index,
                    "childStart": child_start,
                    "childEnd": child_end,
                    "isProactive": is_proactive,
                    "totalSegments": total_segments,
                }),
            );
        }
    }
}

const PROGRESS_EVENT: &str = "accel-progress";

// ---------------------------------------------------------------------------
// Tauri 命令
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn accel_start(
    app: AppHandle,
    url: String,
    file_name: Option<String>,
    save_dir: Option<String>,
    max_segments: Option<i32>,
) -> Result<AccelTaskSnapshot, String> {
    let url = super::downloader::normalize_gitcode_url(&url);
    if url.is_empty() {
        return Err("URL 不能为空".into());
    }

    // 探测：大小 / Range 能力 / ETag / 文件名
    let proxy = ProxyConfig {
        mode: nx_dl_engine::proxy_config::ProxyMode::System,
        ..Default::default()
    };
    let client = build_client(&proxy, "").map_err(|e| e.to_string())?;
    let spec = RequestSpec::empty_get();
    let info = resolve_file_info(&client, &url, &spec)
        .await
        .map_err(|e| format!("探测失败：{e}"))?;

    let dir: PathBuf = match save_dir.filter(|s| !s.trim().is_empty()) {
        Some(s) => PathBuf::from(s),
        None => dirs::download_dir().unwrap_or_else(std::env::temp_dir),
    };
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("创建目录失败：{e}"))?;

    let raw_name = file_name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(info.file_name.clone());
    let name = dedup_filename(&dir, &raw_name, &HashSet::new(), &HashSet::new(), false).await;

    let final_path = dir.join(&name);
    let temp_path = PathBuf::from(format!("{}{}", final_path.display(), TEMP_EXT));

    // 磁盘空间预检（上游 disk_space 同语义：预分配前确认可用空间）
    if info.total_bytes > 0 {
        let needed = info.total_bytes as u64 + 256 * 1024 * 1024;
        if let Some(free) = free_disk_bytes(&dir) {
            if free < needed {
                return Err(format!(
                    "磁盘空间不足：需要 {}，剩余 {}",
                    format_bytes_cn(info.total_bytes as u64),
                    format_bytes_cn(free)
                ));
            }
        }
    }

    let db = Db::open(&engine_dir()).await.map_err(|e| e.to_string())?;
    load_domain_conn_caps(&db).await;
    if !info.etag.is_empty() || !info.last_modified.is_empty() {
        let _ = db.set_task_validator(&name, &info.etag, &info.last_modified).await;
    }
    let _ = db.set_task_range_verified(&name, info.supports_range).await;
    let _ = db
        .set_task_meta(&name, &url, &dir.to_string_lossy(), &name)
        .await;

    let entry = Arc::new(TaskEntry {
        id: name.clone(),
        url: url.clone(),
        file_name: name.clone(),
        save_dir: dir,
        final_path,
        temp_path,
        total_bytes: AtomicI64::new(info.total_bytes),
        downloaded_bytes: AtomicI64::new(0),
        status: AtomicI32::new(5),
        error_message: Mutex::new(String::new()),
        cancel: Mutex::new(CancellationToken::new()),
    });
    tasks().lock().unwrap().insert(entry.id.clone(), entry.clone());

    spawn_runner(app, entry.clone(), client, info, spec, db, max_segments.unwrap_or(0));

    let snap = {
        let map = tasks().lock().unwrap();
        snapshot(map.get(&entry.id).unwrap(), 0, Vec::new())
    };
    Ok(snap)
}

#[tauri::command]
pub async fn accel_pause(id: String) -> Result<(), String> {
    let entry = tasks().lock().unwrap().get(&id).cloned();
    match entry {
        Some(t) => {
            t.token().cancel();
            Ok(())
        }
        None => Err(format!("任务不存在: {id}")),
    }
}

#[tauri::command]
pub async fn accel_resume(app: AppHandle, id: String) -> Result<(), String> {
    // 内存注册表优先；未命中（应用重启后）从 DB 元数据重建
    let entry = {
        let map = tasks().lock().unwrap();
        map.get(&id).cloned()
    };
    let db = Db::open(&engine_dir()).await.map_err(|e| e.to_string())?;

    let entry = match entry {
        Some(entry) => {
            let status = entry.status.load(Ordering::Relaxed);
            if status != 2 && status != 4 {
                return Err("仅暂停/失败的任务可以继续".into());
            }
            entry
        }
        None => {
            let Some(total) = db
                .list_unfinished_tasks()
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .find(|(tid, ..)| *tid == id)
                .map(|(_, _, _, _, total, _)| total)
            else {
                return Err(format!("任务不存在或已完成: {id}"));
            };
            let (url, save_dir, file_name) = db
                .get_task_meta(&id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("任务元数据缺失: {id}"))?;
            let dir = PathBuf::from(&save_dir);
            let final_path = dir.join(&file_name);
            let temp_path = PathBuf::from(format!("{}{}", final_path.display(), TEMP_EXT));
            let entry = Arc::new(TaskEntry {
                id: id.clone(),
                url,
                file_name,
                save_dir: dir,
                final_path,
                temp_path,
                total_bytes: AtomicI64::new(total),
                downloaded_bytes: AtomicI64::new(0),
                status: AtomicI32::new(5),
                error_message: Mutex::new(String::new()),
                cancel: Mutex::new(CancellationToken::new()),
            });
            tasks().lock().unwrap().insert(entry.id.clone(), entry.clone());
            entry
        }
    };

    let proxy = ProxyConfig {
        mode: nx_dl_engine::proxy_config::ProxyMode::System,
        ..Default::default()
    };
    let client = build_client(&proxy, "").map_err(|e| e.to_string())?;
    let spec = RequestSpec::empty_get();

    let total = entry.total_bytes.load(Ordering::Relaxed);
    let range_verified = db.get_task_range_verified(&id).await.unwrap_or(true);
    let (etag, last_modified) = db.get_task_validator(&id).await.unwrap_or_default();
    let info = nx_dl_engine::downloader::FileInfo {
        file_name: entry.file_name.clone(),
        total_bytes: total,
        supports_range: if total > 0 { range_verified } else { false },
        content_type: String::new(),
        etag,
        last_modified,
        content_encoding_compressed: false,
    };
    entry.status.store(5, Ordering::Relaxed);
    spawn_runner(app, entry, client, info, spec, db, 0);
    Ok(())
}

/// 移除任务：退出注册表 + 清持久层分段/元数据 + 延迟删除临时文件。
/// 下载中移除同样生效（先取消令牌，worker 逐 chunk 检查取消后很快退出，
/// 延迟删除避开 Windows 上句柄未释放导致的删除失败）。
#[tauri::command]
pub async fn accel_cancel(id: String) -> Result<(), String> {
    let removed = tasks().lock().unwrap().remove(&id);
    match removed {
        Some(entry) => {
            entry.token().cancel();
            entry.status.store(4, Ordering::Relaxed);
            // 持久层立即清理：worker 后续的段进度写入因行不存在静默失效
            // （与 epoch 守卫同语义），不会复活已删除任务。
            let _ = remove_state_rows(&entry.id).await;
            let temp = entry.temp_path.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(1500)).await;
                let _ = tokio::fs::remove_file(&temp).await;
            });
            Ok(())
        }
        None => Err(format!("任务不存在: {id}")),
    }
}

#[tauri::command]
pub async fn accel_list() -> Vec<AccelTaskSnapshot> {
    let map = tasks().lock().unwrap();
    map.values()
        .map(|t| snapshot(t, 0, Vec::new()))
        .collect()
}

/// 清除域名连接策略学习缓存（正面 hint + 负面 cap，24h TTL）。
/// 用于服务器行为变化或缓存被历史高并发记录污染后手动复位。
#[tauri::command]
pub async fn accel_clear_learned() -> Result<(), String> {
    let db = Db::open(&engine_dir()).await.map_err(|e| e.to_string())?;
    nx_dl_engine::segment_coordinator::clear_domain_conn_caps(&db);
    Ok(())
}

/// 用系统默认程序打开已下载的文件（ShellExecuteW，与启动安装器同模式）。
#[tauri::command]
pub fn accel_open_file(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        if !std::path::Path::new(&path).exists() {
            return Err(format!("文件不存在: {path}"));
        }
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let path_wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        let verb: Vec<u16> = "open\0".encode_utf16().collect();
        let hinst = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                path_wide.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        if hinst as isize <= 32 {
            return Err(format!("打开失败（错误码 {}）", hinst as isize));
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        Err("仅支持 Windows".into())
    }
}

/// 在资源管理器中定位并选中文件（explorer /select）。
#[tauri::command]
pub fn accel_reveal_file(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        if !std::path::Path::new(&path).exists() {
            return Err(format!("文件不存在: {path}"));
        }
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{}\"", path))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("打开目录失败: {e}"))?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        Err("仅支持 Windows".into())
    }
}

/// 全局限速（字节/秒，0 = 不限速）。Token bucket 对所有并发任务生效。
#[tauri::command]
pub async fn accel_set_speed_limit(limit_bps: u64) -> Result<(), String> {
    LIMITER.set_limit(limit_bps);
    Ok(())
}

/// 重启后扫描可续传任务：DB 有分段行、未完成、临时文件仍在。
#[tauri::command]
pub async fn accel_scan_unfinished() -> Vec<AccelTaskSnapshot> {
    let Ok(db) = Db::open(&engine_dir()).await else {
        return Vec::new();
    };
    let Ok(rows) = db.list_unfinished_tasks().await else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (id, url, save_dir, file_name, total, done) in rows {
        let dir = PathBuf::from(&save_dir);
        let final_path = dir.join(&file_name);
        if final_path.exists() {
            continue;
        }
        let temp_path = PathBuf::from(format!("{}{}", final_path.display(), TEMP_EXT));
        if !temp_path.exists() {
            continue;
        }
        // 注册表里活跃的同名任务以内存态为准
        if tasks().lock().unwrap().contains_key(&id) {
            continue;
        }
        out.push(AccelTaskSnapshot {
            id,
            url,
            file_name,
            save_dir,
            total_bytes: total,
            downloaded_bytes: done,
            speed_bps: 0,
            status: STATUS_PAUSED,
            error_message: String::new(),
            segments: Vec::new(),
        });
    }
    out
}

async fn remove_state_rows(task_id: &str) -> Result<(), String> {
    let db = Db::open(&engine_dir()).await.map_err(|e| e.to_string())?;
    db.delete_segments(task_id).await.map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// 下载执行
// ---------------------------------------------------------------------------

fn spawn_runner(
    app: AppHandle,
    entry: Arc<TaskEntry>,
    client: nx_dl_engine::reqwest::Client,
    info: nx_dl_engine::downloader::FileInfo,
    spec: RequestSpec,
    db: Db,
    max_segments: i32,
) {
    let token = entry.swap_token();
    tokio::spawn(async move {
        run_task(app, entry, client, info, spec, db, max_segments, token).await;
    });
}

#[allow(clippy::too_many_arguments)]
async fn run_task(
    app: AppHandle,
    entry: Arc<TaskEntry>,
    client: nx_dl_engine::reqwest::Client,
    info: nx_dl_engine::downloader::FileInfo,
    spec: RequestSpec,
    db: Db,
    max_segments: i32,
    cancel: CancellationToken,
) {
    let sink = Arc::new(TauriSink {
        app: app.clone(),
        task_id: entry.id.clone(),
    });

    entry.status.store(1, Ordering::Relaxed);
    emit_snapshot(&app, &entry, 0, Vec::new());

    let use_segments = info.supports_range && info.total_bytes > 2 * 1024 * 1024;
    let sink_dyn: Arc<dyn EventSink> = sink.clone();
    let mut result = if use_segments {
        download_multi(
            &app, &entry, &client, &info, &spec, &db, &sink_dyn, &cancel, max_segments,
        )
        .await
    } else {
        single_stream(&app, &entry, &client, &spec, &cancel).await
    };

    // 任务级瞬态重试（上游 run_download MAX_RETRIES=5/base 2s 的收敛版）：
    // 仅网络类错误（连接失败/IO），Range 硬失败与取消不在此列
    for attempt in 1..=3u32 {
        if !is_transient(&result) || cancel.is_cancelled() {
            break;
        }
        let delay = Duration::from_secs(2u64.saturating_pow(attempt));
        log::info!("[accel] task {} transient error, retry {attempt}/3 in {delay:?}: {:?}", entry.id, result.as_ref().err());
        tokio::time::sleep(delay).await;
        result = if use_segments {
            download_multi(
                &app, &entry, &client, &info, &spec, &db, &sink_dyn, &cancel, max_segments,
            )
            .await
        } else {
            single_stream(&app, &entry, &client, &spec, &cancel).await
        };
    }

    match result {
        Ok(bytes) => {
            entry
                .downloaded_bytes
                .store(bytes as i64, Ordering::Relaxed);
            // 完整性校验 + 改名落盘
            if info.total_bytes > 0 && bytes != info.total_bytes as u64 {
                let msg = format!("下载不完整：预期 {} 字节，实际 {bytes}", info.total_bytes);
                finish_error(&app, &entry, msg).await;
                return;
            }
            if let Err(e) = finalize_file(&entry.temp_path, &entry.final_path).await {
                finish_error(&app, &entry, format!("落盘失败：{e}")).await;
                return;
            }
            entry.status.store(STATUS_COMPLETED, Ordering::Relaxed);
            let done = entry.total_bytes.load(Ordering::Relaxed);
            entry.downloaded_bytes.store(done.max(bytes as i64), Ordering::Relaxed);
            // 服务器时间写入（上游 use_server_time 语义：保留服务器 mtime）
            if !info.last_modified.is_empty() {
                apply_server_mtime(&entry.final_path, &info.last_modified);
            }
            emit_snapshot(&app, &entry, 0, full_segments(done));
        }
        Err(err) => {
            if err_is_cancel(&err)
                || cancel.is_cancelled()
            {
                // 暂停：从持久层读回真实分段布局推送前端。
                // 推空数组会让 UI 回退成"整文件单块"，视觉上所有线程合并成一个。
                entry.status.store(2, Ordering::Relaxed);
                let segs = match db.load_segments(&entry.id).await {
                    Ok(rows) => rows,
                    Err(_) => Vec::new(),
                };
                let dtos: Vec<SegmentDto> = segs
                    .into_iter()
                    .map(|s| SegmentDto {
                        index: s.index,
                        start_byte: s.start_byte,
                        end_byte: s.end_byte,
                        downloaded_bytes: s.downloaded_bytes,
                    })
                    .collect();
                emit_snapshot(&app, &entry, 0, dtos);
                return;
            }
            // Range 类硬失败 → 清理后单流重试一次
            if range_fatal(&err) {
                log::info!(
                    "[accel] task {} multi-segment fatal ({err}) → single-stream retry",
                    entry.id
                );
                let _ = tokio::fs::remove_file(&entry.temp_path).await;
                let _ = db.delete_segments(&entry.id).await;
                match single_stream(&app, &entry, &client, &spec, &cancel).await {
                    Ok(bytes) => {
                        if info.total_bytes > 0 && bytes != info.total_bytes as u64 {
                            finish_error(
                                &app,
                                &entry,
                                format!("下载不完整：预期 {} 字节，实际 {bytes}", info.total_bytes),
                            )
                            .await;
                            return;
                        }
                        if let Err(e) = finalize_file(&entry.temp_path, &entry.final_path).await {
                            finish_error(&app, &entry, format!("落盘失败：{e}")).await;
                            return;
                        }
                        entry.status.store(STATUS_COMPLETED, Ordering::Relaxed);
                        emit_snapshot(&app, &entry, 0, Vec::new());
                        return;
                    }
                    Err(e2) => {
                        finish_error(&app, &entry, format!("{e2}")).await;
                        return;
                    }
                }
            }
            finish_error(&app, &entry, format!("{err}")).await;
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn download_multi(
    app: &AppHandle,
    entry: &Arc<TaskEntry>,
    client: &nx_dl_engine::reqwest::Client,
    info: &nx_dl_engine::downloader::FileInfo,
    spec: &RequestSpec,
    db: &Db,
    sink: &Arc<dyn EventSink>,
    cancel: &CancellationToken,
    max_segments: i32,
) -> Result<u64, DownloadError> {
    /// 自动档（max_segments==0）的推荐连接数天花板。上游 advisor 在
    /// 大文件 + 多核机器上几乎必推 64，对国内风控型服务器过于激进；
    /// 且正面 hint 学习会让后续任务直接以历史峰值起步。实测风控型
    /// 服务器甜点普遍在 2~8，钳到 8 后 ramp 在此区间内自适应探索，
    /// 用户仍可手动锁 16~64（信任其判断）。
    const AUTO_SEGMENT_CEILING: i32 = 8;

    let advice = advise_static(&AdvisorInput {
        total_bytes: info.total_bytes,
        supports_range: info.supports_range,
    });
    let count = if max_segments > 0 {
        advice.segments.min(max_segments)
    } else {
        advice.segments.min(AUTO_SEGMENT_CEILING)
    };
    log::info!(
        "[accel] task {} plan segments={} ({})",
        entry.id,
        count,
        advice.reason
    );

    let nodes = NodePool::single(client.clone());
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<ProgressUpdate>(256);

    // 进度泵：协调器 ~200ms 一帧 → 节流转发前端
    let pump_app = app.clone();
    let pump_entry = entry.clone();
    let pump_cancel = cancel.clone();
    let pump = tokio::spawn(async move {
        let mut last_emit = Instant::now() - Duration::from_secs(1);
        let mut last_bytes: i64 = 0;
        let mut last_time = Instant::now();
        let mut speed_ema: f64 = 0.0;
        while let Some(up) = progress_rx.recv().await {
            pump_entry
                .total_bytes
                .store(up.total_bytes, Ordering::Relaxed);
            pump_entry
                .downloaded_bytes
                .store(up.downloaded_bytes, Ordering::Relaxed);
            let now = Instant::now();
            let dt = now.duration_since(last_time).as_secs_f64();
            if up.downloaded_bytes >= last_bytes && dt > 0.05 {
                let inst = (up.downloaded_bytes - last_bytes) as f64 / dt;
                speed_ema = if speed_ema == 0.0 {
                    inst
                } else {
                    speed_ema * 0.7 + inst * 0.3
                };
                last_bytes = up.downloaded_bytes;
                last_time = now;
            }
            let status_now = pump_entry.status.load(Ordering::Relaxed);
            let terminal = status_now == 2 || status_now == 3 || status_now == 4;
            if terminal || now.duration_since(last_emit).as_millis() >= 200 {
                last_emit = now;
                let segs: Vec<SegmentDto> = up
                    .segment_details
                    .iter()
                    .flatten()
                    .map(|s| SegmentDto {
                        index: s.index,
                        start_byte: s.start_byte,
                        end_byte: s.end_byte,
                        downloaded_bytes: s.downloaded_bytes,
                    })
                    .collect();
                emit_snapshot(
                    &pump_app,
                    &pump_entry,
                    speed_ema as i64,
                    segs,
                );
            }
            if pump_cancel.is_cancelled() {
                break;
            }
        }
    });

    let gen = SPAWN_GEN.fetch_add(1, Ordering::Relaxed);
    let result = run_coordinated_download(
        &entry.id,
        &entry.url,
        &entry.temp_path,
        info.total_bytes,
        false,
        count,
        nodes,
        db,
        &progress_tx,
        cancel,
        &LIMITER,
        spec,
        sink.as_ref(),
        &info.etag,
        &info.last_modified,
        ReportScope::whole_task(),
        gen,
        false,
        None,
    )
    .await;

    drop(progress_tx);
    let _ = pump.await;
    result.map(|b| b as u64)
}

/// 单流回退（不支持 Range / Range 硬失败时）。
async fn single_stream(
    app: &AppHandle,
    entry: &Arc<TaskEntry>,
    client: &nx_dl_engine::reqwest::Client,
    spec: &RequestSpec,
    cancel: &CancellationToken,
) -> Result<u64, DownloadError> {
    let resp = nx_dl_engine::downloader::build_request(client, &entry.url, spec.method.clone(), spec)
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(DownloadError::Other(format!("HTTP {}", resp.status())));
    }
    let total = resp.content_length().unwrap_or(0);
    entry.total_bytes.store(total as i64, Ordering::Relaxed);

    let mut file = tokio::fs::File::create(&entry.temp_path).await?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_emit = Instant::now() - Duration::from_secs(1);
    let mut last_bytes: u64 = 0;
    let mut last_time = Instant::now();
    let mut speed_ema: f64 = 0.0;

    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;
    while let Some(chunk) = stream.next().await {
        if cancel.is_cancelled() {
            return Err(DownloadError::Cancelled);
        }
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        entry.downloaded_bytes.store(downloaded as i64, Ordering::Relaxed);

        let now = Instant::now();
        let dt = now.duration_since(last_time).as_secs_f64();
        if dt > 0.05 {
            let inst = (downloaded - last_bytes) as f64 / dt;
            speed_ema = if speed_ema == 0.0 { inst } else { speed_ema * 0.7 + inst * 0.3 };
            last_bytes = downloaded;
            last_time = now;
        }
        if now.duration_since(last_emit).as_millis() >= 200 {
            last_emit = now;
            let seg = vec![SegmentDto {
                index: 0,
                start_byte: 0,
                end_byte: total as i64 - 1,
                downloaded_bytes: downloaded as i64,
            }];
            let snap = snapshot(entry, speed_ema as i64, seg);
            let _ = app.emit(PROGRESS_EVENT, &snap);
        }
    }
    file.sync_all().await?;
    Ok(downloaded)
}

fn err_is_cancel(e: &DownloadError) -> bool {
    matches!(e, DownloadError::Cancelled)
}

/// 网络类瞬态错误（可安全整任务重试，续传接续进度）。
fn is_transient(result: &Result<u64, DownloadError>) -> bool {
    matches!(
        result,
        Err(DownloadError::Request(_) | DownloadError::Io(_) | DownloadError::CdnNodeFailed(_))
    )
}

/// 磁盘可用空间（Windows GetDiskFreeSpaceExW；其他平台返回 None 跳过预检）。
#[cfg(windows)]
fn free_disk_bytes(path: &std::path::Path) -> Option<u64> {
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = path
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut free: u64 = 0;
    let ok = unsafe {
        GetDiskFreeSpaceExW(wide.as_ptr(), std::ptr::null_mut(), std::ptr::null_mut(), &mut free)
    };
    if ok != 0 { Some(free) } else { None }
}

#[cfg(not(windows))]
fn free_disk_bytes(_path: &std::path::Path) -> Option<u64> {
    None
}

fn format_bytes_cn(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1} {}", UNITS[i])
}

/// 把服务器 Last-Modified 写为文件修改时间（上游 use_server_time 语义）。
fn apply_server_mtime(path: &std::path::Path, last_modified: &str) {
    use chrono::DateTime;
    match DateTime::parse_from_rfc2822(last_modified) {
        Ok(dt) => {
            let ft = filetime::FileTime::from_unix_time(dt.timestamp(), 0);
            if let Err(e) = filetime::set_file_mtime(path, ft) {
                log::info!("[accel] set mtime failed for {}: {e}", path.display());
            }
        }
        Err(e) => log::info!("[accel] parse Last-Modified failed ({last_modified}): {e}"),
    }
}

/// 需要清盘改单流的硬失败类型（与上游语义一致）。
fn range_fatal(e: &DownloadError) -> bool {
    matches!(
        e,
        DownloadError::RangeNotSupported(_)
            | DownloadError::RangeMisaligned(_)
            | DownloadError::VersionChanged(_)
    )
}

async fn finalize_file(temp: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    if let Ok(f) = tokio::fs::OpenOptions::new().append(true).open(temp).await {
        let _ = f.sync_all().await;
    }
    tokio::fs::rename(temp, dest).await
}

async fn finish_error(app: &AppHandle, entry: &Arc<TaskEntry>, msg: String) {
    *entry.error_message.lock().unwrap() = msg;
    entry.status.store(4, Ordering::Relaxed);
    emit_snapshot(app, entry, 0, Vec::new());
}

fn full_segments(total: i64) -> Vec<SegmentDto> {
    vec![SegmentDto {
        index: 0,
        start_byte: 0,
        end_byte: total.saturating_sub(1),
        downloaded_bytes: total,
    }]
}

fn emit_snapshot(app: &AppHandle, entry: &Arc<TaskEntry>, speed_bps: i64, segments: Vec<SegmentDto>) {
    let snap = snapshot(entry, speed_bps, segments);
    let _ = app.emit(PROGRESS_EVENT, &snap);
}
