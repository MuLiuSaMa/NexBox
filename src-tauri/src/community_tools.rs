//! 社区工具板块：从 GitCode 仓库读取社区工具、GitCode OAuth 授权码登录、通过 PR 提交/删除社区工具、下载/安装/启动。
//!
//! 仓库：https://gitcode.com/MuLiuSaMa/NexBox-Tools （独立仓库，本地根目录为 D:\NexBox\NexBox-Tools，主工程 .gitignore 已排除）
//! 目录约定：plugins/{分类}/{工具id}/{plugin.json|*.zip|icon}

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};
use futures_util::future::join_all;
use tokio::sync::Semaphore;
use tauri::{Emitter, Manager};
use tauri_plugin_store::StoreExt;

/// 读取社区工具列表时的最大并发请求数（GitCode 有 50/min 限流，宁可稍慢也不要触发 429）
const MAX_CONCURRENT: usize = 12;

// ============================== 配置常量 ==============================

const OWNER: &str = "MuLiuSaMa";
const REPO: &str = "NexBox-Tools";
const PLUGINS_PATH: &str = "plugins";
const API_BASE: &str = "https://api.gitcode.com/api/v5";
const RAW_BASE: &str = "https://gitcode.com/MuLiuSaMa/NexBox-Tools/-/raw/main";
const OAUTH_AUTHORIZE: &str = "https://gitcode.com/oauth/authorize";
const OAUTH_TOKEN: &str = "https://gitcode.com/oauth/token";
const REDIRECT_PORT: u16 = 38991;
/// 需在 GitCode 控制台注册 OAuth 应用后填入（用户已提供）
const CLIENT_ID: &str = "0f99323f514149dc9e0bfb07c5bf5bab";
const CLIENT_SECRET: &str = "9765ef4675c848b681a111141da4a0ba";
const OAUTH_SCOPE: &str = "all_projects all_repository all_pr all_user";

/// 走仓库 API 直传的压缩包大小上限（GitCode contents API 单文件限制，较大包请改用 download_url 外链）
const MAX_CONTENTS_UPLOAD: u64 = 1024 * 1024;

/// 社区插件仓库只读下载用的 GitCode 个人访问令牌(PAT)。
/// 仅用于读取/下载以提升接口限额，权限请只开 Repository「上传下载」类，勿开写操作。
/// 留空则退化为匿名请求。⚠ 公开仓库勿硬编码真实令牌，否则会泄露。
const TOOL_API_PAT: &str = "WmAt3-nHJiYGKrCby8bGajtw";

// ============================== 数据模型 ==============================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunityTool {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    pub category: String,
    pub tags: Vec<String>,
    pub author: Option<String>,
    pub publisher: Option<String>,
    pub homepage: Option<String>,
    pub submitted_at: Option<String>,
    pub launch_target: Option<String>,
    pub file: Option<String>,
    pub icon: Option<String>,
    pub download_url: Option<String>,
    pub download_filter: Option<String>,
    pub repo_path: String,
    /// not_installed | installed | update_available
    pub install_status: String,
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCodeUser {
    pub login: String,
    pub avatar_url: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCodeLoginStatus {
    pub logged_in: bool,
    pub user: Option<GitCodeUser>,
}

// ============================== 缓存 ==============================

static CACHE: OnceLock<StdMutex<Option<CacheEntry>>> = OnceLock::new();
struct CacheEntry {
    time: Instant,
    tools: Vec<CommunityTool>,
}
const CACHE_DURATION: Duration = Duration::from_secs(600);

fn cache_lock() -> &'static StdMutex<Option<CacheEntry>> {
    CACHE.get_or_init(|| StdMutex::new(None))
}

fn read_cache() -> Option<Vec<CommunityTool>> {
    let guard = cache_lock().lock().unwrap();
    match guard.as_ref() {
        Some(e) if e.time.elapsed() < CACHE_DURATION => Some(e.tools.clone()),
        _ => None,
    }
}

fn write_cache(tools: Vec<CommunityTool>) {
    let mut guard = cache_lock().lock().unwrap();
    *guard = Some(CacheEntry {
        time: Instant::now(),
        tools,
    });
}

#[tauri::command]
pub fn invalidate_community_cache() {
    let mut guard = cache_lock().lock().unwrap();
    *guard = None;
}

/// 磁盘缓存路径：应用数据目录下的 community_tools_cache.json
fn disk_cache_path(app: &tauri::AppHandle) -> PathBuf {
    app.path().app_data_dir().unwrap_or_default().join("community_tools_cache.json")
}

/// 将上次成功拉取的工具列表持久化到磁盘，供限流/离线时优雅降级
fn write_disk_cache(app: &tauri::AppHandle, tools: &[CommunityTool]) {
    if let Ok(s) = serde_json::to_string(tools) {
        let _ = std::fs::write(disk_cache_path(app), s);
    }
}

/// 读取磁盘缓存；仅当内容非空才视为有效
fn read_disk_cache(app: &tauri::AppHandle) -> Option<Vec<CommunityTool>> {
    let path = disk_cache_path(app);
    let content = std::fs::read_to_string(path).ok()?;
    let tools: Vec<CommunityTool> = serde_json::from_str(&content).ok()?;
    if tools.is_empty() {
        return None;
    }
    Some(tools)
}

// ============================== 工具函数 ==============================

fn tools_root() -> Option<PathBuf> {
    download_tools_dir()
}

/// 检查「下载位置」根目录是否存在该工具的压缩包（文件名 {分类}-{id}.{ext}）
fn tool_archive_path(category: &str, id: &str) -> Option<PathBuf> {
    let prefix = format!("{}-{}", safe_segment(category), safe_segment(id));
    tools_root().and_then(|root| {
        std::fs::read_dir(&root)
            .ok()
            .and_then(|rd| {
                rd.flatten().find(|e| {
                    e.file_type().map(|t| t.is_file()).unwrap_or(false)
                        && e.file_name()
                            .to_string_lossy()
                            .to_lowercase()
                            .starts_with(&format!("{prefix}."))
                })
            })
            .map(|e| e.path())
    })
}

