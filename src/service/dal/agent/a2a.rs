//! A2A Remote Agent DAL
//!
//! 派生自 AgentDal，专门管理 Remote 类型外部 Agent（A2A 协议）。
//! 通过委托模式复用 AgentDal 的所有管理操作，仅在有差异化需求时重写对应方法。
//!
//! 设计原则：每类 Agent Dal 配套自己的 PromptBuilder。
//!
//! Remote Agent 通过 A2A 协议发送纯文本（`message.parts[].text`），同样不支持
//! OpenAI Chat 的角色分离与 function calling，因此配套 `FlatPromptBuilder`，
//! 把 System + User 合成为单条提示词，保证人设不丢。
//!
//! 若后续 A2A 侧需要专属格式（如结构化 parts / 元数据），
//! 在此实现 `RemotePromptBuilder` 并重写 `prompt_builder()` 即可。

use crate::models::agent::Agent;
use crate::models::brain::Brain;
use crate::pkg::RequestContext;
use crate::service::dao::agent::{AgentQuery, AgentSearch};
use common::error::Result;
use common::models::{AgentStats, ModelCallStats, StatsFetchOptions};
use std::sync::Arc;

use super::{AgentDal, AgentFetchOptions, FlatPromptBuilder};

/// A2A Remote Agent DAL
///
/// 委托 Arc<dyn AgentDal> 实现所有管理操作。
/// `prompt_builder` 重写为 `FlatPromptBuilder`，保证 System 人设不丢。
pub struct A2aAgentDal {
    base: Arc<dyn AgentDal>,
}

impl A2aAgentDal {
    pub fn new(base: Arc<dyn AgentDal>) -> Self {
        Self { base }
    }
}

#[async_trait::async_trait]
impl AgentDal for A2aAgentDal {
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

    async fn query(
        &self,
        ctx: RequestContext,
        query: AgentQuery,
    ) -> Result<common::api::PagedResult<Agent>> {
        self.base.query(ctx, query).await
    }

    async fn count(&self, ctx: RequestContext, query: AgentQuery) -> Result<u64> {
        self.base.count(ctx, query).await
    }

    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<Agent>> {
        self.base.find_all(ctx).await
    }

    async fn search(
        &self,
        ctx: RequestContext,
        search: AgentSearch,
    ) -> Result<common::api::PagedResult<Agent>> {
        self.base.search(ctx, search).await
    }

    async fn update(&self, ctx: RequestContext, agent: &Agent) -> Result<()> {
        self.base.update(ctx, agent).await
    }

    async fn delete(&self, ctx: RequestContext, agent: &Agent) -> Result<()> {
        self.base.delete(ctx, agent).await
    }

    async fn wake_brain(&self, ctx: RequestContext, agent: &mut Agent, brain: Brain) -> Result<()> {
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

    /// Remote Agent 配套扁平化 Builder
    ///
    /// 同 `CodexAgentDal`：A2A 只发纯文本，角色分离会导致 System 人设丢失。
    fn prompt_builder(&self) -> Box<dyn crate::models::prompt_builder::PromptBuilder> {
        Box::new(FlatPromptBuilder::new())
    }
}
