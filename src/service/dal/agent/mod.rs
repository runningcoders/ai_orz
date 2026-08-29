//! Agent DAL——总 trait + 单例管理
//!
//! 按消费面拆分为多个子模块（本次文件重构，学习 `lark/` 目录模式）：
//!
//! - **本文件（mod.rs）**：[`AgentDal`] 总 trait + [`AgentFetchOptions`] + [`AgentDalImpl`] 结构 + 单例管理
//! - [`r#impl`]：`AgentDalImpl` 的 CRUD / 搜索 / 统计 / 唤醒 / 向量索引实现
//! - [`builder`]：Prompt 构建器（[`DefaultPromptBuilder`] Local / [`FlatPromptBuilder`] 外部 Agent）
//! - [`a2a`]：A2A Remote Agent 派生 Dal（[`A2aAgentDal`]）
//! - [`codex`]：Codex / CLI Agent 派生 Dal（[`CodexAgentDal`]）
//!
//! 测试文件随模块内聚：`agent_test.rs`（DAL 集成）/ `a2a_test.rs` / `codex_test.rs` /
//! `builder/prompt_builder_test.rs`（Builder 单元测试）。

mod a2a;
mod builder;
mod codex;
mod r#impl;

use crate::models::agent::Agent;
use crate::models::brain::Brain;
use crate::pkg::RequestContext;
use crate::pkg::stats::{AgentAwakeEvent, ModelCallEvent, ToolCallEvent};
use crate::service::dao::agent::{
    self, AgentDao, AgentQuery, AgentSearch, AgentStatsDao, AgentVectorDao,
};
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::model_provider::{ModelProviderDao, ModelProviderStatsDao};
use crate::service::dao::tool::ToolStatsDao;
use common::error::Result;
use common::models::{AgentStats, ModelCallStats, StatsFetchOptions, StatsInterval};
use std::sync::{Arc, OnceLock};

pub use a2a::A2aAgentDal;
pub use builder::{DefaultPromptBuilder, FlatPromptBuilder, build_conversation_prompt};
pub use codex::CodexAgentDal;

#[cfg(test)]
mod a2a_test;
#[cfg(test)]
mod agent_test;
#[cfg(test)]
mod codex_test;

// ==================== 单例管理 ====================

static AGENT_DAL: OnceLock<Arc<dyn AgentDal>> = OnceLock::new();

/// 获取 Agent DAL 单例
pub fn dal() -> Arc<dyn AgentDal> {
    AGENT_DAL.get().cloned().unwrap()
}

/// 初始化 Agent DAL
pub fn init() {
    agent::stats_init();
    crate::service::dao::tool::stats_init();
    crate::service::dao::model_provider::stats_init();
    let _ = AGENT_DAL.set(new(
        agent::dao(),
        agent::vector_dao(),
        agent::stats_dao(),
        crate::service::dao::tool::stats_dao(),
        crate::service::dao::model_provider::stats_dao(),
        crate::service::dao::cortex::dao(),
        crate::service::dao::model_provider::dao(),
    ));
}

/// 创建 Agent DAL（返回 trait 对象）
pub fn new(
    agent_dao: Arc<dyn AgentDao + Send + Sync>,
    agent_vector_dao: Arc<dyn AgentVectorDao + Send + Sync>,
    agent_stats_dao: Arc<dyn AgentStatsDao<AwakeEvent = AgentAwakeEvent>>,
    tool_stats_dao: Arc<dyn ToolStatsDao<ToolCallEvent = ToolCallEvent>>,
    model_provider_stats_dao: Arc<dyn ModelProviderStatsDao<ModelCallEvent = ModelCallEvent>>,
    cortex_dao: Arc<dyn CortexDao + Send + Sync>,
    model_provider_dao: Arc<dyn ModelProviderDao + Send + Sync>,
) -> Arc<dyn AgentDal> {
    Arc::new(AgentDalImpl {
        agent_dao,
        agent_vector_dao,
        agent_stats_dao,
        tool_stats_dao,
        model_provider_stats_dao,
        cortex_dao,
        model_provider_dao,
    })
}

// ==================== DAL 接口 ====================

/// Agent 附带信息获取选项
#[derive(Debug, Clone, Default)]
pub struct AgentFetchOptions {
    /// 是否加载运行时状态（默认 true）
    pub with_runtime_state: Option<bool>,
    /// 是否加载 Agent 绑定的工具（绑定工具 + tag 匹配的内置工具）
    pub with_tools: Option<bool>,
    /// 是否加载 Agent 已安装的技能副本（author_id = agent_id，排除 Expired）
    pub with_skills: Option<bool>,
    /// 是否加载统计信息（AgentStats: 唤醒次数汇总）
    pub with_stats: Option<bool>,
    /// 是否加载模型调用统计（ModelCallStats: token + 时序）
    pub with_model_call_stats: Option<bool>,
    /// 统计过滤条件（with_stats=true 时生效，按任务 ID 过滤）
    pub stats_task_id: Option<String>,
    /// 统计时间范围（毫秒），None 表示全部历史
    pub stats_time_range: Option<(i64, i64)>,
    /// 时序查询粒度，None 时默认 Daily
    pub stats_interval: Option<StatsInterval>,
}

