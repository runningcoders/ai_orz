//! 轮级环图：把主思考循环拆成独立节点
//!
//! 原先 `run_think_loop` 是一个「调用模型 + 执行工具 + 策略裁决」的大循环，
//! 想调整思考方向（例如在一轮内插入额外判断、换一种回环条件）只能改这段直线代码。
//! 现在每轮拆成独立节点：
//!
//! - [`blueprint`]：`think → dispatch → evaluate → think` 的可序列化拓扑；
//!   Agent 开启 `enable_round_reflect` 时额外挂载 `evaluate → reflect → think` 分叉
//! - [`nodes`]：各节点的实现（模型调用 / 工具执行 / 策略裁决 / 自省换向）
//! - [`state`]：跨节点状态（对话、token、待执行工具调用、终止结果）
//! - [`deps`]：`'static` 的共享依赖容器
//! - [`runner`]：装配蓝图 + 节点池并驱动引擎，同时落盘轮级轨迹（think_trace）
//!
//! 外层 awaken / settle / compact 拓扑本阶段不动，仍由 `run_think_loop` 调用本模块。

mod blueprint;
mod deps;
mod nodes;
mod runner;
mod state;

pub(crate) use runner::{RoundRun, run};
