#![allow(dead_code)]

//! 酷狗音乐 API — 完全移植自 Mineradio kugou-api.js
//!
//! 所有签名算法、API 端点、请求参数、降级策略均与 kugou-api.js 完全一致。

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use reqwest::header::{COOKIE, REFERER, USER_AGENT};
use serde_json::{json, Value};

use super::models::*;

// ============================================================
//  常量 (对照 kugou-api.js)
// ============================================================

const KUGOU_SEARCH_URL: &str = "http://songsearch.kugou.com/song_search_v2";
const KUGOU_PLAY_MOBILE: &str = "http://m.kugou.com/app/i/getSongInfo.php";
const KUGOU_PLAY_WEB: &str = "https://wwwapi.kugou.com/yy/index.php";
const KUGOU_LYRIC_SEARCH: &str = "https://krcs.kugou.com/search";
const KUGOU_LYRIC_DOWNLOAD: &str = "https://krcs.kugou.com/download";
const KUGOU_GATEWAY: &str = "https://gateway.kugou.com";

const KUGOU_APPID: u64 = 1005;
const KUGOU_WEB_APPID: u64 = 1014;
const KUGOU_CLIENTVER: u64 = 20489;
const KUGOU_ANDROID_SALT: &str = "OIlwieks28dk2k092lksi2UIkp";
const KUGOU_H5_SALT: &str = "NVPh5oo715z5DIWAeQlhMDsWXXQV4hwt";
const KUGOU_H5_SRC_APPID: &str = "2919";
const KUGOU_H5_CLIENTVER: u64 = 20000;
const KUGOU_SIGN_KEY_SALT: &str = "57ae12eb6890223e355ccfcb74edf70d";
const KUGOU_GATEWAY_UA: &str = "Android15-1070-11083-46-0-DiscoveryDRADProtocol-wifi";
const KUGOU_H5_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
const KUGOU_REFERER: &str = "https://www.kugou.com/";
const KUGOU_VIP_ROLEINFO_URL: &str = "https://vip.kugou.com/recharge/roleinfo";

// Web roleinfo 角色映射 (对照 Minerradio KUGOU_WEB_VIP_ROLES / SVIP / MUSIC_PACKAGE)
const KUGOU_WEB_VIP_ROLES: &[i64] = &[1, 2];
const KUGOU_WEB_SVIP_ROLES: &[i64] = &[6, 11, 13];
const KUGOU_WEB_MUSIC_PACKAGE_ROLES: &[i64] = &[31, 33];

// Web roleinfo 字段候选名称
const KUGOU_WEB_ROLE_KEYS: &[&str] = &[
    "role", "user_type", "userType", "usertype",
    "user_y_type", "userYType", "userytype", "y_type", "yType", "ytype",
];

const KUGOU_QUALITY_CHAIN: &[(&str, &str, &str)] = &[
    ("jymaster", "Hi-Res", "ResFileHash"),
    ("hires", "Hi-Res", "ResFileHash"),
    ("lossless", "无损", "SQFileHash"),
    ("exhigh", "极高", "HQFileHash"),
    ("standard", "标准", "FileHash"),
];

// ============================================================
//  MD5 工具
// ============================================================

fn md5_hex(input: &str) -> String {
    use md5::{Digest, Md5};
    let mut hasher = Md5::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ============================================================
//  Cookie 解析 (对照 kugou-api.js)
// ============================================================

/// 解析 cookie 字符串为 HashMap
pub fn parse_cookie_string(cookie: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for part in cookie.split(';') {
        let part = part.trim();
        if let Some(eq) = part.find('=') {
            let key = part[..eq].trim().to_string();
            let value = part[eq + 1..].trim().to_string();
            if !key.is_empty() {
                map.insert(key, value);
            }
        }
    }
    map
}

/// 解析 KuGoo 复合字段 (URL 编码的 key=value&key=value)
pub fn parse_kugoo_compound(raw: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut text = raw.trim().to_string();
    if text.is_empty() {
        return out;
    }
    // 尝试 URL 解码
    if let Ok(decoded) = urlencoding::decode(&text) {
        text = decoded.to_string();
    }
    for part in text.split('&') {
        if let Some(eq) = part.find('=') {
            let key = part[..eq].trim().to_string();
            let value = part[eq + 1..].trim().to_string();
            if !key.is_empty() {
                out.insert(key, value);
            }
        }
    }
    out
}

/// 解码酷狗显示文本 (对照 decodeKugouDisplayText)
/// 处理 %uXXXX (非标准 Unicode 编码) 和标准 URL 编码
fn decode_display_text(text: &str) -> String {
    let raw = text.trim();
    if raw.is_empty() {
        return String::new();
    }
    let mut result = raw.to_string();

    // 1. 处理 %uXXXX 编码 (酷狗 KuGoo cookie 中常用)
    if result.contains("%u") {
        let mut decoded = String::with_capacity(result.len());
        let mut chars = result.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '%' && chars.peek() == Some(&'u') {
                chars.next(); // consume 'u'
                let hex: String = chars.by_ref().take(4).collect();
                if let Ok(code) = u32::from_str_radix(&hex, 16) {
                    if let Some(c) = char::from_u32(code) {
                        decoded.push(c);
                        continue;
                    }
                }
                // 解码失败, 保留原始
                decoded.push('%');
                decoded.push('u');
                decoded.push_str(&hex);
            } else {
                decoded.push(ch);
            }
        }
        result = decoded;
    }

    // 2. 标准 URL 解码 (仅当没有 CJK 字符时, 避免破坏已解码的内容)
    let has_cjk = result.chars().any(|c| ('\u{3400}'..='\u{9fff}').contains(&c));
    if !has_cjk && result.contains('%') {
        if let Ok(decoded) = urlencoding::decode(&result.replace('+', " ")) {
            result = decoded.to_string();
        }
    }

    result.trim().to_string()
}

/// 去除 HTML 标签
fn strip_html(text: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;
    for ch in text.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            result.push(ch);
        }
    }
    decode_display_text(&result.trim())
}

/// 酷狗认证信息 (对照 extractKugouAuth)
#[derive(Debug, Clone, Default)]
pub struct KugouAuth {
    pub userid: String,
    pub token: String,
    pub mid: String,
    pub dfid: String,
    pub nickname: String,
    pub avatar: String,
    pub vip_type: i32,
    pub svip_type: i32,
    pub vip_level: String,
    pub is_vip: bool,
    pub is_svip: bool,
    pub logged_in: bool,
    pub playback_ready: bool,
}

/// 创建酷狗 mid (MD5) — 对照 createKugouMid
fn create_kugou_mid(seed: &str) -> String {
    let raw = format!("{}{}", seed, rand::random::<u64>());
    md5_hex(&raw)
}

