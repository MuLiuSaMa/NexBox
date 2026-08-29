use std::sync::Arc;
use std::collections::HashMap;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

const STORE_FILE: &str = "music-cookies.json";

fn get_store(app: &AppHandle) -> Result<Arc<tauri_plugin_store::Store<tauri::Wry>>, String> {
    app.store(STORE_FILE).map_err(|e| format!("Failed to load cookie store: {e}"))
}

pub fn save_cookie(app: &AppHandle, provider: &str, cookie: &str) -> Result<(), String> {
    let store = get_store(app)?;
    store.set(format!("cookie_{provider}"), cookie);
    store.save().map_err(|e| format!("Failed to save cookie: {e}"))
}

pub fn load_cookie(app: &AppHandle, provider: &str) -> Result<String, String> {
    let store = get_store(app)?;
    Ok(store
        .get(format!("cookie_{provider}"))
        .map(|v| v.as_str().unwrap_or("").to_string())
        .unwrap_or_default())
}

pub fn clear_cookie(app: &AppHandle, provider: &str) -> Result<(), String> {
    let store = get_store(app)?;
    store.delete(format!("cookie_{provider}"));
    store.delete(format!("uid_{provider}"));
    store.save().map_err(|e| format!("Failed to clear cookie: {e}"))
}

/// 保存平台用户 id (咪咕 listen 接口需要 uid 请求头)
pub fn save_user_id(app: &AppHandle, provider: &str, user_id: &str) -> Result<(), String> {
    let store = get_store(app)?;
    store.set(format!("uid_{provider}"), user_id);
    store.save().map_err(|e| format!("Failed to save user id: {e}"))
}

/// 加载平台用户 id
pub fn load_user_id(app: &AppHandle, provider: &str) -> Result<String, String> {
    let store = get_store(app)?;
    Ok(store
        .get(format!("uid_{provider}"))
        .map(|v| v.as_str().unwrap_or("").to_string())
        .unwrap_or_default())
}

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

pub fn normalize_cookie_header(raw: &str) -> String {
    let map = parse_cookie_string(raw);
    map.iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// 检查网易云 Cookie 是否包含登录态 (MUSIC_U)
pub fn netease_cookie_has_login(cookie: &str) -> bool {
    parse_cookie_string(cookie).contains_key("MUSIC_U")
}

/// 提取 QQ 音乐 uin (对照 Mineradio qqCookieUin)
pub fn qq_extract_uin(cookie: &str) -> String {
    let obj = parse_cookie_string(cookie);
    let login_type = obj.get("login_type").map(|v| v.trim().parse::<i32>().unwrap_or(0)).unwrap_or(0);
    let raw = if login_type == 2 {
        // 微信登录: 优先 wxuin
        obj.get("wxuin").or_else(|| obj.get("uin")).or_else(|| obj.get("p_uin"))
    } else {
        // QQ 登录
        obj.get("uin").or_else(|| obj.get("qqmusic_uin")).or_else(|| obj.get("wxuin")).or_else(|| obj.get("p_uin"))
    };
    raw.map(|v| v.replace(|c: char| !c.is_ascii_digit(), ""))
        .unwrap_or_default()
        .trim_start_matches('0')
        .to_string()
}

/// 提取 QQ 音乐 musicKey (对照 Mineradio qqCookieMusicKey)
pub fn qq_extract_music_key(cookie: &str) -> String {
    let obj = parse_cookie_string(cookie);
    obj.get("qm_keyst")
        .or_else(|| obj.get("qqmusic_key"))
        .or_else(|| obj.get("music_key"))
        .or_else(|| obj.get("p_skey"))
        .or_else(|| obj.get("skey"))
        .or_else(|| obj.get("psrf_qqaccess_token"))
        .or_else(|| obj.get("psrf_qqrefresh_token"))
        .or_else(|| obj.get("wxrefresh_token"))
        .or_else(|| obj.get("wxskey"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// 提取 QQ 音乐 playbackKey (对照 Mineradio qqCookiePlaybackKey)
pub fn qq_extract_playback_key(cookie: &str) -> String {
    let obj = parse_cookie_string(cookie);
    obj.get("qm_keyst")
        .or_else(|| obj.get("qqmusic_key"))
        .or_else(|| obj.get("music_key"))
        .or_else(|| obj.get("wxskey"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// 检查 QQ 音乐 Cookie 是否包含登录态 (uin + musicKey)
pub fn qq_cookie_has_login(cookie: &str) -> bool {
    let uin = qq_extract_uin(cookie);
    let music_key = qq_extract_music_key(cookie);
    !uin.is_empty() && !music_key.is_empty()
}

/// 检查 QQ 音乐 Cookie 是否包含播放权限 (uin + playbackKey)
pub fn qq_cookie_has_playback(cookie: &str) -> bool {
    let uin = qq_extract_uin(cookie);
    let playback_key = qq_extract_playback_key(cookie);
    !uin.is_empty() && !playback_key.is_empty()
}