/// 对仓库相对路径的每一段做 URL 编码（支持中文目录名/文件名）
fn encode_path_segments(path: &str) -> String {
    path.split('/')
        .map(|s| urlencoding::encode(s).into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// 拼接 API URL：仅当存在 token 时才附加 access_token 查询参数（公共仓库读取无需认证）
fn api_url(path: &str, token: &str, query: &[(&str, &str)]) -> String {
    let mut url = format!("{API_BASE}/{path}");
    let mut pairs: Vec<String> = Vec::new();
    if !token.is_empty() {
        pairs.push(format!("access_token={}", urlencoding::encode(token)));
    }
    for (k, v) in query {
        pairs.push(format!("{}={}", k, urlencoding::encode(v)));
    }
    if !pairs.is_empty() {
        url.push('?');
        url.push_str(&pairs.join("&"));
    }
    url
}

fn authed_client(_token: &str) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .user_agent("NexBox-Community")
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
    // 调用方使用 get().header("Authorization", ...) 附加 Bearer，见 api_get 等
}

/// 429 限流重试上限
const RATE_LIMIT_MAX_RETRY: u32 = 3;

/// 统一发送 GitCode 请求：遇 429 按 Retry-After / X-RateLimit-Reset 等待后重试
async fn send_with_retry(
    token: &str,
    build: impl Fn(&reqwest::Client) -> reqwest::RequestBuilder,
) -> Result<reqwest::Response, String> {
    for attempt in 0..RATE_LIMIT_MAX_RETRY {
        let client = authed_client(token);
        let mut req = build(&client);
        if !token.is_empty() {
            req = req.header("Authorization", format!("Bearer {token}"));
        }
        let resp = req.send().await.map_err(|e| format!("请求 GitCode 失败: {e}"))?;
        if resp.status().as_u16() == 429 {
            let wait = resp_retry_seconds(&resp);
            if attempt == RATE_LIMIT_MAX_RETRY - 1 {
                return Err("GitCode 接口限流(429)，请稍后再试".to_string());
            }
            // 加入随机抖动（±20%），避免并发线程同时醒来集中重试再次触发 429
            use rand::Rng;
            let wait = ((wait as f32) * (0.8 + rand::thread_rng().gen::<f32>() * 0.4))
                .round()
                .max(1.0) as u64;
            tokio::time::sleep(Duration::from_secs(wait)).await;
            continue;
        }
        return Ok(resp);
    }
    Err("请求失败".to_string())
}

/// 从响应头读取限流等待秒数（Retry-After 优先，其次 X-RateLimit-Reset 剩余秒数）
fn resp_retry_seconds(resp: &reqwest::Response) -> u64 {
    if let Some(v) = resp.headers().get("Retry-After").and_then(|v| v.to_str().ok()) {
        if let Ok(s) = v.parse::<u64>() {
            return s.clamp(1, 60);
        }
    }
    if let Some(v) = resp.headers().get("X-RateLimit-Reset").and_then(|v| v.to_str().ok()) {
        if let Ok(reset) = v.parse::<i64>() {
            let now = chrono::Utc::now().timestamp();
            let remain = reset - now;
            if remain > 0 {
                return (remain as u64).min(60);
            }
        }
    }
    15
}

async fn api_get(token: &str, path: &str, query: &[(&str, &str)]) -> Result<serde_json::Value, String> {
    let url = api_url(path, token, query);
    let resp = send_with_retry(token, |client| client.get(&url)).await?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if status.is_success() {
        serde_json::from_str(&text).map_err(|_| text)
    } else {
        if text.len() > 300 {
            Err(format!("GitCode HTTP {} : {}...", status.as_u16(), &text[..300]))
        } else {
            Err(format!("GitCode HTTP {} : {}", status.as_u16(), text))
        }
    }
}

async fn api_post(
    token: &str,
    path: &str,
    query: &[(&str, &str)],
    body: &[(String, String)],
) -> Result<serde_json::Value, String> {
    let url = api_url(path, token, query);
    let form: Vec<(String, String)> = body.to_vec();
    let resp = send_with_retry(token, |client| client.post(&url).form(&form)).await?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if status.is_success() || status.as_u16() == 201 {
        serde_json::from_str(&text).map_err(|_| text)
    } else {
        if text.len() > 300 {
            Err(format!("GitCode HTTP {} : {}...", status.as_u16(), &text[..300]))
        } else {
            Err(format!("GitCode HTTP {} : {}", status.as_u16(), text))
        }
    }
}

// ============================== 配置持久化（settings.json，与 hotkey.rs 一致） ==============================

fn save_settings_value(app: &tauri::AppHandle, key: &str, value: serde_json::Value) {
    let Ok(dir) = app.path().app_data_dir() else { return };
    let path = dir.join("settings.json");
    let mut json: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if let Some(obj) = json.as_object_mut() {
        obj.insert(key.to_string(), value.clone());
    }
    if let Ok(content) = serde_json::to_string_pretty(&json) {
        let _ = std::fs::write(&path, content);
    }
    // `settings.json` 同时被前端 @tauri-apps/plugin-store 以「整份内存缓存」的方式读写：
    // 任意一次 store.set()/store.save()（含 100ms 自动保存）都会把该实例的整份缓存写回磁盘。
    // 如果这里只直接写文件、不同步插件内存缓存，下一次前端保存其它设置时就会用旧缓存把
    // 这个键抹掉（社区工具下载位置就曾因此表现为“改完重启又回到默认下载目录”）。
    // 同步更新插件内存中的同一个 store 实例，保证后续整文件写回不会丢失该键。
    if let Ok(settings) = app.store("settings.json") {
        settings.set(key, value);
    }
}

fn read_settings_value(app: &tauri::AppHandle, key: &str) -> Option<serde_json::Value> {
    let dir = app.path().app_data_dir().ok()?;
    let path = dir.join("settings.json");
    let content = std::fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json.get(key).cloned()
}

fn save_token(app: &tauri::AppHandle, access: &str, refresh: &str) {
    save_settings_value(app, "gitcode_access_token", serde_json::json!(access));
    save_settings_value(app, "gitcode_refresh_token", serde_json::json!(refresh));
}

fn read_token(app: &tauri::AppHandle) -> Option<String> {
    match read_settings_value(app, "gitcode_access_token") {
        Some(serde_json::Value::String(s)) if !s.trim().is_empty() => Some(s),
        _ => None,
    }
}

fn clear_token(app: &tauri::AppHandle) {
    save_settings_value(app, "gitcode_access_token", serde_json::Value::Null);
    save_settings_value(app, "gitcode_refresh_token", serde_json::Value::Null);
}

// ============================== 读取社区工具列表 ==============================

/// contents 目录条目
#[derive(Debug, Deserialize)]
struct ContentEntry {
    #[serde(rename = "type")]
    kind: String,
    name: String,
    path: String,
    sha: Option<String>,
}

fn parse_content_entries(v: &serde_json::Value) -> Vec<ContentEntry> {
    let mut out = Vec::new();
    if let Some(arr) = v.as_array() {
        for item in arr {
            let kind = item.get("type").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let name = item.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let path = item.get("path").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let sha = item.get("sha").and_then(|x| x.as_str()).map(str::to_string);
            out.push(ContentEntry { kind, name, path, sha });
        }
    }
    out
}

/// 通过 contents 单文件接口获取 base64 content（带 429 限流重试）
async fn fetch_plugin_json(encoded_path: &str, token: &str) -> Option<String> {
    let v = api_get(
        token,
        &format!("repos/{OWNER}/{REPO}/contents/{encoded_path}"),
        &[],
    )
    .await
    .ok()?;
    let content = v.get("content")?.as_str()?;
    let cleaned: String = content.chars().filter(|c| !c.is_whitespace()).collect();
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD.decode(&cleaned).ok()?;
    Some(String::from_utf8(bytes).ok()?)
}

/// 读取单个工具目录：列目录拿 sha 映射（图标 blob 直链用）+ 下载 plugin.json → 解析
async fn fetch_one_tool(
    token: &str,
    cat_name: &str,
    tool_path: &str,
    tool_name: &str,
    sem: &Arc<Semaphore>,
) -> Option<CommunityTool> {
    let _permit = sem.acquire().await.ok()?;
    let tool_seg = encode_path_segments(tool_path);
    // 列工具目录拿 full path -> sha 映射（含图标），并确认存在 plugin.json
    let tool_list = api_get(token, &format!("repos/{OWNER}/{REPO}/contents/{tool_seg}"), &[]).await.ok()?;
    let entries = parse_content_entries(&tool_list);
    if !entries.iter().any(|e| e.name.eq_ignore_ascii_case("plugin.json")) {
        return None;
    }
    let sha_map: HashMap<String, String> = entries
        .iter()
        .filter_map(|e| e.sha.clone().map(|s| (e.path.clone(), s)))
        .collect();
    let plugin_path = format!("{}/plugin.json", tool_seg);
    let json = fetch_plugin_json(&plugin_path, token).await?;
    // cat.name 是纯分类目录名（不含 plugins/ 前缀），避免 repo_path 重复
    parse_plugin_json(&json, cat_name, tool_name, &sha_map)
}

/// 通过 raw blob 直链拉取文件文本（raw 域不占 GitCode API 限流配额）
async fn fetch_raw_blob(sha: &str, file_name: &str) -> Option<String> {
    let url = format!(
        "https://raw.gitcode.com/{OWNER}/{REPO}/blobs/{}/{}",
        sha,
        urlencoding::encode(file_name)
    );
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("NexBox-Community")
        .build()
        .ok()?;
    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.text().await.ok()
}

/// 拉取整棵 plugins 目录树（带分页），返回 (path, type, sha)。
/// GitCode 树接口默认每页 20，此前「截断约 20 条」正是因此；设 per_page=100 并翻页拿全量。
async fn fetch_tree_entries(token: &str) -> Result<Vec<(String, String, String)>, String> {
    let mut entries: Vec<(String, String, String)> = Vec::new();
    let prefix = format!("{PLUGINS_PATH}/");
    for page in 1..=10u32 {
        let v = api_get(
            token,
            &format!("repos/{OWNER}/{REPO}/git/trees/main"),
            &[("recursive", "1"), ("per_page", "100"), ("page", &page.to_string())],
        )
        .await?;
        let tree = v.get("tree").and_then(|x| x.as_array()).cloned().unwrap_or_default();
        let cur: Vec<(String, String, String)> = tree
            .into_iter()
            .filter_map(|item| {
                let p = item.get("path").and_then(|x| x.as_str())?;
                if !p.starts_with(&prefix) {
                    return None;
                }
                let t = item.get("type").and_then(|x| x.as_str()).unwrap_or("");
                let s = item.get("sha").and_then(|x| x.as_str()).unwrap_or("");
                Some((p.to_string(), t.to_string(), s.to_string()))
            })
            .collect();
        let n = cur.len();
        entries.extend(cur);
        // 未取满一页说明没有更多内容，停止翻页
        if n < 100 {
            break;
        }
    }
    Ok(entries)
}

/// 树驱动构建：从一次目录树请求枚举全部工具，再经 raw blob 直链读 plugin.json。
/// 相比旧版「contents 逐层遍历」（1 根 + N 分类 + 2×M 工具），API 调用数骤降至 1 次，彻底规避 429 限流。
async fn build_tools_from_tree(
    token: &str,
    sem: &Arc<Semaphore>,
) -> Result<Vec<CommunityTool>, String> {
    let entries = fetch_tree_entries(token).await?;
    let prefix = format!("{PLUGINS_PATH}/");

    struct ToolCtx {
        plugin_sha: String,
        icon_sha_map: HashMap<String, String>,
    }
    let mut tool_map: std::collections::BTreeMap<String, (String, String, ToolCtx)> =
        std::collections::BTreeMap::new();

    for (path, kind, sha) in &entries {
        let rel = path.strip_prefix(&prefix).unwrap_or("");
        let mut segs = rel.split('/');
        let (cat, tool) = match (segs.next(), segs.next()) {
            (Some(c), Some(t)) if !c.is_empty() && !t.is_empty() => (c.to_string(), t.to_string()),
            _ => continue, // 根、分类目录
        };
        let key = format!("{cat}/{tool}");
        let ctx_entry = tool_map
            .entry(key)
            .or_insert_with(|| (cat.clone(), tool.clone(), ToolCtx { plugin_sha: String::new(), icon_sha_map: HashMap::new() }));
        let ctx = &mut ctx_entry.2;
        if *kind == "blob" {
            if rel.ends_with("/plugin.json") {
                ctx.plugin_sha = sha.clone();
            } else if !sha.is_empty() {
                ctx.icon_sha_map.insert(path.clone(), sha.clone());
            }
        }
    }

    let plan: Vec<(String, String, String, HashMap<String, String>)> = tool_map
        .into_iter()
        .filter(|(_, (_, _, c))| !c.plugin_sha.is_empty())
        .map(|(_, (cat, tool, c))| (cat, tool, c.plugin_sha, c.icon_sha_map))
        .collect();

    if plan.is_empty() {
        return Err("树请求未解析到任何工具".to_string());
    }

    let results = join_all(plan.into_iter().map(|(cat, tool, plugin_sha, sha_map)| {
        let sem = sem.clone();
        async move {
            let _permit = sem.acquire().await.ok()?;
            let json = fetch_raw_blob(&plugin_sha, "plugin.json").await?;
            parse_plugin_json(&json, &cat, &tool, &sha_map)
        }
    }))
    .await;

    Ok(results.into_iter().flatten().collect())
}

/// 加载社区工具：优先「单次树请求 + raw 直链读 plugin.json」，失败时回退到 contents 逐层遍历。
async fn load_tools_inner() -> Result<Vec<CommunityTool>, String> {
    let token = TOOL_API_PAT;
    let sem = Arc::new(Semaphore::new(MAX_CONCURRENT));

    if let Ok(tools) = build_tools_from_tree(token, &sem).await {
        if !tools.is_empty() {
            return Ok(tools);
        }
    }

    // 回退：contents 逐层遍历（保留旧路径作为安全网）
    // 根目录：plugins 下的分类
    let root_seg = encode_path_segments(PLUGINS_PATH);
    let root = api_get(token, &format!("repos/{OWNER}/{REPO}/contents/{root_seg}"), &[])
        .await
        .map_err(|e| format!("无法读取社区插件目录：{e}"))?;
    let cats: Vec<ContentEntry> = parse_content_entries(&root)
        .into_iter()
        .filter(|c| c.kind == "dir")
        .collect();

    // 各分类目录并发展开为工具目录
    let cat_results = join_all(cats.iter().map(|cat| {
        let sem = sem.clone();
        let token = token.to_string();
        let cat_name = cat.name.clone();
        async move {
            let _permit = sem.acquire().await.ok()?;
            let cat_seg = encode_path_segments(&cat.path);
            let cat_list = api_get(&token, &format!("repos/{OWNER}/{REPO}/contents/{cat_seg}"), &[]).await.ok()?;
            let tool_dirs: Vec<ContentEntry> = parse_content_entries(&cat_list)
                .into_iter()
                .filter(|e| e.kind == "dir")
                .collect();
            Some((cat_name, tool_dirs))
        }
    }))
    .await;

    // 对所有 (分类, 工具) 并发拉详情
    let plan: Vec<(String, ContentEntry)> = cat_results
        .into_iter()
        .flatten()
        .flat_map(|(cat, dirs)| dirs.into_iter().map(move |d| (cat.clone(), d)))
        .collect();
    let results = join_all(plan.iter().map(|(cat_name, te)| {
        let sem = sem.clone();
        let token = token.to_string();
        let cat_name = cat_name.clone();
        let te_path = te.path.clone();
        let te_name = te.name.clone();
        async move { fetch_one_tool(&token, &cat_name, &te_path, &te_name, &sem).await }
    }))
    .await;

    Ok(results.into_iter().flatten().collect())
}

fn parse_plugin_json(
    json: &str,
    category: &str,
    fallback_name: &str,
    sha_map: &HashMap<String, String>,
) -> Option<CommunityTool> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let id = v.get("id").and_then(|x| x.as_str()).map(str::to_string).unwrap_or_else(|| fallback_name.to_string());
    let name = v.get("name").and_then(|x| x.as_str()).map(str::to_string).unwrap_or_else(|| fallback_name.to_string());
    if id.trim().is_empty() || name.trim().is_empty() {
        return None;
    }
    let cat = v.get("category").and_then(|x| x.as_str()).map(str::to_string).unwrap_or_else(|| category.to_string());
    let tags: Vec<String> = v
        .get("tags")
        .and_then(|x| x.as_array())
        .map(|arr| arr.iter().filter_map(|t| t.as_str().map(str::to_string)).collect())
        .unwrap_or_default();

    let file = v.get("file").and_then(|x| x.as_str()).map(str::to_string);
    let icon = v.get("icon").and_then(|x| x.as_str()).map(str::to_string);

    let repo_path = format!("{PLUGINS_PATH}/{category}/{}", fallback_name);
    // 图标优先用 GitCode blob 直链（需文件 sha），无 sha 时回退到 raw 分支链接
    let icon_url = icon.as_ref().map(|ic| {
        let full = format!("{repo_path}/{ic}");
        match sha_map.get(&full) {
            Some(sha) => format!(
                "https://raw.gitcode.com/{OWNER}/{REPO}/blobs/{}/{}",
                sha,
                urlencoding::encode(ic)
            ),
            None => format!("{RAW_BASE}/{}", encode_path_segments(&full)),
        }
    });

    let install_status = if tool_archive_path(&cat, &id).is_some() {
        "installed".to_string()
    } else {
        "not_installed".to_string()
    };

    Some(CommunityTool {
        id,
        name,
        version: v.get("version").and_then(|x| x.as_str()).map(str::to_string),
        description: v.get("description").and_then(|x| x.as_str()).map(str::to_string),
        category: if cat.trim().is_empty() { category.to_string() } else { cat },
        tags,
        author: v.get("author").and_then(|x| x.as_str()).map(str::to_string),
        publisher: v.get("publisher").and_then(|x| x.as_str()).map(str::to_string),
        homepage: v.get("homepage").and_then(|x| x.as_str()).map(str::to_string),
        submitted_at: v.get("submittedAt").and_then(|x| x.as_str()).map(str::to_string),
        launch_target: v.get("launchTarget").and_then(|x| x.as_str()).map(str::to_string),
        file,
        icon,
        download_url: v.get("downloadUrl").and_then(|x| x.as_str()).map(str::to_string),
        download_filter: v.get("downloadFilter").and_then(|x| x.as_str()).map(str::to_string),
        repo_path,
        install_status,
        icon_url,
    })
}

