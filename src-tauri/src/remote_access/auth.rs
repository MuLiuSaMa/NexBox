//! 配对码 + 设备令牌的鉴权与持久化。
//!
//! - 配对码：6 位数字，短时效（默认 5 分钟），仅用于一次性换取长期设备令牌。
//! - 设备令牌：`nbx_` + URL-safe base64 随机（不含 `+ / =`，便于直接放进 WS query）；服务端仅存 `sha1(server_secret || ":" || token)` 哈希。
//! - 校验采用常量时间比较，防时序侧信道；配对失败按 IP 计数锁定，防暴力猜码。

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use base64::Engine;
use parking_lot::Mutex;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use tauri::Manager;
use tokio::sync::broadcast;

const PAIR_CODE_TTL_MS: u64 = 5 * 60 * 1000;
const MAX_DEVICES: usize = 10;
const MAX_FAILS: u32 = 5;
const LOCK_MS: u64 = 60 * 1000;

/// 「请求配对」待审批的有效期：手机发起后，PC 端要在这么久之内点确认。
const PAIR_REQ_TTL_MS: u64 = 120 * 1000;
/// 批准/拒绝后，结果保留这么久供手机轮询取走。
const PAIR_REQ_RESULT_TTL_MS: u64 = 60 * 1000;
/// 同时挂起的请求上限，避免被刷。
const MAX_PENDING_REQ: usize = 5;

#[derive(Serialize, Deserialize, Clone)]
struct Device {
    id: String,
    /// 手机端稳定身份（ANDROID_ID，取不到时退回安装 UUID）。老数据无此字段时按空串处理，不参与去重。
    #[serde(default)]
    uid: String,
    name: String,
    token_hash: String,
    created_at: u64,
    last_seen: u64,
    /// 最近一次连本机时看到的手机端 IP（DHCP 会变，每次上线刷新）。老数据无此字段时为空串。
    #[serde(default)]
    ip: String,
}

/// 待审批的配对请求（对外视图，不含令牌）。
#[derive(Serialize, Clone)]
pub struct PairRequestView {
    pub id: String,
    pub ip: String,
    pub device_name: String,
    pub created_at: u64,
    pub expires_at: u64,
}

/// 手机轮询的结果。
#[derive(Serialize)]
pub struct PairRequestStatus {
    /// pending | approved | denied | expired | unknown
    pub status: String,
    pub token: Option<String>,
    pub device_id: Option<String>,
}

/// 待审批请求的内部记录。
struct Pending {
    view: PairRequestView,
    /// 手机端稳定身份，批准时用于对同一台手机去重（空串表示不参与去重）
    device_uid: String,
    /// None = 待审批；Some(true) = 已批准；Some(false) = 已拒绝
    resolved: Option<bool>,
    /// 批准后签发的结果 (device_id, 明文令牌)，等手机轮询取走后即失效
    issued: Option<(String, String)>,
}

#[derive(Serialize, Deserialize, Default)]
struct Persisted {
    /// 服务端随机密钥（hex），用于令牌哈希，跨重启保持稳定。
    server_secret: String,
    devices: Vec<Device>,
    pairing_code: Option<String>,
    code_expires_at: u64,
}

#[derive(Default)]
struct Runtime {
    persisted: Persisted,
    /// 每 IP 的配对失败计数与锁定截止（仅内存）。
    fails: HashMap<String, (u32, u64)>,
    last_persist: u64,
    /// 待审批的配对请求（仅内存，重启即失效）。
    pending: Vec<Pending>,
}

static DB: Mutex<Option<Runtime>> = Mutex::new(None);

/// 当前持有活动 WebSocket 的设备 id（供首页卡片展示「已连接」）。
static CONNECTED: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// 每设备的「踢下线」广播通道；撤销/顶号时用它立即断开其活动连接。
static CONNS: Mutex<Option<HashMap<String, broadcast::Sender<()>>>> = Mutex::new(None);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
    .unwrap_or(0)
}

fn gen_secret_hex() -> String {
    let mut rng = rand::thread_rng();
    let mut b = [0u8; 32];
    rng.fill(&mut b);
    hex::encode(b)
}

fn gen_code() -> String {
    let mut rng = rand::thread_rng();
    format!("{:06}", rng.gen_range(0..1_000_000))
}

