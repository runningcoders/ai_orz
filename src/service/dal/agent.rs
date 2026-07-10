//! Agent DAL 模块

use common::error::Result;
use common::models::{AgentStats, ModelCallStats, StatsFetchOptions};
use crate::models::agent::Agent;
use crate::models::brain::Brain;
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
use crate::pkg::RequestContext;
use crate::pkg::stats::{ModelCallEvent, AgentAwakeEvent};
use crate::service::dao::agent;
use crate::service::dao::agent::{AgentDao, AgentQuery, AgentStatsDao, AgentStatsQuery};
use crate::service::dao::model_provider::{ModelProviderStatsDao, ModelProviderStatsQuery};
use common::enums::AgentStatus;
use std::sync::{Arc, OnceLock};

use crate::enrich_ctx;
// ==================== 单例管理 ====================

static AGENT_DAL: OnceLock<Arc<dyn AgentDal>> = OnceLock::new();

/// 获取 Agent DAL 单例
pub fn dal() -> Arc<dyn AgentDal> {
    AGENT_DAL.get().cloned().unwrap()
}

/// 初始化 Agent DAL
pub fn init() {
    agent::stats_init();
    crate::service::dao::model_provider::stats_init();
    let _ = AGENT_DAL.set(new(
        agent::dao(),
        agent::stats_dao(),
        crate::service::dao::model_provider::stats_dao(),
    ));
}

/// 创建 Agent DAL（返回 trait 对象）
pub fn new(
    agent_dao: Arc<dyn AgentDao + Send + Sync>,
    agent_stats_dao: Arc<dyn AgentStatsDao<AwakeEvent = AgentAwakeEvent>>,
    model_provider_stats_dao: Arc<dyn ModelProviderStatsDao<ModelCallEvent = ModelCallEvent>>,
) -> Arc<dyn AgentDal> {
    Arc::new(AgentDalImpl { agent_dao, agent_stats_dao, model_provider_stats_dao })
}

// ==================== DAL 接口 ====================

/// Agent 附带信息获取选项
#[derive(Debug, Clone, Default)]
pub struct AgentFetchOptions {
    /// 是否加载运行时状态（默认 true）
    pub with_runtime_state: Option<bool>,
    /// 是否加载统计信息
    pub with_stats: Option<bool>,
    /// 统计过滤条件（with_stats=true 时生效，按任务 ID 过滤）
    pub stats_task_id: Option<String>,
}

/// Agent DAL 接口
#[async_trait::async_trait]
pub trait AgentDal: Send + Sync {
    /// 创建 Agent
    async fn create(&self, ctx: RequestContext, agent: &Agent) -> Result<()>;

    /// 根据 ID 查询 Agent
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Agent>>;

    /// 根据 ID 查询 Agent（带附带信息选项）
    async fn get_agent(&self, ctx: RequestContext, id: &str, options: AgentFetchOptions) -> Result<Option<Agent>>;

    /// 通用综合查询
    ///
    /// 支持组合查询条件，所有字段都是 Option
    async fn query(&self, ctx: RequestContext, query: AgentQuery) -> Result<Vec<Agent>>;

    /// 查询所有 Agent
    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<Agent>>;

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
    async fn wake_brain(
        &self,
        ctx: RequestContext,
        agent: &mut Agent,
        brain: Brain,
    ) -> Result<()>;

    // ==================== 统计查询 ====================

    /// 获取 Agent 自身统计数据
    async fn get_stats(&self, ctx: RequestContext, agent_id: &str, options: StatsFetchOptions) -> Result<AgentStats>;

    /// 获取 Agent 维度的模型调用统计
    ///
    /// 由 ModelProviderStatsDao（模型调用领域）负责计算，
    /// 按 agent_id 过滤后返回 ModelCallStats。
    async fn get_model_call_stats(&self, ctx: RequestContext, agent_id: &str, options: StatsFetchOptions) -> Result<ModelCallStats>;
}

/// Agent DAL 实现
struct AgentDalImpl {
    agent_dao: Arc<dyn AgentDao>,
    agent_stats_dao: Arc<dyn AgentStatsDao<AwakeEvent = AgentAwakeEvent>>,
    model_provider_stats_dao: Arc<dyn ModelProviderStatsDao<ModelCallEvent = ModelCallEvent>>,
}

