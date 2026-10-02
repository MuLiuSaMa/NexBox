//! axum 路由与 handler：REST + WebSocket。
//!
//! 鉴权：控制类端点需 `Authorization: Bearer <token>`；仅 `/api/info`、`/api/pair` 免鉴权。
//! 服务关闭时统一返回 404；令牌无效 401；配对码错误/过期 403；需确认 428；爆破锁定 429。

use std::net::SocketAddr;
use std::time::Duration;

use axum::extract::connect_info::ConnectInfo;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{header::AUTHORIZATION, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Emitter;
use tokio::sync::broadcast;

use super::auth;
use super::models::{ActionReq, PairReq, QueryReq};
use super::registry;

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
    Router::new()
        .route("/api/info", get(info))
        .route("/api/pair", post(pair).delete(unpair))
        .route("/api/pair/request", post(pair_request))
        .route("/api/pair/request/:id", get(pair_request_status).delete(pair_request_cancel))
        .route("/api/capabilities", get(capabilities))
        .route("/api/query", post(query))
        .route("/api/action", post(action))
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

    loop {
        tokio::select! {
            _ = tick.tick() => {
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
