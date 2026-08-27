pub mod audio_proxy;
pub mod cookie;
pub mod crypto;
pub mod kugou;
pub mod models;
pub mod netease;
pub mod qqmusic;

use std::collections::HashMap;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;
use url::Url;
use models::*;

// ============================================================
//  Tauri Commands - 网易云
// ============================================================

#[tauri::command]
pub async fn music_search(keywords: String, limit: Option<u32>) -> Result<Vec<Song>, String> {
    let app_cookie = get_app_cookie().await;
    netease::search(&keywords, limit.unwrap_or(30), &app_cookie).await
}

#[tauri::command]
pub async fn music_song_url(id: String, quality: Option<String>) -> Result<SongUrlResult, String> {
    let app_cookie = get_app_cookie().await;
    netease::song_url(&id, &quality.unwrap_or_else(|| "hires".into()), &app_cookie).await
}

#[tauri::command]
pub async fn music_login_qr_key() -> Result<String, String> {
    let app_cookie = get_app_cookie().await;
    netease::login_qr_key(&app_cookie).await
}

#[tauri::command]
pub async fn music_login_qr_create(key: String) -> Result<String, String> {
    let app_cookie = get_app_cookie().await;
    netease::login_qr_create(&key, &app_cookie).await
}

#[tauri::command]
pub async fn music_login_qr_check(app: AppHandle, key: String) -> Result<QrCheckResult, String> {
    let app_cookie = get_app_cookie().await;
    let result = netease::login_qr_check(&key, &app_cookie).await?;

    // code 803 = 登录成功，自动保存 cookie
    if result.code == 803 {
        if let Some(ref cookie) = result.cookie {
            let normalized = cookie::normalize_cookie_header(cookie);
            if cookie::netease_cookie_has_login(&normalized) {
                let _ = cookie::save_cookie(&app, "netease", &normalized);
                set_app_cookie(normalized).await;
                log::info!("[MusicAPI] QR login successful, cookie saved");
            }
        }
    }

    Ok(result)
}

#[tauri::command]
pub async fn music_login_status(app: AppHandle) -> Result<LoginInfo, String> {
    let app_cookie = load_app_cookie(&app).await;
    netease::login_status(&app_cookie).await
}

#[tauri::command]
pub async fn music_login_cookie(app: AppHandle, cookie: String) -> Result<LoginInfo, String> {
    let normalized = cookie::normalize_cookie_header(&cookie);
    if !cookie::netease_cookie_has_login(&normalized) {
        return Ok(LoginInfo {
            provider: "netease".into(),
            ..Default::default()
        });
    }
    cookie::save_cookie(&app, "netease", &normalized)?;
    set_app_cookie(normalized).await;
    netease::login_status(get_app_cookie().await.as_str()).await
}

#[tauri::command]
pub async fn music_logout(app: AppHandle) -> Result<(), String> {
    cookie::clear_cookie(&app, "netease")?;
    set_app_cookie(String::new()).await;
    Ok(())
}

#[tauri::command]
pub async fn music_user_playlist(app: AppHandle) -> Result<Vec<Playlist>, String> {
    let app_cookie = load_app_cookie(&app).await;
    log::info!("[MusicAPI] music_user_playlist: cookie length={}, has MUSIC_U={}", 
        app_cookie.len(), cookie::netease_cookie_has_login(&app_cookie));
    
    let info = netease::login_status(&app_cookie).await?;
    log::info!("[MusicAPI] music_user_playlist: logged_in={}, user_id={}", info.logged_in, info.user_id);
    
    if !info.logged_in || info.user_id.is_empty() {
        return Ok(vec![]);
    }
    let result = netease::user_playlist(&info.user_id, &app_cookie).await;
    log::info!("[MusicAPI] music_user_playlist: result count={}", result.as_ref().map(|v| v.len()).unwrap_or(0));
    result
}

#[tauri::command]
pub async fn music_playlist_tracks(id: String) -> Result<(Playlist, Vec<Song>), String> {
    let app_cookie = get_app_cookie().await;
    netease::playlist_tracks(&id, &app_cookie).await
}

#[tauri::command]
pub async fn music_playlist_tracks_range(id: String, start: usize, count: usize) -> Result<Vec<Song>, String> {
    let app_cookie = get_app_cookie().await;
    netease::playlist_tracks_range(&id, start, count, &app_cookie).await
}

#[tauri::command]
pub async fn music_playlist_info_with_track_ids(id: String) -> Result<(Playlist, Vec<String>), String> {
    let app_cookie = get_app_cookie().await;
    netease::playlist_info_with_track_ids(&id, &app_cookie).await
}

#[tauri::command]
pub async fn music_playlist_detail(id: String) -> Result<Playlist, String> {
    let app_cookie = get_app_cookie().await;
    netease::playlist_detail(&id, &app_cookie).await
}

#[tauri::command]
pub async fn music_likelist(app: AppHandle) -> Result<Vec<String>, String> {
    let app_cookie = load_app_cookie(&app).await;
    let info = netease::login_status(&app_cookie).await?;
    netease::likelist(&info.user_id, &app_cookie).await
}

#[tauri::command]
pub async fn music_like(id: String, like: bool) -> Result<(), String> {
    let app_cookie = get_app_cookie().await;
    netease::like(&id, like, &app_cookie).await
}

