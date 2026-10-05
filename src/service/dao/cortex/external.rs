//! ExternalCortexDao - 外部 Agent 的虚拟 Cortex DAO
//!
//! 为外部 Agent（CLI / A2A 远程）实现 `native::CortexDao` trait，
//! 使其能通过统一接口被调用。
//!
//! 设计：
//! - `think()` 桥接到 AgentRuntimeDao 执行（包装为 ThinkResult::Final）
//! - `embed()` 不支持（外部 agent 自己处理向量），返回 Internal 错误

use async_trait::async_trait;
use common::error::{Result, err};

use crate::models::agent::{AgentPo, ExternalAgentConfig};
use crate::models::cortex_types::{ChatMessage, ThinkResult, ToolDescriptor};
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::RequestContext;
use crate::service::dao::agent_runtime::{
    AgentRuntimeDao, a2a::A2aRuntimeDao, codex::CodexRuntimeDao,
};
use crate::service::dao::cortex::native::CortexDao;

/// 外部 Agent 的虚拟 Cortex DAO
///
/// 注意：当前不通过 registry 路由（external agent 不走 cortex::native::registry()），
/// 而是由 brain.think() 直接按 brain.kind 分发。此实现保留供未来统一接入。
pub struct ExternalCortexDao {
    agent: AgentPo,
    runtime_dao: Box<dyn AgentRuntimeDao>,
}

impl ExternalCortexDao {
    /// 从 AgentPo 创建 ExternalCortexDao
    ///
    /// 如果 Agent 不是外部类型或配置不完整，返回 None
    pub fn from_agent(agent: &AgentPo) -> Option<Self> {
        let config = agent.get_external_config()?;
        let runtime_dao: Box<dyn AgentRuntimeDao> = match config {
            ExternalAgentConfig::Cli {
                command,
                args,
                work_dir,
                env,
                timeout_secs,
                prompt_template,
            } => Box::new(CodexRuntimeDao::new(
                crate::service::dao::agent_runtime::codex::CliRuntimeConfig {
                    command,
                    args,
                    work_dir,
                    env,
                    timeout_secs,
                    prompt_template,
                },
            )),
            ExternalAgentConfig::Remote {
                endpoint,
                agent_name,
                auth_token,
                timeout_secs,
            } => Box::new(A2aRuntimeDao::new(
                crate::service::dao::agent_runtime::a2a::A2aRuntimeConfig {
                    endpoint,
                    agent_name,
                    auth_token,
                    timeout_secs,
                },
            )),
        };

        Some(Self {
            agent: agent.clone(),
            runtime_dao,
        })
    }

    /// 获取 Agent ID
    pub fn agent_id(&self) -> &str {
        &self.agent.id
    }

    /// 获取 Agent 名称
    pub fn agent_name(&self) -> &str {
        &self.agent.name
    }
}

#[async_trait]
impl CortexDao for ExternalCortexDao {
    async fn think(
        &self,
        ctx: RequestContext,
        _provider: &ModelProviderPo,
        messages: &[ChatMessage],
        _tools: &[ToolDescriptor],
    ) -> Result<ThinkResult> {
        // 外部 agent 不支持多轮工具调用，提取最后一条 user 消息作为 prompt
        let prompt = messages
            .iter()
            .rev()
            .find_map(|m| match m {
                ChatMessage::User { content } => Some(content.as_str()),
                // 多模态消息取 text part（防静默空 prompt；批4 方案 §2.4）
                ChatMessage::UserMultimodal { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .unwrap_or("");
        let content = self
            .runtime_dao
            .invoke(ctx, &self.agent, prompt)
            .await
            .map_err(|e| err!(Internal, "external agent invoke failed: {}", e))?;
        Ok(ThinkResult::Final {
            content,
            usage: crate::models::cortex_types::TokenUsage::default(),
        })
    }

    async fn embed(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
        _texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        Err(err!(Internal, "ExternalCortexDao 不支持 embed"))
    }
}
#[cfg(test)]
#[path = "external_tests.rs"]
mod tests;
