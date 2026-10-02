use serde::{Deserialize, Serialize};

/// 前端弹窗使用的接入状态快照。
#[derive(Serialize, Clone)]
pub struct AccessInfo {
    pub enabled: bool,
    pub ip: Option<String>,
    pub port: u16,
    pub url: Option<String>,
    /// 当前有效配对码（6 位数字）；未开启或服务关闭时为 None。
    pub pairing_code: Option<String>,
    /// 配对码到期时间（epoch 毫秒）。
    pub code_expires_at: u64,
    /// 已配对设备数量。
    pub device_count: usize,
}

/// 已配对设备视图（不含明文/哈希令牌）。
#[derive(Serialize, Clone)]
pub struct PairedDeviceView {
    pub device_id: String,
    pub name: String,
    pub created_at: u64,
    pub last_seen: u64,
    /// 最近一次连接本机的手机端 IP，未知时为空串。
    pub ip: String,
    /// 当前是否有活动 WebSocket（即手机此刻正连着）。
    pub connected: bool,
}

/// POST /api/pair 请求体。
#[derive(Deserialize)]
pub struct PairReq {
    pub code: String,
    #[serde(default)]
    pub device_name: Option<String>,
    /// 手机端稳定身份，PC 端据此对同一台手机去重，避免重复已配对设备
    #[serde(default)]
    pub device_uid: Option<String>,
}

/// POST /api/query 请求体。
#[derive(Deserialize)]
pub struct QueryReq {
    pub key: String,
    #[serde(default)]
    pub args: Option<serde_json::Value>,
}

/// POST /api/action 请求体。
#[derive(Deserialize)]
pub struct ActionReq {
    pub key: String,
    #[serde(default)]
    pub args: Option<serde_json::Value>,
    /// needs_confirm 动作必须携带 confirm=true。
    #[serde(default)]
    pub confirm: bool,
}
