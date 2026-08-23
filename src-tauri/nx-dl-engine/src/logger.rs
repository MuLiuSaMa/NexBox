//! 日志宏适配层 —— 对齐上游 `crate::logger::log_info!` 的调用形态，
//! 落到 `log` crate（NexBox 侧经 tauri-plugin-log 输出）。

/// 与上游同名的信息级日志宏：`log_info!("[mod] msg {}", arg)`。
macro_rules! log_info {
    ($($arg:tt)*) => {
        ::log::info!($($arg)*)
    };
}
pub(crate) use log_info;