#[tauri::command]
pub async fn get_community_tools(app: tauri::AppHandle) -> Result<Vec<CommunityTool>, String> {
    if let Some(cached) = read_cache() {
        return Ok(cached);
    }
    match load_tools_inner().await {
        Ok(tools) => {
            write_cache(tools.clone());
            write_disk_cache(&app, &tools);
            Ok(tools)
        }
        Err(e) => {
            // API 失败（尤其 429）时回退磁盘缓存，避免直接报空列表/红字错误
            if let Some(cached) = read_disk_cache(&app) {
                return Ok(cached);
            }
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn get_community_categories(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let tools = get_community_tools(app).await?;
    let mut cats: Vec<String> = tools
        .iter()
        .map(|t| t.category.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    cats.sort();
    Ok(cats)
}

#[tauri::command]
pub async fn search_community_tools(query: String, app: tauri::AppHandle) -> Result<Vec<CommunityTool>, String> {
    let tools = get_community_tools(app).await?;
    let q = query.trim().to_lowercase();
    Ok(tools
        .into_iter()
        .filter(|t| {
            t.name.to_lowercase().contains(&q)
                || t.description.as_deref().unwrap_or("").to_lowercase().contains(&q)
                || t.tags.iter().any(|tag| tag.to_lowercase().contains(&q))
                || t.category.to_lowercase().contains(&q)
        })
        .collect())
}

#[tauri::command]
pub fn get_community_install_status(category: String, id: String) -> bool {
    tool_archive_path(&category, &id).is_some()
}

// ============================== GitCode OAuth 授权码登录（localhost 回调） ==============================

struct LoginShared {
    app: tauri::AppHandle,
    expected_state: String,
    shutdown: StdMutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

async fn callback_handler(
    shared: Arc<LoginShared>,
    params: HashMap<String, String>,
) -> impl axum::response::IntoResponse {
    let mut html = String::from("<h3>GitCode 登录</h3>");
    let state = params.get("state").cloned().unwrap_or_default();
    let code = params.get("code").cloned().unwrap_or_default();
    if state != shared.expected_state {
        html.push_str("<p style='color:red'>state 不匹配，请重试。</p>");
    } else if code.is_empty() {
        html.push_str("<p style='color:red'>未收到授权码，可能已取消授权。</p>");
    } else {
        // 交换 token
        let url = format!(
            "{OAUTH_TOKEN}?grant_type=authorization_code&code={}&client_id={}&client_secret={}",
            urlencoding::encode(&code),
            urlencoding::encode(CLIENT_ID),
            urlencoding::encode(CLIENT_SECRET)
        );
        match reqwest::Client::new().post(&url).send().await {
            Ok(resp) => match resp.error_for_status() {
                Ok(resp) => match resp.json::<serde_json::Value>().await {
                    Ok(value) => {
                        let access = value.get("access_token").and_then(|x| x.as_str()).unwrap_or("");
                        let refresh = value.get("refresh_token").and_then(|x| x.as_str()).unwrap_or("");
                        if !access.is_empty() {
                            save_token(&shared.app, access, refresh);
                            html.push_str("<p style='color:green'>登录成功！可以关闭此页面并回到应用。</p>");
                        } else {
                            html.push_str("<p style='color:red'>未获取到 access_token。</p>");
                        }
                    }
                    Err(_) => html.push_str("<p style='color:red'>令牌交换响应解析失败，请重试。</p>"),
                },
                Err(_) => html.push_str("<p style='color:red'>令牌交换失败，请重试。</p>"),
            },
            Err(_) => html.push_str("<p style='color:red'>令牌交换请求失败，请重试。</p>"),
        }
    }
    // 触发服务关闭
    if let Some(tx) = shared.shutdown.lock().unwrap().take() {
        let _ = tx.send(());
    }
    (axum::http::StatusCode::OK, axum::response::Html(html))
}

/// 启动授权码回调服务，返回需要让前端在浏览器打开的授权 URL
async fn start_oauth_server(app: tauri::AppHandle, state_val: String) -> Result<String, String> {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], REDIRECT_PORT));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("无法启动本地回调端口 {}：{e}", REDIRECT_PORT))?;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let shared = Arc::new(LoginShared {
        app,
        expected_state: state_val.clone(),
        shutdown: StdMutex::new(Some(shutdown_tx)),
    });

    let shared_route = shared.clone();
    let router = axum::Router::new().route(
        "/callback",
        axum::routing::get(move |params: axum::extract::Query<HashMap<String, String>>| {
            callback_handler(shared_route.clone(), params.0)
        }),
    );

    tauri::async_runtime::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    let redirect_uri = format!("http://127.0.0.1:{REDIRECT_PORT}/callback");
    let authorize = format!(
        "{OAUTH_AUTHORIZE}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}",
        urlencoding::encode(CLIENT_ID),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(OAUTH_SCOPE),
        urlencoding::encode(&state_val),
    );
    Ok(authorize)
}

#[tauri::command]
pub async fn gitcode_login_start(app: tauri::AppHandle) -> Result<String, String> {
    let state_val = uuid::Uuid::new_v4().simple().to_string();
    let authorize = start_oauth_server(app, state_val).await?;
    Ok(authorize)
}

#[tauri::command]
pub async fn gitcode_logout(app: tauri::AppHandle) {
    clear_token(&app);
    avatar_cache_lock().lock().unwrap().take();
}

/// 从 /user 响应提取头像 URL（GitCode 账号类型不同，头像在 avatar_url 或 photo 字段）
fn extract_avatar(v: &serde_json::Value) -> Option<String> {
    v.get("avatar_url")
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| v.get("photo").and_then(|x| x.as_str()).filter(|s| !s.is_empty()))
        .map(str::to_string)
}

