pub mod audio_proxy;
pub mod cookie;
pub mod crypto;
pub mod kugou;
pub mod migu;
pub mod models;
pub mod netease;
pub mod qishui;
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
//  Tauri Commands - 咪咕音乐
// ============================================================

#[tauri::command]
pub async fn migu_search(app: AppHandle, keywords: String, limit: Option<u32>) -> Result<Vec<Song>, String> {
    let cookie = load_provider_cookie(&app, "migu").await;
    migu::search(&keywords, limit.unwrap_or(30), &cookie).await
}

#[tauri::command]
pub async fn migu_playlist_search(app: AppHandle, keywords: String, limit: Option<u32>) -> Result<Vec<Playlist>, String> {
    let _ = load_provider_cookie(&app, "migu").await;
    migu::playlist_search(&keywords, limit.unwrap_or(30)).await
}

#[tauri::command]
pub async fn migu_artist_search(keywords: String, limit: Option<u32>) -> Result<Vec<Artist>, String> {
    migu::artist_search(&keywords, limit.unwrap_or(30)).await
}

#[tauri::command]
pub async fn migu_artist_songs(artist_name: String, limit: Option<u32>, offset: Option<u32>) -> Result<Vec<Song>, String> {
    migu::artist_songs(&artist_name, limit.unwrap_or(50), offset.unwrap_or(0)).await
}

#[tauri::command]
pub async fn migu_song_url(
    app: AppHandle,
    content_id: String,
    copyright_id: String,
    quality: Option<String>,
) -> Result<SongUrlResult, String> {
    let cookie = load_provider_cookie(&app, "migu").await;
    let uid = load_migu_uid(&app).await;
    migu::song_url(
        &cookie,
        &uid,
        &content_id,
        &copyright_id,
        &quality.unwrap_or_else(|| "lossless".into()),
    )
    .await
}

#[tauri::command]
pub async fn migu_lyric(app: AppHandle, content_id: String, copyright_id: String) -> Result<Lyrics, String> {
    let cookie = load_provider_cookie(&app, "migu").await;
    let uid = load_migu_uid(&app).await;
    migu::lyric(&cookie, &uid, &content_id, &copyright_id).await
}

#[tauri::command]
pub async fn migu_login_status(app: AppHandle) -> Result<LoginInfo, String> {
    let cookie = load_provider_cookie(&app, "migu").await;
    migu::login_info(&cookie).await
}

#[tauri::command]
pub async fn migu_logout(app: AppHandle) -> Result<(), String> {
    cookie::clear_cookie(&app, "migu")?;
    set_provider_cookie("migu", String::new()).await;
    set_migu_uid(String::new()).await;
    Ok(())
}

#[tauri::command]
pub async fn migu_user_playlists(app: AppHandle) -> Result<Vec<Playlist>, String> {
    let cookie = load_provider_cookie(&app, "migu").await;
    migu::user_playlists(&cookie).await
}

#[tauri::command]
pub async fn migu_playlist_tracks(id: String) -> Result<(Playlist, Vec<Song>), String> {
    migu::playlist_tracks(&id).await
}

#[tauri::command]
pub async fn migu_playlist_tracks_range(id: String, start: usize, count: usize) -> Result<Vec<Song>, String> {
    migu::playlist_tracks_range(&id, start, count).await
}

#[tauri::command]
pub async fn migu_rank_list() -> Result<Vec<Playlist>, String> {
    Ok(migu::rank_list().await)
}

#[tauri::command]
pub async fn migu_rank_songs(rank_id: String, limit: Option<u32>) -> Result<Vec<Song>, String> {
    migu::rank_songs(&rank_id, limit.unwrap_or(30)).await
}

