//! 远程接入：统一「当前播放」——软件内播放器 / 外部 SMTC 二选一。
//!
//! 手机端只需要一条「当前播放」，优先级为：
//! 软件内在播 → 外部在播 → 软件内(暂停) → 外部(暂停) → 无。
//!
//! 两个来源的状态都来自各自模块的全局缓存：
//! - 软件内：`crate::smtc::current_state()`（前端 zustand 每秒推送，见 `smtc_update_state`）
//! - 外部：`crate::external_player::current_state()`（后台线程每秒轮询系统会话）
//!
//! 封面**不随 WS 帧下发**（外部封面 data URI 可达 5MB，base64 后 ~6.7MB），
//! 只给 `coverKey` 与相对 `coverUrl`；字节由 `GET /api/music/cover` 按需提供，
//! 内容寻址 → 同一首歌 URL 不变，手机端可长期缓存。

use std::sync::Mutex;

use serde::Serialize;
use serde_json::{json, Value};
use tauri::Emitter;

use crate::external_player::{ExternalPlayback, ExternalTrack};
use crate::smtc::SmtcState;

/// 手机端可见的统一「当前播放」快照。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicState {
    /// `"app"` | `"external"`；`None` 表示当前无播放内容。
    pub source: Option<String>,
    pub playing: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub position_ms: i64,
    pub duration_ms: i64,
    /// 时长已知才可拖动进度。
    pub can_seek: bool,
    /// 封面内容标识（封面来源字符串的稳定 hash）；无封面为 None。
    pub cover_key: Option<String>,
    /// 相对路径，手机端自行拼 host 与 token。
    pub cover_url: Option<String>,
    /// 外部来源的 AUMID（如 `cloudmusic.exe`）；软件内为 None。
    pub source_app_id: Option<String>,
    /// 服务端合成时刻（epoch 毫秒）。
    pub updated_at: u64,
}

impl MusicState {
    /// 变化签名：签名不变时 WS 不必重推全量（位置推进由 `position_ms` 单独判定）。
    pub fn signature(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}",
            self.source.as_deref().unwrap_or(""),
            self.title,
            self.artist,
            self.album,
            self.playing,
            self.duration_ms,
            self.cover_key.as_deref().unwrap_or(""),
        )
    }
}

/// 当前生效的来源。
enum Active {
    App(SmtcState),
    External(ExternalPlayback),
}

/// 按优先级选定当前来源；标题为空的来源视为无效（与灵动岛的判定口径一致）。
fn pick() -> Option<Active> {
    let app = crate::smtc::current_state().filter(|s| !s.title.trim().is_empty());
    let ext = crate::external_player::current_state()
        .filter(|e| e.track.as_ref().map(|t| !t.title.trim().is_empty()).unwrap_or(false));

    if app.as_ref().map(|s| s.playing).unwrap_or(false) {
        return app.map(Active::App);
    }
    if ext.as_ref().map(|e| e.is_playing).unwrap_or(false) {
        return ext.map(Active::External);
    }
    app.map(Active::App).or_else(|| ext.map(Active::External))
}

/// 封面来源字符串 → 稳定短 hash（FNV-1a 64）。
/// 刻意不用 `DefaultHasher`：其输出不保证跨 Rust 版本稳定，而 `coverKey` 会进 URL 被手机端缓存。
fn cover_key(src: &str) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = OFFSET;
    for b in src.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(PRIME);
    }
    format!("{h:016x}")
}

