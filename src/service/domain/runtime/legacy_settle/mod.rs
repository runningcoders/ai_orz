//! 沉淀路径实例：用新图引擎承载「扫完所有未沉淀记忆」
//!
//! ## 定位
//!
//! `RuntimeAwakening::settle_pending` 的**执行体**。与 `legacy_awaken` 共用同一套
//! `pkg::think_path` 引擎与 Provider 实现，拓扑按分批语义重新编排：
//!
//! ```text
//! prepare ──ready──▶ settle ──next──▶ prepare
//!    │                  │
//!    └──empty──▶ finalize ◀──blocked──┘
//! ```
//!
//! ## 与 awaken 的对称关系
//!
//! | 维度 | awaken | settle |
//! |------|--------|--------|
//! | 触发 | 外部消息 | 定时任务 / 神经工具 |
//! | 状态 | Busy | Resting |
//! | 批次 | 单次思考（溢出才回环） | 按上下文预算分批扫队列 |
//! | Trace | 一条 | **每批一条** |
//! | 终态 | Final / 溢出 / 超轮次 / 取消 | 只有 Final 才标记 Settled |
//!
//! ## 单向依赖
//!
//! `pkg::think_path`（通用引擎）← 本包；`legacy_awaken`（Provider 实现）← 本包。
//! 反向依赖一律禁止。

pub mod backend;
pub mod blueprint;
pub mod context;
pub mod nodes;
pub mod pending;
pub mod runner;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
#[path = "integration_tests.rs"]
mod integration_tests;