fn gen_token() -> String {
    let mut rng = rand::thread_rng();
    let mut b = [0u8; 32];
    rng.fill(&mut b);
    format!("nbx_{}", B64.encode(b))
}

fn hash_token(secret_hex: &str, token: &str) -> String {
    let secret = hex::decode(secret_hex).unwrap_or_default();
    let mut h = Sha1::new();
    h.update(&secret);
    h.update(b":");
    h.update(token.as_bytes());
    hex::encode(h.finalize())
}

/// 常量时间字符串比较。
fn const_eq(a: &str, b: &str) -> bool {
    let (ab, bb) = (a.as_bytes(), b.as_bytes());
    if ab.len() != bb.len() {
        return false;
    }
    let mut diff = 0u8;
    for i in 0..ab.len() {
        diff |= ab[i] ^ bb[i];
    }
    diff == 0
}

fn persisted_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    Some(dir.join("remote-access").join("paired-devices.json"))
}

/// init 时载入历史配对设备（不重开服务）。
pub fn load_persisted(app: &tauri::AppHandle) {
    let loaded = match persisted_path(app) {
        Some(p) => std::fs::read_to_string(&p)
            .ok()
            .and_then(|s| serde_json::from_str::<Persisted>(&s).ok()),
        None => None,
    };
    let persisted = loaded.unwrap_or_else(|| Persisted {
        server_secret: gen_secret_hex(),
        devices: Vec::new(),
        pairing_code: None,
        code_expires_at: 0,
    });
    let mut guard = DB.lock();
    let rt = guard.get_or_insert_with(Runtime::default);
    rt.persisted = persisted;
    if rt.persisted.server_secret.is_empty() {
        rt.persisted.server_secret = gen_secret_hex();
    }
}

