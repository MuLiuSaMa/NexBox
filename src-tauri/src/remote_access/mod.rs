//! 局域网远程接入网关（安卓原生 App 对接）
//!
//! 在既有只读远程监控 `remote_monitor` 之上，提供"配对码 + 设备令牌"鉴权的
//! 远程控制能力：结构化 REST + WebSocket JSON API，以及局域网 UDP 设备发现。
//! 安全模型：
//! - 仅监听局域网；控制服务默认关闭，需用户在弹窗手动开启。
//! - 只暴露"精选安全白名单"动作/查询（见 `registry.rs` / `actions.rs`），
//!   危险项（注册表批量、驱动装卸、卸载、删除、网络重置等）从不注册、天然不可达。
//! - 首次用短时效 6 位配对码换取长期随机设备令牌；令牌仅存哈希、常量时间比较；PC 端可查看/撤销。

mod actions;
mod api;
mod auth;
mod discovery;
mod models;
mod registry;

use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::OnceLock;

use parking_lot::Mutex;
use tauri::Manager;

use models::AccessInfo;
use models::PairedDeviceView;

/// 局域网设备发现固定端口（PC / 安卓两侧写死同一常量）。
pub const DISCOVERY_UDP_PORT: u16 = 45689;

/// 安卓探测包中用于识别的关键子串（安卓广播含此串的包，PC 单播回应答）。
pub const DISCOVERY_PROBE_TOKEN: &str = "nexbox.discover";

static PORT: AtomicU16 = AtomicU16::new(0);
static ENABLED: AtomicBool = AtomicBool::new(false);
static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

/// HTTP 服务与发现应答器的任务句柄，退出时 abort 释放端口。
static SERVER_TASK: Mutex<Option<tokio::task::JoinHandle<()>>> = Mutex::new(None);
static DISCOVERY_TASK: Mutex<Option<tokio::task::JoinHandle<()>>> = Mutex::new(None);

/// 供 handler 内部（发现、WS）取用 AppHandle 的全局引用（照搬 audio_proxy 范式）。
fn app_handle() -> Option<tauri::AppHandle> {
    APP_HANDLE.get().cloned()
}

/// 在 setup 阶段注入 AppHandle 并载入历史配对设备（不自动重开服务，与 remote_monitor 一致）。
pub fn init(app: &tauri::AppHandle) {
    let _ = APP_HANDLE.set(app.clone());
    auth::load_persisted(app);
}

/// 远程控制服务当前是否开启。
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// 正在监听的 HTTP 端口，0 表示未运行。
pub fn http_port() -> u16 {
    PORT.load(Ordering::Relaxed)
}

