//! axum 路由与 handler：REST + WebSocket。
//!
//! 鉴权：控制类端点需 `Authorization: Bearer <token>`；仅 `/api/info`、`/api/pair` 免鉴权。
//! 服务关闭时统一返回 404；令牌无效 401；配对码错误/过期 403；需确认 428；爆破锁定 429。

use std::net::SocketAddr;
use std::time::Duration;

use axum::body::Body;
use axum::extract::connect_info::ConnectInfo;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State};
use axum::http::{header, header::AUTHORIZATION, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Emitter;
use tokio::io::AsyncWriteExt;
use tokio::sync::broadcast;
use tokio_util::io::ReaderStream;

use super::auth;
use super::models::{ActionReq, PairReq, QueryReq};
use super::registry;
use super::transfer;

/// handler 共享状态：AppHandle + 动作事件广播通道。
#[derive(Clone)]
pub struct AppState {
    pub app: tauri::AppHandle,
    pub tx: broadcast::Sender<String>,
}

/// 全局广播通道（供潜在内部事件发布；当前在 set 时幂等保存）。
#[allow(dead_code)]
static BUS: std::sync::OnceLock<broadcast::Sender<String>> = std::sync::OnceLock::new();

#[allow(dead_code)]
pub fn set_bus(tx: broadcast::Sender<String>) {
    let _ = BUS.set(tx);
}

/// 向所有活动 WS 广播一帧文本（文件互传等模块在 HTTP 之外触发变更时用）。
pub fn broadcast(msg: String) {
    if let Some(tx) = BUS.get() {
        let _ = tx.send(msg);
    }
}

// ───────────────────────── helpers ─────────────────────────

fn json_ok(data: Value) -> Response {
    (StatusCode::OK, Json(json!({ "ok": true, "data": data }))).into_response()
}

fn json_err(status: StatusCode, msg: &str) -> Response {
    (
        status,
        Json(json!({ "ok": false, "error": { "code": status.as_u16(), "message": msg } })),
    )
        .into_response()
}

/// 校验 Bearer 令牌，命中返回 device_id；否则返回错误响应。
fn authorize(headers: &HeaderMap) -> Result<String, Response> {
    if !super::is_enabled() {
        return Err(json_err(StatusCode::NOT_FOUND, "closed"));
    }
    let auth = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let token = auth.strip_prefix("Bearer ").map(str::trim).unwrap_or("");
    if token.is_empty() {
        return Err(json_err(StatusCode::UNAUTHORIZED, "missing token"));
    }
    auth::auth_token(token).ok_or_else(|| json_err(StatusCode::UNAUTHORIZED, "invalid token"))
}

fn computer_name() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_default()
}

// ───────────────────────── routes ─────────────────────────

pub fn router(state: AppState) -> Router {
    // 文件互传：上传走 multipart 流式落盘，放开默认 2MB body 限制（仅这一组路由）
    let transfer_routes = Router::new()
        .route("/files", get(transfer_list).post(transfer_upload))
        .route("/files/:id/download", get(transfer_download))
        .route("/files/:id/ack", post(transfer_ack))
        .route("/files/:id/progress", post(transfer_progress_report))
        .route("/files/:id", delete(transfer_remove))
        .layer(DefaultBodyLimit::disable())
        .with_state(state.clone());

    Router::new()
        .route("/api/info", get(info))
        .route("/api/pair", post(pair).delete(unpair))
        .route("/api/pair/request", post(pair_request))
        .route("/api/pair/request/:id", get(pair_request_status).delete(pair_request_cancel))
        .route("/api/capabilities", get(capabilities))
        .route("/api/query", post(query))
        .route("/api/action", post(action))
        .route("/api/music/cover", get(music_cover))
        .nest("/api/transfer", transfer_routes)
        .route("/api/ws", get(ws))
        .with_state(state)
}

async fn info(State(state): State<AppState>) -> Response {
    let _ = &state;
    let code = if super::is_enabled() { auth::current_code_info().0 } else { None };
    json_ok(json!({
        "online": true,
        "service": "nexbox",
        "version": env!("CARGO_PKG_VERSION"),
        "controlEnabled": super::is_enabled(),
        "pairingActive": code.is_some(),
        "lanName": computer_name(),
    }))
}

