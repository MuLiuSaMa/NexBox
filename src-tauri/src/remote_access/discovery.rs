//! 局域网设备发现应答器（手机主动搜索）。
//!
//! 控制服务开启时随主服务启动：在固定 UDP 端口监听安卓侧广播的探测包
//! （包含 magic `"nexbox.discover"`），单播回本机 HTTP 端口等信息，免手输 IP。
//! bind 失败仅记日志、不影响主服务；此时手机退化为扫码/手输 IP。

use serde_json::json;

async fn responder(socket: std::sync::Arc<tokio::net::UdpSocket>) {
    let mut buf = [0u8; 512];
    loop {
        let (len, peer) = match socket.recv_from(&mut buf).await {
            Ok(v) => v,
            Err(e) => {
                log::warn!("[RemoteAccess] 发现应答 recv 错误: {e}");
                break;
            }
        };
        // 控制与只读监控都关闭时不应答，避免噪声
        if !super::is_enabled() && !crate::remote_monitor::is_enabled() {
            continue;
        }
        let probe = String::from_utf8_lossy(&buf[..len]);
        if !probe.contains(super::DISCOVERY_PROBE_TOKEN) {
            continue;
        }
        let ip = crate::remote_monitor::local_ip().unwrap_or_default();
        let resp = json!({
            "service": "nexbox",
            "name": std::env::var("COMPUTERNAME").unwrap_or_default(),
            "version": env!("CARGO_PKG_VERSION"),
            "ip": ip,
            "tcpPort": super::http_port(),
            "controlEnabled": super::is_enabled(),
            "monitorEnabled": crate::remote_monitor::is_enabled(),
        });
        let _ = socket.send_to(resp.to_string().as_bytes(), peer).await;
    }
}

/// 由 `ensure_server` spawn；被 abort 时循环在 await 点退出、socket 随任务释放。
pub async fn run() {
    let socket = match tokio::net::UdpSocket::bind(("0.0.0.0", super::DISCOVERY_UDP_PORT)).await {
        Ok(s) => std::sync::Arc::new(s),
        Err(e) => {
            log::warn!(
                "[RemoteAccess] 发现端口 {} 绑定失败（不影响连接，退化为扫码/手输 IP）: {e}",
                super::DISCOVERY_UDP_PORT
            );
            return;
        }
    };
    log::info!("[RemoteAccess] 设备发现监听 UDP {}", super::DISCOVERY_UDP_PORT);
    responder(socket).await;
}