#[tauri::command]
pub async fn music_playlist_subscribe(id: String, subscribe: bool) -> Result<(), String> {
    let app_cookie = get_app_cookie().await;
    netease::playlist_subscribe(&id, subscribe, &app_cookie).await
}

#[tauri::command]
pub async fn music_lyric(id: String) -> Result<Lyrics, String> {
    let app_cookie = get_app_cookie().await;
    netease::lyric(&id, &app_cookie).await
}

#[tauri::command]
pub async fn music_song_comments(id: String, page: Option<u32>, page_size: Option<u32>) -> Result<CommentPage, String> {
    let app_cookie = get_app_cookie().await;
    netease::song_comments(&id, page.unwrap_or(1), page_size.unwrap_or(20), &app_cookie).await
}

#[tauri::command]
pub async fn music_send_comment(id: String, content: String) -> Result<(), String> {
    let app_cookie = get_app_cookie().await;
    netease::send_comment(&id, &content, &app_cookie).await
}

#[tauri::command]
pub async fn music_personalized() -> Result<Vec<Playlist>, String> {
    let app_cookie = get_app_cookie().await;
    netease::personalized(&app_cookie).await
}

#[tauri::command]
pub async fn music_recommend_songs() -> Result<Vec<Song>, String> {
    let app_cookie = get_app_cookie().await;
    netease::recommend_songs(&app_cookie).await
}

#[tauri::command]
pub async fn music_recommend_resource() -> Result<Vec<Playlist>, String> {
    let app_cookie = get_app_cookie().await;
    netease::recommend_resource(&app_cookie).await
}

/// 相似歌曲 (心动模式): 根据当前歌曲 id 返回口味相似的歌曲
#[tauri::command]
pub async fn music_simi_song(id: String, limit: Option<u32>) -> Result<Vec<Song>, String> {
    let app_cookie = get_app_cookie().await;
    netease::simi_song(&id, limit.unwrap_or(50), &app_cookie).await
}

#[tauri::command]
pub async fn music_artist_search(keywords: String, limit: Option<u32>) -> Result<Vec<Artist>, String> {
    let app_cookie = get_app_cookie().await;
    netease::artist_search(&keywords, limit.unwrap_or(30), &app_cookie).await
}

#[tauri::command]
pub async fn music_playlist_search(keywords: String, limit: Option<u32>) -> Result<Vec<Playlist>, String> {
    let app_cookie = get_app_cookie().await;
    netease::playlist_search(&keywords, limit.unwrap_or(30), &app_cookie).await
}

#[tauri::command]
pub async fn music_artist_songs(artist_id: String, limit: Option<u32>, offset: Option<u32>) -> Result<Vec<Song>, String> {
    let app_cookie = get_app_cookie().await;
    netease::artist_songs(&artist_id, limit.unwrap_or(50), offset.unwrap_or(0), &app_cookie).await
}

#[tauri::command]
pub async fn music_artist_detail(artist_id: String) -> Result<ArtistDetail, String> {
    let app_cookie = get_app_cookie().await;
    netease::artist_detail(&artist_id, &app_cookie).await
}

#[tauri::command]
pub async fn music_artist_albums(artist_id: String, limit: Option<u32>, offset: Option<u32>) -> Result<Vec<Album>, String> {
    let app_cookie = get_app_cookie().await;
    netease::artist_albums(&artist_id, limit.unwrap_or(50), offset.unwrap_or(0), &app_cookie).await
}

#[tauri::command]
pub async fn music_artist_mvs(artist_id: String, limit: Option<u32>, offset: Option<u32>) -> Result<Vec<Mv>, String> {
    let app_cookie = get_app_cookie().await;
    netease::artist_mvs(&artist_id, limit.unwrap_or(50), offset.unwrap_or(0), &app_cookie).await
}

#[tauri::command]
pub async fn music_album_detail(album_id: String) -> Result<(Album, Vec<Song>), String> {
    let app_cookie = get_app_cookie().await;
    netease::album_detail(&album_id, &app_cookie).await
}

#[tauri::command]
pub async fn music_mv_url(mv_id: String, resolution: Option<u32>) -> Result<String, String> {
    let app_cookie = get_app_cookie().await;
    netease::mv_url(&mv_id, resolution.unwrap_or(1080), &app_cookie).await
}

// ============================================================
//  Tauri Commands - 酷狗音乐
// ============================================================

#[tauri::command]
pub async fn kugou_search(app: AppHandle, keywords: String, limit: Option<u32>) -> Result<Vec<Song>, String> {
    let cookie = load_provider_cookie(&app, "kugou").await;
    kugou::search(&keywords, limit.unwrap_or(30), &cookie).await
}

#[tauri::command]
pub async fn kugou_artist_search(app: AppHandle, keywords: String, limit: Option<u32>) -> Result<Vec<Artist>, String> {
    let cookie = load_provider_cookie(&app, "kugou").await;
    kugou::artist_search(&keywords, limit.unwrap_or(30), &cookie).await
}

#[tauri::command]
pub async fn kugou_playlist_search(app: AppHandle, keywords: String, limit: Option<u32>) -> Result<Vec<Playlist>, String> {
    let cookie = load_provider_cookie(&app, "kugou").await;
    kugou::playlist_search(&keywords, limit.unwrap_or(30), &cookie).await
}

