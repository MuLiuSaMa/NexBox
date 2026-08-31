#![allow(dead_code)]

//! QQ 音乐 API — 完全移植自 Mineradio server.js
//!
//! 所有 API 端点、签名算法、请求参数、降级策略均与 server.js 完全一致。

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use reqwest::header::{COOKIE, REFERER, USER_AGENT};
use serde_json::{json, Value};

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

use super::cookie as cookie_util;
use super::models::*;

// ============================================================
//  常量 (对照 server.js)
// ============================================================

const QQ_MUSICU_URL: &str = "https://u.y.qq.com/cgi-bin/musicu.fcg";
const QQ_SMARTBOX_URL: &str = "https://c.y.qq.com/splcloud/fcgi-bin/smartbox_new.fcg";
const QQ_PLAYLIST_CREATED_URL: &str = "https://c.y.qq.com/rsc/fcgi-bin/fcg_user_created_diss";
const QQ_PLAYLIST_COLLECTED_URL: &str = "https://c.y.qq.com/fav/fcgi-bin/fcg_get_profile_order_asset.fcg";
const QQ_PLAYLIST_TRACKS_URL: &str = "https://c.y.qq.com/qzone/fcg-bin/fcg_ucc_getcdinfo_byids_cp.fcg";
const QQ_PROFILE_URL: &str = "https://c.y.qq.com/rsc/fcgi-bin/fcg_get_profile_homepage.fcg";
const QQ_LYRIC_LEGACY_URL: &str = "https://c.y.qq.com/lyric/fcgi-bin/fcg_query_lyric_new.fcg";
const QQ_ALBUM_INFO_URL: &str = "https://c.y.qq.com/v8/fcg-bin/fcg_v8_album_info_cp.fcg";

const QQ_HEADERS_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const QQ_SEARCH_UA: &str = "QQMusic 14090508(android 12)";
const QQ_HEADERS_REFERER: &str = "https://y.qq.com/";

const QQ_LIKED_DIRID: i64 = 201;
const QQ_LIKED_PLAYLIST_ID: &str = "liked";
const QQ_LIKED_PLAYLIST_NAME: &str = "QQ 音乐·我的喜欢";
const QQ_LIKED_PLAYLIST_COVER: &str = "https://y.gtimg.cn/mediastyle/global/img/cover_like.png";

const QQ_PLAYLIST_SYNC_PAGE_SIZE: u32 = 200;
const QQ_PLAYLIST_SYNC_MAX_PAGES: usize = 25;

const QQ_VKEY_REQUEST_TIMEOUT_MS: u64 = 6000;
const QQ_AUDIO_PROBE_TOTAL_MS: u64 = 6200;
const QQ_AUDIO_PROBE_ATTEMPT_MS: u64 = 2000;
const AUDIO_URL_PROBE_BYTES: usize = 8192;

// ============================================================
//  HTTP 请求工具
// ============================================================

fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("Failed to build HTTP client")
}

pub(crate) fn build_client_with_timeout(timeout_ms: u64) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()
        .expect("Failed to build HTTP client")
}