/// 头像 data URI 缓存（1 小时）
static AVATAR_CACHE: OnceLock<StdMutex<Option<(Instant, String)>>> = OnceLock::new();
const AVATAR_CACHE_DURATION: Duration = Duration::from_secs(3600);

fn avatar_cache_lock() -> &'static StdMutex<Option<(Instant, String)>> {
    AVATAR_CACHE.get_or_init(|| StdMutex::new(None))
}

/// 后端抓取头像并转 base64 data URI（避免 WebView 加载 GitCode CDN 图片失败）
#[tauri::command]
pub async fn get_gitcode_avatar_data(app: tauri::AppHandle) -> Option<String> {
    let token = read_token(&app)?;
    let v = api_get(&token, "user", &[]).await.ok()?;
    let url = extract_avatar(&v)?;

    if let Some((t, data)) = avatar_cache_lock().lock().unwrap().as_ref() {
        if t.elapsed() < AVATAR_CACHE_DURATION {
            return Some(data.clone());
        }
    }

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) NexBox")
        .timeout(Duration::from_secs(15))
        .build()
        .ok()?;
    let bytes = client.get(&url).send().await.ok()?.bytes().await.ok()?;
    use base64::Engine as _;
    let uri = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    );
    *avatar_cache_lock().lock().unwrap() = Some((Instant::now(), uri.clone()));
    Some(uri)
}

#[tauri::command]
pub async fn get_gitcode_login_status(app: tauri::AppHandle) -> GitCodeLoginStatus {
    let Some(token) = read_token(&app) else {
        return GitCodeLoginStatus { logged_in: false, user: None };
    };
    match api_get(&token, "user", &[]).await {
        Ok(v) => {
            let login = v.get("login").and_then(|x| x.as_str()).unwrap_or("").to_string();
            if login.is_empty() {
                clear_token(&app);
                return GitCodeLoginStatus { logged_in: false, user: None };
            }
            GitCodeLoginStatus {
                logged_in: true,
                user: Some(GitCodeUser {
                    login,
                    avatar_url: extract_avatar(&v),
                    name: v.get("name").and_then(|x| x.as_str()).map(str::to_string),
                }),
            }
        }
        Err(_) => GitCodeLoginStatus { logged_in: false, user: None },
    }
}

// ============================== 编辑辅助：fork / 分支 / 上传 / PR ==============================

async fn ensure_token(app: &tauri::AppHandle) -> Result<String, String> {
    let Some(token) = read_token(app) else {
        return Err("请先在社区工具页登录 GitCode".to_string());
    };
    Ok(token)
}

async fn current_login(token: &str) -> Result<String, String> {
    let v = api_get(token, "user", &[]).await.map_err(|e| format!("获取用户信息失败：{e}"))?;
    v.get("login")
        .and_then(|x| x.as_str())
        .map(str::to_string)
        .ok_or_else(|| "无法解析 GitCode 用户名".to_string())
}

/// 确保用户已 fork 上游仓库，返回 fork 的 owner（即用户 login）
async fn ensure_fork(token: &str) -> Result<String, String> {
    let user = current_login(token).await?;
    // 检查 fork 是否已存在
    match api_get(token, &format!("repos/{user}/{REPO}"), &[]).await {
        Ok(_) => Ok(user.clone()),
        Err(_) => {
            // 创建 fork
            api_post(token, &format!("repos/{OWNER}/{REPO}/forks"), &[], &[]).await
                .map_err(|e| format!("Fork 社区仓库失败：{e}"))?;
            // 等待 fork 就绪
            tokio::time::sleep(Duration::from_secs(5)).await;
            Ok(user)
        }
    }
}