/// Agent DAL 接口
#[async_trait::async_trait]
pub trait AgentDal: Send + Sync {
    /// 创建 Agent
    async fn create(&self, ctx: RequestContext, agent: &Agent) -> Result<()>;

    /// 根据 ID 查询 Agent
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Agent>>;

    /// 根据 ID 查询 Agent（带附带信息选项）
    async fn get_agent(
        &self,
        ctx: RequestContext,
        id: &str,
        options: AgentFetchOptions,
    ) -> Result<Option<Agent>>;

    /// 通用综合查询
    ///
    /// 支持组合查询条件，所有字段都是 Option
    async fn query(
        &self,
        ctx: RequestContext,
        query: AgentQuery,
    ) -> Result<common::api::PagedResult<Agent>>;

    /// 统计符合查询条件的 Agent 数量（透传 DAO count）
    async fn count(&self, ctx: RequestContext, query: AgentQuery) -> Result<u64>;

    /// 查询所有 Agent
    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<Agent>>;

    /// 🔍 搜索 Agent（关键词 + 向量语义混合搜索）
    ///
    /// 自动根据参数选择搜索策略：
    /// - keyword 存在 → 走 FTS5 全文检索
    /// - query_vector 存在 → 走向量语义搜索
    /// - 两者都有 → 混合搜索，合并结果（三态匹配 + 综合排序）
    ///
    /// 返回分页结果，支持 runtime_state 内存过滤。
    async fn search(
        &self,
        ctx: RequestContext,
        search: AgentSearch,
    ) -> Result<common::api::PagedResult<Agent>>;

    /// 更新 Agent
    async fn update(&self, ctx: RequestContext, agent: &Agent) -> Result<()>;

    /// 删除 Agent
    async fn delete(&self, ctx: RequestContext, agent: &Agent) -> Result<()>;

    /// 唤醒 Brain
    ///
    /// 直接使用传入的 brain 赋值给 Agent，不负责创建 brain
    /// Brain 已经持有 ModelProvider，可以从中获取 model_provider_id 更新到 Agent po
    ///
    /// 唤醒完成后将 brain 写入 Agent 的 brain 字段
    /// 如果 model_provider_id 发生变化，自动更新数据库
    async fn wake_brain(&self, ctx: RequestContext, agent: &mut Agent, brain: Brain) -> Result<()>;

    // ==================== 统计查询 ====================

    /// 获取 Agent 自身统计数据
    async fn get_stats(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        options: StatsFetchOptions,
    ) -> Result<AgentStats>;

    /// 获取 Agent 维度的模型调用统计
    ///
    /// 由 ModelProviderStatsDao（模型调用领域）负责计算，
    /// 按 agent_id 过滤后返回 ModelCallStats。
    async fn get_model_call_stats(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        options: StatsFetchOptions,
    ) -> Result<ModelCallStats>;

    /// 🔄 重建所有 Agent 的向量索引
    ///
    /// 清空向量集合后，查询全量 Agent，逐条重新生成 embedding 并 upsert。
    /// 单条失败不影响整体，用 log_warn! 记录。
    async fn rebuild_vectors(&self, ctx: RequestContext) -> Result<()>;

    /// 返回该 Dal 对应 Agent 类型的 PromptBuilder
    ///
    /// 基础实现返回 DefaultPromptBuilder（Local Agent 使用）。
    /// 派生 Dal（CodexAgentDal/A2aAgentDal）可重写此方法返回专属 builder。
    fn prompt_builder(&self) -> Box<dyn crate::models::prompt_builder::PromptBuilder> {
        Box::new(DefaultPromptBuilder::new())
    }
}

/// Agent DAL 实现
///
/// 字段为私有；实现（[`r#impl`]）以子模块身份可访问。
struct AgentDalImpl {
    agent_dao: Arc<dyn AgentDao>,
    agent_vector_dao: Arc<dyn AgentVectorDao>,
    agent_stats_dao: Arc<dyn AgentStatsDao<AwakeEvent = AgentAwakeEvent>>,
    tool_stats_dao: Arc<dyn ToolStatsDao<ToolCallEvent = ToolCallEvent>>,
    model_provider_stats_dao: Arc<dyn ModelProviderStatsDao<ModelCallEvent = ModelCallEvent>>,
    cortex_dao: Arc<dyn CortexDao>,
    model_provider_dao: Arc<dyn ModelProviderDao>,
}
