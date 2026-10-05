//! 文件互传（文件篮模式）：PC ↔ 已配对手机之间的局域网文件暂存与回执。
//!
//! - 每台设备一个独立目录 `app_data_dir/remote-access/transfer/<device_id>/`：
//!   - `outbox/`：PC 添加、等待手机下载（方向 `to_device`）
//!   - `inbox/`：手机上传、等待 PC 另存为（方向 `from_device`）
//!   - `index.json`：条目元数据，重启后列表可恢复
//! - 上传/下载全程流式（multipart 边收边写、`ReaderStream` 分块读），不整块进内存；
//!   落盘先写 `<name>.part` 临时名，完成后原子 rename，断网不留半截文件。
//! - 「已接收」回执：手机下载完成调 ack；PC 另存为完成标记 saved。两端状态由
//!   WS `transfer.update` 帧与 `remote-access://transfer-changed` 事件驱动刷新。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use rand::Rng;
use serde::{Deserialize, Serialize};
use tauri::Manager;

/// 单个互传文件条目（index.json 元素 + 前端/手机端通用视图）。
#[derive(Serialize, Deserialize, Clone)]
pub struct TransferFile {
    pub id: String,
    /// 落盘文件名（重名时已加序号，同时作为显示名）
    pub name: String,
    pub size: u64,
    /// `to_device`：PC 发往手机；`from_device`：手机发来
    pub direction: String,
    pub created_at: u64,
    /// 接收方已确认接收（手机下载完成 / PC 已另存为）
    pub acked: bool,
    /// PC 端「另存为」后的目标路径（仅 from_device 有意义）
    #[serde(default)]
    pub saved_path: String,
}

/// PC 视角的列表：发往手机的 / 手机发来的。
#[derive(Serialize, Clone, Default)]
pub struct TransferLists {
    pub to_device: Vec<TransferFile>,
    pub from_device: Vec<TransferFile>,
}

#[derive(Default)]
struct Runtime {
    /// device_id -> 条目列表（与 index.json 同步）
    files: HashMap<String, Vec<TransferFile>>,
}

static DB: Mutex<Option<Runtime>> = Mutex::new(None);

/// 进行中的传输进度（key = `<device_id>:<file_id>`）。
/// 手机下载走 HTTP 上报；手机上传由 PC 端 multipart handler 自己写入。
/// 仅内存，完成后由 ack / finalize / remove 清除。
#[derive(Serialize, Clone)]
pub struct TransferProgress {
    pub done: u64,
    pub total: u64,
    /// 字节/秒
    pub speed: f64,
    pub updated_at: u64,
}

static PROGRESS: Mutex<Option<HashMap<String, TransferProgress>>> = Mutex::new(None);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn gen_id() -> String {
    let mut rng = rand::thread_rng();
    format!("{:016x}", rng.gen::<u64>())
}

fn transfer_root(app: &tauri::AppHandle) -> Option<PathBuf> {
    Some(app.path().app_data_dir().ok()?.join("remote-access").join("transfer"))
}

fn device_dir(app: &tauri::AppHandle, device_id: &str) -> Result<PathBuf, String> {
    // device_id 由服务端签发（hex），这里再兜一道，防异常输入越出 transfer 根目录
    if device_id.is_empty() || !device_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("非法设备标识".into());
    }
    transfer_root(app)
        .map(|root| root.join(device_id))
        .ok_or_else(|| "应用数据目录不可用".into())
}

/// init 时扫描并载入所有设备的 index.json（设备撤销时目录会被清掉，孤儿目录无害）。
pub fn load_all(app: &tauri::AppHandle) {
    let mut files = HashMap::new();
    if let Some(root) = transfer_root(app) {
        if let Ok(entries) = std::fs::read_dir(&root) {
            for e in entries.flatten() {
                let idx = e.path().join("index.json");
                if let Ok(s) = std::fs::read_to_string(&idx) {
                    if let Ok(list) = serde_json::from_str::<Vec<TransferFile>>(&s) {
                        files.insert(e.file_name().to_string_lossy().to_string(), list);
                    }
                }
            }
        }
    }
    DB.lock().get_or_insert_with(Runtime::default).files = files;
}

fn save_index(app: &tauri::AppHandle, device_id: &str, list: &[TransferFile]) {
    let Ok(dir) = device_dir(app, device_id) else { return };
    let _ = std::fs::create_dir_all(&dir);
    match serde_json::to_string_pretty(list) {
        Ok(content) => {
            if let Err(e) = std::fs::write(dir.join("index.json"), content) {
                log::warn!("[RemoteAccess] 互传索引写盘失败: {e}");
            }
        }
        Err(e) => log::warn!("[RemoteAccess] 互传索引序列化失败: {e}"),
    }
}