/// 写盘（节流：最多每 5 秒一次，revoke 等关键操作强制写）。
fn persist_locked(rt: &mut Runtime, force: bool) {
    let now = now_ms();
    if !force && now.saturating_sub(rt.last_persist) < 5_000 {
        return;
    }
    rt.last_persist = now;
    let app = match super::app_handle() {
        Some(a) => a,
        None => return,
    };
    let path = match persisted_path(&app) {
        Some(p) => p,
        None => return,
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_string_pretty(&rt.persisted) {
        Ok(content) => {
            if let Err(e) = std::fs::write(&path, content) {
                log::warn!("[RemoteAccess] 持久化失败: {e}");
            }
        }
        Err(e) => log::warn!("[RemoteAccess] 序列化失败: {e}"),
    }
}

/// 关键操作后的强制写盘。
pub fn persist(_app: &tauri::AppHandle) {
    let mut guard = DB.lock();
    if let Some(rt) = guard.as_mut() {
        persist_locked(rt, true);
    }
}

/// 刷新配对码（开启服务、手动刷新、或过期后自动续期时调用）。
pub fn rotate_code() {
    let mut guard = DB.lock();
    if let Some(rt) = guard.as_mut() {
        rt.persisted.pairing_code = Some(gen_code());
        rt.persisted.code_expires_at = now_ms() + PAIR_CODE_TTL_MS;
        persist_locked(rt, true);
    }
}

/// 返回当前有效配对码与到期时间；过期则自动生成新的。
pub fn current_code_info() -> (Option<String>, u64) {
    let mut guard = DB.lock();
    if let Some(rt) = guard.as_mut() {
        if rt.persisted.code_expires_at <= now_ms() || rt.persisted.pairing_code.is_none() {
            rt.persisted.pairing_code = Some(gen_code());
            rt.persisted.code_expires_at = now_ms() + PAIR_CODE_TTL_MS;
            persist_locked(rt, false);
        }
        return (rt.persisted.pairing_code.clone(), rt.persisted.code_expires_at);
    }
    (None, 0)
}

/// 供 cmd_get：确保有有效码。
pub fn ensure_valid_code() {
    let _ = current_code_info();
}

#[derive(Debug)]
pub enum PairError {
    /// 配对码错误或已过期
    CodeInvalid,
    /// 失败过多被锁定
    Locked,
    /// 设备数量达上限
    Limit,
}

/// 用配对码换取设备令牌。成功返回 (device_id, 明文令牌)。
pub fn try_pair(
    ip: &str,
    code: &str,
    device_name: Option<String>,
    device_uid: Option<String>,
) -> Result<(String, String), PairError> {
    let mut guard = DB.lock();
    let rt = guard.as_mut().ok_or(PairError::CodeInvalid)?;
    let now = now_ms;

    // 锁定检查
    if let Some((_, lock_until)) = rt.fails.get(ip) {
        if *lock_until > now() {
            return Err(PairError::Locked);
        }
    }

    // 码有效性
    let code_ok = rt.persisted.code_expires_at > now()
        && rt
            .persisted
            .pairing_code
            .as_deref()
            .map(|exp| const_eq(exp, code))
            .unwrap_or(false);

    if !code_ok {
        let (cnt, _) = rt.fails.get(ip).cloned().unwrap_or((0, 0));
        let cnt = cnt + 1;
        let lock_until = if cnt >= MAX_FAILS {
            now() + LOCK_MS
        } else {
            0
        };
        rt.fails.insert(ip.to_string(), (if cnt >= MAX_FAILS { 0 } else { cnt }, lock_until));
        // 达阈值后清掉当前码并换新，避免继续被撞
        if cnt >= MAX_FAILS {
            rt.persisted.pairing_code = Some(gen_code());
            rt.persisted.code_expires_at = now() + PAIR_CODE_TTL_MS;
        }
        return Err(PairError::CodeInvalid);
    }

    // 成功：清除失败计数
    rt.fails.remove(ip);

    let (id, token) = mint_device_locked(rt, device_name, device_uid, ip)?;
    // 用后即焚：换新码
    rt.persisted.pairing_code = Some(gen_code());
    rt.persisted.code_expires_at = now() + PAIR_CODE_TTL_MS;
    persist_locked(rt, true);
    Ok((id, token))
}

/// 签发/复用一个设备令牌（调用方需已持有 DB 锁）。
/// 配对码路径与"请求配对"路径共用，保证两条通道的设备语义完全一致。
/// 若携带的 device_uid 与已配对设备相同，则**更新原记录**（换发新令牌、刷新名称/last_seen）
/// 而非新建，从根源避免同一台手机在 PC 端出现多个重复已配对设备。
fn mint_device_locked(
    rt: &mut Runtime,
    device_name: Option<String>,
    device_uid: Option<String>,
    peer_ip: &str,
) -> Result<(String, String), PairError> {
    let name = device_name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Android".to_string());
    let uid = device_uid
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_default();

    let token = gen_token();
    let token_hash = hash_token(&rt.persisted.server_secret, &token);
    let now = now_ms();

    // 命中同一台手机（uid 非空且相同）：更新而非新建，且不受数量上限约束
    if !uid.is_empty() {
        if let Some(existing) = rt.persisted.devices.iter_mut().find(|d| d.uid == uid) {
            existing.name = name;
            existing.token_hash = token_hash;
            existing.last_seen = now;
            if !peer_ip.is_empty() {
                existing.ip = peer_ip.to_string();
            }
            log::info!("[RemoteAccess] 复用已配对设备 id={} uid={uid}（重配去重）", existing.id);
            return Ok((existing.id.clone(), token));
        }
    }

    if rt.persisted.devices.len() >= MAX_DEVICES {
        return Err(PairError::Limit);
    }
    let id: String = {
        let mut rng = rand::thread_rng();
        format!("{:08x}", rng.gen_range(0..u32::MAX))
    };
    rt.persisted.devices.push(Device {
        id: id.clone(),
        uid,
        name,
        token_hash,
        created_at: now,
        last_seen: now,
        ip: peer_ip.to_string(),
    });
    Ok((id, token))
}

// ───────────────────────── 请求配对（免配对码，PC 端人工批准） ─────────────────────────

/// 清掉已过期/已取走的请求，防止内存堆积。
fn purge_requests(rt: &mut Runtime) {
    let now = now_ms();
    rt.pending.retain(|p| {
        let taken = p.resolved == Some(true) && p.issued.is_none();
        !taken && p.view.expires_at > now
    });
}

/// 手机发起配对请求：不校验配对码，改为等 PC 端用户在界面上点「允许」。
pub fn create_request(
    ip: &str,
    device_name: Option<String>,
    device_uid: Option<String>,
) -> Result<PairRequestView, PairError> {
    let mut guard = DB.lock();
    let rt = guard.as_mut().ok_or(PairError::CodeInvalid)?;
    purge_requests(rt);

    let uid = device_uid
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_default();
    // 已配对过的同一台手机重新请求时不受数量上限约束（批准后会走更新而非新建）
    let already_paired = !uid.is_empty() && rt.persisted.devices.iter().any(|d| d.uid == uid);
    if !already_paired && rt.persisted.devices.len() >= MAX_DEVICES {
        return Err(PairError::Limit);
    }
    if rt.pending.iter().filter(|p| p.resolved.is_none()).count() >= MAX_PENDING_REQ {
        return Err(PairError::Locked);
    }

    let id = {
        let mut rng = rand::thread_rng();
        format!("{:016x}{:016x}", rng.gen::<u64>(), rng.gen::<u64>())
    };
    let created = now_ms();
    let name = device_name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Android".to_string());

    let view = PairRequestView {
        id,
        ip: ip.to_string(),
        device_name: name,
        created_at: created,
        expires_at: created + PAIR_REQ_TTL_MS,
    };
    rt.pending.push(Pending {
        view: view.clone(),
        device_uid: uid,
        resolved: None,
        issued: None,
    });
    log::info!("[RemoteAccess] 收到配对请求 ip={ip} name={}", view.device_name);
    Ok(view)
}

/// 待审批列表（供 PC 端界面展示）。
pub fn pending_requests() -> Vec<PairRequestView> {
    let mut guard = DB.lock();
    let Some(rt) = guard.as_mut() else {
        return Vec::new();
    };
    purge_requests(rt);
    rt.pending
        .iter()
        .filter(|p| p.resolved.is_none())
        .map(|p| p.view.clone())
        .collect()
}

/// PC 端用户批准/拒绝。批准时立刻签发令牌，等手机轮询取走。
pub fn resolve_request(id: &str, approve: bool) -> Result<(), PairError> {
    let mut guard = DB.lock();
    let rt = guard.as_mut().ok_or(PairError::CodeInvalid)?;
    purge_requests(rt);

    let Some(pos) = rt.pending.iter().position(|p| p.view.id == id) else {
        return Err(PairError::CodeInvalid);
    };
    if rt.pending[pos].resolved.is_some() {
        return Err(PairError::CodeInvalid);
    }

    if approve {
        let name = rt.pending[pos].view.device_name.clone();
        let uid = rt.pending[pos].device_uid.clone();
        let ip = rt.pending[pos].view.ip.clone();
        let (device_id, token) = mint_device_locked(rt, Some(name), Some(uid), &ip)?;
        rt.pending[pos].issued = Some((device_id, token));
        rt.pending[pos].resolved = Some(true);
        persist_locked(rt, true);
        log::info!("[RemoteAccess] 配对请求已批准 id={id}");
    } else {
        rt.pending[pos].resolved = Some(false);
        log::info!("[RemoteAccess] 配对请求已拒绝 id={id}");
    }
    // 结果多留一会儿，给手机轮询取走的机会
    rt.pending[pos].view.expires_at = now_ms() + PAIR_REQ_RESULT_TTL_MS;
    Ok(())
}

/// 手机取消请求（如退出页面）。
pub fn cancel_request(id: &str) {
    let mut guard = DB.lock();
    if let Some(rt) = guard.as_mut() {
        rt.pending.retain(|p| p.view.id != id);
    }
}

/// 手机轮询结果。令牌只在**同一 IP** 轮询时下发，避免请求 ID 泄漏后被别的机器取走。
pub fn poll_request(id: &str, peer_ip: &str) -> PairRequestStatus {
    let mut guard = DB.lock();
    let Some(rt) = guard.as_mut() else {
        return PairRequestStatus { status: "unknown".into(), token: None, device_id: None };
    };
    purge_requests(rt);

    let Some(pos) = rt.pending.iter().position(|p| p.view.id == id) else {
        return PairRequestStatus { status: "expired".into(), token: None, device_id: None };
    };
    if rt.pending[pos].view.ip != peer_ip {
        return PairRequestStatus { status: "unknown".into(), token: None, device_id: None };
    }

    match rt.pending[pos].resolved {
        None => PairRequestStatus { status: "pending".into(), token: None, device_id: None },
        Some(false) => PairRequestStatus { status: "denied".into(), token: None, device_id: None },
        Some(true) => match rt.pending[pos].issued.take() {
            // 取走即失效：同一次请求只能换一次令牌
            Some((device_id, token)) => PairRequestStatus {
                status: "approved".into(),
                token: Some(token),
                device_id: Some(device_id),
            },
            None => PairRequestStatus { status: "expired".into(), token: None, device_id: None },
        },
    }
}

/// 校验令牌，命中返回 device_id 并刷新 last_seen。
pub fn auth_token(token: &str) -> Option<String> {
    let mut guard = DB.lock();
    let rt = guard.as_mut()?;
    let hash = hash_token(&rt.persisted.server_secret, token);
    let mut found: Option<String> = None;
    for d in rt.persisted.devices.iter_mut() {
        if const_eq(&d.token_hash, &hash) {
            d.last_seen = now_ms();
            found = Some(d.id.clone());
            break;
        }
    }
    if found.is_some() {
        persist_locked(rt, false);
    }
    found
}

/// 撤销设备（按 id 删除）；若其正连着则立即踢下线。
pub fn revoke(device_id: &str) {
    {
        let mut guard = DB.lock();
        if let Some(rt) = guard.as_mut() {
            rt.persisted.devices.retain(|d| d.id != device_id);
        }
    }
    kick(device_id);
}

// ───────────────────────── 活动连接跟踪（供首页卡片展示 / 断开） ─────────────────────────

/// 设备建立一条 WS：登记在线并返回「踢下线」信号订阅者（调用方需在 WS 生命周期内持有）。
/// 顺带刷新该设备的来源 IP 与最后在线时间——DHCP 会让手机换 IP，以这里看到的对端地址为准。
pub fn register_connection(id: &str, peer_ip: &str) -> broadcast::Receiver<()> {
    let rx = {
        let mut guard = CONNS.lock();
        let map = guard.get_or_insert_with(HashMap::new);
        let (tx, rx) = broadcast::channel(1);
        map.insert(id.to_string(), tx);
        rx
    };
    {
        let mut c = CONNECTED.lock();
        if !c.iter().any(|x| x == id) {
            c.push(id.to_string());
        }
    }
    {
        let mut guard = DB.lock();
        if let Some(rt) = guard.as_mut() {
            if let Some(d) = rt.persisted.devices.iter_mut().find(|d| d.id == id) {
                if !peer_ip.is_empty() {
                    d.ip = peer_ip.to_string();
                }
                d.last_seen = now_ms();
                persist_locked(rt, false);
            }
        }
    }
    rx
}

/// WS 结束：移除在线标记与 kick 通道。
pub fn unregister_connection(id: &str) {
    {
        let mut guard = CONNS.lock();
        if let Some(map) = guard.as_mut() {
            map.remove(id);
        }
    }
    let mut c = CONNECTED.lock();
    c.retain(|x| x != id);
}

/// 主动断开某设备的实时连接（撤销时调用）：通知其 WS 立即结束。
fn kick(id: &str) {
    let guard = CONNS.lock();
    if let Some(map) = guard.as_ref() {
        if let Some(tx) = map.get(id) {
            let _ = tx.send(());
        }
    }
}

fn is_connected(id: &str) -> bool {
    CONNECTED.lock().iter().any(|x| x == id)
}

pub fn device_count() -> usize {
    DB.lock().as_ref().map(|rt| rt.persisted.devices.len()).unwrap_or(0)
}

/// 已配对设备列表（供 PC 端管理）。
pub fn devices_view() -> Vec<crate::remote_access::models::PairedDeviceView> {
    let guard = DB.lock();
    guard
        .as_ref()
        .map(|rt| {
            rt.persisted
                .devices
                .iter()
                .map(|d| crate::remote_access::models::PairedDeviceView {
                    device_id: d.id.clone(),
                    name: d.name.clone(),
                    created_at: d.created_at,
                    last_seen: d.last_seen,
                    ip: d.ip.clone(),
                    connected: is_connected(&d.id),
                })
                .collect()
        })
        .unwrap_or_default()
}