/// 从 Cookie 提取认证信息 (对照 extractKugouAuth)
pub fn extract_kugou_auth(cookie: &str) -> KugouAuth {
    let obj = parse_cookie_string(cookie);
    let kugoo = parse_kugoo_compound(
        obj.get("KuGoo")
            .or_else(|| obj.get("kugou"))
            .or_else(|| obj.get("Kugou"))
            .map(|s| s.as_str())
            .unwrap_or(""),
    );

    let userid = obj
        .get("userid")
        .or_else(|| obj.get("UserId"))
        .or_else(|| obj.get("KugooID"))
        .or_else(|| obj.get("kugouID"))
        .or_else(|| kugoo.get("KugooID"))
        .or_else(|| kugoo.get("kugouID"))
        .or_else(|| kugoo.get("userid"))
        .or_else(|| kugoo.get("uid"))
        .map(|s| s.chars().filter(|c| c.is_ascii_digit()).collect::<String>())
        .unwrap_or_default();

    let token = obj
        .get("token")
        .or_else(|| obj.get("Token"))
        .or_else(|| obj.get("t"))
        .or_else(|| obj.get("T"))
        .or_else(|| kugoo.get("t"))
        .or_else(|| kugoo.get("token"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let mid = obj
        .get("kg_mid")
        .or_else(|| obj.get("KG_MID"))
        .or_else(|| obj.get("KUGOU_API_MID"))
        .or_else(|| obj.get("mid"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| create_kugou_mid("nexbox"));

    let dfid = obj
        .get("kg_dfid")
        .or_else(|| obj.get("KG_DFID"))
        .or_else(|| obj.get("dfid"))
        .or_else(|| obj.get("DFID"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "-".to_string());

    let nickname = kugoo
        .get("NickName")
        .or_else(|| kugoo.get("nickname"))
        .or_else(|| obj.get("NickName"))
        .or_else(|| obj.get("nickname"))
        .or_else(|| obj.get("UserName"))
        .or_else(|| obj.get("username"))
        .map(|s| decode_display_text(s))
        .unwrap_or_default();

    let avatar = kugoo
        .get("Pic")
        .or_else(|| kugoo.get("pic"))
        .or_else(|| obj.get("Pic"))
        .or_else(|| obj.get("avatar"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let logged_in =
        (!userid.is_empty() && userid != "0") || obj.contains_key("KuGoo") || obj.contains_key("kugou") || obj.contains_key("Kugou");
    let playback_ready = !userid.is_empty() && userid != "0" && !token.is_empty();

    // 从 Cookie 提取 VIP 信息 (对照 extractKugouAuth 中的 firstPositiveKugouNumber)
    let vip_type = first_positive_kugou_cookie(&[&obj, &kugoo], &[
        "isVIP", "isVip", "is_vip", "vip_type", "VIPType", "vipLevel", "vip_level",
        "vip_status", "member_type", "member_level", "vip",
    ]);
    let svip_type = first_positive_kugou_cookie(&[&obj, &kugoo], &[
        "isSVIP", "isSvip", "is_svip", "svip_type", "SVIPType", "superVip", "super_vip",
        "svip_level", "svip_status", "svip",
    ]);
    let is_svip = svip_type > 0.0;
    let is_vip = is_svip || vip_type > 0.0;
    let vip_level = if is_svip { "svip" } else if is_vip { "vip" } else { "none" };

    KugouAuth {
        userid,
        token,
        mid,
        dfid,
        nickname,
        avatar,
        vip_type: vip_type as i32,
        svip_type: svip_type as i32,
        vip_level: vip_level.to_string(),
        is_vip,
        is_svip,
        logged_in,
        playback_ready,
    }
}

/// 检查酷狗 Cookie 是否有登录态
pub fn kugou_cookie_has_login(cookie: &str) -> bool {
    extract_kugou_auth(cookie).logged_in
}

/// 检查酷狗 Cookie 是否有播放权限
pub fn kugou_cookie_has_playback(cookie: &str) -> bool {
    extract_kugou_auth(cookie).playback_ready
}

/// 从 Cookie HashMap 列表中查找第一个正数 (对照 firstPositiveKugouNumber, 用于 cookie 对象)
fn first_positive_kugou_cookie(objects: &[&HashMap<String, String>], keys: &[&str]) -> f64 {
    for obj in objects {
        for key in keys {
            if let Some(raw) = obj.get(*key) {
                if let Ok(n) = raw.parse::<f64>() {
                    if n > 0.0 && n.is_finite() {
                        return n;
                    }
                }
                let lower = raw.trim().to_lowercase();
                if matches!(lower.as_str(), "true" | "yes" | "active" | "valid" | "enabled" | "vip" | "svip" | "premium" | "member") {
                    return 1.0;
                }
            }
        }
    }
    0.0
}

// ============================================================
//  签名与请求构建 (对照 kugou-api.js)
// ============================================================

/// kugou_cloud_key: MD5(hash + "kgcloud")
fn kugou_cloud_key(hash: &str) -> String {
    md5_hex(&format!("{}kgcloud", hash))
}

/// sign_key: MD5(hash + SIGN_KEY_SALT + appid + mid + userid)
fn sign_key(hash: &str, mid: &str, userid: &str, appid: u64) -> String {
    md5_hex(&format!("{}{}{}{}{}", hash, KUGOU_SIGN_KEY_SALT, appid, mid, userid))
}

/// Android 签名: MD5(SALT + 排序参数 + body + SALT)
fn signature_android(params: &[(String, String)], body: &str) -> String {
    let mut sorted = params.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let params_str: String = sorted
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("");
    md5_hex(&format!("{}{}{}{}", KUGOU_ANDROID_SALT, params_str, body, KUGOU_ANDROID_SALT))
}

/// H5 签名: MD5(SALT + 排序参数 + JSON(body) + SALT)
fn signature_h5(params: &[(String, String)], body: Option<&Value>) -> String {
    let mut parts: Vec<String> = {
        let mut sorted = params.to_vec();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        sorted.iter().map(|(k, v)| format!("{}={}", k, v)).collect()
    };
    if let Some(body) = body {
        if !body.is_null() {
            parts.push(body.to_string());
        }
    }
    md5_hex(&format!("{}{}{}", KUGOU_H5_SALT, parts.join(""), KUGOU_H5_SALT))
}

/// 构建 H5 请求参数
/// 注意: extra 参数会覆盖默认参数 (对齐 JS Object.assign 语义)
fn build_h5_params(auth: &KugouAuth, extra: Vec<(String, String)>) -> Vec<(String, String)> {
    let now = now_ms();
    let mut params: Vec<(String, String)> = vec![
        ("srcappid".into(), KUGOU_H5_SRC_APPID.into()),
        ("clientver".into(), KUGOU_H5_CLIENTVER.to_string()),
        ("clienttime".into(), now.to_string()),
        ("mid".into(), if auth.mid.is_empty() { create_kugou_mid("gateway") } else { auth.mid.clone() }),
        ("uuid".into(), now.to_string()),
        ("dfid".into(), if auth.dfid.is_empty() { "-".into() } else { auth.dfid.clone() }),
        ("appid".into(), KUGOU_WEB_APPID.to_string()),
        ("token".into(), auth.token.clone()),
        ("userid".into(), if auth.userid.is_empty() { "0".into() } else { auth.userid.clone() }),
    ];
    // 对齐 JS Object.assign: 同名 key 覆盖而非追加, 避免签名不匹配
    for (k, v) in extra {
        if let Some(existing) = params.iter_mut().find(|(key, _)| key == &k) {
            existing.1 = v;
        } else {
            params.push((k, v));
        }
    }
    params
}

/// 构建 Android Gateway 请求参数
/// 注意: extra 参数会覆盖默认参数 (对齐 JS Object.assign 语义)
fn build_gateway_params(auth: &KugouAuth, extra: Vec<(String, String)>) -> Vec<(String, String)> {
    let clienttime = now_secs();
    let mut params: Vec<(String, String)> = vec![
        ("dfid".into(), if auth.dfid.is_empty() { "-".into() } else { auth.dfid.clone() }),
        ("mid".into(), if auth.mid.is_empty() { create_kugou_mid("gateway") } else { auth.mid.clone() }),
        ("uuid".into(), "-".into()),
        ("appid".into(), KUGOU_APPID.to_string()),
        ("clientver".into(), KUGOU_CLIENTVER.to_string()),
        ("clienttime".into(), clienttime.to_string()),
        ("token".into(), auth.token.clone()),
        ("userid".into(), if auth.userid.is_empty() { "0".into() } else { auth.userid.clone() }),
    ];
    // 对齐 JS Object.assign: 同名 key 覆盖而非追加, 避免签名不匹配
    for (k, v) in extra {
        if let Some(existing) = params.iter_mut().find(|(key, _)| key == &k) {
            existing.1 = v;
        } else {
            params.push((k, v));
        }
    }
    params
}

/// 构建 Cookie 请求头 (补充 mid/dfid)
pub fn build_request_cookie(cookie: &str) -> String {
    let obj = parse_cookie_string(cookie);
    let mid = obj
        .get("kg_mid")
        .or_else(|| obj.get("KG_MID"))
        .cloned()
        .unwrap_or_else(|| create_kugou_mid("nexbox"));
    let dfid = obj
        .get("kg_dfid")
        .or_else(|| obj.get("KG_DFID"))
        .cloned()
        .unwrap_or_else(|| "-".to_string());

    let mut parts: Vec<String> = Vec::new();
    if !cookie.trim().is_empty() {
        parts.push(cookie.trim().to_string());
    }
    if !obj.contains_key("kg_mid") && !obj.contains_key("KG_MID") {
        parts.push(format!("kg_mid={}", mid));
    }
    if !obj.contains_key("kg_dfid") && !obj.contains_key("KG_DFID") {
        parts.push(format!("kg_dfid={}", dfid));
    }

    // 去重合并
    let mut merged: HashMap<String, String> = HashMap::new();
    for part in parts.join("; ").split(';') {
        if let Some(eq) = part.find('=') {
            let key = part[..eq].trim().to_string();
            let value = part[eq + 1..].trim().to_string();
            if !key.is_empty() {
                merged.insert(key, value);
            }
        }
    }
    merged
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("; ")
}

// ============================================================
//  HTTP 请求 (对照 kugou-api.js)
// ============================================================

fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .expect("Failed to build HTTP client")
}

/// 发送请求并返回 JSON
async fn request_json(
    url: &str,
    method: reqwest::Method,
    headers: &[(&str, &str)],
    body: Option<&str>,
) -> Result<Value, String> {
    let client = build_client();
    let mut req = client
        .request(method, url);

    for (key, value) in headers {
        req = req.header(*key, *value);
    }

    if let Some(body) = body {
        req = req.body(body.to_string());
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {e}"))?;

    if !status.is_success() {
        return Err(format!("HTTP {}: {}", status.as_u16(), &text[..text.len().min(200)]));
    }

    serde_json::from_str(&text)
        .map_err(|e| format!("Failed to parse JSON: {e}, body: {}", &text[..text.len().min(200)]))
}

/// H5 Gateway 请求 (对照 kugouH5GatewayRequest)
async fn h5_gateway_request(
    path: &str,
    method: &str,
    cookie: &str,
    extra_params: Vec<(String, String)>,
    body: Option<Value>,
    router: Option<&str>,
) -> Result<Value, String> {
    let auth = extract_kugou_auth(cookie);
    if !auth.playback_ready {
        return Err("KUGOU_AUTH_REQUIRED".into());
    }

    let body_text = body
        .as_ref()
        .map(|b| b.to_string())
        .unwrap_or_default();

    let mut params = build_h5_params(&auth, extra_params);
    let sig = signature_h5(&params, body.as_ref());
    params.push(("signature".into(), sig));

    let base_url = KUGOU_GATEWAY;
    let url = format!("{}{}", base_url, path);

    let client = build_client();
    let mut req_builder = client.request(
        if method == "POST" { reqwest::Method::POST } else { reqwest::Method::GET },
        &url,
    );

    // 添加 query 参数
    for (k, v) in &params {
        req_builder = req_builder.query(&[(k.as_str(), v.as_str())]);
    }

    // Headers
    let cookie_header = build_request_cookie(cookie);
    req_builder = req_builder
        .header(USER_AGENT, KUGOU_H5_UA)
        .header(REFERER, KUGOU_REFERER)
        .header(COOKIE, &cookie_header);

    if let Some(r) = router {
        req_builder = req_builder.header("x-router", r);
    }

    // 对齐 Mineradio: H5 网关不设置 Content-Type
    if !body_text.is_empty() {
        req_builder = req_builder
            .body(body_text);
    }

    let resp = req_builder.send().await.map_err(|e| format!("H5 Gateway request failed: {e}"))?;
    let text = resp.text().await.map_err(|e| format!("Failed to read response: {e}"))?;
    let json: Value = serde_json::from_str(&text).map_err(|e| {
        log::warn!("[KugouH5] JSON parse failed, path={}, body preview: {}", path, &text[..text.len().min(300)]);
        format!("Failed to parse JSON: {e}")
    })?;

    // status === 0 表示错误 (对齐 JS: Number(json.status) === 0)
    // JS 的 Number() 能将 "0"(string), null, false 转为 0
    let status_is_zero = json.get("status").map(|v| {
        match v {
            Value::Number(n) => n.as_f64().map(|f| f == 0.0).unwrap_or(false),
            Value::String(s) => s.trim().parse::<f64>().map(|f| f == 0.0).unwrap_or(false),
            Value::Bool(b) => !b,
            Value::Null => true,
            _ => false,
        }
    }).unwrap_or(false);
    if status_is_zero {
        let msg = json
            .get("error")
            .or_else(|| json.get("msg"))
            .or_else(|| json.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("KUGOU_GATEWAY_FAILED");
        log::warn!("[KugouH5] {} status=0, err={}, response preview: {}", path, msg, &text[..text.len().min(300)]);
        return Err(msg.to_string());
    }

    Ok(json)
}

/// Android Gateway 请求 (对照 kugouGatewayRequest)
async fn gateway_request(
    path: &str,
    method: &str,
    cookie: &str,
    extra_params: Vec<(String, String)>,
    body: Option<Value>,
    router: Option<&str>,
    skip_signature: bool,
) -> Result<Value, String> {
    gateway_request_full(path, method, cookie, extra_params, body, router, skip_signature, None, None).await
}

/// Android Gateway 请求 (完整版, 支持自定义 Referer 和 base URL)
async fn gateway_request_full(
    path: &str,
    method: &str,
    cookie: &str,
    extra_params: Vec<(String, String)>,
    body: Option<Value>,
    router: Option<&str>,
    skip_signature: bool,
    custom_referer: Option<&str>,
    custom_base_url: Option<&str>,
) -> Result<Value, String> {
    let auth = extract_kugou_auth(cookie);
    if !auth.playback_ready {
        return Err("KUGOU_AUTH_REQUIRED".into());
    }

    let body_text = body
        .as_ref()
        .map(|b| b.to_string())
        .unwrap_or_default();

    let mut params = build_gateway_params(&auth, extra_params);
    if !skip_signature {
        let sig = signature_android(&params, &body_text);
        log::info!("[KugouGateway] {} body_text={}, sig={}",
            path, &body_text[..body_text.len().min(200)], &sig[..sig.len().min(16)]);
        params.push(("signature".into(), sig));
    }

    let base = custom_base_url.unwrap_or(KUGOU_GATEWAY);
    let url = format!("{}{}", base, path);

    let client = build_client();
    let mut req_builder = client.request(
        if method == "POST" { reqwest::Method::POST } else { reqwest::Method::GET },
        &url,
    );

    for (k, v) in &params {
        req_builder = req_builder.query(&[(k.as_str(), v.as_str())]);
    }

    let cookie_header = build_request_cookie(cookie);
    let clienttime = params.iter().find(|(k, _)| k == "clienttime").map(|(_, v)| v.clone()).unwrap_or_default();
    let referer = custom_referer.unwrap_or(KUGOU_REFERER);
    req_builder = req_builder
        .header(USER_AGENT, KUGOU_GATEWAY_UA)
        .header(REFERER, referer)
        .header(COOKIE, &cookie_header)
        .header("dfid", &auth.dfid)
        .header("mid", &auth.mid)
        .header("clienttime", &clienttime)
        .header("kg-rc", "1")
        .header("kg-thash", "5d816a0")
        .header("kg-rec", "1")
        .header("kg-rf", "B9EDA08A64250DEFFBCADDEE00F8F25F");

    if let Some(r) = router {
        req_builder = req_builder.header("x-router", r);
    }

    if !body_text.is_empty() {
        req_builder = req_builder
            .header("Content-Type", "application/json")
            .body(body_text);
    }

    let resp = req_builder.send().await.map_err(|e| format!("Gateway request failed: {e}"))?;
    let text = resp.text().await.map_err(|e| format!("Failed to read response: {e}"))?;
    let json: Value = serde_json::from_str(&text).map_err(|e| {
        log::warn!("[KugouGateway] JSON parse failed, path={}, body preview: {}", path, &text[..text.len().min(300)]);
        format!("Failed to parse JSON: {e}")
    })?;

    if json.get("status").and_then(|v| v.as_i64()).map(|n| n == 0).unwrap_or(false) {
        let msg = json
            .get("error")
            .or_else(|| json.get("msg"))
            .or_else(|| json.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("KUGOU_GATEWAY_FAILED");
        log::warn!("[KugouGateway] {} status=0, err={}, response preview: {}", path, msg, &text[..text.len().min(300)]);
        return Err(msg.to_string());
    }

    Ok(json)
}

// ============================================================
//  工具函数
// ============================================================

fn kugou_cover_url(raw: &str, size: u32) -> String {
    let url = raw.trim();
    if url.is_empty() {
        return String::new();
    }
    url.replace("{size}", &size.to_string())
}

fn value_as_string(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn first_string(item: &Value, keys: &[&str]) -> String {
    for key in keys {
        let text = value_as_string(item.get(*key));
        if !text.is_empty() {
            return text;
        }
    }
    String::new()
}

fn first_u64(item: &Value, keys: &[&str]) -> u64 {
    for key in keys {
        if let Some(value) = item.get(*key) {
            if let Some(n) = value.as_u64() {
                return n;
            }
            if let Some(n) = value.as_i64() {
                if n > 0 {
                    return n as u64;
                }
            }
            if let Some(s) = value.as_str() {
                if let Ok(n) = s.trim().parse::<u64>() {
                    return n;
                }
            }
        }
    }
    0
}

fn split_artist_title(raw: &str) -> (String, String) {
    let text = strip_html(raw);
    if let Some((artist, title)) = text.split_once(" - ") {
        let artist = artist.trim();
        let title = title.trim();
        if !artist.is_empty() && !title.is_empty() {
            return (artist.to_string(), title.to_string());
        }
    }
    (String::new(), text)
}

fn resolve_album_audio_id(params: &Value) -> String {
    let candidates = ["mixSongId", "mixsongid", "albumAudioId", "album_audio_id"];
    for key in &candidates {
        if let Some(val) = params.get(*key) {
            let text = match val {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                _ => continue,
            };
            if text.chars().all(|c| c.is_ascii_digit()) && !text.is_empty() {
                return text;
            }
        }
    }
    String::new()
}

fn pick_play_url(json: &Value) -> String {
    if json.is_null() {
        return String::new();
    }
    let pick = |val: &Value| -> Option<String> {
        match val {
            Value::Array(arr) => arr.iter().find(|v| !v.is_null()).and_then(|v| v.as_str()).map(|s| s.to_string()),
            Value::String(s) if !s.is_empty() => Some(s.clone()),
            _ => None,
        }
    };
    let data = json.get("data").unwrap_or(&Value::Null);
    let url = pick(json.get("url").unwrap_or(&Value::Null))
        .or_else(|| pick(json.get("play_url").unwrap_or(&Value::Null)))
        .or_else(|| pick(json.get("backupUrl").unwrap_or(&Value::Null)))
        .or_else(|| pick(data.get("url").unwrap_or(&Value::Null)))
        .or_else(|| pick(data.get("play_url").unwrap_or(&Value::Null)))
        .or_else(|| pick(data.get("backupUrl").unwrap_or(&Value::Null)))
        .unwrap_or_default();
    url.replace("\\/", "/").trim().to_string()
}

fn normalize_quality_preference(q: &str) -> String {
    let q = q.to_lowercase();
    match q.as_str() {
        "jymaster" | "hires" | "lossless" | "exhigh" | "standard" => q,
        _ => "standard".to_string(),
    }
}

fn kugou_quality_param(requested_quality: &str) -> String {
    let level = normalize_quality_preference(requested_quality);
    match level.as_str() {
        "jymaster" => "viper_tape".into(),
        "hires" => "hires".into(),
        "lossless" => "flac".into(),
        "exhigh" => "320".into(),
        _ => "128".into(),
    }
}

fn kugou_quality_from_param(param: &str, fallback_level: &str) -> String {
    let text = param.to_lowercase();
    if text == "viper_tape" || text == "jymaster" {
        return "jymaster".into();
    }
    if text == "hires" || text == "hi_res" {
        return "hires".into();
    }
    if text == "flac" || text == "lossless" || text == "sq" {
        return "lossless".into();
    }
    if param.parse::<u32>().map(|n| n >= 320).unwrap_or(false) || text == "320" || text == "exhigh" || text == "hq" {
        return "exhigh".into();
    }
    if param.parse::<u32>().map(|n| n >= 192).unwrap_or(false) {
        return "exhigh".into();
    }
    normalize_quality_preference(fallback_level)
}

fn map_artists(item: &Value) -> Vec<Artist> {
    let singers = item.get("Singers").and_then(|v| v.as_array());
    if let Some(singers) = singers {
        let artists: Vec<Artist> = singers
            .iter()
            .map(|s| Artist {
                id: s.get("id").or_else(|| s.get("SingerId")).and_then(|v| v.as_str().map(|s| s.to_string()).or_else(|| v.as_i64().map(|n| n.to_string()))),
                name: strip_html(s.get("name").or_else(|| s.get("SingerName")).and_then(|v| v.as_str()).unwrap_or("")),
                ..Default::default()
            })
            .filter(|a| !a.name.is_empty())
            .collect();
        if !artists.is_empty() {
            return artists;
        }
    }

    let singer_name = item.get("SingerName").and_then(|v| v.as_str()).unwrap_or("");
    let names: Vec<String> = singer_name
        .split(|c| c == '、' || c == '/' || c == ',')
        .map(|s| strip_html(s))
        .filter(|s| !s.is_empty())
        .collect();
    let ids = item.get("SingerId").and_then(|v| v.as_array());
    names
        .iter()
        .enumerate()
        .map(|(i, name)| Artist {
            id: ids.and_then(|arr| arr.get(i)).and_then(|v| v.as_str().map(|s| s.to_string()).or_else(|| v.as_i64().map(|n| n.to_string()))),
            name: name.clone(),
            ..Default::default()
        })
        .collect()
}

/// 将酷狗搜索结果映射到统一 Song 结构 (对照 mapKugouSearchItem)
fn map_kugou_artists(item: &Value) -> Vec<Artist> {
    let singers = item
        .get("Singers")
        .and_then(|v| v.as_array())
        .or_else(|| item.get("authors").and_then(|v| v.as_array()))
        .or_else(|| item.get("singerinfo").and_then(|v| v.as_array()));
    if let Some(singers) = singers {
        let artists: Vec<Artist> = singers
            .iter()
            .map(|s| {
                let id = first_string(s, &["id", "SingerId", "author_id", "singerid"]);
                Artist {
                    id: if id.is_empty() { None } else { Some(id) },
                    name: strip_html(&first_string(s, &["name", "SingerName", "author_name", "singername"])),
                    ..Default::default()
                }
            })
            .filter(|a| !a.name.is_empty())
            .collect();
        if !artists.is_empty() {
            return artists;
        }
    }

    let singer_name = first_string(item, &["SingerName", "singername", "author_name", "singer"]);
    let ids = item.get("SingerId").and_then(|v| v.as_array());
    singer_name
        .split(|c| c == '/' || c == ',' || c == '\u{3001}')
        .map(|s| strip_html(s))
        .filter(|s| !s.is_empty())
        .enumerate()
        .map(|(i, name)| Artist {
            id: ids
                .and_then(|arr| arr.get(i))
                .map(|v| value_as_string(Some(v)))
                .filter(|s| !s.is_empty()),
            name,
            ..Default::default()
        })
        .collect()
}

fn map_search_item(item: &Value) -> Song {
    let artists = map_kugou_artists(item);
    let hash = item.get("FileHash").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
    let album_id = item.get("AlbumID").map(|v| match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }).unwrap_or_default();
    let mix_song_id = item.get("MixSongID").map(|v| match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }).unwrap_or_default();
    let album_audio_id_raw = item.get("EMixSongID").or_else(|| item.get("AlbumAudioID")).or_else(|| item.get("album_audio_id")).map(|v| match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }).unwrap_or_default();
    let album_audio_id = if mix_song_id.chars().all(|c| c.is_ascii_digit()) && !mix_song_id.is_empty() {
        mix_song_id.clone()
    } else {
        album_audio_id_raw.clone()
    };

    let name = strip_html(item.get("SongName").or_else(|| item.get("FileName")).or_else(|| item.get("OriSongName")).and_then(|v| v.as_str()).unwrap_or(""));
    let artist = if !artists.is_empty() {
        artists.iter().map(|a| a.name.clone()).collect::<Vec<_>>().join(" / ")
    } else {
        strip_html(item.get("SingerName").and_then(|v| v.as_str()).unwrap_or(""))
    };
    let privilege = item.get("Privilege").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let duration = item.get("Duration").and_then(|v| v.as_u64()).unwrap_or(0) * 1000;

    let cover_raw = item.get("Image")
        .or_else(|| item.get("AlbumImage"))
        .or_else(|| item.get("cover"))
        .or_else(|| item.get("img"))
        .or_else(|| item.get("album_cover"))
        .or_else(|| item.get("album_img"))
        .or_else(|| item.get("pic"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    // 也检查嵌套的 albuminfo
    let cover_from_albuminfo = item.get("albuminfo")
        .and_then(|a| a.get("img").or_else(|| a.get("cover")).or_else(|| a.get("sizable_cover")))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let cover_from_trans = item.get("trans_param")
        .and_then(|t| t.get("union_cover"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let cover = kugou_cover_url(
        if !cover_raw.is_empty() { cover_raw } else if !cover_from_albuminfo.is_empty() { cover_from_albuminfo } else { cover_from_trans },
        240,
    );

    Song {
        provider: "kugou".into(),
        id: if !hash.is_empty() { hash.clone() } else if !mix_song_id.is_empty() { mix_song_id.clone() } else { album_audio_id.clone() },
        name,
        artist,
        artists,
        album: strip_html(item.get("AlbumName").and_then(|v| v.as_str()).unwrap_or("")),
        cover,
        duration,
        fee: if privilege >= 10 { 1 } else { 0 },
        playable: privilege <= 8,
        language: 0,
        hash: Some(hash.clone()),
        album_id: Some(album_id.clone()),
        album_audio_id: Some(album_audio_id.clone()),
        hq_hash: Some(item.get("HQFileHash").and_then(|v| v.as_str()).unwrap_or("").to_string()),
        sq_hash: Some(item.get("SQFileHash").and_then(|v| v.as_str()).unwrap_or("").to_string()),
        res_hash: Some(item.get("ResFileHash").and_then(|v| v.as_str()).unwrap_or("").to_string()),
        ..Default::default()
    }
}

/// 将歌单曲目映射到 Song (对照 mapKugouPlaylistTrack)
fn map_playlist_track(item: &Value) -> Song {
    let singers = item.get("singerinfo")
        .and_then(|v| v.as_array())
        .or_else(|| item.get("Singers").and_then(|v| v.as_array()))
        .or_else(|| item.get("authors").and_then(|v| v.as_array()));
    let artist_label = singers
        .map(|arr| {
            arr.iter()
                .map(|s| first_string(s, &["name", "SingerName", "author_name", "singername"]))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" / ")
        })
        .unwrap_or_default();

    let hash = item.get("hash")
        .or_else(|| item.get("FileHash"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_lowercase();

    let mix_song_id = item.get("mixsongid")
        .map(|v| match v { Value::String(s) => s.clone(), Value::Number(n) => n.to_string(), _ => String::new() })
        .or_else(|| item.get("MixSongID").map(|v| match v { Value::String(s) => s.clone(), Value::Number(n) => n.to_string(), _ => String::new() }))
        .or_else(|| item.get("album_audio_id").map(|v| match v { Value::String(s) => s.clone(), Value::Number(n) => n.to_string(), _ => String::new() }))
        .unwrap_or_default();

    let album_audio_id = if mix_song_id.chars().all(|c| c.is_ascii_digit()) && !mix_song_id.is_empty() {
        mix_song_id.clone()
    } else {
        item.get("album_audio_id").map(|v| match v { Value::String(s) => s.clone(), Value::Number(n) => n.to_string(), _ => String::new() }).unwrap_or_default()
    };

    let album_id = item.get("albuminfo")
        .and_then(|a| a.get("id"))
        .or_else(|| item.get("album_id"))
        .or_else(|| item.get("AlbumID"))
        .map(|v| match v { Value::String(s) => s.clone(), Value::Number(n) => n.to_string(), _ => String::new() })
        .unwrap_or_default();

    let explicit_name = first_string(item, &["name", "SongName", "songname", "audio_name"]);
    let filename = first_string(item, &["filename", "FileName"]);
    let (filename_artist, filename_title) = split_artist_title(&filename);
    let name_raw = if !explicit_name.is_empty() {
        explicit_name
    } else if !filename_title.is_empty() {
        filename_title
    } else {
        filename
    };
    let singer_name = {
        let direct = first_string(item, &["SingerName", "singername", "author_name", "singer"]);
        if !direct.is_empty() {
            direct
        } else if !artist_label.is_empty() {
            artist_label.clone()
        } else {
            filename_artist
        }
    };
    let duration_secs = {
        let duration = first_u64(item, &["duration", "Duration"]);
        if duration > 1000 {
            duration / 1000
        } else if duration > 0 {
            duration
        } else {
            first_u64(item, &["timelen", "timelength", "duration_ms"]) / 1000
        }
    };

    // 构建一个合并的 item 用于 map_search_item
    let merged = json!({
        "FileHash": hash,
        "SongName": name_raw,
        "SingerName": singer_name,
        "Singers": singers,
        "AlbumID": album_id,
        "MixSongID": mix_song_id,
        "EMixSongID": if album_audio_id.chars().all(|c| c.is_ascii_digit()) && !album_audio_id.is_empty() { album_audio_id.clone() } else { String::new() },
        "AlbumName": item.get("albuminfo").and_then(|a| a.get("name")).or_else(|| item.get("album_name")).or_else(|| item.get("AlbumName")).and_then(|v| v.as_str()).unwrap_or(""),
        "Image": item.get("cover").or_else(|| item.get("img")).or_else(|| item.get("Image")).or_else(|| item.get("trans_param").and_then(|t| t.get("union_cover"))),
        "Duration": duration_secs,
        "Privilege": item.get("media_privilege").or_else(|| item.get("privilege")).or_else(|| item.get("Privilege")),
        "HQFileHash": first_string(item, &["HQFileHash", "320hash", "hash_320"]),
        "SQFileHash": first_string(item, &["SQFileHash", "sqhash", "hash_flac", "hash_sq"]),
        "ResFileHash": first_string(item, &["ResFileHash", "hash_high", "hash_super"]),
    });

    let mut mapped = map_search_item(&merged);
    if mapped.hash.as_deref().unwrap_or("").is_empty() && !hash.is_empty() {
        mapped.hash = Some(hash.clone());
    }
    if mapped.album_audio_id.as_deref().unwrap_or("").is_empty() && !album_audio_id.is_empty() {
        mapped.album_audio_id = Some(album_audio_id.clone());
    }
    if mapped.album_id.as_deref().unwrap_or("").is_empty() && !album_id.is_empty() {
        mapped.album_id = Some(album_id.clone());
    }
    mapped
}

fn map_playlist_item(item: &Value) -> Playlist {
    let id = item.get("global_collection_id")
        .or_else(|| item.get("global_specialid"))
        .or_else(|| item.get("gid"))
        .or_else(|| item.get("specialid"))
        .or_else(|| item.get("listid"))
        .or_else(|| item.get("list_id"))
        .or_else(|| item.get("id"))
        .and_then(|v| match v { Value::String(s) => Some(s.clone()), Value::Number(n) => Some(n.to_string()), _ => None })
        .unwrap_or_default();

    let cover_raw = item.get("pic")
        .or_else(|| item.get("img"))
        .or_else(|| item.get("imgurl"))
        .or_else(|| item.get("sizable_cover"))
        .or_else(|| item.get("create_user_pic"))
        .or_else(|| item.get("user_avatar"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    Playlist {
        provider: "kugou".into(),
        id,
        name: strip_html(item.get("name").or_else(|| item.get("listname")).or_else(|| item.get("specialname")).or_else(|| item.get("title")).and_then(|v| v.as_str()).unwrap_or("酷狗歌单")),
        cover: kugou_cover_url(cover_raw, 240),
        track_count: item.get("count").or_else(|| item.get("m_count")).or_else(|| item.get("song_count")).or_else(|| item.get("songcount")).or_else(|| item.get("total")).or_else(|| item.get("list_count")).and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        creator: strip_html(item.get("nickname").or_else(|| item.get("username")).or_else(|| item.get("user_name")).or_else(|| item.get("list_create_username")).and_then(|v| v.as_str()).unwrap_or("")),
        subscribed: false,
    }
}

/// 从 gateway 歌单列表数据中提取歌单数组 (对照 extractKugouGatewayPlaylistLists)
fn extract_gateway_playlist_lists(data: &Value) -> Vec<Value> {
    let data = data.get("data").unwrap_or(data);
    if let Some(arr) = data.get("info").and_then(|v| v.as_array()) {
        return arr.clone();
    }
    let info = data.get("info").unwrap_or(data);
    let mut result = Vec::new();
    for key in &["collect", "love", "self", "list"] {
        if let Some(arr) = info.get(*key).and_then(|v| v.as_array()) {
            result.extend(arr.iter().cloned());
        }
    }
    if let Some(arr) = data.get("list").and_then(|v| v.as_array()) {
        result.extend(arr.iter().cloned());
    }
    result
}

fn parse_kugou_list_id(playlist_id: &str) -> String {
    let id = playlist_id.trim();
    if id.is_empty() {
        return String::new();
    }
    if id.chars().all(|c| c.is_ascii_digit()) {
        return id.to_string();
    }
    if id.starts_with("collection_") {
        let parts: Vec<&str> = id.split('_').collect();
        if parts.len() >= 5 && !parts[3].is_empty() {
            return parts[3].to_string();
        }
    }
    // 尝试匹配 collection_\d+_\d+_(\d+)_\d+
    if let Some(start) = id.find("collection_") {
        let rest = &id[start + 11..];
        let nums: Vec<&str> = rest.split('_').collect();
        if nums.len() >= 4 && !nums[2].is_empty() {
            return nums[2].to_string();
        }
    }
    id.to_string()
}

// ============================================================
//  API 函数 (对照 kugou-api.js)
// ============================================================

/// 搜索歌曲 (对照 kugouSearch)
fn public_special_id_from_playlist_id(playlist_id: &str) -> String {
    let id = playlist_id.trim();
    if let Some(rest) = id.strip_prefix("special_") {
        return rest.trim().to_string();
    }
    if id.chars().all(|c| c.is_ascii_digit()) {
        return id.to_string();
    }
    String::new()
}

fn public_special_id_from_item(item: &Value) -> String {
    first_string(item, &["specialid", "collection_id"])
}

fn extract_mobile_list(json: &Value) -> Vec<Value> {
    if let Some(arr) = json.get("data").and_then(|v| v.as_array()) {
        return arr.clone();
    }
    if let Some(arr) = json
        .get("data")
        .and_then(|d| d.get("info"))
        .and_then(|v| v.as_array())
    {
        return arr.clone();
    }
    Vec::new()
}

fn map_singer_item(item: &Value) -> Artist {
    let id = first_string(item, &["singerid", "SingerId", "id"]);
    let pic = first_string(item, &["imgurl", "sizable_avatar", "avatar", "pic", "image"]);
    let size = first_u64(item, &["songcount", "song_count", "music_size"]);
    Artist {
        id: if id.is_empty() { None } else { Some(id) },
        name: strip_html(&first_string(item, &["singername", "SingerName", "name"])),
        pic_url: if pic.is_empty() { None } else { Some(kugou_cover_url(&pic, 240)) },
        music_size: if size == 0 { None } else { Some(size as i64) },
        ..Default::default()
    }
}

/// 通过歌手详情接口获取头像 (搜索接口不返回头像字段)
async fn singer_avatar(singer_id: &str, cookie: &str) -> String {
    if singer_id.is_empty() {
        return String::new();
    }
    let mut url = match url::Url::parse(&format!("{}/api/v3/singer/info", KUGOU_MOBILE_CDN)) {
        Ok(u) => u,
        Err(_) => return String::new(),
    };
    url.query_pairs_mut()
        .append_pair("format", "json")
        .append_pair("singerid", singer_id);

    let cookie_header = build_request_cookie(cookie);
    let json = match request_json(
        url.as_str(),
        reqwest::Method::GET,
        &[
            (USER_AGENT.as_str(), KUGOU_H5_UA),
            (REFERER.as_str(), KUGOU_REFERER),
            (COOKIE.as_str(), &cookie_header),
        ],
        None,
    )
    .await
    {
        Ok(j) => j,
        Err(_) => return String::new(),
    };

    let img = json
        .get("data")
        .map(|d| first_string(d, &["imgurl", "ImgUrl", "avatar", "pic"]))
        .unwrap_or_default();
    kugou_cover_url(&img, 240)
}

/// 酷狗歌手搜索接口不返回头像，并行通过 /api/v3/singer/info 补充
async fn fill_singer_avatars(artists: &mut [Artist], cookie: &str) {
    use futures_util::StreamExt;

    let cookie_owned = cookie.to_string();
    let futures: Vec<_> = artists
        .iter()
        .filter(|a| {
            let id = a.id.as_deref().unwrap_or("");
            let has_pic = a.pic_url.as_deref().map(|u| !u.is_empty()).unwrap_or(false);
            !id.is_empty() && !has_pic
        })
        .map(|a| {
            let id = a.id.clone().unwrap_or_default();
            let cookie = cookie_owned.clone();
            async move {
                let avatar = singer_avatar(&id, &cookie).await;
                (id, avatar)
            }
        })
        .collect();

    let mut stream = futures_util::stream::iter(futures).buffer_unordered(8);
    while let Some((id, avatar)) = stream.next().await {
        if !avatar.is_empty() {
            if let Some(artist) = artists.iter_mut().find(|a| a.id.as_deref() == Some(id.as_str())) {
                artist.pic_url = Some(avatar);
            }
        }
    }
}

async fn public_special_info(special_id: &str, cookie: &str) -> Playlist {
    let mut url = match url::Url::parse(&format!("{}/api/v3/special/info", KUGOU_MOBILE_CDN)) {
        Ok(url) => url,
        Err(_) => return Playlist::default(),
    };
    url.query_pairs_mut()
        .append_pair("format", "json")
        .append_pair("specialid", special_id);
    let cookie_header = build_request_cookie(cookie);
    match request_json(
        url.as_str(),
        reqwest::Method::GET,
        &[
            (USER_AGENT.as_str(), KUGOU_H5_UA),
            (REFERER.as_str(), KUGOU_REFERER),
            (COOKIE.as_str(), &cookie_header),
        ],
        None,
    )
    .await
    {
        Ok(json) => {
            let mut playlist = json.get("data").map(map_playlist_item).unwrap_or_default();
            if playlist.id.is_empty() {
                playlist.id = format!("special_{}", special_id);
            }
            playlist
        }
        Err(_) => Playlist::default(),
    }
}

async fn public_special_tracks_paged(
    playlist_id: &str,
    cookie: &str,
    start: usize,
    count: usize,
) -> Result<(Playlist, Vec<Song>), String> {
    let special_id = public_special_id_from_playlist_id(playlist_id);
    if special_id.is_empty() {
        return Ok((Playlist::default(), Vec::new()));
    }

    let pagesize = count.clamp(1, 50) as u32;
    let page = (start / pagesize as usize) as u32 + 1;
    let mut url = url::Url::parse(&format!("{}/api/v3/special/song", KUGOU_MOBILE_CDN))
        .map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("format", "json")
        .append_pair("specialid", &special_id)
        .append_pair("page", &page.to_string())
        .append_pair("pagesize", &pagesize.to_string());

    let cookie_header = build_request_cookie(cookie);
    let json = request_json(
        url.as_str(),
        reqwest::Method::GET,
        &[
            (USER_AGENT.as_str(), KUGOU_H5_UA),
            (REFERER.as_str(), KUGOU_REFERER),
            (COOKIE.as_str(), &cookie_header),
        ],
        None,
    )
    .await?;

    let total = json
        .get("data")
        .and_then(|d| d.get("total"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let mut playlist = public_special_info(&special_id, cookie).await;
    if playlist.id.is_empty() {
        playlist.id = format!("special_{}", special_id);
    }
    if playlist.track_count == 0 {
        playlist.track_count = total;
    }

    let songs = extract_mobile_list(&json)
        .iter()
        .map(map_playlist_track)
        .filter(|s| !s.name.is_empty() && (s.hash.as_deref().unwrap_or("").is_empty() == false || !s.id.is_empty()))
        .collect();

    Ok((playlist, songs))
}

pub async fn search(keywords: &str, limit: u32, cookie: &str) -> Result<Vec<Song>, String> {
    let auth = extract_kugou_auth(cookie);
    let page_size = limit.clamp(1, 20);
    let page = 1u32;

    let mut url = url::Url::parse(KUGOU_SEARCH_URL).map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("keyword", keywords)
        .append_pair("page", &page.to_string())
        .append_pair("pagesize", &page_size.to_string())
        .append_pair("userid", if auth.userid.is_empty() { "-1" } else { &auth.userid })
        .append_pair("clientver", "2000")
        .append_pair("platform", "WebFilter")
        .append_pair("tag", "em")
        .append_pair("filter", "2")
        .append_pair("iscorrection", "1")
        .append_pair("privilege_filter", "0")
        .append_pair("filter_ver", "2")
        .append_pair("appid", &KUGOU_WEB_APPID.to_string())
        .append_pair("token", &auth.token)
        .append_pair("mid", &auth.mid);

    let cookie_header = build_request_cookie(cookie);
    let json = request_json(
        url.as_str(),
        reqwest::Method::GET,
        &[
            (USER_AGENT.as_str(), KUGOU_H5_UA),
            (REFERER.as_str(), KUGOU_REFERER),
            (COOKIE.as_str(), &cookie_header),
        ],
        None,
    )
    .await?;

    let list = json
        .get("data")
        .and_then(|d| d.get("lists"))
        .and_then(|l| l.as_array())
        .cloned()
        .unwrap_or_default();

    let songs: Vec<Song> = list
        .iter()
        .map(|item| map_search_item(item))
        .filter(|s| !s.name.is_empty() && (s.hash.as_deref().unwrap_or("").is_empty() == false || !s.id.is_empty()))
        .collect();

    Ok(songs)
}

/// H5 方式获取播放 URL (对照 kugouPlayViaH5)
pub async fn artist_search(keywords: &str, limit: u32, cookie: &str) -> Result<Vec<Artist>, String> {
    let page_size = limit.clamp(1, 30);
    let mut url = url::Url::parse(&format!("{}/api/v3/search/singer", KUGOU_MOBILE_CDN))
        .map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("format", "json")
        .append_pair("keyword", keywords)
        .append_pair("page", "1")
        .append_pair("pagesize", &page_size.to_string());

    let cookie_header = build_request_cookie(cookie);
    let json = request_json(
        url.as_str(),
        reqwest::Method::GET,
        &[
            (USER_AGENT.as_str(), KUGOU_H5_UA),
            (REFERER.as_str(), KUGOU_REFERER),
            (COOKIE.as_str(), &cookie_header),
        ],
        None,
    )
    .await?;

    let mut artists: Vec<Artist> = extract_mobile_list(&json)
        .iter()
        .map(map_singer_item)
        .filter(|a| !a.name.is_empty())
        .collect();

    // 搜索接口不返回头像，通过歌手详情接口补充
    fill_singer_avatars(&mut artists, cookie).await;

    Ok(artists)
}

pub async fn playlist_search(keywords: &str, limit: u32, cookie: &str) -> Result<Vec<Playlist>, String> {
    let page_size = limit.clamp(1, 30);
    let mut url = url::Url::parse(&format!("{}/api/v3/search/special", KUGOU_MOBILE_CDN))
        .map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("format", "json")
        .append_pair("keyword", keywords)
        .append_pair("page", "1")
        .append_pair("pagesize", &page_size.to_string());

    let cookie_header = build_request_cookie(cookie);
    let json = request_json(
        url.as_str(),
        reqwest::Method::GET,
        &[
            (USER_AGENT.as_str(), KUGOU_H5_UA),
            (REFERER.as_str(), KUGOU_REFERER),
            (COOKIE.as_str(), &cookie_header),
        ],
        None,
    )
    .await?;

    Ok(extract_mobile_list(&json)
        .iter()
        .map(|item| {
            let mut playlist = map_playlist_item(item);
            let special_id = public_special_id_from_item(item);
            if !special_id.is_empty() {
                playlist.id = format!("special_{}", special_id);
            }
            playlist
        })
        .filter(|pl| !pl.id.is_empty() && !pl.name.is_empty())
        .collect())
}

pub async fn artist_songs(artist_id: &str, limit: u32, offset: u32, cookie: &str) -> Result<Vec<Song>, String> {
    let page_size = limit.clamp(1, 50);
    let page = (offset / page_size) + 1;
    let mut url = url::Url::parse(&format!("{}/api/v3/singer/song", KUGOU_MOBILE_CDN))
        .map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("format", "json")
        .append_pair("singerid", artist_id)
        .append_pair("page", &page.to_string())
        .append_pair("pagesize", &page_size.to_string());

    let cookie_header = build_request_cookie(cookie);
    let json = request_json(
        url.as_str(),
        reqwest::Method::GET,
        &[
            (USER_AGENT.as_str(), KUGOU_H5_UA),
            (REFERER.as_str(), KUGOU_REFERER),
            (COOKIE.as_str(), &cookie_header),
        ],
        None,
    )
    .await?;

    Ok(extract_mobile_list(&json)
        .iter()
        .map(map_playlist_track)
        .filter(|s| !s.name.is_empty() && (s.hash.as_deref().unwrap_or("").is_empty() == false || !s.id.is_empty()))
        .collect())
}

async fn play_via_h5(
    hash: &str,
    album_id: &str,
    album_audio_id: &str,
    cookie: &str,
    requested_quality: &str,
) -> Result<Option<PlayResult>, String> {
    let auth = extract_kugou_auth(cookie);
    if !auth.playback_ready {
        return Ok(None);
    }
    let quality = kugou_quality_param(requested_quality);
    let file_hash = hash.to_lowercase();
    let mut params = build_h5_params(&auth, vec![
        ("album_id".into(), album_id.parse::<u64>().unwrap_or(0).to_string()),
        ("area_code".into(), "1".into()),
        ("hash".into(), file_hash.clone()),
        ("ssa_flag".into(), "is_fromtrack".into()),
        ("version".into(), "11430".into()),
        ("quality".into(), quality.clone()),
        ("album_audio_id".into(), album_audio_id.parse::<u64>().unwrap_or(0).to_string()),
        ("behavior".into(), "play".into()),
        ("pid".into(), "2".into()),
        ("cmd".into(), "26".into()),
        ("pidversion".into(), "3001".into()),
        ("IsFreePart".into(), "1".into()),
        ("cdnBackup".into(), "1".into()),
        ("module".into(), "".into()),
    ]);
    let key = sign_key(&file_hash, &auth.mid, &auth.userid, KUGOU_WEB_APPID);
    params.push(("key".into(), key));
    let sig = signature_h5(&params, None);
    params.push(("signature".into(), sig));

    let mut url = url::Url::parse(&format!("{}{}", KUGOU_GATEWAY, "/v5/url")).map_err(|e| e.to_string())?;
    for (k, v) in &params {
        url.query_pairs_mut().append_pair(k, v);
    }

    let cookie_header = build_request_cookie(cookie);
    let json = request_json(
        url.as_str(),
        reqwest::Method::GET,
        &[
            (USER_AGENT.as_str(), KUGOU_H5_UA),
            (REFERER.as_str(), KUGOU_REFERER),
            ("x-router", "trackercdn.kugou.com"),
            (COOKIE.as_str(), &cookie_header),
        ],
        None,
    )
    .await?;

    let url_str = pick_play_url(&json);
    let status = json.get("status").and_then(|v| v.as_i64()).unwrap_or(0);
    if status == 1 && !url_str.is_empty() {
        let level = kugou_quality_from_param(&quality, requested_quality);
        return Ok(Some(PlayResult {
            url: url_str,
            level: level.clone(),
            quality: level,
            trial: false,
            source: "h5".into(),
            ..Default::default()
        }));
    }

    let err_msg = json.get("error").or_else(|| json.get("msg")).and_then(|v| v.as_str()).unwrap_or("");
    if err_msg.contains("付费") || err_msg.contains("会员") || err_msg.contains("vip") || err_msg.contains("登录") {
        return Ok(Some(PlayResult {
            restricted: true,
            category: if auth.playback_ready { "vip_required" } else { "login_required" }.into(),
            message: err_msg.to_string(),
            ..Default::default()
        }));
    }

    Ok(None)
}

/// Mobile 方式获取播放 URL (对照 kugouPlayViaMobile)
async fn play_via_mobile(hash: &str, album_id: &str, cookie: &str) -> Result<Option<PlayResult>, String> {
    let auth = extract_kugou_auth(cookie);
    let key = kugou_cloud_key(hash);

    let mut url = url::Url::parse(KUGOU_PLAY_MOBILE).map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("cmd", "playInfo")
        .append_pair("hash", hash)
        .append_pair("key", &key)
        .append_pair("album_id", album_id)
        .append_pair("pid", "1")
        .append_pair("forceDown", "0")
        .append_pair("vip", "65530");

    if !auth.userid.is_empty() {
        url.query_pairs_mut().append_pair("userid", &auth.userid);
    }
    if !auth.token.is_empty() {
        url.query_pairs_mut().append_pair("token", &auth.token);
    }

    let cookie_header = build_request_cookie(cookie);
    let json = request_json(
        url.as_str(),
        reqwest::Method::GET,
        &[
            (USER_AGENT.as_str(), KUGOU_H5_UA),
            (REFERER.as_str(), "https://m.kugou.com/"),
            (COOKIE.as_str(), &cookie_header),
        ],
        None,
    )
    .await?;

    let url_str = json.get("url").or_else(|| json.get("backup_url")).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let status = json.get("status").and_then(|v| v.as_i64()).unwrap_or(0);
    if status == 1 && !url_str.is_empty() {
        return Ok(Some(PlayResult {
            url: url_str,
            level: "standard".into(),
            quality: "标准".into(),
            trial: false,
            source: "mobile".into(),
            ..Default::default()
        }));
    }

    let err_msg = json.get("error").or_else(|| json.get("errmsg")).and_then(|v| v.as_str()).unwrap_or("");
    if err_msg.contains("付费") || err_msg.contains("会员") || err_msg.contains("vip") {
        return Ok(Some(PlayResult {
            restricted: true,
            category: "vip_required".into(),
            message: "酷狗歌曲需要会员或付费权限".into(),
            ..Default::default()
        }));
    }

    Ok(None)
}

/// Web 方式获取播放 URL (对照 kugouPlayViaWeb)
async fn play_via_web(hash: &str, album_id: &str, album_audio_id: &str, cookie: &str) -> Result<Option<PlayResult>, String> {
    let auth = extract_kugou_auth(cookie);

    let mut url = url::Url::parse(KUGOU_PLAY_WEB).map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("r", "play/getdata")
        .append_pair("hash", hash)
        .append_pair("album_id", album_id);
    if !album_audio_id.is_empty() {
        url.query_pairs_mut().append_pair("album_audio_id", album_audio_id);
    }
    url.query_pairs_mut()
        .append_pair("appid", &KUGOU_WEB_APPID.to_string())
        .append_pair("platid", "4")
        .append_pair("mid", &auth.mid)
        .append_pair("dfid", if auth.dfid.is_empty() { "-" } else { &auth.dfid })
        .append_pair("userid", if auth.userid.is_empty() { "0" } else { &auth.userid })
        .append_pair("token", &auth.token);

    let cookie_header = build_request_cookie(cookie);
    let json = request_json(
        url.as_str(),
        reqwest::Method::GET,
        &[
            (USER_AGENT.as_str(), KUGOU_H5_UA),
            (REFERER.as_str(), KUGOU_REFERER),
            (COOKIE.as_str(), &cookie_header),
        ],
        None,
    )
    .await?;

    let data = json.get("data").unwrap_or(&Value::Null);
    let url_str = data.get("play_url").or_else(|| data.get("play_backup_url")).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let status = json.get("status").and_then(|v| v.as_i64()).unwrap_or(0);
    if status == 1 && !url_str.is_empty() {
        let bitrate = data.get("bitrate").and_then(|v| v.as_u64()).unwrap_or(0);
        let level = if bitrate >= 900 { "lossless" } else if bitrate >= 300 { "exhigh" } else { "standard" };
        return Ok(Some(PlayResult {
            url: url_str.replace("\\/", "/").trim().to_string(),
            level: level.into(),
            quality: level.into(),
            trial: false,
            source: "web".into(),
            ..Default::default()
        }));
    }

    Ok(None)
}

/// Android Gateway 方式获取播放 URL (对照 kugouPlayViaGateway)
async fn play_via_gateway(
    hash: &str,
    album_id: &str,
    album_audio_id: &str,
    cookie: &str,
    requested_quality: &str,
) -> Result<Option<PlayResult>, String> {
    let auth = extract_kugou_auth(cookie);
    if !auth.playback_ready {
        return Ok(None);
    }
    let quality = kugou_quality_param(requested_quality);
    let clienttime = now_secs();
    let params = vec![
        ("dfid".into(), if auth.dfid.is_empty() { "-".into() } else { auth.dfid.clone() }),
        ("mid".into(), auth.mid.clone()),
        ("uuid".into(), "-".into()),
        ("appid".into(), KUGOU_APPID.to_string()),
        ("clientver".into(), KUGOU_CLIENTVER.to_string()),
        ("clienttime".into(), clienttime.to_string()),
        ("token".into(), auth.token.clone()),
        ("userid".into(), auth.userid.clone()),
        ("album_id".into(), album_id.parse::<u64>().unwrap_or(0).to_string()),
        ("area_code".into(), "1".into()),
        ("hash".into(), hash.to_lowercase()),
        ("ssa_flag".into(), "is_fromtrack".into()),
        ("version".into(), "11430".into()),
        ("quality".into(), quality.clone()),
        ("album_audio_id".into(), album_audio_id.parse::<u64>().unwrap_or(0).to_string()),
        ("behavior".into(), "play".into()),
        ("pid".into(), "2".into()),
        ("cmd".into(), "26".into()),
        ("pidversion".into(), "3001".into()),
        ("IsFreePart".into(), "1".into()),
        ("cdnBackup".into(), "1".into()),
        ("module".into(), "".into()),
    ];
    let key = sign_key(&hash.to_lowercase(), &auth.mid, &auth.userid, KUGOU_APPID);
    let mut all_params = params.clone();
    all_params.push(("key".into(), key));
    let sig = signature_android(&all_params, "");
    all_params.push(("signature".into(), sig));

    let mut url = url::Url::parse(&format!("{}{}", KUGOU_GATEWAY, "/v5/url")).map_err(|e| e.to_string())?;
    for (k, v) in &all_params {
        url.query_pairs_mut().append_pair(k, v);
    }

    let cookie_header = build_request_cookie(cookie);
    let json = request_json(
        url.as_str(),
        reqwest::Method::GET,
        &[
            (USER_AGENT.as_str(), KUGOU_GATEWAY_UA),
            (REFERER.as_str(), KUGOU_REFERER),
            ("dfid", &auth.dfid),
            ("mid", &auth.mid),
            ("clienttime", &clienttime.to_string()),
            ("x-router", "trackercdn.kugou.com"),
            (COOKIE.as_str(), &cookie_header),
        ],
        None,
    )
    .await?;

    let data = json.get("data").unwrap_or(&json);
    let url_str = data.get("url")
        .or_else(|| data.get("play_url"))
        .or_else(|| data.get("play_backup_url"))
        .and_then(|v| {
            match v {
                Value::String(s) => Some(s.clone()),
                Value::Array(arr) => arr.first().and_then(|v| v.as_str()).map(|s| s.to_string()),
                _ => None,
            }
        })
        .unwrap_or_default();

    if !url_str.is_empty() {
        let level = kugou_quality_from_param(&quality, requested_quality);
        return Ok(Some(PlayResult {
            url: url_str.replace("\\/", "/").trim().to_string(),
            level: level.clone(),
            quality: level,
            trial: false,
            source: "gateway".into(),
            ..Default::default()
        }));
    }

    Ok(None)
}

/// 播放结果内部结构
#[derive(Debug, Clone, Default)]
struct PlayResult {
    url: String,
    level: String,
    quality: String,
    trial: bool,
    source: String,
    restricted: bool,
    category: String,
    message: String,
}

/// hash 候选 (对照 hashCandidatesFromSong)
fn hash_candidates_from_song(song: &Song, requested_quality: &str) -> Vec<(String, String, String)> {
    let requested = normalize_quality_preference(requested_quality);
    let start_idx = KUGOU_QUALITY_CHAIN.iter().position(|(key, _, _)| *key == requested).unwrap_or(0);
    let chain = &KUGOU_QUALITY_CHAIN[start_idx..];

    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for (key, label, field) in chain {
        let hash = if *field == "FileHash" {
            song.hash.clone().unwrap_or_default()
        } else {
            match *field {
                "HQFileHash" => song.hq_hash.clone().unwrap_or_default(),
                "SQFileHash" => song.sq_hash.clone().unwrap_or_default(),
                "ResFileHash" => song.res_hash.clone().unwrap_or_default(),
                _ => String::new(),
            }
        };
        if hash.is_empty() || seen.contains(&hash) {
            continue;
        }
        seen.insert(hash.clone());
        out.push((hash, key.to_string(), label.to_string()));
    }

    let song_hash = song.hash.clone().unwrap_or_default();
    if !song_hash.is_empty() && !seen.contains(&song_hash) {
        out.push((song_hash, "standard".into(), "标准".into()));
    }

    out
}

/// 获取歌曲播放 URL — 4级降级策略 (对照 handleKugouSongUrl)
pub async fn song_url(
    hash: &str,
    album_id: &str,
    album_audio_id: &str,
    quality: &str,
    cookie: &str,
    hq_hash: &str,
    sq_hash: &str,
    res_hash: &str,
) -> Result<SongUrlResult, String> {
    let auth = extract_kugou_auth(cookie);
    let hash = hash.trim();
    if hash.is_empty() {
        return Ok(SongUrlResult {
            url: None,
            playable: false,
            trial: false,
            level: String::new(),
            quality: String::new(),
            br: 0,
            reason: Some("MISSING_HASH".into()),
            message: Some("缺少酷狗歌曲 hash".into()),
            fee: None,
            ..Default::default()
        });
    }

    let requested_quality = normalize_quality_preference(quality);

    // 构建 song 用于 hash 候选 (使用传入的高音质 hash 值)
    let song = Song {
        hash: Some(hash.to_string()),
        hq_hash: Some(hq_hash.to_string()),
        sq_hash: Some(sq_hash.to_string()),
        res_hash: Some(res_hash.to_string()),
        ..Default::default()
    };

    let candidates = hash_candidates_from_song(&song, &requested_quality);
    let candidates = if candidates.is_empty() {
        vec![(hash.to_string(), "standard".into(), "标准".into())]
    } else {
        candidates
    };

    let mut last_restriction: Option<PlayResult> = None;

    for (candidate_hash, level, _label) in &candidates {
        // 1. H5
        if let Ok(Some(h5)) = play_via_h5(candidate_hash, album_id, album_audio_id, cookie, level).await {
            if !h5.url.is_empty() {
                return Ok(SongUrlResult {
                    url: Some(h5.url),
                    playable: true,
                    trial: false,
                    level: h5.level,
                    quality: h5.quality,
                    br: 0,
                    ..Default::default()
                });
            }
            if h5.restricted {
                last_restriction = Some(h5);
            }
        }

        // 2. Mobile
        if let Ok(Some(mobile)) = play_via_mobile(candidate_hash, album_id, cookie).await {
            if !mobile.url.is_empty() {
                return Ok(SongUrlResult {
                    url: Some(mobile.url),
                    playable: true,
                    trial: false,
                    level: level.clone(),
                    quality: level.clone(),
                    br: 0,
                    ..Default::default()
                });
            }
            if mobile.restricted {
                last_restriction = Some(mobile);
            }
        }

        // 3. Web
        if let Ok(Some(web)) = play_via_web(candidate_hash, album_id, album_audio_id, cookie).await {
            if !web.url.is_empty() {
                return Ok(SongUrlResult {
                    url: Some(web.url),
                    playable: true,
                    trial: false,
                    level: web.level,
                    quality: web.quality,
                    br: 0,
                    ..Default::default()
                });
            }
            if web.restricted {
                last_restriction = Some(web);
            }
        }

        // 4. Gateway
        if let Ok(Some(gateway)) = play_via_gateway(candidate_hash, album_id, album_audio_id, cookie, level).await {
            if !gateway.url.is_empty() {
                return Ok(SongUrlResult {
                    url: Some(gateway.url),
                    playable: true,
                    trial: false,
                    level: gateway.level,
                    quality: gateway.quality,
                    br: 0,
                    ..Default::default()
                });
            }
            if gateway.restricted {
                last_restriction = Some(gateway);
            }
        }
    }

    let restriction = last_restriction.unwrap_or_else(|| PlayResult {
        category: if auth.playback_ready { "vip_required" } else { "login_required" }.into(),
        message: if auth.playback_ready { "酷狗歌曲需要会员或付费权限" } else { "酷狗歌曲需要登录后再播放" }.into(),
        ..Default::default()
    });

    Ok(SongUrlResult {
        url: None,
        playable: false,
        trial: false,
        level: requested_quality.clone(),
        quality: requested_quality,
        br: 0,
        reason: Some(restriction.category),
        message: Some(restriction.message),
        fee: None,
        ..Default::default()
    })
}

/// 获取歌词 (对照 handleKugouLyric)
pub async fn lyric(hash: &str, album_audio_id: &str, duration_sec: u64) -> Result<Lyrics, String> {
    let file_hash = hash.trim();
    if file_hash.is_empty() {
        return Ok(Lyrics::default());
    }

    let mut search_url = url::Url::parse(KUGOU_LYRIC_SEARCH).map_err(|e| e.to_string())?;
    search_url.query_pairs_mut()
        .append_pair("ver", "1")
        .append_pair("man", "yes")
        .append_pair("client", "pc")
        .append_pair("keyword", "")
        .append_pair("duration", &duration_sec.to_string())
        .append_pair("hash", file_hash);
    if !album_audio_id.is_empty() {
        search_url.query_pairs_mut().append_pair("album_audio_id", album_audio_id);
    }

    let search_json = request_json(
        search_url.as_str(),
        reqwest::Method::GET,
        &[
            (USER_AGENT.as_str(), KUGOU_H5_UA),
            (REFERER.as_str(), KUGOU_REFERER),
        ],
        None,
    )
    .await?;

    let candidate = search_json
        .get("candidates")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first());

    let candidate_id = match candidate.and_then(|c| c.get("id")) {
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::String(s)) => s.clone(),
        _ => return Ok(Lyrics { lyric: String::new(), ..Default::default() }),
    };
    let accesskey = candidate
        .and_then(|c| c.get("accesskey"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let mut dl_url = url::Url::parse(KUGOU_LYRIC_DOWNLOAD).map_err(|e| e.to_string())?;
    dl_url.query_pairs_mut()
        .append_pair("ver", "1")
        .append_pair("client", "pc")
        .append_pair("id", &candidate_id)
        .append_pair("accesskey", accesskey)
        .append_pair("fmt", "lrc")
        .append_pair("charset", "utf8");

    let lyric_json = request_json(
        dl_url.as_str(),
        reqwest::Method::GET,
        &[
            (USER_AGENT.as_str(), KUGOU_H5_UA),
            (REFERER.as_str(), KUGOU_REFERER),
        ],
        None,
    )
    .await?;

    let content_raw = lyric_json.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let lyric = if !content_raw.is_empty() {
        // Base64 解码
        match base64::engine::general_purpose::STANDARD.decode(content_raw) {
            Ok(decoded) => {
                let text = String::from_utf8_lossy(&decoded);
                // 去掉 BOM
                let text = text.strip_prefix('\u{FEFF}').unwrap_or(&text).to_string();
                if text.contains('[') || text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)) {
                    text
                } else {
                    content_raw.to_string()
                }
            }
            Err(_) => content_raw.to_string(),
        }
    } else {
        String::new()
    };

    Ok(Lyrics {
        lyric,
        ..Default::default()
    })
}

/// 获取用户歌单 (对照 handleKugouUserPlaylists)
pub async fn user_playlists(cookie: &str) -> Result<Vec<Playlist>, String> {
    let auth = extract_kugou_auth(cookie);
    if !auth.playback_ready {
        return Ok(Vec::new());
    }

    let body = json!({
        "userid": auth.userid.parse::<u64>().unwrap_or(0),
        "token": auth.token,
        "total_ver": 979,
        "type": 2,
        "page": 1,
        "pagesize": 50,
    });

    let json = h5_gateway_request(
        "/v7/get_all_list",
        "POST",
        cookie,
        vec![("plat".into(), "1".into())],
        Some(body),
        Some("cloudlist.service.kugou.com"),
    )
    .await?;

    // 登录态失效/接口异常时明确报错，避免静默显示"暂无歌单"
    let errmsg = json.get("errmsg")
        .or_else(|| json.get("error"))
        .or_else(|| json.get("msg"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let status = json.get("status").and_then(|v| v.as_i64())
        .or_else(|| json.get("code").and_then(|v| v.as_i64()))
        .unwrap_or(0);
    if !errmsg.is_empty() {
        log::warn!("[KugouPlaylist] user_playlists error: status={}, msg={}", status, errmsg);
        return Err(format!("酷狗歌单获取失败: {}", errmsg));
    }

    let data = json.get("data").unwrap_or(&Value::Null);
    let lists = extract_gateway_playlist_lists(data);

    // 提取用户头像, 用于默认歌单封面回退
    let (_nickname, avatar) = pick_profile_from_lists(&lists, &auth);
    let fallback_avatar = if !avatar.is_empty() { avatar.clone() } else { auth.avatar.clone() };

    let playlists: Vec<Playlist> = lists
        .iter()
        .map(|item| {
            let mut pl = map_playlist_item(item);
            // 默认收藏/我喜欢歌单没有封面时, 使用用户头像
            if pl.cover.is_empty() && !fallback_avatar.is_empty() {
                pl.cover = fallback_avatar.clone();
            }
            pl
        })
        .filter(|pl| !pl.id.is_empty() && !pl.name.is_empty())
        .collect();

    Ok(playlists)
}

/// 从歌单列表中提取用户资料 (对照 pickKugouProfileFromLists)
fn pick_profile_from_lists(lists: &[Value], auth: &KugouAuth) -> (String, String) {
    let selected = lists.iter().find(|item| {
        let item_user_id = item
            .get("list_create_userid")
            .or_else(|| item.get("userid"))
            .or_else(|| item.get("user_id"))
            .or_else(|| item.get("owner_id"))
            .and_then(|v| match v { Value::String(s) => Some(s.clone()), Value::Number(n) => Some(n.to_string()), _ => None })
            .map(|s| s.chars().filter(|c| c.is_ascii_digit()).collect::<String>())
            .unwrap_or_default();
        !auth.userid.is_empty() && !item_user_id.is_empty() && item_user_id == auth.userid
    }).or_else(|| lists.first());

    let selected = match selected {
        Some(s) => s,
        None => return (String::new(), String::new()),
    };

    let nickname = strip_html(
        selected.get("nickname")
            .or_else(|| selected.get("username"))
            .or_else(|| selected.get("user_name"))
            .or_else(|| selected.get("list_create_username"))
            .or_else(|| selected.get("owner_name"))
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    );
    let avatar_raw = selected.get("create_user_pic")
        .or_else(|| selected.get("user_pic"))
        .or_else(|| selected.get("avatar"))
        .or_else(|| selected.get("pic"))
        .or_else(|| selected.get("img"))
        .or_else(|| selected.get("imgurl"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let avatar = kugou_cover_url(avatar_raw, 120);

    (nickname, avatar)
}

/// 获取歌单曲目 (对照 handleKugouPlaylistTracks)
pub async fn playlist_tracks(playlist_id: &str, cookie: &str) -> Result<(Playlist, Vec<Song>), String> {
    if playlist_id.trim().starts_with("special_") {
        return public_special_tracks_paged(playlist_id, cookie, 0, 50).await;
    }

    let auth = extract_kugou_auth(cookie);
    if !auth.playback_ready {
        return Ok((Playlist::default(), Vec::new()));
    }

    let listid = parse_kugou_list_id(playlist_id);
    if listid.is_empty() {
        return Ok((Playlist::default(), Vec::new()));
    }

    let pagesize = 50u32;
    let mut all_tracks = Vec::new();
    let mut total = 0u32;

    for round in 0..500u32 {
        let body = json!({
            "listid": listid.parse::<u64>().unwrap_or(0),
            "userid": auth.userid.parse::<u64>().unwrap_or(0),
            "area_code": 1,
            "show_relate_goods": 0,
            "pagesize": pagesize,
            "allplatform": 1,
            "show_cover": 1,
            "type": 0,
            "token": auth.token,
            "page": round + 1,
        });

        let json = match h5_gateway_request(
            "/v4/get_list_all_file",
            "POST",
            cookie,
            vec![("plat".into(), "1".into())],
            Some(body),
            Some("cloudlist.service.kugou.com"),
        )
        .await {
            Ok(j) => j,
            Err(e) => {
                // H5 gateway 失败 — 可能是排行榜 ID, 尝试 mobilecdn rank API 回退
                log::info!("[KugouPlaylist] H5 gateway failed for {}, trying rank fallback: {}", listid, e);
                if all_tracks.is_empty() {
                    match get_rank_songs(cookie, &listid, pagesize).await {
                        Ok(rank_songs) if !rank_songs.is_empty() => {
                            let playlist = Playlist {
                                provider: "kugou".into(),
                                id: listid.clone(),
                                name: String::new(),
                                cover: String::new(),
                                track_count: rank_songs.len() as u32,
                                creator: "酷狗音乐".into(),
                                subscribed: false,
                            };
                            return Ok((playlist, rank_songs));
                        }
                        _ => {}
                    }
                }
                if all_tracks.is_empty() {
                    match public_special_tracks_paged(&listid, cookie, 0, pagesize as usize).await {
                        Ok((playlist, songs)) if !songs.is_empty() => {
                            return Ok((playlist, songs));
                        }
                        _ => {}
                    }
                }
                break;
            }
        };

        let data = json.get("data").unwrap_or(&Value::Null);
        // 对照 JS: data.info || data.songs || data.lists || data.file || []
        // JS 还处理 chunk.file 的情况 (chunk 不是数组但有 .file 属性)
        let chunk = data.get("info")
            .or_else(|| data.get("songs"))
            .or_else(|| data.get("lists"))
            .or_else(|| data.get("file"))
            .unwrap_or(&Value::Null);
        let chunk_arr: Vec<Value> = if let Some(arr) = chunk.as_array() {
            arr.clone()
        } else if let Some(file_arr) = chunk.get("file").and_then(|v| v.as_array()) {
            file_arr.clone()
        } else {
            Vec::new()
        };

        if chunk_arr.is_empty() {
            // 第一页就为空 — 可能是排行榜 ID, 尝试 mobilecdn rank API 回退
            if all_tracks.is_empty() {
                log::info!("[KugouPlaylist] H5 gateway returned empty for {}, trying rank fallback", listid);
                match get_rank_songs(cookie, &listid, pagesize).await {
                    Ok(rank_songs) if !rank_songs.is_empty() => {
                        let playlist = Playlist {
                            provider: "kugou".into(),
                            id: listid.clone(),
                            name: String::new(),
                            cover: String::new(),
                            track_count: rank_songs.len() as u32,
                            creator: "酷狗音乐".into(),
                            subscribed: false,
                        };
                        return Ok((playlist, rank_songs));
                    }
                    _ => {}
                }
            }
            if all_tracks.is_empty() {
                match public_special_tracks_paged(&listid, cookie, 0, pagesize as usize).await {
                    Ok((playlist, songs)) if !songs.is_empty() => {
                        return Ok((playlist, songs));
                    }
                    _ => {}
                }
            }
            break;
        }

        total = data.get("count").and_then(|v| v.as_u64()).unwrap_or(all_tracks.len() as u64) as u32;

        for item in &chunk_arr {
            let mapped = map_playlist_track(item);
            if !mapped.name.is_empty() && (mapped.hash.as_deref().unwrap_or("").is_empty() == false || !mapped.id.is_empty()) {
                all_tracks.push(mapped);
            }
        }

        if (chunk_arr.len() as u32) < pagesize || (total > 0 && all_tracks.len() as u32 >= total) {
            break;
        }
    }

    // 酷狗歌单顺序是倒序的，需要反转 (对照 reverseKugouTracks)
    all_tracks.reverse();

    let playlist = Playlist {
        provider: "kugou".into(),
        id: listid.clone(),
        name: String::new(),
        cover: String::new(),
        track_count: total,
        creator: String::new(),
        subscribed: false,
    };

    Ok((playlist, all_tracks))
}

/// 分页获取歌单曲目
pub async fn playlist_tracks_paged(playlist_id: &str, cookie: &str, start: usize, count: usize) -> Result<Vec<Song>, String> {
    if playlist_id.trim().starts_with("special_") {
        return public_special_tracks_paged(playlist_id, cookie, start, count)
            .await
            .map(|(_, songs)| songs);
    }

    let auth = extract_kugou_auth(cookie);
    if !auth.playback_ready {
        return Ok(Vec::new());
    }

    let listid = parse_kugou_list_id(playlist_id);
    if listid.is_empty() {
        return Ok(Vec::new());
    }

    let pagesize = count.clamp(10, 50) as u32;
    let page_no = (start / pagesize as usize) as u32 + 1;

    let body = json!({
        "listid": listid.parse::<u64>().unwrap_or(0),
        "userid": auth.userid.parse::<u64>().unwrap_or(0),
        "area_code": 1,
        "show_relate_goods": 0,
        "pagesize": pagesize,
        "allplatform": 1,
        "show_cover": 1,
        "type": 0,
        "token": auth.token,
        "page": page_no,
    });

    let json = h5_gateway_request(
        "/v4/get_list_all_file",
        "POST",
        cookie,
        vec![("plat".into(), "1".into())],
        Some(body),
        Some("cloudlist.service.kugou.com"),
    )
    .await?;

    let data = json.get("data").unwrap_or(&Value::Null);
    let chunk = data.get("info")
        .or_else(|| data.get("songs"))
        .or_else(|| data.get("lists"))
        .or_else(|| data.get("file"))
        .unwrap_or(&Value::Null);
    let chunk_arr: Vec<Value> = if let Some(arr) = chunk.as_array() {
        arr.clone()
    } else if let Some(file_arr) = chunk.get("file").and_then(|v| v.as_array()) {
        file_arr.clone()
    } else {
        Vec::new()
    };

    let tracks: Vec<Song> = chunk_arr
        .iter()
        .map(|item| map_playlist_track(item))
        .filter(|s| !s.name.is_empty() && (s.hash.as_deref().unwrap_or("").is_empty() == false || !s.id.is_empty()))
        .collect();

    // 反转
    let mut tracks = tracks;
    tracks.reverse();

    Ok(tracks)
}

/// 猜你喜欢/推荐缓存 (防止频繁调用)
static GUESS_CACHE: tokio::sync::RwLock<(u64, Vec<Song>)> = tokio::sync::RwLock::const_new((0, Vec::new()));

/// 猜你喜欢/推荐 (对照 handleKugouGuessLike)
pub async fn guess_like(cookie: &str, limit: u32) -> Result<Vec<Song>, String> {
    let auth = extract_kugou_auth(cookie);
    if !auth.playback_ready {
        return Ok(Vec::new());
    }

    // 缓存检查: 30秒内不重复调用
    {
        let cache = GUESS_CACHE.read().await;
        if !cache.1.is_empty() && now_ms().saturating_sub(cache.0) < 30_000 {
            let take = cache.1.iter().take(limit as usize).cloned().collect();
            return Ok(take);
        }
    }

    let limit = limit.clamp(1, 20);
    let clienttime = now_ms();
    let sign_key = md5_hex(&format!("{}{}{}{}", KUGOU_APPID, KUGOU_CLIENTVER, clienttime, KUGOU_ANDROID_SALT));

    log::info!("[KugouGuessLike] userid={}, token_len={}, mid={}, clienttime={}, key={}",
        auth.userid, auth.token.len(), &auth.mid[..auth.mid.len().min(16)], clienttime, &sign_key[..sign_key.len().min(16)]);

    let body = json!({
        "appid": KUGOU_APPID,
        "area_code": 1,
        "clienttime": clienttime,
        "clientver": KUGOU_CLIENTVER,
        "data": [{"fmid": "0", "fmtype": 2, "offset": -1, "size": limit, "singername": ""}],
        "get_tracker": 1,
        "key": sign_key,
        "mid": auth.mid,
        "uid": auth.userid.parse::<u64>().unwrap_or(0),
    });

    log::info!("[KugouGuessLike] body={}", body);

    let json = gateway_request(
        "/v1/app_song_list_offset",
        "POST",
        cookie,
        vec![],
        Some(body),
        Some("fm.service.kugou.com"),
        false,
    )
    .await;

    let fm_result = match json {
        Ok(json) => {
            let data = json.get("data").unwrap_or(&Value::Null);
            let candidates = [
                data.get("info"), data.get("song_list"), data.get("songs"),
                data.get("list"), data.get("songlist"),
                json.get("info"), json.get("list"),
            ];
            let mut found = Vec::new();
            for candidate in &candidates {
                if let Some(arr) = candidate.and_then(|v| v.as_array()) {
                    if !arr.is_empty() {
                        found = arr
                            .iter()
                            .map(|item| map_playlist_track(item))
                            .filter(|s| !s.name.is_empty() && (s.hash.as_deref().unwrap_or("").is_empty() == false || !s.id.is_empty()))
                            .take(limit as usize)
                            .collect();
                        if !found.is_empty() {
                            break;
                        }
                    }
                }
            }
            found
        }
        Err(e) => {
            log::warn!("[KugouGuessLike] fm: {e}");
            Vec::new()
        }
    };

    // FM 推荐有结果, 缓存并返回
    if !fm_result.is_empty() {
        let mut cache = GUESS_CACHE.write().await;
        *cache = (now_ms(), fm_result.clone());
        return Ok(fm_result);
    }

    // FM 推荐为空或失败, 降级到排行榜歌曲 (酷狗TOP500)
    log::info!("[KugouGuessLike] FM empty/failed, falling back to rank songs");
    match get_rank_songs(cookie, "24971", limit).await {
        Ok(rank_songs) if !rank_songs.is_empty() => {
            log::info!("[KugouGuessLike] got {} songs from rank fallback", rank_songs.len());
            let mut cache = GUESS_CACHE.write().await;
            *cache = (now_ms(), rank_songs.clone());
            Ok(rank_songs)
        }
        _ => {
            // 排行榜也失败, 尝试飙升榜
            log::info!("[KugouGuessLike] rank 24971 empty, trying rank 8888");
            match get_rank_songs(cookie, "8888", limit).await {
                Ok(rank_songs) if !rank_songs.is_empty() => {
                    let mut cache = GUESS_CACHE.write().await;
                    *cache = (now_ms(), rank_songs.clone());
                    Ok(rank_songs)
                }
                _ => Ok(Vec::new()),
            }
        }
    }
}

// ============================================================
//  VIP 探测 (对照 fetchKugouVipInfo + normalizeKugouVipPayloadV2)
// ============================================================

/// VIP 信号 key 集合 (对照 KUGOU_VIP_SIGNAL_KEYS)
const VIP_SIGNAL_KEYS: &[&str] = &[
    "vip", "viptype", "isvip", "viplevel", "vipstatus", "membertype", "memberlevel",
    "musicviplevel", "mtype", "ptype", "vipytype", "unionviptype", "userviptype",
];

/// SVIP 信号 key 集合 (对照 KUGOU_SVIP_SIGNAL_KEYS)
const SVIP_SIGNAL_KEYS: &[&str] = &[
    "svip", "sviptype", "issvip", "sviplevel", "svipstatus", "supervip",
    "superviplevel", "superviptype", "luxuryviptype", "vipluxurytype",
];

/// VIP 过期时间 key 集合 (对照 KUGOU_VIP_EXPIRY_KEYS)
const VIP_EXPIRY_KEYS: &[&str] = &[
    "vipendtime", "vipexpiretime", "vipexpire", "musicvipendtime",
    "svipendtime", "svipexpiretime", "supervipendtime", "luxuryvipendtime",
];

/// 规范化 key (小写 + 移除非字母数字)
fn normalize_membership_key(key: &str) -> String {
    key.to_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect()
}

/// 检查对象是否有会员信号 (对照 kugouObjectHasMembershipSignal)
fn object_has_membership_signal(obj: &Value) -> bool {
    object_has_membership_signal_with_web(obj, false)
}

/// 检查对象是否包含会员信号 (可选 web role 字段)
fn object_has_membership_signal_with_web(obj: &Value, allow_web_signals: bool) -> bool {
    if !obj.is_object() || obj.is_array() {
        return false;
    }
    // 规范化 web role key 列表
    let web_role_normalized: Vec<String> = KUGOU_WEB_ROLE_KEYS.iter()
        .map(|k| normalize_membership_key(k))
        .collect();
    if let Some(map) = obj.as_object() {
        for (key, val) in map {
            // 跳过嵌套对象/数组 (与 JS 逻辑一致)
            if val.is_object() || val.is_array() {
                continue;
            }
            let normalized = normalize_membership_key(key);
            if VIP_SIGNAL_KEYS.contains(&normalized.as_str())
                || SVIP_SIGNAL_KEYS.contains(&normalized.as_str())
                || VIP_EXPIRY_KEYS.contains(&normalized.as_str())
                || (allow_web_signals && web_role_normalized.contains(&normalized))
            {
                return true;
            }
        }
    }
    false
}

/// 从对象列表中查找第一个正数的 key 值 (对照 firstPositiveKugouNumber)
fn first_positive_kugou_number(objects: &[&Value], keys: &[&str]) -> f64 {
    for obj in objects {
        if obj.is_null() || !obj.is_object() {
            continue;
        }
        for key in keys {
            if let Some(raw) = obj.get(*key) {
                if let Some(n) = raw.as_f64() {
                    if n > 0.0 && n.is_finite() {
                        return n;
                    }
                }
                if let Some(b) = raw.as_bool() {
                    if b {
                        return 1.0;
                    }
                }
                if let Some(s) = raw.as_str() {
                    let lower = s.trim().to_lowercase();
                    if matches!(lower.as_str(), "true" | "yes" | "active" | "valid" | "enabled" | "vip" | "svip" | "premium" | "member") {
                        return 1.0;
                    }
                    if let Ok(n) = s.parse::<f64>() {
                        if n > 0.0 && n.is_finite() {
                            return n;
                        }
                    }
                }
            }
        }
    }
    0.0
}

/// 递归收集 JSON 中的所有对象 (对照 collectKugouVipObjects)
fn collect_vip_objects(value: &Value, out: &mut Vec<Value>, depth: u32) {
    if depth > 6 || value.is_null() {
        return;
    }
    if let Some(arr) = value.as_array() {
        for item in arr {
            collect_vip_objects(item, out, depth + 1);
        }
        return;
    }
    if value.is_object() {
        out.push(value.clone());
        if let Some(map) = value.as_object() {
            for (_, child) in map {
                if child.is_object() {
                    collect_vip_objects(child, out, depth + 1);
                }
            }
        }
    }
}

/// 时间状态检查 (对照 kugouTimeState)
fn kugou_time_state(objects: &[&Value], keys: &[&str]) -> (bool, bool) {
    let now_sec = now_secs();
    let now_ms = now_ms();
    let mut present = false;
    let mut future = false;
    for obj in objects {
        if obj.is_null() || !obj.is_object() {
            continue;
        }
        for key in keys {
            if let Some(val) = obj.get(*key) {
                let value = val.as_f64().unwrap_or(0.0);
                if value <= 0.0 || !value.is_finite() {
                    continue;
                }
                if value > 100000000000.0 {
                    // 毫秒时间戳
                    present = true;
                    if value > now_ms as f64 {
                        future = true;
                    }
                } else if value > 1000000000.0 {
                    // 秒时间戳
                    present = true;
                    if value > now_sec as f64 {
                        future = true;
                    }
                }
            }
        }
    }
    (present, future)
}

/// VIP 规范化结果
#[derive(Debug, Clone, Default)]
struct VipInfo {
    vip_type: f64,
    svip_type: f64,
    vip_level: String,
    is_vip: bool,
    is_svip: bool,
    membership_known: bool,
    web_role: f64,
    web_role_known: bool,
}

/// 规范化 VIP 负载 (对照 normalizeKugouVipPayloadV2)
/// 支持 web roleinfo payload 和 gateway API payload
fn normalize_vip_payload(payload: &Value, fallback: &KugouAuth) -> VipInfo {
    let membership_origin = payload.get("__kugouMembershipOrigin")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let is_web_role_payload = membership_origin == "kugou-web-roleinfo";

    let data = payload.get("data")
        .or_else(|| payload.get("result"))
        .or_else(|| payload.get("vip"))
        .unwrap_or(payload);

    let expected_userid: String = fallback.userid.chars().filter(|c| c.is_ascii_digit()).collect();

    // 收集所有对象
    let mut all_objects = Vec::new();
    collect_vip_objects(data, &mut all_objects, 0);

    // 按 userid 过滤
    let payload_objects: Vec<&Value> = all_objects.iter().filter(|obj| {
        if expected_userid.is_empty() {
            return true;
        }
        let obj_userid: String = obj.get("userid")
            .or_else(|| obj.get("user_id"))
            .or_else(|| obj.get("userId"))
            .or_else(|| obj.get("uid"))
            .or_else(|| obj.get("KugooID"))
            .map(|v| match v {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                _ => String::new(),
            })
            .unwrap_or_default()
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect();
        obj_userid.is_empty() || obj_userid == expected_userid
    }).collect();

    if payload_objects.is_empty() {
        return VipInfo::default();
    }

    // 检查 web role 字段 (仅在 web roleinfo payload 中启用)
    let web_role_state = if is_web_role_payload {
        first_positive_kugou_number(&payload_objects, KUGOU_WEB_ROLE_KEYS)
    } else {
        0.0
    };
    let web_role_known = web_role_state > 0.0 && is_web_role_payload;

    // Web role 检测: 优先使用 roleinfo 提供的 role 映射
    if web_role_known {
        let web_role = web_role_state as i64;
        let role_is_vip = KUGOU_WEB_VIP_ROLES.contains(&web_role);
        let role_is_svip = KUGOU_WEB_SVIP_ROLES.contains(&web_role);

        // 检查过期时间
        let (vip_present, vip_future) = kugou_time_state(&payload_objects, &[
            "rawVipEndTime", "raw_vip_end_time", "rawvipendtime",
            "vip_end_time", "vipEndTime", "vip_expire_time", "vipExpireTime",
        ]);
        let role_expiry_current = !vip_present || (vip_present && vip_future);

        let is_svip = role_is_svip && role_expiry_current;
        let is_vip = (role_is_vip || role_is_svip) && role_expiry_current;
        let vip_level = if is_svip { "svip" } else if is_vip { "vip" } else { "none" };

        return VipInfo {
            vip_type: if is_vip { web_role as f64 } else { 0.0 },
            svip_type: if is_svip { web_role as f64 } else { 0.0 },
            vip_level: vip_level.to_string(),
            is_vip,
            is_svip,
            membership_known: true,
            web_role: web_role as f64,
            web_role_known: true,
        };
    }

    // 常规 gateway API 检测
    let allow_web_in_signal = is_web_role_payload;
    let api_membership_known = payload_objects.iter().any(|obj| object_has_membership_signal_with_web(obj, allow_web_in_signal));
    if !api_membership_known {
        return VipInfo::default();
    }

    let vip_type = first_positive_kugou_number(&payload_objects, &[
        "vipType", "vip_type", "VIPType", "isVIP", "isVip", "is_vip", "vip_level", "vipLevel",
        "music_vip_level", "musicVipLevel", "m_type", "p_type", "vip_y_type", "union_vip_type",
        "user_vip_type", "vip_status", "member_type", "member_level", "vip",
    ]);
    let svip_type = first_positive_kugou_number(&payload_objects, &[
        "svipType", "svip_type", "SVIPType", "isSVIP", "isSvip", "is_svip", "superVip", "super_vip",
        "superVipLevel", "super_vip_level", "super_vip_type", "luxury_vip_type", "vip_luxury_type",
        "svip_level", "svip_status", "svip",
    ]);
    let (vip_present, vip_future) = kugou_time_state(&payload_objects, &[
        "vip_end_time", "vipEndTime", "vip_expire_time", "vipExpireTime", "vip_expire", "vipExpire",
        "music_vip_end_time", "musicVipEndTime",
    ]);
    let (svip_present, svip_future) = kugou_time_state(&payload_objects, &[
        "svip_end_time", "svipEndTime", "svip_expire_time", "svipExpireTime",
        "super_vip_end_time", "superVipEndTime", "luxury_vip_end_time", "luxuryVipEndTime",
    ]);

    let is_svip = svip_future || (svip_type > 0.0 && !svip_present)
        || payload_objects.iter().any(|obj| obj.get("isSvip").and_then(|v| v.as_bool()).unwrap_or(false) && !svip_present);
    let is_vip = is_svip || vip_future || (vip_type > 0.0 && !vip_present)
        || payload_objects.iter().any(|obj| obj.get("isVip").and_then(|v| v.as_bool()).unwrap_or(false) && !vip_present);
    let vip_level = if is_svip { "svip" } else if is_vip { "vip" } else { "none" };

    VipInfo {
        vip_type: if is_svip { vip_type.max(svip_type) } else { vip_type },
        svip_type,
        vip_level: vip_level.to_string(),
        is_vip,
        is_svip,
        membership_known: api_membership_known,
        web_role: 0.0,
        web_role_known: false,
    }
}

/// 获取酷狗 Web VIP 信息 (对照 fetchKugouWebVipInfo)
/// 调用 https://vip.kugou.com/recharge/roleinfo — 最权威的 VIP 来源
async fn fetch_kugou_web_vip_info(cookie: &str, auth: &KugouAuth) -> Option<Value> {
    if !auth.logged_in || cookie.is_empty() {
        return None;
    }
    let cookie_header = build_request_cookie(cookie);
    let url = format!("{}?n={}", KUGOU_VIP_ROLEINFO_URL, now_ms());

    let client = super::qqmusic::build_client_with_timeout(2500);
    let resp = client
        .get(&url)
        .header("Accept", "application/json, text/javascript, */*; q=0.01")
        .header("Referer", "https://vip.kugou.com/")
        .header("User-Agent", KUGOU_H5_UA)
        .header("X-Requested-With", "XMLHttpRequest")
        .header("Cookie", &cookie_header)
        .send()
        .await;

    match resp {
        Ok(resp) => {
            let status = resp.status();
            let body: Value = match resp.json().await {
                Ok(v) => v,
                Err(_) => return None,
            };
            if !status.is_success() {
                log::info!("[KugouWebVip] HTTP {} from roleinfo", status.as_u16());
                return None;
            }
            // 检查是否有错误码
            let data = body.get("data").unwrap_or_else(|| &body);
            let has_error = [body.get("errno"), body.get("error_code"), body.get("errorCode"), body.get("errcode"),
                data.get("errno"), data.get("error_code"), data.get("errorCode"), data.get("errcode")]
                .iter()
                .any(|v| v.and_then(|n| n.as_i64()).map(|n| n > 0).unwrap_or(false));
            let explicitly_failed = body.get("success").and_then(|v| v.as_bool()) == Some(false)
                || body.get("status").and_then(|v| match v {
                    Value::Number(n) => Some(n.as_f64()? == 0.0),
                    Value::String(s) => Some(s == "0" || s == "-1" || s.eq_ignore_ascii_case("false") || s.eq_ignore_ascii_case("fail") || s.eq_ignore_ascii_case("failed") || s.eq_ignore_ascii_case("error")),
                    _ => None,
                }).unwrap_or(false)
                || body.get("error").and_then(|v| v.as_str()).map(|s| !s.trim().is_empty()).unwrap_or(false);
            if has_error || explicitly_failed {
                log::info!("[KugouWebVip] roleinfo returned error/unsuccessful status");
                return None;
            }
            // 包装为带 origin 标记的 payload (与 normalizeKugouWebRoleInfoPayload 一致)
            let role_data = body.get("data").unwrap_or(&body);
            Some(json!({
                "data": role_data,
                "__kugouMembershipOrigin": "kugou-web-roleinfo",
            }))
        }
        Err(e) => {
            log::info!("[KugouWebVip] request failed: {}", e);
            None
        }
    }
}

/// 获取酷狗 VIP 信息 (对照 fetchKugouVipInfo)
/// 先尝试 web roleinfo，然后降级到 gateway 探针
async fn fetch_kugou_vip_info(cookie: &str, auth: &KugouAuth) -> Option<Value> {
    if !auth.logged_in {
        return None;
    }

    // 第一步: 尝试 web roleinfo (最权威的来源)
    if let Some(web_result) = fetch_kugou_web_vip_info(cookie, auth).await {
        let web_vip = normalize_vip_payload(&web_result, auth);
        if web_vip.membership_known {
            log::info!("[KugouVip] web roleinfo succeeded: is_vip={}, is_svip={}, web_role={}",
                web_vip.is_vip, web_vip.is_svip, web_vip.web_role);
            return Some(web_result);
        }
        log::info!("[KugouVip] web roleinfo unknown, falling back to gateway probes");
    }

    // 如果没有 playback token，到此为止
    if !auth.playback_ready {
        return None;
    }

    let cookie = cookie.to_string();
    let auth_userid = auth.userid.clone();
    let auth_clone = auth.clone();

    let mut handles: Vec<tokio::task::JoinHandle<Option<Value>>> = Vec::new();

    // 辅助宏：生成一个 gateway 探测任务
    macro_rules! spawn_gw {
        ($path:expr, $params:expr, $base:expr) => {{
            let c = cookie.clone();
            let a = auth_clone.clone();
            let p = $path.to_string();
            let extra: Vec<(String, String)> = $params.iter().map(|(k, v): &(&str, &str)| (k.to_string(), v.to_string())).collect();
            let bu: Option<String> = $base.map(|s: &str| s.to_string());
            handles.push(tokio::spawn(async move {
                log::info!("[KugouVip] trying {} with params: {:?}, base: {:?}", p, extra, bu);
                match gateway_request_full(&p, "GET", &c, extra, None, None, false, Some("https://vip.kugou.com/"), bu.as_deref()).await {
                    Ok(json) => {
                        if normalize_vip_payload(&json, &a).membership_known {
                            Some(json)
                        } else { None }
                    }
                    Err(e) => { log::info!("[KugouVip] {} failed: {}", p, e); None }
                }
            }));
        }};
    }

    // 辅助宏：生成一个 H5 gateway 探测任务
    macro_rules! spawn_h5 {
        ($path:expr) => {{
            let c = cookie.clone();
            let a = auth_clone.clone();
            let p = $path.to_string();
            handles.push(tokio::spawn(async move {
                match h5_gateway_request(&p, "GET", &c, vec![("busi_type".into(), "concept".into())], None, None).await {
                    Ok(json) => {
                        if normalize_vip_payload(&json, &a).membership_known {
                            log::info!("[KugouVip] H5 gateway {} succeeded", p);
                            Some(json)
                        } else { None }
                    }
                    Err(e) => { log::info!("[KugouVip] H5 gateway {} failed: {}", p, e); None }
                }
            }));
        }};
    }

    // Gateway 端点 (并行发起)
    spawn_gw!("/v1/get_union_vip", vec![("busi_type", "concept")], None::<&str>);
    spawn_gw!("/v1/vipuser_sub", vec![("busi_type", "concept")], None::<&str>);
    spawn_gw!("/kugouvip/v2/batch_union_vipinfo", vec![("busi_type", "concept"), ("userids", &auth_userid)], None::<&str>);
    spawn_gw!("/kugouvip/v1/batch_union_vipinfo", vec![("busi_type", "concept"), ("userids", &auth_userid)], None::<&str>);
    spawn_gw!("/mobile/vipinfo", vec![("plat", "0")], None::<&str>);
    spawn_gw!("/v1/get_union_vip", vec![("busi_type", "concept")], Some("https://kugouvip.kugou.com"));

    // H5 Gateway 端点
    spawn_h5!("/v1/get_union_vip");
    spawn_h5!("/v1/vipuser_sub");

    // mobilecdn web API
    {
        let c = cookie.clone();
        let a = auth_clone.clone();
        let uid = auth_userid.clone();
        let token = auth.token.clone();
        handles.push(tokio::spawn(async move {
            let cookie_header = build_request_cookie(&c);
            let mobile_url = format!(
                "http://mobilecdn.kugou.com/api/v3/user/info?userid={}&token={}",
                uid, token
            );
            match request_json(
                &mobile_url,
                reqwest::Method::GET,
                &[
                    (USER_AGENT.as_str(), KUGOU_H5_UA),
                    (REFERER.as_str(), "http://m.kugou.com/"),
                    (COOKIE.as_str(), &cookie_header),
                ],
                None,
            ).await {
                Ok(json) => {
                    if normalize_vip_payload(&json, &a).membership_known {
                        log::info!("[KugouVip] mobilecdn web API succeeded");
                        Some(json)
                    } else { None }
                }
                Err(e) => { log::info!("[KugouVip] mobilecdn web API failed: {}", e); None }
            }
        }));
    }

    // 等待任意一个成功返回
    for handle in handles {
        match handle.await {
            Ok(Some(json)) => return Some(json),
            _ => continue,
        }
    }

    None
}

/// 获取登录信息 (对照 getKugouLoginInfo)
pub async fn login_info(cookie: &str) -> Result<LoginInfo, String> {
    let auth = extract_kugou_auth(cookie);

    // 从歌单获取昵称/头像
    let (nickname, avatar) = if auth.nickname.is_empty() || auth.avatar.is_empty() {
        if auth.playback_ready {
            match fetch_profile_from_playlists(cookie, &auth).await {
                Ok((nick, av)) => (nick, av),
                Err(_) => (String::new(), String::new()),
            }
        } else {
            (String::new(), String::new())
        }
    } else {
        (String::new(), String::new())
    };

    let final_nickname = if !auth.nickname.is_empty() {
        auth.nickname.clone()
    } else if !nickname.is_empty() {
        nickname
    } else if auth.logged_in {
        format!("酷狗 {}", if auth.userid.is_empty() { "用户" } else { &auth.userid })
    } else {
        "酷狗音乐".to_string()
    };
    let final_avatar = if !auth.avatar.is_empty() { auth.avatar.clone() } else { avatar };

    // VIP 探测 (对照 fetchKugouVipInfo)
    // API探测失败时回退到Cookie中的VIP信息
    // 加 5 秒超时，避免网络不通时长时间阻塞登录状态返回
    let vip = if auth.playback_ready {
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            fetch_kugou_vip_info(cookie, &auth),
        ).await
        {
            Ok(Some(vip_json)) => normalize_vip_payload(&vip_json, &auth),
            _ => {
                log::info!("[KugouVip] API probe failed or timed out, falling back to cookie-based VIP: is_vip={}, is_svip={}, vip_level={}",
                    auth.is_vip, auth.is_svip, auth.vip_level);
                VipInfo {
                    vip_type: auth.vip_type as f64,
                    svip_type: auth.svip_type as f64,
                    vip_level: auth.vip_level.clone(),
                    is_vip: auth.is_vip,
                    is_svip: auth.is_svip,
                    membership_known: auth.is_vip || auth.is_svip,
                    web_role: 0.0,
                    web_role_known: false,
                }
            }
        }
    } else {
        VipInfo::default()
    };

    Ok(LoginInfo {
        provider: "kugou".into(),
        // 登录态必须包含有效 token（token 过期/缺失视为未登录），
        // 否则长时间不用后 token 失效但界面仍显示已登录、歌单却拉不到
        logged_in: auth.playback_ready,
        user_id: auth.userid,
        nickname: final_nickname,
        avatar: final_avatar,
        vip_type: vip.vip_type as i32,
        vip_level: vip.vip_level,
        is_vip: vip.is_vip,
        is_svip: vip.is_svip,
    })
}

/// 从歌单列表获取用户资料 (对照 fetchKugouProfileFromPlaylists)
async fn fetch_profile_from_playlists(cookie: &str, auth: &KugouAuth) -> Result<(String, String), String> {
    let body = json!({
        "userid": auth.userid.parse::<u64>().unwrap_or(0),
        "token": auth.token,
        "total_ver": 979,
        "type": 2,
        "page": 1,
        "pagesize": 20,
    });

    let json = h5_gateway_request(
        "/v7/get_all_list",
        "POST",
        cookie,
        vec![("plat".into(), "1".into())],
        Some(body),
        Some("cloudlist.service.kugou.com"),
    )
    .await?;

    let data = json.get("data").unwrap_or(&Value::Null);
    let lists = extract_gateway_playlist_lists(data);
    Ok(pick_profile_from_lists(&lists, auth))
}

// ============================================================
//  排行榜 API (官方榜单) — 使用 mobilecdn 公开 API
// ============================================================

const KUGOU_MOBILE_CDN: &str = "http://mobilecdn.kugou.com";

/// 酷狗官方榜单预设 (常用榜单 ID, 作为 fallback)
const KUGOU_RANK_PRESETS: &[(&str, &str)] = &[
    ("8888", "飙升榜"),
    ("23784", "热歌榜"),
    ("6666", "新歌榜"),
    ("31313", "网络红歌榜"),
    ("24971", "酷狗TOP500"),
    ("11379452", "华语新歌榜"),
];

/// 获取酷狗排行榜列表 (使用 mobilecdn API)
pub async fn get_rank_list(cookie: &str) -> Result<Vec<Playlist>, String> {
    let cookie_header = build_request_cookie(cookie);

    // 使用 mobilecdn API 获取排行榜列表 (公开 API, 不需要登录)
    let url = format!("{}/api/v3/rank/list?version=9108&plat=0&withsong=1", KUGOU_MOBILE_CDN);
    log::info!("[KugouRank] fetching rank list from mobilecdn");

    let json = request_json(
        &url,
        reqwest::Method::GET,
        &[
            (USER_AGENT.as_str(), KUGOU_H5_UA),
            (REFERER.as_str(), "http://m.kugou.com/rank/list"),
            (COOKIE.as_str(), &cookie_header),
        ],
        None,
    ).await;

    match json {
        Ok(json) => {
            let data = json.get("data").unwrap_or(&Value::Null);
            let info = data.get("info").and_then(|v| v.as_array());

            if let Some(arr) = info {
                if !arr.is_empty() {
                    let playlists: Vec<Playlist> = arr.iter().filter_map(|item| {
                        let id = item.get("rankid")
                            .or_else(|| item.get("rank_id"))
                            .or_else(|| item.get("id"))
                            .map(|v| match v {
                                Value::String(s) => s.clone(),
                                Value::Number(n) => n.to_string(),
                                _ => String::new(),
                            })
                            .unwrap_or_default();
                        let name = item.get("rankname")
                            .or_else(|| item.get("rank_name"))
                            .or_else(|| item.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if id.is_empty() || name.is_empty() {
                            return None;
                        }
                        let cover_raw = item.get("imgurl")
                            .or_else(|| item.get("bannerurl_950"))
                            .or_else(|| item.get("bannerurl"))
                            .or_else(|| item.get("img"))
                            .or_else(|| item.get("pic"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let track_count = item.get("song_num")
                            .or_else(|| item.get("count"))
                            .or_else(|| item.get("total"))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as u32;
                        Some(Playlist {
                            provider: "kugou".into(),
                            id,
                            name: strip_html(name),
                            cover: kugou_cover_url(cover_raw, 240),
                            track_count,
                            creator: "酷狗音乐".into(),
                            subscribed: false,
                        })
                    }).collect();
                    if !playlists.is_empty() {
                        log::info!("[KugouRank] got {} rank lists from mobilecdn", playlists.len());
                        return Ok(playlists);
                    }
                }
            }
            log::warn!("[KugouRank] mobilecdn returned empty data");
        }
        Err(e) => {
            log::warn!("[KugouRank] mobilecdn failed: {}", e);
        }
    }

    // mobilecdn API 失败, 返回预设榜单
    log::info!("[KugouRank] falling back to preset rank list");
    Ok(KUGOU_RANK_PRESETS.iter().map(|(id, name)| Playlist {
        provider: "kugou".into(),
        id: id.to_string(),
        name: name.to_string(),
        cover: String::new(),
        track_count: 0,
        creator: "酷狗音乐".into(),
        subscribed: false,
    }).collect())
}

/// 获取酷狗排行榜歌曲 (使用 mobilecdn API)
pub async fn get_rank_songs(cookie: &str, rank_id: &str, limit: u32) -> Result<Vec<Song>, String> {
    let cookie_header = build_request_cookie(cookie);
    let pagesize = limit.clamp(1, 50);

    // 使用 mobilecdn API 获取排行榜歌曲 (公开 API)
    let url = format!(
        "{}/api/v3/rank/song?version=9108&rankid={}&page=1&pagesize={}&plat=0",
        KUGOU_MOBILE_CDN, rank_id, pagesize
    );
    log::info!("[KugouRank] fetching songs for rank {} from mobilecdn", rank_id);

    let json = request_json(
        &url,
        reqwest::Method::GET,
        &[
            (USER_AGENT.as_str(), KUGOU_H5_UA),
            (REFERER.as_str(), "http://m.kugou.com/rank/list"),
            (COOKIE.as_str(), &cookie_header),
        ],
        None,
    ).await;

    match json {
        Ok(json) => {
            let data = json.get("data").unwrap_or(&Value::Null);
            let songs_arr = data.get("info")
                .and_then(|v| v.as_array());

            if let Some(arr) = songs_arr {
                if !arr.is_empty() {
                    let songs: Vec<Song> = arr.iter()
                        .map(|item| map_playlist_track(item))
                        .filter(|s| !s.name.is_empty() && (s.hash.as_deref().unwrap_or("").is_empty() == false || !s.id.is_empty()))
                        .take(limit as usize)
                        .collect();
                    if !songs.is_empty() {
                        log::info!("[KugouRank] got {} songs from rank {}", songs.len(), rank_id);
                        return Ok(songs);
                    }
                }
            }
            log::warn!("[KugouRank] mobilecdn returned empty songs for rank {}", rank_id);
        }
        Err(e) => {
            log::warn!("[KugouRank] mobilecdn failed: {}", e);
        }
    }

    Ok(Vec::new())
}

// ============================================================
//  点赞/喜欢功能 (对照 handleKugouLikeToggle / handleKugouLikeCheck)
// ============================================================

/// 收藏歌单 ID 缓存 (对照 kugouFavoriteListCache, 5 分钟 TTL)
struct FavoriteListCache {
    list_id: String,
    userid: String,
    at: u64,
}
static FAVORITE_LIST_CACHE: tokio::sync::RwLock<FavoriteListCache> =
    tokio::sync::RwLock::const_new(FavoriteListCache {
        list_id: String::new(),
        userid: String::new(),
        at: 0,
    });

/// fileId-by-hash 缓存 (对照 kugouLikeFileIdByHash)
/// 避免取消喜欢时反复拉取歌单查找 fileId
static FILE_ID_CACHE: std::sync::OnceLock<tokio::sync::RwLock<std::collections::HashMap<String, String>>> = std::sync::OnceLock::new();

/// 获取 fileId 缓存实例
async fn file_id_cache() -> &'static tokio::sync::RwLock<std::collections::HashMap<String, String>> {
    FILE_ID_CACHE.get_or_init(|| tokio::sync::RwLock::new(std::collections::HashMap::new()))
}

/// 判断是否为"我喜欢"歌单名称
fn is_favorite_playlist_name(name: &str) -> bool {
    let n = name.trim();
    n.contains("我喜欢") || n.contains("我的收藏")
    || n.to_lowercase().contains("favorite")
    || n.to_lowercase().contains("liked")
}

/// 判断是否为主要的"我喜欢"歌单名称
fn is_primary_favorite_playlist_name(name: &str) -> bool {
    let n = name.trim().to_lowercase();
    n.contains("我喜欢") || n.contains("liked music") || n.contains("my favorites")
}

/// 从歌单列表中找到"我喜欢"歌单 (对照 pickKugouFavoritePlaylist)
fn pick_favorite_playlist(lists: &[Value]) -> Option<&Value> {
    // 优先匹配 "我喜欢" / "liked music" / "my favorites"
    let fav = lists.iter().find(|item| {
        let name = item.get("name")
            .or_else(|| item.get("listname"))
            .or_else(|| item.get("specialname"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        is_primary_favorite_playlist_name(name)
    });
    if fav.is_some() { return fav; }

    // 其次匹配 type=0 且名字包含 "我喜欢"
    let fav = lists.iter().find(|item| {
        let item_type = item.get("type").and_then(|v| v.as_u64()).unwrap_or(999);
        let name = item.get("name")
            .or_else(|| item.get("listname"))
            .or_else(|| item.get("specialname"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        item_type == 0 && is_primary_favorite_playlist_name(name)
    });
    if fav.is_some() { return fav; }

    // 再次匹配任何收藏类名称
    let fav = lists.iter().find(|item| {
        let name = item.get("name")
            .or_else(|| item.get("listname"))
            .or_else(|| item.get("specialname"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        is_favorite_playlist_name(name)
    });
    if fav.is_some() { return fav; }

    // 最后匹配 is_default=1 的歌单
    lists.iter().find(|item| {
        item.get("is_default").and_then(|v| v.as_u64()).unwrap_or(0) == 1
        || item.get("default").and_then(|v| v.as_u64()).unwrap_or(0) == 1
    })
}

/// 解析收藏歌单 ID (对照 resolveKugouFavoriteListId)
async fn resolve_favorite_list_id(cookie: &str) -> String {
    let auth = extract_kugou_auth(cookie);
    if !auth.playback_ready {
        return String::new();
    }

    // 检查缓存 (5 分钟 TTL, 对照 kugouFavoriteListCache)
    {
        let cache = FAVORITE_LIST_CACHE.read().await;
        if !cache.list_id.is_empty()
            && cache.userid == auth.userid
            && now_ms().saturating_sub(cache.at) < 300_000
        {
            return cache.list_id.clone();
        }
    }

    let body = json!({
        "userid": auth.userid.parse::<u64>().unwrap_or(0),
        "token": auth.token,
        "total_ver": 979,
        "type": 2,
        "page": 1,
        "pagesize": 50,
    });

    let json = match h5_gateway_request(
        "/v7/get_all_list",
        "POST",
        cookie,
        vec![("plat".into(), "1".into())],
        Some(body),
        Some("cloudlist.service.kugou.com"),
    ).await {
        Ok(j) => j,
        Err(e) => {
            log::warn!("[KugouLike] resolve_favorite_list_id failed: {}", e);
            return String::new();
        }
    };

    let data = json.get("data").unwrap_or(&Value::Null);
    let lists = extract_gateway_playlist_lists(data);

    if let Some(fav) = pick_favorite_playlist(&lists) {
        let list_id = fav.get("list_create_listid")
            .or_else(|| fav.get("listid"))
            .or_else(|| fav.get("list_id"))
            .or_else(|| fav.get("global_collection_id"))
            .or_else(|| fav.get("id"))
            .map(|v| match v {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                _ => String::new(),
            })
            .unwrap_or_default();
        let list_id = parse_kugou_list_id(&list_id);

        // 写入缓存
        if !list_id.is_empty() {
            let mut cache = FAVORITE_LIST_CACHE.write().await;
            cache.list_id = list_id.clone();
            cache.userid = auth.userid.clone();
            cache.at = now_ms();
        }

        return list_id;
    }

    String::new()
}

/// 从 Song 结构解析 mixsongid (对照 resolveKugouAlbumAudioId)
/// Song 中 album_audio_id 已由 map_search_item/map_playlist_track 从 MixSongID 等字段提取
fn resolve_song_mixsongid(song: &Song) -> u64 {
    if let Some(aid) = &song.album_audio_id {
        let text = aid.trim();
        if !text.is_empty() && text.chars().all(|c| c.is_ascii_digit()) {
            return text.parse::<u64>().unwrap_or(0);
        }
    }
    0
}

/// 构建歌曲资源对象 (对照 buildKugouSongResource)
fn build_song_resource(song: &Song) -> Value {
    // 对齐 JS: song.hash || song.fileHash || song.id — 空字符串视为 falsy
    let hash = song.hash.as_deref()
        .filter(|h| !h.is_empty())
        .unwrap_or(&song.id)
        .trim()
        .to_lowercase();
    let album_id = song.album_id.as_deref().unwrap_or("").parse::<u64>().unwrap_or(0);
    let mixsongid = resolve_song_mixsongid(song);
    let duration_ms = if song.duration > 1000 { song.duration } else { song.duration * 1000 };

    log::info!(
        "[KugouLike] build_song_resource: hash={}, album_id={}, mixsongid={}, duration_ms={}, name={}",
        hash, album_id, mixsongid, duration_ms, song.name
    );

    json!({
        "number": 1,
        "name": song.name,
        "hash": hash,
        "size": 0,
        "sort": 0,
        "timelen": duration_ms,
        "bitrate": 0,
        "album_id": album_id,
        "mixsongid": mixsongid,
    })
}

/// 添加歌曲到歌单 (对照 handleKugouAddSongToList)
async fn add_song_to_list(list_id: &str, song: &Song, cookie: &str) -> Result<bool, String> {
    let auth = extract_kugou_auth(cookie);
    if !auth.playback_ready {
        return Err("KUGOU_AUTH_REQUIRED".into());
    }

    let target_list_id = if list_id.is_empty() {
        resolve_favorite_list_id(cookie).await
    } else {
        list_id.to_string()
    };
    if target_list_id.is_empty() {
        return Err("KUGOU_FAVORITE_LIST_NOT_FOUND".into());
    }

    let resource = build_song_resource(song);
    // listid 可能是数字或字符串, 尝试解析为数字, 失败则用字符串
    let listid_value = target_list_id.parse::<u64>()
        .map(|n| json!(n))
        .unwrap_or_else(|_| json!(target_list_id));
    let body = json!({
        "userid": auth.userid.parse::<u64>().unwrap_or(0),
        "token": auth.token,
        "listid": listid_value,
        "list_ver": 0,
        "type": 0,
        "slow_upload": 1,
        "scene": "false;null",
        "data": [resource],
    });

    let mut extra_params = vec![("last_time".into(), format!("{}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()))];
    extra_params.push(("last_area".into(), "gztx".into()));
    extra_params.push(("userid".into(), auth.userid.clone()));
    extra_params.push(("token".into(), auth.token.clone()));

    log::info!(
        "[KugouLike] add_song_to_list: list_id={}, song_hash={}, song_id={}, song_name={}",
        target_list_id,
        song.hash.as_deref().unwrap_or(""),
        song.id,
        song.name
    );

    let result = h5_gateway_request(
        "/v6/add_song",
        "POST",
        cookie,
        extra_params,
        Some(body),
        Some("cloudlist.service.kugou.com"),
    ).await;

    match &result {
        Ok(json) => {
            let status = json.get("status").map(|v| v.to_string()).unwrap_or_default();
            let err_code = json.get("err_code").map(|v| v.to_string()).unwrap_or_default();
            log::info!("[KugouLike] add_song SUCCESS: status={}, err_code={}", status, err_code);
        }
        Err(e) => {
            log::warn!("[KugouLike] add_song FAILED: {}", e);
        }
    }
    result?;

    // 失效 fileId 缓存 (对照 kugouLikeFileIdByHash.delete)
    // 对齐 build_song_resource 的 hash 提取逻辑: 空字符串视为无 hash
    let hash = song.hash.as_deref()
        .filter(|h| !h.is_empty())
        .unwrap_or(&song.id)
        .trim()
        .to_lowercase();
    if !hash.is_empty() {
        let mut cache = file_id_cache().await.write().await;
        cache.remove(&hash);
    }

    Ok(true)
}

/// 从歌单中查找歌曲的 fileId (对照 findKugouFavoriteFileId)
/// 直接调用 H5 gateway 获取原始数据, 从中提取 fileId
/// 使用 FILE_ID_CACHE 缓存避免重复请求 (对照 kugouLikeFileIdByHash)
async fn find_song_file_id(song: &Song, cookie: &str, list_id: &str) -> String {
    let hash = song.hash.as_deref().unwrap_or(&song.id).trim().to_lowercase();
    if hash.is_empty() {
        return String::new();
    }

    // 检查缓存 (对照 kugouLikeFileIdByHash)
    {
        let cache = file_id_cache().await.read().await;
        if let Some(file_id) = cache.get(&hash) {
            return file_id.clone();
        }
    }

    let auth = extract_kugou_auth(cookie);
    if !auth.playback_ready {
        return String::new();
    }

    let target_list_id = if list_id.is_empty() {
        resolve_favorite_list_id(cookie).await
    } else {
        list_id.to_string()
    };
    if target_list_id.is_empty() {
        return String::new();
    }

    // 直接调用 H5 gateway 获取原始数据, 从中提取 fileId
    for page in 1..=6u32 {
        let body = json!({
            "listid": target_list_id.parse::<u64>().unwrap_or(0),
            "userid": auth.userid.parse::<u64>().unwrap_or(0),
            "area_code": 1,
            "show_relate_goods": 0,
            "pagesize": 50,
            "allplatform": 1,
            "show_cover": 1,
            "type": 0,
            "token": auth.token,
            "page": page,
        });

        let json = match h5_gateway_request(
            "/v4/get_list_all_file",
            "POST",
            cookie,
            vec![("plat".into(), "1".into())],
            Some(body),
            Some("cloudlist.service.kugou.com"),
        ).await {
            Ok(j) => j,
            Err(_) => break,
        };

        let data = json.get("data").unwrap_or(&Value::Null);
        let chunk = data.get("info")
            .or_else(|| data.get("songs"))
            .or_else(|| data.get("lists"))
            .or_else(|| data.get("file"))
            .unwrap_or(&Value::Null);
        let chunk_arr: Vec<Value> = if let Some(arr) = chunk.as_array() {
            arr.clone()
        } else if let Some(file_arr) = chunk.get("file").and_then(|v| v.as_array()) {
            file_arr.clone()
        } else {
            Vec::new()
        };

        if chunk_arr.is_empty() { break; }

        for track in &chunk_arr {
            let track_hash = track.get("hash")
                .or_else(|| track.get("FileHash"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_lowercase();
            if track_hash == hash {
                // 从原始数据中提取 fileId
                let file_id = track.get("fileid")
                    .or_else(|| track.get("fileId"))
                    .or_else(|| track.get("file_id"))
                    .or_else(|| track.get("FileId"))
                    .map(|v| match v {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => n.to_string(),
                        _ => String::new(),
                    })
                    .unwrap_or_default();
                if !file_id.is_empty() {
                    // 写入缓存 (对照 kugouLikeFileIdByHash.set)
                    let mut cache = file_id_cache().await.write().await;
                    cache.insert(hash.clone(), file_id.clone());
                    return file_id;
                }
            }
        }

        if chunk_arr.len() < 50 { break; }
    }

    String::new()
}

/// 从歌单中删除歌曲 (对照 handleKugouRemoveSongFromList)
async fn remove_song_from_list(list_id: &str, song: &Song, cookie: &str) -> Result<bool, String> {
    let auth = extract_kugou_auth(cookie);
    if !auth.playback_ready {
        return Err("KUGOU_AUTH_REQUIRED".into());
    }

    let target_list_id = if list_id.is_empty() {
        resolve_favorite_list_id(cookie).await
    } else {
        list_id.to_string()
    };
    if target_list_id.is_empty() {
        return Err("KUGOU_FAVORITE_LIST_NOT_FOUND".into());
    }

    let file_id = find_song_file_id(song, cookie, &target_list_id).await;
    if file_id.is_empty() {
        return Err("KUGOU_SONG_NOT_IN_LIST".into());
    }

    // fileId 可能是数字或字符串, 尝试解析为数字, 失败则用字符串
    let file_id_value = file_id.parse::<u64>()
        .map(|n| json!(n))
        .unwrap_or_else(|_| json!(file_id));

    // listid 也可能是数字或字符串
    let listid_value = target_list_id.parse::<u64>()
        .map(|n| json!(n))
        .unwrap_or_else(|_| json!(target_list_id));

    let body = json!({
        "listid": listid_value,
        "userid": auth.userid.parse::<u64>().unwrap_or(0),
        "token": auth.token,
        "type": 0,
        "list_ver": 0,
        "data": [{ "fileid": file_id_value }],
    });

    h5_gateway_request(
        "/v4/delete_songs",
        "POST",
        cookie,
        vec![],
        Some(body),
        Some("cloudlist.service.kugou.com"),
    ).await?;

    // 失效 fileId 缓存 (对照 kugouLikeFileIdByHash.delete)
    let hash = song.hash.as_deref()
        .filter(|h| !h.is_empty())
        .unwrap_or(&song.id)
        .trim()
        .to_lowercase();
    if !hash.is_empty() {
        let mut cache = file_id_cache().await.write().await;
        cache.remove(&hash);
    }

    Ok(true)
}

/// 切换喜欢状态 (对照 handleKugouLikeToggle)
pub async fn like_toggle(song: &Song, like: bool, cookie: &str) -> Result<bool, String> {
    if like {
        add_song_to_list("", song, cookie).await
    } else {
        remove_song_from_list("", song, cookie).await
    }
}

/// 检查歌曲是否已喜欢 (对照 handleKugouLikeCheck)
/// 返回已喜欢的 hash 集合
pub async fn like_check(hashes: &[String], cookie: &str) -> Result<std::collections::HashMap<String, bool>, String> {
    let auth = extract_kugou_auth(cookie);
    if !auth.playback_ready {
        return Ok(std::collections::HashMap::new());
    }

    let list_id = resolve_favorite_list_id(cookie).await;
    if list_id.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let hash_set: std::collections::HashSet<String> = hashes.iter()
        .map(|h| h.trim().to_lowercase())
        .filter(|h| !h.is_empty())
        .collect();

    if hash_set.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let mut liked = std::collections::HashMap::new();

    // 遍历收藏歌单, 检查哪些 hash 存在
    for page in 1..=6u32 {
        let tracks = match playlist_tracks_paged(&list_id, cookie, ((page - 1) * 50) as usize, 50).await {
            Ok(t) => t,
            Err(_) => break,
        };
        if tracks.is_empty() { break; }

        for track in &tracks {
            let track_hash = track.hash.as_deref().unwrap_or(&track.id).trim().to_lowercase();
            if !track_hash.is_empty() && hash_set.contains(&track_hash) {
                liked.insert(track_hash, true);
            }
        }

        if liked.len() >= hash_set.len() { break; }
        if tracks.len() < 50 { break; }
    }

    Ok(liked)
}

/// 获取所有已喜欢的歌曲 hash 列表
pub async fn liked_hashes(cookie: &str) -> Result<Vec<String>, String> {
    let auth = extract_kugou_auth(cookie);
    if !auth.playback_ready {
        return Ok(Vec::new());
    }

    let list_id = resolve_favorite_list_id(cookie).await;
    if list_id.is_empty() {
        return Ok(Vec::new());
    }

    let mut all_hashes = Vec::new();

    for page in 1..=20u32 {
        let tracks = match playlist_tracks_paged(&list_id, cookie, ((page - 1) * 50) as usize, 50).await {
            Ok(t) => t,
            Err(_) => break,
        };
        if tracks.is_empty() { break; }

        for track in &tracks {
            let hash = track.hash.as_deref().unwrap_or(&track.id).trim().to_lowercase();
            if !hash.is_empty() {
                all_hashes.push(hash);
            }
        }

        if tracks.len() < 50 { break; }
    }

    Ok(all_hashes)
}
