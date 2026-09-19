use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// 「特别鸣谢」配置文件地址（gitee 仓库 muliuawa/nexbox，与 qq_groups.json / sponsors.json 同目录）
const THANKS_URL: &str = "https://gitee.com/muliuawa/nexbox/raw/master/thanks.json";
const CONNECT_TIMEOUT_SECS: u64 = 3;
const REQUEST_TIMEOUT_SECS: u64 = 6;
/// 内存缓存时长：关于页每次打开都要尽量实时，所以 TTL 设短一些
const MEMORY_CACHE_TTL_SECS: u64 = 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThanksItem {
    pub name: String,
    pub url: String,
    /// 项目 Logo URL（gitee raw 等），为空时前端不显示图片
    #[serde(default)]
    pub logo: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThanksRoot {
    pub update_time: String,
    pub items: Vec<ThanksItem>,
}

/// 内置兜底数据：gitee 拉取失败/为空时使用（与 thanks.json 内容一致）
fn default_thanks() -> Vec<ThanksItem> {
    vec![
        ThanksItem {
            name: "AtomGit".to_string(),
            url: "https://atomgit.com".to_string(),
            logo: "https://raw.giteeusercontent.com/muliuawa/nexbox/raw/master/web/gitcode.webp".to_string(),
        },
        ThanksItem {
            name: "SJMC Launcher".to_string(),
            url: "https://mc.sjtu.cn/sjmcl".to_string(),
            logo: "https://raw.giteeusercontent.com/muliuawa/nexbox/raw/master/web/SJMCL.webp".to_string(),
        },
        ThanksItem {
            name: "图吧工具箱CE".to_string(),
            url: "https://tubawinui3.cn/".to_string(),
            logo: "https://raw.giteeusercontent.com/muliuawa/nexbox/raw/master/web/tuba.webp".to_string(),
        },
        ThanksItem {
            name: "Pyisland".to_string(),
            url: "https://www.pyisland.com/".to_string(),
            logo: "https://raw.giteeusercontent.com/muliuawa/nexbox/raw/master/web/Pyisland.webp".to_string(),
        },
        ThanksItem {
            name: "Watt Toolkit".to_string(),
            url: "https://steampp.net/".to_string(),
            logo: "https://raw.giteeusercontent.com/muliuawa/nexbox/raw/master/web/watt-toolkit.webp".to_string(),
        },
        ThanksItem {
            name: "MCTier".to_string(),
            url: "https://mctier.pmhs.top/".to_string(),
            logo: "https://raw.giteeusercontent.com/muliuawa/nexbox/raw/master/web/MCTier.webp".to_string(),
        },
        ThanksItem {
            name: "MineRadio".to_string(),
            url: "https://mineradio.cn/".to_string(),
            logo: "https://raw.giteeusercontent.com/muliuawa/nexbox/raw/master/web/MR.webp".to_string(),
        },
        ThanksItem {
            name: "小哆啦工具箱".to_string(),
            url: "https://gitee.com/doralite/doras-little-toolbox".to_string(),
            logo: "https://raw.giteeusercontent.com/muliuawa/nexbox/raw/master/web/duola.webp".to_string(),
        },
    ]
}

struct MemoryCache {
    data: Option<Vec<ThanksItem>>,
    fetched_at: Option<Instant>,
}

impl MemoryCache {
    fn new() -> Self {
        Self {
            data: None,
            fetched_at: None,
        }
    }

    fn get(&self) -> Option<Vec<ThanksItem>> {
        if let (Some(data), Some(fetched_at)) = (&self.data, &self.fetched_at) {
            if fetched_at.elapsed() < Duration::from_secs(MEMORY_CACHE_TTL_SECS) {
                return Some(data.clone());
            }
        }
        None
    }

    fn set(&mut self, data: Vec<ThanksItem>) {
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

async fn fetch_thanks() -> Result<Vec<ThanksItem>, String> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(THANKS_URL)
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

    let data: ThanksRoot =
        serde_json::from_str(&text).map_err(|e| format!("JSON parse error: {}", e))?;

    let items = if data.items.is_empty() {
        // gitee 文件为空时回退内置数据
        default_thanks()
    } else {
        data.items
    };

    let cache = get_memory_cache();
    cache.write().await.set(items.clone());

    Ok(items)
}

/// 获取「特别鸣谢」列表：优先内存缓存，其次 gitee 配置，最后内置兜底
#[tauri::command]
pub async fn get_thanks() -> Vec<ThanksItem> {
    {
        let cache = get_memory_cache();
        if let Some(data) = cache.read().await.get() {
            return data;
        };
    }

    match fetch_thanks().await {
        Ok(data) => data,
        Err(e) => {
            log::warn!("Failed to fetch thanks list: {e}, using default thanks");
            default_thanks()
        }
    }
}