/// 去掉路径成分与 Windows 非法字符，防止 multipart 文件名注入路径。
fn sanitize_name(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let mut s: String = base
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '|' | '?' | '*' | '\0'..='\x1f' => '_',
            c => c,
        })
        .collect();
    // Windows 不允许文件名以点/空格结尾
    while s.ends_with('.') || s.ends_with(' ') {
        s.pop();
    }
    if s.chars().count() > 120 {
        let ext = Path::new(&s)
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        let stem: String = s.chars().take(120 - ext.chars().count()).collect();
        s = format!("{stem}{ext}");
    }
    s
}

/// 在现有名字集合上派生不重名的名字：`a.txt` → `a (1).txt`。
fn unique_name(taken: &[String], name: &str) -> String {
    if !taken.iter().any(|t| t.eq_ignore_ascii_case(name)) {
        return name.to_string();
    }
    let stem = Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| name.to_string());
    let ext = Path::new(name)
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    for n in 1..1000u32 {
        let cand = format!("{stem} ({n}){ext}");
        if !taken.iter().any(|t| t.eq_ignore_ascii_case(&cand)) {
            return cand;
        }
    }
    format!("{stem} ({}){ext}", gen_id())
}

fn push_entry(app: &tauri::AppHandle, device_id: &str, mut entry: TransferFile) -> TransferFile {
    let mut guard = DB.lock();
    let rt = guard.get_or_insert_with(Runtime::default);
    let list = rt.files.entry(device_id.to_string()).or_default();
    entry.name = unique_name(&list.iter().map(|f| f.name.clone()).collect::<Vec<_>>(), &entry.name);
    // 重名序号是在整表上取的，方向混存也没问题
    list.push(entry.clone());
    save_index(app, device_id, list);
    entry
}

/// PC 添加文件：拷贝进 outbox。paths 为本机绝对路径。
pub async fn add_outbox(device_id: &str, paths: &[String]) -> Result<Vec<TransferFile>, String> {
    let app = super::app_handle().ok_or("远程接入服务尚未初始化")?;
    let dir = device_dir(&app, device_id)?.join("outbox");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建互传目录失败: {e}"))?;

    let mut added = Vec::new();
    for src in paths {
        let src_path = PathBuf::from(src.trim().trim_matches('"'));
        let meta = tokio::fs::metadata(&src_path)
            .await
            .map_err(|e| format!("读取 {} 失败: {e}", src_path.display()))?;
        if !meta.is_file() {
            return Err(format!("{} 不是文件", src_path.display()));
        }
        let raw_name = src_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        // 先用占位名锁好条目名，再落盘，保证列表名与磁盘名一致
        let entry = push_entry(
            &app,
            device_id,
            TransferFile {
                id: gen_id(),
                name: sanitize_name(&raw_name),
                size: meta.len(),
                direction: "to_device".into(),
                created_at: now_ms(),
                acked: false,
                saved_path: String::new(),
            },
        );
        let dst = dir.join(&entry.name);
        match tokio::fs::copy(&src_path, &dst).await {
            Ok(real) => {
                let mut done = entry.clone();
                done.size = real;
                update_entry(&app, device_id, &done);
                added.push(done);
            }
            Err(e) => {
                remove(device_id, &entry.id).ok();
                return Err(format!("拷贝 {} 失败: {e}", src_path.display()));
            }
        }
    }
    Ok(added)
}

/// 手机上传开始：分配条目并返回 .part 临时路径（handler 流式写入）。
pub fn alloc_incoming(device_id: &str, raw_name: &str) -> Result<(TransferFile, PathBuf), String> {
    let app = super::app_handle().ok_or("远程接入服务尚未初始化")?;
    let dir = device_dir(&app, device_id)?.join("inbox");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建互传目录失败: {e}"))?;
    let entry = push_entry(
        &app,
        device_id,
        TransferFile {
            id: gen_id(),
            name: sanitize_name(raw_name),
            size: 0,
            direction: "from_device".into(),
            created_at: now_ms(),
            acked: false,
            saved_path: String::new(),
        },
    );
    let part = dir.join(format!("{}.part", entry.name));
    Ok((entry, part))
}

