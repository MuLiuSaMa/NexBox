//! 引擎事件适配层 —— 对齐上游 `crate::events` 的接口形态。
//!
//! 上游 [`events.rs`](https://github.com/zerx-lab/FluxDown) 的
//! `EngineEvent` 有 20+ 变体（BT/RSS/队列/插件…），NexBox 仅复刻 HTTP
//! 多线程加速链路，协调器源码实际构造的只有 [`EngineEvent::SegmentSplit`]，
//! 故此处仅保留该变体（字段与上游逐字一致）。后续若搬运更多链路，按需扩充。

/// 引擎运行期间产生的、宿主需要感知的事件。
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum EngineEvent {
    /// 动态分段拆分发生通知(IDM 风格协调器),实时发送以便 UI 播放拆分动画。
    /// 对应上游 `hub::signals::SegmentSplitEvent`。
    SegmentSplit {
        task_id: String,
        /// 被缩小的父分段索引。
        parent_index: i32,
        /// 拆分后父分段的新 end_byte。
        parent_new_end: i64,
        /// 新建子分段的索引。
        child_index: i32,
        /// 新子分段的起始字节(= 拆分点)。
        child_start: i64,
        /// 新子分段的结束字节(= 父分段原 end)。
        child_end: i64,
        /// 是否为主动拆分(true)还是抢救式/按需拆分(false)。
        is_proactive: bool,
        /// 拆分后的当前分段总数。
        total_segments: i32,
    },
}

/// 引擎事件的接收端,由宿主实现并注入。
///
/// # 契约(与上游一致)
///
/// `emit` 是**同步**方法,fire-and-forget 语义。实现**不得**执行阻塞操作或
/// 长时间持锁;任何异步/耗时工作必须由实现自行 `spawn`,不得让调用方等待。
pub trait EventSink: Send + Sync {
    /// 上报一个引擎事件。必须立即返回,不得阻塞或长时间持锁。
    fn emit(&self, event: EngineEvent);
}