/// 启动 HTTP 服务与发现应答器（幂等）。
async fn ensure_server() -> Result<(), String> {
    if PORT.load(Ordering::Relaxed) > 0 {
        return Ok(());
    }

    let app = app_handle().ok_or("远程接入服务尚未初始化（AppHandle 缺失）")?;

    let (tx, _rx) = tokio::sync::broadcast::channel::<String>(32);
    let state = api::AppState {
        app: app.clone(),
        tx: tx.clone(),
    };
    api::set_bus(tx);

    let router = api::router(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:0")
        .await
        .map_err(|e| format!("绑定远程接入端口失败: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("获取监听地址失败: {e}"))?
        .port();
    PORT.store(port, Ordering::Relaxed);

    *SERVER_TASK.lock() = Some(tokio::spawn(async move {
        log::info!("[RemoteAccess] HTTP 服务监听 0.0.0.0:{port}");
        let result = axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await;
        if let Err(e) = result {
            log::error!("[RemoteAccess] HTTP 服务错误: {e}");
        }
    }));

    // 设备发现应答器（失败不影响主服务）
    if DISCOVERY_TASK.lock().is_none() {
        *DISCOVERY_TASK.lock() = Some(tokio::spawn(discovery::run()));
    }

    Ok(())
}

/// 关闭服务并释放端口，清空运行态；保留已配对设备与令牌哈希。
pub fn shutdown() {
    if let Some(h) = SERVER_TASK.lock().take() {
        h.abort();
    }
    if let Some(h) = DISCOVERY_TASK.lock().take() {
        h.abort();
    }
    ENABLED.store(false, Ordering::Relaxed);
    PORT.store(0, Ordering::Relaxed);
    log::info!("[RemoteAccess] 已关闭远程控制服务");
}

/// 尽力添加防火墙入站规则（TCP 程序规则 + UDP 发现端口规则）。非致命，失败忽略。
fn try_add_firewall_rules() {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return,
    };
    let exe_str = exe.to_string_lossy().to_string();

    let spawn_hidden = |args: Vec<String>| {
        let mut cmd = std::process::Command::new("netsh");
        cmd.args(&args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let _ = cmd.spawn();
    };

    // TCP：按程序放行（HTTP 端口随机，程序规则覆盖所有端口）
    spawn_hidden(vec![
        "advfirewall".into(),
        "firewall".into(),
        "add".into(),
        "rule".into(),
        "name=NexBox Remote Access TCP".into(),
        "dir=in".into(),
        "action=allow".into(),
        "protocol=TCP".into(),
        format!("program={exe_str}"),
    ]);
    // UDP：按固定发现端口放行，否则手机搜不到设备
    spawn_hidden(vec![
        "advfirewall".into(),
        "firewall".into(),
        "add".into(),
        "rule".into(),
        "name=NexBox Remote Access UDP".into(),
        "dir=in".into(),
        "action=allow".into(),
        "protocol=UDP".into(),
        format!("localport={DISCOVERY_UDP_PORT}"),
    ]);
}

fn build_info() -> AccessInfo {
    let port = PORT.load(Ordering::Relaxed);
    let ip = crate::remote_monitor::local_ip();
    let url = match (&ip, port) {
        (Some(ip), p) if p > 0 => Some(format!("http://{ip}:{p}/")),
        _ => None,
    };
    let (pairing_code, code_expires_at) = if ENABLED.load(Ordering::Relaxed) {
        auth::current_code_info()
    } else {
        (None, 0)
    };
    AccessInfo {
        enabled: ENABLED.load(Ordering::Relaxed),
        ip,
        port,
        url,
        pairing_code,
        code_expires_at,
        device_count: auth::device_count(),
    }
}

// ───────────────────────── Tauri commands ─────────────────────────

#[tauri::command]
pub async fn cmd_get_remote_access() -> Result<AccessInfo, String> {
    // 若已过期则顺带刷新一次配对码，保证前端展示始终有效
    if ENABLED.load(Ordering::Relaxed) {
        auth::ensure_valid_code();
    }
    Ok(build_info())
}

#[tauri::command]
pub async fn cmd_enable_remote_access(on: bool) -> Result<AccessInfo, String> {
    if on {
        ensure_server().await?;
        ENABLED.store(true, Ordering::Relaxed);
        auth::rotate_code();
        try_add_firewall_rules();
    } else {
        shutdown();
    }
    Ok(build_info())
}

#[tauri::command]
pub async fn cmd_rotate_pairing_code() -> Result<AccessInfo, String> {
    auth::rotate_code();
    Ok(build_info())
}

#[tauri::command]
pub async fn cmd_list_paired_devices(app: tauri::AppHandle) -> Result<Vec<PairedDeviceView>, String> {
    let _ = app;
    Ok(auth::devices_view())
}

#[tauri::command]
pub async fn cmd_revoke_device(app: tauri::AppHandle, device_id: String) -> Result<Vec<PairedDeviceView>, String> {
    auth::revoke(&device_id);
    auth::persist(&app);
    Ok(auth::devices_view())
}

// ───────── 请求配对（免配对码）的 PC 端确认 ─────────

/// 当前待审批的配对请求，供弹窗展示。
#[tauri::command]
pub async fn cmd_list_pair_requests() -> Result<Vec<auth::PairRequestView>, String> {
    Ok(auth::pending_requests())
}

/// 用户点了「允许 / 拒绝」。返回处理完剩下的待审批列表。
#[tauri::command]
pub async fn cmd_resolve_pair_request(
    app: tauri::AppHandle,
    id: String,
    approve: bool,
) -> Result<Vec<auth::PairRequestView>, String> {
    auth::resolve_request(&id, approve).map_err(|e| format!("{e:?}"))?;
    auth::persist(&app);
    Ok(auth::pending_requests())
}

// 保证 `Manager` trait 在需要时被引用（app_handle().path() 在 auth 持久化内使用）。
#[allow(dead_code)]
fn _manager_marker(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_data_dir().ok()
}