impl AgentDalImpl {
    /// 注入运行时状态到 Agent 实体
    fn inject_runtime_state(agent: Agent) -> Agent {
        let runtime_info = AgentRuntimeStateManager::global()
            .get(&agent.po.id);
        Agent {
            runtime_info,
            ..agent
        }
    }
}

#[async_trait::async_trait]
impl AgentDal for AgentDalImpl {
    async fn create(&self, ctx: RequestContext, agent: &Agent) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, agent);
        self.agent_dao.insert(ctx, &agent.po).await
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Agent>> {
        let opt = self.agent_dao.find_by_id(ctx, id).await?;
        Ok(opt.map(Agent::from_po).map(Self::inject_runtime_state))
    }

    async fn get_agent(&self, ctx: RequestContext, id: &str, options: AgentFetchOptions) -> Result<Option<Agent>> {
        let opt = self.agent_dao.find_by_id(ctx.clone(), id).await?;
        let Some(mut agent) = opt.map(Agent::from_po) else {
            return Ok(None);
        };

        let with_runtime = options.with_runtime_state.unwrap_or(true);
        if with_runtime {
            agent = Self::inject_runtime_state(agent);
        }

        if options.with_stats.unwrap_or(false) {
            let stats_options = StatsFetchOptions {
                with_call_summary: true,
                with_token_summary: false,
                with_time_series: false,
                time_range: None,
                interval: None,
            };
            let query = AgentStatsQuery {
                agent_id: id.to_string(),
                task_id: options.stats_task_id.clone(),
                time_range: stats_options.time_range,
                ..Default::default()
            };
            let stats = self.agent_stats_dao.get_stats(ctx.clone(), query, stats_options).await?;
            agent.stats = Some(stats);
        }

        Ok(Some(agent))
    }

    async fn query(&self, ctx: RequestContext, query: AgentQuery) -> Result<Vec<Agent>> {
        let agents = self.agent_dao.query(ctx, query).await?;
        Ok(agents.into_iter().map(Agent::from_po).map(Self::inject_runtime_state).collect())
    }

    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<Agent>> {
        self.query(
            ctx,
            AgentQuery {
                exclude_status: Some(AgentStatus::Deleted),
                ..Default::default()
            },
        )
        .await
    }

    async fn update(&self, ctx: RequestContext, agent: &Agent) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, agent);
        self.agent_dao.update(ctx, &agent.po).await
    }

    async fn delete(&self, ctx: RequestContext, agent: &Agent) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, agent);
        self.agent_dao.delete(ctx, &agent.po).await
    }

    async fn wake_brain(
        &self,
        ctx: RequestContext,
        agent: &mut Agent,
        brain: Brain,
    ) -> Result<()> {
        // 1. 从 Brain 中获取 Cortex，Cortex 持有 ModelProvider，从中获取 model_provider_id
        let model_provider_id = brain.cortex().model_provider.po.id.clone();

        // 2. 如果 model_provider_id 发生变化，更新 Agent po 中的 model_provider_id
        let need_update = agent.po.model_provider_id != model_provider_id;

        if need_update {
            agent.po.model_provider_id = model_provider_id;
        }
        // 3. 直接使用传入的 brain 赋值给 Agent
        agent.set_brain(brain);

        // 4. 如果我们更新了 model_provider_id，需要更新数据库
        if need_update {
            let ctx = enrich_ctx!(&ctx, &*agent);
            self.update(ctx, agent).await?;
        }

        Ok(())
    }

    // ==================== 统计查询 ====================

    async fn get_stats(&self, ctx: RequestContext, agent_id: &str, options: StatsFetchOptions) -> Result<AgentStats> {
        let query = AgentStatsQuery {
            agent_id: agent_id.to_string(),
            time_range: options.time_range,
            ..Default::default()
        };
        self.agent_stats_dao.get_stats(ctx, query, options).await
    }

    async fn get_model_call_stats(&self, ctx: RequestContext, agent_id: &str, options: StatsFetchOptions) -> Result<ModelCallStats> {
        let query = ModelProviderStatsQuery {
            agent_id: Some(agent_id.to_string()),
            time_range: options.time_range,
            interval: options.interval,
            ..Default::default()
        };
        self.model_provider_stats_dao.get_stats(ctx, query, options).await
    }
}