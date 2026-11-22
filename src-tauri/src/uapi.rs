//! UAPI (uapis.cn) 接口接入模块。
//!
//! 目前实现「随机图片」接口：
//!   GET https://uapis.cn/api/v1/random/image
//! 文档：https://uapis.cn/docs/api-reference/get-random-image
//!
//! 设计说明：
//! - 请求统一走 Rust 后端，避免 WebView 跨域 (CORS) 限制；
//! - API 密钥通过 `Authorization: Bearer <KEY>` 请求头发送，密钥以 `uapi-` 开头；
//! - 密钥读取优先级：环境变量 `UAPI_API_KEY` > 前端配置（设置页输入，经 store 持久化）。

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;
use std::time::Duration;

/// 随机图片接口完整 API 地址（文档标注的完整地址，含 /api/v1 前缀）
const RANDOM_IMAGE_URL: &str = "https://uapis.cn/api/v1/random/image";
/// 从环境变量读取密钥时的变量名
const API_KEY_ENV_VAR: &str = "UAPI_API_KEY";

const CONNECT_TIMEOUT_SECS: u64 = 5;
const REQUEST_TIMEOUT_SECS: u64 = 20;

/// 接口支持的主类别（与官方文档一致，用于参数校验）
const SUPPORTED_CATEGORIES: &[&str] = &[
    "acg",
    "landscape",
    "anime",
    "pc_wallpaper",
    "mobile_wallpaper",
    "general_anime",
    "ai_drawing",
    "bq",
    "furry",
];

/// 接口支持的子类别（与官方文档一致，用于参数校验）
const SUPPORTED_TYPES: &[&str] = &[
    "pc",
    "mb",
    "eciyuan",
    "ikun",
    "4k",
    "s4k",
    "z4k",
    "szs8k",
    "xiongmao",
    "maomao",
    "waiguoren",
];

/// UAPI 文档约定的 JSON 错误响应体
#[derive(Debug, Deserialize)]
struct UapiErrorBody {
    code: Option<String>,
    message: Option<String>,
}

