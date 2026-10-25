//! 思考路径实例：用新图引擎承载现有 `awaken` 链路
//!
//! ## 定位
//!
//! `RuntimeAwakening::awaken` 的**执行体**。拓扑、节点职责、数据总线契约都按
//! 新框架写，但 deliberate 节点内部仍调用老链路的思考循环（`ThinkBackend` 把
//! LLM 触碰点收成四个方法）。
//!
//! ## 三个阶段
//!
//! 1. ✅ **编排换血**：`awaken` 只做入参转换，图编排承担全部控制流。
//! 2. ✅ **节点下沉（意图/检索/发现三节点）**：entry 换成 `intent`，
//!    主链 =「意图识别 → 机械检索 → 短聚焦装配 → 执行」，`deliberate` 的
//!    `max_rounds` 退出先经 `discover`（新发现识别）判断——有新词回
//!    `retrieve` 再检索装配，无新词走 `force_compact` 压缩退出。数据经
//!    ThinkBus 总线流动（`ContextKey::Intent/Knowledge` 分区 + run 级
//!    检索状态 `retrieval`）。
//! 3. **路径分化**：反射弧 / 多轮 planning 等场景各自拥有蓝图，不再共用一条；
//!    `deliberate` 内部（小脑路由、工具二分、8 个退出策略）继续下沉成节点。
//!
//! 当前拓扑（blueprint.rs 是唯一真相）：
//!
//! ```text
//! intent ──done──▶ retrieve ──done──▶ prepare ──done──▶ deliberate
//!                    ▲                                   │
//!                    │ new_findings                      ├─ final ──▶ consolidate
//!                    └──────────── discover ◀─max_rounds──┤
//!                                │                       └─ overflow ─▶ compact ──▶ prepare
//!                                └─ no_new_findings ─▶ force_compact ──▶ consolidate
//! ```
//!
//! ## 与 pkg 的关系
//!
//! `pkg::think_path` 是通用引擎（零业务），本包是它的**第一个业务实例**，
//! 单向依赖，反向禁止。

pub mod backend;
pub mod blueprint;
pub mod context;
pub mod discover_node;
pub mod intent_node;
pub mod nodes;
pub mod retrieve_node;
pub mod runner;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
#[path = "integration_tests.rs"]
mod integration_tests;

// `awaken`（上层消费者）直接走 `runner::run_legacy_awaken`，
// 这里不再 re-export：多一层转发就多一处「应该从哪进」的模糊。
