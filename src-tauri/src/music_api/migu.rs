//! 咪咕音乐 API (参考 musiche-master web/src/utils/api/migu.ts)
//!
//! 所有接口均为明文 HTTP，唯一特殊处理是 listen/v2.0 的响应体
//! 为逐字节偏移的混淆流，需按固定密钥解码。

use serde_json::Value;

use super::models::{Artist, LoginInfo, Lyrics, Playlist, Song, SongUrlResult};

const MIGU_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/117.0.0.0 Safari/537.36";
const BY_HEADER: &str = "22210ca73bf1af2ec2eace74a96ee356";
const IMG_PREFIX: &str = "https://d.musicapp.migu.cn";
/// listen/v2.0 响应体解码密钥 (31 字节)
const LISTEN_KEY: &[u8] = b"Jk8qzuePiJ1qE3mDYhLQ3T73DtDoAhLP";

/// 榜单 columnId (热歌/新歌/原创)
pub const RANK_HOT: &str = "27186466";
pub const RANK_NEW: &str = "27553319";
pub const RANK_ORIGINAL: &str = "27553408";

fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .expect("Failed to build HTTP client")
}

/// 发送 GET 请求并返回 JSON
async fn request_json(url: &str, headers: &[(&str, &str)]) -> Result<Value, String> {
    let client = build_client();
    let mut req = client.get(url).header("User-Agent", MIGU_UA);
    for (key, value) in headers {
        req = req.header(*key, *value);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("Migu request failed: {e}"))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("Migu read response failed: {e}"))?;
    if !status.is_success() {
        return Err(format!("Migu HTTP {}: {}", status.as_u16(), &text[..text.len().min(200)]));
    }
    serde_json::from_str(&text)
        .map_err(|e| format!("Migu parse JSON failed: {e}, body: {}", &text[..text.len().min(200)]))
}

/// 发送 GET 请求并返回原始字节 (listen 接口响应体是混淆流，不能按文本读)
async fn request_bytes(url: &str, headers: &[(&str, &str)]) -> Result<Vec<u8>, String> {
    let client = build_client();
    let mut req = client.get(url).header("User-Agent", MIGU_UA);
    for (key, value) in headers {
        req = req.header(*key, *value);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("Migu request failed: {e}"))?;
    let status = resp.status();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Migu read body failed: {e}"))?;
    if !status.is_success() {
        return Err(format!("Migu HTTP {}", status.as_u16()));
    }
    Ok(bytes.to_vec())
}

/// 相对路径补全 (//xxx 补 https:，/prod 补图片前缀，http 强制 https)
fn pad_image(url: &str) -> String {
    let url = url.trim();
    if url.is_empty() {
        return String::new();
    }
    let mut url = if let Some(rest) = url.strip_prefix("//") {
        format!("https://{rest}")
    } else if url.starts_with("/prod") {
        format!("{IMG_PREFIX}{url}")
    } else if !url.starts_with("http") {
        format!("{IMG_PREFIX}/{url}")
    } else {
        url.to_string()
    };
    if url.starts_with("http://") {
        url = url.replacen("http://", "https://", 1);
    }
    url
}

/// NexBox 音质档位 → 咪咕 toneFlag
fn tone_flag_for(quality: &str) -> &'static str {
    match quality {
        "standard" => "PQ",
        "exhigh" => "HQ",
        "lossless" => "SQ",
        "hires" | "jymaster" => "ZQ",
        _ => "SQ",
    }
}

/// 咪咕 toneFlag → 展示用音质标签 (与网易云的中文标签风格一致)
fn quality_for_tone_flag(flag: &str) -> &'static str {
    match flag {
        "PQ" => "标准",
        "HQ" => "高品质",
        "SQ" => "无损",
        "ZQ" => "臻品",
        _ => "标准",
    }
}

fn tone_flag_degrade_order() -> [&'static str; 4] {
    ["PQ", "SQ", "HQ", "ZQ"]
}

