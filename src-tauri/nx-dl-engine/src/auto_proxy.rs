//! auto_proxy 适配层 —— 上游 `ProxyMode::Auto` 直连/代理热切换的类型壳。
//!
//! NexBox 复刻链路恒以 `auto_proxy: None` 调用协调器（无代理自动切换），
//! 因此这里只需提供**可编译的最小类型面**：协调器源码中
//! `auto_proxy.map(AutoSwitchState::new)` 与 `TickObs { .. }` 的构造点
//! 在 `None` 分支下永不执行，但类型必须存在且签名逐字对齐。

use std::sync::Arc;

use crate::cdn::NodePool;
use crate::db::Db;
use crate::downloader::RequestSpec;
use crate::events::EventSink;

/// `ProxyMode::Auto` 热切换上下文（上游携带候选代理与 host 决策缓存；
/// NexBox 无此能力，保持不透明壳）。
pub struct AutoProxyCtx {
    _priv: (),
}

/// ramp tick 观察值 —— 字段与上游逐字一致（协调器 TickObs 构造点的形状）。
pub struct TickObs {
    pub throughput_bps: f64,
    pub alive: usize,
    pub remaining_bytes: i64,
    pub limiter_active: bool,
    pub conn_sensitive: bool,
}

/// 每任务的热切换状态机（上游 off-loop 采样；此处为 no-op 壳）。
pub struct AutoSwitchState {
    _priv: (),
}

impl AutoSwitchState {
    /// 与上游同签名的构造入口。
    pub fn new(_ctx: Arc<AutoProxyCtx>) -> Self {
        Self { _priv: () }
    }

    /// 每个 ramp tick 的采样/切换钩子。NexBox 恒直连，no-op。
    #[allow(clippy::too_many_arguments)]
    pub async fn on_ramp_tick(
        &mut self,
        _obs: TickObs,
        _nodes: &Arc<NodePool>,
        _db: &Db,
        _sink: &dyn EventSink,
        _task_id: &str,
        _url: &str,
        _spec: &RequestSpec,
        _etag: &str,
        _last_modified: &str,
    ) {
    }
}
