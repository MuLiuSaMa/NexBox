//! CDN 节点池适配层 —— 上游多节点镜像池的单节点退化版。
//!
//! 上游 [`node_pool.rs`](https://github.com/zerx-lab/FluxDown) 支持
//! 同资源多 IP 钉定租借 + 健康度 EWMA + 熔断踢除；NexBox 复刻链路暂不启用
//! 多节点聚合，恒走 [`NodePool::single`]（包裹任务 client，行为与上游单节点
//! 退化路径逐字节一致——这是上游 `NodePool::single` 的语义）。接口签名与
//! 协调器源码的调用面（`lease()` / `report()` / `is_pinned()` / `describe()` /
//! `is_node_attributable()`）逐字对齐。

use std::net::IpAddr;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use reqwest::Client;

use crate::downloader::DownloadError;
use crate::logger::log_info;

/// 默认 EWMA 吞吐先验（字节/秒），对齐上游 DEFAULT_EWMA_BPS。
const DEFAULT_EWMA_BPS: f64 = 2.0 * 1024.0 * 1024.0;

/// 单槽位状态（SYS 节点：无钉定 IP，client 恒存在）。
struct NodeSlot {
    ip: Option<IpAddr>,
    client: Client,
    ewma_bps: f64,
    fail_streak: u32,
    bytes_done: u64,
}

/// 单节点退化池。见模块文档。
pub struct NodePool {
    inner: StdMutex<NodeSlot>,
}

/// 一次段派工的节点租约。持有期间不计数（单节点无需并发记账）；Drop 归还
/// 为 no-op —— 与上游「Drop 归还、report 只管健康度」的解耦语义一致。
pub struct NodeLease {
    client: Client,
    ip: Option<IpAddr>,
}

impl NodeLease {
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// 是否钉定节点（非 SYS）。单节点池恒 false → 错误翻译路径永不生效。
    pub fn is_pinned(&self) -> bool {
        self.ip.is_some()
    }

    /// 诊断用节点描述。
    pub fn describe(&self) -> String {
        match self.ip {
            Some(ip) => ip.to_string(),
            None => "SYS".to_string(),
        }
    }
}

impl NodePool {
    /// 单节点退化：包裹现有任务 client，无钉定，零事件。与上游同名同义。
    pub fn single(client: Client) -> Arc<Self> {
        Arc::new(Self {
            inner: StdMutex::new(NodeSlot {
                ip: None,
                client,
                ewma_bps: DEFAULT_EWMA_BPS,
                fail_streak: 0,
                bytes_done: 0,
            }),
        })
    }

    /// 是否多节点池。单节点退化恒 false。
    pub fn is_multi(&self) -> bool {
        false
    }

    /// 永不阻塞、永不失败地租借一个节点（单节点池恒返回 SYS 租约）。
    pub fn lease(self: &Arc<Self>) -> NodeLease {
        let slot = self.inner.lock().map(|s| NodeLease {
            client: s.client.clone(),
            ip: s.ip,
        });
        match slot {
            Ok(lease) => lease,
            Err(_) => unreachable!("node pool mutex poisoned"),
        }
    }

    /// 段级健康度回报。单节点池仅累计字节数 + 维护 EWMA/fail_streak
    /// （供诊断；永不踢除 SYS 节点——与上游一致）。
    pub fn report(
        &self,
        lease: &NodeLease,
        bytes: u64,
        elapsed: Duration,
        outcome: Result<(), &DownloadError>,
    ) {
        let _ = lease;
        if let Ok(mut slot) = self.inner.lock() {
            match outcome {
                Ok(()) => {
                    slot.fail_streak = 0;
                    slot.bytes_done = slot.bytes_done.saturating_add(bytes);
                    if bytes >= 256 * 1024 && !elapsed.is_zero() {
                        let rate = bytes as f64 / elapsed.as_secs_f64();
                        slot.ewma_bps = 0.7 * slot.ewma_bps + 0.3 * rate;
                    }
                }
                Err(e) => {
                    slot.fail_streak += 1;
                    slot.ewma_bps *= 0.5;
                    log_info!("[cdn-pool] segment report err (SYS, no kick): {}", e);
                }
            }
        }
    }
}

/// 钉定节点的可归因错误翻译门。单节点池无钉定租约（`lease.is_pinned()`
/// 恒 false），协调器调用点短路于此；恒 false 与上游 SYS 节点语义完全一致。
pub fn is_node_attributable(_e: &DownloadError) -> bool {
    false
}