/// 上传完成：rename 掉 .part 并回填真实大小。
pub fn finalize_incoming(device_id: &str, id: &str, size: u64) -> Result<TransferFile, String> {
    let app = super::app_handle().ok_or("远程接入服务尚未初始化")?;
    let dir = device_dir(&app, device_id)?.join("inbox");
    let mut entry = find(device_id, id).ok_or("条目不存在")?;
    let final_path = dir.join(&entry.name);
    let part = dir.join(format!("{}.part", entry.name));
    std::fs::rename(&part, &final_path).map_err(|e| format!("落盘失败: {e}"))?;
    entry.size = size;
    update_entry(&app, device_id, &entry);
    clear_progress(device_id, id);
    Ok(entry)
}

/// 上传中断：清理条目与半截文件。
pub fn abort_incoming(device_id: &str, id: &str) {
    if let Some(entry) = find(device_id, id) {
        remove(device_id, id).ok();
        if let Some(app) = super::app_handle() {
            if let Ok(dir) = device_dir(&app, device_id) {
                let _ = std::fs::remove_file(dir.join("inbox").join(format!("{}.part", entry.name)));
            }
        }
    }
}

/// 条目落盘名对应的物理路径（download handler / 另存为 / reveal 用）。
pub fn file_path(device_id: &str, entry: &TransferFile) -> Result<PathBuf, String> {
    let app = super::app_handle().ok_or("远程接入服务尚未初始化")?;
    let sub = if entry.direction == "to_device" { "outbox" } else { "inbox" };
    Ok(device_dir(&app, device_id)?.join(sub).join(&entry.name))
}

fn update_entry(app: &tauri::AppHandle, device_id: &str, entry: &TransferFile) {
    let mut guard = DB.lock();
    let rt = guard.get_or_insert_with(Runtime::default);
    let list = rt.files.entry(device_id.to_string()).or_default();
    if let Some(slot) = list.iter_mut().find(|f| f.id == entry.id) {
        *slot = entry.clone();
    }
    save_index(app, device_id, list);
}

pub fn find(device_id: &str, id: &str) -> Option<TransferFile> {
    DB.lock()
        .as_ref()?
        .files
        .get(device_id)?
        .iter()
        .find(|f| f.id == id)
        .cloned()
}

/// 手机视角列表：`incoming` = PC 发来的（to_device），`outgoing` = 本机已发送的（from_device）。
pub fn phone_view(device_id: &str) -> TransferLists {
    let guard = DB.lock();
    let list = guard
        .as_ref()
        .and_then(|rt| rt.files.get(device_id))
        .cloned()
        .unwrap_or_default();
    TransferLists {
        to_device: list.iter().filter(|f| f.direction == "to_device").cloned().collect(),
        from_device: list.iter().filter(|f| f.direction == "from_device").cloned().collect(),
    }
}

/// 标记接收方已确认（手机下载完成 / PC 已另存为）。
pub fn mark_acked(device_id: &str, id: &str) -> Option<TransferFile> {
    let app = super::app_handle()?;
    let mut entry = find(device_id, id)?;
    entry.acked = true;
    update_entry(&app, device_id, &entry);
    clear_progress(device_id, id);
    Some(entry)
}

/// PC 另存为完成：记录目标路径并标记已接收（手机端显示「已接收」）。
pub fn mark_saved(device_id: &str, id: &str, dest: &Path) -> Result<TransferFile, String> {
    let app = super::app_handle().ok_or("远程接入服务尚未初始化")?;
    let mut entry = find(device_id, id).ok_or("条目不存在")?;
    entry.acked = true;
    entry.saved_path = dest.to_string_lossy().to_string();
    update_entry(&app, device_id, &entry);
    Ok(entry)
}

/// 删除条目与实体文件。返回被删条目（供日志/回执）。
pub fn remove(device_id: &str, id: &str) -> Result<TransferFile, String> {
    let app = super::app_handle().ok_or("远程接入服务尚未初始化")?;
    let entry = find(device_id, id).ok_or("条目不存在")?;
    {
        let mut guard = DB.lock();
        let rt = guard.get_or_insert_with(Runtime::default);
        let list = rt.files.entry(device_id.to_string()).or_default();
        list.retain(|f| f.id != id);
        save_index(&app, device_id, list);
    }
    clear_progress(device_id, id);
    if let Ok(path) = file_path(device_id, &entry) {
        let _ = std::fs::remove_file(path);
    }
    Ok(entry)
}

// ───────────────────────── 传输进度（两端互见） ─────────────────────────

fn progress_key(device_id: &str, file_id: &str) -> String {
    format!("{device_id}:{file_id}")
}