/// 删除用户自己的 fork 仓库（忽略失败——仓库可能不存在）
async fn delete_repo_quiet(token: &str, owner: &str) {
    if token.is_empty() || owner.eq_ignore_ascii_case(OWNER) {
        return;
    }
    let url = api_url(&format!("repos/{owner}/{REPO}"), token, &[]);
    let _ = authed_client(token)
        .delete(&url)
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await;
}

/// 重建 fork：删除旧的过时 fork 再重新 fork。
/// GitCode 不支持 REST 同步 fork（web 按钮 / git 命令才可），而第三方 fork 不会自动获得上游已合并内容，
/// 删除前重建 fork 即可让新的 main = 上游最新，保证要删除的目录存在。
///
/// 会话内对同一账号只重建一次（cached），避免反复 DELETE+POST forks 触发 GitCode 限流(429)。
async fn refresh_fork(token: &str, fork_owner: &str) -> Result<(), String> {
    if fork_owner.eq_ignore_ascii_case(OWNER) {
        return Ok(()); // 自己就是上游，无需重建
    }
    if fork_already_refreshed(fork_owner) {
        return Ok(()); // 本会话已重建过，直接复用
    }
    delete_repo_quiet(token, fork_owner).await;
    api_post(token, &format!("repos/{OWNER}/{REPO}/forks"), &[], &[]).await
        .map_err(|e| format!("重新 Fork 社区仓库失败：{e}"))?;
    tokio::time::sleep(Duration::from_secs(5)).await;
    mark_fork_refreshed(fork_owner);
    Ok(())
}

/// 本会话已重建过 fork 的账号集合（key = fork_owner login）
static FORK_REFRESHED: OnceLock<StdMutex<HashSet<String>>> = OnceLock::new();

fn fork_already_refreshed(fork_owner: &str) -> bool {
    FORK_REFRESHED
        .get_or_init(|| StdMutex::new(HashSet::new()))
        .lock()
        .unwrap()
        .contains(fork_owner)
}

fn mark_fork_refreshed(fork_owner: &str) {
    FORK_REFRESHED
        .get_or_init(|| StdMutex::new(HashSet::new()))
        .lock()
        .unwrap()
        .insert(fork_owner.to_string());
}

/// 创建分支（refs 为起点分支名，branch_name 为新分支）——GitCode 用 form-urlencoded 请求体
async fn create_branch(token: &str, owner: &str, refs: &str, branch_name: &str) -> Result<(), String> {
    let body = vec![
        ("refs".to_string(), refs.to_string()),
        ("branch_name".to_string(), branch_name.to_string()),
    ];
    api_post(token, &format!("repos/{owner}/{REPO}/branches"), &[], &body).await
        .map_err(|e| format!("创建分支失败：{e}"))?;
    Ok(())
}

/// 最佳努力删除分支（用于失败时清理残留临时分支，失败静默忽略）
async fn delete_branch_quiet(token: &str, owner: &str, branch: &str) {
    if token.is_empty() {
        return;
    }
    let url = api_url(&format!("repos/{owner}/{REPO}/branches/{branch}"), token, &[]);
    let _ = authed_client(token)
        .delete(&url)
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await;
}

/// 上传文件到指定分支（content 为 base64）
async fn upload_file(
    token: &str,
    owner: &str,
    repo_path: &str,
    branch: &str,
    content_b64: &str,
    message: &str,
) -> Result<(), String> {
    let encoded = encode_path_segments(repo_path);
    let body = vec![
        ("content".to_string(), content_b64.to_string()),
        ("message".to_string(), message.to_string()),
        ("branch".to_string(), branch.to_string()),
    ];
    api_post(token, &format!("repos/{owner}/{REPO}/contents/{encoded}"), &[], &body).await
        .map_err(|e| format!("上传 {repo_path} 失败：{e}"))?;
    Ok(())
}

/// 创建 PR（上游仓库）。
/// 同仓库 PR：head 只传分支名、不带 fork_path；跨仓库（真正 fork）PR：head 用 owner:branch 且必填 fork_path。
async fn create_pr(
    token: &str,
    owner_head: &str,
    branch: &str,
    title: &str,
    body: &str,
) -> Result<String, String> {
    let same_repo = owner_head.eq_ignore_ascii_case(OWNER);
    let mut pr_body: Vec<(String, String)> = vec![
        ("title".to_string(), title.to_string()),
        ("base".to_string(), "main".to_string()),
        ("body".to_string(), body.to_string()),
    ];
    if same_repo {
        pr_body.push(("head".to_string(), branch.to_string()));
    } else {
        pr_body.push(("head".to_string(), format!("{owner_head}:{branch}")));
        pr_body.push(("fork_path".to_string(), format!("{owner_head}/{REPO}")));
    }
    let v = api_post(token, &format!("repos/{OWNER}/{REPO}/pulls"), &[], &pr_body).await
        .map_err(|e| format!("创建 PR 失败：{e}"))?;
    // 解析 PR 链接：html_url → web_url → 用 number 拼链接
    if let Some(u) = v
        .get("html_url")
        .and_then(|x| x.as_str())
        .or_else(|| v.get("web_url").and_then(|x| x.as_str()))
    {
        return Ok(u.to_string());
    }
    if let Some(n) = v.get("number").and_then(|x| x.as_i64()).or_else(|| {
        v.get("number")
            .and_then(|x| x.as_str())
            .and_then(|s| s.parse::<i64>().ok())
    }) {
        return Ok(format!("https://gitcode.com/{OWNER}/{REPO}/pulls/{n}"));
    }
    // 依旧拿不到链接时返回整个响应体，便于排查
    Err(format!("无法解析 PR 链接，原始响应：{}", v))
}

fn build_plugin_json(
    name: &str,
    description: &str,
    category: &str,
    tags: &str,
    launch_target: &str,
    publisher: &str,
    homepage: &str,
    version: &str,
    author: &str,
    file_name: Option<&str>,
    icon_name: Option<&str>,
    download_url: Option<&str>,
    download_filter: Option<&str>,
    tool_id: &str,
) -> String {
    let tag_list: Vec<String> = tags
        .split(|c| c == ',' || c == '，' || c == ';' || c == '；')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let mut map = serde_json::Map::new();
    map.insert("id".into(), tool_id.into());
    map.insert("name".into(), name.into());
    map.insert(
        "version".into(),
        serde_json::json!(if version.trim().is_empty() { "1.0" } else { version }),
    );
    map.insert("description".into(), serde_json::json!(description));
    map.insert("category".into(), serde_json::json!(category));
    map.insert("tags".into(), serde_json::json!(tag_list));
    if !publisher.trim().is_empty() {
        map.insert("publisher".into(), serde_json::json!(publisher));
    }
    map.insert("author".into(), serde_json::json!(author));
    map.insert("submittedAt".into(), serde_json::json!(chrono::Utc::now().to_rfc3339()));
    if !homepage.trim().is_empty() {
        map.insert("homepage".into(), serde_json::json!(homepage));
    }
    if !launch_target.trim().is_empty() {
        map.insert("launchTarget".into(), serde_json::json!(launch_target));
    }
    if let Some(f) = file_name {
        map.insert("file".into(), serde_json::json!(f));
    }
    if let Some(ic) = icon_name {
        map.insert("icon".into(), serde_json::json!(ic));
    }
    if let Some(du) = download_url.filter(|s| !s.trim().is_empty()) {
        map.insert("downloadUrl".into(), serde_json::json!(du));
    }
    if let Some(df) = download_filter.filter(|s| !s.trim().is_empty()) {
        map.insert("downloadFilter".into(), serde_json::json!(df));
    }
    serde_json::to_string_pretty(&serde_json::Value::Object(map)).unwrap_or_default()
}

pub fn generate_tool_id(name: &str) -> String {
    use regex::Regex;
    let mut id = name.to_lowercase();
    let re = Regex::new(r"[^a-z0-9\u4e00-\u9fff\-]").unwrap();
    id = re.replace_all(&id, "-").to_string();
    let re2 = Regex::new(r"-+").unwrap();
    id = re2.replace_all(&id, "-").to_string();
    let id = id.trim_matches('-').to_string();
    let id = if id.len() > 50 { id[..50].to_string() } else { id };
    if id.trim().is_empty() {
        let s = format!("tool-{}", uuid::Uuid::new_v4().simple());
        s[..16].to_string()
    } else {
        id
    }
}

fn emit_progress(app: &tauri::AppHandle, msg: &str) {
    let _ = app.emit("community-submit-progress", serde_json::json!({ "message": msg }));
}