#[tauri::command]
pub async fn kugou_artist_songs(app: AppHandle, artist_id: String, limit: Option<u32>, offset: Option<u32>) -> Result<Vec<Song>, String> {
    let cookie = load_provider_cookie(&app, "kugou").await;
    kugou::artist_songs(&artist_id, limit.unwrap_or(50), offset.unwrap_or(0), &cookie).await
}

#[tauri::command]
pub async fn kugou_song_url(
    app: AppHandle,
    hash: String,
    album_id: Option<String>,
    album_audio_id: Option<String>,
    quality: Option<String>,
    hq_hash: Option<String>,
    sq_hash: Option<String>,
    res_hash: Option<String>,
) -> Result<SongUrlResult, String> {
    let cookie = load_provider_cookie(&app, "kugou").await;
    kugou::song_url(
        &hash,
        &album_id.unwrap_or_default(),
        &album_audio_id.unwrap_or_default(),
        &quality.unwrap_or_else(|| "standard".into()),
        &cookie,
        &hq_hash.unwrap_or_default(),
        &sq_hash.unwrap_or_default(),
        &res_hash.unwrap_or_default(),
    )
    .await
}

#[tauri::command]
pub async fn kugou_lyric(
    hash: String,
    album_audio_id: Option<String>,
    duration: Option<u64>,
) -> Result<Lyrics, String> {
    kugou::lyric(&hash, &album_audio_id.unwrap_or_default(), duration.unwrap_or(0)).await
}

#[tauri::command]
pub async fn kugou_login_status(app: AppHandle) -> Result<LoginInfo, String> {
    let cookie = load_provider_cookie(&app, "kugou").await;
    kugou::login_info(&cookie).await
}

#[tauri::command]
pub async fn kugou_login_cookie(app: AppHandle, cookie: String) -> Result<LoginInfo, String> {
    let normalized = cookie::normalize_cookie_header(&cookie);
    if !kugou::kugou_cookie_has_login(&normalized) {
        return Ok(LoginInfo {
            provider: "kugou".into(),
            ..Default::default()
        });
    }
    cookie::save_cookie(&app, "kugou", &normalized)?;
    set_provider_cookie("kugou", normalized).await;
    let c = get_provider_cookie("kugou").await;
    kugou::login_info(&c).await
}

#[tauri::command]
pub async fn kugou_logout(app: AppHandle) -> Result<(), String> {
    cookie::clear_cookie(&app, "kugou")?;
    set_provider_cookie("kugou", String::new()).await;
    Ok(())
}

#[tauri::command]
pub async fn kugou_user_playlists(app: AppHandle) -> Result<Vec<Playlist>, String> {
    let cookie = load_provider_cookie(&app, "kugou").await;
    kugou::user_playlists(&cookie).await
}

#[tauri::command]
pub async fn kugou_playlist_tracks(app: AppHandle, id: String) -> Result<(Playlist, Vec<Song>), String> {
    let cookie = load_provider_cookie(&app, "kugou").await;
    kugou::playlist_tracks(&id, &cookie).await
}

#[tauri::command]
pub async fn kugou_playlist_tracks_range(app: AppHandle, id: String, start: usize, count: usize) -> Result<Vec<Song>, String> {
    let cookie = load_provider_cookie(&app, "kugou").await;
    kugou::playlist_tracks_paged(&id, &cookie, start, count).await
}

#[tauri::command]
pub async fn kugou_guess_like(app: AppHandle, limit: Option<u32>) -> Result<Vec<Song>, String> {
    let cookie = load_provider_cookie(&app, "kugou").await;
    kugou::guess_like(&cookie, limit.unwrap_or(12)).await
}

#[tauri::command]
pub async fn kugou_rank_list(app: AppHandle) -> Result<Vec<Playlist>, String> {
    let cookie = load_provider_cookie(&app, "kugou").await;
    kugou::get_rank_list(&cookie).await
}

#[tauri::command]
pub async fn kugou_rank_songs(app: AppHandle, rank_id: String, limit: Option<u32>) -> Result<Vec<Song>, String> {
    let cookie = load_provider_cookie(&app, "kugou").await;
    kugou::get_rank_songs(&cookie, &rank_id, limit.unwrap_or(30)).await
}

#[tauri::command]
pub async fn kugou_like_toggle(app: AppHandle, song: Song, like: bool) -> Result<bool, String> {
    let cookie = load_provider_cookie(&app, "kugou").await;
    kugou::like_toggle(&song, like, &cookie).await
}

#[tauri::command]
pub async fn kugou_liked_hashes(app: AppHandle) -> Result<Vec<String>, String> {
    let cookie = load_provider_cookie(&app, "kugou").await;
    kugou::liked_hashes(&cookie).await
}

// ============================================================
//  Tauri Commands - QQ 音乐
// ============================================================

#[tauri::command]
pub async fn qq_search(app: AppHandle, keywords: String, limit: Option<u32>) -> Result<Vec<Song>, String> {
    let cookie = load_provider_cookie(&app, "qqmusic").await;
    qqmusic::search(&keywords, limit.unwrap_or(30), &cookie).await
}

