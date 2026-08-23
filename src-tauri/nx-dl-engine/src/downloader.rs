// ===========================================================================
// AGPL-3.0 SOURCE PORT NOTICE
// 本文件源自 FluxDown 项目: https://github.com/zerx-lab/FluxDown
// 原始路径: native/engine/src/downloader.rs (裁剪版: 移除 DownloadParams/run_download 管理层, 仅保留协调器所需原语)
// 许可证: GNU Affero General Public License v3.0 (AGPL-3.0)
// 依据 GPL-3.0 第 13 条并入 NexBox(GPL-3.0);对本文件的修改须继续以
// AGPL-3.0 授权并保留本声明。上游提交哈希以 FluxDown-main 快照为准。
// ===========================================================================
use std::error::Error as StdError;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::Client;
use reqwest::header::HeaderValue;
use thiserror::Error;

use crate::logger::log_info;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Error, Debug)]
pub enum DownloadError {
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("db error: {0}")]
    Db(#[from] crate::db::DbError),
    #[error("cancelled")]
    Cancelled,
    #[error("checksum mismatch: {0}")]
    ChecksumMismatch(String),
    /// Server does not honour `Range` requests — returned the enclosed HTTP
    /// status (e.g. `200 OK`) instead of `206 Partial Content`.
    /// Multi-segment assembly is impossible; the caller should fall back to
    /// single-stream mode.
    #[error("server does not support Range requests (returned {0} instead of 206 Partial Content)")]
    RangeNotSupported(String),
    /// 服务器在 probe 与分段/续传请求之间【更换了文件】：Range 响应的
    /// validator（ETag/Last-Modified）与已落盘版本不一致。与
    /// [`RangeNotSupported`] 严格区分：后者是服务器根本不支持 Range；本变体
    /// 意味着旧数据已不能与当前响应拼接，必须清空临时文件后重新下载。文件变化
    /// 与服务器 Range 能力无关，因此绝不记录主机单连接缓存。
    #[error("file changed on server during download (validator mismatch, server returned {0})")]
    VersionChanged(String),
    /// 服务器对 `Range: bytes=X-Y` 请求回了 `206 Partial Content`，但响应的
    /// `Content-Range` 起点与我们请求的偏移【不一致】（或整体缺失且请求非从 0
    /// 起）——典型于劣质 CDN（如 123 盘免费下载节点）在签名 URL 失效/超配额时，
    /// 对任意 Range 请求都回 206 却实际返回【从 byte 0 的全量流】。若不拦截，
    /// seek 到段偏移写入的却是文件开头字节 → 各段字节数写满区间（骗过末尾仅校验
    /// 字节数量的完整性检查），但内容整体错位 → 完整大小的损坏文件（无 checksum
    /// 时无从察觉）。与 [`RangeNotSupported`]（非 206）、[`VersionChanged`]（validator
    /// 不匹配的 200）严格区分：本变体是【206 但区间错位】，重试只会拿到同样的错位
    /// 响应，故调用方应立即回退单流（单流全量请求不带 Range，服务器"忽略 Range 返
    /// 全量"的行为对单流反而正确，能下到完整文件），【绝不】记录主机单连接缓存，
    /// 也【绝不】当瞬时错误退避重试。
    #[error(
        "server returned a misaligned Range response (206 but Content-Range does not match the requested offset: {0})"
    )]
    RangeMisaligned(String),
    /// 多段下载时，服务器在 206 响应的 `Content-Range: bytes X-Y/<total>` 里【自报的
    /// 真实总大小】明显【大于】本次规划的总大小。典型成因（BUG-HTTP-HINT-UNDERSIZED）：
    /// 浏览器扩展在 `<video>` Range 流式播放一段【仍在渐进上传】的视频时，抓到的是
    /// 【当时的部分大小】并作为 `hint_file_size` 传入；hint 模式为保护一次性签名 URL 而
    /// 跳过 probe，把这个偏小的 hint 当作权威总大小，多段只请求 `[0, hint)` → 拿满即
    /// 完成 → 落盘的是完整文件的【前缀】（静默截断，无 checksum 时无从察觉）。
    ///
    /// 与 [`RangeMisaligned`]（206 但区间【错位】、数据错位）严格区分：本变体区间
    /// 【对齐】、已下字节【正确】，只是规划的总量偏小。携带值为服务器自报的真实总
    /// 大小。coordinator 捕获后【就地扩容】（延长预分配 + 追加尾段，已下数据零丢弃，
    /// 见 `segment_coordinator` 的 `MAX_SIZE_EXPANSIONS`）；仅当扩容配额耗尽（文件
    /// 持续增长/病态分母膨胀）或扩容无法执行时才冒泡到 `run_download_inner`，以
    /// status=4 显式终止——DB 段行与临时文件保留，重试时 resume 重新 probe 续下。
    /// 绝不记录主机单连接缓存（与 Range 能力无关），也绝不当瞬时错误退避重试
    /// （重试只会拿到同样的分母）。
    #[error("server reports a larger true size than planned (Content-Range total: {0})")]
    TrueSizeLarger(i64),
    /// 多 CDN 节点池中【单个钉定节点】的可归因失败（连接失败/超时/停滞/
    /// 跨节点 validator 不一致/HTTP 拒绝）。由 worker 在钉定租约上翻译产生
    /// （见 `cdn::is_node_attributable`），coordinator 按 retryable 语义把段
    /// 回收重派（下一次租约自然避开被降权/踢除的节点），【绝不】把单节点
    /// 的问题升级为任务失败。SYS（系统 DNS）节点的错误不经翻译，语义与
    /// 现状完全一致——故本变体永不成为任务的 final_error。
    #[error("cdn node failed: {0}")]
    CdnNodeFailed(String),
    #[error("ed2k error: {0}")]
    Ed2k(String),
    /// ED2K 协议完整性违规：hashset 投毒 / 块 MD4 不匹配 / SENDINGPART 越界 /
    /// 未请求数据 / 区间碎片超限。与 [`DownloadError::Ed2k`]（纯网络类）区分，
    /// 调度层据此把违规 peer 拉黑（贯穿整个下载调用），而非仅退避。
    #[error("ed2k integrity violation: {0}")]
    Ed2kIntegrity(String),
    #[error("{0}")]
    Other(String),
}

