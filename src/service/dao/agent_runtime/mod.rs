//! Agent Runtime DAO 模块
//!
//! 职责：封装外部 Agent 的执行能力，与 agent 实体 DAO 平级。
//! 不同类型的外部 Agent（CLI、A2A 远程等）有各自的实现。

use async_trait::async_trait;
use common::error::Result;
use dyn_clone::DynClone;
use crate::models::agent::AgentPo;
use crate::pkg::RequestContext;

pub mod codex;
pub mod a2a;

/// Agent Runtime DAO trait
///
/// 外部 Agent 执行层的统一抽象。
/// 不同的执行后端（CLI 子进程、A2A HTTP 等）实现此 trait。
///
/// 设计：接收 `&AgentPo` 而非 `agent_id`，方便实现直接使用 agent 的
/// 配置字段（kind、runtime_config、name 等），也便于未来扩展。
#[async_trait]
pub trait AgentRuntimeDao: Send + Sync + DynClone {
    /// 调用外部 Agent 执行 prompt，返回执行结果文本
    async fn invoke(&self, ctx: RequestContext, agent: &AgentPo, prompt: &str) -> Result<String>;
}
