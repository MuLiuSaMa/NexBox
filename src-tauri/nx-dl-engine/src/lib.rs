//! nx-dl-engine —— NexBox 多线程下载加速引擎。
//!
//! 核心算法逐字搬运自 FluxDown（AGPL-3.0，https://github.com/zerx-lab/FluxDown），
//! 依据 GPL-3.0 §13 并入本项目；所有带 NOTICE 头的文件修改须继续遵守 AGPL-3.0。
//!
//! 模块布局与上游 `fluxdown_engine` 完全同构，使搬运文件内的
//! `crate::xxx` 路径零改动即可编译：
//! - 原样搬运：[`segment_advisor`] / [`speed_limiter`] / [`segment_coordinator`]
//!   / [`proxy_config`] / [`downloader`]（裁剪版，仅保留协调器所需原语）
//! - 适配层（本仓库重写，接口与上游对齐）：[`db`]（JSON 持久化替代 SQLite）、
//!   [`events`]、[`cdn`]（单节点退化池）、[`auto_proxy`]（类型壳）、[`logger`]

pub mod auto_proxy;
pub mod cdn;
pub mod db;
pub mod downloader;
pub mod events;
pub mod logger;
pub mod proxy_config;
pub mod segment_advisor;
pub mod segment_coordinator;
pub mod speed_limiter;

/// 重导出引擎使用的 reqwest（0.12）——宿主驱动层构造/传递 Client 时
/// 必须用此版本，避免与主工程的 reqwest 0.11 类型混淆。
pub use reqwest;