#[tauri::command]
pub async fn qq_song_url(
    app: AppHandle,
    mid: String,
    media_mid: Option<String>,
    quality: Option<String>,
) -> Result<SongUrlResult, String> {
    let cookie = load_provider_cookie(&app, "qqmusic").await;
    qqmusic::song_url(&mid, &media_mid.unwrap_or_default(), &quality.unwrap_or_else(|| "hires".into()), &cookie).await
}

#[tauri::command]
pub async fn qq_lyric(app: AppHandle, mid: String, id: Option<String>) -> Result<Lyrics, String> {
    let cookie = load_provider_cookie(&app, "qqmusic").await;
    qqmusic::lyric(&mid, &id.unwrap_or_default(), &cookie).await
}

#[tauri::command]
pub async fn qq_login_status(app: AppHandle) -> Result<LoginInfo, String> {
    let cookie = load_provider_cookie(&app, "qqmusic").await;
    qqmusic::login_info(&cookie).await
}

#[tauri::command]
pub async fn qq_login_cookie(app: AppHandle, cookie: String) -> Result<LoginInfo, String> {
    let normalized = cookie::normalize_cookie_header(&cookie);
    if !cookie::qq_cookie_has_login(&normalized) {
        return Ok(LoginInfo {
            provider: "qqmusic".into(),
            ..Default::default()
        });
    }
    cookie::save_cookie(&app, "qqmusic", &normalized)?;
    set_provider_cookie("qqmusic", normalized).await;
    let c = get_provider_cookie("qqmusic").await;
    qqmusic::login_info(&c).await
}

#[tauri::command]
pub async fn qq_logout(app: AppHandle) -> Result<(), String> {
    cookie::clear_cookie(&app, "qqmusic")?;
    set_provider_cookie("qqmusic", String::new()).await;
    Ok(())
}

#[tauri::command]
pub async fn qq_user_playlists(app: AppHandle) -> Result<Vec<Playlist>, String> {
    let cookie = load_provider_cookie(&app, "qqmusic").await;
    qqmusic::user_playlists(&cookie).await
}

#[tauri::command]
pub async fn qq_playlist_tracks(app: AppHandle, id: String) -> Result<(Playlist, Vec<Song>), String> {
    let cookie = load_provider_cookie(&app, "qqmusic").await;
    qqmusic::playlist_tracks(&id, &cookie).await
}

#[tauri::command]
pub async fn qq_playlist_tracks_range(app: AppHandle, id: String, start: usize, count: usize) -> Result<Vec<Song>, String> {
    let cookie = load_provider_cookie(&app, "qqmusic").await;
    qqmusic::playlist_tracks_range(&id, start, count, &cookie).await
}

#[tauri::command]
pub async fn qq_artist_search(app: AppHandle, keywords: String, limit: Option<u32>) -> Result<Vec<Artist>, String> {
    let cookie = load_provider_cookie(&app, "qqmusic").await;
    qqmusic::artist_search(&keywords, limit.unwrap_or(30), &cookie).await
}

#[tauri::command]
pub async fn qq_artist_songs(app: AppHandle, artist_id: String, limit: Option<u32>, offset: Option<u32>) -> Result<Vec<Song>, String> {
    let cookie = load_provider_cookie(&app, "qqmusic").await;
    qqmusic::artist_songs(&artist_id, limit.unwrap_or(50), offset.unwrap_or(0), &cookie).await
}

#[tauri::command]
pub async fn qq_playlist_search(app: AppHandle, keywords: String, limit: Option<u32>) -> Result<Vec<Playlist>, String> {
    let cookie = load_provider_cookie(&app, "qqmusic").await;
    qqmusic::playlist_search(&keywords, limit.unwrap_or(30), &cookie).await
}

#[tauri::command]
pub async fn qq_rank_list(app: AppHandle) -> Result<Vec<Playlist>, String> {
    let cookie = load_provider_cookie(&app, "qqmusic").await;
    qqmusic::get_rank_list(&cookie).await
}

#[tauri::command]
pub async fn qq_rank_songs(app: AppHandle, rank_id: String, limit: Option<u32>) -> Result<Vec<Song>, String> {
    let cookie = load_provider_cookie(&app, "qqmusic").await;
    qqmusic::get_rank_songs(&cookie, &rank_id, limit.unwrap_or(30)).await
}

#[tauri::command]
pub async fn music_qq_recommend_playlists() -> Result<Vec<Playlist>, String> {
    qqmusic::recommend_playlists().await
}

#[tauri::command]
pub async fn qq_liked_hashes(app: AppHandle) -> Result<Vec<String>, String> {
    let cookie = load_provider_cookie(&app, "qqmusic").await;
    qqmusic::liked_hashes(&cookie).await
}

#[tauri::command]
pub async fn qq_like_toggle(app: AppHandle, song: Song, like: bool) -> Result<bool, String> {
    let cookie = load_provider_cookie(&app, "qqmusic").await;
    qqmusic::like_toggle(&song, like, &cookie).await
}

// ============================================================
//  Tauri Commands - 多平台管理
// ============================================================