/// 当前生效来源的封面「来源字符串」：
/// 软件内是 URL / `file://` 路径 / data URI；外部是 data URI。
fn active_cover_source() -> Option<String> {
    match pick()? {
        Active::App(s) => s.cover.filter(|c| !c.trim().is_empty()),
        Active::External(e) => e.track.and_then(|t| t.cover).filter(|c| !c.trim().is_empty()),
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 合成一次统一快照。
pub fn snapshot() -> MusicState {
    let (source, playing, title, artist, album, position_ms, duration_ms, source_app_id, ckey) =
        match pick() {
            Some(Active::App(s)) => (
                Some("app".to_string()),
                s.playing,
                s.title,
                s.artist,
                s.album,
                s.position_ms,
                s.duration_ms,
                None,
                s.cover.filter(|c| !c.trim().is_empty()).map(|c| cover_key(&c)),
            ),
            Some(Active::External(e)) => {
                let t: ExternalTrack = e.track.unwrap_or_default();
                (
                    Some("external".to_string()),
                    e.is_playing,
                    t.title,
                    t.artist,
                    t.album,
                    e.position_ms,
                    e.duration_ms,
                    Some(t.source_app_id),
                    t.cover.filter(|c| !c.trim().is_empty()).map(|c| cover_key(&c)),
                )
            }
            None => (
                None,
                false,
                String::new(),
                String::new(),
                String::new(),
                0,
                0,
                None,
                None,
            ),
        };

    MusicState {
        cover_url: ckey.as_ref().map(|k| format!("/api/music/cover?k={k}")),
        cover_key: ckey,
        can_seek: duration_ms > 0,
        updated_at: now_ms(),
        source,
        playing,
        title,
        artist,
        album,
        position_ms,
        duration_ms,
        source_app_id,
    }
}

/// 供 `exec_query("music.state")` 使用。
pub fn state_json() -> Result<Value, String> {
    serde_json::to_value(snapshot()).map_err(|e| e.to_string())
}

/// 控制当前播放。
///
/// - `source` = `None` / `"auto"`：按统一优先级打给当前生效来源
/// - `"app"` / `"external"`：强制指定目标
/// - `action`：`play-pause` | `prev` | `next` | `seek`（`value_ms` 仅 seek 使用，毫秒）
///
/// 无播放内容时返回 `target:"none"`（HTTP 200，手机端据此提示即可，不当作错误）。
pub fn control(
    app: &tauri::AppHandle,
    action: &str,
    value_ms: i64,
    source: Option<&str>,
) -> Result<Value, String> {
    let target = match source {
        Some("app") => Some("app"),
        Some("external") => Some("external"),
        _ => match pick() {
            Some(Active::App(_)) => Some("app"),
            Some(Active::External(_)) => Some("external"),
            None => None,
        },
    };

    match target {
        // 软件内播放器活在 WebView 里，只能把命令发回前端执行
        // （走独立事件 remote-music:control，不复用 smtc:control：
        //   后者被 mediaKeysEnabled 门禁与 150ms 去重挡住，语义也不对）
        Some("app") => {
            app.emit(
                "remote-music:control",
                json!({ "action": action, "positionMs": value_ms }),
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({ "ok": true, "target": "app" }))
        }
        Some("external") => {
            crate::external_player::control(action, value_ms);
            Ok(json!({ "ok": true, "target": "external" }))
        }
        _ => Ok(json!({ "ok": true, "target": "none" })),
    }
}

/// 单条封面缓存：(coverKey, 字节, mime)。避免同一张封面被反复下载/解码。
static COVER_CACHE: Mutex<Option<(String, Vec<u8>, &'static str)>> = Mutex::new(None);

/// 依据文件头嗅探图片类型（SMTC 缩略图与各平台封面一般是 png/jpeg/webp）。
fn sniff_mime(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg"
    } else if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        "image/png"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "application/octet-stream"
    }
}

/// 取封面字节。`k` 必须等于**当前生效来源**的封面 hash，否则返回 None
/// （既拦下过期 key，也杜绝通过该接口读取任意本机文件）。
///
/// **阻塞**（网络下载 + 图片解码），只能在 `spawn_blocking` 中调用。
pub fn cover_bytes(k: &str) -> Option<(Vec<u8>, &'static str)> {
    let src = active_cover_source()?;
    if cover_key(&src) != k {
        return None;
    }
    if let Ok(cache) = COVER_CACHE.lock() {
        if let Some((ck, bytes, mime)) = cache.as_ref() {
            if ck == k {
                return Some((bytes.clone(), *mime));
            }
        }
    }
    let bytes = crate::smtc::cover_bytes_for(&src)?;
    let mime = sniff_mime(&bytes);
    if let Ok(mut cache) = COVER_CACHE.lock() {
        *cache = Some((k.to_string(), bytes.clone(), mime));
    }
    Some((bytes, mime))
}