async fn pair(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Json(req): Json<PairReq>,
) -> Response {
    if !super::is_enabled() {
        return json_err(StatusCode::NOT_FOUND, "closed");
    }
    let ip = peer.ip().to_string();
    match auth::try_pair(&ip, &req.code, req.device_name, req.device_uid) {
        Ok((device_id, token)) => {
            log::info!("[RemoteAccess] 设备配对成功 ip={ip} id={device_id}");
            json_ok(json!({ "token": token, "deviceId": device_id }))
        }
        Err(auth::PairError::CodeInvalid) => json_err(StatusCode::FORBIDDEN, "invalid or expired code"),
        Err(auth::PairError::Locked) => json_err(StatusCode::TOO_MANY_REQUESTS, "too many attempts, locked"),
        Err(auth::PairError::Limit) => json_err(StatusCode::FORBIDDEN, "device limit reached"),
    }
}

// ───────── 请求配对：免配对码，改为在 PC 端界面上人工点「允许」 ─────────

#[derive(Deserialize)]
struct PairRequestBody {
    #[serde(default)]
    device_name: Option<String>,
    /// 手机端稳定身份，PC 端据此对同一台手机去重，避免重复已配对设备
    #[serde(default)]
    device_uid: Option<String>,
}

/// POST /api/pair/request
/// 手机只报上自己的设备名，PC 端弹出确认；批准后手机轮询取走令牌。
async fn pair_request(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
    Json(req): Json<PairRequestBody>,
) -> Response {
    if !super::is_enabled() {
        return json_err(StatusCode::NOT_FOUND, "closed");
    }
    let ip = peer.ip().to_string();
    match auth::create_request(&ip, req.device_name, req.device_uid) {
        Ok(view) => {
            // 推给 PC 端界面，弹出「是否允许配对」
            let _ = state.app.emit("remote-access://pair-request", &view);
            json_ok(json!({ "requestId": view.id, "expiresAt": view.expires_at }))
        }
        Err(auth::PairError::Limit) => json_err(StatusCode::FORBIDDEN, "device limit reached"),
        Err(_) => json_err(StatusCode::TOO_MANY_REQUESTS, "too many pending requests"),
    }
}

/// GET /api/pair/request/:id —— 手机轮询审批结果
async fn pair_request_status(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
) -> Response {
    if !super::is_enabled() {
        return json_err(StatusCode::NOT_FOUND, "closed");
    }
    let st = auth::poll_request(&id, &peer.ip().to_string());
    json_ok(json!({
        "status": st.status,
        "token": st.token,
        "deviceId": st.device_id,
    }))
}

/// DELETE /api/pair/request/:id —— 手机主动取消（退出页面等）
async fn pair_request_cancel(Path(id): Path<String>) -> Response {
    auth::cancel_request(&id);
    json_ok(json!({ "ok": true }))
}

async fn unpair(headers: HeaderMap) -> Response {    match authorize(&headers) {
        Ok(device_id) => {
            // 设备自助解绑：删除自身令牌
            auth::revoke(&device_id);
            if let Some(app) = super::app_handle() {
                auth::persist(&app);
            }
            json_ok(json!({ "ok": true }))
        }
        Err(r) => r,
    }
}

async fn capabilities(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let _ = &state;
    match authorize(&headers) {
        Ok(_) => json_ok(registry::capabilities()),
        Err(r) => r,
    }
}