/// 获取所有平台登录状态 (并行执行，避免酷狗/VIP等慢速接口阻塞整体登录)
#[tauri::command]
pub async fn music_get_login_statuses(app: AppHandle) -> Result<HashMap<String, LoginInfo>, String> {
    let netease_cookie = load_provider_cookie(&app, "netease").await;
    let kugou_cookie = load_provider_cookie(&app, "kugou").await;
    let qq_cookie = load_provider_cookie(&app, "qqmusic").await;

    let (netease_result, kugou_result, qq_result) = tokio::join!(
        async {
            if !netease_cookie.is_empty() {
                netease::login_status(&netease_cookie).await.ok()
            } else { None }
        },
        async {
            if !kugou_cookie.is_empty() {
                kugou::login_info(&kugou_cookie).await.ok()
            } else { None }
        },
        async {
            if !qq_cookie.is_empty() {
                qqmusic::login_info(&qq_cookie).await.ok()
            } else { None }
        },
    );

    let mut result = HashMap::new();
    if let Some(info) = netease_result { result.insert("netease".into(), info); }
    if let Some(info) = kugou_result { result.insert("kugou".into(), info); }
    if let Some(info) = qq_result { result.insert("qqmusic".into(), info); }
    Ok(result)
}

/// 切换播放源平台
#[tauri::command]
pub async fn music_switch_provider(app: AppHandle, provider: String) -> Result<(), String> {
    match provider.as_str() {
        "netease" | "kugou" | "qqmusic" => {}
        _ => return Err(format!("Unknown provider: {}", provider)),
    }
    let store = app.store("music-cookies.json").map_err(|e| e.to_string())?;
    store.set("playback_source", provider);
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

/// 获取当前播放源平台
#[tauri::command]
pub async fn music_get_playback_source(app: AppHandle) -> Result<String, String> {
    let store = app.store("music-cookies.json").map_err(|e| e.to_string())?;
    Ok(store
        .get("playback_source")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "netease".into()))
}

// ============================================================
//  登录窗口 - 多平台
// ============================================================

/// 网易云登录 cookie 优先级 (参考 Mineradio)
const NETEASE_COOKIE_PRIORITY: &[&str] = &[
    "MUSIC_U",
    "__csrf",
    "NMTID",
    "MUSIC_A",
    "__remember_me",
    "_ntes_nuid",
    "_ntes_nnid",
    "WEVNSM",
    "WNMCID",
    "JSESSIONID-WYYY",
];

/// 酷狗登录 cookie 优先级 (参考 Mineradio KUGOU_LOGIN_COOKIE_PRIORITY)
const KUGOU_COOKIE_PRIORITY: &[&str] = &[
    "KuGoo",
    "token",
    "userid",
    "KugooID",
    "kugouID",
    "UserId",
    "kg_mid",
    "kg_dfid",
    "Kugou",
    "NickName",
];

/// QQ 音乐登录 cookie 优先级 (参考 Mineradio QQ_LOGIN_COOKIE_PRIORITY)
const QQ_COOKIE_PRIORITY: &[&str] = &[
    "uin",
    "qqmusic_uin",
    "wxuin",
    "login_type",
    "qm_keyst",
    "qqmusic_key",
    "p_skey",
    "skey",
    "psrf_qqopenid",
    "psrf_qqunionid",
    "psrf_qqaccess_token",
    "psrf_qqrefresh_token",
    "wxopenid",
    "wxunionid",
    "wxrefresh_token",
    "wxskey",
    "p_uin",
    "ptcz",
    "RK",
];

/// 检查域名是否属于网易云
fn is_netease_domain(domain: &str) -> bool {
    let d = domain.trim_start_matches('.').to_lowercase();
    d == "163.com" || d.ends_with(".163.com") ||
    d == "music.163.com" || d.ends_with(".music.163.com") ||
    d == "netease.com" || d.ends_with(".netease.com")
}

/// 检查域名是否属于酷狗 (参考 Mineradio isKugouCookieDomain)
fn is_kugou_domain(domain: &str) -> bool {
    let d = domain.trim_start_matches('.').to_lowercase();
    d == "kugou.com" || d.ends_with(".kugou.com")
}

/// 检查域名是否属于 QQ 音乐 (参考 Mineradio isQQCookieDomain)
fn is_qq_domain(domain: &str) -> bool {
    let d = domain.trim_start_matches('.').to_lowercase();
    d == "qq.com" || d.ends_with(".qq.com") || d.ends_with("qqmusic.qq.com")
}