/// 检测下载错误是否为服务器主动拒绝（403 Forbidden / 429 Too Many Requests）。
///
/// 这类错误通常意味着服务器限制了并发连接数，多段下载的额外连接被拒绝。
/// 与网络超时、连接重置等瞬时错误不同，重试这类错误毫无意义——应当立即
/// 通知 coordinator 进行降级处理。
pub(crate) fn is_server_rejection(e: &DownloadError) -> bool {
    match e {
        DownloadError::Request(req_err) => {
            if let Some(status) = req_err.status() {
                matches!(status.as_u16(), 403 | 429)
            } else {
                false
            }
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

pub struct FileInfo {
    pub file_name: String,
    pub total_bytes: i64,
    pub supports_range: bool,
    /// MIME content type from the server (e.g. "text/html", "application/octet-stream").
    /// Empty when the probe phase was skipped (hint_file_size > 0).
    pub content_type: String,
    /// ETag header value from the server (e.g. `"abc123"` or `W/"abc123"`).
    /// Used by multi-segment downloads to verify all connections fetch the same
    /// file version.  Empty when the server did not provide an ETag.
    pub etag: String,
    /// Last-Modified header value from the server (RFC 7232 §2.2).
    /// Used together with `etag` for file-identity verification across segments.
    /// Empty when the server did not provide Last-Modified.
    pub last_modified: String,
    /// `true` when the server's probe response included a `Content-Encoding`
    /// other than `identity` (e.g. gzip, br, deflate).  Because reqwest is
    /// built WITHOUT gzip/brotli/deflate Cargo features, the compressed bytes
    /// would be written raw to disk, corrupting the file.  Callers should
    /// treat this as a warning and avoid multi-segment downloads.
    #[allow(dead_code)]
    pub content_encoding_compressed: bool,
}

#[derive(Default)]
pub struct ProgressUpdate {
    pub task_id: String,
    pub downloaded_bytes: i64,
    pub total_bytes: i64,
    pub status: i32,
    pub error_message: String,
    /// Non-empty only on initial status=1 update (resolved file name).
    pub file_name: String,
    /// Per-segment progress info (for IDM-style visualization).
    /// `None` for single-thread downloads; `Some(vec)` for multi-segment.
    pub segment_details: Option<Vec<SegmentProgressInfo>>,
    /// 实时上传速率（字节/秒）。仅 BT 任务的周期性进度上报携带非零值
    /// （librqbit 统计），其余协议恒为 0。
    pub upload_speed_bps: i64,
    /// BT 数据下载完成标记：`stats.finished` 时刻（piece 全部下完，但校验
    /// 与 staging→save_dir 搬移尚未完成、任务未进终态）置 `true` 一次。
    /// `progress_reporter` 据此立即发 `EngineEvent::BtDataFinished`（按
    /// task_id 去重），对应 aria2 `onBtDownloadComplete` 通知语义。
    pub bt_data_finished: bool,
    /// 已上传字节数（BT 做种）。仅 BT 任务有意义，默认 0。
    pub uploaded_bytes: i64,
    /// Seeding status: 0=none, 1=active seeding, 2=ratio reached,
    /// 3=time reached, 4=user stopped, 5=task deleted, 6=session released,
    /// 7=inactive time reached, 8=queued for a seeding slot.
    pub seeding_status: i32,
    /// BT 做种状态的辅助说明（如停止原因）。无错误/未做种时为空。
    pub seeding_message: String,
    /// 累计做种秒数（发帧时刻；排队/暂停不计）。仅 BT 做种帧非零。
    pub seeding_time_secs: i64,
}

/// Snapshot of a single segment's progress, sent from downloader to progress_reporter.
#[derive(Clone)]
pub struct SegmentProgressInfo {
    pub index: i32,
    pub start_byte: i64,
    pub end_byte: i64,
    pub downloaded_bytes: i64,
}

/// 将浏览器扩展捕获的额外 HTTP 头应用到请求构建器上。
///
/// 使用 `req.headers(map)` 而非逐个 `req.header()`，确保**覆盖**语义：
/// 当 extra_headers 中包含 User-Agent、Accept 等已由 reqwest Client
/// 默认设置的头时，浏览器的真实值会替代默认值，而不是追加产生重复头。
/// 这是 IDM/NDM 的核心策略——原样复制浏览器的请求头。
///
/// 无效的 header name 或 value 会被静默跳过。
///
/// **Defense-in-depth filtering**: Even though the browser extension already
/// strips dangerous headers on the TypeScript side, we filter them again here
/// at the Rust boundary.  This protects against:
///   - A buggy or outdated extension version that forgets to filter,
///   - Manual API callers that bypass the extension entirely,
///   - Future protocol changes that add new dangerous headers.
///
/// Filtered headers:
///   - `accept-encoding` / `content-encoding` — reqwest has NO gzip/br/deflate
///     Cargo features enabled; forwarding these causes the server to send
///     compressed bytes that are written raw to disk → file corruption.
///   - `transfer-encoding` — hop-by-hop header; must not be forwarded.
///   - `host` — must match the actual request target, not the browser's.
///   - `content-length` — meaningless on a GET; can confuse intermediaries.
///   - `connection` — hop-by-hop header managed by the HTTP stack.
///   - `range` / `if-range` — 分段/续传维度由下载引擎独占管理。浏览器播放
///     媒体时对 `.m4s`/流分段发的 `Range: bytes=<seek偏移>-` 若被透传到整轨
///     或整段 GET，会与引擎自己的 Range 冲突：偏移越界即触发 416 Range Not
///     Satisfiable（B站 DASH 音频轨实测），或悄悄只下回一小片导致文件损坏。
pub(crate) fn apply_extra_headers(
    req: reqwest::RequestBuilder,
    extra_headers: &std::collections::HashMap<String, String>,
) -> reqwest::RequestBuilder {
    if extra_headers.is_empty() {
        return req;
    }

    /// Headers that must never be forwarded from the browser extension.
    /// Compared case-insensitively via `HeaderName` (which lowercases).
    const BLOCKED_HEADERS: &[&str] = &[
        "accept-encoding",
        "content-encoding",
        "transfer-encoding",
        "host",
        "content-length",
        "connection",
        "range",
        "if-range",
    ];

    let mut map = reqwest::header::HeaderMap::with_capacity(extra_headers.len());
    for (name, value) in extra_headers {
        if let (Ok(header_name), Ok(header_value)) = (
            reqwest::header::HeaderName::from_bytes(name.as_bytes()),
            reqwest::header::HeaderValue::from_str(value),
        ) {
            if BLOCKED_HEADERS
                .iter()
                .any(|&blocked| header_name.as_str() == blocked)
            {
                log_info!(
                    "[extra-headers] filtered dangerous header: {}",
                    header_name.as_str()
                );
                continue;
            }
            map.insert(header_name, header_value);
        }
    }
    // req.headers(map) 内部用 insert 逐个替换同名头，
    // 确保浏览器的真实 User-Agent 等值覆盖 build_client 设的默认值。
    req.headers(map)
}

// ---------------------------------------------------------------------------
// RequestSpec: 完整 HTTP 请求事务的内部表示
// ---------------------------------------------------------------------------
//
// 设计动机：FluxDown 早期把每个下载视为"URL → 内容"的简化模型，所有
// HTTP 请求都通过 `client.get(url)` 重发。这个假设在以下场景全部失败：
//   - form POST 触发的下载（uupdump.net）：服务器对 GET 返回 HTML 页面
//   - 一次性签名 URL：被 probe 消费后再请求拿到 403/HTML
//   - 内容协商响应：method/headers 不同 → body 不同
//
// 现在统一为「请求事务 = method + url + headers + cookies + body」，由扩展
// 在 `webRequest.onBeforeRequest` 抓取后透传至 Rust，downloader 用 `build_request`
// 一比一重建浏览器看到的请求。

/// 解码后的请求体——`reqwest::RequestBuilder` 可直接消费的形式。
#[derive(Debug, Clone)]
pub enum RequestBodyDecoded {
    /// 表单字段对——`reqwest::form()` 会编码为 `application/x-www-form-urlencoded`。
    Form(Vec<(String, String)>),
    /// 已经序列化好的 url-encoded 字符串，原样作为 body 发送。
    Urlencoded(String),
    /// 原始字节流。`content_type` 为 `None` 时不主动设置 Content-Type 头。
    Raw {
        bytes: Vec<u8>,
        content_type: Option<String>,
    },
}

/// 完整 HTTP 请求事务规格——`build_request` 的唯一输入来源。
///
/// 字段含义：
///   - `method`：浏览器原始 method；缺省视为 GET
///   - `cookies`：`Cookie:` 头的完整字符串（"k1=v1; k2=v2"）
///   - `referrer`：浏览器原始 Referer
///   - `extra_headers`：扩展捕获的其他请求头（UA/Accept/Sec-Fetch-* 等），
///     由 `apply_extra_headers` 过滤危险头后注入
///   - `body`：仅非 GET 有意义；GET 请求即使携带也会被忽略（见 build_request）
#[derive(Debug, Clone)]
pub struct RequestSpec {
    pub method: reqwest::Method,
    pub cookies: String,
    pub referrer: String,
    pub extra_headers: std::collections::HashMap<String, String>,
    pub body: Option<RequestBodyDecoded>,
}

/// 浏览器扩展/Native Messaging 捕获的原始请求体——引擎侧的传输无关表示。
/// `hub` 侧从 `native_messaging::RequestBody`(wire 格式,字段名受 NM 协议
/// 约束)转换为此类型后再调用 [`RequestSpec::from_captured`],使得
/// `downloader`/`download_manager` 不直接依赖 `native_messaging`。
#[derive(Debug, Clone)]
pub enum CapturedRequestBody {
    FormData {
        fields: std::collections::HashMap<String, Vec<String>>,
    },
    Urlencoded {
        raw: String,
    },
    /// `bytes_b64`：base64 编码的原始字节(XHR/fetch 直接发送 ArrayBuffer 场景)。
    Raw {
        bytes_b64: String,
        content_type: Option<String>,
    },
}

impl RequestSpec {
    /// 默认 GET、无 cookies/headers/body——用于 download_manager 内部的"裸"
    /// HTTP 请求场景(如 BT/HLS 元数据获取,无浏览器会话上下文)。
    pub fn empty_get() -> Self {
        Self {
            method: reqwest::Method::GET,
            cookies: String::new(),
            referrer: String::new(),
            extra_headers: std::collections::HashMap::new(),
            body: None,
        }
    }

    /// GET / HEAD 请求——可以多段下载、可以做 HEAD probe。
    /// 其他 method(POST/PUT/PATCH/DELETE/...)一律强制单流,跳过 HEAD probe。
    pub fn is_get_like(&self) -> bool {
        self.method == reqwest::Method::GET || self.method == reqwest::Method::HEAD
    }

    /// 从浏览器扩展/Native Messaging 捕获的原始字段构造。
    ///
    /// `method` 解析失败(非法字符串)时回退为 GET 并记录日志,确保单一坏请求
    /// 不会让整个下载链路崩溃。
    /// `body` 解码失败(base64 错误等)时回退为 None。
    ///
    /// **OPTIONS 重映射为 GET(纵深防御)**:OPTIONS 是 CORS 预检请求,
    /// 永远不可能是真实的下载事务——扩展若把预检误当下载请求捕获
    /// (旧版本存在此 bug:预检先于真实 GET 发出,而无 body 的 GET 不会
    /// 覆盖缓存记录),原样回放 OPTIONS 会拿到 404/HTML,且非 GET 会被
    /// 强制单流,丢失多线程吞吐。此处统一降级为 GET:预检必无 body,
    /// 降级后与"扩展未捕获到 method"的默认路径完全等价。
    #[allow(clippy::too_many_arguments)]
    pub fn from_captured(
        method: Option<&str>,
        cookies: String,
        referrer: String,
        extra_headers: std::collections::HashMap<String, String>,
        body: Option<CapturedRequestBody>,
    ) -> Self {
        use base64::Engine;

        let method = method
            .and_then(|s| {
                let upper = s.trim().to_ascii_uppercase();
                reqwest::Method::from_bytes(upper.as_bytes()).ok()
            })
            .map(|m| {
                if m == reqwest::Method::OPTIONS {
                    log_info!(
                        "[request-spec] captured method OPTIONS is a CORS preflight, not a real \
                         download transaction — remapping to GET"
                    );
                    reqwest::Method::GET
                } else {
                    m
                }
            })
            .unwrap_or(reqwest::Method::GET);

        let body = body.and_then(|b| match b {
            CapturedRequestBody::FormData { fields } => {
                let mut pairs: Vec<(String, String)> = Vec::new();
                for (k, vs) in fields {
                    for v in vs {
                        pairs.push((k.clone(), v.clone()));
                    }
                }
                Some(RequestBodyDecoded::Form(pairs))
            }
            CapturedRequestBody::Urlencoded { raw } => Some(RequestBodyDecoded::Urlencoded(raw)),
            CapturedRequestBody::Raw {
                bytes_b64,
                content_type,
            } => match base64::engine::general_purpose::STANDARD.decode(&bytes_b64) {
                Ok(bytes) => Some(RequestBodyDecoded::Raw {
                    bytes,
                    content_type,
                }),
                Err(e) => {
                    log_info!("[request-spec] failed to base64-decode raw body: {}", e);
                    None
                }
            },
        });

        Self {
            method,
            cookies,
            referrer,
            extra_headers,
            body,
        }
    }
}

/// Referer 合法性检查：仅接受非空且以 `http://` / `https://` 开头的值。
///
/// 浏览器（downloads API / fetch 规范）在 JS 触发下载等场景会给出
/// `about:client` 之类的占位符 referrer，这不是真实来源页 URL；
/// 将其原样发给服务器会被部分 CDN 的防盗链判为非法请求（HTTP 403）。
///
/// # Examples
///
/// ```ignore
/// use nx_dl_engine::downloader::is_valid_referrer;
///
/// assert!(is_valid_referrer("https://example.com/page"));
/// assert!(is_valid_referrer("http://example.com"));
/// assert!(!is_valid_referrer("about:client"));
/// assert!(!is_valid_referrer(""));
/// ```
pub fn is_valid_referrer(referrer: &str) -> bool {
    let r = referrer.trim();
    ["http://", "https://"]
        .iter()
        .any(|scheme| r.len() > scheme.len() && r[..scheme.len()].eq_ignore_ascii_case(scheme))
}

/// 统一请求构建入口——所有发出 HTTP 请求的地方都应通过此函数。
///
/// 此函数替代了散落在 downloader / segment_coordinator / hls / dash 等
/// 模块中的 `client.get(url) + apply_extra_headers(...)` 模式。
///
/// 参数 `method` 允许覆盖 `spec.method`——主要用于 probe 阶段（HEAD probe
/// 总是发送 HEAD，与 spec 自身的 method 无关）。下载阶段通常传 `spec.method.clone()`。
///
/// **请求体语义**：
///   - GET / HEAD：即使 `spec.body` 非空也不会被附加（HTTP 标准上 GET/HEAD 不应携带 body）
///   - 其他 method：按 `RequestBodyDecoded` 类型重建请求体
pub fn build_request(
    client: &Client,
    url: &str,
    method: reqwest::Method,
    spec: &RequestSpec,
) -> reqwest::RequestBuilder {
    let attaches_body = method != reqwest::Method::GET && method != reqwest::Method::HEAD;
    let mut req = client.request(method, url);

    if !spec.cookies.is_empty() {
        req = req.header("Cookie", &spec.cookies);
    }
    // Referer 只接受真实的 http(s) URL。浏览器扩展捕获的 referrer 可能是
    // fetch 规范的占位符 "about:client"（JS 触发的下载没有真实来源页），
    // 部分 CDN（如 hembed）对非法 Referer 值直接回 403 —— 这类伪值等同于无。
    if is_valid_referrer(&spec.referrer) {
        req = req.header(reqwest::header::REFERER, &spec.referrer);
    }
    req = apply_extra_headers(req, &spec.extra_headers);

    if attaches_body && let Some(body) = &spec.body {
        match body {
            RequestBodyDecoded::Form(pairs) => {
                req = req.form(pairs);
            }
            RequestBodyDecoded::Urlencoded(raw) => {
                req = req
                    .header(
                        reqwest::header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .body(raw.clone());
            }
            RequestBodyDecoded::Raw {
                bytes,
                content_type,
            } => {
                if let Some(ct) = content_type {
                    req = req.header(reqwest::header::CONTENT_TYPE, ct);
                }
                req = req.body(bytes.clone());
            }
        }
    }

    req
}

/// Content-Encoding types that the server may apply to response bodies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentEncoding {
    Gzip,
    Brotli,
    Deflate,
    Zstd,
}

/// Detect the `Content-Encoding` from response headers.
///
/// Returns `Some(encoding)` when the server applied compression (gzip, br,
/// deflate, zstd).  Returns `None` when the header is absent, empty, or
/// `identity` (i.e. the body is uncompressed).
///
/// Unknown encodings are mapped to `None` — callers that need strict
/// validation should check the raw header separately.
pub fn detect_content_encoding(headers: &reqwest::header::HeaderMap) -> Option<ContentEncoding> {
    let ce = headers.get(reqwest::header::CONTENT_ENCODING)?;
    let value = ce.to_str().unwrap_or("");
    // HTTP allows comma-separated encodings (e.g. "gzip, identity").
    // Take the first non-identity encoding as the dominant one.
    for part in value.split(',') {
        let lower = part.trim().to_ascii_lowercase();
        match lower.as_str() {
            "gzip" | "x-gzip" => return Some(ContentEncoding::Gzip),
            "br" | "brotli" => return Some(ContentEncoding::Brotli),
            "deflate" => return Some(ContentEncoding::Deflate),
            "zstd" => return Some(ContentEncoding::Zstd),
            _ => continue, // "identity", "", "compress", unknown
        }
    }
    None
}

/// 检测响应是否带有【存在但本引擎无法解码】的 Content-Encoding（如 `compress`）。
///
/// `detect_content_encoding` 把未知编码映射为 `None`，调用方据此当作 identity 原样
/// 写盘——但若服务器实际做了我们不认识的压缩，原始压缩字节落盘即文件损坏
/// （BUG-HTTP-UNKNOWN-ENCODING-RAW）。本函数在存在非 identity、且不属于受支持集合
/// 的编码时返回该编码名，调用方应据此报错而非静默写出损坏内容。
pub fn unsupported_content_encoding(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let ce = headers.get(reqwest::header::CONTENT_ENCODING)?;
    let value = ce.to_str().ok()?;
    // 收集所有【非 identity】编码层。解压管线（maybe_decompress_stream）只能反转
    // 单一层，因此"无法完整还原"的情形有二：
    //   (1) 存在任何【未知】编码（如 compress）；
    //   (2) 存在【多于一层】非 identity 编码（如 gzip, gzip / gzip, compress）——
    //       即便每层都受支持，也只反转得了第一层，残留层落盘即损坏。
    let mut layers: Vec<String> = Vec::new();
    let mut has_unknown = false;
    for part in value.split(',') {
        let lower = part.trim().to_ascii_lowercase();
        match lower.as_str() {
            // "none" 不是 IANA 登记的编码 token，但部分服务器/反代用它显式
            // 表达"未压缩"（等价于省略该头或写 identity）。按未知编码处理会把
            // 明确声明"无压缩"的响应误判为不可解码的压缩层，导致下载被永久拒绝
            // （BUG-HTTP-NONE-ENCODING-FALSE-POSITIVE）。
            "identity" | "none" | "" => {}
            "gzip" | "x-gzip" | "br" | "brotli" | "deflate" | "zstd" => layers.push(lower),
            other => {
                has_unknown = true;
                layers.push(other.to_string());
            }
        }
    }
    if has_unknown || layers.len() > 1 {
        Some(layers.join(", "))
    } else {
        None
    }
}

/// 大小写不敏感地剥离 `Content-Range` 值的 `bytes ` 单位前缀。
///
/// RFC 9110 §14.1 规定 range-unit 比较【不区分大小写】——个别服务器/代理会发
/// `Bytes 0-1/100`。前 6 字节 ASCII 相等才剥离，故返回的切片起点必在字符边界上。
fn strip_bytes_unit_prefix(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    let prefix = trimmed.as_bytes().get(..6)?;
    if !prefix.eq_ignore_ascii_case(b"bytes ") {
        return None;
    }
    Some(&trimmed[6..])
}

/// 从 `Content-Range` 响应头解析【起始字节】。
///
/// `Content-Range` 形如 `bytes <start>-<end>/<total>`（RFC 9110 §14.4）。多段下载
/// 据此校验"服务器返回的区间起点是否等于我们请求的 Range 起点"——劣质 CDN 在链接
/// 失效时会对 `Range: bytes=X-Y` 回 206 却发【从 0 的全量流】，其 `Content-Range`
/// 起点为 0（或整体缺失），与请求偏移不符。
///
/// 以下情形一律返回 `None`（交由 [`is_range_response_misaligned`] 按"起点未知"裁决）：
///   - 头缺失或非 ASCII；
///   - 值不以 `bytes ` 前缀开头（大小写不敏感，见 [`strip_bytes_unit_prefix`]）；
///   - unsatisfied-range 形式 `bytes */<total>`（`*` 非法数字）；
///   - `<start>` 解析失败。
pub(crate) fn parse_content_range_start(headers: &reqwest::header::HeaderMap) -> Option<i64> {
    let raw = headers.get("content-range")?.to_str().ok()?;
    // "bytes 100-199/1234" → 去前缀 → "100-199/1234"
    let rest = strip_bytes_unit_prefix(raw)?;
    // 取 '/' 前的区间部分："100-199"（unsatisfied 时为 "*"）
    let range_part = rest.split('/').next()?;
    // 取 '-' 前的起点："100"（unsatisfied 时为 "*"，parse 失败 → None）
    let start_str = range_part.split('-').next()?;
    start_str.trim().parse::<i64>().ok()
}

/// 从 `Content-Range` 响应头解析【文件总大小】（斜杠后的分母）。
///
/// `Content-Range` 形如 `bytes <start>-<end>/<total>`（RFC 9110 §14.4）。多段下载据此
/// 发现服务器【自报的真实总大小】——当它明显大于当前规划的总大小（如浏览器扩展给的
/// hint 偏小、或文件仍在上传中增长）时，规划区间 `[0, planned)` 只覆盖了文件前缀，继续
/// 下去会静默截断。coordinator 据此【就地扩容】（追加尾段）下满整文件。
///
/// 以下情形返回 `None`（总大小未知，调用方【不据此扩容】，避免误判）：
///   - 头缺失或非 ASCII；
///   - 值不以 `bytes ` 前缀开头（大小写不敏感，见 [`strip_bytes_unit_prefix`]）；
///   - `<total>` 为 `*`（unsatisfied/未知）或整体缺失/解析失败。
pub(crate) fn parse_content_range_total(headers: &reqwest::header::HeaderMap) -> Option<i64> {
    let raw = headers.get("content-range")?.to_str().ok()?;
    // "bytes 100-199/1234" → 去前缀 → "100-199/1234" → 取 '/' 后 → "1234"
    let rest = strip_bytes_unit_prefix(raw)?;
    let total_str = rest.split('/').nth(1)?;
    total_str.trim().parse::<i64>().ok()
}

/// 判定一个 206 响应的 `Content-Range` 起点（由 [`parse_content_range_start`] 解析）
/// 是否与本段请求的 Range 起点 `actual_start` 【错位】。
///
/// - `Some(s)`：服务器明确回了起点 `s` → 错位当且仅当 `s != actual_start`。
/// - `None`（Content-Range 缺失/不可解析）：
///     - `actual_start == 0`：本就要从 0 写，即便服务器发全量流也落在正确位置，
///       不算错位（段 #0 与从 0 起的续传对此免疫）→ `false`；
///     - `actual_start > 0`：请求文件中段却拿不到 Content-Range 佐证，无法确认服务器
///       是否从 0 全量发送 → 保守判定错位 → `true`（回退单流，牺牲多段并行换正确性）。
///
/// 注：合法 206 响应【必须】携带 Content-Range（RFC 9110 §15.3.7），故对合规服务器
/// 此函数在正常 Range 下恒返回 `false`，不影响多段吞吐；只有真正错位或破损的响应
/// 才触发回退。
pub(crate) fn is_range_response_misaligned(cr_start: Option<i64>, actual_start: i64) -> bool {
    match cr_start {
        Some(s) => s != actual_start,
        None => actual_start > 0,
    }
}

/// Wrap a response byte stream with transparent decompression if the server
/// returned a compressed `Content-Encoding`.  For `identity` or missing
/// encoding, returns the original stream unchanged.
///
/// This is the core fix for file corruption: instead of writing raw gzip
/// bytes to disk, we decompress on-the-fly and write the original file
/// content.
///
/// The output stream uses `std::io::Error` because `reqwest::Error` is opaque
/// and cannot be constructed from an `io::Error`.  Callers should convert via
/// `DownloadError::Io` when consuming chunks.
pub fn maybe_decompress_stream(
    stream: impl futures_util::Stream<Item = Result<bytes::Bytes, reqwest::Error>>
    + Unpin
    + Send
    + 'static,
    encoding: Option<ContentEncoding>,
) -> Box<dyn futures_util::Stream<Item = Result<bytes::Bytes, std::io::Error>> + Unpin + Send> {
    // Map the incoming reqwest::Error stream to io::Error so every branch
    // has a uniform error type.
    let io_stream = stream.map(|result| result.map_err(std::io::Error::other));

    let Some(enc) = encoding else {
        return Box::new(io_stream);
    };

    let reader = tokio_util::io::StreamReader::new(io_stream);

    // Wrap with the appropriate decompressor and convert back to a stream.
    match enc {
        ContentEncoding::Gzip => {
            let decoder = async_compression::tokio::bufread::GzipDecoder::new(reader);
            Box::new(tokio_util::io::ReaderStream::new(decoder))
        }
        ContentEncoding::Brotli => {
            let decoder = async_compression::tokio::bufread::BrotliDecoder::new(reader);
            Box::new(tokio_util::io::ReaderStream::new(decoder))
        }
        ContentEncoding::Deflate => {
            let decoder = async_compression::tokio::bufread::DeflateDecoder::new(reader);
            Box::new(tokio_util::io::ReaderStream::new(decoder))
        }
        ContentEncoding::Zstd => {
            let decoder = async_compression::tokio::bufread::ZstdDecoder::new(reader);
            Box::new(tokio_util::io::ReaderStream::new(decoder))
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP client builder (shared config)
// ---------------------------------------------------------------------------

/// Default User-Agent for HTTP requests.
///
/// Uses a neutral download-manager identifier instead of a browser UA.
///
/// **Why not Chrome UA?**  Cloudflare's Bot Management compares the TLS
/// fingerprint (JA3/JA4) against the declared User-Agent.  rustls produces a
/// JA3 fingerprint that does not match Chrome's.  When a non-browser TLS
/// fingerprint is paired with a Chrome UA, Cloudflare flags the request as
/// bot traffic and returns 403/404 — this breaks downloads from any CDN
/// behind Cloudflare (e.g. JetBrains' `download-cdn.clf.jetbrains.com.cn`).
///
/// When the browser extension captures a download it passes the real browser
/// UA via `extra_headers`.  That UA is applied on the first attempt; if the
/// server returns 4xx we automatically retry *without* the browser UA so that
/// Cloudflare-protected CDNs also work (see [`resolve_file_info`]).
///
/// **Version rule（同 aria2 的 `aria2/<版本>`）**：NexBox 移植版固定
/// `NexBox-DL/1.0`（上游经 build.rs 注入 `FLUXDOWN_APP_VERSION`，本仓无
/// build 脚本，改为常量；语义不变——中性下载器标识）。
const DEFAULT_UA: &str = "NexBox-DL/1.0";

/// Build a properly configured HTTP client with strict TLS certificate validation.
///
/// When `proxy_config` specifies a proxy, it is injected into the client builder.
/// - `ProxyMode::None`   → explicit `no_proxy()` to disable env-var proxies
/// - `ProxyMode::System`  → auto-detect from Windows registry / environment
/// - `ProxyMode::Manual`  → user-specified proxy URL (HTTP/HTTPS/SOCKS4/SOCKS5)
///
/// When `user_agent` is non-empty, it overrides the built-in Chrome UA.
pub fn build_client(
    proxy_config: &crate::proxy_config::ProxyConfig,
    user_agent: &str,
) -> Result<Client, DownloadError> {
    build_client_with_tls_policy(proxy_config, user_agent, false)
}

/// 构建【钉定 IP】的下载 client：与 [`build_client_with_tls_policy`] 完全同参
/// 装配（代理/UA/TLS 语义逐项继承），仅额外把 `host` 的 DNS 解析钉定到
/// `ip`（reqwest `.resolve()`）。SNI 与 Host 头保持域名不变，TLS 证书照常
/// 严格校验——伪造节点拿不到该域名的有效证书，这是多节点完整性的锚。
/// 重定向到【其他 host】时钉定不生效（按系统 DNS 解析），行为与现状一致。
///
/// `SocketAddr` 的端口传 0：DNS 覆盖没有端口概念，实际端口取自请求 URL
/// （reqwest 文档明确忽略此处端口）。
pub fn build_pinned_client(
    proxy_config: &crate::proxy_config::ProxyConfig,
    user_agent: &str,
    ignore_tls_errors: bool,
    host: &str,
    ip: std::net::IpAddr,
) -> Result<Client, DownloadError> {
    build_client_inner(
        proxy_config,
        user_agent,
        ignore_tls_errors,
        Some((host, ip)),
    )
}

/// Build a download client with an explicit per-task TLS certificate policy.
///
/// `ignore_tls_errors` must only come from an explicit user choice for the
/// current task. The secure default is enforced by [`build_client`].
pub fn build_client_with_tls_policy(
    proxy_config: &crate::proxy_config::ProxyConfig,
    user_agent: &str,
    ignore_tls_errors: bool,
) -> Result<Client, DownloadError> {
    build_client_inner(proxy_config, user_agent, ignore_tls_errors, None)
}

/// 共享装配核心：[`build_client_with_tls_policy`] 与 [`build_pinned_client`]
/// 的唯一实现体。`pin = Some((host, ip))` 时追加 `.resolve()` DNS 钉定，
/// 其余配置两者逐字节相同（代理/UA/TLS/池参数绝不允许分叉）。
fn build_client_inner(
    proxy_config: &crate::proxy_config::ProxyConfig,
    user_agent: &str,
    ignore_tls_errors: bool,
    pin: Option<(&str, std::net::IpAddr)>,
) -> Result<Client, DownloadError> {
    use crate::proxy_config::{ProxyMode, detect_system_proxy};

    let ua = if user_agent.is_empty() {
        DEFAULT_UA
    } else {
        user_agent
    };
    let mut builder = Client::builder()
        .user_agent(ua)
        // TLS defaults to strict verification. Only a task whose confirmation
        // dialog explicitly enabled the insecure option reaches `true` here.
        // This accepts expired/self-signed/hostname-mismatched certificates and
        // also permits undetectable HTTPS interception by a MITM proxy.
        .danger_accept_invalid_certs(ignore_tls_errors)
        // HTTP version — force HTTP/1.1 for download manager use cases:
        //  1. Range requests are reliable and well-tested on HTTP/1.1.
        //  2. Multi-segment downloads use separate TCP connections; HTTP/2
        //     multiplexing would force all segments onto one connection.
        //  3. Some servers advertise h2 via ALPN but have buggy HTTP/2
        //     implementations that close connections mid-response.
        .http1_only()
        // TCP tuning — disable Nagle's algorithm to eliminate up to 200 ms
        // latency on small writes (Range request headers, TLS handshake
        // messages).  All high-performance download managers (IDM, aria2)
        // set this.  Safe for bulk transfers because BufWriter already
        // coalesces writes into 256 KB chunks before hitting the socket.
        .tcp_nodelay(true)
        // TCP Keep-Alive — 60s 间隔比系统默认（通常 >2min）更激进，
        // 确保 NAT/防火墙不会因空闲超时而断开长时间下载的连接。
        // reqwest 底层设置 TCP_KEEPIDLE=60s（首次探测前等待时间）。
        .tcp_keepalive(Duration::from_secs(60))
        // Redirects — follow up to 30 hops like Chrome
        .redirect(reqwest::redirect::Policy::limited(30))
        // Timeouts — 15 s is sufficient for initial TCP+TLS handshake;
        // the stall detector handles mid-transfer
        // hangs separately.  Shorter timeout lets failed segments retry
        // faster instead of blocking a worker for 30 s.
        .connect_timeout(Duration::from_secs(15))
        // No global timeout — downloads can be very long
        // Connection pool — keep enough idle connections to cover all
        // segments of a multi-segment download so workers reuse warm
        // keep-alive connections instead of paying TCP+TLS re-handshake
        // costs when finishing one segment and starting the next.
        // 64 == MAX_SEGMENTS (segment_advisor caps io_cap at cpu_cores*4,
        // which reaches 64 on 16+ logical-core machines downloading large
        // files).  The previous value of 16 (sized for a 4-core machine)
        // starved the idle pool on many-core hosts, forcing re-handshakes
        // for every segment beyond the 16th.  90 s idle timeout reclaims
        // the extra connections shortly after the download finishes.
        .pool_idle_timeout(Duration::from_secs(90))
        .pool_max_idle_per_host(64)
        // Cookies — needed for session-based downloads (Google Drive, etc.).
        // reqwest follows RFC 6265: cookies are scoped to their domain.
        .cookie_store(true)
        // Do NOT enable auto-decompression (.gzip/.brotli/.deflate).
        // A download manager must receive raw bytes so that:
        //  1. Content-Length matches the actual bytes written to disk.
        //  2. Range-based multi-segment downloads use correct byte offsets.
        //  3. The integrity check (file size vs Content-Length) works reliably.
        //
        // The gzip/brotli/deflate Cargo features are intentionally NOT enabled
        // to keep the binary small and avoid accidental decompression.
        // We explicitly set `Accept-Encoding: identity` so the server never
        // sends compressed content and Content-Length always equals raw bytes.
        .default_headers({
            let mut h = reqwest::header::HeaderMap::new();
            h.insert(
                reqwest::header::ACCEPT_ENCODING,
                HeaderValue::from_static("identity"),
            );
            h
        });

    // --- Proxy injection ---
    match proxy_config.mode {
        ProxyMode::None => {
            // Explicitly disable proxy so env vars (HTTP_PROXY etc.) are ignored.
            builder = builder.no_proxy();
        }
        ProxyMode::System => {
            // Read Windows registry / env vars for system proxy.
            match detect_system_proxy() {
                Ok(Some(sys_proxy)) => {
                    if let Some(url) = sys_proxy.to_proxy_url() {
                        log_info!(
                            "[build_client] system proxy detected (url redacted for security)"
                        );
                        match reqwest::Proxy::all(&url) {
                            Ok(mut proxy) => {
                                if !sys_proxy.username.is_empty() {
                                    proxy =
                                        proxy.basic_auth(&sys_proxy.username, &sys_proxy.password);
                                }
                                if !sys_proxy.no_proxy_list.is_empty() {
                                    proxy = proxy.no_proxy(reqwest::NoProxy::from_string(
                                        &sys_proxy.no_proxy_list,
                                    ));
                                }
                                builder = builder.proxy(proxy);
                            }
                            Err(e) => {
                                log_info!("[build_client] failed to parse system proxy URL: {}", e);
                            }
                        }
                    } else {
                        log_info!("[build_client] system proxy enabled but no URL resolved");
                    }
                }
                Ok(None) => {
                    log_info!("[build_client] system proxy: not configured");
                }
                Err(e) => {
                    log_info!("[build_client] system proxy detection error: {}", e);
                }
            }
        }
        ProxyMode::Manual => {
            if let Some(url) = proxy_config.to_proxy_url() {
                log_info!("[build_client] manual proxy configured");
                match reqwest::Proxy::all(&url) {
                    Ok(mut proxy) => {
                        if !proxy_config.username.is_empty() {
                            proxy =
                                proxy.basic_auth(&proxy_config.username, &proxy_config.password);
                        }
                        if !proxy_config.no_proxy_list.is_empty() {
                            proxy = proxy.no_proxy(reqwest::NoProxy::from_string(
                                &proxy_config.no_proxy_list,
                            ));
                        }
                        builder = builder.proxy(proxy);
                    }
                    Err(e) => {
                        log_info!("[build_client] failed to create proxy from URL: {}", e);
                    }
                }
            } else {
                log_info!("[build_client] manual proxy: incomplete config, using direct");
                builder = builder.no_proxy();
            }
        }
        ProxyMode::Auto => {
            // Auto 的全局/兜底 client 恒为直连：具体代理只会由 auto_proxy
            // 决策路径以 Manual 配置显式构建（见 crate::auto_proxy 模块文档）。
            builder = builder.no_proxy();
        }
    }

    // --- DNS 钉定（多 CDN 节点池的 pinned client）---
    if let Some((host, ip)) = pin {
        builder = builder.resolve(host, std::net::SocketAddr::new(ip, 0));
    }

    let client = builder.build()?;
    Ok(client)
}

// ---------------------------------------------------------------------------
// Resolve file info (HEAD probe → GET fallback)
// ---------------------------------------------------------------------------

/// Timeout for the probe requests (HEAD / GET Range:0-0).
/// 15 seconds is sufficient for most servers; the retry mechanism handles
/// transient failures without making users wait excessively.
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

/// Maximum retries for the probe phase (HEAD + GET).
///
/// 3 attempts total:
///   1. Original headers (incl. browser UA from extension, if any)
///   2. Normal retry (same headers, covers DNS/TLS cold-start)
///   3. **UA-downgrade retry** — strips browser UA from extra_headers so that
///      the request uses the neutral `DEFAULT_UA`.  This handles Cloudflare
///      Bot Management which rejects requests where the TLS fingerprint
///      (rustls ≠ Chrome) contradicts a Chrome User-Agent header.
const PROBE_MAX_RETRIES: u32 = 3;

/// Base delay for probe retries (used with exponential backoff).
const PROBE_RETRY_BASE_DELAY: Duration = Duration::from_secs(1);

/// Resolve file info with automatic retry on transient failures.
///
/// On Windows, the very first HTTPS request from a new process can fail due to
/// DNS resolver cold-start, rustls TLS session initialisation, or firewall
/// first-connection inspection.  Retrying transparently hides this from users.
pub async fn resolve_file_info(
    client: &Client,
    url: &str,
    spec: &RequestSpec,
) -> Result<FileInfo, DownloadError> {
    // Prepare a fallback spec that strips browser-like User-Agent.
    // On the last attempt we use this to avoid Cloudflare JA3-vs-UA mismatch.
    let headers_without_browser_ua: std::collections::HashMap<String, String> = spec
        .extra_headers
        .iter()
        .filter(|(k, _)| !k.eq_ignore_ascii_case("user-agent"))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let has_browser_ua = spec
        .extra_headers
        .keys()
        .any(|k| k.eq_ignore_ascii_case("user-agent"));

    // Holder for the UA-downgraded variant; allocated once outside the loop so
    // we can borrow it without repeated cloning.
    let downgraded_spec = RequestSpec {
        method: spec.method.clone(),
        cookies: spec.cookies.clone(),
        referrer: spec.referrer.clone(),
        extra_headers: headers_without_browser_ua,
        body: spec.body.clone(),
    };

    let mut last_err = None;
    for attempt in 0..PROBE_MAX_RETRIES {
        // Last attempt: if extra_headers carried a browser UA, drop it so
        // the request falls back to DEFAULT_UA ("FluxDown/<version>").  This
        // avoids Cloudflare's TLS-fingerprint-vs-UA bot detection.
        let use_downgraded_ua = has_browser_ua && attempt + 1 == PROBE_MAX_RETRIES;
        let attempt_spec = if use_downgraded_ua {
            if attempt == 0 {
                // Should not happen with PROBE_MAX_RETRIES >= 2, but guard anyway.
                spec
            } else {
                log_info!(
                    "[resolve] retry {}/{}: stripping browser UA to avoid bot detection",
                    attempt + 1,
                    PROBE_MAX_RETRIES
                );
                &downgraded_spec
            }
        } else {
            spec
        };

        match resolve_file_info_once(client, url, attempt_spec).await {
            Ok(info) => return Ok(info),
            Err(e) => {
                log_info!(
                    "[resolve] probe attempt {}/{} failed: {}",
                    attempt + 1,
                    PROBE_MAX_RETRIES,
                    e
                );
                last_err = Some(e);
                if attempt + 1 < PROBE_MAX_RETRIES {
                    let delay = PROBE_RETRY_BASE_DELAY * 2u32.saturating_pow(attempt);
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }
    Err(last_err.unwrap_or_else(|| DownloadError::Other("probe failed after retries".to_string())))
}

/// Walk the std::error::Error source chain and return a " → cause1 → cause2" suffix string.
/// Returns an empty string when there is no source, so it can be appended directly to a message.
fn format_error_chain(mut src: Option<&dyn StdError>) -> String {
    let mut s = String::new();
    while let Some(cause) = src {
        s.push_str(" → ");
        s.push_str(&cause.to_string());
        src = cause.source();
    }
    s
}

async fn resolve_file_info_once(
    client: &Client,
    url: &str,
    spec: &RequestSpec,
) -> Result<FileInfo, DownloadError> {
    // 非 GET（form POST 等）：HEAD 通常返回 405 Method Not Allowed，POST + Range
    // 在 HTTP 标准上未定义。改为只发一次原始 method+body 请求，从响应头读取
    // 文件元数据后立即终止读取（drop response）。
    if !spec.is_get_like() {
        return resolve_file_info_non_get(client, url, spec).await;
    }

    let cookies = spec.cookies.as_str();
    // --- Concurrent HEAD + GET probe ----------------------------------------
    // Fire both HEAD and GET Range:0-0 in parallel.  HEAD is faster when it
    // works, but many servers/CDNs omit Content-Disposition on HEAD.  By
    // running both concurrently we avoid the serial HEAD→GET penalty.
    //
    // IMPORTANT for Content-Encoding handling:
    // Many CDNs (Cloudflare, Akamai) add Content-Encoding: gzip to HEAD and
    // full-GET responses but **omit** it from 206 Partial Content responses.
    // This is correct per HTTP semantics: Range requests operate on the
    // *original* (identity) representation, not the compressed one.
    //
    // We therefore check Content-Encoding on the GET Range:0-0 response
    // **separately** from the merged headers.  If GET returned 206 without
    // Content-Encoding, Range requests are safe for multi-segment downloads
    // even when HEAD advertised compression.

    let head_fut = build_request(client, url, reqwest::Method::HEAD, spec)
        .timeout(PROBE_TIMEOUT)
        .send();

    let get_fut = build_request(client, url, reqwest::Method::GET, spec)
        .header("Range", "bytes=0-0")
        .timeout(PROBE_TIMEOUT)
        .send();

    let (head_result, get_result) = tokio::join!(head_fut, get_fut);

    // Extract HEAD response (if successful)
    let mut head_status_desc = String::new();
    let head_data = match head_result {
        Ok(r) if r.status().is_success() => {
            let u = r.url().clone();
            let h = r.headers().clone();
            Some((h, u))
        }
        Ok(r) => {
            head_status_desc = r.status().as_u16().to_string();
            log_info!(
                "[resolve] HEAD failed: status={}, url={}, cookies_len={}",
                r.status(),
                r.url(),
                cookies.len()
            );
            None
        }
        Err(e) => {
            head_status_desc = format!("network-error: {}", e);
            log_info!(
                "[resolve] HEAD network error: {}{}, cookies_len={}",
                e,
                format_error_chain(e.source()),
                cookies.len()
            );
            None
        }
    };

    // Extract GET response (if successful)
    let mut get_status_desc = String::new();
    let get_data = match get_result {
        Ok(r) if r.status().is_success() => {
            let u = r.url().clone();
            let h = r.headers().clone();
            let got_206 = r.status() == reqwest::StatusCode::PARTIAL_CONTENT;
            // Check Content-Encoding on the GET Range:0-0 response BEFORE
            // merging with HEAD.  This tells us whether Range responses
            // carry compression — the key signal for multi-segment safety.
            let get_range_compressed = got_206 && detect_content_encoding(&h).is_some();
            drop(r); // release connection immediately
            Some((h, u, got_206, get_range_compressed))
        }
        Ok(r) => {
            get_status_desc = r.status().as_u16().to_string();
            log_info!(
                "[resolve] GET failed: status={}, url={}, cookies_len={}",
                r.status(),
                r.url(),
                cookies.len()
            );
            None
        }
        Err(e) => {
            get_status_desc = format!("network-error: {}", e);
            log_info!(
                "[resolve] GET network error: {}{}, cookies_len={}",
                e,
                format_error_chain(e.source()),
                cookies.len()
            );
            None
        }
    };

    // Track whether the GET Range:0-0 response itself carried compression.
    // false = either GET didn't succeed, returned 200 (not 206), or returned
    //         206 without Content-Encoding → Range requests are safe.
    // true  = GET returned 206 WITH Content-Encoding → rare but must disable
    //         multi-segment to avoid corrupt byte-range splicing.
    let range_response_compressed = get_data
        .as_ref()
        .is_some_and(|(_, _, _, compressed)| *compressed);

    // Merge results: HEAD as base, GET to fill in missing data.
    let (mut headers, mut final_url) = match (&head_data, &get_data) {
        (Some((hh, hu)), _) => (hh.clone(), hu.clone()),
        (None, Some((gh, gu, _, _))) => (gh.clone(), gu.clone()),
        (None, None) => {
            // 双探测（HEAD + Range GET）均失败。部分合法服务器（如飞牛 OS
            // multiple-download 端点）对下载 token 有并发/次数配额：HEAD 恒
            // 405，带 Range 的 GET 恒 400（配额已耗尽/不支持 Range），但一次
            // 【无 Range 的普通 GET】能正常 200。在判死这次探测前，再试一次
            // 普通 GET 作为最后的降级路径，避免把可下载的任务误判为失败。
            return resolve_file_info_plain_get_fallback(
                client,
                url,
                spec,
                &head_status_desc,
                &get_status_desc,
            )
            .await;
        }
    };

    // If HEAD succeeded but lacks Content-Disposition, merge from GET.
    if head_data.is_some()
        && let Some((get_headers, get_url, got_206, _)) = &get_data
    {
        if !headers.contains_key(reqwest::header::CONTENT_DISPOSITION)
            && let Some(cd) = get_headers.get(reqwest::header::CONTENT_DISPOSITION)
        {
            headers.insert(reqwest::header::CONTENT_DISPOSITION, cd.clone());
        }
        if let Some(ct) = get_headers.get(reqwest::header::CONTENT_TYPE) {
            headers.insert(reqwest::header::CONTENT_TYPE, ct.clone());
        }
        // Prefer GET's final URL (may differ after redirect)
        final_url = get_url.clone();
        // If GET gave us 206, copy Content-Range for accurate file size
        if *got_206 && let Some(cr) = get_headers.get("content-range") {
            headers.insert(
                reqwest::header::HeaderName::from_static("content-range"),
                cr.clone(),
            );
        }
    }

    // --- Phase 3: Parse metadata from merged headers ------------------------
    // A 206 response from GET proves range support even without Accept-Ranges header.
    let got_206_from_get = get_data.as_ref().is_some_and(|(_, _, got, _)| *got);
    let mut supports_range = got_206_from_get
        || headers
            .get(reqwest::header::ACCEPT_RANGES)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v != "none");

    let total_bytes = if let Some(cr) = headers.get("content-range") {
        // e.g. "bytes 0-0/12345"
        cr.to_str()
            .ok()
            .and_then(|v| v.rsplit('/').next())
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0)
    } else if got_206_from_get {
        // F021: GET Range:0-0 返回 206 却缺失 Content-Range 头（违反 RFC 9110
        // 但现实存在的破损服务器/中间件）。此时 Content-Length=1 是【范围长度】
        // （0-0 这一个字节），不是文件总大小，绝不能拿来当 total_bytes，否则会
        // 被当成 1 字节文件处理并几乎必然触发后续 size mismatch。改为置 0
        // （未知大小），走下游 unknown-size 单流路径（读到 EOF、跳过 size 校验），
        // 语义正确且不会误判。
        log_info!(
            "[resolve] WARNING: GET returned 206 without Content-Range — Content-Length is \
             the range length (not file size); treating total_bytes as unknown (0)"
        );
        0
    } else {
        headers
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0)
    };

    let content_type = headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let file_name = extract_filename(&headers, url, final_url.as_str());
    log_info!(
        "[resolve] url={} → name={}, size={}, range={}, ct={}",
        url,
        file_name,
        total_bytes,
        supports_range,
        content_type
    );

    let etag = headers
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let last_modified = headers
        .get(reqwest::header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    // --- Content-Encoding handling -------------------------------------------
    //
    // The merged `headers` may carry Content-Encoding from the HEAD response.
    // However, this does NOT mean Range responses are also compressed.
    //
    // HTTP semantics (RFC 9110 §8.8.3): Range requests operate on the
    // "selected representation" which is typically the **identity** encoding.
    // Most CDNs (Cloudflare, Akamai, AWS CloudFront) correctly:
    //   - HEAD / full GET → Content-Encoding: gzip (if Accept-Encoding allows)
    //   - GET Range:bytes=X-Y → 206 with NO Content-Encoding (raw bytes)
    //
    // We use the GET Range:0-0 probe result (`range_response_compressed`) as
    // the authoritative signal for multi-segment safety:
    //
    //   GET 206 WITHOUT Content-Encoding → Range returns raw bytes → safe
    //   GET 206 WITH    Content-Encoding → rare; server compresses Range
    //                                      responses too → NOT safe
    //   Only HEAD available (GET failed)  → conservative; use HEAD's signal
    //
    // When Range responses ARE compressed, we disable multi-segment and let
    // `download_single` decompress the full-GET stream on-the-fly.
    //
    // When Range responses are NOT compressed (the common case), multi-segment
    // can proceed normally even if HEAD showed Content-Encoding.

    // Did *any* probe response (HEAD or GET) indicate compression?
    let content_encoding_compressed = detect_content_encoding(&headers).is_some();

    // Should we disable Range support due to compression?
    // Only if the GET Range:0-0 *itself* returned compressed content,
    // OR if we have no GET data and must rely on HEAD alone.
    let got_get_206 = get_data.as_ref().is_some_and(|(_, _, got, _)| *got);
    let disable_range_for_compression = if got_get_206 {
        // We have a 206 response — use its Content-Encoding as ground truth.
        range_response_compressed
    } else {
        // No 206 available (GET failed or returned 200) — fall back to the
        // merged headers (conservative: if HEAD says compressed, disable).
        content_encoding_compressed
    };

    if disable_range_for_compression {
        log_info!(
            "[resolve] WARNING: Range response itself carries Content-Encoding: {:?} — \
             byte ranges are invalid on compressed streams; disabling multi-segment",
            headers
                .get(reqwest::header::CONTENT_ENCODING)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("?")
        );
        supports_range = false;
    } else if content_encoding_compressed {
        // HEAD indicated compression but the GET 206 did NOT — Range requests
        // return raw (identity) bytes.  Multi-segment is safe.  The HEAD's
        // Content-Length may be the compressed size though — if we got a
        // Content-Range from the 206, that already gave us the real file size.
        log_info!(
            "[resolve] HEAD indicated Content-Encoding: {:?} but GET Range:0-0 \
             returned 206 without compression — Range requests use identity \
             encoding; multi-segment is safe (total_bytes={}, range={})",
            headers
                .get(reqwest::header::CONTENT_ENCODING)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("?"),
            total_bytes,
            supports_range
        );
    }

    Ok(FileInfo {
        file_name,
        total_bytes,
        supports_range,
        content_type,
        etag,
        last_modified,
        content_encoding_compressed,
    })
}

/// 拼接三次探测（HEAD / ranged GET / plain GET）全部失败时的诊断文案，
/// 形如 `"probes failed: HEAD=405, ranged GET=400, plain GET=400"`。抽成
/// 纯函数（不涉及网络 I/O）便于单元测试覆盖格式，不必依赖 mock HTTP server。
fn format_probe_failure(
    head_status_desc: &str,
    get_status_desc: &str,
    plain_status_desc: &str,
) -> String {
    format!(
        "probes failed: HEAD={}, ranged GET={}, plain GET={}",
        head_status_desc, get_status_desc, plain_status_desc
    )
}

/// 双探测（HEAD + Range GET）失败后的最后一次降级尝试：发一次【无 Range 头】
/// 的普通 GET，只读响应头后立即 drop response（绝不读 body），从中提取文件
/// 元数据。
///
/// 背景（例如飞牛 OS「多文件下载」multiple-download 端点）：一次性/限配额
/// 下载 token 只允许消耗有限次数的请求——HEAD 方法本身不被支持（恒 405），
/// 带 Range 的 GET 也被拒绝（恒 400，配额已耗尽或该端点根本不支持 Range），
/// 但不带 Range 的普通 GET 能正常返回 200。旧逻辑在双探测失败时直接判死
/// 任务，而这类服务器其实是可下载的——只是探测方式不对路。
///
/// 这是 `resolve_file_info` 重试循环里【每一轮】最多发出的第 3 个请求
/// （HEAD + ranged GET + 这次的 plain GET），不会叠加成
/// `PROBE_MAX_RETRIES` × 3 次请求；每轮只在前两个探测都失败时才会触发。
///
/// 不区分"服务器可达但状态码错误"与"纯网络错误（连不上）"两种失败：后者
/// 再发一次请求大概率也会失败，但无害，为简单起见不做区分。
async fn resolve_file_info_plain_get_fallback(
    client: &Client,
    url: &str,
    spec: &RequestSpec,
    head_status_desc: &str,
    get_status_desc: &str,
) -> Result<FileInfo, DownloadError> {
    log_info!(
        "[resolve] both probes failed (HEAD={}, ranged GET={}), falling back to plain GET \
         (no Range header) — some servers (e.g. fnOS multiple-download) reject HEAD and \
         Range requests due to a limited-quota token but serve a normal GET",
        head_status_desc,
        get_status_desc
    );

    let resp = match build_request(client, url, reqwest::Method::GET, spec)
        .timeout(PROBE_TIMEOUT)
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            let plain_status_desc = r.status().as_u16().to_string();
            log_info!(
                "[resolve] plain GET fallback also failed: status={}, url={}",
                r.status(),
                r.url()
            );
            return Err(DownloadError::Other(format_probe_failure(
                head_status_desc,
                get_status_desc,
                &plain_status_desc,
            )));
        }
        Err(e) => {
            let plain_status_desc = format!("network-error: {}", e);
            log_info!(
                "[resolve] plain GET fallback network error: {}{}",
                e,
                format_error_chain(e.source())
            );
            return Err(DownloadError::Other(format_probe_failure(
                head_status_desc,
                get_status_desc,
                &plain_status_desc,
            )));
        }
    };

    let final_url = resp.url().clone();
    let headers = resp.headers().clone();
    // 只读响应头，立即 drop response——绝不读取 body，避免消耗一次性 token
    // 或对配额受限的连接造成额外压力。真正的下载阶段会重新发起独立请求。
    drop(resp);

    let total_bytes = headers
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);

    let content_type = headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let file_name = extract_filename(&headers, url, final_url.as_str());

    let etag = headers
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let last_modified = headers
        .get(reqwest::header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let content_encoding_compressed = detect_content_encoding(&headers).is_some();

    // supports_range 决策：这条降级路径【没有 206 佐证】——我们只拿到一次
    // 普通 200 响应，从未验证过服务器真的会诚实响应 Range 请求；而且走到
    // 这里之前，带 Range 的 GET 已经被服务器以 400 明确拒绝过（见调用方的
    // both-probes-failed 分支）。哪怕这次响应头里带了 `Accept-Ranges: bytes`，
    // 也不能采信：这类一次性/限配额 token 的 Accept-Ranges 广告与实际行为
    // 经常脱节，继续相信它会让多段下载阶段对同一个受限 token 再次发起 Range
    // 请求，大概率复现同样的 400，甚至提前耗尽配额导致连单流都下不成。因此
    // 这里保守地强制单流（false），宁可错过少数"这次探测恰好用坏了 token、
    // 其实支持 Range"的场景，也要保证已确认可用的普通 GET 单流路径不被多段
    // 探测拖下水。
    let supports_range = false;

    log_info!(
        "[resolve] plain GET fallback succeeded: name={}, size={}, range={}, ct={}",
        file_name,
        total_bytes,
        supports_range,
        content_type
    );

    Ok(FileInfo {
        file_name,
        total_bytes,
        supports_range,
        content_type,
        etag,
        last_modified,
        content_encoding_compressed,
    })
}

/// 非 GET 请求的元数据探测——只发送一次原始 method+body 请求，从响应头
/// 提取文件名/大小/MIME，不读响应体（drop response 立即释放连接）。
///
/// 设计理由：
///   - HEAD 对 POST 端点通常返回 405/501，且不能携带 body
///   - POST + Range:bytes=0-0 在 HTTP 标准上未定义，服务端实现不一致
///   - 多段下载（Range 分割）对 non-GET 不可靠，统一强制单流
///
/// 因此 supports_range 强制为 false，调用方据此选择单流路径。
async fn resolve_file_info_non_get(
    client: &Client,
    url: &str,
    spec: &RequestSpec,
) -> Result<FileInfo, DownloadError> {
    log_info!(
        "[resolve-non-get] method={} url={} body_present={}",
        spec.method,
        url,
        spec.body.is_some()
    );

    let resp = build_request(client, url, spec.method.clone(), spec)
        .timeout(PROBE_TIMEOUT)
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(DownloadError::Other(format!(
            "non-GET probe returned status {}",
            resp.status()
        )));
    }

    let final_url = resp.url().clone();
    let headers = resp.headers().clone();
    // drop(resp) 在此释放——我们只需头部，body 留给真正的下载阶段
    drop(resp);

    let total_bytes = headers
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);

    let content_type = headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let file_name = extract_filename(&headers, url, final_url.as_str());

    let etag = headers
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let last_modified = headers
        .get(reqwest::header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let content_encoding_compressed = detect_content_encoding(&headers).is_some();

    log_info!(
        "[resolve-non-get] resolved: name={}, size={}, ct={}",
        file_name,
        total_bytes,
        content_type
    );

    Ok(FileInfo {
        file_name,
        total_bytes,
        // 非 GET 强制单流——POST + Range 在标准上未定义，服务端实现不一致
        supports_range: false,
        content_type,
        etag,
        last_modified,
        content_encoding_compressed,
    })
}

// ---------------------------------------------------------------------------
// File-name extraction
// ---------------------------------------------------------------------------

/// MIME type → common extension mapping for when there is no filename.
fn mime_to_ext(content_type: &str) -> Option<&'static str> {
    let ct = content_type.split(';').next().unwrap_or("").trim();
    match ct {
        "application/pdf" => Some("pdf"),
        "application/zip" => Some("zip"),
        "application/x-gzip" | "application/gzip" => Some("gz"),
        "application/x-tar" => Some("tar"),
        "application/x-bzip2" => Some("bz2"),
        "application/x-xz" => Some("xz"),
        "application/x-7z-compressed" => Some("7z"),
        "application/x-rar-compressed" | "application/vnd.rar" => Some("rar"),
        "application/json" => Some("json"),
        "application/xml" | "text/xml" => Some("xml"),
        "application/javascript" | "text/javascript" => Some("js"),
        "application/wasm" => Some("wasm"),
        "application/octet-stream" => None, // generic binary
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => Some("xlsx"),
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => Some("docx"),
        "application/vnd.openxmlformats-officedocument.presentationml.presentation" => Some("pptx"),
        "application/msword" => Some("doc"),
        "application/vnd.ms-excel" => Some("xls"),
        "application/vnd.ms-powerpoint" => Some("ppt"),
        "application/x-iso9660-image" => Some("iso"),
        "application/x-msdownload" | "application/x-dosexec" => Some("exe"),
        "application/vnd.android.package-archive" => Some("apk"),
        "application/java-archive" => Some("jar"),
        "application/x-shockwave-flash" => Some("swf"),
        "application/x-debian-package" => Some("deb"),
        "application/x-rpm" => Some("rpm"),
        "application/x-msi" => Some("msi"),
        "application/vnd.apple.installer+xml" => Some("pkg"),
        "text/html" => Some("html"),
        "text/css" => Some("css"),
        "text/csv" => Some("csv"),
        "text/plain" => Some("txt"),
        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "image/svg+xml" => Some("svg"),
        "image/bmp" => Some("bmp"),
        "image/x-icon" | "image/vnd.microsoft.icon" => Some("ico"),
        "image/tiff" => Some("tiff"),
        "image/avif" => Some("avif"),
        "audio/mpeg" => Some("mp3"),
        "audio/ogg" => Some("ogg"),
        "audio/wav" | "audio/x-wav" => Some("wav"),
        "audio/flac" => Some("flac"),
        "audio/aac" => Some("aac"),
        "audio/mp4" | "audio/x-m4a" => Some("m4a"),
        "audio/webm" => Some("weba"),
        "video/mp4" => Some("mp4"),
        "video/webm" => Some("webm"),
        "video/x-matroska" => Some("mkv"),
        "video/x-msvideo" => Some("avi"),
        "video/quicktime" => Some("mov"),
        "video/x-flv" => Some("flv"),
        "video/mp2t" => Some("ts"),
        "video/3gpp" => Some("3gp"),
        "font/woff" => Some("woff"),
        "font/woff2" => Some("woff2"),
        "font/ttf" | "application/x-font-ttf" => Some("ttf"),
        "font/otf" => Some("otf"),
        _ => None,
    }
}

pub(crate) fn extract_filename(
    headers: &reqwest::header::HeaderMap,
    request_url: &str,
    final_url: &str,
) -> String {
    // 1. Try Content-Disposition: attachment; filename="xxx"
    if let Some(name) = extract_from_content_disposition(headers) {
        return name;
    }

    // 2. Try URL path (after removing query & fragment).
    //
    // Redirects make this ambiguous: either side may hold the real name.
    // GitHub's `archive/refs/tags/<tag>.zip` redirects to codeload's
    // `.../zip/refs/tags/<tag>` (extension dropped — a dot inside the tag,
    // like "11.0-1b", then manufactures a bogus ".0-1b"), while a
    // `download.php`-style endpoint redirects to a CDN URL that is the only
    // place the real filename exists. No static preference is right for
    // both, so decide per-case:
    //
    //   a. The final segment equals the request segment minus its (possibly
    //      multi-part) extension → the redirect provably dropped the
    //      extension (codeload pattern, both `.zip` and `.tar.gz`); use the
    //      request segment, restoring it.
    //   b. The final segment carries a plausible extension → trust it, same
    //      as the pre-redirect-aware behavior (covers shortlinks and
    //      `download.php` → CDN redirects).
    //   c. Only the request segment carries a plausible extension → use it
    //      (redirect target structurally lacks a filename).
    //   d. Neither looks like a filename → final, then request. The request
    //      tier is new relative to the pre-redirect-aware code and outranks
    //      the MIME fallback: an extensionless request segment beats a
    //      generic "download.<ext>".
    let from_request = extract_from_url(request_url);
    let from_final = extract_from_url(final_url);
    if let (Some(req), Some(fin)) = (&from_request, &from_final)
        && let Some(rest) = req.strip_prefix(fin.as_str())
        && let Some(ext) = rest.strip_prefix('.')
        && !ext.is_empty()
        && ext.split('.').all(|part| {
            (1..=10).contains(&part.len()) && part.chars().all(|c| c.is_ascii_alphanumeric())
        })
    {
        return req.clone();
    }
    if let Some(fin) = &from_final
        && has_plausible_extension(fin)
    {
        return fin.clone();
    }
    if let Some(req) = &from_request
        && has_plausible_extension(req)
    {
        return req.clone();
    }
    if let Some(name) = from_final {
        return name;
    }
    if let Some(name) = from_request {
        return name;
    }

    // 3. Try Content-Type → build "download.ext"
    if let Some(ct) = headers.get(reqwest::header::CONTENT_TYPE)
        && let Ok(ct_str) = ct.to_str()
        && let Some(ext) = mime_to_ext(ct_str)
    {
        return format!("download.{}", ext);
    }

    "download".to_string()
}

/// Whether `name`'s trailing "extension" (the part after the last `.`) looks
/// like a real file extension: 1–10 ASCII alphanumeric characters. Used to
/// prefer a URL whose last path segment carries a genuine extension over one
/// that merely contains a stray dot — e.g. a version number embedded in a
/// path segment, as in GitHub's codeload redirect targets.
fn has_plausible_extension(name: &str) -> bool {
    match name.rfind('.') {
        Some(pos) if pos + 1 < name.len() => {
            let ext = &name[pos + 1..];
            (1..=10).contains(&ext.len()) && ext.chars().all(|c| c.is_ascii_alphanumeric())
        }
        _ => false,
    }
}

fn extract_from_content_disposition(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let disposition = headers.get(reqwest::header::CONTENT_DISPOSITION)?;
    // Use from_utf8 instead of to_str(): the http crate's to_str() rejects any byte > 0x7E,
    // but some servers (e.g. z-lib CDN) embed raw UTF-8 characters (Chinese, Japanese, etc.)
    // directly in the filename="" parameter.  Those bytes are valid UTF-8 even though they
    // are not ASCII, so from_utf8 succeeds where to_str would silently return None.
    let value = std::str::from_utf8(disposition.as_bytes()).ok()?;

    // Prefer filename*= (RFC 5987 / RFC 6266) over filename=
    for part in value.split(';') {
        let trimmed = part.trim();
        if let Some(name) = trimmed.strip_prefix("filename*=") {
            // Format: charset'language'percent-encoded-name
            // e.g. UTF-8''My%20File.pdf
            //
            // 注：按 RFC 5987 charset 字段明确指定编码，严格实现
            // 应该读取该字段。目前以 urlencoding_decode 的
            // "UTF-8 优先，GBK fallback" 表现足够应对老旧中文服务器
            // （它们通常话不对题，声明 UTF-8 但发 GBK）。
            // 非标准实现（腾讯云 COS 等）会把整个 ext-value 用双引号包起来：
            // `filename*="UTF-8''foo.exe"`。RFC 6266 的 ext-value 是 token 不
            // 允许加引号，若原样保留，尾引号会跟进文件名（Windows 上再被
            // sanitize_filename 换成 `_`，落盘名多一个下划线）。
            let name = name.trim().trim_matches('"').trim();
            if let Some(encoded) = name.split('\'').nth(2)
                && let Ok(decoded) = urlencoding_decode(encoded)
            {
                let decoded = decoded.trim();
                if !decoded.is_empty() {
                    return Some(sanitize_filename(decoded));
                }
            }
        }
    }

    for part in value.split(';') {
        let trimmed = part.trim();
        if let Some(name) = trimmed.strip_prefix("filename=") {
            let name = name.trim_matches(|c| c == '"' || c == '\'' || c == ' ');
            if !name.is_empty() {
                // Heuristic: some servers (e.g. Chinese cloud storage OBS/S3)
                // percent-encode the filename= value instead of using the
                // RFC 5987 filename*= syntax.  When the raw value contains
                // percent-encoded sequences, try URL-decoding it so that
                // `%E6%B0%B8%E7%94%9F.mp4` becomes `永生.mp4`.
                if name.contains('%')
                    && let Ok(decoded) = urlencoding_decode(name)
                {
                    let decoded = decoded.trim();
                    if !decoded.is_empty() && decoded != name {
                        return Some(sanitize_filename(decoded));
                    }
                }
                return Some(sanitize_filename(name));
            }
        }
    }

    None
}

pub fn extract_from_url(url: &str) -> Option<String> {
    // Strip query and fragment
    let path = url.split('?').next().unwrap_or(url);
    let path = path.split('#').next().unwrap_or(path);
    let segment = path.rsplit('/').next()?;
    let decoded = urlencoding_decode(segment).unwrap_or_else(|_| segment.to_string());
    let decoded = decoded.trim();
    if decoded.is_empty() || decoded == "/" {
        return None;
    }
    Some(sanitize_filename(decoded))
}

/// 文件名单组件的最大字节数（F051）。
///
/// 大多数文件系统（ext4/APFS/NTFS）的单路径组件上限为 255 字节；这里取 200
/// 作为保守预算，给 `.fdownloading` 临时后缀（13 字节）及未来可能的 dedup
/// `" (NN)"` 后缀留出余量。超长的 Content-Disposition / URL 段若原样放行，
/// `save_dir.join(name) + ".fdownloading"` 会触顶导致 create 报 ENAMETOOLONG，
/// 下载以晦涩 OS 错误失败。多字节 CJK 约 66 字即可触及 200 字节。
const MAX_FILENAME_BYTES: usize = 200;

/// Windows 保留设备名（不区分大小写，比较时取扩展名前的 stem）。
///
/// 在 Windows 上创建这些名字（无论是否带扩展名，如 `CON`、`NUL.txt`）会失败
/// 或行为异常。本项目主要目标平台为 Windows，故统一在文件名出口处规避。
const WINDOWS_RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Remove or replace characters that are illegal in file names on Windows/macOS/Linux.
///
/// 额外保证（F051）：
///   - 规避 Windows 保留设备名（CON/PRN/AUX/NUL/COM1-9/LPT1-9）——在 stem 前
///     加下划线；
///   - 把结果按字节截断到 [`MAX_FILENAME_BYTES`]，截断在 char 边界进行，避免
///     切断多字节 CJK 字符。
pub fn sanitize_filename(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let s = s.trim_matches(|c: char| c == '.' || c == ' ');
    if s.is_empty() {
        return "download".to_string();
    }

    // --- F051(1): Windows 保留设备名规避 ---
    // 取扩展名前的 stem（首个 '.' 之前的部分）做大小写无关比较。
    let stem_end = s.find('.').unwrap_or(s.len());
    let stem = &s[..stem_end];
    let s = if WINDOWS_RESERVED_NAMES
        .iter()
        .any(|r| stem.eq_ignore_ascii_case(r))
    {
        format!("_{}", s)
    } else {
        s.to_string()
    };

    // --- F051(2): 字节长度截断（在 char 边界） ---
    if s.len() <= MAX_FILENAME_BYTES {
        return s;
    }
    // 保留扩展名（最后一个 '.' 起的部分），从 stem 尾部按 char 边界裁剪。
    let ext_start = s.rfind('.').unwrap_or(s.len());
    let (stem, ext) = s.split_at(ext_start);
    let budget = MAX_FILENAME_BYTES.saturating_sub(ext.len());
    // 找到 <= budget 的最大 char 边界。
    let cut = stem
        .char_indices()
        .map(|(i, _)| i)
        .take_while(|&i| i <= budget)
        .last()
        .unwrap_or(0);
    let truncated = format!("{}{}", &stem[..cut], ext);
    // 截断后再次 trim 尾部 '.'/' '（避免裁出以点/空格结尾的名）；若整体为空则兜底。
    let truncated = truncated.trim_matches(|c: char| c == '.' || c == ' ');
    if truncated.is_empty() {
        "download".to_string()
    } else {
        truncated.to_string()
    }
}

/// 将单个十六进制 ASCII 字节解析为 0..=15 的半字节（nibble）。
///
/// 仅接受 `0-9` / `a-f` / `A-F`；其他字节返回 `None`。供 `urlencoding_decode`
/// 按字节解析 `%XX` 转义使用，避免对 `&str` 切片导致的字符边界 panic。
fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// 解码 URL 路径段 / Content-Disposition 文件名中的百分号转义。
///
/// **按字节解析，绝不对 `&str` 切片**：原实现用 `&s[i+1..i+3]` 取两位十六进制，
/// 当 `%` 后紧跟原始多字节 UTF-8 字符（如 `50%折扣.txt`）时，切片终点会落在
/// 多字节字符内部触发 `byte index N is not a char boundary` panic（F017）。改为
/// 直接对 `bytes[i+1]` / `bytes[i+2]` 解析半字节后即可消除该 panic。
///
/// **不把 `+` 解码为空格**（F046）：按 RFC 3986，`+` 仅在
/// `application/x-www-form-urlencoded`（query / form body）中表示空格；在 URL
/// 路径段、Content-Disposition、RFC 5987 `filename*=` 中 `+` 都是字面加号
/// （空格用 `%20`）。本函数的所有调用方（extract_from_url /
/// extract_from_content_disposition）均为路径/文件名场景，且 extract_from_url
/// 在调用前已 `split('?')` 丢弃 query，故 `+`→空格 在所有实际用途下都是错的
/// （会把 `C++Primer.pdf` 损坏成 `C  Primer.pdf`）。
fn urlencoding_decode(s: &str) -> Result<String, String> {
    let mut result = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_nibble(bytes[i + 1]), hex_nibble(bytes[i + 2]))
        {
            result.push((hi << 4) | lo);
            i += 3;
            continue;
        }
        // 非法 `%` 转义或普通字节：原样保留（含字面 `+`）。
        result.push(bytes[i]);
        i += 1;
    }
    decode_bytes_utf8_or_gbk(&result)
}

/// 将一组字节解码为字符串，优先 UTF-8，失败时回退到 GBK。
///
/// HTML5 规范要求 URL percent-encoding 使用 UTF-8，但大量老旧中文站点
/// （包括一些 CDN/云存储）仍使用 GBK 编码，如 `%CE%C4%BC%FE.txt`
/// 对应 GBK 的 "文件.txt"。若不做回退则 UTF-8 解码必然失败，最终
/// 用户看到 `%CE%C4%BC%FE.txt` 这种看似乱码的文件名。
///
/// # 已知局限
///
/// GBK 的字节空间很宽松（0x81-0xFE × 0x40-0xFE），其他二字节编码
/// 的字节序列（如 Big5、Shift-JIS）也可能被 GBK “成功”解码为错误的中文。
/// 权衡上这个误判仅在罕见场景下发生（现代 Big5/Latin 站点几乎不会
/// 在 URL 中使用非 UTF-8 percent-encoding），而 GBK 中文乱码是老旧中文
/// 站点的高频问题。
///
/// # 返回值
///
/// 返回 Err 仅当两种编码都无法解码时（极罕见，需要出现 GBK 不允许的
/// 字节组合，如 0x81 0x7F）。
pub(crate) fn decode_bytes_utf8_or_gbk(bytes: &[u8]) -> Result<String, String> {
    match std::str::from_utf8(bytes) {
        Ok(s) => Ok(s.to_string()),
        Err(_) => {
            // 使用 decode_without_bom_handling_and_without_replacement：
            // 遇到非法字节时返回 None，不插入 U+FFFD。
            // 这样可以准确区分 “GBK 中合法但含替换字符” 和 “GBK 解码失败”。
            match encoding_rs::GBK.decode_without_bom_handling_and_without_replacement(bytes) {
                Some(decoded) => Ok(decoded.into_owned()),
                None => Err(format!(
                    "bytes are neither valid UTF-8 nor valid GBK ({} bytes)",
                    bytes.len()
                )),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Dedup file name: "file.txt" → "file (1).txt" etc.
// ---------------------------------------------------------------------------

/// Deduplicate a filename so it does not collide with any existing file in
/// `dir` **nor** with any in-flight download that has already reserved the
/// same temporary path.
///
/// # Parameters
/// - `dir`      – target save directory.
/// - `name`     – desired filename (e.g. `"video.mp4"`).
/// - `reserved` – snapshot of `DownloadManager::reserved_temp_paths`;
///   contains the `.fdownloading` paths that concurrent tasks have already
///   claimed.  Pass an empty set when the caller has no reserved paths to
///   check (e.g. resume tasks, which skip dedup entirely).
///
/// # Why `reserved` is needed
/// `dedup_filename` is called from inside a spawned tokio task, well after
/// the manager's synchronous section has finished.  Multiple tasks spawned
/// in the same batch can all enter `dedup_filename` concurrently; each sees
/// the same on-disk state (no `.fdownloading` file yet) and all independently
/// choose the same filename.  They then race to write the same temp file,
/// causing the last writer to silently overwrite the earlier ones.
///
/// By consulting `reserved` — a snapshot taken **before** spawning, in the
/// manager's synchronous section — each task can see which names its siblings
/// have already claimed and avoid them.
/// `avoid`:**小写折叠**的额外占用名(如 finalize 冲突时从 DB 采集的同目录
/// 未完成任务 file_name)。与磁盘条目一并视为冲突,防止 finalize 换名撞上
/// 兄弟任务「已预订但临时文件尚未落盘」的名字造成 DB 指针别名(两任务
/// file_name 指向同一磁盘名,误删其一即毁对方产物)。
///
/// `allow_overwrite`（config `file_exists_behavior` == "overwrite"）:为
/// true 时,磁盘上**仅最终文件**存在不算冲突——保留原名,完成时由
/// finalize 覆盖旧文件;`.fdownloading` 临时文件、`reserved` 预订与
/// `avoid` 集合命中仍是硬冲突,照旧编号改名。目录同名也照旧改名
/// (文件不能覆盖目录)。
pub async fn dedup_filename(
    dir: &Path,
    name: &str,
    reserved: &std::collections::HashSet<std::path::PathBuf>,
    avoid: &std::collections::HashSet<String>,
    allow_overwrite: bool,
) -> String {
    // Phase 1: fast probe — most of the time there is no conflict.
    let candidate = dir.join(name);
    let temp_candidate = PathBuf::from(format!("{}{}", candidate.display(), TEMP_EXT));
    // Also check the in-flight reservation set BEFORE the async disk probes
    // so that two tasks starting simultaneously both see each other's claim.
    let final_conflict = if allow_overwrite {
        // overwrite 模式:仅目录算最终名冲突(文件不能覆盖目录);普通
        // 文件存在 = 保留原名,finalize 时覆盖。
        tokio::fs::metadata(&candidate)
            .await
            .map(|m| m.is_dir())
            .unwrap_or(false)
    } else {
        tokio::fs::try_exists(&candidate).await.unwrap_or(false)
    };
    if !reserved.contains(&temp_candidate)
        && !avoid.contains(&name.to_lowercase())
        && !final_conflict
        && !tokio::fs::try_exists(&temp_candidate)
            .await
            .unwrap_or(false)
    {
        return name.to_string();
    }

    // Phase 2: conflict detected — scan directory into memory to avoid
    // up to 19998 filesystem calls in the dedup loop.
    //
    // 条目名**小写折叠**后入集:Windows/APFS 大小写不敏感,精确字节比较会
    // 漏判 `MOVIE (1).mp4` vs 已存在的 `Movie (1).mp4`,finalize rename 的
    // REPLACE 语义会静默覆盖真实文件。非 UTF-8 名经 lossy 转换,只可能把
    // 不冲突误判为冲突(多让一个编号),决不会漏判。
    let existing: std::collections::HashSet<String> = {
        let mut set = std::collections::HashSet::new();
        if let Ok(mut entries) = tokio::fs::read_dir(dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                set.insert(entry.file_name().to_string_lossy().to_lowercase());
            }
        }
        set
    };

    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    let ext = Path::new(name).extension().and_then(|s| s.to_str());

    for i in 1..=9999 {
        let new_name = if let Some(ext) = ext {
            format!("{} ({}).{}", stem, i, ext)
        } else {
            format!("{} ({})", stem, i)
        };
        let temp_name = format!("{}{}", new_name, TEMP_EXT);
        let temp_path = dir.join(&temp_name);
        // Check the final/in-progress disk files, the in-flight set AND avoid.
        if !reserved.contains(&temp_path)
            && !avoid.contains(&new_name.to_lowercase())
            && !existing.contains(&new_name.to_lowercase())
            && !existing.contains(&temp_name.to_lowercase())
        {
            return new_name;
        }
    }
    // 极端兜底:1..=9999 个编号变体全被占用时,此前返回**原名不变**,finalize
    // rename 会静默覆盖已存在文件丢数据。与 BT 侧 `dedup_name_in_dir` 对齐,
    // 用 UUID 后缀保证唯一。(BUG-BT-DEDUP-FALLBACK-OVERWRITE 的 HTTP 同类)
    let uniq = uuid::Uuid::new_v4();
    match ext {
        Some(e) => format!("{} ({}).{}", stem, uniq, e),
        None => format!("{} ({})", stem, uniq),
    }
}

/// Temporary file extension used during download (like Chrome's `.crdownload`).
/// The file is renamed to the final name only after all data is verified.
pub const TEMP_EXT: &str = ".fdownloading";

/// Buffer size for `BufWriter` wrapping file I/O during downloads.
/// 256 KB reduces the frequency of syscalls compared to the default 8 KB,
/// significantly improving throughput especially with many concurrent segments.
pub const BUF_WRITER_CAPACITY: usize = 256 * 1024;

/// Interval (in seconds) between DB persistence of download progress.
/// Balances crash-recovery granularity (max ~3 s of re-download) against
/// SQLite Mutex contention (reduces writes from ~80/s to ~5/s with 16 segments).
pub const DB_SAVE_INTERVAL_SECS: u64 = 3;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