async fn query(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<QueryReq>,
) -> Response {
    if let Err(r) = authorize(&headers) {
        return r;
    }
    if !registry::is_known_query(&req.key) {
        return json_err(StatusCode::NOT_FOUND, "unknown query key");
    }
    let args = req.args.unwrap_or(Value::Null);
    match super::actions::exec_query(&state.app, &req.key, &args).await {
        Ok(data) => json_ok(data),
        Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

async fn action(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ActionReq>,
) -> Response {
    let device_id = match authorize(&headers) {
        Ok(d) => d,
        Err(r) => return r,
    };
    if !registry::is_known_action(&req.key) {
        return json_err(StatusCode::NOT_FOUND, "unknown action key");
    }
    if registry::action_needs_confirm(&req.key) && !req.confirm {
        return json_err(StatusCode::PRECONDITION_REQUIRED, "requires confirm=true");
    }
    let args = req.args.unwrap_or(Value::Null);
    let result = super::actions::exec_action(&state.app, &req.key, &args).await;
    match result {
        Ok(data) => {
            log::info!("[RemoteAccess] 设备 {device_id} 执行动作 {}", req.key);
            // 向所有 WS 客户端广播，多端状态同步
            let _ = state.tx.send(json!({ "type": "action.done", "key": req.key }).to_string());
            json_ok(data)
        }
        Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

// ───────────────────────── 音乐封面 ─────────────────────────

/// 封面端点的鉴权：query token 优先（方便手机端图片加载器直接拼 URL），回退 Bearer。
fn authorize_cover(headers: &HeaderMap, token_q: Option<&str>) -> Result<String, Response> {
    if !super::is_enabled() {
        return Err(json_err(StatusCode::NOT_FOUND, "closed"));
    }
    let token = token_q
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .or_else(|| {
            headers
                .get(AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|a| a.strip_prefix("Bearer "))
                .map(|s| s.trim().to_string())
        })
        .unwrap_or_default();
    if token.is_empty() {
        return Err(json_err(StatusCode::UNAUTHORIZED, "missing token"));
    }
    auth::auth_token(&token).ok_or_else(|| json_err(StatusCode::UNAUTHORIZED, "invalid token"))
}

#[derive(Deserialize)]
struct CoverQuery {
    token: Option<String>,
    /// 封面内容标识（`music.state.coverKey`）。必须与当前生效来源的 hash 一致。
    k: Option<String>,
}

/// GET /api/music/cover?k=<coverKey>&token=<deviceToken>
/// 返回当前「正在播放」那首歌的封面字节（内容寻址，可长期缓存）。
async fn music_cover(headers: HeaderMap, Query(q): Query<CoverQuery>) -> Response {
    if let Err(r) = authorize_cover(&headers, q.token.as_deref()) {
        return r;
    }
    let Some(k) = q.k.filter(|s| !s.is_empty()) else {
        return json_err(StatusCode::BAD_REQUEST, "missing k");
    };
    // 取封面是阻塞操作（网络下载 + 图片解码），必须挪出 async 上下文
    let fetched = tokio::task::spawn_blocking(move || super::music::cover_bytes(&k)).await;
    match fetched {
        // 显式构造响应：不要用 `([headers], Vec<u8>)` 元组形式——
        // axum 对 Vec<u8> 的 IntoResponse 会先塞 application/octet-stream，
        // 元组再 extend 会变成两个 Content-Type。
        Ok(Some((bytes, mime))) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime)
            .header(header::CACHE_CONTROL, "public, max-age=86400, immutable")
            .body(Body::from(bytes))
            .unwrap_or_else(|_| {
                json_err(StatusCode::INTERNAL_SERVER_ERROR, "build cover response failed")
            }),
        // 无封面 / key 过期 / key 与当前来源不符，统一 404（不泄漏本机路径信息）
        _ => json_err(StatusCode::NOT_FOUND, "no cover"),
    }
}

#[derive(Deserialize)]
struct WsQuery {
    token: Option<String>,
}

async fn ws(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
    Query(q): Query<WsQuery>,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !super::is_enabled() {
        return json_err(StatusCode::NOT_FOUND, "closed");
    }
    let token = q.token.unwrap_or_default();
    let device_id = match auth::auth_token(&token) {
        Some(id) => id,
        None => return json_err(StatusCode::UNAUTHORIZED, "invalid token"),
    };
    // 登记为在线（供首页「手机远程」卡片展示），并拿到踢下线信号；撤销时靠它立即断开。
    let kick = auth::register_connection(&device_id, &peer.ip().to_string());
    let _ = state.app.emit("remote-access://connection-changed", ());
    upgrade.on_upgrade(move |socket| run_socket(socket, state, device_id, kick))
}

/// WS 连接：每 ~1s 推送硬件快照；订阅动作事件；收到关闭帧、被踢下线或写失败即结束。
async fn run_socket(
    socket: WebSocket,
    state: AppState,
    device_id: String,
    mut kick: broadcast::Receiver<()>,
) {
    let (mut sink, mut stream) = socket.split();
    let mut rx = state.tx.subscribe();
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    // 首个 tick 立即触发
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // 音乐状态推送去重：签名（换歌/播放态/封面）变化，或位置推进 ≥800ms 才推。
    // 封面只给 coverKey（字节走 /api/music/cover），因此帧很小、可以每秒推。
    let mut last_music_sig = String::new();
    let mut last_music_pos: Option<i64> = None;

    loop {
        tokio::select! {
            _ = tick.tick() => {
                // 音乐状态放在硬件快照之前，避免被下面的 continue 跳过
                let music = super::music::snapshot();
                let music_sig = music.signature();
                let pos_moved = match last_music_pos {
                    Some(prev) => (music.position_ms - prev).abs() >= 800,
                    None => true, // 首拍必推一次（含无播放的空态）
                };
                if music_sig != last_music_sig || pos_moved {
                    last_music_sig = music_sig;
                    last_music_pos = Some(music.position_ms);
                    if let Ok(v) = serde_json::to_value(&music) {
                        let frame = json!({ "type": "music.state", "data": v }).to_string();
                        if sink.send(Message::Text(frame)).await.is_err() {
                            break;
                        }
                    }
                }

                let data = tokio::task::spawn_blocking(crate::overlay_panel::collect_hardware_data).await;
                let payload = match data {
                    Ok(d) => match serde_json::to_value(&d) {
                        Ok(mut v) => {
                            if let Some(obj) = v.as_object_mut() {
                                obj.insert("meta".to_string(), json!({
                                    "computer_name": computer_name(),
                                    "ts": std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map(|d| d.as_millis() as u64).unwrap_or(0),
                                }));
                            }
                            json!({ "type": "stats", "data": v }).to_string()
                        }
                        Err(_) => continue,
                    },
                    Err(_) => continue,
                };
                if sink.send(Message::Text(payload)).await.is_err() {
                    break;
                }
            }
            ev = rx.recv() => {
                if let Ok(s) = ev {
                    if sink.send(Message::Text(s)).await.is_err() {
                        break;
                    }
                }
            }
            incoming = stream.next() => {
                match incoming {
                    None => break,
                    Some(Ok(Message::Close(_))) => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
            // 被服务端主动踢下线（撤销 / 顶号）：立即结束本连接
            _ = kick.recv() => break,
        }
    }

    auth::unregister_connection(&device_id);
    let _ = state.app.emit("remote-access://connection-changed", ());
}

// ───────────────────────── 文件互传（手机端调用） ─────────────────────────

/// GET /api/transfer/files —— 手机视角：incoming=PC 发来的待接收，outgoing=本机已发送。
async fn transfer_list(headers: HeaderMap) -> Response {
    let device_id = match authorize(&headers) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let view = transfer::phone_view(&device_id);
    json_ok(json!({
        "incoming": view.to_device,
        "outgoing": view.from_device,
    }))
}

/// POST /api/transfer/files —— 手机 multipart 上传（可多文件），流式落盘 inbox。
async fn transfer_upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut mp: Multipart,
) -> Response {
    let device_id = match authorize(&headers) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let mut added: Vec<transfer::TransferFile> = Vec::new();

    loop {
        match mp.next_field().await {
            Ok(Some(mut field)) => {
                let Some(raw_name) = field.file_name().map(str::to_string) else { continue };
                let Ok((entry, part_path)) = transfer::alloc_incoming(&device_id, &raw_name) else {
                    continue;
                };
                let mut file = match tokio::fs::File::create(&part_path).await {
                    Ok(f) => f,
                    Err(e) => {
                        transfer::abort_incoming(&device_id, &entry.id);
                        return json_err(StatusCode::INTERNAL_SERVER_ERROR, &format!("create file: {e}"));
                    }
                };
                let mut size: u64 = 0;
                let mut failed = false;
                // 接收进度：节流后推给 PC 前端，让「手机发来的文件」在电脑上也看得到速度
                let started = std::time::Instant::now();
                let mut last_emit = started - Duration::from_millis(500);
                while let Some(chunk) = field.chunk().await.transpose() {
                    match chunk {
                        Ok(bytes) => {
                            size += bytes.len() as u64;
                            if let Err(e) = file.write_all(&bytes).await {
                                log::warn!("[RemoteAccess] 互传落盘失败: {e}");
                                failed = true;
                                break;
                            }
                            let now = std::time::Instant::now();
                            if now.duration_since(last_emit) >= Duration::from_millis(400) {
                                last_emit = now;
                                let secs = now.duration_since(started).as_secs_f64().max(0.05);
                                let _ = state.app.emit(
                                    "remote-access://transfer-progress",
                                    json!({
                                        "deviceId": device_id,
                                        "fileId": entry.id,
                                        "done": size,
                                        "total": 0u64,
                                        "speed": size as f64 / secs,
                                    }),
                                );
                            }
                        }
                        Err(_) => {
                            failed = true;
                            break;
                        }
                    }
                }
                if failed {
                    transfer::abort_incoming(&device_id, &entry.id);
                    return json_err(StatusCode::BAD_REQUEST, "upload interrupted");
                }
                drop(file);
                match transfer::finalize_incoming(&device_id, &entry.id, size) {
                    Ok(done) => added.push(done),
                    Err(_) => {
                        transfer::abort_incoming(&device_id, &entry.id);
                        return json_err(StatusCode::INTERNAL_SERVER_ERROR, "finalize failed");
                    }
                }
            }
            Ok(None) => break,
            Err(_) => {
                // 连接中断：字段中途的失败已在各自分支 abort 落盘条目，
                // 能走到这里只在两个字段之间，没有在途条目需要清理
                return json_err(StatusCode::BAD_REQUEST, "upload aborted");
            }
        }
    }

    if !added.is_empty() {
        log::info!("[RemoteAccess] 设备 {device_id} 上传 {} 个文件", added.len());
        // 通知 PC 前端：有新来件待另存为
        let _ = state.app.emit("remote-access://transfer-changed", ());
    }
    json_ok(json!({ "added": added }))
}

/// GET /api/transfer/files/:id/download —— 手机流式下载 PC 发来的文件。
async fn transfer_download(headers: HeaderMap, Path(id): Path<String>) -> Response {
    let device_id = match authorize(&headers) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let Some(entry) = transfer::find(&device_id, &id) else {
        return json_err(StatusCode::NOT_FOUND, "no such file");
    };
    if entry.direction != "to_device" {
        return json_err(StatusCode::FORBIDDEN, "not downloadable");
    }
    let path = match transfer::file_path(&device_id, &entry) {
        Ok(p) => p,
        Err(e) => return json_err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    };
    let Ok(file) = tokio::fs::File::open(&path).await else {
        return json_err(StatusCode::NOT_FOUND, "file missing");
    };
    let len = match file.metadata().await {
        Ok(m) => m.len(),
        Err(_) => entry.size,
    };
    let encoded_name = urlencoding::encode(&entry.name);
    let stream = ReaderStream::new(file);
    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream")),
            (
                header::CONTENT_LENGTH,
                HeaderValue::from_str(&len.to_string()).unwrap_or(HeaderValue::from_static("0")),
            ),
            (
                header::CONTENT_DISPOSITION,
                HeaderValue::from_str(&format!("attachment; filename*=UTF-8''{encoded_name}"))
                    .unwrap_or(HeaderValue::from_static("attachment")),
            ),
        ],
        Body::from_stream(stream),
    )
        .into_response()
}

/// POST /api/transfer/files/:id/ack —— 手机下载完成回执，PC 端显示「已接收」。
async fn transfer_ack(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let device_id = match authorize(&headers) {
        Ok(d) => d,
        Err(r) => return r,
    };
    match transfer::mark_acked(&device_id, &id) {
        Some(entry) => {
            let _ = state.app.emit("remote-access://transfer-changed", ());
            json_ok(json!({ "file": entry }))
        }
        None => json_err(StatusCode::NOT_FOUND, "no such file"),
    }
}

/// DELETE /api/transfer/files/:id —— 手机清除自己会话里的条目（来件/去件都可清，同一份共享列表）。
async fn transfer_remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let device_id = match authorize(&headers) {
        Ok(d) => d,
        Err(r) => return r,
    };
    if transfer::find(&device_id, &id).is_none() {
        return json_err(StatusCode::NOT_FOUND, "no such file");
    }
    match transfer::remove(&device_id, &id) {
        Ok(_) => {
            let _ = state.app.emit("remote-access://transfer-changed", ());
            json_ok(json!({ "ok": true }))
        }
        Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

/// POST /api/transfer/files/:id/progress —— 手机下载时上报进度/速度，
/// PC 端列表同步显示「手机接收中 · xx MB/s」（手机侧节流 ~500ms 一次）。
#[derive(Deserialize)]
struct TransferProgressBody {
    #[serde(default)]
    done: u64,
    #[serde(default)]
    total: u64,
    #[serde(default)]
    speed: f64,
}

async fn transfer_progress_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<TransferProgressBody>,
) -> Response {
    let device_id = match authorize(&headers) {
        Ok(d) => d,
        Err(r) => return r,
    };
    if transfer::note_progress(&device_id, &id, body.done, body.total, body.speed) {
        let _ = state.app.emit(
            "remote-access://transfer-progress",
            json!({
                "deviceId": device_id,
                "fileId": id,
                "done": body.done,
                "total": body.total,
                "speed": body.speed,
            }),
        );
    }
    json_ok(json!({ "ok": true }))
}
