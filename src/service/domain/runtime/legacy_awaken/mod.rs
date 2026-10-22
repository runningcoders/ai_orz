//! 第一版思考路径：用新图引擎重排现有 `awaken` 链路
//!
//! ⚠️ 尚未接入调用方：整包临时豁免 `dead_code`。
//! 影子对比接上线（`runner` 有了真实调用点）后应移除下面这行。
#![allow(dead_code)]
//!
//! ## 定位
//!
//! 这是**替换的前置版本**：拓扑、节点职责、数据总线契约都按新框架写，
//! 但每个节点内部仍调用老链路的能力。**行为等价**，因此可以先存在、先对比，
//! 不必等到 `run_think_loop`（480 行）全部下沉才能上线。
//!
//! ## 三个阶段
//!
//! 1. **现在**：整体编排重排（本包），老 `awaken` 仍是唯一入口。
//! 2. **影子对比**：同一个消息跑两条链路，比耗时 / token / 退出原因 / 路径指纹。
//! 3. **下沉**：把 `run_think_loop` 内部（小脑路由、工具二分、8 个退出策略）
//!    逐块搬成独立节点，届时本包只剩拓扑，不再有「委托老函数」的痕迹。
//!
//! ## 与 pkg 的关系
//!
//! `pkg::think_path` 是通用引擎（零业务），本包是它的**第一个业务实例**，
//! 单向依赖，反向禁止。

pub mod backend;
pub mod blueprint;
pub mod context;
pub mod nodes;
pub mod runner;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
#[path = "integration_tests.rs"]
mod integration_tests;

#[allow(unused_imports)]
pub use runner::{LegacyAwakenOutcome, ShadowReport, run_legacy_awaken, shadow_run};