/// 记录一帧进度（手机 HTTP 上报 / multipart handler 写入）。
/// 条目已不存在（被删）时返回 false 丢弃。总大小未知传 0。
pub fn note_progress(device_id: &str, file_id: &str, done: u64, total: u64, speed: f64) -> bool {
    if find(device_id, file_id).is_none() {
        return false;
    }
    let mut guard = PROGRESS.lock();
    let map = guard.get_or_insert_with(HashMap::new);
    map.insert(
        progress_key(device_id, file_id),
        TransferProgress {
            done,
            total,
            speed,
            updated_at: now_ms(),
        },
    );
    true
}

pub fn clear_progress(device_id: &str, file_id: &str) {
    if let Some(map) = PROGRESS.lock().as_mut() {
        map.remove(&progress_key(device_id, file_id));
    }
}

/// 全部设备中尚未保存的手机来件数（首页卡片角标用）。
pub fn unsaved_count() -> usize {
    DB.lock()
        .as_ref()
        .map(|rt| {
            rt.files
                .values()
                .flatten()
                .filter(|f| f.direction == "from_device" && f.saved_path.is_empty())
                .count()
        })
        .unwrap_or(0)
}

/// 设备撤销时清理其整个互传目录（含 outbox/inbox/index）。
pub fn cleanup_device(device_id: &str) {
    if let Some(app) = super::app_handle() {
        if let Ok(dir) = device_dir(&app, device_id) {
            if dir.starts_with(transfer_root(&app).unwrap_or_default()) {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
    }
    DB.lock().as_mut().map(|rt| rt.files.remove(device_id));
}

// ───────────────────────── Tauri commands ─────────────────────────

fn broadcast_to_device(device_id: &str) {
    super::api::broadcast(
        serde_json::json!({ "type": "transfer.update", "device_id": device_id }).to_string(),
    );
}

fn notify_frontend() {
    if let Some(app) = super::app_handle() {
        use tauri::Emitter;
        let _ = app.emit("remote-access://transfer-changed", ());
    }
}

/// PC 添加文件到「发往手机」列表（弹窗第二页按钮）。
#[tauri::command]
pub async fn cmd_transfer_add_files(device_id: String, paths: Vec<String>) -> Result<Vec<TransferFile>, String> {
    let added = add_outbox(&device_id, &paths).await?;
    if !added.is_empty() {
        broadcast_to_device(&device_id);
        notify_frontend();
    }
    Ok(added)
}

/// PC 端列表（弹窗第二页 + 首页卡片角标）。
#[tauri::command]
pub async fn cmd_transfer_list(device_id: String) -> Result<TransferLists, String> {
    Ok(phone_view(&device_id))
}

/// PC 删除条目（两个方向都可删）。
#[tauri::command]
pub async fn cmd_transfer_remove(device_id: String, file_id: String) -> Result<TransferLists, String> {
    remove(&device_id, &file_id)?;
    broadcast_to_device(&device_id);
    notify_frontend();
    Ok(phone_view(&device_id))
}

/// PC「另存为」：把手机来件拷到用户选的位置，完成后标记已接收并通知手机。
#[tauri::command]
pub async fn cmd_transfer_save_file(
    device_id: String,
    file_id: String,
    dest_path: String,
) -> Result<TransferFile, String> {
    let entry = find(&device_id, &file_id).ok_or("条目不存在")?;
    let src = file_path(&device_id, &entry)?;
    let dest = PathBuf::from(dest_path.trim().trim_matches('"'));
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
    }
    tokio::fs::copy(&src, &dest)
        .await
        .map_err(|e| format!("保存失败: {e}"))?;
    let done = mark_saved(&device_id, &file_id, &dest)?;
    broadcast_to_device(&device_id);
    notify_frontend();
    Ok(done)
}

/// 打开已保存文件的所在位置；未保存过时打开暂存文件本身。
#[tauri::command]
pub async fn cmd_transfer_reveal(device_id: String, file_id: String) -> Result<(), String> {
    let entry = find(&device_id, &file_id).ok_or("条目不存在")?;
    let path = if entry.saved_path.is_empty() {
        file_path(&device_id, &entry)?
    } else {
        PathBuf::from(&entry.saved_path)
    };
    tauri_plugin_opener::reveal_item_in_dir(&path).map_err(|e| format!("定位失败: {e}"))
}

/// 全部设备未保存来件总数（首页「手机远程」卡片副标题）。
#[tauri::command]
pub async fn cmd_transfer_unsaved_count() -> Result<usize, String> {
    Ok(unsaved_count())
}
