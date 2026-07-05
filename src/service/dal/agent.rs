//! Agent DAL 模块

use common::error::Result;
use common::models::{AgentStats, StatsFetchOptions, TimeSeriesPoint, TokenSumResult};
use crate::models::agent::Agent;
use crate::models::brain::Brain;
use crate::pkg::RequestContext;
use crate::pkg::stats::{AggregationRow, ModelCallEvent};
use crate::service::dao::agent;
use crate::service::dao::agent::{AgentDao, AgentQuery, AgentStatsDao, AgentStatsQuery};
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
    let _ = AGENT_DAL.set(new(agent::dao(), agent::stats_dao()));
}

/// 创建 Agent DAL（返回 trait 对象）
pub fn new(
    agent_dao: Arc<dyn AgentDao + Send + Sync>,
    agent_stats_dao: Arc<dyn AgentStatsDao<ModelCallEvent = ModelCallEvent>>,
) -> Arc<dyn AgentDal> {
    Arc::new(AgentDalImpl { agent_dao, agent_stats_dao })
}

// ==================== DAL 接口 ====================

/// Agent DAL 接口
#[async_trait::async_trait]
pub trait AgentDal: Send + Sync {
    /// 创建 Agent
    async fn create(&self, ctx: RequestContext, agent: &Agent) -> Result<()>;

    /// 根据 ID 查询 Agent
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Agent>>;

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

    /// Token 汇总
    async fn sum_tokens(&self, ctx: RequestContext, query: AgentStatsQuery) -> Result<TokenSumResult>;

    /// 模型调用次数汇总
    async fn sum_calls(&self, ctx: RequestContext, query: AgentStatsQuery) -> Result<u64>;

    /// 模型调用时序查询
    async fn query_model_call_time_series(&self, ctx: RequestContext, query: AgentStatsQuery) -> Result<Vec<TimeSeriesPoint>>;

    /// 模型调用聚合查询
    async fn query_model_call_aggregation(&self, ctx: RequestContext, query: AgentStatsQuery) -> Result<Vec<AggregationRow>>;

    /// 获取 Agent 统计数据（按 options 控制返回哪些维度）
    async fn get_stats(&self, ctx: RequestContext, query: AgentStatsQuery, options: StatsFetchOptions) -> Result<AgentStats>;
}

/// Agent DAL 实现
struct AgentDalImpl {
    agent_dao: Arc<dyn AgentDao>,
    agent_stats_dao: Arc<dyn AgentStatsDao<ModelCallEvent = ModelCallEvent>>,
}

#[async_trait::async_trait]
impl AgentDal for AgentDalImpl {
    async fn create(&self, ctx: RequestContext, agent: &Agent) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, agent);
        self.agent_dao.insert(ctx, &agent.po).await
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Agent>> {
        let opt = self.agent_dao.find_by_id(ctx, id).await?;
        Ok(opt.map(Agent::from_po))
    }

    async fn query(&self, ctx: RequestContext, query: AgentQuery) -> Result<Vec<Agent>> {
        let agents = self.agent_dao.query(ctx, query).await?;
        Ok(agents.into_iter().map(Agent::from_po).collect())
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

    async fn sum_tokens(&self, ctx: RequestContext, query: AgentStatsQuery) -> Result<TokenSumResult> {
        self.agent_stats_dao.sum_tokens(ctx, query).await
    }

    async fn sum_calls(&self, ctx: RequestContext, query: AgentStatsQuery) -> Result<u64> {
        self.agent_stats_dao.sum_calls(ctx, query).await
    }

    async fn query_model_call_time_series(&self, ctx: RequestContext, query: AgentStatsQuery) -> Result<Vec<TimeSeriesPoint>> {
        self.agent_stats_dao.query_model_call_time_series(ctx, query).await
    }

    async fn query_model_call_aggregation(&self, ctx: RequestContext, query: AgentStatsQuery) -> Result<Vec<AggregationRow>> {
        self.agent_stats_dao.query_model_call_aggregation(ctx, query).await
    }

    async fn get_stats(&self, ctx: RequestContext, query: AgentStatsQuery, options: StatsFetchOptions) -> Result<AgentStats> {
        self.agent_stats_dao.get_stats(ctx, query, options).await
    }
}