#[tauri::command]
pub async fn migu_recommend_playlists() -> Result<Vec<Playlist>, String> {
    migu::recommend_playlists().await
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
    let migu_cookie = load_provider_cookie(&app, "migu").await;
    let qishui_cookie = load_provider_cookie(&app, "qishui").await;

    let (netease_result, kugou_result, qq_result, migu_result, qishui_result) = tokio::join!(
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
        async {
            if !migu_cookie.is_empty() {
                migu::login_info(&migu_cookie).await.ok()
            } else { None }
        },
        async {
            if !qishui_cookie.is_empty() && qishui::qishui_cookie_has_login(&qishui_cookie) {
                match qishui::qishui_fetch_profile(&qishui_cookie).await {
                    Some(info) => Some(info),
                    // 网络未就绪导致取不到：优先用上次缓存的信息（含 VIP），避免开机后会员标识丢失
                    None => cached_profile("qishui")
                        .or_else(|| Some(qishui::qishui_status_info(&qishui_cookie))),
                }
            } else { None }
        },
    );

    // 缓存成功拉到的个人信息，供下次冷启动网络未就绪时兜底
    for info in [&netease_result, &kugou_result, &qq_result, &migu_result].into_iter().flatten() {
        remember_profile(info);
    }

    let mut result = HashMap::new();
    if let Some(info) = netease_result { result.insert("netease".into(), info); }
    if let Some(info) = kugou_result { result.insert("kugou".into(), info); }
    if let Some(info) = qq_result { result.insert("qqmusic".into(), info); }
    if let Some(info) = migu_result { result.insert("migu".into(), info); }
    if let Some(info) = qishui_result { result.insert("qishui".into(), info); }
    Ok(result)
}