/// 安装进度事件：message 为文案，percent 为可选百分比（None 表示未知/解压阶段）
fn emit_install_progress(app: &tauri::AppHandle, message: &str, percent: Option<u32>) {
    let _ = app.emit(
        "community-install-progress",
        serde_json::json!({ "message": message, "percent": percent }),
    );
}

/// 读取图标，并按需压缩后编码为 PNG，返回（字节内容, 文件名）。
/// GitCode contents API 对 base64 载荷有约 1MB 限制，940K 原图 base64 后会超限导致 400；
/// 这里统一缩放到 256px 以内、重编码为 PNG，体积显著减小且不损失展示效果。
fn prepare_icon_bytes(path: &str) -> Result<(Vec<u8>, String), String> {
    let p = std::path::Path::new(path);
    let base_name = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("icon")
        .to_string();

    // 优先用 image 解码并压缩
    let decoded = image::ImageReader::open(p)
        .map_err(|_| ())
        .and_then(|r| r.decode().map_err(|_| ()));
    match decoded {
        Ok(img) => {
            let (w, h) = (img.width(), img.height());
            let max_dim = w.max(h).max(1);
            let scale = (256f32 / max_dim as f32).min(1.0);
            let nw = ((w as f32) * scale).round().max(1.0) as u32;
            let nh = ((h as f32) * scale).round().max(1.0) as u32;
            let resized = if scale < 1.0 {
                img.resize(nw, nh, image::imageops::FilterType::Lanczos3)
            } else {
                img
            };
            let mut buf = std::io::Cursor::new(Vec::new());
            resized
                .write_to(&mut buf, image::ImageFormat::Png)
                .map_err(|e| format!("编码图标失败：{e}"))?;
            Ok((buf.into_inner(), format!("{base_name}.png")))
        }
        Err(_) => {
            // 无法解码的图片格式：保留原始数据，但限制大小避免触发 contents 限制
            let bytes = std::fs::read(p).map_err(|e| format!("读取图标失败：{e}"))?;
            if (bytes.len() as u64) > MAX_CONTENTS_UPLOAD {
                return Err(format!("图标超过限制，请使用较小的图片"));
            }
            let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("png").to_string();
            Ok((bytes, format!("{base_name}.{ext}")))
        }
    }
}

// ============================== 提交 / 删除 PR ==============================

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn submit_community_tool(
    app: tauri::AppHandle,
    name: String,
    description: String,
    category: String,
    tags: String,
    zip_path: Option<String>,
    launch_target: Option<String>,
    publisher: Option<String>,
    homepage: Option<String>,
    version: Option<String>,
    icon_path: Option<String>,
    download_url: Option<String>,
    download_filter: Option<String>,
) -> Result<String, String> {
    let token = ensure_token(&app).await?;
    let author = current_login(&token).await?;

    if name.trim().is_empty() {
        return Err("工具名称不能为空".to_string());
    }
    let category_final = if category.trim().is_empty() { "综合工具".to_string() } else { category.trim().to_string() };
    let tool_id = generate_tool_id(&name);
    emit_progress(&app, "正在 Fork 社区仓库...");
    let fork_owner = ensure_fork(&token).await?;

    // 分支名固定 plugin/{tool_id}-<时间戳>，保证唯一，彻底避免"分支已存在"导致的创建失败(400 UN_KNOW)
    let branch = format!("plugin/{tool_id}-{}", chrono::Utc::now().timestamp());
    emit_progress(&app, "正在创建分支...");
    create_branch(&token, &fork_owner, "main", &branch).await?;

    let repo_sub = format!("{PLUGINS_PATH}/{category_final}/{tool_id}");

    // 上传与 PR 在一个 async 块内执行；任一环节失败则删除该临时分支，避免反复重试堆积分支
    let result: Result<String, String> = async {
        emit_progress(&app, "正在上传工具包...");
        let zipped_file_name = if let Some(zip) = zip_path.as_ref() {
            if !std::path::Path::new(zip).exists() {
                return Err("压缩包文件不存在".to_string());
            }
            let meta = std::fs::metadata(zip).map_err(|e| format!("读取压缩包失败：{e}"))?;
            if meta.len() > MAX_CONTENTS_UPLOAD {
                return Err(format!(
                    "压缩包超过 {}KB，请改用「提供下载链接」方式或减小体积后重试",
                    MAX_CONTENTS_UPLOAD / 1024
                ));
            }
            let bytes = std::fs::read(zip).map_err(|e| format!("读取压缩包失败：{e}"))?;
            use base64::Engine as _;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            let fname = std::path::Path::new(zip).file_name().and_then(|x| x.to_str()).unwrap_or("tool.zip").to_string();
            let repo_file = format!("{repo_sub}/{fname}");
            upload_file(&token, &fork_owner, &repo_file, &branch, &b64, &format!("feat: upload {fname}")).await?;
            Some(fname)
        } else {
            None
        };

        emit_progress(&app, "正在上传图标...");
        let icon_file_name = if let Some(icon) = icon_path.as_ref() {
            if std::path::Path::new(icon).exists() {
                let (bytes, fname) = prepare_icon_bytes(icon)?;
                use base64::Engine as _;
                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                let repo_file = format!("{repo_sub}/{fname}");
                upload_file(&token, &fork_owner, &repo_file, &branch, &b64, &format!("feat: upload {fname}")).await?;
                Some(fname)
            } else {
                None
            }
        } else {
            None
        };

        emit_progress(&app, "正在提交插件信息...");
        let plugin_json = build_plugin_json(
            name.trim(),
            description.trim(),
            &category_final,
            &tags,
            launch_target.as_deref().unwrap_or(""),
            publisher.as_deref().unwrap_or(""),
            homepage.as_deref().unwrap_or(""),
            version.as_deref().unwrap_or(""),
            &author,
            zipped_file_name.as_deref(),
            icon_file_name.as_deref(),
            download_url.as_deref(),
            download_filter.as_deref(),
            &tool_id,
        );
        use base64::Engine as _;
        let plugin_b64 = base64::engine::general_purpose::STANDARD.encode(plugin_json.as_bytes());
        upload_file(
            &token,
            &fork_owner,
            &format!("{repo_sub}/plugin.json"),
            &branch,
            &plugin_b64,
            &format!("feat: add plugin - {tool_id}"),
        )
        .await?;

        emit_progress(&app, "正在创建 Pull Request...");
        let pr_body = format!(
            "## 新增社区工具\n\n- **名称**：{name}\n- **分类**：{category_final}\n- **描述**：{description}\n- **提交者**：@{author}\n"
        );
        let pr_url = create_pr(&token, &fork_owner, &branch, &format!("[社区工具] {name}"), &pr_body).await?;
        Ok(pr_url)
    }
    .await;

    if result.is_err() {
        // 失败清理临时分支，避免下次重试残留
        delete_branch_quiet(&token, &fork_owner, &branch).await;
    }

    let pr_url = result?;
    emit_progress(&app, "提交成功！");
    invalidate_community_cache();
    Ok(pr_url)
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn delete_community_tool(
    app: tauri::AppHandle,
    id: String,
    name: String,
    category: String,
    author: Option<String>,
    repo_path: Option<String>,
) -> Result<String, String> {
    let token = ensure_token(&app).await?;
    let login = current_login(&token).await?;
    let has_author = author.as_deref().map(|a| a.to_lowercase() == login.to_lowercase()).unwrap_or(false);
    if !has_author {
        return Err("只能删除自己提交的工具".to_string());
    }

    emit_progress(&app, "正在 Fork 社区仓库...");
    let fork_owner = ensure_fork(&token).await?;
    let branch = format!("delete/{id}-{}", chrono::Utc::now().timestamp());

    // 先尝试删除；若目录不存在（第三方 fork 过时 404），则重建 fork 一次后重试
    let mut attempt = 0;
    let result: Result<String, String> = loop {
        let once: Result<String, String> = async {
            // 重试循环会多次建 async 块，内部用 clone 避免把外层 String 移走
            let owner_h = fork_owner.clone();
            let branch_h = branch.clone();
            let login_h = login.clone();
            let name_h = name.clone();
            let category_h = category.clone();
            let repo_sub = repo_path
                .clone()
                .filter(|p| !p.trim().is_empty())
                .unwrap_or_else(|| format!("{PLUGINS_PATH}/{category_h}/{id}"));

            emit_progress(&app, "正在创建分支...");
            create_branch(&token, &owner_h, "main", &branch_h).await?;

            emit_progress(&app, "正在删除文件...");
            let files = list_dir_recursive(&token, &owner_h, &repo_sub, &branch_h).await?;
            if files.is_empty() {
                return Err("没有找到待删除的文件".to_string());
            }
            for f in files {
                delete_file(&token, &owner_h, &f, &branch_h, &format!("chore: remove {f}")).await?;
            }

            emit_progress(&app, "正在创建删除 Pull Request...");
            let pr_body = format!(
                "## 删除社区工具\n\n- **名称**：{name_h}\n- **分类**：{category_h}\n- **请求者**：@{login_h}\n\n工具提交者请求删除此工具。"
            );
            let pr_url = create_pr(&token, &owner_h, &branch_h, &format!("[删除工具] {name_h}"), &pr_body).await?;
            Ok(pr_url)
        }
        .await;

        match once {
            Ok(v) => break Ok(v),
            Err(e) => {
                let need_fresh = (e.contains("不存在") || e.contains("404"))
                    && !fork_owner.eq_ignore_ascii_case(OWNER)
                    && attempt == 0;
                if need_fresh {
                    emit_progress(&app, "正在同步社区仓库最新内容...");
                    if let Err(fe) = refresh_fork(&token, &fork_owner).await {
                        break Err(fe);
                    }
                    attempt += 1;
                    continue;
                }
                break Err(e);
            }
        }
    };

    if result.is_err() {
        delete_branch_quiet(&token, &fork_owner, &branch).await;
    }

    let pr_url = result?;
    emit_progress(&app, "删除请求已提交！");
    invalidate_community_cache();
    Ok(pr_url)
}

async fn list_dir_recursive(
    token: &str,
    owner: &str,
    root: &str,
    branch: &str,
) -> Result<Vec<String>, String> {
    let mut result = Vec::new();
    let mut stack = vec![root.to_string()];
    while let Some(dir) = stack.pop() {
        let encoded = encode_path_segments(&dir);
        let v = match api_get(token, &format!("repos/{owner}/{REPO}/contents/{encoded}"), &[("ref", branch)]).await {
            Ok(v) => v,
            Err(e) if e.contains("404") => {
                return Err(format!(
                    "工具目录在 main 分支上不存在（{dir}）——该工具可能尚未合并，请先合并其提交 PR 后再删除。\n详情：{e}"
                ));
            }
            Err(e) => return Err(format!("读取目录失败：{e}")),
        };
        for item in parse_content_entries(&v) {
            if item.kind == "dir" {
                stack.push(item.path);
            } else {
                result.push(item.path);
            }
        }
    }
    Ok(result)
}

async fn delete_file(
    token: &str,
    owner: &str,
    repo_path: &str,
    branch: &str,
    message: &str,
) -> Result<(), String> {
    // 先取 sha
    let encoded = encode_path_segments(repo_path);
    let v = api_get(token, &format!("repos/{owner}/{REPO}/contents/{encoded}"), &[("ref", branch)]).await?;
    let sha = v.get("sha").and_then(|x| x.as_str()).unwrap_or("").to_string();
    if sha.is_empty() {
        return Err(format!("无法定位文件 {repo_path} 的 sha"));
    }
    let url = api_url(
        &format!("repos/{owner}/{REPO}/contents/{encoded}"),
        token,
        &[("sha", &sha), ("message", message), ("branch", branch)],
    );
    let client = authed_client(token);
    let resp = client
        .delete(&url)
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| format!("删除请求失败：{e}"))?;
    if !resp.status().is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("删除 {repo_path} 失败：{}", if text.len() > 200 { text[..200].to_string() } else { text }));
    }
    Ok(())
}

