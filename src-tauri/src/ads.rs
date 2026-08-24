use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tauri::Manager;
use tokio::sync::RwLock;

/// 广告配置文件地址（gitee 仓库 muliuawa/nexbox，与 qq_groups.json / notice.json 同目录）
const ADS_URL: &str = "https://gitee.com/muliuawa/nexbox/raw/master/ads.json";
const CONNECT_TIMEOUT_SECS: u64 = 3;
const REQUEST_TIMEOUT_SECS: u64 = 6;
/// 内存缓存时长，避免每次启动/回到主页都请求 gitee
const MEMORY_CACHE_TTL_SECS: u64 = 600;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplashAd {
    /// 开屏广告图片 URL（gitee 仓库），空则不展示该条
    #[serde(default)]
    pub image: String,
    /// 点击跳转链接，为空时前端不可点击
    #[serde(default)]
    pub link: String,
    /// 图下小字标识（如俱乐部名），可缺省
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomeAd {
    /// 主页广告卡片图片 URL，空则不展示该条
    #[serde(default)]
    pub image: String,
    /// 俱乐部名
    #[serde(default)]
    pub name: String,
    /// 简介文案
    #[serde(default)]
    pub description: String,
    /// 点击跳转链接，为空时前端不可点击
    #[serde(default)]
    pub link: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdsConfig {
    pub update_time: String,
    pub splash: Vec<SplashAd>,
    pub home: Vec<HomeAd>,
}

struct MemoryCache {
    data: Option<AdsConfig>,
    fetched_at: Option<Instant>,
}

impl MemoryCache {
    fn new() -> Self {
        Self {
            data: None,
            fetched_at: None,
        }
    }

    fn get(&self) -> Option<AdsConfig> {
        if let (Some(data), Some(fetched_at)) = (&self.data, &self.fetched_at) {
            if fetched_at.elapsed() < Duration::from_secs(MEMORY_CACHE_TTL_SECS) {
                return Some(data.clone());
            }
        }
        None
    }

    fn set(&mut self, data: AdsConfig) {
        self.data = Some(data);
        self.fetched_at = Some(Instant::now());
    }
}

static MEMORY_CACHE: OnceLock<Arc<RwLock<MemoryCache>>> = OnceLock::new();

fn get_memory_cache() -> Arc<RwLock<MemoryCache>> {
    MEMORY_CACHE
        .get_or_init(|| Arc::new(RwLock::new(MemoryCache::new())))
        .clone()
}

async fn fetch_ads() -> Result<AdsConfig, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(ADS_URL)
        .send()
        .await
        .map_err(|e| format!("Network request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    let data: AdsConfig =
        serde_json::from_str(&text).map_err(|e| format!("JSON parse error: {}", e))?;

    let cache = get_memory_cache();
    cache.write().await.set(data.clone());

    Ok(data)
}

/// 获取广告配置：优先内存缓存，其次 gitee 配置。失败/为空时返回空配置（不显示任何广告）。
#[tauri::command]
pub async fn get_ads() -> AdsConfig {
    {
        let cache = get_memory_cache();
        if let Some(data) = cache.read().await.get() {
            return data;
        };
    }

    match fetch_ads().await {
        Ok(data) => data,
        Err(e) => {
            log::warn!("Failed to fetch ads: {e}, returning empty ads");
            AdsConfig::default()
        }
    }
}

/// 下载广告图片到应用缓存，返回本地文件路径（前端用 convertFileSrc 转换成可显示地址）。
/// 这样不依赖 WebView 直连 gitee（WebView 通常加载不了 gitee raw 图），来源仍是 gitee（远程）。
#[tauri::command]
pub async fn get_ad_image(app: tauri::AppHandle, url: String) -> Result<String, String> {
    if url.trim().is_empty() {
        return Ok(String::new());
    }

    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("ad_images");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    let file = dir.join(format!("{}.img", hasher.finish()));

    if !file.exists() {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(20))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 NexBox")
            .build()
            .map_err(|e| format!("client error: {e}"))?;

        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("network error: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("image http {}", resp.status()));
        }
        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
        std::fs::write(&file, &bytes).map_err(|e| e.to_string())?;
    }

    Ok(file.to_string_lossy().to_string())
}