/// 咪咕接口的 id 字段可能是字符串或数字，统一取成 String
fn json_str(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// 解析旧版 length 字段 ("00:04:30" / "04:30.06") → 毫秒
fn parse_length_ms(s: &str) -> u64 {
    let parts: Vec<u64> = s
        .split(':')
        .filter_map(|p| p.split('.').next()?.trim().parse::<u64>().ok())
        .collect();
    // 从右到左：秒、分、时
    let (sec, min, hour) = match parts.as_slice() {
        [s] => (*s, 0, 0),
        [m, s] => (*s, *m, 0),
        [h, m, s] => (*s, *m, *h),
        _ => return 0,
    };
    (hour * 3600 + min * 60 + sec) * 1000
}

/// 从旧版 albumImgs 数组取封面 (imgSizeType 03 → 02 → 01)
fn cover_from_album_imgs(data: &Value) -> String {
    let Some(arr) = data.get("albumImgs").and_then(|v| v.as_array()) else {
        return String::new();
    };
    for t in ["03", "02", "01"] {
        if let Some(u) = arr
            .iter()
            .find(|m| m.get("imgSizeType").and_then(|v| v.as_str()) == Some(t))
            .and_then(|m| m.get("img"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            return pad_image(u);
        }
    }
    String::new()
}

/// 解析歌曲字段，兼容两种结构：
/// - 新版 (搜索/歌单): name/singerList/img1-3/duration 秒/downloadTags
/// - 旧版 (榜单 objectInfo): songName/singer 字符串+artists 数组/albumImgs/length "hh:mm:ss"/vipFlag
fn parse_song3(data: &Value) -> Option<Song> {
    let name = data
        .get("name")
        .and_then(|v| v.as_str())
        .or_else(|| data.get("songName").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    if name.is_empty() {
        return None;
    }
    let copyright_id = json_str(data.get("copyrightId")).unwrap_or_default();
    let content_id = json_str(data.get("contentId"))
        .or_else(|| json_str(data.get("songId")))
        .unwrap_or_default();
    if copyright_id.is_empty() && content_id.is_empty() {
        return None;
    }

    // 歌手：新版 singerList 数组 (含 id/img 头像)；旧版 singer 字符串 + artists 数组
    let mut artists: Vec<Artist> = data
        .get("singerList")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| {
                    let n = s.get("name").and_then(|v| v.as_str())?;
                    Some(Artist {
                        id: json_str(s.get("id")),
                        name: n.to_string(),
                        pic_url: s
                            .get("img")
                            .and_then(|v| v.as_str())
                            .filter(|s| !s.is_empty())
                            .map(pad_image),
                        ..Default::default()
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let mut artist = artists
        .iter()
        .map(|a| a.name.as_str())
        .collect::<Vec<_>>()
        .join(" / ");
    if artist.is_empty() {
        artist = data
            .get("singer")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        artists = data
            .get("artists")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| {
                        json_str(a.get("name")).map(|n| Artist {
                            id: json_str(a.get("id")),
                            name: n,
                            ..Default::default()
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        if artist.is_empty() {
            artist = artists
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(" / ");
        }
    }

    // 封面：新版 img3/img2/img1；旧版 albumImgs
    let mut cover = data
        .get("img3")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| data.get("img2").and_then(|v| v.as_str()))
        .or_else(|| data.get("img1").and_then(|v| v.as_str()))
        .map(pad_image)
        .unwrap_or_default();
    if cover.is_empty() {
        cover = cover_from_album_imgs(data);
    }

    // 时长：新版 duration 秒；旧版 length 字符串
    let duration_ms = data
        .get("duration")
        .and_then(|v| v.as_u64())
        .map(|s| s * 1000)
        .unwrap_or_else(|| {
            parse_length_ms(data.get("length").and_then(|v| v.as_str()).unwrap_or(""))
        });

    // VIP：新版 downloadTags 含 vip；旧版 vipFlag == 1
    let vip = data
        .get("downloadTags")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().any(|t| t.as_str() == Some("vip")))
        .unwrap_or(false)
        || data.get("vipFlag").and_then(|v| v.as_i64()).unwrap_or(0) == 1;

    Some(Song {
        provider: "migu".into(),
        id: copyright_id,
        content_id: if content_id.is_empty() { None } else { Some(content_id) },
        name,
        artist: artist.clone(),
        artists,
        album: data
            .get("album")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        cover,
        duration: duration_ms,
        fee: if vip { 1 } else { 0 },
        playable: true,
        ..Default::default()
    })
}

fn parse_song_list(value: Option<&Value>) -> Vec<Song> {
    value
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(parse_song3).collect())
        .unwrap_or_default()
}

// ============================================================
//  登录
// ============================================================

/// 布尔类 JSON 值的宽松真值判断 (1/true/"1"/"true"/"vip" 均视为真)
fn truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().unwrap_or(0.0) > 0.0,
        Value::String(s) => matches!(s.trim(), "1" | "true" | "VIP" | "vip" | "是"),
        _ => false,
    }
}

/// VIP 识别：咪咕未公开 userInfoItem 的字段文档，按常见命名探测；
/// 同时把原始字段记录到日志，探测失败时可据此修正
fn detect_vip(user: &Value) -> (bool, i32, String) {
    let raw = serde_json::to_string(user).unwrap_or_default();
    let truncated: String = raw.chars().take(600).collect();
    log::info!("[Migu] userInfoItem: {truncated}");
    let is_vip = ["vipFlag", "vipStatus", "isVip", "vip", "isVipUser", "vipMark"]
        .iter()
        .any(|k| user.get(*k).map(truthy).unwrap_or(false))
        || user
            .get("vipLevel")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty() && s != "0")
            .unwrap_or(false)
        || user
            .get("vipEndTime")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty() && s != "0")
            .unwrap_or(false);
    let vip_type = user
        .get("vipType")
        .and_then(|v| v.as_i64())
        .unwrap_or(if is_vip { 1 } else { 0 }) as i32;
    let vip_level = user
        .get("vipLevel")
        .map(|v| match v {
            Value::String(s) => s.clone(),
            other => other.to_string().trim_matches('"').to_string(),
        })
        .unwrap_or_default();
    (is_vip, vip_type, vip_level)
}

/// 校验 cookie 并获取用户信息 (对照 musiche userInfo)
/// GET https://app.c.nf.migu.cn/pc/user/h5/queryUserInfo/v1.0
pub async fn login_info(cookie: &str) -> Result<LoginInfo, String> {
    if cookie.trim().is_empty() {
        return Ok(LoginInfo {
            provider: "migu".into(),
            ..Default::default()
        });
    }
    let json = request_json(
        "https://app.c.nf.migu.cn/pc/user/h5/queryUserInfo/v1.0",
        &[("Cookie", cookie)],
    )
    .await?;

    let user = json.get("userInfoItem").unwrap_or(&Value::Null);
    let user_id = user
        .get("userId")
        .map(|v| match v {
            Value::String(s) => s.clone(),
            other => other.to_string().trim_matches('"').to_string(),
        })
        .unwrap_or_default();
    if user_id.is_empty() {
        return Ok(LoginInfo {
            provider: "migu".into(),
            ..Default::default()
        });
    }
    let (is_vip, vip_type, vip_level) = detect_vip(user);
    Ok(LoginInfo {
        provider: "migu".into(),
        logged_in: true,
        user_id,
        nickname: user
            .get("nickName")
            .and_then(|v| v.as_str())
            .unwrap_or("咪咕用户")
            .to_string(),
        avatar: user
            .get("smallIcon")
            .and_then(|v| v.as_str())
            .map(pad_image)
            .unwrap_or_default(),
        vip_type,
        vip_level,
        is_vip,
        is_svip: false,
    })
}

// ============================================================
//  搜索
// ============================================================

/// 歌曲搜索 (对照 musiche search)
/// GET https://app.u.nf.migu.cn/pc/resource/song/item/search/v1.0
pub async fn search(keywords: &str, limit: u32, _cookie: &str) -> Result<Vec<Song>, String> {
    search_page(keywords, 1, limit.clamp(1, 50)).await
}

/// 歌曲搜索 (带页码)
async fn search_page(keywords: &str, page: u32, page_size: u32) -> Result<Vec<Song>, String> {
    let text: String = url::form_urlencoded::byte_serialize(keywords.as_bytes()).collect();
    let url = format!(
        "https://app.u.nf.migu.cn/pc/resource/song/item/search/v1.0?text={text}&pageNo={page}&pageSize={page_size}"
    );
    let json = request_json(
        &url,
        &[
            ("by", BY_HEADER),
            ("Referer", "https://music.migu.cn/"),
        ],
    )
    .await?;
    Ok(parse_song_list(Some(&json)))
}

/// 歌单搜索 (对照 musiche searchPlaylist)
/// GET https://app.u.nf.migu.cn/pc/v1.0/content/search_all.do
pub async fn playlist_search(keywords: &str, limit: u32) -> Result<Vec<Playlist>, String> {
    let page_size = limit.clamp(1, 50);
    let text: String = url::form_urlencoded::byte_serialize(keywords.as_bytes()).collect();
    let url = format!(
        "https://app.u.nf.migu.cn/pc/v1.0/content/search_all.do?text={text}&pageNo=1&pageSize={page_size}&searchSwitch=%7B%22songlist%22:+1%7D"
    );
    let json = request_json(
        &url,
        &[
            ("by", BY_HEADER),
            ("Referer", "https://music.migu.cn/"),
        ],
    )
    .await?;

    let list = json
        .get("songListResultData")
        .and_then(|d| d.get("result"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let playlists: Vec<Playlist> = list
        .iter()
        .filter_map(|m| {
            let id = json_str(m.get("id"))?;
            let name = m.get("name").and_then(|v| v.as_str())?;
            Some(Playlist {
                provider: "migu".into(),
                id,
                name: name.to_string(),
                cover: m
                    .get("musicListPicUrl")
                    .and_then(|v| v.as_str())
                    .map(pad_image)
                    .unwrap_or_default(),
                track_count: m.get("musicNum").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                ..Default::default()
            })
        })
        .collect();
    Ok(playlists)
}

/// 歌手搜索 (search_all.do 的 singerResultData，实测返回 [{id, name}] 无头像，
/// 头像用歌名搜索反查 singerList[].img 补齐)
pub async fn artist_search(keywords: &str, limit: u32) -> Result<Vec<Artist>, String> {
    let page_size = limit.clamp(1, 50);
    let text: String = url::form_urlencoded::byte_serialize(keywords.as_bytes()).collect();
    let url = format!(
        "https://app.u.nf.migu.cn/pc/v1.0/content/search_all.do?text={text}&pageNo=1&pageSize={page_size}&searchSwitch=%7B%22songlist%22:+1,%22singer%22:+1%7D"
    );
    let json = request_json(
        &url,
        &[
            ("by", BY_HEADER),
            ("Referer", "https://music.migu.cn/"),
        ],
    )
    .await?;

    let list = json
        .get("singerResultData")
        .and_then(|d| d.get("result"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut artists: Vec<Artist> = list
        .iter()
        .filter_map(|m| {
            let name = m.get("name").and_then(|v| v.as_str())?;
            if name.is_empty() {
                return None;
            }
            Some(Artist {
                id: json_str(m.get("id")),
                name: name.to_string(),
                ..Default::default()
            })
        })
        .collect();

    // 并发补头像 (最多前 10 个歌手，每个一次歌曲搜索)
    let pairs: Vec<(String, String)> = artists
        .iter()
        .take(10)
        .map(|a| (a.id.clone().unwrap_or_default(), a.name.clone()))
        .collect();
    let futs = pairs
        .iter()
        .map(|(id, name)| fetch_singer_avatar(id, name));
    let avatars = futures_util::future::join_all(futs).await;
    for (i, avatar) in avatars.into_iter().enumerate() {
        if let (Some(url), Some(a)) = (avatar, artists.get_mut(i)) {
            a.pic_url = Some(url);
        }
    }

    Ok(artists)
}

/// 反查歌手头像：按歌手名搜歌曲，找到 singerList 里匹配 (id 优先) 的歌手取 img
async fn fetch_singer_avatar(singer_id: &str, name: &str) -> Option<String> {
    let songs = search_page(name, 1, 10).await.ok()?;
    for s in songs {
        for a in &s.artists {
            let id_match = !singer_id.is_empty() && a.id.as_deref() == Some(singer_id);
            let name_match = a.name.trim() == name.trim();
            if (id_match || name_match) && a.pic_url.is_some() {
                return a.pic_url.clone();
            }
        }
    }
    None
}

/// 歌手歌曲：咪咕歌手歌曲接口已失效，用歌曲搜索 + 歌手名精确匹配过滤
pub async fn artist_songs(artist_name: &str, limit: u32, offset: u32) -> Result<Vec<Song>, String> {
    let name = artist_name.trim();
    if name.is_empty() {
        return Ok(Vec::new());
    }
    let page_size = limit.clamp(1, 50).max(30);
    let page = offset / page_size + 1;
    let songs = search_page(name, page, page_size).await?;

    let matched: Vec<Song> = songs
        .into_iter()
        .filter(|s| {
            s.artists
                .iter()
                .any(|a| a.name.trim().eq_ignore_ascii_case(name))
                || s.artist
                    .split(['/', ',', '、', ';', '&'])
                    .any(|p| p.trim().eq_ignore_ascii_case(name))
        })
        .collect();
    Ok(matched)
}

// ============================================================
//  我的歌单
// ============================================================

/// 从 actionUrl 提取 musicListId (官方歌单项)
fn extract_music_list_id(action_url: &str) -> Option<String> {
    let pos = action_url.find("musicListId=")?;
    let rest = &action_url[pos + "musicListId=".len()..];
    let end = rest.find('&').unwrap_or(rest.len());
    let id = rest[..end].trim();
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

/// 歌单条目的歌曲数量：字段名因接口而异，逐个候选尝试
fn playlist_count(m: &Value) -> u32 {
    ["musicNum", "musicCount", "songNum", "num", "count", "contentCount", "trackCount"]
        .iter()
        .find_map(|k| m.get(*k).and_then(|v| v.as_u64()))
        .unwrap_or(0) as u32
}

/// 用户歌单 (对照 musiche yours)
/// GET https://app.c.nf.migu.cn/pc/user/home-page/v2.0
pub async fn user_playlists(cookie: &str) -> Result<Vec<Playlist>, String> {
    let json = request_json(
        "https://app.c.nf.migu.cn/pc/user/home-page/v2.0",
        &[("Cookie", cookie)],
    )
    .await?;
    let data = json.get("data").unwrap_or(&Value::Null);

    let mut playlists: Vec<Playlist> = Vec::new();

    // 官方歌单 (userPrivateItems, actionUrl 带 musicListId)
    if let Some(items) = data.get("userPrivateItems").and_then(|v| v.as_array()) {
        for m in items {
            let action_url = m.get("actionUrl").and_then(|v| v.as_str()).unwrap_or("");
            let Some(id) = extract_music_list_id(action_url) else {
                continue;
            };
            playlists.push(Playlist {
                provider: "migu".into(),
                id,
                name: m.get("title").and_then(|v| v.as_str()).unwrap_or("官方歌单").to_string(),
                cover: m
                    .get("picUrl")
                    .and_then(|v| v.as_str())
                    .map(pad_image)
                    .unwrap_or_default(),
                track_count: playlist_count(m),
                ..Default::default()
            });
        }
    }

    // 收藏歌单 (resourceType === '2021')
    if let Some(items) = data
        .get("myCollectedMusicLists")
        .and_then(|v| v.get("collectMusicLists"))
        .and_then(|v| v.as_array())
    {
        for m in items {
            if m.get("resourceType").and_then(|v| v.as_str()) != Some("2021") {
                continue;
            }
            let Some(id) = m
                .get("musicListId")
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string().trim_matches('"').to_string(),
                })
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            playlists.push(Playlist {
                provider: "migu".into(),
                id,
                name: m.get("title").and_then(|v| v.as_str()).unwrap_or("收藏歌单").to_string(),
                cover: m
                    .get("imgItem")
                    .and_then(|v| v.get("img"))
                    .and_then(|v| v.as_str())
                    .map(pad_image)
                    .unwrap_or_default(),
                track_count: playlist_count(m),
                ..Default::default()
            });
        }
    }

    // 自建歌单
    if let Some(items) = data
        .get("myCreatedMusicLists")
        .and_then(|v| v.get("createdMusicLists"))
        .and_then(|v| v.as_array())
    {
        for m in items {
            let Some(id) = m
                .get("musicListId")
                .map(|v| match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string().trim_matches('"').to_string(),
                })
                .filter(|s| !s.is_empty())
            else {
                continue;
            };
            playlists.push(Playlist {
                provider: "migu".into(),
                id,
                name: m.get("title").and_then(|v| v.as_str()).unwrap_or("创建歌单").to_string(),
                cover: m
                    .get("imgItem")
                    .and_then(|v| v.get("img"))
                    .and_then(|v| v.as_str())
                    .map(pad_image)
                    .unwrap_or_default(),
                track_count: playlist_count(m),
                ..Default::default()
            });
        }
    }

    Ok(playlists)
}

// ============================================================
//  歌单详情
// ============================================================

/// 歌单元信息 (对照 musiche playlistInfo)
/// GET https://app.c.nf.migu.cn/resource/playlist/v2.0
async fn playlist_info(id: &str) -> Result<Playlist, String> {
    let url = format!("https://app.c.nf.migu.cn/resource/playlist/v2.0?playlistId={id}");
    let json = request_json(
        &url,
        &[
            ("by", BY_HEADER),
            ("Referer", "https://m.music.migu.cn/v4/playlist"),
            ("appid", "h5"),
        ],
    )
    .await?;
    let data = json.get("data").unwrap_or(&Value::Null);
    Ok(Playlist {
        provider: "migu".into(),
        id: data
            .get("musicListId")
            .and_then(|v| v.as_str())
            .unwrap_or(id)
            .to_string(),
        name: data.get("title").and_then(|v| v.as_str()).unwrap_or("咪咕歌单").to_string(),
        cover: data
            .get("imgItem")
            .and_then(|v| v.get("img"))
            .and_then(|v| v.as_str())
            .map(pad_image)
            .unwrap_or_default(),
        track_count: data.get("musicNum").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        ..Default::default()
    })
}

/// 歌单歌曲分页 (对照 musiche playlistDetail)
/// GET https://app.c.nf.migu.cn/MIGUM3.0/resource/playlist/song/v2.0?pageNo=&pageSize=30&playlistId=
async fn playlist_songs_page(id: &str, page_no: u32) -> Result<Vec<Song>, String> {
    let url = format!(
        "https://app.c.nf.migu.cn/MIGUM3.0/resource/playlist/song/v2.0?pageNo={page_no}&pageSize=30&playlistId={id}"
    );
    let json = request_json(
        &url,
        &[
            ("by", BY_HEADER),
            ("Referer", "https://m.music.migu.cn/v4/playlist"),
            ("appid", "h5"),
        ],
    )
    .await?;
    Ok(parse_song_list(json.get("data").and_then(|d| d.get("songList"))))
}

/// 完整歌单 (信息 + 全部歌曲，封顶 500 首防止超长歌单卡死)
pub async fn playlist_tracks(id: &str) -> Result<(Playlist, Vec<Song>), String> {
    let info = playlist_info(id).await?;
    let mut songs: Vec<Song> = Vec::new();
    let max_pages = 17u32; // 30/页 × 17 ≈ 510 首
    for page in 1..=max_pages {
        let chunk = playlist_songs_page(id, page).await?;
        let chunk_len = chunk.len();
        songs.extend(chunk);
        if chunk_len < 30 {
            break;
        }
    }
    Ok((info, songs))
}

/// 偏移式取歌单片段 (start/count → pageNo 换算，跨页补齐)
pub async fn playlist_tracks_range(id: &str, start: usize, count: usize) -> Result<Vec<Song>, String> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let first_page = (start / 30) as u32 + 1;
    let last_page = ((start + count - 1) / 30) as u32 + 1;

    let mut songs: Vec<Song> = Vec::new();
    for page in first_page..=last_page {
        let chunk = playlist_songs_page(id, page).await?;
        let chunk_len = chunk.len();
        songs.extend(chunk);
        if chunk_len < 30 {
            break;
        }
    }
    // 截取 [start, start+count) 段
    let begin = start.min(songs.len());
    let end = (start + count).min(songs.len());
    Ok(songs[begin..end].to_vec())
}

// ============================================================
//  播放 URL (listen/v2.0 + 字节流解码)
// ============================================================

pub struct ListenData {
    pub url: String,
    pub lrc_url: String,
    pub tone_flag: String,
}

/// 解码 listen/v2.0 的混淆响应体 (对照 musiche eF)
/// out[i-4] = body[i] + body[3] - KEY[(i-4) % 31]
fn decode_listen_body(body: &[u8]) -> Result<Value, String> {
    if body.len() <= 4 {
        return Err("Migu listen body too short".into());
    }
    let offset = body[3];
    let key = LISTEN_KEY;
    let decoded: Vec<u8> = body[4..]
        .iter()
        .enumerate()
        .map(|(s, &b)| b.wrapping_add(offset).wrapping_sub(key[s % key.len()]))
        .collect();
    let text = String::from_utf8_lossy(&decoded).to_string();
    serde_json::from_str(&text)
        .map_err(|e| format!("Migu decode listen body failed: {e}, text: {}", &text[..text.len().min(120)]))
}

/// 调用 listen 接口取播放 URL (对照 musiche listenUrl)
async fn listen_url(
    cookie: &str,
    uid: &str,
    content_id: &str,
    copyright_id: &str,
    tone_flag: &str,
) -> Result<ListenData, String> {
    let url = format!(
        "https://app.c.nf.migu.cn/strategy/pc/listen/v2.0?contentId={content_id}&copyrightId={copyright_id}&scene=&netType=01&resourceType=2&toneFlag={tone_flag}"
    );
    let body = request_bytes(
        &url,
        &[
            ("channel", "014X031"),
            ("cookie", cookie),
            ("uid", uid),
            ("appid", "h5"),
            ("birth", "h5page"),
            ("signature", "1"),
            ("referer", "https://music.migu.cn/"),
        ],
    )
    .await?;
    let json = decode_listen_body(&body)?;
    let data = json.get("data").unwrap_or(&Value::Null);

    let cannot_code = data
        .get("cannotCode")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let audio_url = data.get("url").and_then(|v| v.as_str()).unwrap_or("");
    if audio_url.is_empty() {
        let code = if cannot_code.is_empty() { "empty_url" } else { cannot_code };
        return Err(format!("MiguUrlBlocked:{code}"));
    }

    Ok(ListenData {
        url: audio_url.replacen("http://", "https://", 1),
        lrc_url: data
            .get("lrcUrl")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        tone_flag: tone_flag.to_string(),
    })
}

/// 获取播放 URL，440018 (无版权/需VIP) 时按 PQ→SQ→HQ→ZQ 降级重试
pub async fn song_url(
    cookie: &str,
    uid: &str,
    content_id: &str,
    copyright_id: &str,
    quality: &str,
) -> Result<SongUrlResult, String> {
    let target = tone_flag_for(quality);
    let order = tone_flag_degrade_order();

    // 目标档位优先，失败后按顺序换档
    let mut try_flags: Vec<&str> = vec![target];
    for flag in order {
        if flag != target {
            try_flags.push(flag);
        }
    }

    let mut last_err = String::new();
    for flag in try_flags {
        match listen_url(cookie, uid, content_id, copyright_id, flag).await {
            Ok(data) => {
                return Ok(SongUrlResult {
                    url: Some(data.url),
                    playable: true,
                    trial: false,
                    level: data.tone_flag.clone(),
                    quality: quality_for_tone_flag(&data.tone_flag).to_string(),
                    br: 0,
                    reason: None,
                    message: if data.tone_flag != target {
                        Some(format!("已降级为 {} 音质", data.tone_flag))
                    } else {
                        None
                    },
                    fee: None,
                });
            }
            Err(e) => {
                last_err = e;
                // 仅在版权受限时换档重试，其他错误直接返回
                if !last_err.contains("MiguUrlBlocked") {
                    return Err(last_err);
                }
            }
        }
    }

    Ok(SongUrlResult {
        url: None,
        playable: false,
        trial: false,
        level: target.to_string(),
        quality: quality_for_tone_flag(target).to_string(),
        br: 0,
        reason: Some(last_err),
        message: Some("无版权或需要咪咕 VIP".into()),
        fee: Some(1),
    })
}

// ============================================================
//  歌词
// ============================================================

/// 获取歌词：listen 返回 lrcUrl → 直接 GET 纯文本 LRC
pub async fn lyric(cookie: &str, uid: &str, content_id: &str, copyright_id: &str) -> Result<Lyrics, String> {
    let data = match listen_url(cookie, uid, content_id, copyright_id, "PQ").await {
        Ok(d) => d,
        Err(_) => {
            // PQ 取不到时降级试一次 SQ
            listen_url(cookie, uid, content_id, copyright_id, "SQ").await?
        }
    };
    if data.lrc_url.is_empty() {
        return Ok(Lyrics::default());
    }
    let client = build_client();
    let resp = client
        .get(&data.lrc_url)
        .header("User-Agent", MIGU_UA)
        .header("Referer", "https://music.migu.cn/")
        .send()
        .await
        .map_err(|e| format!("Migu lyric request failed: {e}"))?;
    let text = resp
        .text()
        .await
        .map_err(|e| format!("Migu lyric read failed: {e}"))?;
    Ok(Lyrics {
        lyric: text,
        ..Default::default()
    })
}

// ============================================================
//  榜单 / 推荐歌单
// ============================================================

/// 榜单歌曲 (对照 musiche ranking)
/// GET https://app.c.nf.migu.cn/MIGUM3.0/column/rank/h5/v1.0
pub async fn rank_songs(column_id: &str, limit: u32) -> Result<Vec<Song>, String> {
    let url = format!(
        "https://app.c.nf.migu.cn/MIGUM3.0/column/rank/h5/v1.0?pageSize={}&columnId={column_id}",
        limit.clamp(1, 100)
    );
    let json = request_json(&url, &[]).await?;
    let contents = json
        .get("data")
        .and_then(|d| d.get("columnInfo"))
        .and_then(|d| d.get("contents"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(contents
        .iter()
        .filter_map(|item| item.get("objectInfo")).filter_map(parse_song3)
        .collect())
}

/// 榜单列表 (三个内置榜单，逐个拉取封面与真实榜单名，如「尖叫热歌榜」)
pub async fn rank_list() -> Vec<Playlist> {
    let defs = [(RANK_HOT, "咪咕热歌榜"), (RANK_NEW, "咪咕新歌榜"), (RANK_ORIGINAL, "咪咕原创榜")];
    let mut out = Vec::new();
    for (id, fallback_name) in defs {
        let mut pl = Playlist {
            provider: "migu".into(),
            id: id.into(),
            name: fallback_name.into(),
            ..Default::default()
        };
        if let Ok(json) = request_json(
            &format!("https://app.c.nf.migu.cn/MIGUM3.0/column/rank/h5/v1.0?pageSize=1&columnId={id}"),
            &[],
        )
        .await
        {
            if let Some(ci) = json.get("data").and_then(|d| d.get("columnInfo")) {
                if let Some(t) = ci.get("columnTitle").and_then(|v| v.as_str()) {
                    if !t.is_empty() {
                        pl.name = t.to_string();
                    }
                }
                let cover = ci
                    .get("columnPicUrl")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .or_else(|| ci.get("columnSmallpicUrl").and_then(|v| v.as_str()))
                    .map(pad_image)
                    .unwrap_or_default();
                if !cover.is_empty() {
                    pl.cover = cover;
                }
            }
        }
        out.push(pl);
    }
    out
}

/// 推荐歌单广场 (对照 musiche recommend)
/// GET https://app.c.nf.migu.cn/pc/bmw/page-data/playlist-square-recommend/v1.0
pub async fn recommend_playlists() -> Result<Vec<Playlist>, String> {
    let url = format!(
        "https://app.c.nf.migu.cn/pc/bmw/page-data/playlist-square-recommend/v1.0?templateVersion=2&_t={}",
        chrono::Utc::now().timestamp_millis()
    );
    // blob 风格随机 cookie id (musiche 用 URL.createObjectURL(new Blob()) 生成)
    let migu_cookie_id = uuid::Uuid::new_v4().simple().to_string();
    let json = request_json(
        &url,
        &[
            ("by", BY_HEADER),
            ("Referer", "https://m.music.migu.cn/v4/music/playlist"),
            ("Cookie", &format!("migu_cookie_id={migu_cookie_id}")),
            ("appid", "h5"),
        ],
    )
    .await?;

    let mut playlists: Vec<Playlist> = Vec::new();
    // 三层嵌套 contents，resType === '2021' 且 title !== '标题' 才是歌单项
    fn collect(items: Option<&Vec<Value>>, out: &mut Vec<Playlist>) {
        let Some(items) = items else { return };
        for m in items {
            if m.get("resType").and_then(|v| v.as_str()) == Some("2021")
                && m.get("title").and_then(|v| v.as_str()) != Some("标题")
            {
                if let (Some(id), Some(name)) = (
                    m.get("resId").and_then(|v| v.as_str()),
                    m.get("txt").and_then(|v| v.as_str()),
                ) {
                    out.push(Playlist {
                        provider: "migu".into(),
                        id: id.to_string(),
                        name: name.to_string(),
                        cover: m
                            .get("img")
                            .and_then(|v| v.as_str())
                            .map(pad_image)
                            .unwrap_or_default(),
                        ..Default::default()
                    });
                }
            }
            collect(m.get("contents").and_then(|v| v.as_array()), out);
        }
    }
    collect(
        json.get("data")
            .and_then(|d| d.get("contents"))
            .and_then(|v| v.as_array()),
        &mut playlists,
    );
    Ok(playlists)
}