// ============================== 下载 / 安装 / 启动 ==============================

/// 社区工具下载根目录（可在设置/页面上修改，默认「下载 / NexBox / 社区工具」）
static DOWNLOAD_DIR: OnceLock<StdMutex<Option<PathBuf>>> = OnceLock::new();

fn download_dir_lock() -> &'static StdMutex<Option<PathBuf>> {
    DOWNLOAD_DIR.get_or_init(|| StdMutex::new(None))
}

fn default_download_dir() -> Option<PathBuf> {
    let mut path = dirs::download_dir()?;
    path.push("NexBox");
    path.push("社区工具");
    Some(path)
}

/// 应用启动时从 settings 载入用户配置的下载位置（无则不覆盖默认）
pub fn init_community_download_dir(app: &tauri::AppHandle) {
    if let Some(serde_json::Value::String(s)) = read_settings_value(app, "community_download_dir") {
        if !s.trim().is_empty() && PathBuf::from(&s).is_absolute() {
            *download_dir_lock().lock().unwrap() = Some(PathBuf::from(s));
        }
    }
}

fn download_tools_dir() -> Option<PathBuf> {
    let root = match download_dir_lock().lock().unwrap().clone() {
        Some(r) if r.is_absolute() => r,
        _ => default_download_dir()?,
    };
    let _ = std::fs::create_dir_all(&root);
    Some(root)
}

#[tauri::command]
pub fn get_community_download_dir(app: tauri::AppHandle) -> String {
    if let Some(r) = download_dir_lock().lock().unwrap().clone() {
        if r.is_absolute() {
            return r.to_string_lossy().into_owned();
        }
    }
    if let Some(serde_json::Value::String(s)) = read_settings_value(&app, "community_download_dir") {
        if !s.trim().is_empty() {
            return s;
        }
    }
    default_download_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[tauri::command]
pub fn set_community_download_dir(app: tauri::AppHandle, dir: String) -> Result<String, String> {
    let dir = dir.trim().to_string();
    if dir.is_empty() {
        return Err("目录不能为空".to_string());
    }
    let p = PathBuf::from(&dir);
    if !p.is_absolute() {
        return Err("下载位置必须是绝对路径".to_string());
    }
    std::fs::create_dir_all(&p).map_err(|e| format!("创建目录失败：{e}"))?;
    save_settings_value(&app, "community_download_dir", serde_json::json!(dir));
    *download_dir_lock().lock().unwrap() = Some(p);
    Ok(dir)
}

#[tauri::command]
pub fn pick_community_download_dir() -> Option<String> {
    rfd::FileDialog::new()
        .set_title("选择社区工具下载位置")
        .pick_folder()
        .map(|f| f.to_string_lossy().into_owned())
}

/// 在资源管理器中定位某个文件
fn reveal_in_explorer(path: &std::path::Path) {
    let p = path.to_path_buf();
    let _ = std::thread::spawn(move || {
        let _ = std::process::Command::new("explorer").arg("/select,").arg(&p).spawn();
    });
}

/// 对仓库内相对路径，优先通过 contents 单文件接口拿到 blob sha，返回 blob 直链
/// （https://raw.gitcode.com/{owner}/{repo}/blobs/{sha}/{filename}）。
/// git pull 等工具官方使用 blob 直链下载，可绕过分支 raw 对二进制返回 HTML/404 的问题。
/// 查不到 sha 时回退到 API v5 raw 端点。
async fn resolve_gitcode_file_url(token: &str, relative_path: &str) -> String {
    let encoded = encode_path_segments(relative_path);
    let fname = relative_path.rsplit('/').next().unwrap_or("download");
    // 1) 单文件 contents 接口拿 sha
    if let Ok(v) = api_get(token, &format!("repos/{OWNER}/{REPO}/contents/{encoded}"), &[]).await {
        if let Some(sha) = v.get("sha").and_then(|x| x.as_str()) {
            if !sha.trim().is_empty() {
                return format!(
                    "https://raw.gitcode.com/{OWNER}/{REPO}/blobs/{}/{}",
                    sha,
                    urlencoding::encode(fname)
                );
            }
        }
    }
    // 2) 回退：API v5 raw 端点
    format!(
        "{}repos/{}/{}/raw/{}?ref=main",
        API_BASE,
        OWNER,
        REPO,
        encode_path_segments(relative_path)
    )
}

/// 用可下载直链（blob 优先）下载仓库文件，并按需上报下载进度
async fn download_from_raw(app: &tauri::AppHandle, relative_path: &str, dest: &PathBuf) -> Result<(), String> {
    let url = resolve_gitcode_file_url("", relative_path).await;
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("下载失败：{e}"))?
        .error_for_status()
        .map_err(|e| format!("下载失败：{e}"))?;
    let total = resp.content_length().unwrap_or(0);
    use futures_util::StreamExt;
    let mut file = std::fs::File::create(dest).map_err(|e| format!("创建文件失败：{e}"))?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("下载失败：{e}"))?;
        std::io::Write::write_all(&mut file, &chunk).map_err(|e| format!("写入失败：{e}"))?;
        downloaded += chunk.len() as u64;
        let percent = if total > 0 {
            Some(((downloaded * 100) / total).min(100) as u32)
        } else {
            None
        };
        emit_install_progress(app, "下载中...", percent);
    }
    Ok(())
}

