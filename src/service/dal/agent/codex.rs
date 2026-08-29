//! Codex / CLI Agent DAL
//!
//! 派生自 AgentDal，专门管理 Cli 类型外部 Agent。
//! 通过委托模式复用 AgentDal 的所有管理操作，仅在有差异化需求时重写对应方法。
//!
//! 设计原则：每类 Agent Dal 配套自己的 PromptBuilder。
//!
//! Cli Agent 不支持 OpenAI Chat 协议的角色分离，也不支持 function calling，
//! 因此配套 `FlatPromptBuilder`：把 System + User 合成为单条纯文本提示词，
//! 并按 `ExternalAgentConfig::Cli.prompt_template` 适配具体 CLI 的推荐格式。
//!
//! 若后续某个 CLI（如 codex / claude / aider）需要完全定制的拼装规则，
//! 在此实现 `CliPromptBuilder` 并重写 `prompt_builder()` 即可，扩展点已保留。

use crate::models::agent::Agent;
use crate::models::brain::Brain;
use crate::pkg::RequestContext;
use crate::service::dao::agent::{AgentQuery, AgentSearch};
use common::error::Result;
use common::models::{AgentStats, ModelCallStats, StatsFetchOptions};
use std::sync::Arc;

use super::{AgentDal, AgentFetchOptions, FlatPromptBuilder};

/// Codex / CLI Agent DAL
///
/// 委托 Arc<dyn AgentDal> 实现所有管理操作。
/// `prompt_builder` 重写为 `FlatPromptBuilder`，保证 System 人设不丢。
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

    /// Cli Agent 配套扁平化 Builder
    ///
    /// 不走 trait 默认的 `DefaultPromptBuilder`——那会产出 `[System, User]` 角色分离
    /// 结构，而 `BrainDal::think` 的 Cli 分支只取最后一条 User，人设和技能会整块丢失。
    fn prompt_builder(&self) -> Box<dyn crate::models::prompt_builder::PromptBuilder> {
        Box::new(FlatPromptBuilder::new())
    }
}