/// 发送请求并返回文本 (对照 requestText)
async fn request_text(url: &str, method: &str, headers: &[(&str, &str)], body: Option<&str>, timeout_ms: u64) -> Result<String, String> {
    let client = build_client_with_timeout(timeout_ms);
    let method = match method {
        "POST" => reqwest::Method::POST,
        _ => reqwest::Method::GET,
    };
    let mut req = client.request(method, url);
    for (key, value) in headers {
        req = req.header(*key, *value);
    }
    if let Some(b) = body {
        req = req.body(b.to_string());
    }
    let resp = req.send().await.map_err(|e| format!("Request failed: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("Failed to read response: {e}"))?;
    if !status.is_success() {
        return Err(format!("HTTP {}: {}", status.as_u16(), &text[..text.len().min(200)]));
    }
    Ok(text)
}

/// 解析 JSON 文本，处理 callback 包裹 (对照 parseJSONText)
/// 兼容 callback(...) / MusicJsonCallback(...) / jsonCallback(...) 等多种 JSONP 包裹
fn parse_json_text(text: &str) -> Result<Value, String> {
    let raw = text.trim();
    // 检测 JSONP 包裹: 以已知 callback 前缀开头，且以 ) 或 ); 结尾
    let is_jsonp = raw.ends_with(')') || raw.ends_with(");");
    let has_callback_prefix = raw.starts_with("callback")
        || raw.starts_with("MusicJsonCallback")
        || raw.starts_with("jsonCallback")
        || raw.starts_with("Callback");

    let json_str = if is_jsonp && has_callback_prefix {
        let start = raw.find('(').map(|i| i + 1).unwrap_or(0);
        let end = raw.rfind(')').unwrap_or(raw.len());
        if start < end {
            &raw[start..end]
        } else {
            raw
        }
    } else {
        raw
    };
    serde_json::from_str(json_str)
        .map_err(|e| format!("Failed to parse JSON: {e}, body: {}", &text[..text.len().min(300)]))
}

/// 发送 musicu.fcg 请求 (对照 qqMusicRequest)
async fn qq_musicu_request(payload: &Value, cookie: &str, timeout_ms: u64) -> Result<Value, String> {
    let body = serde_json::to_string(payload).map_err(|e| e.to_string())?;
    let mut headers = vec![
        ("Referer", QQ_HEADERS_REFERER),
        ("User-Agent", QQ_HEADERS_UA),
        ("Content-Type", "application/json;charset=UTF-8"),
    ];
    if !cookie.is_empty() {
        headers.push(("Cookie", cookie));
    }
    let text = request_text(QQ_MUSICU_URL, "POST", &headers, Some(&body), timeout_ms).await?;
    log::debug!("[QQMusicu] response preview: {}", &text[..text.len().min(500)]);
    parse_json_text(&text)
}

/// 发送 GET JSON 请求 (对照 qqGetJSON)
async fn qq_get_json(url: &str, params: &[(&str, &str)], cookie: &str, extra_headers: &[(&str, &str)]) -> Result<Value, String> {
    let mut full_url = url.to_string();
    if !params.is_empty() {
        let query: Vec<String> = params.iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect();
        full_url = format!("{}?{}", full_url, query.join("&"));
    }
    let mut headers = vec![
        (REFERER.as_str(), QQ_HEADERS_REFERER),
        (USER_AGENT.as_str(), QQ_HEADERS_UA),
    ];
    if !cookie.is_empty() {
        headers.push((COOKIE.as_str(), cookie));
    }
    for (k, v) in extra_headers {
        headers.push((k, v));
    }
    let text = request_text(&full_url, "GET", &headers, None, 10000).await?;
    parse_json_text(&text)
}

// ============================================================
//  搜索签名 (对照 qqSearchSign)
// ============================================================

/// QQ 音乐搜索签名
fn qq_search_sign(text: &str) -> String {
    use sha1::{Sha1, Digest};
    let mut hasher = Sha1::new();
    hasher.update(text.as_bytes());
    let hash = hex::encode(hasher.finalize());
    let hash_bytes = hash.as_bytes();

    let part1_indices = [23usize, 14, 6, 36, 16, 40, 7, 19];
    let part2_indices = [16usize, 1, 32, 12, 19, 27, 8, 5];
    let scramble = [89u8, 39, 179, 150, 218, 82, 58, 252, 177, 52, 186, 123, 120, 64, 242, 133, 143, 161, 121, 179];

    let part1: String = part1_indices.iter()
        .filter_map(|&i| hash_bytes.get(i).map(|&b| b as char))
        .collect();
    let part2: String = part2_indices.iter()
        .filter_map(|&i| hash_bytes.get(i).map(|&b| b as char))
        .collect();

    let bytes: Vec<u8> = scramble.iter().enumerate()
        .map(|(i, &v)| {
            let hex_pair = &hash[i * 2..i * 2 + 2];
            v ^ u8::from_str_radix(hex_pair, 16).unwrap_or(0)
        })
        .collect();

    let middle = base64::engine::general_purpose::STANDARD.encode(&bytes)
        .chars()
        .filter(|c| *c != '\\' && *c != '/' && *c != '+' && *c != '=')
        .collect::<String>();

    format!("zzc{}{}{}", part1, middle, part2).to_lowercase()
}

// ============================================================
//  Cookie / 认证 (对照 server.js)
// ============================================================

/// QQ 音乐认证信息
#[derive(Debug, Clone, Default)]
pub struct QQAuth {
    pub uin: String,
    pub music_key: String,
    pub playback_key: String,
    pub login_type: i32,
    pub nickname: String,
    pub avatar: String,
    pub logged_in: bool,
    pub playback_ready: bool,
}

/// 从 Cookie 提取认证信息
pub fn extract_qq_auth(cookie: &str) -> QQAuth {
    let uin = cookie_util::qq_extract_uin(cookie);
    let music_key = cookie_util::qq_extract_music_key(cookie);
    let playback_key = cookie_util::qq_extract_playback_key(cookie);
    let login_type = cookie_util::parse_cookie_string(cookie)
        .get("login_type")
        .map(|v| v.trim().parse::<i32>().unwrap_or(0))
        .unwrap_or(0);

    let logged_in = !uin.is_empty() && !music_key.is_empty();
    let playback_ready = !uin.is_empty() && !playback_key.is_empty();

    QQAuth {
        uin,
        music_key,
        playback_key,
        login_type,
        logged_in,
        playback_ready,
        ..Default::default()
    }
}

/// 检查 Cookie 是否有登录态
pub fn qq_cookie_has_login(cookie: &str) -> bool {
    cookie_util::qq_cookie_has_login(cookie)
}

/// 检查 Cookie 是否有播放权限
pub fn qq_cookie_has_playback(cookie: &str) -> bool {
    cookie_util::qq_cookie_has_playback(cookie)
}

/// 解码 QQ Cookie 值 (对照 decodeQQCookieValue)
/// 处理 hex / URL / GB18030 / Unicode 转义 / Latin1→UTF8 多策略择优
pub fn decode_qq_cookie_value(value: &str) -> String {
    let raw = value.trim();
    if raw.is_empty() {
        return String::new();
    }

    let mut candidates: Vec<String> = vec![raw.to_string()];

    // 策略 1: Hex 解码 (QQ音乐 Cookie 最常见的编码方式)
    // 例如: e4b8ade69687 → 中文
    if raw.len() >= 4 && raw.len() % 2 == 0 && raw.chars().all(|c| c.is_ascii_hexdigit()) {
        if let Ok(bytes) = hex::decode(raw) {
            if let Ok(text) = String::from_utf8(bytes) {
                let trimmed = text.trim().to_string();
                if !trimmed.is_empty() && trimmed.chars().any(|c| !c.is_ascii()) {
                    candidates.push(trimmed);
                }
            }
        }
    }

    // 策略 2: URL 解码 (把 + 替换为 %20 后解码)
    let plus_safe = raw.replace('+', "%20");
    if let Ok(decoded) = urlencoding::decode(&plus_safe) {
        let text = decoded.to_string();
        if !text.is_empty() && text != raw {
            candidates.push(text);
        }
    }

    // 策略 3: 百分比字节解码 → GB18030 / UTF-8
    let mut percent_bytes: Vec<u8> = Vec::new();
    let mut has_percent = false;
    let chars: Vec<char> = plus_safe.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '%' && i + 2 < chars.len() {
            let hex_pair: String = chars[i + 1..i + 3].iter().collect();
            if hex_pair.chars().all(|c| c.is_ascii_hexdigit()) {
                if let Ok(byte) = u8::from_str_radix(&hex_pair, 16) {
                    percent_bytes.push(byte);
                    has_percent = true;
                    i += 3;
                    continue;
                }
            }
        }
        // 非 % 字符，转为 UTF-8 字节
        let ch_str: String = ch.to_string();
        percent_bytes.extend_from_slice(ch_str.as_bytes());
        i += 1;
    }
    if has_percent && !percent_bytes.is_empty() {
        // 尝试 GB18030 解码
        let (gb_text, _, _) = encoding_rs::GB18030.decode(&percent_bytes);
        let gb_trimmed = gb_text.trim().to_string();
        if !gb_trimmed.is_empty() {
            candidates.push(gb_trimmed);
        }
        // 尝试 UTF-8 解码
        if let Ok(utf8_text) = String::from_utf8(percent_bytes.clone()) {
            let utf8_trimmed = utf8_text.trim().to_string();
            if !utf8_trimmed.is_empty() {
                candidates.push(utf8_trimmed);
            }
        }
    }

    // 策略 4: Unicode 转义 \uXXXX
    for item in candidates.clone() {
        if item.contains("\\u") {
            // 简单处理 \uXXXX 转义
            let replaced = item.replace("\\u", "\\u");
            if let Ok(v) = serde_json::from_str::<String>(&format!("\"{}\"", replaced)) {
                let trimmed = v.trim().to_string();
                if !trimmed.is_empty() {
                    candidates.push(trimmed);
                }
            }
        }
    }

    // 策略 5: Latin1 → UTF-8 (处理乱码 Ã×× 之类)
    for item in candidates.clone() {
        if item.chars().any(|c| c == '\u{c3}' || c == '\u{c2}' || (c >= '\u{c0}' && c <= '\u{ff}' && c >= '\u{80}')) {
            let bytes: Vec<u8> = item.chars().map(|c| c as u8).collect();
            if let Ok(utf8_text) = String::from_utf8(bytes) {
                let trimmed = utf8_text.trim().to_string();
                if !trimmed.is_empty() {
                    candidates.push(trimmed);
                }
            }
        }
    }

    // 评分择优: 含中文字符越多越好，含乱码/百分号越少越好
    fn score(text: &str) -> f64 {
        let mut s = 0.0;
        // 惩罚: 替换字符、百分号、反斜杠u、乱码字符
        s += text.matches('\u{FFFD}').count() as f64 * 80.0;
        s += text.matches(|c: char| c == '%').count() as f64 * 10.0;
        s += text.matches("\\u").count() as f64 * 8.0;
        s += text.matches(|c: char| c == '\u{c3}' || c == '\u{c2}').count() as f64 * 34.0;
        s += text.matches(|c: char| c >= '\u{80}' && c <= '\u{9f}').count() as f64 * 42.0;
        s += text.matches(|c: char| (c >= '\0' && c <= '\u{8}') || (c >= '\u{e}' && c <= '\u{1f}') || c == '\u{7f}').count() as f64 * 50.0;
        // 奖励: 中文字符
        s -= text.matches(|c: char| c >= '\u{4e00}' && c <= '\u{9fff}').count() as f64 * 2.0;
        s + (text.len().min(80) as f64) * 0.02
    }

    candidates
        .into_iter()
        .filter(|s| !s.is_empty())
        .min_by(|a, b| score(a).partial_cmp(&score(b)).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or_else(|| raw.to_string())
        .trim()
        .to_string()
}

// ============================================================
//  工具函数
// ============================================================

/// 专辑封面 URL (对照 qqAlbumCover)
fn qq_album_cover(album_mid: &str, size: u32) -> String {
    if album_mid.is_empty() {
        return String::new();
    }
    format!("https://y.qq.com/music/photo_new/T002R{}x{}M000{}.jpg?max_age=2592000", size, size, album_mid)
}

/// 歌手头像 URL (对照 qqSingerAvatar)
fn qq_singer_avatar(singer_mid: &str, size: u32) -> String {
    if singer_mid.is_empty() {
        return String::new();
    }
    format!("https://y.qq.com/music/photo_new/T001R{}x{}M000{}.jpg?max_age=2592000", size, size, singer_mid)
}

/// 映射 QQ 歌手列表 (对照 mapQQArtists)
fn map_qq_artists(raw: &Value) -> Vec<Artist> {
    raw.as_array()
        .map(|arr| {
            arr.iter()
                .map(|a| Artist {
                    id: a.get("id").and_then(|v| v.as_i64()).map(|n| n.to_string()),
                    mid: a.get("mid").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    name: a.get("name")
                        .or_else(|| a.get("title"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    pic_url: None,
                    music_size: None,
                })
                .filter(|a| !a.name.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// 映射 QQ 歌曲详情 (对照 mapQQTrack)
fn map_qq_track(track: &Value, fallback: &Song) -> Song {
    let album = track.get("album").unwrap_or(&Value::Null);
    let artists = map_qq_artists(track.get("singer").unwrap_or(&Value::Null));
    let mid = track.get("mid")
        .or_else(|| track.get("songmid"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let fallback_mid = fallback.mid.as_deref().unwrap_or("");
    let song_mid = if !mid.is_empty() { mid.as_str() } else { fallback_mid };

    let album_mid = album.get("mid")
        .or_else(|| album.get("pmid"))
        .and_then(|v| v.as_str())
        .or_else(|| track.get("albummid").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();

    let media_mid = track.get("file")
        .and_then(|f| f.get("media_mid"))
        .and_then(|v| v.as_str())
        .or_else(|| track.get("strMediaMid").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();

    let name = track.get("name")
        .or_else(|| track.get("title"))
        .or_else(|| track.get("songname"))
        .and_then(|v| v.as_str())
        .unwrap_or(&fallback.name)
        .to_string();

    let artist_str = if !artists.is_empty() {
        artists.iter().map(|a| a.name.clone()).collect::<Vec<_>>().join(" / ")
    } else {
        fallback.artist.clone()
    };

    let album_name = album.get("name")
        .or_else(|| album.get("title"))
        .and_then(|v| v.as_str())
        .or_else(|| track.get("albumname").and_then(|v| v.as_str()))
        .unwrap_or(&fallback.album)
        .to_string();

    let duration = track.get("interval")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) * 1000;

    let fee = track.get("pay")
        .and_then(|p| p.get("pay_play"))
        .and_then(|v| v.as_i64())
        .map(|n| if n > 0 { 1 } else { 0 })
        .unwrap_or(0);

    let id = if !song_mid.is_empty() { song_mid.to_string() } else {
        track.get("id")
            .or_else(|| track.get("songid"))
            .and_then(|v| v.as_i64())
            .map(|n| n.to_string())
            .unwrap_or_else(|| fallback.id.clone())
    };

    let qq_song_id = track.get("id")
        .or_else(|| track.get("songid"))
        .and_then(|v| v.as_i64());

    Song {
        provider: "qqmusic".into(),
        id,
        mid: if !song_mid.is_empty() { Some(song_mid.into()) } else { fallback.mid.clone() },
        media_mid: if !media_mid.is_empty() { Some(media_mid) } else { fallback.media_mid.clone() },
        name,
        artist: artist_str,
        artists: if !artists.is_empty() { artists } else { fallback.artists.clone() },
        album: album_name,
        cover: {
            // 800x800：封面渐染/大图场景需要高分辨率，300 会放大到模糊
            let c = qq_album_cover(&album_mid, 800);
            if !c.is_empty() { c } else { fallback.cover.clone() }
        },
        duration,
        fee,
        playable: false,
        language: 0,
        qq_song_id,
        ..Default::default()
    }
}

/// 映射 QQ 歌单曲目 (对照 mapQQPlaylistTrack)
fn map_qq_playlist_track(raw: &Value) -> Song {
    let track = if raw.get("songid").is_some() || raw.get("songmid").is_some() || raw.get("mid").is_some() {
        raw.clone()
    } else {
        raw.get("track_info")
            .or_else(|| raw.get("songInfo"))
            .or_else(|| raw.get("songinfo"))
            .or_else(|| raw.get("song"))
            .cloned()
            .unwrap_or_else(|| raw.clone())
    };

    let album = track.get("album").cloned().unwrap_or(Value::Null);
    let artists = map_qq_artists(track.get("singer").or_else(|| track.get("singers")).unwrap_or(&Value::Null));

    let mid = track.get("mid")
        .or_else(|| track.get("songmid"))
        .or_else(|| raw.get("mid"))
        .or_else(|| raw.get("songmid"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let album_mid = album.get("mid")
        .or_else(|| track.get("albummid"))
        .or_else(|| raw.get("albummid"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let media_mid = track.get("file")
        .and_then(|f| f.get("media_mid"))
        .or_else(|| track.get("strMediaMid"))
        .or_else(|| track.get("media_mid"))
        .or_else(|| raw.get("strMediaMid"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let name = track.get("name")
        .or_else(|| track.get("songname"))
        .or_else(|| raw.get("songname"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let artist_str = if !artists.is_empty() {
        artists.iter().map(|a| a.name.clone()).collect::<Vec<_>>().join(" / ")
    } else {
        track.get("singername")
            .or_else(|| raw.get("singername"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };

    let album_name = album.get("name")
        .or_else(|| album.get("title"))
        .or_else(|| track.get("albumname"))
        .or_else(|| raw.get("albumname"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let duration = track.get("interval")
        .or_else(|| raw.get("interval"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) * 1000;

    let fee = track.get("pay")
        .and_then(|p| p.get("pay_play"))
        .and_then(|v| v.as_i64())
        .map(|n| if n > 0 { 1 } else { 0 })
        .unwrap_or(0);

    let id = if !mid.is_empty() { mid.clone() } else {
        track.get("id")
            .or_else(|| track.get("songid"))
            .or_else(|| raw.get("id"))
            .or_else(|| raw.get("songid"))
            .and_then(|v| v.as_i64())
            .map(|n| n.to_string())
            .unwrap_or_default()
    };
    let qq_song_id = track.get("id")
        .or_else(|| track.get("songid"))
        .or_else(|| raw.get("songid"))
        .and_then(|v| v.as_i64());

    Song {
        provider: "qqmusic".into(),
        id,
        mid: if !mid.is_empty() { Some(mid) } else { None },
        media_mid: if !media_mid.is_empty() { Some(media_mid) } else { None },
        name,
        artist: artist_str,
        artists,
        album: album_name,
        cover: qq_album_cover(&album_mid, 800),
        duration,
        fee,
        playable: false,
        language: 0,
        qq_song_id,
        ..Default::default()
    }
}

/// 映射 QQ 歌单 (对照 mapQQPlaylist)
fn map_qq_playlist(pl: &Value, kind: &str) -> Playlist {
    let dirid_num = pl.get("dirid")
        .or_else(|| pl.get("dir_id"))
        .or_else(|| pl.get("dirId"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let liked = dirid_num == QQ_LIKED_DIRID || is_qq_favorite_playlist(pl);

    let id = if liked {
        QQ_LIKED_PLAYLIST_ID.to_string()
    } else {
        // 对照 Mineradio: pl.dissid || pl.tid || dirid || pl.id || pl.diss_id
        pl.get("dissid")
            .or_else(|| pl.get("tid"))
            .or_else(|| pl.get("dissId"))
            .or_else(|| pl.get("id"))
            .or_else(|| pl.get("diss_id"))
            .and_then(|v| match v {
                Value::String(s) => Some(s.clone()),
                Value::Number(n) => Some(n.to_string()),
                _ => None,
            })
            .unwrap_or_else(|| dirid_num.to_string())
    };

    let raw_name = pl.get("diss_name")
        .or_else(|| pl.get("dissname"))
        .or_else(|| pl.get("name"))
        .or_else(|| pl.get("title"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let name = if liked {
        QQ_LIKED_PLAYLIST_NAME.to_string()
    } else {
        decode_qq_cookie_value(raw_name)
    };

    let cover = if liked {
        QQ_LIKED_PLAYLIST_COVER.to_string()
    } else {
        pl.get("diss_cover")
            .or_else(|| pl.get("dissCover"))
            .or_else(|| pl.get("logo"))
            .or_else(|| pl.get("picurl"))
            .or_else(|| pl.get("cover"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };

    let track_count = pl.get("song_cnt")
        .or_else(|| pl.get("songCnt"))
        .or_else(|| pl.get("songnum"))
        .or_else(|| pl.get("songNum"))
        .or_else(|| pl.get("total_song_num"))
        .or_else(|| pl.get("song_count"))
        .or_else(|| pl.get("songCount"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    let creator = pl.get("hostname")
        .or_else(|| pl.get("nick"))
        .or_else(|| pl.get("creator"))
        .or_else(|| pl.get("nickname"))
        .and_then(|v| match v {
            Value::String(s) => Some(s.clone()),
            Value::Object(_) => v.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()),
            _ => None,
        })
        .unwrap_or_else(|| "QQ 音乐".to_string());

    Playlist {
        provider: "qqmusic".into(),
        id,
        name,
        cover,
        track_count,
        creator,
        subscribed: kind == "collect",
    }
}

/// 检查是否为 QQ 喜欢歌单 (对照 isQQLikedPlaylistId)
fn is_qq_liked_playlist_id(id: &str) -> bool {
    let value = id.trim().to_lowercase();
    value == QQ_LIKED_PLAYLIST_ID || value == "qq-liked" || value == QQ_LIKED_DIRID.to_string()
}

/// 检查是否为 QQ 收藏歌单 (对照 isQQFavoritePlaylist)
fn is_qq_favorite_playlist(pl: &Value) -> bool {
    // 检查 dirid (兼容 camelCase)
    let dirid = pl.get("dirid")
        .or_else(|| pl.get("dir_id"))
        .or_else(|| pl.get("dirId"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    if dirid == QQ_LIKED_DIRID {
        return true;
    }
    // 检查 id
    if let Some(id_str) = pl.get("id").and_then(|v| v.as_str()) {
        if is_qq_liked_playlist_id(id_str) {
            return true;
        }
    }
    // 检查 tid / dissid
    if let Some(tid_str) = pl.get("tid").or_else(|| pl.get("dissid")).and_then(|v| v.as_str()) {
        if is_qq_liked_playlist_id(tid_str) {
            return true;
        }
    }
    // 检查名称 (兼容更多字段名)
    let name = pl.get("diss_name")
        .or_else(|| pl.get("dissname"))
        .or_else(|| pl.get("name"))
        .or_else(|| pl.get("title"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let normalized = name.replace(['·', '•', '・', '_', '-', ' ', '\t', '\n'], "").to_lowercase();
    matches!(normalized.as_str(),
        "我喜欢" | "我的喜欢" | "喜欢的音乐" |
        "qq音乐我喜欢" | "qq音乐我的喜欢" | "qq音乐喜欢的音乐"
    )
}

/// 检查是否为 QQ 空间背景音乐歌单 (对照 isQzoneBackgroundPlaylist)
fn is_qzone_background_playlist(pl: &Value) -> bool {
    let name = pl.get("name")
        .or_else(|| pl.get("diss_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let creator = pl.get("hostname")
        .or_else(|| pl.get("creator"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let text = format!("{} {}", name, creator).to_lowercase();
    text.contains("qzone") || text.contains("空间") || text.contains("背景音乐")
}

// ============================================================
//  搜索 (对照 handleQQSearch / qqFullSongSearch / qqSmartboxSearch)
// ============================================================

/// 智能搜索 (对照 qqSmartboxSearch)
async fn smartbox_search(keywords: &str, limit: u32) -> Result<Vec<Song>, String> {
    let limit = limit.clamp(1, 10);
    let params: Vec<(&str, &str)> = vec![
        ("format", "json"),
        ("key", keywords),
        ("g_tk", "5381"),
        ("loginUin", "0"),
        ("hostUin", "0"),
        ("inCharset", "utf8"),
        ("outCharset", "utf-8"),
        ("notice", "0"),
        ("platform", "yqq.json"),
        ("needNewCode", "0"),
    ];
    let headers: Vec<(&str, &str)> = vec![
        (REFERER.as_str(), QQ_HEADERS_REFERER),
        (USER_AGENT.as_str(), QQ_HEADERS_UA),
    ];
    let full_url = format!("{}?{}", QQ_SMARTBOX_URL,
        params.iter().map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v))).collect::<Vec<_>>().join("&"));
    let text = request_text(&full_url, "GET", &headers, None, 10000).await?;
    let json = parse_json_text(&text)?;
    let items = json.get("data")
        .and_then(|d| d.get("song"))
        .and_then(|s| s.get("itemlist"))
        .and_then(|l| l.as_array());

    let songs = items.map(|arr| {
        arr.iter().take(limit as usize).map(|item| {
            let mid = item.get("mid")
                .or_else(|| item.get("songmid"))
                .or_else(|| item.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = strip_html_tags(
                &item.get("name")
                    .or_else(|| item.get("title"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
            );
            let singer = item.get("singer")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let id = mid.clone();
            Song {
                provider: "qqmusic".into(),
                id,
                mid: if !mid.is_empty() { Some(mid) } else { None },
                name,
                artist: singer.clone(),
                artists: if !singer.is_empty() {
                    vec![Artist { name: singer, ..Default::default() }]
                } else { vec![] },
                ..Default::default()
            }
        }).collect()
    }).unwrap_or_default();
    Ok(songs)
}

/// 完整搜索 (对照 qqFullSongSearch)
async fn full_song_search(keywords: &str, limit: u32, offset: u32) -> Result<Vec<Song>, String> {
    let limit = limit.clamp(1, 30);
    let page_num = offset / limit + 1;
    let payload = json!({
        "comm": {
            "ct": "11",
            "cv": "14090508",
            "v": "14090508",
            "tmeAppID": "qqmusic",
            "phonetype": "EBG-AN10",
            "os_ver": "12",
            "OpenUDID": "0",
            "QIMEI36": "0",
            "udid": "0",
            "chid": "0",
            "aid": "0",
            "oaid": "0",
            "taid": "0",
            "tid": "0",
            "wid": "0",
            "uid": "0",
            "sid": "0",
            "modeSwitch": "6",
            "teenMode": "0",
            "ui_mode": "2",
            "nettype": "1020"
        },
        "req": {
            "module": "music.search.SearchCgiService",
            "method": "DoSearchForQQMusicMobile",
            "param": {
                "search_type": 0,
                "searchid": format!("{}{:06}", chrono::Utc::now().timestamp_millis(), rand::random::<u32>() % 1000000),
                "query": keywords,
                "page_num": page_num,
                "num_per_page": limit,
                "highlight": 0,
                "nqc_flag": 0,
                "multi_zhida": 0,
                "cat": 2,
                "grp": 1,
                "sin": offset,
                "sem": 0
            }
        }
    });

    let body_text = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    let sign = qq_search_sign(&body_text);
    let url = format!("https://u.y.qq.com/cgi-bin/musics.fcg?sign={}", sign);
    let headers = vec![
        ("User-Agent", QQ_SEARCH_UA),
        ("Content-Type", "application/json"),
    ];
    let text = request_text(&url, "POST", &headers, Some(&body_text), 10000).await?;
    let json = parse_json_text(&text)?;

    let data = json.get("req")
        .and_then(|r| r.get("data"))
        .unwrap_or(&Value::Null);
    let body = data.get("body").unwrap_or(data);
    let items = body.get("item_song")
        .or_else(|| body.get("song").and_then(|s| s.get("list")))
        .or_else(|| body.get("list"))
        .and_then(|l| l.as_array());

    let songs = items.map(|arr| {
        arr.iter().map(|item| {
            let track = item.get("track_info")
                .or_else(|| item.get("songInfo"))
                .or_else(|| item.get("songinfo"))
                .or_else(|| item.get("song"))
                .unwrap_or(item);
            let mut song = map_qq_track(track, &Song::default());
            song.name = strip_html_tags(&song.name);
            song
        }).filter(|s| !s.name.is_empty() && (s.mid.is_some() || !s.id.is_empty()))
        .collect()
    }).unwrap_or_default();
    Ok(songs)
}

/// 歌曲详情 (对照 qqSongDetail)
async fn song_detail(mid: &str, fallback: &Song) -> Song {
    if mid.is_empty() {
        return fallback.clone();
    }
    let payload = json!({
        "comm": { "ct": 24, "cv": 0 },
        "songinfo": {
            "module": "music.pf_song_detail_svr",
            "method": "get_song_detail_yqq",
            "param": { "song_mid": mid }
        }
    });
    match qq_musicu_request(&payload, "", 10000).await {
        Ok(json) => {
            let data = json.get("songinfo")
                .and_then(|s| s.get("data"))
                .unwrap_or(&Value::Null);
            let track_info = data.get("track_info").unwrap_or(&Value::Null);
            map_qq_track(track_info, fallback)
        }
        Err(_) => fallback.clone(),
    }
}

/// 搜索 (对照 handleQQSearch)
pub async fn search(keywords: &str, limit: u32, _cookie: &str) -> Result<Vec<Song>, String> {
    let kw = keywords.trim();
    if kw.is_empty() {
        return Ok(vec![]);
    }
    let limit = limit.clamp(1, 30);

    let mut base = Vec::new();
    match full_song_search(kw, limit, 0).await {
        Ok(results) => base = results,
        Err(e) => log::warn!("[QQSearch] full search failed: {e}"),
    }

    if base.is_empty() {
        match smartbox_search(kw, limit).await {
            Ok(results) => base = results,
            Err(e) => log::warn!("[QQSearch] smartbox failed: {e}"),
        }
    }

    // 补全详情 (并行执行,避免串行等待拖慢搜索)
    let futures: Vec<_> = base.iter().map(|item| {
        let mid = item.mid.as_deref().unwrap_or("").to_string();
        let fallback = item.clone();
        async move {
            if !mid.is_empty() {
                song_detail(&mid, &fallback).await
            } else {
                fallback
            }
        }
    }).collect();
    let detailed = futures_util::future::join_all(futures).await;

    // 去重
    let mut seen = std::collections::HashSet::new();
    let songs: Vec<Song> = detailed.into_iter()
        .filter(|s| {
            let key = s.mid.as_deref().unwrap_or(&s.id).to_string();
            let name_key = &s.name;
            if name_key.is_empty() {
                return false;
            }
            if seen.contains(&key) {
                return false;
            }
            seen.insert(key);
            true
        })
        .collect();

    Ok(songs)
}

// ============================================================
//  播放地址 (对照 handleQQSongUrl)
// ============================================================

/// 探测音频 URL (对照 probeQQAudioUrl)
async fn probe_audio_url(audio_url: &str, timeout_ms: u64) -> bool {
    if audio_url.is_empty() {
        return false;
    }
    let client = build_client_with_timeout(timeout_ms);
    let range = format!("bytes=0-{}", AUDIO_URL_PROBE_BYTES - 1);
    let resp = match client
        .get(audio_url)
        .header("Range", &range)
        .header("Referer", QQ_HEADERS_REFERER)
        .header("User-Agent", QQ_HEADERS_UA)
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => return false,
    };

    let status = resp.status().as_u16();
    if status != 200 && status != 206 {
        return false;
    }

    let content_type = resp.headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();
    if content_type.contains("text/html") || content_type.contains("application/json")
        || content_type.contains("application/xml") || content_type.contains("text/plain")
    {
        return false;
    }

    // 读取前 AUDIO_URL_PROBE_BYTES 字节检测魔数
    let body = match resp.bytes().await {
        Ok(b) => b,
        Err(_) => return false,
    };

    if body.len() < 512 {
        return false;
    }

    // 检测文件头魔数
    let _magic = &body[..body.len().min(12)];
    let is_mp3 = body.starts_with(b"ID3");
    let is_flac = body.starts_with(b"fLaC");
    let is_ogg = body.starts_with(b"OggS");
    let is_wav = body.len() >= 12 && &body[0..4] == b"RIFF" && &body[8..12] == b"WAVE";
    let is_mp4 = body.len() >= 8 && &body[4..8] == b"ftyp";
    // MPEG frame sync
    let is_mpeg = (0..body.len().saturating_sub(1).min(2048))
        .any(|i| body[i] == 0xff && (body[i + 1] & 0xe0) == 0xe0);

    is_mp3 || is_flac || is_ogg || is_wav || is_mp4 || is_mpeg
}

/// 获取播放地址 (对照 handleQQSongUrl)
pub async fn song_url(mid: &str, media_mid: &str, quality: &str, cookie: &str) -> Result<SongUrlResult, String> {
    let songmid = mid.trim();
    if songmid.is_empty() {
        return Ok(SongUrlResult {
            url: None,
            playable: false,
            ..Default::default()
        });
    }

    let auth = extract_qq_auth(cookie);
    let uin = if auth.uin.is_empty() { "0".to_string() } else { auth.uin.clone() };
    let music_key = &auth.music_key;
    let _playback_key = &auth.playback_key;

    // 生成随机 guid
    let guid = format!("{}", 10000000 + rand::random::<u32>() % 90000000);
    let file_media_mid = media_mid.trim().to_string();

    // 按音质模板构建 filename 候选列表
    let requested_quality = normalize_quality(quality);
    let quality_start = QQ_QUALITY_TEMPLATES.iter()
        .position(|(_, _, level, _)| *level == requested_quality)
        .unwrap_or(0);
    let templates = &QQ_QUALITY_TEMPLATES[quality_start..];

    // 构建 mediaIds 列表
    let mut media_ids = Vec::new();
    if !file_media_mid.is_empty() {
        media_ids.push(file_media_mid.clone());
    }
    if !songmid.is_empty() && !media_ids.contains(&songmid.to_string()) {
        media_ids.push(songmid.to_string());
    }

    // 构建 filename 候选列表
    let mut file_candidates: Vec<(String, String, String)> = Vec::new(); // (filename, level, label)
    for media_id in &media_ids {
        for (prefix, ext, level, label) in templates {
            let filename = format!("{}{}{}", prefix, media_id, ext);
            file_candidates.push((filename, level.to_string(), label.to_string()));
        }
    }

    let filenames: Vec<String> = file_candidates.iter().map(|(f, _, _)| f.clone()).collect();
    let songmids: Vec<String> = filenames.iter().map(|_| songmid.to_string()).collect();
    let songtypes: Vec<i32> = filenames.iter().map(|_| 0).collect();

    let comm = if !music_key.is_empty() {
        json!({
            "uin": uin,
            "format": "json",
            "ct": 19,
            "cv": 0,
            "authst": music_key
        })
    } else {
        json!({
            "uin": uin,
            "format": "json",
            "ct": 24,
            "cv": 0
        })
    };

    let mut param = json!({
        "guid": guid,
        "songmid": songmids,
        "songtype": songtypes,
        "uin": uin,
        "loginflag": 1,
        "platform": "20",
    });
    if !filenames.is_empty() {
        param["filename"] = json!(filenames);
    }

    let payload = json!({
        "comm": comm,
        "req_0": {
            "module": "vkey.GetVkeyServer",
            "method": "CgiGetVkey",
            "param": param
        }
    });

    let json = qq_musicu_request(&payload, cookie, QQ_VKEY_REQUEST_TIMEOUT_MS).await?;
    let data = json.get("req_0")
        .and_then(|r| r.get("data"))
        .unwrap_or(&Value::Null);

    let infos = data.get("midurlinfo")
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();
    let purl_infos: Vec<&Value> = infos.iter()
        .filter(|item| item.get("purl").and_then(|v| v.as_str()).map(|s| !s.is_empty()).unwrap_or(false))
        .collect();

    let sips: Vec<String> = data.get("sip")
        .and_then(|s| s.as_array())
        .map(|arr| arr.iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .filter(|s| !s.is_empty())
            .collect())
        .unwrap_or_else(|| vec!["https://ws.stream.qqmusic.qq.com/".into()]);

    // 探测音频 URL（快速版），参考 Mineradio 但大幅降低超时时间
    // 每次探测 800ms，总超时 2s，确保用户点击后快速响应
    let probe_deadline_ms = 2000u64;
    let probe_attempt_ms = 800u64;
    let start_time = std::time::Instant::now();

    for candidate_info in &purl_infos {
        for sip in &sips {
            if start_time.elapsed().as_millis() as u64 > probe_deadline_ms - 200 {
                break;
            }
            let purl = candidate_info.get("purl")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if purl.is_empty() {
                continue;
            }
            let candidate_url = format!("{}{}", sip, purl);
            let remaining_ms = probe_deadline_ms.saturating_sub(start_time.elapsed().as_millis() as u64);
            let probe_timeout = probe_attempt_ms.min(remaining_ms);
            if probe_audio_url(&candidate_url, probe_timeout).await {
                let filename = candidate_info.get("filename")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let file_meta = file_candidates.iter()
                    .find(|(f, _, _)| f == filename);
                let level = file_meta.map(|(_, l, _)| l.clone()).unwrap_or_default();
                let label = file_meta.map(|(_, _, lb)| lb.clone()).unwrap_or_default();

                return Ok(SongUrlResult {
                    url: Some(candidate_url),
                    playable: true,
                    trial: false,
                    level,
                    quality: label,
                    br: 0,
                    ..Default::default()
                });
            }
        }
    }

    // 探测全部失败，返回第一个候选 URL（让前端处理失败）
    // 相比 Mineradio 直接返回不可用，提供一个 URL 让前端有机会重试
    if let Some(first_info) = purl_infos.first() {
        if let Some(purl) = first_info.get("purl").and_then(|v| v.as_str()) {
            if !purl.is_empty() {
                let sip = sips.first().map(|s| s.as_str()).unwrap_or("https://ws.stream.qqmusic.qq.com/");
                let fallback_url = format!("{}{}", sip, purl);
                let filename = first_info.get("filename").and_then(|v| v.as_str()).unwrap_or("");
                let file_meta = file_candidates.iter().find(|(f, _, _)| f == filename);
                let level = file_meta.map(|(_, l, _)| l.clone()).unwrap_or_default();
                let label = file_meta.map(|(_, _, lb)| lb.clone()).unwrap_or_default();

                return Ok(SongUrlResult {
                    url: Some(fallback_url),
                    playable: true,
                    trial: false,
                    level,
                    quality: label,
                    br: 0,
                    ..Default::default()
                });
            }
        }
    }

    // 无可用 URL
    let restriction_msg = if auth.logged_in && auth.playback_ready {
        "QQ 音乐没有返回播放地址，可能受版权、会员或官方客户端限制"
    } else if auth.logged_in {
        "QQ 音乐当前只拿到了网页登录状态，还缺少播放授权，请重新登录"
    } else {
        "QQ 音乐需要登录或授权后才能获取播放地址"
    };

    Ok(SongUrlResult {
        url: None,
        playable: false,
        trial: false,
        level: requested_quality,
        quality: String::new(),
        br: 0,
        reason: Some("QQ_URL_UNAVAILABLE".into()),
        message: Some(restriction_msg.into()),
        fee: None,
    })
}

// ============================================================
//  歌词 (对照 handleQQLyric)
// ============================================================

/// 解码 HTML 实体 (对照 decodeHtmlEntities)
fn decode_html_entities(text: &str) -> String {
    let mut result = text
        .replace("&#x2014;", "\u{2014}")
        .replace("&#x2019;", "\u{2019}")
        .replace("&#x2026;", "\u{2026}")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&nbsp;", " ");
    // 处理 &#xHEX; 和 &#DEC; 格式
    // 简单处理：去掉其他实体
    result = regex::Regex::new(r"&#x?[0-9a-fA-F]+;")
        .map(|re| re.replace_all(&result, "").to_string())
        .unwrap_or(result);
    result
}

/// 去除搜索结果名称中的 HTML 高亮标签 (如 <em>2026</em>), 避免把原始标签文本显示出来
fn strip_html_tags(text: &str) -> String {
    if text.contains('<') || text.contains('>') {
        regex::Regex::new(r"<[^>]*>")
            .map(|re| re.replace_all(text, "").to_string())
            .unwrap_or_else(|_| text.to_string())
    } else {
        text.to_string()
    }
}

/// 解码 QQ 歌词文本 (对照 decodeQQLyricText)
fn decode_qq_lyric_text(text: &str) -> String {
    let raw = decode_html_entities(text.trim());
    if raw.is_empty() {
        return String::new();
    }
    // 检测是否像 Base64
    let compact = raw.replace(|c: char| c.is_whitespace(), "");
    let looks_base64 = compact.len() >= 8 && compact.len() % 4 == 0
        && compact.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=');
    if looks_base64 && !raw.starts_with('[') {
        if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(&compact) {
            if let Ok(text) = String::from_utf8(decoded) {
                let text = text.trim_start_matches('\u{feff}');
                if text.contains('[') || text.chars().any(|c| c >= '\u{4e00}' && c <= '\u{9fff}') {
                    return decode_html_entities(&text.replace("\r\n", "\n")).trim().to_string();
                }
            }
        }
    }
    decode_html_entities(&raw.replace("\r\n", "\n")).trim().to_string()
}

/// 获取歌词 (对照 handleQQLyric)
pub async fn lyric(mid: &str, id: &str, cookie: &str) -> Result<Lyrics, String> {
    let song_mid = mid.trim();
    let song_id: i64 = id.trim().parse::<i64>().unwrap_or(0);

    if song_mid.is_empty() && song_id == 0 {
        return Ok(Lyrics { lyric: String::new(), ..Default::default() });
    }

    let mut param = json!({});
    if !song_mid.is_empty() {
        param["songMID"] = json!(song_mid);
    }
    if song_id > 0 {
        param["songID"] = json!(song_id);
    }

    let payload = json!({
        "comm": { "ct": 24, "cv": 0 },
        "lyric": {
            "module": "music.musichallSong.PlayLyricInfo",
            "method": "GetPlayLyricInfo",
            "param": param
        }
    });

    let mut lyric_text = String::new();
    let mut trans_text = String::new();
    let mut roma_text = String::new();
    let mut qrc_text = String::new();

    match qq_musicu_request(&payload, cookie, 10000).await {
        Ok(json) => {
            let data = json.get("lyric")
                .and_then(|l| l.get("data"))
                .unwrap_or(&Value::Null);
            lyric_text = decode_qq_lyric_text(data.get("lyric").and_then(|v| v.as_str()).unwrap_or(""));
            trans_text = decode_qq_lyric_text(data.get("trans").and_then(|v| v.as_str()).unwrap_or(""));
            qrc_text = decode_qq_lyric_text(data.get("qrc").and_then(|v| v.as_str()).unwrap_or(""));
            roma_text = decode_qq_lyric_text(data.get("roma").and_then(|v| v.as_str()).unwrap_or(""));
        }
        Err(e) => {
            log::warn!("[QQLyric] musicu failed: {e}");
        }
    }

    // 降级到旧版 API
    if lyric_text.is_empty() && !song_mid.is_empty() {
        let login_uin = cookie_util::qq_extract_uin(cookie);
        let params: Vec<(&str, &str)> = vec![
            ("songmid", song_mid),
            ("songtype", "0"),
            ("format", "json"),
            ("nobase64", "1"),
            ("g_tk", "5381"),
            ("loginUin", &login_uin),
            ("hostUin", "0"),
            ("inCharset", "utf8"),
            ("outCharset", "utf-8"),
            ("notice", "0"),
            ("platform", "yqq.json"),
            ("needNewCode", "0"),
        ];
        let headers: Vec<(&str, &str)> = vec![
            ("Referer", "https://y.qq.com/portal/player.html"),
        ];
        match qq_get_json(QQ_LYRIC_LEGACY_URL, &params, cookie, &headers).await {
            Ok(body) => {
                lyric_text = decode_qq_lyric_text(body.get("lyric").and_then(|v| v.as_str()).unwrap_or(""));
                if trans_text.is_empty() {
                    trans_text = decode_qq_lyric_text(
                        body.get("trans").or_else(|| body.get("tlyric"))
                            .and_then(|v| v.as_str()).unwrap_or("")
                    );
                }
            }
            Err(e) => {
                log::warn!("[QQLyric] legacy failed: {e}");
            }
        }
    }

    Ok(Lyrics {
        lyric: lyric_text,
        translation: if trans_text.is_empty() { None } else { Some(trans_text) },
        roma: if roma_text.is_empty() { None } else { Some(roma_text) },
        yrc: if qrc_text.is_empty() { None } else { Some(qrc_text) },
    })
}

// ============================================================
//  登录信息 (对照 getQQLoginInfo / normalizeQQProfile)
// ============================================================

/// 获取 QQ 音乐登录信息
pub async fn login_info(cookie: &str) -> Result<LoginInfo, String> {
    let auth = extract_qq_auth(cookie);
    let uin = auth.uin.clone();
    let music_key = auth.music_key.clone();

    if uin.is_empty() || music_key.is_empty() {
        return Ok(LoginInfo {
            provider: "qqmusic".into(),
            logged_in: false,
            ..Default::default()
        });
    }

    // 调用 profile API 获取昵称/头像
    let mut nickname = String::new();
    let mut avatar = String::new();

    // 策略 1: 使用 musicu API 获取登录用户信息 (最可靠)
    let comm = if !music_key.is_empty() {
        json!({
            "uin": uin,
            "format": "json",
            "ct": 19,
            "cv": 0,
            "authst": music_key
        })
    } else {
        json!({ "ct": 24, "cv": 0 })
    };
    let musicu_payload = json!({
        "comm": comm,
        "req_0": {
            "module": "music.UserInfo.userInfoServer",
            "method": "GetLoginUserInfo",
            "param": {}
        }
    });
    match qq_musicu_request(&musicu_payload, cookie, 10000).await {
        Ok(json) => {
            let data = json.get("req_0")
                .and_then(|r| r.get("data"))
                .unwrap_or(&Value::Null);
            // 尝试多种字段名
            let user = data.get("user_info")
                .or_else(|| data.get("userInfo"))
                .or_else(|| data.get("user"))
                .or_else(|| data.get("profile"))
                .unwrap_or(data);
            nickname = user.get("nick")
                .or_else(|| user.get("nickname"))
                .or_else(|| user.get("name"))
                .or_else(|| user.get("hostname"))
                .or_else(|| user.get("title"))
                .and_then(|v| v.as_str())
                .map(|s| decode_qq_cookie_value(s))
                .unwrap_or_default();
            avatar = user.get("headpic")
                .or_else(|| user.get("avatar"))
                .or_else(|| user.get("avatarUrl"))
                .or_else(|| user.get("head_url"))
                .or_else(|| user.get("picurl"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
        }
        Err(e) => {
            log::warn!("[QQLogin] musicu GetLoginUserInfo failed: {e}");
        }
    }

    // 存储 profile 响应中的 VIP 信息 (用于降级)
    let mut profile_vip_info: Option<Value> = None;

    // 策略 2: 使用 profile homepage API (降级)
    // 即使有昵称也调用，用于获取 VIP 信息
    {
        let profile_url = format!(
            "{}?cid=205360838&userid={}&reqfrom=1&g_tk=5381&loginUin={}&hostUin=0&format=json&inCharset=utf8&outCharset=utf-8&notice=0&platform=yqq.json&needNewCode=0",
            QQ_PROFILE_URL, uin, uin
        );

        let headers = vec![
            (REFERER.as_str(), QQ_HEADERS_REFERER),
            (USER_AGENT.as_str(), QQ_HEADERS_UA),
            (COOKIE.as_str(), cookie),
        ];

        match request_text(&profile_url, "GET", &headers, None, 10000).await {
            Ok(text) => {
                if let Ok(body) = parse_json_text(&text) {
                    let data = body.get("data")
                        .or_else(|| body.get("profile"))
                        .or_else(|| body.get("creator"))
                        .or_else(|| body.get("result"))
                        .unwrap_or(&Value::Null);
                    let creator = data.get("creator")
                        .or_else(|| data.get("user"))
                        .or_else(|| data.get("profile"))
                        .unwrap_or(data);

                    // 提取 profile 中的 VIP 信息 (对照 normalizeQQProfile)
                    let vip_info = data.get("vipInfo")
                        .or_else(|| data.get("vipinfo"))
                        .or_else(|| data.get("vip"))
                        .or_else(|| creator.get("vipInfo"))
                        .or_else(|| creator.get("vipinfo"));
                    if let Some(vi) = vip_info {
                        if vi.is_object() {
                            profile_vip_info = Some(vi.clone());
                        }
                    }
                    // 也可以从整个 data+creator 中检测
                    if profile_vip_info.is_none() {
                        // 检查 creator/data 本身是否有 VIP 信号
                        let combined = json!({ "data": data, "creator": creator });
                        let mut assessments = Vec::new();
                        collect_qq_vip_assessments(&combined, &mut assessments, 0, &uin);
                        if !assessments.is_empty() {
                            let result = combine_qq_vip_results(&assessments);
                            if result.membership_known {
                                profile_vip_info = Some(json!({
                                    "isVip": result.is_vip,
                                    "isSvip": result.is_svip,
                                    "vipLevel": result.vip_level,
                                    "vipType": result.vip_type,
                                    "svipType": result.svip_type,
                                }));
                            }
                        }
                    }

                    if nickname.is_empty() {
                        nickname = creator.get("nick")
                            .or_else(|| creator.get("nickname"))
                            .or_else(|| creator.get("name"))
                            .or_else(|| creator.get("hostname"))
                            .or_else(|| creator.get("title"))
                            .and_then(|v| v.as_str())
                            .map(|s| decode_qq_cookie_value(s))
                            .unwrap_or_default();
                    }
                    if avatar.is_empty() {
                        avatar = creator.get("headpic")
                            .or_else(|| creator.get("avatar"))
                            .or_else(|| creator.get("avatarUrl"))
                            .or_else(|| creator.get("logo"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                    }
                }
            }
            Err(e) => {
                log::warn!("[QQLogin] profile check failed: {e}");
            }
        }
    }

    // 策略 3: 从 cookie 提取昵称/头像作为回退 (对照 qqCookieNickname)
    if nickname.is_empty() {
        let obj = cookie_util::parse_cookie_string(cookie);
        // 优先精确匹配 ptnick_{uin} 和 ptnick_0{uin}
        let padded_uin = format!("0{}", uin);
        let precise_keys = [
            format!("ptnick_{}", uin),
            format!("ptnick_{}", padded_uin),
        ];
        for key in &precise_keys {
            if let Some(val) = obj.get(key) {
                let decoded = decode_qq_cookie_value(val);
                if !decoded.is_empty() {
                    nickname = decoded;
                    break;
                }
            }
        }
        // 模糊匹配 ptnick_*
        if nickname.is_empty() {
            let ptnick_keys: Vec<String> = obj.keys()
                .filter(|k| k.starts_with("ptnick_"))
                .cloned()
                .collect();
            for key in &ptnick_keys {
                if let Some(val) = obj.get(key) {
                    let decoded = decode_qq_cookie_value(val);
                    if !decoded.is_empty() {
                        nickname = decoded;
                        break;
                    }
                }
            }
        }
        // 其他 cookie 字段
        if nickname.is_empty() {
            nickname = obj.get("ptnick")
                .or_else(|| obj.get("nick"))
                .or_else(|| obj.get("nickname"))
                .or_else(|| obj.get("qq_nickname"))
                .map(|s| decode_qq_cookie_value(s))
                .unwrap_or_default();
        }
    }
    if avatar.is_empty() {
        let obj = cookie_util::parse_cookie_string(cookie);
        avatar = obj.get("qqmusic_avatar")
            .or_else(|| obj.get("avatar"))
            .or_else(|| obj.get("avatarUrl"))
            .or_else(|| obj.get("headpic"))
            .map(|s| decode_qq_cookie_value(s))
            .unwrap_or_default();
        if avatar.is_empty() && !uin.is_empty() {
            avatar = format!("https://q1.qlogo.cn/g?b=qq&nk={}&s=100", uin);
        }
    }
    if nickname.is_empty() && !uin.is_empty() {
        nickname = format!("QQ {}", uin);
    }

    // VIP 探测: 使用多探针策略
    let probe_result = fetch_qq_vip_multi_probe(cookie, &uin, &music_key).await;

    // 合并策略: 多探针 > profile VIP 信息 > 旧版单探针降级
    let (is_vip, is_svip, vip_level, vip_type) = if probe_result.resolved && probe_result.membership_known {
        log::info!("[QQLogin] multi-probe result: is_vip={}, is_svip={}, level={}",
            probe_result.is_vip, probe_result.is_svip, probe_result.vip_level);
        (probe_result.is_vip, probe_result.is_svip, probe_result.vip_level, probe_result.vip_type)
    } else if let Some(ref pvi) = profile_vip_info {
        // 从 profile homepage 提取的 VIP 信息作为降级
        let p_is_vip = pvi.get("isVip").and_then(|v| v.as_bool()).unwrap_or(false)
            || pvi.get("vipType").and_then(|v| v.as_i64()).map(|n| n > 0).unwrap_or(false);
        let p_is_svip = pvi.get("isSvip").and_then(|v| v.as_bool()).unwrap_or(false)
            || pvi.get("svipType").and_then(|v| v.as_i64()).map(|n| n > 0).unwrap_or(false);
        let p_vip_type = pvi.get("vipType").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let p_level = pvi.get("vipLevel").and_then(|v| v.as_str()).unwrap_or(
            if p_is_svip { "svip" } else if p_is_vip { "vip" } else { "none" }
        );
        log::info!("[QQLogin] using profile VIP fallback: is_vip={}, is_svip={}, level={}",
            p_is_vip, p_is_svip, p_level);
        (p_is_vip, p_is_svip, p_level, p_vip_type)
    } else {
        // 最后的降级: 旧版单探针 (兼容性)
        log::info!("[QQLogin] multi-probe and profile unresolved, falling back to single probe");
        fetch_vip_status(cookie, &uin, &music_key).await
    };

    Ok(LoginInfo {
        provider: "qqmusic".into(),
        logged_in: true,
        user_id: uin,
        nickname,
        avatar,
        vip_type,
        vip_level: vip_level.to_string(),
        is_vip,
        is_svip,
    })
}

/// QQ VIP 探测结果
#[derive(Debug, Clone, Default)]
struct QqVipResult {
    is_vip: bool,
    is_svip: bool,
    vip_level: &'static str,
    vip_type: i32,
    svip_type: i32,
    resolved: bool,
    membership_known: bool,
    expires_at: i64,
}

/// 规范化键名 (对照 canonicalQQVipKey)
fn canonical_qq_vip_key(value: &str) -> String {
    value.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

/// 解析原始会员信号 (对照 primitiveMembershipSignal)
fn parse_qq_membership_signal(value: &Value) -> Option<i32> {
    match value {
        Value::Bool(true) => Some(1),
        Value::Bool(false) => Some(0),
        Value::Number(n) => {
            let f = n.as_f64().unwrap_or(0.0);
            if f.is_finite() { Some(if f > 0.0 { 1 } else { 0 }) } else { None }
        }
        Value::String(s) => {
            let text = s.trim().to_lowercase();
            if text.is_empty() { return None; }
            if let Ok(n) = text.parse::<f64>() {
                if n.is_finite() { return Some(if n > 0.0 { 1 } else { 0 }); }
            }
            if matches!(text.as_str(), "true" | "yes" | "active" | "valid" | "opened" | "open" | "vip" | "svip" | "premium" | "member") { return Some(1); }
            if matches!(text.as_str(), "false" | "no" | "none" | "normal" | "ordinary" | "expired" | "inactive" | "closed" | "invalid") { return Some(0); }
            // 中文标签
            if matches!(text.as_str(), "已开通" | "有效" | "会员" | "绿钻" | "豪华绿钻") { return Some(1); }
            if matches!(text.as_str(), "未开通" | "已过期" | "过期" | "普通用户" | "普通账号" | "非会员") { return Some(0); }
            None
        }
        _ => None,
    }
}

/// 解析过期时间 (对照 normalizedExpiryMs)
fn parse_qq_expiry_ms(value: &Value) -> i64 {
    let raw = match value {
        Value::Number(n) => n.as_i64().unwrap_or(0),
        Value::String(s) => {
            if let Ok(n) = s.trim().parse::<i64>() { n }
            else { 0 }
        }
        _ => return 0,
    };
    if raw <= 0 { return 0; }
    // 秒级时间戳转为毫秒
    let ms = if raw < 10000000000 { raw * 1000 } else { raw };
    if ms >= 946684800000 { ms } else { 0 }
}

// QQ VIP 类型键 (对照 VIP_TYPE_KEYS)
const QQ_VIP_TYPE_KEYS: &[&str] = &[
    "viptype", "viplevel", "musicviptype", "musicviplevel",
    "greenviptype", "greenviplevel", "greenlevel",
    "associatortype", "associatorlevel",
];
// QQ SVIP 类型键 (对照 SVIP_TYPE_KEYS)
const QQ_SVIP_TYPE_KEYS: &[&str] = &[
    "sviptype", "sviplevel", "superviptype", "superviplevel",
    "luxuryviptype", "luxuryviplevel", "greensvip",
];
// QQ VIP 标志键 (对照 VIP_FLAG_KEYS)
const QQ_VIP_FLAG_KEYS: &[&str] = &[
    "isvip", "ivipflag", "inewvip", "vip", "vipflag",
    "isgreenvip", "greenvip", "ismember", "member",
    "isassociator", "associator",
];
// QQ SVIP 标志键 (对照 SVIP_FLAG_KEYS)
const QQ_SVIP_FLAG_KEYS: &[&str] = &[
    "issvip", "isupervip", "inewsupervip", "svip",
    "issupervip", "supervip", "isluxuryvip", "luxuryvip",
];
// QQ 过期时间键 (对照 MEMBERSHIP_EXPIRY_KEYS)
const QQ_EXPIRY_KEYS: &[&str] = &[
    "expire", "expires", "expireat", "expiretime",
    "expiry", "expiryat", "expirytime", "endtime",
    "validuntil", "validtime", "deadline", "duetime",
    "vipexpireat", "vipexpiretime", "vipexpirytime", "vipendtime",
    "musicvipexpiretime", "musicvipendtime",
    "greenvipexpiretime", "greenvipendtime",
    "associatorexpiretime", "associatorendtime",
    "svipexpiretime", "svipendtime",
    "supervipexpiretime", "supervipendtime", "superendtime",
    "luxuryvipexpiretime", "luxuryvipendtime",
];

/// 评估单个 QQ VIP 对象 (对照 assessQQVipObject)
fn assess_qq_vip_object(obj: &Value) -> Option<QqVipResult> {
    if !obj.is_object() { return None; }
    let map = match obj.as_object() { Some(m) => m, None => { return None; } };

    let mut vip_positive = false;
    let mut svip_positive = false;
    let mut vip_type: i64 = 0;
    let mut svip_type: i64 = 0;
    let mut evidence = false;
    let mut vip_expiry_values: Vec<i64> = Vec::new();
    let mut svip_expiry_values: Vec<i64> = Vec::new();
    let now_ms = now_ms() as i64;

    for (key, val) in map {
        let normalized = canonical_qq_vip_key(key);

        // 过期时间检查
        if QQ_EXPIRY_KEYS.contains(&normalized.as_str()) {
            let expiry = parse_qq_expiry_ms(val);
            if expiry > 0 {
                let is_svip_expiry = normalized.starts_with("svip") || normalized.starts_with("supervip") || normalized.starts_with("luxuryvip")
                    || normalized.starts_with("superend");
                if is_svip_expiry {
                    svip_expiry_values.push(expiry);
                } else {
                    vip_expiry_values.push(expiry);
                }
            }
            continue;
        }

        if QQ_VIP_TYPE_KEYS.contains(&normalized.as_str()) {
            evidence = true;
            if let Some(n) = val.as_i64() {
                if n > 0 { vip_type = vip_type.max(n); vip_positive = true; }
            }
            continue;
        }
        if QQ_SVIP_TYPE_KEYS.contains(&normalized.as_str()) {
            evidence = true;
            if let Some(n) = val.as_i64() {
                if n > 0 { svip_type = svip_type.max(n); svip_positive = true; }
            }
            continue;
        }
        if QQ_VIP_FLAG_KEYS.contains(&normalized.as_str()) {
            if let Some(sig) = parse_qq_membership_signal(val) {
                evidence = true;
                if sig > 0 { vip_positive = true; }
            }
            continue;
        }
        if QQ_SVIP_FLAG_KEYS.contains(&normalized.as_str()) {
            if let Some(sig) = parse_qq_membership_signal(val) {
                evidence = true;
                if sig > 0 { svip_positive = true; }
            }
        }
    }

    if !evidence { return None; }

    // 过期时间覆盖
    let vip_expires_at = vip_expiry_values.iter().max().copied().unwrap_or(0);
    let svip_expires_at = svip_expiry_values.iter().max().copied().unwrap_or(0);
    if !vip_expiry_values.is_empty() && vip_expires_at > 0 && vip_expires_at <= now_ms {
        vip_positive = false; vip_type = 0;
    }
    if !svip_expiry_values.is_empty() && svip_expires_at > 0 && svip_expires_at <= now_ms {
        svip_positive = false; svip_type = 0;
    }

    let is_svip = svip_positive;
    let is_vip = is_svip || vip_positive;
    let vip_level = if is_svip { "svip" } else if is_vip { "vip" } else { "none" };
    let expires_at = if is_svip { svip_expires_at.max(vip_expires_at) } else if is_vip { vip_expires_at } else { 0 };

    Some(QqVipResult {
        is_vip,
        is_svip,
        vip_level,
        vip_type: vip_type as i32,
        svip_type: svip_type as i32,
        resolved: true,
        membership_known: true,
        expires_at,
    })
}

/// 递归收集 QQ VIP 评估 (对照 collectQQVipAssessments)
fn collect_qq_vip_assessments(value: &Value, out: &mut Vec<QqVipResult>, depth: u32, expected_uin: &str) {
    if depth > 8 || value.is_null() { return; }
    if let Some(arr) = value.as_array() {
        for item in arr {
            collect_qq_vip_assessments(item, out, depth + 1, expected_uin);
        }
        return;
    }
    if !value.is_object() { return; }
    if let Some(map) = value.as_object() {
        // 检查 uin 过滤
        if !expected_uin.is_empty() {
            let obj_uin: String = map.get("uin")
                .or_else(|| map.get("useruin"))
                .or_else(|| map.get("userid"))
                .or_else(|| map.get("qq"))
                .or_else(|| map.get("accountuin"))
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    _ => String::new(),
                })
                .unwrap_or_default()
                .chars().filter(|c| c.is_ascii_digit()).collect();
            if !obj_uin.is_empty() && obj_uin != expected_uin { return; }
        }

        if let Some(assessment) = assess_qq_vip_object(value) {
            out.push(assessment);
        }

        for (_, child) in map {
            if child.is_object() || child.is_array() {
                collect_qq_vip_assessments(child, out, depth + 1, expected_uin);
            }
        }
    }
}

/// 合并多个 QQ VIP 结果 (对照 combineQQVipResults)
fn combine_qq_vip_results(results: &[QqVipResult]) -> QqVipResult {
    let positives: Vec<&QqVipResult> = results.iter().filter(|r| r.is_vip || r.is_svip).collect();
    if positives.is_empty() {
        return QqVipResult { resolved: true, ..Default::default() };
    }
    let is_svip = positives.iter().any(|r| r.is_svip);
    let is_vip = true;
    let vip_type = positives.iter().map(|r| r.vip_type).max().unwrap_or(0);
    let svip_type = if is_svip { positives.iter().map(|r| r.svip_type).max().unwrap_or(0) } else { 0 };
    let vip_level = if is_svip { "svip" } else { "vip" };
    let expires_at = positives.iter().map(|r| r.expires_at).filter(|&e| e > 0).max().unwrap_or(0);

    QqVipResult {
        is_vip, is_svip, vip_level, vip_type, svip_type,
        resolved: true, membership_known: true, expires_at,
    }
}

/// VIP 状态探测 (对照 fetchQQVipStatus + resolveQQVipFromProbes)
/// 使用3个探针: SRFVipQuery_V2 list, SRFVipQuery V1 list, SRFVipQuery_V2 single
async fn fetch_qq_vip_multi_probe(cookie: &str, uin: &str, music_key: &str) -> QqVipResult {
    if uin.is_empty() || music_key.is_empty() {
        return QqVipResult::default();
    }

    let comm = json!({
        "uin": uin,
        "format": "json",
        "ct": 24,
        "cv": 0,
        "authst": music_key
    });

    // 定义3个探针 (与 Mineradio 完全一致)
    struct Probe {
        _source: &'static str,
        response_key: &'static str,
        payload: Value,
    }

    let probes: Vec<Probe> = vec![
        Probe {
            _source: "qq-vip-query-v2-list",
            response_key: "req_1",
            payload: json!({
                "comm": comm.clone(),
                "req_1": {
                    "module": "userInfo.VipQueryServer",
                    "method": "SRFVipQuery_V2",
                    "param": { "uin_list": [uin.to_string()] }
                }
            }),
        },
        Probe {
            _source: "qq-vip-query-v1-list",
            response_key: "req_1",
            payload: json!({
                "comm": comm.clone(),
                "req_1": {
                    "module": "userInfo.VipQueryServer",
                    "method": "SRFVipQuery",
                    "param": { "uin_list": [uin.to_string()] }
                }
            }),
        },
        Probe {
            _source: "qq-vip-query-v2-single",
            response_key: "vip",
            payload: json!({
                "comm": comm.clone(),
                "vip": {
                    "module": "userInfo.VipQueryServer",
                    "method": "SRFVipQuery_V2",
                    "param": { "uin": uin.to_string(), "uin_list": [uin.to_string()] }
                }
            }),
        },
    ];

    // 并行执行所有探针
    let futures: Vec<_> = probes.iter().map(|probe| {
        let payload = probe.payload.clone();
        let response_key = probe.response_key;
        let uin_owned = uin.to_string();
        let cookie_owned = cookie.to_string();
        async move {
            match qq_musicu_request(&payload, &cookie_owned, 4200).await {
                Ok(json) => {
                    let data = json.get(response_key)
                        .and_then(|r| r.get("data"))
                        .unwrap_or(&Value::Null);
                    let mut assessments = Vec::new();
                    collect_qq_vip_assessments(data, &mut assessments, 0, &uin_owned);
                    if !assessments.is_empty() {
                        Some(combine_qq_vip_results(&assessments))
                    } else {
                        None
                    }
                }
                Err(_) => None,
            }
        }
    }).collect();

    let results: Vec<QqVipResult> = futures_util::future::join_all(futures).await
        .into_iter()
        .filter_map(|r| r)
        .collect();

    if !results.is_empty() {
        combine_qq_vip_results(&results)
    } else {
        QqVipResult::default()
    }
}

// 保留旧版 fetch_vip_status 供兼容 (内部调用新多探针版本)
async fn fetch_vip_status(cookie: &str, uin: &str, music_key: &str) -> (bool, bool, &'static str, i32) {
    let result = fetch_qq_vip_multi_probe(cookie, uin, music_key).await;
    (result.is_vip, result.is_svip, result.vip_level, result.vip_type)
}

// ============================================================
//  歌单同步 (对照 handleQQUserPlaylists)
// ============================================================

/// 获取用户创建的歌单 (对照 fetchQQCreatedPlaylists)
async fn fetch_created_playlists(uin: &str, cookie: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for page in 0..QQ_PLAYLIST_SYNC_MAX_PAGES {
        let sin = page * QQ_PLAYLIST_SYNC_PAGE_SIZE as usize;
        let sin_str = sin.to_string();
        let size_str = QQ_PLAYLIST_SYNC_PAGE_SIZE.to_string();
        let params: Vec<(&str, &str)> = vec![
            ("hostUin", "0"),
            ("hostuin", uin),
            ("sin", &sin_str),
            ("size", &size_str),
            ("g_tk", "5381"),
            ("loginUin", uin),
            ("format", "json"),
            ("inCharset", "utf8"),
            ("outCharset", "utf-8"),
            ("notice", "0"),
            ("platform", "yqq.json"),
            ("needNewCode", "0"),
        ];
        let headers = vec![("Referer", "https://y.qq.com/portal/profile.html")];
        match qq_get_json(QQ_PLAYLIST_CREATED_URL, &params, cookie, &headers).await {
            Ok(body) => {
                let rows = body.get("data")
                    .and_then(|d| d.get("disslist"))
                    .and_then(|l| l.as_array())
                    .cloned()
                    .unwrap_or_default();
                let len = rows.len();
                out.extend(rows);
                if len < QQ_PLAYLIST_SYNC_PAGE_SIZE as usize {
                    break;
                }
            }
            Err(e) => {
                log::warn!("[QQPlaylist] created page {} failed: {e}", page);
                break;
            }
        }
    }
    out
}

/// 获取用户收藏的歌单 (musicu API — 现代接口)
/// 使用 music.musicasset.PlaylistBaseRead / GetPlaylistByUin, reqtype=2 表示收藏歌单
async fn fetch_collected_playlists_musicu(uin: &str, cookie: &str) -> Vec<Value> {
    let auth = extract_qq_auth(cookie);
    let mut out = Vec::new();

    for page in 0..QQ_PLAYLIST_SYNC_MAX_PAGES {
        let page_num = (page + 1) as u32;
        let comm = if !auth.music_key.is_empty() {
            json!({
                "uin": uin,
                "format": "json",
                "ct": 19,
                "cv": 0,
                "authst": auth.music_key
            })
        } else {
            json!({ "ct": 24, "cv": 0 })
        };

        let payload = json!({
            "comm": comm,
            "req_0": {
                "module": "music.musicasset.PlaylistBaseRead",
                "method": "GetPlaylistByUin",
                "param": {
                    "hostUin": uin,
                    "reqtype": 2,
                    "page": page_num,
                    "size": QQ_PLAYLIST_SYNC_PAGE_SIZE,
                    "order": 5
                }
            }
        });

        match qq_musicu_request(&payload, cookie, 10000).await {
            Ok(json) => {
                let req0 = json.get("req_0").unwrap_or(&Value::Null);
                let data = req0.get("data").unwrap_or(&Value::Null);
                // 检查错误码
                let code = req0.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
                if code != 0 {
                    log::warn!("[QQPlaylist] collected musicu page {} code={}, data keys: {}",
                        page, code, data.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>().join(",")).unwrap_or_default());
                    break;
                }
                // 兼容多种响应字段名
                let rows = data.get("v_playlist")
                    .or_else(|| data.get("cdlist"))
                    .or_else(|| data.get("playlist"))
                    .or_else(|| data.get("disslist"))
                    .and_then(|l| l.as_array())
                    .cloned()
                    .unwrap_or_default();
                let len = rows.len();
                log::info!("[QQPlaylist] collected musicu page {} got {} playlists, data keys: {}",
                    page, len, data.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>().join(",")).unwrap_or_default());
                if len == 0 {
                    // 如果是第一页且没有数据，记录完整响应用于调试
                    if page == 0 {
                        log::warn!("[QQPlaylist] collected musicu page 0 empty, full req_0: {}",
                            serde_json::to_string(req0).unwrap_or_default());
                    }
                    break;
                }
                out.extend(rows);
                if len < QQ_PLAYLIST_SYNC_PAGE_SIZE as usize {
                    break;
                }
            }
            Err(e) => {
                log::warn!("[QQPlaylist] collected musicu page {} failed: {e}", page);
                break;
            }
        }
    }
    out
}

/// 获取用户收藏的歌单 (对照 fetchQQCollectedPlaylists)
/// 优先使用旧版 API (reqtype=3)，与 Mineradio 完全一致
async fn fetch_collected_playlists(uin: &str, cookie: &str) -> Vec<Value> {
    // 优先使用旧版 API (Mineradio 方式，reqtype=3)
    let mut out = Vec::new();
    for page in 0..QQ_PLAYLIST_SYNC_MAX_PAGES {
        let sin = page * QQ_PLAYLIST_SYNC_PAGE_SIZE as usize;
        let ein = sin + QQ_PLAYLIST_SYNC_PAGE_SIZE as usize - 1;
        let sin_str = sin.to_string();
        let ein_str = ein.to_string();
        let params: Vec<(&str, &str)> = vec![
            ("ct", "20"),
            ("cid", "205360956"),
            ("userid", uin),
            ("reqtype", "3"),
            ("sin", &sin_str),
            ("ein", &ein_str),
        ];
        let headers = vec![("Referer", "https://y.qq.com/portal/profile.html")];
        match qq_get_json(QQ_PLAYLIST_COLLECTED_URL, &params, cookie, &headers).await {
            Ok(body) => {
                let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
                let data = body.get("data").unwrap_or(&Value::Null);
                let rows = data.get("cdlist")
                    .or_else(|| data.get("v_playlist"))
                    .or_else(|| data.get("playlist"))
                    .or_else(|| data.get("disslist"))
                    .and_then(|l| l.as_array())
                    .cloned()
                    .unwrap_or_default();
                let len = rows.len();
                log::info!("[QQPlaylist] collected legacy page {} code={} got {} playlists, data keys: {}",
                    page, code, len, data.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>().join(",")).unwrap_or_default());
                if len == 0 && page == 0 {
                    let body_str = serde_json::to_string(&body).unwrap_or_default();
                    let preview = &body_str[..body_str.len().min(500)];
                    log::warn!("[QQPlaylist] collected legacy page 0 empty, body preview: {}", preview);
                }
                out.extend(rows);
                if len < QQ_PLAYLIST_SYNC_PAGE_SIZE as usize {
                    break;
                }
            }
            Err(e) => {
                log::warn!("[QQPlaylist] collected legacy page {} failed: {e}", page);
                break;
            }
        }
    }

    // 如果旧版 API 返回空，降级到 musicu API
    if out.is_empty() {
        log::info!("[QQPlaylist] legacy collected empty, trying musicu API");
        out = fetch_collected_playlists_musicu(uin, cookie).await;
        if !out.is_empty() {
            log::info!("[QQPlaylist] collected via musicu fallback: {} playlists", out.len());
        }
    } else {
        log::info!("[QQPlaylist] collected via legacy: {} playlists", out.len());
    }
    out
}

/// 获取我喜欢歌单卡片
async fn get_liked_playlist_card(cookie: &str) -> Playlist {
    let auth = extract_qq_auth(cookie);
    if !auth.playback_ready {
        return Playlist {
            provider: "qqmusic".into(),
            id: QQ_LIKED_PLAYLIST_ID.into(),
            name: QQ_LIKED_PLAYLIST_NAME.into(),
            cover: QQ_LIKED_PLAYLIST_COVER.into(),
            track_count: 0,
            creator: if !auth.uin.is_empty() { auth.uin.clone() } else { "QQ 音乐".into() },
            subscribed: false,
        };
    }
    // 尝试获取一首歌来获取封面
    let payload = json!({
        "comm": { "ct": 24, "cv": 0 },
        "req_0": {
            "module": "music.srfDissInfo.DissInfo",
            "method": "CgiGetDiss",
            "param": {
                "disstid": 0,
                "dirid": QQ_LIKED_DIRID,
                "tag": 1,
                "song_begin": 0,
                "song_num": 1,
                "userinfo": 1,
                "orderlist": 1
            }
        }
    });
    let mut count = 0u32;
    if let Ok(json) = qq_musicu_request(&payload, cookie, 10000).await {
        let data = json.get("req_0")
            .and_then(|r| r.get("data"))
            .unwrap_or(&Value::Null);
        count = data.get("total_song_num")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32;
    }
    Playlist {
        provider: "qqmusic".into(),
        id: QQ_LIKED_PLAYLIST_ID.into(),
        name: QQ_LIKED_PLAYLIST_NAME.into(),
        cover: QQ_LIKED_PLAYLIST_COVER.into(),
        track_count: count,
        creator: if !auth.uin.is_empty() { auth.uin.clone() } else { "QQ 音乐".into() },
        subscribed: false,
    }
}

/// 获取用户歌单 (对照 handleQQUserPlaylists)
pub async fn user_playlists(cookie: &str) -> Result<Vec<Playlist>, String> {
    let auth = extract_qq_auth(cookie);
    if !auth.logged_in || auth.uin.is_empty() {
        log::warn!("[QQPlaylist] user_playlists: not logged in");
        return Ok(vec![]);
    }
    let uin = auth.uin.clone();
    log::info!("[QQPlaylist] user_playlists: uin={}, logged_in={}, playback_ready={}", uin, auth.logged_in, auth.playback_ready);

    let created_cookie = cookie.to_string();
    let collected_cookie = cookie.to_string();
    let liked_cookie = cookie.to_string();

    let (created_raw, collect_raw, liked_card) = tokio::join!(
        fetch_created_playlists(&uin, &created_cookie),
        fetch_collected_playlists(&uin, &collected_cookie),
        get_liked_playlist_card(&liked_cookie),
    );

    log::info!("[QQPlaylist] raw counts: created={}, collected={}, liked_card.id={}",
        created_raw.len(), collect_raw.len(), liked_card.id);

    let created: Vec<Playlist> = created_raw.iter()
        .map(|pl| map_qq_playlist(pl, "created"))
        .collect();
    let collected: Vec<Playlist> = collect_raw.iter()
        .map(|pl| map_qq_playlist(pl, "collect"))
        .collect();

    log::info!("[QQPlaylist] mapped counts: created={}, collected={}", created.len(), collected.len());

    // 对照 Mineradio: 先过滤掉 created+collected 中的"我喜欢"歌单，再添加 liked_card
    // created.concat(collected).filter(pl => !isQQFavoritePlaylist(pl))
    // base.unshift(likedCard)
    let mut base: Vec<Playlist> = created.into_iter().chain(collected.into_iter())
        .filter(|pl| pl.id != QQ_LIKED_PLAYLIST_ID)
        .collect();
    base.insert(0, liked_card);

    // 记录去重前的 ID 列表
    let before_ids: Vec<String> = base.iter().map(|pl| format!("{}:{}", pl.id, pl.name)).collect();
    log::info!("[QQPlaylist] before dedup ({}): {}", base.len(), before_ids.join(", "));

    // 过滤掉 QQ 空间背景音乐歌单和重复
    let mut seen = std::collections::HashSet::new();
    let playlists: Vec<Playlist> = base.into_iter()
        .filter(|pl| {
            if pl.id.is_empty() || pl.name.is_empty() {
                return false;
            }
            if seen.contains(&pl.id) {
                log::debug!("[QQPlaylist] dedup removing duplicate id={}", pl.id);
                return false;
            }
            seen.insert(pl.id.clone());
            true
        })
        .collect();

    log::info!("[QQPlaylist] user_playlists result: {} playlists", playlists.len());
    Ok(playlists)
}

/// 获取歌单曲目 (对照 handleQQPlaylistTracks)
///
/// 使用旧版 CGI 拉取。部分"快闪/爆火"歌单会被 QQ 隐私保护拒绝
/// (subcode=4000 check privacy error), 属于平台限制, 无法绕过。
pub async fn playlist_tracks(id: &str, cookie: &str) -> Result<(Playlist, Vec<Song>), String> {
    let pid = id.trim();
    if pid.is_empty() {
        return Ok((Playlist::default(), vec![]));
    }

    // 如果是我喜欢歌单
    if is_qq_liked_playlist_id(pid) {
        return liked_playlist_tracks(cookie, 0, 100).await;
    }

    // 旧版 CGI 歌单曲目接口 (无需登录也能抓取公开歌单)
    playlist_tracks_legacy(pid, cookie).await
}

/// 旧版 CGI 歌单曲目接口 (兜底, 无需登录可抓取公开歌单)
async fn playlist_tracks_legacy(pid: &str, cookie: &str) -> Result<(Playlist, Vec<Song>), String> {
    let auth = extract_qq_auth(cookie);
    let uin = if auth.uin.is_empty() { "0".to_string() } else { auth.uin.clone() };
    let params: Vec<(&str, &str)> = vec![
        ("type", "1"),
        ("utf8", "1"),
        ("disstid", pid),
        ("song_begin", "0"),
        ("song_num", "100"),
        ("loginUin", &uin),
        ("format", "json"),
        ("inCharset", "utf8"),
        ("outCharset", "utf-8"),
        ("notice", "0"),
        ("platform", "yqq.json"),
        ("needNewCode", "0"),
    ];
    let headers = vec![("Referer", "https://y.qq.com/n/yqq/playlist")];
    let body = qq_get_json(QQ_PLAYLIST_TRACKS_URL, &params, cookie, &headers).await?;

    // 诊断: 输出接口返回, 便于排查"歌单打不开"
    let resp_subcode = body.get("subcode").and_then(|v| v.as_i64()).unwrap_or(0);
    let resp_msg = body.get("msg").or_else(|| body.get("errmsg")).and_then(|v| v.as_str()).unwrap_or("");
    let cdlist_len = body.get("cdlist").and_then(|c| c.as_array()).map(|a| a.len()).unwrap_or(0);
    if resp_subcode != 0 || !resp_msg.is_empty() || cdlist_len == 0 {
        log::warn!("[QQPlaylist] tracks subcode={} msg={} cdlist_len={} body={}", resp_subcode, resp_msg, cdlist_len, body);
    }

    let detail = body.get("cdlist")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .cloned()
        .unwrap_or(Value::Null);

    let raw_tracks = detail.get("songlist")
        .and_then(|s| s.as_array())
        .cloned()
        .unwrap_or_default();

    let tracks: Vec<Song> = raw_tracks.iter()
        .map(|raw| map_qq_playlist_track(raw))
        .filter(|s| !s.name.is_empty() && (s.mid.is_some() || !s.id.is_empty()))
        .collect();

    let total = detail.get("total_song_num")
        .or_else(|| detail.get("songnum"))
        .or_else(|| detail.get("song_cnt"))
        .or_else(|| detail.get("song_count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    let playlist = Playlist {
        provider: "qqmusic".into(),
        id: pid.to_string(),
        name: detail.get("dissname")
            .or_else(|| detail.get("diss_name"))
            .or_else(|| detail.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        cover: detail.get("logo")
            .or_else(|| detail.get("diss_cover"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        track_count: total,
        creator: "QQ 音乐".into(),
        subscribed: false,
    };

    Ok((playlist, tracks))
}

/// 获取歌单曲目（分页）
pub async fn playlist_tracks_range(id: &str, start: usize, count: usize, cookie: &str) -> Result<Vec<Song>, String> {
    let pid = id.trim();
    if pid.is_empty() {
        return Ok(vec![]);
    }

    // 如果是我喜欢歌单
    if is_qq_liked_playlist_id(pid) {
        let (_, tracks) = liked_playlist_tracks(cookie, start, count).await?;
        return Ok(tracks);
    }

    // 旧版 CGI 歌单曲目接口（分页）
    playlist_tracks_legacy_range(pid, start, count, cookie).await
}

/// 旧版 CGI 歌单曲目接口（分页）
async fn playlist_tracks_legacy_range(pid: &str, start: usize, count: usize, cookie: &str) -> Result<Vec<Song>, String> {
    let auth = extract_qq_auth(cookie);
    let uin = if auth.uin.is_empty() { "0".to_string() } else { auth.uin.clone() };
    let start_str = start.to_string();
    let limit_str = count.to_string();
    let params: Vec<(&str, &str)> = vec![
        ("type", "1"),
        ("utf8", "1"),
        ("disstid", pid),
        ("song_begin", &start_str),
        ("song_num", &limit_str),
        ("loginUin", &uin),
        ("format", "json"),
        ("inCharset", "utf8"),
        ("outCharset", "utf-8"),
        ("notice", "0"),
        ("platform", "yqq.json"),
        ("needNewCode", "0"),
    ];
    let headers = vec![("Referer", "https://y.qq.com/n/yqq/playlist")];
    let body = qq_get_json(QQ_PLAYLIST_TRACKS_URL, &params, cookie, &headers).await?;

    let detail = body.get("cdlist")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .cloned()
        .unwrap_or(Value::Null);

    let raw_tracks = detail.get("songlist")
        .and_then(|s| s.as_array())
        .cloned()
        .unwrap_or_default();

    let tracks: Vec<Song> = raw_tracks.iter()
        .map(|raw| map_qq_playlist_track(raw))
        .filter(|s| !s.name.is_empty() && (s.mid.is_some() || !s.id.is_empty()))
        .collect();

    Ok(tracks)
}

/// 获取我喜欢歌单的曲目 (对照 handleQQLikedPlaylistTracks)，支持分页
async fn liked_playlist_tracks(cookie: &str, start: usize, count: usize) -> Result<(Playlist, Vec<Song>), String> {
    let auth = extract_qq_auth(cookie);
    if !auth.playback_ready {
        return Ok((
            Playlist {
                provider: "qqmusic".into(),
                id: QQ_LIKED_PLAYLIST_ID.into(),
                name: QQ_LIKED_PLAYLIST_NAME.into(),
                cover: QQ_LIKED_PLAYLIST_COVER.into(),
                track_count: 0,
                creator: if !auth.uin.is_empty() { auth.uin.clone() } else { "QQ 音乐".into() },
                subscribed: false,
            },
            vec![],
        ));
    }

    let payload = json!({
        "comm": { "ct": 24, "cv": 0 },
        "req_0": {
            "module": "music.srfDissInfo.DissInfo",
            "method": "CgiGetDiss",
            "param": {
                "disstid": 0,
                "dirid": QQ_LIKED_DIRID,
                "tag": 1,
                "song_begin": start,
                "song_num": count,
                "userinfo": 1,
                "orderlist": 1
            }
        }
    });

    match qq_musicu_request(&payload, cookie, 10000).await {
        Ok(json) => {
            let block = json.get("req_0").unwrap_or(&Value::Null);
            let data = block.get("data").unwrap_or(&Value::Null);
            let raw_tracks = data.get("songlist")
                .and_then(|s| s.as_array())
                .cloned()
                .unwrap_or_default();
            let tracks: Vec<Song> = raw_tracks.iter()
                .map(|raw| map_qq_playlist_track(raw))
                .filter(|s| !s.name.is_empty() && (s.mid.is_some() || !s.id.is_empty()))
                .collect();
            let total = data.get("total_song_num")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;

            Ok((
                Playlist {
                    provider: "qqmusic".into(),
                    id: QQ_LIKED_PLAYLIST_ID.into(),
                    name: QQ_LIKED_PLAYLIST_NAME.into(),
                    cover: QQ_LIKED_PLAYLIST_COVER.into(),
                    track_count: total,
                    creator: if !auth.uin.is_empty() { auth.uin.clone() } else { "QQ 音乐".into() },
                    subscribed: false,
                },
                tracks,
            ))
        }
        Err(e) => {
            log::warn!("[QQLiked] tracks failed: {e}");
            Ok((
                Playlist {
                    provider: "qqmusic".into(),
                    id: QQ_LIKED_PLAYLIST_ID.into(),
                    name: QQ_LIKED_PLAYLIST_NAME.into(),
                    cover: QQ_LIKED_PLAYLIST_COVER.into(),
                    track_count: 0,
                    creator: if !auth.uin.is_empty() { auth.uin.clone() } else { "QQ 音乐".into() },
                    subscribed: false,
                },
                vec![],
            ))
        }
    }
}

// ============================================================
//  歌手 / 歌单搜索 (对照 handleQQArtistDetail / handleQQSearch)
// ============================================================
fn extract_toplist_item(item: &Value) -> Option<Playlist> {
    let id = item.get("topId")
        .or_else(|| item.get("topID"))
        .and_then(|v| match v {
            Value::Number(n) => Some(n.to_string()),
            Value::String(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let name = item.get("topName")
        .or_else(|| item.get("title"))
        .or_else(|| item.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let cover = item.get("pic")
        .or_else(|| item.get("headPic"))
        .or_else(|| item.get("frontPic"))
        .or_else(|| item.get("imgurl"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let track_count = item.get("songCount")
        .or_else(|| item.get("songNum"))
        .or_else(|| item.get("song_count"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;

    if id.is_empty() || name.is_empty() {
        return None;
    }
    Some(Playlist {
        provider: "qqmusic".into(),
        id,
        name,
        cover,
        track_count,
        creator: "QQ 音乐".into(),
        subscribed: false,
    })
}

/// 从 CGI/musicu 响应中解析榜单列表 (兼容多种格式)
fn parse_toplist_list_from_json(json: &Value) -> Vec<Playlist> {
    let mut playlists = Vec::new();

    // 格式 1: data.topList[] (扁平数组)
    let flat_list = json.get("data")
        .and_then(|d| d.get("topList"))
        .and_then(|t| t.as_array());
    if let Some(arr) = flat_list {
        for item in arr {
            if let Some(pl) = extract_toplist_item(item) {
                playlists.push(pl);
            }
        }
    }

    // 格式 2: data.groupList[].topList[] (分组数组)
    if playlists.is_empty() {
        if let Some(groups) = json.get("data")
            .and_then(|d| d.get("groupList"))
            .and_then(|t| t.as_array())
        {
            for group in groups {
                if let Some(arr) = group.get("topList").and_then(|t| t.as_array()) {
                    for item in arr {
                        if let Some(pl) = extract_toplist_item(item) {
                            playlists.push(pl);
                        }
                    }
                }
            }
        }
    }

    // 格式 3: data.topGroup[].topList[]
    if playlists.is_empty() {
        if let Some(groups) = json.get("data")
            .and_then(|d| d.get("topGroup"))
            .and_then(|t| t.as_array())
        {
            for group in groups {
                if let Some(arr) = group.get("topList").and_then(|t| t.as_array()) {
                    for item in arr {
                        if let Some(pl) = extract_toplist_item(item) {
                            playlists.push(pl);
                        }
                    }
                }
            }
        }
    }

    // 格式 4: 顶层 topList[]
    if playlists.is_empty() {
        if let Some(arr) = json.get("topList").and_then(|t| t.as_array()) {
            for item in arr {
                if let Some(pl) = extract_toplist_item(item) {
                    playlists.push(pl);
                }
            }
        }
    }

    playlists
}

/// 从 CGI/musicu 响应中解析歌曲列表
fn parse_toplist_songs_from_json(json: &Value, limit: usize) -> Vec<Song> {
    // 尝试多种路径
    let song_list = json.get("data")
        .and_then(|d| d.get("songInfoList"))
        .and_then(|s| s.as_array())
        .or_else(|| {
            json.get("data")
                .and_then(|d| d.get("song"))
                .and_then(|s| s.get("songInfoList"))
                .and_then(|s| s.as_array())
        })
        .or_else(|| {
            json.get("data")
                .and_then(|d| d.get("songlist"))
                .and_then(|s| s.as_array())
        })
        .or_else(|| {
            json.get("songlist")
                .and_then(|s| s.as_array())
        })
        .or_else(|| {
            json.get("data")
                .and_then(|d| d.get("song"))
                .and_then(|s| s.as_array())
        });

    match song_list {
        Some(arr) => {
            arr.iter().take(limit).map(|item| {
                let track = item.get("data")
                    .or_else(|| item.get("songInfo"))
                    .or_else(|| item.get("song"))
                    .unwrap_or(item);
                map_qq_track(track, &Song::default())
            }).filter(|s| !s.name.is_empty() && (s.mid.is_some() || !s.id.is_empty()))
            .collect()
        }
        None => Vec::new(),
    }
}

/// QQ 音乐排行榜预设列表 (榜单列表接口已失效, 使用实测可用的 topid)
const QQ_RANK_PRESETS: &[(&str, &str, &str)] = &[
    ("26", "巅峰榜·热歌", "https://y.gtimg.cn/music/photo_new/T003R300x300M000003kErY34CR2zg.jpg"),
    ("27", "巅峰榜·新歌", "https://y.gtimg.cn/music/photo_new/T003R300x300M000003lDPtw0sKXJK.jpg"),
    ("4", "巅峰榜·流行指数", "https://y.gtimg.cn/music/photo_new/T003R300x300M000002aA7GQ0YoZYe.jpg"),
    ("62", "飙升榜", "https://y.gtimg.cn/music/photo_new/T003R300x300M000002DWfWl0cjhjP.jpg"),
    ("5", "巅峰榜·内地", "https://y.gtimg.cn/music/photo_new/T003R300x300M000003V5yNn4TIRVP.jpg"),
    ("6", "巅峰榜·港台", "https://y.gtimg.cn/music/photo_new/T003R300x300M000000CLXb916ik5d.jpg"),
    ("105", "日本公信榜", "https://y.gtimg.cn/music/photo_new/T003R300x300M000003EpI9F1a0tFY.jpg"),
    ("51", "巅峰榜·明日之子", "https://y.gtimg.cn/music/common/upload/iphone_order_channel/toplist_51_300_203451421.jpg"),
    ("52", "巅峰榜·腾讯音乐人原创榜", "https://y.gtimg.cn/music/photo_new/T003R300x300M000000UTVyf0BnyCn.jpg"),
    ("53", "机车", "https://y.gtimg.cn/music/photo_new/T003R300x300M000002nNMsB2S520Y.jpg"),
];

/// 获取 QQ 音乐排行榜列表
/// 策略 1: CGI page=index
/// 策略 2: musicu 带认证 (ct:19, 完整 comm)
/// 策略 3: musicu 不带认证 (ct:24, 完整 comm)
/// 策略 4: 预设列表
pub async fn get_rank_list(cookie: &str) -> Result<Vec<Playlist>, String> {
    let mut playlists = Vec::new();

    // 策略 1: CGI 端点 page=index
    {
        let headers = vec![
            ("Referer", QQ_HEADERS_REFERER),
            ("User-Agent", QQ_HEADERS_UA),
        ];
        if !cookie.is_empty() {
            // CGI 也尝试带 cookie
        }
        let url = format!(
            "https://c.y.qq.com/v8/fcg-bin/fcg_v8_toplist_cp.fcg?\
            g_tk=5381&loginUin=0&hostUin=0&inCharset=utf8&outCharset=utf-8&\
            notice=0&platform=yqq&needNewCode=0&\
            type=top&page=index&format=json&tpl=3"
        );

        log::info!("[QQRank] fetching rank list from CGI (page=index)");
        match request_text(&url, "GET", &headers, None, 10000).await {
            Ok(text) => {
                log::debug!("[QQRank] CGI response preview: {}", &text[..text.len().min(500)]);
                match parse_json_text(&text) {
                    Ok(json) => {
                        let code = json.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
                        log::info!("[QQRank] CGI list code={}", code);
                        playlists = parse_toplist_list_from_json(&json);
                        log::info!("[QQRank] CGI list parsed {} playlists", playlists.len());
                        if playlists.is_empty() {
                            let preview = serde_json::to_string(&json).unwrap_or_default();
                            log::warn!("[QQRank] CGI list empty, preview: {}", &preview[..preview.len().min(500)]);
                        }
                    }
                    Err(e) => log::warn!("[QQRank] CGI list parse failed: {e}"),
                }
            }
            Err(e) => log::warn!("[QQRank] CGI list request failed: {e}"),
        }
    }

    // 策略 2: musicu 带认证 (ct:19, 完整 comm)
    if playlists.is_empty() && !cookie.is_empty() {
        let auth = extract_qq_auth(cookie);
        if auth.logged_in && !auth.music_key.is_empty() {
            log::info!("[QQRank] Trying musicu GetAllTop with auth");
            let payload = json!({
                "comm": {
                    "uin": auth.uin,
                    "format": "json",
                    "ct": 19,
                    "cv": 0,
                    "authst": auth.music_key
                },
                "req_0": {
                    "module": "musicToplist.ToplistInfoServer",
                    "method": "GetAllTop",
                    "param": {}
                }
            });
            playlists = try_musicu_get_all_top(&payload, cookie).await;
        }
    }

    // 策略 3: musicu 不带认证 (ct:24, 完整 comm)
    if playlists.is_empty() {
        log::info!("[QQRank] Trying musicu GetAllTop without auth");
        let payload = json!({
            "comm": { "uin": "0", "format": "json", "ct": 24, "cv": 0 },
            "req_0": {
                "module": "musicToplist.ToplistInfoServer",
                "method": "GetAllTop",
                "param": {}
            }
        });
        playlists = try_musicu_get_all_top(&payload, "").await;
    }

    // 策略 4: 预设列表
    if playlists.is_empty() {
        log::info!("[QQRank] All APIs failed, using preset list");
        return Ok(QQ_RANK_PRESETS.iter().map(|(id, name, cover)| Playlist {
            provider: "qqmusic".into(),
            id: id.to_string(),
            name: name.to_string(),
            cover: cover.to_string(),
            track_count: 0,
            creator: "QQ 音乐".into(),
            subscribed: false,
        }).collect());
    }

    Ok(playlists)
}

/// QQ 音乐推荐歌单 (歌单广场-推荐分类)
/// 接口: fcg_get_diss_by_tag categoryId=10000000 (推荐)
pub async fn recommend_playlists() -> Result<Vec<Playlist>, String> {
    let url = format!(
        "https://c.y.qq.com/splcloud/fcgi-bin/fcg_get_diss_by_tag.fcg?\
        g_tk=5381&loginUin=0&hostUin=0&inCharset=utf8&outCharset=utf-8&\
        notice=0&platform=yqq&needNewCode=0&\
        categoryId=10000000&sin=0&size=20&order=play&format=json"
    );
    let headers = vec![
        (REFERER.as_str(), QQ_HEADERS_REFERER),
        (USER_AGENT.as_str(), QQ_HEADERS_UA),
    ];
    let text = request_text(&url, "GET", &headers, None, 10000).await?;
    let json = parse_json_text(&text)?;
    let list = json.get("data")
        .and_then(|d| d.get("list"))
        .and_then(|l| l.as_array())
        .cloned()
        .unwrap_or_default();

    Ok(list
        .iter()
        .filter_map(|item| {
            let id = item.get("dissid")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| item.get("dissid").and_then(|v| v.as_i64()).map(|n| n.to_string()))
                .unwrap_or_default();
            if id.is_empty() {
                return None;
            }
            let creator = item.get("creator").map(|c| {
                if let Some(s) = c.as_str() {
                    s.to_string()
                } else {
                    c.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string()
                }
            }).unwrap_or_default();
            Some(Playlist {
                provider: "qqmusic".into(),
                id,
                name: item.get("dissname").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                cover: item.get("imgurl").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                track_count: item.get("songnum")
                    .and_then(|v| v.as_u64().or_else(|| v.as_i64().map(|n| n as u64)))
                    .map(|n| n as u32)
                    .unwrap_or(0),
                creator,
                subscribed: false,
            })
        })
        .collect())
}

/// 辅助: musicu GetAllTop
async fn try_musicu_get_all_top(payload: &Value, cookie: &str) -> Vec<Playlist> {
    match qq_musicu_request(payload, cookie, 10000).await {
        Ok(json) => {
            let req0 = json.get("req_0").unwrap_or(&Value::Null);
            let data = req0.get("data").unwrap_or(&Value::Null);
            let code = req0.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
            log::info!("[QQRank] musicu GetAllTop code={}", code);
            if code == 0 {
                let playlists = parse_toplist_list_from_json(data);
                log::info!("[QQRank] musicu GetAllTop parsed {} playlists", playlists.len());
                return playlists;
            }
        }
        Err(e) => log::warn!("[QQRank] musicu GetAllTop failed: {e}"),
    }
    Vec::new()
}

/// 获取排行榜歌曲
/// 策略 1: CGI page=detail&topid=X
/// 策略 2: musicu 带认证 (ct:19)
/// 策略 3: musicu 不带认证 (ct:24)
pub async fn get_rank_songs(cookie: &str, rank_id: &str, limit: u32) -> Result<Vec<Song>, String> {
    // 不设上限：接口返回多少就取多少
    let limit = limit.max(1) as usize;
    let mut songs = Vec::new();

    // 策略 1: CGI 端点 page=detail&topid=X
    {
        let headers = vec![
            ("Referer", QQ_HEADERS_REFERER),
            ("User-Agent", QQ_HEADERS_UA),
        ];
        let url = format!(
            "https://c.y.qq.com/v8/fcg-bin/fcg_v8_toplist_cp.fcg?\
            g_tk=5381&loginUin=0&hostUin=0&inCharset=utf8&outCharset=utf-8&\
            notice=0&platform=yqq&needNewCode=0&\
            type=top&topid={}&format=json&tpl=3&page=detail",
            rank_id
        );

        log::info!("[QQRank] fetching songs from CGI, rank_id={}", rank_id);
        match request_text(&url, "GET", &headers, None, 10000).await {
            Ok(text) => {
                match parse_json_text(&text) {
                    Ok(json) => {
                        let code = json.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
                        log::info!("[QQRank] CGI songs code={}, rank_id={}", code, rank_id);
                        songs = parse_toplist_songs_from_json(&json, limit);
                        log::info!("[QQRank] CGI songs parsed {} songs", songs.len());
                        if songs.is_empty() {
                            let preview = serde_json::to_string(json.get("data").unwrap_or(&Value::Null)).unwrap_or_default();
                            log::warn!("[QQRank] CGI songs empty, data: {}", &preview[..preview.len().min(500)]);
                        }
                    }
                    Err(e) => log::warn!("[QQRank] CGI songs parse failed: {e}"),
                }
            }
            Err(e) => log::warn!("[QQRank] CGI songs request failed: {e}"),
        }
    }

    // 策略 2: musicu 带认证 (ct:19)
    if songs.is_empty() && !cookie.is_empty() {
        let auth = extract_qq_auth(cookie);
        if auth.logged_in && !auth.music_key.is_empty() {
            log::info!("[QQRank] Trying musicu GetDetail with auth, rank_id={}", rank_id);
            let top_id: i64 = rank_id.parse().unwrap_or(0);
            let payload = json!({
                "comm": {
                    "uin": auth.uin,
                    "format": "json",
                    "ct": 19,
                    "cv": 0,
                    "authst": auth.music_key
                },
                "req_0": {
                    "module": "musicToplist.ToplistInfoServer",
                    "method": "GetDetail",
                    "param": { "topId": top_id, "offset": 0, "num": limit }
                }
            });
            songs = try_musicu_get_detail(&payload, cookie, limit).await;
        }
    }

    // 策略 3: musicu 不带认证 (ct:24)
    if songs.is_empty() {
        log::info!("[QQRank] Trying musicu GetDetail without auth, rank_id={}", rank_id);
        let top_id: i64 = rank_id.parse().unwrap_or(0);
        let payload = json!({
            "comm": { "uin": "0", "format": "json", "ct": 24, "cv": 0 },
            "req_0": {
                "module": "musicToplist.ToplistInfoServer",
                "method": "GetDetail",
                "param": { "topId": top_id, "offset": 0, "num": limit }
            }
        });
        songs = try_musicu_get_detail(&payload, "", limit).await;
    }

    Ok(songs)
}

/// 辅助: musicu GetDetail
async fn try_musicu_get_detail(payload: &Value, cookie: &str, limit: usize) -> Vec<Song> {
    match qq_musicu_request(payload, cookie, 10000).await {
        Ok(json) => {
            let req0 = json.get("req_0").unwrap_or(&Value::Null);
            let data = req0.get("data").unwrap_or(&Value::Null);
            let code = req0.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
            log::info!("[QQRank] musicu GetDetail code={}", code);
            if code == 0 {
                // musicu 返回格式: data.data.songInfoList[]
                let song_list = data.get("data")
                    .and_then(|d| d.get("songInfoList"))
                    .and_then(|s| s.as_array())
                    .or_else(|| {
                        data.get("data")
                            .and_then(|d| d.get("song"))
                            .and_then(|s| s.get("songInfoList"))
                            .and_then(|s| s.as_array())
                    })
                    .or_else(|| {
                        data.get("data")
                            .and_then(|d| d.get("song"))
                            .and_then(|s| s.as_array())
                    })
                    .or_else(|| {
                        data.get("songInfoList")
                            .and_then(|s| s.as_array())
                    });

                if let Some(arr) = song_list {
                    let songs: Vec<Song> = arr.iter().take(limit).map(|item| {
                        let track = item.get("songInfo")
                            .or_else(|| item.get("song"))
                            .unwrap_or(item);
                        map_qq_track(track, &Song::default())
                    }).filter(|s| !s.name.is_empty() && (s.mid.is_some() || !s.id.is_empty()))
                    .collect();
                    log::info!("[QQRank] musicu GetDetail parsed {} songs", songs.len());
                    return songs;
                }
            }
        }
        Err(e) => log::warn!("[QQRank] musicu GetDetail failed: {e}"),
    }
    Vec::new()
}

// ============================================================
//  歌手 / 歌单搜索 (对照 handleQQArtistDetail / handleQQSearch)
// ============================================================

/// 歌手搜索
pub async fn artist_search(keywords: &str, limit: u32, _cookie: &str) -> Result<Vec<Artist>, String> {
    let kw = keywords.trim();
    if kw.is_empty() {
        return Ok(vec![]);
    }
    let limit = limit.clamp(1, 20);

    // 使用 smartbox 搜索歌手
    let params: Vec<(&str, &str)> = vec![
        ("format", "json"),
        ("key", kw),
        ("g_tk", "5381"),
        ("loginUin", "0"),
        ("hostUin", "0"),
        ("inCharset", "utf8"),
        ("outCharset", "utf-8"),
        ("notice", "0"),
        ("platform", "yqq.json"),
        ("needNewCode", "0"),
    ];
    let full_url = format!("{}?{}", QQ_SMARTBOX_URL,
        params.iter().map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v))).collect::<Vec<_>>().join("&"));
    let headers = vec![
        (REFERER.as_str(), QQ_HEADERS_REFERER),
        (USER_AGENT.as_str(), QQ_HEADERS_UA),
    ];
    let text = request_text(&full_url, "GET", &headers, None, 10000).await?;
    let json = parse_json_text(&text)?;

    let items = json.get("data")
        .and_then(|d| d.get("singer"))
        .and_then(|s| s.get("itemlist"))
        .and_then(|l| l.as_array());

    let artists = items.map(|arr| {
        arr.iter().take(limit as usize).map(|item| {
            let mid = item.get("mid")
                .or_else(|| item.get("singerMid"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = item.get("name")
                .or_else(|| item.get("singerName"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Artist {
                id: None,
                mid: if !mid.is_empty() { Some(mid.clone()) } else { None },
                name,
                pic_url: Some(qq_singer_avatar(&item.get("mid").and_then(|v| v.as_str()).unwrap_or(""), 300)),
                music_size: None,
            }
        }).filter(|a| !a.name.is_empty())
        .collect()
    }).unwrap_or_default();

    Ok(artists)
}

/// 歌手歌曲 (对照 handleQQArtistDetail)
pub async fn artist_songs(artist_mid: &str, limit: u32, offset: u32, cookie: &str) -> Result<Vec<Song>, String> {
    let singer_mid = artist_mid.trim();
    if singer_mid.is_empty() {
        return Ok(vec![]);
    }
    let num = limit.clamp(10, 80);

    let payload = json!({
        "comm": { "ct": 24, "cv": 0 },
        "singer": {
            "module": "music.web_singer_info_svr",
            "method": "get_singer_detail_info",
            "param": {
                "sort": 5,
                "singermid": singer_mid,
                "sin": offset,
                "num": num
            }
        }
    });

    match qq_musicu_request(&payload, cookie, 10000).await {
        Ok(json) => {
            let block = json.get("singer").unwrap_or(&Value::Null);
            let data = block.get("data").unwrap_or(&Value::Null);
            let raw_songs = data.get("songlist")
                .and_then(|s| s.as_array())
                .cloned()
                .unwrap_or_default();

            let songs: Vec<Song> = raw_songs.iter()
                .map(|raw| {
                    let track = raw.get("track_info")
                        .or_else(|| raw.get("songInfo"))
                        .or_else(|| raw.get("songinfo"))
                        .or_else(|| raw.get("song"))
                        .unwrap_or(raw);
                    map_qq_track(track, &Song::default())
                })
                .filter(|s| !s.name.is_empty() && (s.mid.is_some() || !s.id.is_empty()))
                .collect();

            Ok(songs)
        }
        Err(e) => {
            log::warn!("[QQArtist] songs failed: {e}");
            Ok(vec![])
        }
    }
}

/// 歌单搜索
/// 优先使用与单曲搜索相同的通道 (DoSearchForQQMusicMobile + 搜索签名, search_type=1004 为歌单),
/// 无结果时回退旧版 musicu UniformSearch
pub async fn playlist_search(keywords: &str, limit: u32, cookie: &str) -> Result<Vec<Playlist>, String> {
    let kw = keywords.trim();
    if kw.is_empty() {
        return Ok(vec![]);
    }
    let limit = limit.clamp(1, 30);

    let mut playlists: Vec<Playlist> = Vec::new();

    // 主路径: DoSearchForQQMusicMobile (search_type=1004 歌单)
    let payload = json!({
        "comm": {
            "ct": "11",
            "cv": "14090508",
            "v": "14090508",
            "tmeAppID": "qqmusic",
            "phonetype": "EBG-AN10",
            "os_ver": "12",
            "OpenUDID": "0",
            "QIMEI36": "0",
            "udid": "0",
            "chid": "0",
            "aid": "0",
            "oaid": "0",
            "taid": "0",
            "tid": "0",
            "wid": "0",
            "uid": "0",
            "sid": "0",
            "modeSwitch": "6",
            "teenMode": "0",
            "ui_mode": "2",
            "nettype": "1020"
        },
        "req": {
            "module": "music.search.SearchCgiService",
            "method": "DoSearchForQQMusicMobile",
            "param": {
                "search_type": 3,
                "searchid": format!("{}{:06}", chrono::Utc::now().timestamp_millis(), rand::random::<u32>() % 1000000),
                "query": kw,
                "page_num": 1,
                "num_per_page": limit,
                "highlight": 0,
                "nqc_flag": 0,
                "multi_zhida": 0,
                "cat": 2,
                "grp": 1,
                "sin": 0,
                "sem": 0
            }
        }
    });

    let body_text = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
    let sign = qq_search_sign(&body_text);
    let url = format!("https://u.y.qq.com/cgi-bin/musics.fcg?sign={}", sign);
    let headers = vec![
        ("User-Agent", QQ_SEARCH_UA),
        ("Content-Type", "application/json"),
    ];

    if let Ok(text) = request_text(&url, "POST", &headers, Some(&body_text), 10000).await {
        if let Ok(json) = parse_json_text(&text) {
            let data = json.get("req")
                .and_then(|r| r.get("data"))
                .unwrap_or(&Value::Null);
            let body = data.get("body").unwrap_or(data);
            let items = body.get("item_songlist")
                .or_else(|| body.get("playlist"))
                .or_else(|| body.get("songlist"))
                .and_then(|l| l.as_array());

            if let Some(arr) = items {
                playlists = arr.iter().filter_map(|item| {
                    let id = item.get("dissid")
                        .or_else(|| item.get("tid"))
                        .or_else(|| item.get("id"))
                        .and_then(|v| match v {
                            Value::String(s) => Some(s.clone()),
                            Value::Number(n) => Some(n.to_string()),
                            _ => None,
                        })
                        .unwrap_or_default();
                    if id.is_empty() {
                        return None;
                    }
                    let name = strip_html_tags(
                        &item.get("dissname")
                            .or_else(|| item.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                    );
                    if name.is_empty() {
                        return None;
                    }
                    let creator = item.get("nickname")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .or_else(|| {
                            item.get("creator").map(|c| {
                                if let Some(s) = c.as_str() {
                                    s.to_string()
                                } else {
                                    c.get("name").or_else(|| c.get("nick"))
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("QQ 音乐")
                                        .to_string()
                                }
                            })
                        })
                        .unwrap_or_else(|| "QQ 音乐".to_string());
                    Some(Playlist {
                        provider: "qqmusic".into(),
                        id,
                        name,
                        cover: item.get("pic_url")
                            .or_else(|| item.get("picurl"))
                            .or_else(|| item.get("logo"))
                            .or_else(|| item.get("imgurl"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        track_count: item.get("songnum")
                            .or_else(|| item.get("song_count"))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as u32,
                        creator,
                        subscribed: false,
                    })
                }).collect();
            }
        }
    }

    // 兜底: 主路径无结果时回退旧版 musicu UniformSearch
    if playlists.is_empty() {
        let payload = json!({
            "comm": { "ct": 19, "cv": 0 },
            "req_0": {
                "module": "music.musicz.UniformSearch",
                "method": "CgiUniformSearch",
                "param": {
                    "search_type": 4,
                    "query": kw,
                    "page_num": 1,
                    "num_per_page": limit,
                    "highlight": 0
                }
            }
        });
        if let Ok(json) = qq_musicu_request(&payload, cookie, 10000).await {
            let data = json.get("req_0")
                .and_then(|r| r.get("data"))
                .unwrap_or(&Value::Null);
            let body = data.get("body").unwrap_or(data);
            let items = body.get("playlist")
                .or_else(|| body.get("item"))
                .and_then(|l| l.as_array());
            if let Some(arr) = items {
                playlists = arr.iter().filter_map(|item| {
                    let id = item.get("dissid")
                        .or_else(|| item.get("id"))
                        .and_then(|v| match v {
                            Value::String(s) => Some(s.clone()),
                            Value::Number(n) => Some(n.to_string()),
                            _ => None,
                        })
                        .unwrap_or_default();
                    if id.is_empty() {
                        return None;
                    }
                    let name = strip_html_tags(
                        &item.get("dissname")
                            .or_else(|| item.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                    );
                    if name.is_empty() {
                        return None;
                    }
                    Some(Playlist {
                        provider: "qqmusic".into(),
                        id,
                        name,
                        cover: item.get("logo")
                            .or_else(|| item.get("picurl"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        track_count: item.get("song_count")
                            .or_else(|| item.get("songnum"))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as u32,
                        creator: item.get("creator")
                            .and_then(|c| c.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("QQ 音乐")
                            .to_string(),
                        subscribed: false,
                    })
                }).collect();
            }
        }
    }

    log::info!("[QQPlaylistSearch] query={} results={}", kw, playlists.len());
    Ok(playlists)
}

// ============================================================
//  喜欢功能 (对照 handleQQLikedPlaylistTracks)
// ============================================================

/// 获取喜欢的歌曲 mid 列表
pub async fn liked_hashes(cookie: &str) -> Result<Vec<String>, String> {
    let auth = extract_qq_auth(cookie);
    if !auth.playback_ready {
        return Ok(vec![]);
    }

    let payload = json!({
        "comm": { "ct": 24, "cv": 0 },
        "req_0": {
            "module": "music.srfDissInfo.DissInfo",
            "method": "CgiGetDiss",
            "param": {
                "disstid": 0,
                "dirid": QQ_LIKED_DIRID,
                "tag": 1,
                "song_begin": 0,
                "song_num": 500,
                "userinfo": 1,
                "orderlist": 1
            }
        }
    });

    match qq_musicu_request(&payload, cookie, 10000).await {
        Ok(json) => {
            let block = json.get("req_0").unwrap_or(&Value::Null);
            let data = block.get("data").unwrap_or(&Value::Null);
            let raw_tracks = data.get("songlist")
                .and_then(|s| s.as_array())
                .cloned()
                .unwrap_or_default();
            let mids: Vec<String> = raw_tracks.iter()
                .filter_map(|raw| {
                    let track = raw.get("track_info")
                        .or_else(|| raw.get("songInfo"))
                        .or_else(|| raw.get("songinfo"))
                        .or_else(|| raw.get("song"))
                        .unwrap_or(raw);
                    track.get("mid")
                        .or_else(|| track.get("songmid"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .collect();
            Ok(mids)
        }
        Err(e) => {
            log::warn!("[QQLiked] hashes failed: {e}");
            Ok(vec![])
        }
    }
}

/// 喜欢/取消喜欢歌曲
pub async fn like_toggle(song: &Song, like: bool, cookie: &str) -> Result<bool, String> {
    let auth = extract_qq_auth(cookie);
    log::info!("[QQLike] toggle called: id={}, mid={:?}, like={}, playback_ready={}",
        song.id, song.mid, like, auth.playback_ready);
    
    if !auth.playback_ready {
        log::warn!("[QQLike] playback not ready, rejecting");
        return Err("QQ 音乐需要完整授权才能操作喜欢".into());
    }

    let mid = song.mid.as_deref().unwrap_or("");
    let song_id = song.qq_song_id.unwrap_or(0);
    log::info!("[QQLike] mid='{}', song_id={}", mid, song_id);

    if mid.is_empty() && song_id == 0 {
        return Err("Missing QQ song mid or id".into());
    }

    let mut param = json!({
        "dirId": QQ_LIKED_DIRID,
    });
    if song_id > 0 {
        param["songId"] = json!(song_id);
    }
    if !mid.is_empty() {
        param["songmid"] = json!(mid);
    }

    // 收藏/取消收藏歌曲到「我喜欢」
    let method = if like { "CgiCollectSong" } else { "CgiRemoveCollectSong" };
    let payload = json!({
        "comm": {
            "uin": auth.uin,
            "format": "json",
            "ct": 19,
            "cv": 0,
            "authst": auth.music_key
        },
        "req_0": {
            "module": "music.musicInfoCollection",
            "method": method,
            "param": param
        }
    });

    match qq_musicu_request(&payload, cookie, 10000).await {
        Ok(json) => {
            let code = json.get("req_0")
                .and_then(|r| r.get("code"))
                .and_then(|v| v.as_i64())
                .unwrap_or(-1);
            log::info!("[QQLike] API response code={}, payload_preview={}",
                code, serde_json::to_string(&payload).unwrap_or_default());
            if code == 0 {
                Ok(true)
            } else {
                let msg = format!("QQ API returned code {}", code);
                log::warn!("[QQLike] {}", msg);
                Err(msg)
            }
        }
        Err(e) => {
            log::warn!("[QQLike] toggle failed: {e}");
            Err(e)
        }
    }
}
