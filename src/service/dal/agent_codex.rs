//! Codex / CLI Agent DAL
//!
//! 派生自 AgentDal，专门管理 Cli 类型外部 Agent。
//! 通过委托模式复用 AgentDal 的所有管理操作，仅在有差异化需求时重写对应方法。
//!
//! 设计原则：每类 Agent Dal 配套自己的 PromptBuilder；没有专属 builder 时
//! 复用 trait 默认方法提供的 DefaultPromptBuilder，不引入笼统的"外部 builder"。
//! 未来实现 CliPromptBuilder 后在此重写 prompt_builder()。

use common::error::Result;
use common::models::{AgentStats, ModelCallStats, StatsFetchOptions};
use crate::models::agent::Agent;
use crate::models::brain::Brain;
use crate::pkg::RequestContext;
use crate::service::dao::agent::{AgentQuery, AgentSearch};
use std::sync::Arc;

use super::agent::{AgentDal, AgentFetchOptions};

/// Codex / CLI Agent DAL
///
/// 委托 Arc<dyn AgentDal> 实现所有管理操作。
/// prompt_builder 走 trait 默认方法返回 DefaultPromptBuilder，
/// 实现 CliPromptBuilder 后在此重写。
pub struct CodexAgentDal {
    base: Arc<dyn AgentDal>,
}

impl CodexAgentDal {
    pub fn new(base: Arc<dyn AgentDal>) -> Self {
        Self { base }
    }
}

#[async_trait::async_trait]
impl AgentDal for CodexAgentDal {
    async fn create(&self, ctx: RequestContext, agent: &Agent) -> Result<()> {
        self.base.create(ctx, agent).await
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Agent>> {
        self.base.find_by_id(ctx, id).await
    }

    async fn get_agent(
        &self,
        ctx: RequestContext,
        id: &str,
        options: AgentFetchOptions,
    ) -> Result<Option<Agent>> {
        self.base.get_agent(ctx, id, options).await
    }

    async fn query(&self, ctx: RequestContext, query: AgentQuery) -> Result<common::api::PagedResult<Agent>> {
        self.base.query(ctx, query).await
    }

    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<Agent>> {
        self.base.find_all(ctx).await
    }

    async fn search(&self, ctx: RequestContext, search: AgentSearch) -> Result<Vec<Agent>> {
        self.base.search(ctx, search).await
    }

    async fn update(&self, ctx: RequestContext, agent: &Agent) -> Result<()> {
        self.base.update(ctx, agent).await
    }

    async fn delete(&self, ctx: RequestContext, agent: &Agent) -> Result<()> {
        self.base.delete(ctx, agent).await
    }

    async fn wake_brain(
        &self,
        ctx: RequestContext,
        agent: &mut Agent,
        brain: Brain,
    ) -> Result<()> {
        self.base.wake_brain(ctx, agent, brain).await
    }

    async fn get_stats(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        options: StatsFetchOptions,
    ) -> Result<AgentStats> {
        self.base.get_stats(ctx, agent_id, options).await
    }

    async fn get_model_call_stats(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        options: StatsFetchOptions,
    ) -> Result<ModelCallStats> {
        self.base.get_model_call_stats(ctx, agent_id, options).await
    }

    async fn rebuild_vectors(&self, ctx: RequestContext) -> Result<()> {
        self.base.rebuild_vectors(ctx).await
    }
}
