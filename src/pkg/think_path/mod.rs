//! 思维链路图引擎（Think Path Engine）
//!
//! 把「一次思考」建模为**黑板架构**（Blackboard）：节点池（逻辑）+ 数据总线（累积数据）
//! + 仲裁器（按总线状态选下一步）。蓝图可序列化、可版本化；实例独一无二、可回放。
//!
//! ```text
//! Blueprint（静态 · 可序列化 · 无行为）
//!     │  instantiate(run_id)
//!     ▼
//! ThinkRun（总线 + 轨迹 + 策略覆盖）
//!     │  loop: ensure_loaded → 跑节点 → 记 occurrence → 仲裁下一步
//!     ▼
//! RunTrace（occurrence 序列 + 每次选路的候选与来源）
//! ```
//!
//! 三条不能破的边界：
//! 1. **节点不知道后继** —— 只声明「我在什么条件下可被激活」（准入条件挂在 `to` 上）。
//! 2. **节点只能提案改策略参数**，不能改规则结构（见 `strategy`）。
//! 3. **顾问只选不定** —— LLM 只能在 L0 合法候选集里挑，越界降级；
//!    终止性由 `ActivationBudget` 保证，不交给概率。
//!
//! ⚠️ 本模块是**新引擎**，与既有 `think_loop` / `awakening` 完全并行、互不引用；
//! 切换采用影子对比（新引擎跑但不落库），逐步灰度。
//!
//! ## 为什么放在 `pkg` 而不是 `service/domain/runtime`
//!
//! 它是**引擎**不是业务：`ThinkNode` / `ThinkBus` / 仲裁器 / 蓝图都不认识
//! 「记忆 / 工具 / 技能」具体是什么，只是按契约搬运 `serde_json::Value`。
//! 与 `pkg::aop`（事件引擎）、`pkg::background_task`、`pkg::policy`（判定引擎）
//! 同类，也便于后续把 `sleep_and_settle` / 消息路由等非思考流程复用同一套图引擎。
//!
//! 业务侧（蓝图、具体节点、上下文供给器、顾问实现）后续放
//! `service/domain/runtime/`，单向依赖本模块，反向禁止。

pub mod arbiter;
pub mod blueprint;
pub mod bus;
pub mod engine;
pub mod node;
pub mod policy;
pub mod run;
pub mod strategy;
pub mod types;

#[cfg(test)]
pub mod testkit;

#[cfg(test)]
#[path = "engine_tests.rs"]
mod engine_tests;

#[cfg(test)]
#[path = "policy_tests.rs"]
mod policy_tests;

pub use arbiter::{Arbiter, NodePool, RouteAdvice, RouteAdvisor, RouteOutcome, RouterView};
pub use blueprint::{EdgeSpec, NodeSpec, PathLimits, ThinkPathBlueprint, ThinkPathBuilder};
pub use bus::{ContextProvider, ProviderSet, ThinkBus};
pub use engine::ThinkPathEngine;
pub use node::{
    ActivationBudget, ActivationRule, BusPredicate, NodeContract, NodeOutcome, ThinkNode,
};
pub use policy::{
    ActivationGuard, BudgetPolicy, BusPredicatePolicy, GuardCtx, InSetPolicy, compile_guard,
};
pub use run::{NodeOccurrence, RouteRecord, RouteVia, RunTrace, ThinkRun};
pub use strategy::{StrategyOverlay, StrategyPatch};
pub use types::{ContextKey, LoadDepth, LoadFailureMode, NodeId, NodeSignal, PhaseId, Stimulus};