/// 从 webview cookies 构建指定平台的 cookie 字符串
fn build_cookie_from_webview(cookies: &[tauri::webview::Cookie], priority: &[&str], domain_check: fn(&str) -> bool) -> String {
    use std::collections::HashMap;
    let mut picked: HashMap<String, String> = HashMap::new();
    for c in cookies {
        if let Some(domain) = c.domain() {
            if domain_check(domain) {
                let name = c.name().to_string();
                let value = c.value().to_string();
                if !name.is_empty() && !value.is_empty() {
                    picked.insert(name, value);
                }
            }
        }
    }
    let mut ordered: Vec<(String, String)> = Vec::new();
    for name in priority {
        if let Some(value) = picked.remove(*name) {
            ordered.push((name.to_string(), value));
        }
    }
    for (name, value) in picked {
        ordered.push((name, value));
    }
    ordered
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// 打开登录窗口 (多平台) - 使用 Tauri cookies() API 直接读取 HttpOnly cookie
/// 参考 Mineradio 的 Electron session.cookies.get() 方案
#[tauri::command]
pub async fn music_open_login_window(app: AppHandle, provider: String) -> Result<String, String> {
    match provider.as_str() {
        "netease" => open_netease_login_window(&app).await,
        "kugou" => open_kugou_login_window(&app).await,
        "qqmusic" => open_qq_login_window(&app).await,
        _ => Err(format!("Unknown provider: {}", provider)),
    }
}

/// 打开网易云登录窗口
async fn open_netease_login_window(app: &AppHandle) -> Result<String, String> {
    use tauri::WebviewUrl;

    let url = "https://music.163.com/#/login";
    let label = "netease-login";

    if let Some(existing) = app.get_webview_window(label) {
        let _ = existing.clear_all_browsing_data();
        let login_url = url.parse::<Url>().map_err(|e| e.to_string())?;
        let _ = existing.navigate(login_url);
        let _ = existing.set_focus();
        return Ok("window_refreshed".into());
    }

    let login_window = tauri::WebviewWindowBuilder::new(
        app,
        label,
        WebviewUrl::External("about:blank".parse().map_err(|e: url::ParseError| e.to_string())?),
    )
    .title("网易云音乐登录")
    // 与其它窗口保持一致的 WebView2 参数（禁用 Chromium 自动媒体会话，避免与 smtc.rs 会话重复）
    .additional_browser_args("--disable-features=MediaSessionService,HardwareMediaKeyHandling --autoplay-policy=no-user-gesture-required")
    .inner_size(940.0, 760.0)
    .min_inner_size(780.0, 580.0)
    .build()
    .map_err(|e| format!("Failed to create login window: {e}"))?;

    let _ = login_window.clear_all_browsing_data();
    let login_url = url.parse::<Url>().map_err(|e| e.to_string())?;
    let _ = login_window.navigate(login_url);

    let win = login_window.clone();
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;

        for _ in 0..150 {
            match win.cookies() {
                Ok(cookies) => {
                    let cookie_str = build_cookie_from_webview(&cookies, NETEASE_COOKIE_PRIORITY, is_netease_domain);

                    if cookie::netease_cookie_has_login(&cookie_str) {
                        log::info!("[MusicAPI] MUSIC_U cookie found, cookie length: {}", cookie_str.len());
                        let _ = cookie::save_cookie(&app_handle, "netease", &cookie_str);
                        set_app_cookie(cookie_str).await;
                        let _ = win.close();
                        let app_cookie = get_app_cookie().await;
                        match netease::login_status(&app_cookie).await {
                            Ok(info) => {
                                if info.logged_in {
                                    let _ = app_handle.emit("netease-login-success", &info);
                                } else {
                                    let _ = app_handle.emit("netease-login-failed", "Cookie 无效或已过期");
                                }
                            }
                            Err(e) => {
                                let _ = app_handle.emit("netease-login-failed", &e);
                            }
                        }
                        return;
                    }
                }
                Err(e) => {
                    log::warn!("[MusicAPI] Failed to read cookies from webview: {e}");
                    // 窗口可能已被用户关闭，检测到后停止轮询
                    if !win.is_visible().unwrap_or(false) {
                        log::info!("[MusicAPI] Login window closed, stop polling");
                        break;
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        log::warn!("[MusicAPI] Login window polling timed out after 5 minutes");
    });

    Ok("window_created".into())
}

/// 打开酷狗登录窗口 (参考 Mineradio openKugouMusicLoginWindow)
/// 包含 Warmup 机制: 首次登录可能只有 loggedIn 没有 playbackReady,
/// 需要导航到 warmup URL 触发更多 cookie 写入
async fn open_kugou_login_window(app: &AppHandle) -> Result<String, String> {
    use tauri::WebviewUrl;

    let url = "https://www.kugou.com/";
    let warmup_url = "https://www.kugou.com/newuc/user/uc/type=edit";
    let label = "kugou-login";

    if let Some(existing) = app.get_webview_window(label) {
        let _ = existing.clear_all_browsing_data();
        let login_url = url.parse::<Url>().map_err(|e| e.to_string())?;
        let _ = existing.navigate(login_url);
        let _ = existing.set_focus();
        return Ok("window_refreshed".into());
    }

    let login_window = tauri::WebviewWindowBuilder::new(
        app,
        label,
        WebviewUrl::External("about:blank".parse().map_err(|e: url::ParseError| e.to_string())?),
    )
    .title("酷狗音乐登录")
    // 与其它窗口保持一致的 WebView2 参数（禁用 Chromium 自动媒体会话，避免与 smtc.rs 会话重复）
    .additional_browser_args("--disable-features=MediaSessionService,HardwareMediaKeyHandling --autoplay-policy=no-user-gesture-required")
    .inner_size(900.0, 720.0)
    .min_inner_size(760.0, 560.0)
    .build()
    .map_err(|e| format!("Failed to create login window: {e}"))?;

    let _ = login_window.clear_all_browsing_data();
    let login_url = url.parse::<Url>().map_err(|e| e.to_string())?;
    let _ = login_window.navigate(login_url);

    let win = login_window.clone();
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        // 等待页面加载
        tokio::time::sleep(Duration::from_secs(3)).await;

        let mut warmup_started = false;

        for _ in 0..150 {
            match win.cookies() {
                Ok(cookies) => {
                    let cookie_str = build_cookie_from_webview(&cookies, KUGOU_COOKIE_PRIORITY, is_kugou_domain);

                    if kugou::kugou_cookie_has_playback(&cookie_str) {
                        // 登录完成 (playbackReady: userid + token)
                        log::info!("[KugouLogin] playbackReady cookie found, length: {}", cookie_str.len());
                        let _ = cookie::save_cookie(&app_handle, "kugou", &cookie_str);
                        set_provider_cookie("kugou", cookie_str).await;
                        let _ = win.close();
                        let kugou_cookie = get_provider_cookie("kugou").await;
                        match kugou::login_info(&kugou_cookie).await {
                            Ok(info) => {
                                if info.logged_in {
                                    let _ = app_handle.emit("kugou-login-success", &info);
                                } else {
                                    let _ = app_handle.emit("kugou-login-failed", "Cookie 无效或已过期");
                                }
                            }
                            Err(e) => {
                                let _ = app_handle.emit("kugou-login-failed", &e);
                            }
                        }
                        return;
                    } else if kugou::kugou_cookie_has_login(&cookie_str) && !warmup_started {
                        // 有登录态但 token 不完整 → warmup
                        // 参考 Mineradio: 导航到 warmup URL 触发更多 cookie 写入
                        warmup_started = true;
                        log::info!("[KugouLogin] loggedIn but not playbackReady, starting warmup...");
                        if let Ok(warmup) = warmup_url.parse::<Url>() {
                            let _ = win.navigate(warmup);
                        }
                    }
                }
                Err(e) => {
                    log::warn!("[KugouLogin] Failed to read cookies from webview: {e}");
                    // 窗口可能已被用户关闭，检测到后停止轮询
                    if !win.is_visible().unwrap_or(false) {
                        log::info!("[KugouLogin] Login window closed, stop polling");
                        break;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(1200)).await;
        }

        // 超时 — 最后检查一次 cookie
        if let Ok(cookies) = win.cookies() {
            let cookie_str = build_cookie_from_webview(&cookies, KUGOU_COOKIE_PRIORITY, is_kugou_domain);
            if kugou::kugou_cookie_has_login(&cookie_str) {
                log::info!("[KugouLogin] Timeout but found partial login, saving cookie");
                let _ = cookie::save_cookie(&app_handle, "kugou", &cookie_str);
                set_provider_cookie("kugou", cookie_str).await;
                let kugou_cookie = get_provider_cookie("kugou").await;
                if let Ok(info) = kugou::login_info(&kugou_cookie).await {
                    let _ = app_handle.emit("kugou-login-success", &info);
                }
                return;
            }
        }
        log::warn!("[KugouLogin] Polling timed out after 5 minutes");
    });

    Ok("window_created".into())
}

/// 打开 QQ 音乐登录窗口 (参考 Mineradio openQQMusicLoginWindow)
/// 包含 Warmup 机制: 首次登录可能只有 loggedIn 没有 playbackReady,
/// 需要导航到 warmup URL 触发更多 cookie 写入
async fn open_qq_login_window(app: &AppHandle) -> Result<String, String> {
    use tauri::WebviewUrl;

    let url = "https://y.qq.com/n/ryqq/profile";
    let warmup_url = "https://y.qq.com/n/ryqq/player";
    let label = "qqmusic-login";

    if let Some(existing) = app.get_webview_window(label) {
        let _ = existing.clear_all_browsing_data();
        let login_url = url.parse::<Url>().map_err(|e| e.to_string())?;
        let _ = existing.navigate(login_url);
        let _ = existing.set_focus();
        return Ok("window_refreshed".into());
    }

    let login_window = tauri::WebviewWindowBuilder::new(
        app,
        label,
        WebviewUrl::External("about:blank".parse().map_err(|e: url::ParseError| e.to_string())?),
    )
    .title("QQ 音乐登录")
    // 与其它窗口保持一致的 WebView2 参数（禁用 Chromium 自动媒体会话，避免与 smtc.rs 会话重复）
    .additional_browser_args("--disable-features=MediaSessionService,HardwareMediaKeyHandling --autoplay-policy=no-user-gesture-required")
    .inner_size(900.0, 720.0)
    .min_inner_size(760.0, 560.0)
    .build()
    .map_err(|e| format!("Failed to create login window: {e}"))?;

    let _ = login_window.clear_all_browsing_data();
    let login_url = url.parse::<Url>().map_err(|e| e.to_string())?;
    let _ = login_window.navigate(login_url);

    let win = login_window.clone();
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        // 等待页面加载
        tokio::time::sleep(Duration::from_secs(3)).await;

        let mut warmup_started = false;

        for _ in 0..150 {
            match win.cookies() {
                Ok(cookies) => {
                    let cookie_str = build_cookie_from_webview(&cookies, QQ_COOKIE_PRIORITY, is_qq_domain);

                    if cookie::qq_cookie_has_playback(&cookie_str) {
                        log::info!("[QQLogin] playbackReady cookie found, length: {}", cookie_str.len());
                        let _ = cookie::save_cookie(&app_handle, "qqmusic", &cookie_str);
                        set_provider_cookie("qqmusic", cookie_str).await;
                        let _ = win.close();
                        let qq_cookie = get_provider_cookie("qqmusic").await;
                        match qqmusic::login_info(&qq_cookie).await {
                            Ok(info) => {
                                if info.logged_in {
                                    let _ = app_handle.emit("qqmusic-login-success", &info);
                                } else {
                                    let _ = app_handle.emit("qqmusic-login-failed", "Cookie 无效或已过期");
                                }
                            }
                            Err(e) => {
                                let _ = app_handle.emit("qqmusic-login-failed", &e);
                            }
                        }
                        return;
                    } else if cookie::qq_cookie_has_login(&cookie_str) && !warmup_started {
                        warmup_started = true;
                        log::info!("[QQLogin] loggedIn but not playbackReady, starting warmup...");
                        if let Ok(warmup) = warmup_url.parse::<Url>() {
                            let _ = win.navigate(warmup);
                        }
                    }
                }
                Err(e) => {
                    log::warn!("[QQLogin] Failed to read cookies from webview: {e}");
                    // 窗口可能已被用户关闭，检测到后停止轮询
                    if !win.is_visible().unwrap_or(false) {
                        log::info!("[QQLogin] Login window closed, stop polling");
                        break;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(1200)).await;
        }

        // 超时 — 最后检查一次 cookie
        if let Ok(cookies) = win.cookies() {
            let cookie_str = build_cookie_from_webview(&cookies, QQ_COOKIE_PRIORITY, is_qq_domain);
            if cookie::qq_cookie_has_login(&cookie_str) {
                log::info!("[QQLogin] Timeout but found partial login, saving cookie");
                let _ = cookie::save_cookie(&app_handle, "qqmusic", &cookie_str);
                set_provider_cookie("qqmusic", cookie_str).await;
                let qq_cookie = get_provider_cookie("qqmusic").await;
                if let Ok(info) = qqmusic::login_info(&qq_cookie).await {
                    let _ = app_handle.emit("qqmusic-login-success", &info);
                }
                return;
            }
        }
        log::warn!("[QQLogin] Polling timed out after 5 minutes");
    });

    Ok("window_created".into())
}

// ============================================================
//  全局 Cookie 缓存 (多平台, 内存中)
// ============================================================

static APP_COOKIE: tokio::sync::RwLock<String> = tokio::sync::RwLock::const_new(String::new());
static KUGOU_COOKIE: tokio::sync::RwLock<String> = tokio::sync::RwLock::const_new(String::new());
static QQ_COOKIE: tokio::sync::RwLock<String> = tokio::sync::RwLock::const_new(String::new());

/// 获取网易云 cookie (向后兼容)
async fn get_app_cookie() -> String {
    APP_COOKIE.read().await.clone()
}

/// 设置网易云 cookie (向后兼容)
pub async fn set_app_cookie(cookie: String) {
    let mut guard = APP_COOKIE.write().await;
    *guard = cookie;
}

/// 加载网易云 cookie (向后兼容)
async fn load_app_cookie(app: &AppHandle) -> String {
    let cached = APP_COOKIE.read().await.clone();
    if !cached.is_empty() {
        return cached;
    }
    match cookie::load_cookie(app, "netease") {
        Ok(c) => {
            set_app_cookie(c.clone()).await;
            c
        }
        Err(_) => String::new(),
    }
}

/// 获取指定平台的 cookie
async fn get_provider_cookie(provider: &str) -> String {
    match provider {
        "netease" => APP_COOKIE.read().await.clone(),
        "kugou" => KUGOU_COOKIE.read().await.clone(),
        "qqmusic" => QQ_COOKIE.read().await.clone(),
        _ => String::new(),
    }
}

/// 设置指定平台的 cookie
async fn set_provider_cookie(provider: &str, cookie: String) {
    match provider {
        "netease" => {
            let mut guard = APP_COOKIE.write().await;
            *guard = cookie;
        }
        "kugou" => {
            let mut guard = KUGOU_COOKIE.write().await;
            *guard = cookie;
        }
        "qqmusic" => {
            let mut guard = QQ_COOKIE.write().await;
            *guard = cookie;
        }
        _ => {}
    }
}

/// 加载指定平台的 cookie (先检查内存缓存, 再从 store 加载)
async fn load_provider_cookie(app: &AppHandle, provider: &str) -> String {
    let cached = get_provider_cookie(provider).await;
    if !cached.is_empty() {
        return cached;
    }
    match cookie::load_cookie(app, provider) {
        Ok(c) => {
            set_provider_cookie(provider, c.clone()).await;
            c
        }
        Err(_) => String::new(),
    }
}

/// 初始化时从 store 加载 cookie 到内存
pub async fn init_cookie_cache(app: &AppHandle) {
    if let Ok(c) = cookie::load_cookie(app, "netease") {
        set_provider_cookie("netease", c).await;
    }
    if let Ok(c) = cookie::load_cookie(app, "kugou") {
        set_provider_cookie("kugou", c).await;
    }
    if let Ok(c) = cookie::load_cookie(app, "qqmusic") {
        set_provider_cookie("qqmusic", c).await;
    }
}