/// 解析 API 密钥：
/// 1. 环境变量 `UAPI_API_KEY`（最高优先级）
/// 2. 前端配置传入（来自设置页输入框，经 settings.json 持久化）
fn resolve_api_key(frontend_key: Option<String>) -> Option<String> {
    if let Ok(env_key) = std::env::var(API_KEY_ENV_VAR) {
        let trimmed = env_key.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    frontend_key
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
}

/// 调用 UAPI 随机图片接口，成功时返回图片二进制（base64 编码）。
///
/// # 参数
/// - `category`: 可选，图片主类别（acg / landscape / anime / ...），不传则全局随机
/// - `image_type`: 可选，图片子类别（pc / mb / z4k / ...）
/// - `api_key`: 可选，前端设置中配置的密钥（uapi- 开头）
///
/// # 错误
/// 错误信息统一格式为 `CODE|message`，`CODE` 可能取值：
/// - `INVALID_CATEGORY` / `INVALID_TYPE`：参数校验失败
/// - `NETWORK_ERROR` / `READ_ERROR` / `CLIENT_ERROR` / `EMPTY_RESPONSE`：网络与读取异常
/// - 文档错误码：`NOT_FOUND`、`INTERNAL_SERVER_ERROR`，以及鉴权/限流类（401/402/403/429 等）
#[tauri::command]
pub async fn get_random_image(
    category: Option<String>,
    image_type: Option<String>,
    api_key: Option<String>,
) -> Result<String, String> {
    // ---- 参数校验 ----
    if let Some(c) = category.as_deref().map(str::trim) {
        if !c.is_empty() && !SUPPORTED_CATEGORIES.contains(&c) {
            return Err(format!(
                "INVALID_CATEGORY|不支持的图片类别「{}」，可选值：{}",
                c,
                SUPPORTED_CATEGORIES.join("、")
            ));
        }
    }
    if let Some(t) = image_type.as_deref().map(str::trim) {
        if !t.is_empty() && !SUPPORTED_TYPES.contains(&t) {
            return Err(format!(
                "INVALID_TYPE|不支持的图片子类别「{}」，可选值：{}",
                t,
                SUPPORTED_TYPES.join("、")
            ));
        }
    }

    let key = resolve_api_key(api_key);

    // ---- 构建 HTTP 客户端（连接/总超时） ----
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("CLIENT_ERROR|创建 HTTP 客户端失败: {e}"))?;

    let mut request = client.get(RANDOM_IMAGE_URL);
    if let Some(c) = category.as_deref().map(str::trim) {
        if !c.is_empty() {
            request = request.query(&[("category", c)]);
        }
    }
    if let Some(t) = image_type.as_deref().map(str::trim) {
        if !t.is_empty() {
            request = request.query(&[("type", t)]);
        }
    }
    if let Some(k) = &key {
        // 鉴权：密钥放在请求头 Authorization: Bearer <KEY>
        request = request.header("Authorization", format!("Bearer {k}"));
    }

    // ---- 发起请求，捕获网络异常 ----
    let response = request
        .send()
        .await
        .map_err(|e| format!("NETWORK_ERROR|网络请求失败: {e}"))?;

    let status = response.status();

    // ---- 成功：返回图片二进制 (image/jpeg)，base64 编码 ----
    if status.is_success() {
        let bytes = response
            .bytes()
            .await
            .map_err(|e| format!("READ_ERROR|读取图片数据失败: {e}"))?;
        if bytes.is_empty() {
            return Err("EMPTY_RESPONSE|接口返回了空内容".to_string());
        }
        return Ok(STANDARD.encode(&bytes));
    }

    // ---- 非 2xx：解析错误码，连同限流提示一起返回 ----
    let retry_hint = response
        .headers()
        .get("Retry-After")
        .and_then(|v| v.to_str().ok())
        .map(|s| format!("（Retry-After: {s}s）"))
        .unwrap_or_default();

    let body = response.text().await.unwrap_or_default();
    let (err_code, err_message) = if body.trim().is_empty() {
        (status.to_string(), "接口未返回详细错误信息".to_string())
    } else if let Ok(parsed) = serde_json::from_str::<UapiErrorBody>(&body) {
        (
            parsed.code.unwrap_or_else(|| status.to_string()),
            parsed
                .message
                .unwrap_or_else(|| "接口未返回详细错误信息".to_string()),
        )
    } else {
        // 非 JSON 响应体（例如网关错误页），截断过长内容避免刷屏
        let mut msg = body.trim().to_string();
        if msg.len() > 200 {
            msg.truncate(200);
            msg.push('…');
        }
        (status.to_string(), msg)
    };

    Err(format!("{err_code}|HTTP {status} {retry_hint}{err_message}"))
}

/// 将随机图片的 base64 数据写入磁盘（用于「下载图片」功能）。
///
/// # 参数
/// - `base64_data`: 图片二进制（base64 编码）
/// - `path`: 用户选择的完整保存路径（由前端 `save()` 对话框产生）
#[tauri::command]
pub fn save_random_image_bytes(base64_data: String, path: String) -> Result<(), String> {
    use std::io::Write;

    let data = base64_data.trim();
    if data.is_empty() {
        return Err("EMPTY_DATA|图片数据为空，无法保存".to_string());
    }
    let path_str = path.trim();
    if path_str.is_empty() {
        return Err("INVALID_PATH|保存路径不能为空".to_string());
    }

    let bytes = STANDARD
        .decode(data)
        .map_err(|e| format!("DECODE_ERROR|图片数据解码失败: {e}"))?;

    let file_path = std::path::Path::new(path_str);
    let mut file = std::fs::File::create(file_path)
        .map_err(|e| format!("WRITE_ERROR|创建文件失败: {e}"))?;
    file.write_all(&bytes)
        .map_err(|e| format!("WRITE_ERROR|写入文件失败: {e}"))?;

    Ok(())
}