/// 切换播放源平台
#[tauri::command]
pub async fn music_switch_provider(app: AppHandle, provider: String) -> Result<(), String> {
    match provider.as_str() {
        "netease" | "kugou" | "qqmusic" | "migu" | "qishui" => {}
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

/// 咪咕登录 cookie 优先级 (登录判定靠 queryUserInfo 接口校验，此处仅影响拼接顺序)
const MIGU_COOKIE_PRIORITY: &[&str] = &[
    "MUSIC_SESSION",
    "migu_cookie_id",
    "ESAFEID",
    "mtfk",
    "P_CID",
    "migu_music_tail",
    "migu_userid",
    "SSON",
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

/// 检查域名是否属于咪咕
fn is_migu_domain(domain: &str) -> bool {
    let d = domain.trim_start_matches('.').to_lowercase();
    d == "migu.cn" || d.ends_with(".migu.cn")
}


/// 为登录窗口安装第三方登录弹窗处理 (`window.open`)
///
/// ⭐ 这是「点了 QQ/微信/微博 图标没反应」的根因。
///
/// 各平台登录页的第三方登录按钮走的是 `window.open`，实参形如：
/// ```js
/// window.open(
///   "https://graph.qq.com/oauth2.0/show?...",
///   "QQ帐号",
///   "width=502,height=390,left=,menubar=0,scrollbars=1,status=1,titlebar=0,toobar=0,location=1,resizable=yes"
/// );
/// ```
/// Tauri 默认 **不处理** 新窗口请求 (返回 Deny)，于是弹窗被静默丢弃，
/// 表现为「点了没反应、没有任何报错」。
///
/// 这里改成真正创建子窗口，把第三方登录页面装进去。
/// 子窗口与父窗口共用同一个 WebView2 用户数据目录，因此第三方回跳到
/// 平台域名后写下的登录 cookie 能被父窗口的轮询读到、从而完成登录。
///
/// 子窗口还会在**回跳到平台域名**后自动关闭 —— 见 `is_login_popup_done_url`。
fn attach_login_popup_handler<'a, R: tauri::Runtime, M: tauri::Manager<R>>(
    builder: tauri::webview::WebviewWindowBuilder<'a, R, M>,
    app: &AppHandle<R>,
) -> tauri::webview::WebviewWindowBuilder<'a, R, M> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static POPUP_SEQ: AtomicU32 = AtomicU32::new(0);

    let app_handle = app.clone();
    builder.on_new_window(move |url, _features| {
        let label = format!("music-login-popup-{}", POPUP_SEQ.fetch_add(1, Ordering::Relaxed));
        log::info!("[MusicLogin] window.open intercepted: {url}");
        let builder = tauri::WebviewWindowBuilder::new(
            &app_handle,
            &label,
            tauri::WebviewUrl::External(url.clone()),
        )
        .title("帐号登录")
        .inner_size(560.0, 640.0)
        .additional_browser_args("--disable-features=MediaSessionService,HardwareMediaKeyHandling,msWebOOUI,msPdfOOUI,msSmartScreenProtection,msEdgeAutofill,msEdgeShopping,msEdgeWallet --autoplay-policy=no-user-gesture-required --disable-background-networking --disable-client-side-phishing-detection --disable-component-update --disable-default-apps --disable-extensions --disable-sync")
        // OAuth 走完后第三方会回跳到平台自己的域名，此时登录已完成，
        // 子窗口要自己关掉，否则用户得手动点叉。
        .on_page_load(|window, payload| {
            if payload.event() != tauri::webview::PageLoadEvent::Finished {
                return;
            }
            let loaded = payload.url().clone();
            if login_popup_should_close(&loaded) {
                log::info!("[MusicLogin] popup reached done url, closing: {loaded}");
                let _ = window.close();
            }
        });

        match builder.build() {
            Ok(window) => tauri::webview::NewWindowResponse::Create { window },
            Err(e) => {
                log::warn!("[MusicLogin] failed to create popup for {url}: {e}");
                tauri::webview::NewWindowResponse::Deny
            }
        }
    })
}

/// 关闭登录窗口，并顺带关掉它弹出的所有第三方登录子窗口
///
/// 子窗口的 label 前缀是 `music-login-popup-`。正常情况下子窗口会在
/// OAuth 回跳到平台域名时自己关闭 (见 `login_popup_should_close`)，
/// 但如果用户中途取消授权、或第三方页面停在某个未知域名上，
/// 子窗口就会残留。登录成功后统一清一次，保证不留孤儿窗口。
fn close_login_window_with_popups<R: tauri::Runtime>(
    app: &AppHandle<R>,
    login_label: &str,
) {
    for (_label, win) in app.webview_windows() {
        if _label.starts_with("music-login-popup-") {
            log::info!("[MusicLogin] closing leftover popup: {_label}");
            let _ = win.close();
        }
    }
    if let Some(win) = app.get_webview_window(login_label) {
        let _ = win.close();
    }
}

/// 判断登录子窗口当前 URL 是否表示「第三方授权已走完、可以关窗了」
///
/// 实际回跳链路（以 QQ 为例）：
/// ```text
/// graph.qq.com/oauth2.0/show?...     ← 第三方授权页
///   → {平台域名}/...                  ← 平台收 code / 换票据
///   → {平台域名}/...                  ← 回主站，cookie 已落地
/// ```
/// 所以判据是「**已经回到平台域名**」。同时必须排除第三方授权域，
/// 否则 `graph.qq.com` 若含平台串就会误关。
fn login_popup_should_close(url: &Url) -> bool {
    let host = match url.host_str() {
        Some(h) => h.to_lowercase(),
        None => return false,
    };

    // 第三方授权域：出现这些说明还在授权流程中，绝不能关
    const THIRD_PARTY: &[&str] = &[
        "graph.qq.com",
        "open.weixin.qq.com",
        "api.weibo.com",
        "ptlogin2.qq.com",
        "xui.ptlogin2.qq.com",
        "openapi.weibo.com",
        "passport.",
    ];
    if THIRD_PARTY.iter().any(|d| host == *d || host.ends_with(&format!(".{d}"))) {
        return false;
    }

    // 回到平台域名 = 授权完成
    const PLATFORM: &[&str] = &[
        "163.com",              // 网易云
        "kugou.com",            // 酷狗
        "kugoucdn.com",
        "qq.com",               // QQ 音乐 (y.qq.com 等)
        "migu.cn",              // 咪咕
        "migucdn.com",
    ];
    PLATFORM
        .iter()
        .any(|d| host == *d || host.ends_with(&format!(".{d}")))
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
        "migu" => open_migu_login_window(&app).await,
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

    let login_window = attach_login_popup_handler(
        tauri::WebviewWindowBuilder::new(
            app,
            label,
            WebviewUrl::External("about:blank".parse().map_err(|e: url::ParseError| e.to_string())?),
        )
        .title("网易云音乐登录")
        // 与其它窗口保持一致的 WebView2 参数（禁用 Chromium 自动媒体会话，避免与 smtc.rs 会话重复）
        .additional_browser_args("--disable-features=MediaSessionService,HardwareMediaKeyHandling,msWebOOUI,msPdfOOUI,msSmartScreenProtection,msEdgeAutofill,msEdgeShopping,msEdgeWallet --autoplay-policy=no-user-gesture-required --disable-background-networking --disable-client-side-phishing-detection --disable-component-update --disable-default-apps --disable-extensions --disable-sync")
        .inner_size(940.0, 760.0)
        .min_inner_size(780.0, 580.0),
        app,
    )
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
                        close_login_window_with_popups(&app_handle, "netease-login");
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

    let login_window = attach_login_popup_handler(
        tauri::WebviewWindowBuilder::new(
            app,
            label,
            WebviewUrl::External("about:blank".parse().map_err(|e: url::ParseError| e.to_string())?),
        )
        .title("酷狗音乐登录")
        // 与其它窗口保持一致的 WebView2 参数（禁用 Chromium 自动媒体会话，避免与 smtc.rs 会话重复）
        .additional_browser_args("--disable-features=MediaSessionService,HardwareMediaKeyHandling,msWebOOUI,msPdfOOUI,msSmartScreenProtection,msEdgeAutofill,msEdgeShopping,msEdgeWallet --autoplay-policy=no-user-gesture-required --disable-background-networking --disable-client-side-phishing-detection --disable-component-update --disable-default-apps --disable-extensions --disable-sync")
        .inner_size(900.0, 720.0)
        .min_inner_size(760.0, 560.0),
        app,
    )
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
                        close_login_window_with_popups(&app_handle, "kugou-login");
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

    let login_window = attach_login_popup_handler(
        tauri::WebviewWindowBuilder::new(
            app,
            label,
            WebviewUrl::External("about:blank".parse().map_err(|e: url::ParseError| e.to_string())?),
        )
        .title("QQ 音乐登录")
        // 与其它窗口保持一致的 WebView2 参数（禁用 Chromium 自动媒体会话，避免与 smtc.rs 会话重复）
        .additional_browser_args("--disable-features=MediaSessionService,HardwareMediaKeyHandling,msWebOOUI,msPdfOOUI,msSmartScreenProtection,msEdgeAutofill,msEdgeShopping,msEdgeWallet --autoplay-policy=no-user-gesture-required --disable-background-networking --disable-client-side-phishing-detection --disable-component-update --disable-default-apps --disable-extensions --disable-sync")
        .inner_size(900.0, 720.0)
        .min_inner_size(760.0, 560.0),
        app,
    )
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
                        close_login_window_with_popups(&app_handle, "qqmusic-login");
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

/// 打开咪咕登录窗口
/// 咪咕没有可判定的固定登录 cookie 名，改为「cookie 串变化时调 queryUserInfo 校验 userId」，
/// 校验通过即视为登录完成 (URL 为用户指定的 music.migu.cn/v5/ 网页登录)
async fn open_migu_login_window(app: &AppHandle) -> Result<String, String> {
    use tauri::WebviewUrl;

    let url = "https://music.migu.cn/v5/";
    let label = "migu-login";

    if let Some(existing) = app.get_webview_window(label) {
        let _ = existing.clear_all_browsing_data();
        let login_url = url.parse::<Url>().map_err(|e| e.to_string())?;
        let _ = existing.navigate(login_url);
        let _ = existing.set_focus();
        return Ok("window_refreshed".into());
    }

    let login_window = attach_login_popup_handler(
        tauri::WebviewWindowBuilder::new(
            app,
            label,
            WebviewUrl::External("about:blank".parse().map_err(|e: url::ParseError| e.to_string())?),
        )
        .title("咪咕音乐登录")
        // 与其它窗口保持一致的 WebView2 参数（禁用 Chromium 自动媒体会话，避免与 smtc.rs 会话重复）
        .additional_browser_args("--disable-features=MediaSessionService,HardwareMediaKeyHandling,msWebOOUI,msPdfOOUI,msSmartScreenProtection,msEdgeAutofill,msEdgeShopping,msEdgeWallet --autoplay-policy=no-user-gesture-required --disable-background-networking --disable-client-side-phishing-detection --disable-component-update --disable-default-apps --disable-extensions --disable-sync")
        .inner_size(900.0, 720.0)
        .min_inner_size(760.0, 560.0),
        app,
    )
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

        let mut last_cookie = String::new();

        for _ in 0..150 {
            match win.cookies() {
                Ok(cookies) => {
                    let cookie_str = build_cookie_from_webview(&cookies, MIGU_COOKIE_PRIORITY, is_migu_domain);

                    // 仅在 cookie 串变化时调接口校验，避免轮询打爆 queryUserInfo
                    if !cookie_str.is_empty() && cookie_str != last_cookie {
                        last_cookie = cookie_str.clone();
                        match migu::login_info(&cookie_str).await {
                            Ok(info) if info.logged_in => {
                                log::info!("[MiguLogin] login validated, uid={}", info.user_id);
                                let _ = cookie::save_cookie(&app_handle, "migu", &cookie_str);
                                let _ = cookie::save_user_id(&app_handle, "migu", &info.user_id);
                                set_provider_cookie("migu", cookie_str).await;
                                set_migu_uid(info.user_id.clone()).await;
                                close_login_window_with_popups(&app_handle, "migu-login");
                                let _ = app_handle.emit("migu-login-success", &info);
                                return;
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    log::warn!("[MiguLogin] Failed to read cookies from webview: {e}");
                    // 窗口可能已被用户关闭，检测到后停止轮询
                    if !win.is_visible().unwrap_or(false) {
                        log::info!("[MiguLogin] Login window closed, stop polling");
                        break;
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(1200)).await;
        }

        // 超时 — 最后再校验一次
        if let Ok(cookies) = win.cookies() {
            let cookie_str = build_cookie_from_webview(&cookies, MIGU_COOKIE_PRIORITY, is_migu_domain);
            if !cookie_str.is_empty() {
                if let Ok(info) = migu::login_info(&cookie_str).await {
                    if info.logged_in {
                        log::info!("[MiguLogin] Timeout but login validated, saving cookie");
                        let _ = cookie::save_cookie(&app_handle, "migu", &cookie_str);
                        let _ = cookie::save_user_id(&app_handle, "migu", &info.user_id);
                        set_provider_cookie("migu", cookie_str).await;
                        set_migu_uid(info.user_id.clone()).await;
                        close_login_window_with_popups(&app_handle, "migu-login");
                        let _ = app_handle.emit("migu-login-success", &info);
                        return;
                    }
                }
            }
        }
        let _ = app_handle.emit("migu-login-failed", "登录超时或未完成登录");
        log::warn!("[MiguLogin] Polling timed out after 5 minutes");
    });

    Ok("window_created".into())
}

// ============================================================
//  全局 Cookie 缓存 (多平台, 内存中)
// ============================================================

static APP_COOKIE: tokio::sync::RwLock<String> = tokio::sync::RwLock::const_new(String::new());
static KUGOU_COOKIE: tokio::sync::RwLock<String> = tokio::sync::RwLock::const_new(String::new());
static QQ_COOKIE: tokio::sync::RwLock<String> = tokio::sync::RwLock::const_new(String::new());
static MIGU_COOKIE: tokio::sync::RwLock<String> = tokio::sync::RwLock::const_new(String::new());
static QISHUI_COOKIE: tokio::sync::RwLock<String> = tokio::sync::RwLock::const_new(String::new());
/// 咪咕 uid (listen 接口请求头需要)
static MIGU_UID: tokio::sync::RwLock<String> = tokio::sync::RwLock::const_new(String::new());

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
        "migu" => MIGU_COOKIE.read().await.clone(),
        "qishui" => QISHUI_COOKIE.read().await.clone(),
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
        "migu" => {
            let mut guard = MIGU_COOKIE.write().await;
            *guard = cookie;
        }
        "qishui" => {
            let mut guard = QISHUI_COOKIE.write().await;
            *guard = cookie;
        }
        _ => {}
    }
}

async fn set_migu_uid(uid: String) {
    let mut guard = MIGU_UID.write().await;
    *guard = uid;
}

/// 加载咪咕 uid (缓存 → store → 兜底用 cookie 现查 queryUserInfo 并持久化)
async fn load_migu_uid(app: &AppHandle) -> String {
    let cached = MIGU_UID.read().await.clone();
    if !cached.is_empty() {
        return cached;
    }
    if let Ok(uid) = cookie::load_user_id(app, "migu") {
        if !uid.is_empty() {
            set_migu_uid(uid.clone()).await;
            return uid;
        }
    }
    let cookie = get_provider_cookie("migu").await;
    if cookie.is_empty() {
        return String::new();
    }
    if let Ok(info) = migu::login_info(&cookie).await {
        if info.logged_in && !info.user_id.is_empty() {
            let _ = cookie::save_user_id(app, "migu", &info.user_id);
            set_migu_uid(info.user_id.clone()).await;
            return info.user_id;
        }
    }
    String::new()
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
    if let Ok(c) = cookie::load_cookie(app, "migu") {
        set_provider_cookie("migu", c).await;
    }
    if let Ok(c) = cookie::load_cookie(app, "qishui") {
        set_provider_cookie("qishui", c).await;
    }
    if let Ok(uid) = cookie::load_user_id(app, "migu") {
        set_migu_uid(uid).await;
    }
}

// ============================================================
//  个人信息磁盘缓存
//  刚开机时网络/DNS 往往还没就绪，/me 之类的接口会失败；这里缓存上一次
//  成功拉到的个人信息（含 VIP 标识），避免重启后会员标识丢失。
// ============================================================

fn profile_cache_path() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("NexBox")
        .join("music_profiles.json")
}

fn load_profile_cache() -> HashMap<String, LoginInfo> {
    std::fs::read_to_string(profile_cache_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// 记录一次成功的个人信息
pub(crate) fn remember_profile(info: &LoginInfo) {
    if !info.logged_in || info.nickname.is_empty() {
        return;
    }
    let mut map = load_profile_cache();
    map.insert(info.provider.clone(), info.clone());
    let path = profile_cache_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string(&map) {
        let _ = std::fs::write(&path, text);
    }
}

/// 读取缓存的个人信息
pub(crate) fn cached_profile(provider: &str) -> Option<LoginInfo> {
    load_profile_cache().remove(provider)
}

#[cfg(test)]
mod login_popup_tests {
    use super::login_popup_should_close;

    fn u(s: &str) -> url::Url {
        s.parse().expect("test url should parse")
    }

    /// 真实的 QQ 登录回跳链：每一步都不能提前关窗，最后一步必须关
    #[test]
    fn qq_redirect_chain_closes_only_at_end() {
        // 1) 第三方授权页 —— 必须保持打开
        assert!(
            !login_popup_should_close(&u(
                "https://graph.qq.com/oauth2.0/show?which=ConfirmPage&display=pc&client_id=100243533"
            )),
            "QQ 授权页不应被关闭"
        );

        // 2) 平台收 code 的中间页 —— 已回到平台域名，可以关
        //    (票据已经写进 cookie，继续停留没有意义)
        assert!(
            login_popup_should_close(&u(
                "https://music.163.com/back/qq?code=xxx"
            )),
            "回到网易云域名后应关闭"
        );

        // 3) 平台换票据页
        assert!(
            login_popup_should_close(&u("https://y.qq.com/portal/profile.html")),
            "回到 QQ 音乐域名后应关闭"
        );

        // 4) 主站
        assert!(
            login_popup_should_close(&u("https://www.kugou.com/")),
            "回到酷狗主站后应关闭"
        );
    }

    /// 其它平台的第三方域不能误判为「已完成」
    #[test]
    fn other_third_party_domains_stay_open() {
        let cases = [
            "https://open.weixin.qq.com/connect/qrconnect?appid=wx41c1275bb3e28427",
            "https://api.weibo.com/oauth2/authorize?client_id=2972927130",
            "https://xui.ptlogin2.qq.com/cgi-bin/xlogin",
        ];
        for c in cases {
            assert!(
                !login_popup_should_close(&u(c)),
                "第三方授权域不应被关闭: {c}"
            );
        }
    }

    /// 空 host / about:blank 不能触发关闭
    #[test]
    fn blank_and_hostless_urls_stay_open() {
        assert!(!login_popup_should_close(&u("about:blank")));
    }

    /// 伪造相似域名不能被当作平台域 (防后缀混淆)
    #[test]
    fn lookalike_domains_are_not_treated_as_platform() {
        for c in [
            "https://163.com.evil.com/callback",
            "https://notkugou.com/callback",
            "https://fakemigu.cn.evil.com/callback",
        ] {
            assert!(
                !login_popup_should_close(&u(c)),
                "相似域名不应被误判: {c}"
            );
        }
    }
}
