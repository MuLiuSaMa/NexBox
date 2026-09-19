use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tauri::Manager;
use tokio::sync::RwLock;

/// 外设驱动配置文件地址（gitee 仓库 muliuawa/nexbox，与 qq_groups.json / ads.json 同目录）。
/// 数据文件更新后，应用在缓存过期后自动拉到最新列表，实现"外设驱动"栏目远程增改。
const PERIPHERAL_DRIVERS_URL: &str =
    "https://gitee.com/muliuawa/nexbox/raw/master/peripheral_drivers.json";
const CONNECT_TIMEOUT_SECS: u64 = 3;
const REQUEST_TIMEOUT_SECS: u64 = 6;
/// 内存缓存时长，避免每次进入外设驱动页面都请求 gitee
const MEMORY_CACHE_TTL_SECS: u64 = 600;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeripheralDriver {
    pub id: String,
    /// 品牌显示名，如 "雷蛇 Razer"
    pub name: String,
    /// 图标 URL（可放 gitee 仓库 icons/ 目录），为空时前端显示品牌首字母
    #[serde(default)]
    pub icon: String,
    /// 驱动地址：有在线驱动的品牌填网页驱动地址，否则填官方驱动/下载页
    pub url: String,
    /// online=网页在线驱动，download=官方驱动下载页
    #[serde(default)]
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PeripheralDriverResponse {
    pub update_time: String,
    pub drivers: Vec<PeripheralDriver>,
}

/// 内置兜底数据：gitee 拉取失败/为空时使用，保证功能可用（内容与 peripheral_drivers.json 一致）
fn default_drivers() -> Vec<PeripheralDriver> {
    let d = |id: &str, name: &str, url: &str, kind: &str, _icon: &str| PeripheralDriver {
        id: id.to_string(),
        name: name.to_string(),
        // 图标统一托管在 gitee 仓库 icons/ 目录，按品牌 id 命名（<id>.webp）
        icon: format!(
            "https://raw.giteeusercontent.com/muliuawa/nexbox/raw/master/icons/{id}.webp"
        ),
        url: url.to_string(),
        kind: kind.to_string(),
    };
    vec![
        d("razer", "雷蛇 Razer", "https://synapse.razer.com/", "online", ""),
        d("logitech", "罗技 Logitech", "https://www.logitechg.com/en-us/software/ghub", "download", ""),
        d("steelseries", "赛睿 SteelSeries", "https://steelseries.com/gg", "download", ""),
        d("corsair", "海盗船 Corsair", "https://www.corsair.com/cn/zh/s/downloads", "download", ""),
        d("hyperx", "HyperX", "https://www.hyperx.com/zh-cn/software", "download", ""),
        d("rog", "华硕 ROG (奥创中心)", "https://rog.asus.com.cn/content/armoury-crate/", "download", ""),
        d("rog-se", "奥创极速版 Armoury Crate SE", "https://www.asus.com.cn/supportonly/armoury%20crate/helpdesk_download/", "download", ""),
        d("zowie", "卓威 ZOWIE", "https://zowiegear.benq.com.cn/zh-cn/support/download-search.html", "download", ""),
        d("vgn", "VGN", "https://drive.vgnlab.com.cn/", "online", ""),
        d("mchose", "迈从 MCHOSE", "https://www.mchose.com.cn/#/home", "online", ""),
        d("atk", "ATK", "https://www.atk.store/pages/atk-hub", "online", ""),
        d("akko", "AKKO", "https://web.akkogear.com/", "online", ""),
        d("aula", "狼蛛 AULA", "https://hub.aulacn.com/", "online", ""),
        d("rk", "RK Royal Kludge", "https://drive.rkgaming.com/", "online", ""),
        d("dareu", "达尔优 DAREU", "https://dr.dareu.com/", "online", ""),
        d("rapoo", "雷柏 Rapoo", "https://www.rapoo.cn/downloadcenter", "download", ""),
        d("eyooso", "前行者 E-YOOSO", "https://www.e-yooso.com/download", "download", ""),
        d("ajazz", "黑爵 AJAZZ", "https://www.a-jazz.com/h-col-160.html", "online", ""),
        d("ganss", "高斯 GANSS", "https://www.ganss.cn/page/drives/", "download", ""),
        d("bloody", "血手幽灵 Bloody", "https://www.bloody.cn/Download.php", "download", ""),
        d("thunderobot", "雷神 THUNDEROBOT", "https://www.thunderobot.com/", "download", ""),
        d("machenike", "机械师 MACHENIKE", "https://www.machenike.com/offline/driverUnit", "download", ""),
        d("keychron", "Keychron", "https://launcher.keychron.com/", "online", ""),
        d("wooting", "Wooting", "https://wootility.io/", "online", ""),
        d("nuphy", "NuPhy", "https://www.nuphy.io/en-US", "online", ""),
        d("iqunix", "IQUNIX", "https://docs.iqunix.com/", "download", ""),
        d("durgod", "杜伽 DURGOD", "https://www.durgod.com/driver-download/", "download", ""),
        d("vortex", "Vortex", "https://vortexgear.store/blogs/updates-downloads", "download", ""),
        d("cherry", "樱桃 CHERRY XTRFY", "https://www.cherry.cn/cherry_magcrate.html", "download", ""),
        d("gravastar", "重力星球 GravaStar", "https://hub.gravastar1.com/gravastar/connect", "online", ""),
        d("irok", "艾石头 IROK", "https://via.irok.cn/", "online", ""),
        d("ninjutso", "Ninjutso", "https://ninjaforce.ninjutso.cn/", "online", ""),
        d("lunafury", "Lunafury", "https://www.lunafury.games/", "online", ""),
        d("madcatz", "美加狮 MADCATZ", "https://www.madcatz.com/Zh/Support/Downloads", "download", ""),
    ]
}

struct MemoryCache {
    data: Option<Vec<PeripheralDriver>>,
    fetched_at: Option<Instant>,
}

impl MemoryCache {
    fn new() -> Self {
        Self {
            data: None,
            fetched_at: None,
        }
    }

    fn get(&self) -> Option<Vec<PeripheralDriver>> {
        if let (Some(data), Some(fetched_at)) = (&self.data, &self.fetched_at) {
            if fetched_at.elapsed() < Duration::from_secs(MEMORY_CACHE_TTL_SECS) {
                return Some(data.clone());
            }
        }
        None
    }

    fn set(&mut self, data: Vec<PeripheralDriver>) {
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

async fn fetch_peripheral_drivers() -> Result<Vec<PeripheralDriver>, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(PERIPHERAL_DRIVERS_URL)
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

    let data: PeripheralDriverResponse =
        serde_json::from_str(&text).map_err(|e| format!("JSON parse error: {}", e))?;

    let drivers = if data.drivers.is_empty() {
        // gitee 文件为空时回退内置数据
        default_drivers()
    } else {
        data.drivers
    };

    let cache = get_memory_cache();
    cache.write().await.set(drivers.clone());

    Ok(drivers)
}

/// 获取外设驱动列表：优先内存缓存，其次 gitee 配置，最后内置兜底
#[tauri::command]
pub async fn get_peripheral_drivers() -> Vec<PeripheralDriver> {
    {
        let cache = get_memory_cache();
        if let Some(data) = cache.read().await.get() {
            return data;
        };
    }

    match fetch_peripheral_drivers().await {
        Ok(data) => data,
        Err(e) => {
            log::warn!("Failed to fetch peripheral drivers: {e}, using default list");
            default_drivers()
        }
    }
}

/// 后端下载品牌图标到应用缓存，返回本地文件路径（前端用 convertFileSrc 转换成可显示地址）。
/// WebView 通常无法直接显示 gitee raw 图，走后端下载即可正常显示，来源仍是 gitee icons/ 目录。
#[tauri::command]
pub async fn get_peripheral_driver_icon(app: tauri::AppHandle, url: String) -> Result<String, String> {
    if url.trim().is_empty() {
        return Ok(String::new());
    }

    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("drive_icons");
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
            return Err(format!("icon http {}", resp.status()));
        }
        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
        std::fs::write(&file, &bytes).map_err(|e| e.to_string())?;
    }

    Ok(file.to_string_lossy().to_string())
}