/// 清理路径片段中的非法字符，避免拼进文件名导致失败
fn safe_segment(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c => c,
        })
        .collect()
}

/// 下载到「下载位置」根目录，命名 {分类}-{id}.{ext}，保证显示位置与实际文件位置一致
fn archive_path(root: &PathBuf, category: &str, id: &str, ext: &str) -> PathBuf {
    root.join(format!("{}-{}.{}", safe_segment(category), safe_segment(id), ext))
}

fn find_launch_exe(dir: &PathBuf, launch_target: Option<&str>) -> Option<PathBuf> {
    let wanted = launch_target.map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty());
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return None,
    };
    // 优先精确匹配 launch_target，其次任选一个 exe（浅层优先，避免过大 zip 深度扫描太慢）
    let mut fallback: Option<PathBuf> = None;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file() {
            if let Some(exe_name) = p.file_name().and_then(|n| n.to_str()) {
                if exe_name.to_lowercase().ends_with(".exe") {
                    if fallback.is_none() {
                        fallback = Some(p.clone());
                    }
                    if let Some(w) = &wanted {
                        if exe_name.to_lowercase() == *w {
                            return Some(p);
                        }
                    }
                }
            }
        }
    }
    // 未在顶层命中时，递归搜索 launch_target 精确名
    if let Some(w) = &wanted {
        for e in walkdir::WalkDir::new(dir).into_iter().flatten() {
            if !e.file_type().is_file() {
                continue;
            }
            if e.file_name().to_string_lossy().to_lowercase() == *w {
                return Some(e.path().to_path_buf());
            }
        }
    }
    fallback
}

#[tauri::command]
pub async fn install_community_tool(
    app: tauri::AppHandle,
    category: String,
    id: String,
    file: Option<String>,
    download_url: Option<String>,
) -> Result<String, String> {
    let Some(root) = download_tools_dir() else { return Err("无法定位安装目录".to_string()) };

    let rel_dir = format!("{PLUGINS_PATH}/{category}/{id}");

    // 1) 若有仓库内文件 -> 从 raw 下载到「下载位置」根目录
    if let Some(f) = file.filter(|s| !s.trim().is_empty()) {
        let ext = std::path::Path::new(&f)
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .filter(|e| !e.is_empty())
            .unwrap_or_else(|| "zip".to_string());
        let dest = archive_path(&root, &category, &id, &ext);
        download_from_raw(&app, &format!("{rel_dir}/{f}"), &dest).await?;
        emit_install_progress(&app, "下载完成", Some(100));
        invalidate_community_cache();
        reveal_in_explorer(&dest);
        return Ok(dest.to_string_lossy().into_owned());
    }

    // 2) 外部下载链接
    if let Some(orig) = download_url.filter(|s| !s.trim().is_empty()) {
        // 指向本项目仓库的网页 raw 链接 -> 解析相对路径，走 blob 直链下载
        let url = if let Some(rel) = parse_own_gitcode_raw(&orig) {
            resolve_gitcode_file_url("", &rel).await
        } else if let Some(rest) = orig.strip_prefix("gh:") {
            // gh:owner/repo 简化为 release 下载（此处最小实现：直接用该路径作为相对说明，实际解析留给前端或直接提示）
            format!("https://github.com/{rest}/releases/latest/download/tool.zip")
        } else {
            orig.to_string()
        };
        let ext = orig
            .split('/')
            .last()
            .filter(|s| !s.is_empty())
            .and_then(|s| std::path::Path::new(s).extension())
            .map(|e| e.to_string_lossy().into_owned())
            .filter(|e| !e.is_empty())
            .unwrap_or_else(|| "zip".to_string());
        let dest = archive_path(&root, &category, &id, &ext);
        // 普通 http 直链下载
        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("下载失败：{e}"))?;
        if !resp.status().is_success() {
            return Err(format!("下载失败：HTTP {}", resp.status().as_u16()));
        }
        let total = resp.content_length().unwrap_or(0);
        use futures_util::StreamExt;
        let mut file = std::fs::File::create(&dest).map_err(|e| format!("创建文件失败：{e}"))?;
        let mut stream = resp.bytes_stream();
        let mut downloaded: u64 = 0;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("下载失败：{e}"))?;
            std::io::Write::write_all(&mut file, &chunk).map_err(|e| format!("写入失败：{e}"))?;
            downloaded += chunk.len() as u64;
            let percent = if total > 0 {
                Some(((downloaded * 100) / total).min(100) as u32)
            } else {
                None
            };
            emit_install_progress(&app, "下载中...", percent);
        }
        emit_install_progress(&app, "下载完成", Some(100));
        invalidate_community_cache();
        reveal_in_explorer(&dest);
        return Ok(dest.to_string_lossy().into_owned());
    }

    Err("该工具没有提供下载源".to_string())
}

/// 将 GitCode 网页版 raw 链接（https://gitcode.com/{owner}/{repo}/-/raw/{branch}/{path}）解析为仓库相对路径。
/// 仅对指向本项目 OWNER/REPO 的链接生效；非本项目 raw 链接返回 None。
fn parse_own_gitcode_raw(url: &str) -> Option<String> {
    let prefix = format!("https://gitcode.com/{OWNER}/{REPO}/-/raw/");
    let path = url.strip_prefix(&prefix)?.split_once('/')?.1;
    Some(path.to_string())
}

/// 在「下载位置」根目录定位某工具的压缩包（文件名 {分类}-{id}.{ext}），未找到则报未下载
#[tauri::command]
pub fn open_community_zip(
    category: String,
    id: String,
    _file: Option<String>,
) -> Result<(), String> {
    let Some(root) = download_tools_dir() else { return Err("无法定位下载目录".to_string()) };
    let prefix = format!("{}-{}", safe_segment(&category), safe_segment(&id));
    let zip_path = std::fs::read_dir(&root)
        .ok()
        .and_then(|rd| {
            rd.flatten().find(|e| {
                e.file_type().map(|t| t.is_file()).unwrap_or(false)
                    && e.file_name()
                        .to_string_lossy()
                        .to_lowercase()
                        .starts_with(&format!("{prefix}."))
            })
        })
        .map(|e| e.path())
        .ok_or_else(|| "未找到已下载的压缩包".to_string())?;
    reveal_in_explorer(&zip_path);
    Ok(())
}

// 已弃用：社区工具改为「下载 ZIP + 资源管理器定位」由用户自行解压运行，前端不再调用本命令。
#[tauri::command]
pub async fn run_community_tool(
    category: String,
    id: String,
    launch_target: Option<String>,
) -> Result<(), String> {
    let Some(root) = download_tools_dir() else { return Err("无法定位安装目录".to_string()) };
    let tool_dir = root.join(&category).join(&id);
    if !tool_dir.exists() {
        return Err("工具未安装".to_string());
    }
    let exe = find_launch_exe(&tool_dir, launch_target.as_deref())
        .ok_or_else(|| "未找到可执行文件".to_string())?;
    let exe_str = exe.to_string_lossy().into_owned();
    let workdir = exe.parent().and_then(|p| p.to_str()).unwrap_or("").to_string();
    let _ = std::process::Command::new("cmd")
        .args(["/c", "start", "", &exe_str])
        .current_dir(workdir)
        .creation_flags(0x08000000)
        .spawn()
        .map_err(|e| format!("启动失败：{e}"))?;
    Ok(())
}

#[tauri::command]
pub async fn pick_community_package() -> Option<String> {
    rfd::FileDialog::new()
        .set_title("选择社区工具压缩包")
        .add_filter("压缩包", &["zip"])
        .add_filter("所有文件", &["*"])
        .pick_file()
        .map(|f| f.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn pick_community_icon() -> Option<String> {
    rfd::FileDialog::new()
        .set_title("选择社区工具图标")
        .add_filter("图标", &["png", "jpg", "jpeg", "ico", "bmp"])
        .add_filter("所有文件", &["*"])
        .pick_file()
        .map(|f| f.to_string_lossy().into_owned())
}

/// 列出压缩包内的 .exe 相对路径（供“启动目标”选择）
#[tauri::command]
pub async fn list_zip_entry_exes(zip_path: String) -> Result<Vec<String>, String> {
    let file = std::fs::File::open(&zip_path).map_err(|e| format!("打开压缩包失败：{e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("解析压缩包失败：{e}"))?;
    let mut list = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|e| format!("读取条目失败：{e}"))?;
        let name = entry.name();
        if name.to_lowercase().ends_with(".exe") {
            list.push(name.to_string());
        }
    }
    list.sort();
    Ok(list)
}
