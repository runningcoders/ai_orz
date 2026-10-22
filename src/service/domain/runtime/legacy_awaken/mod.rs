//! 思考路径实例：用新图引擎承载现有 `awaken` 链路
//!
//! ## 定位
//!
//! `RuntimeAwakening::awaken` 的**执行体**。拓扑、节点职责、数据总线契约都按
//! 新框架写，但每个节点内部仍调用老链路的能力（`ThinkBackend` 把 LLM 触碰点
//! 收成两个方法）——**行为等价**，因此可以先把编排换掉，再逐步把 `run_think_loop`
//!（480 行）内部的下沉到图里。
//!
//! ## 三个阶段
//!
//! 1. ✅ **编排换血**（本次）：`awaken` 只做入参转换，图编排承担全部控制流。
//! 2. **节点下沉**：把 `run_think_loop` 内部（小脑路由、工具二分、8 个退出策略）
//!    逐块搬成独立节点，届时本包只剩拓扑，不再有「委托老函数」的痕迹。
//! 3. **路径分化**：反射弧 / 多轮 planning 等场景各自拥有蓝图，不再共用一条。
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

// `awaken`（上层消费者）直接走 `runner::run_legacy_awaken`，
// 这里不再 re-export：多一层转发就多一处「应该从哪进」的模糊。
