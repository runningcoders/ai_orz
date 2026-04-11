//! Agent DAL 模块

use crate::error::AppError;
use crate::models::agent::Agent;
use crate::models::brain::Brain;
use crate::pkg::RequestContext;
use crate::service::dao::agent::AgentDaoTrait;
use std::sync::{Arc, OnceLock};
use crate::service::dao;
use crate::service::dao::agent;
// ==================== 单例管理 ====================

static AGENT_DAL: OnceLock<Arc<dyn AgentDalTrait>> = OnceLock::new();

/// 获取 Agent DAL 单例
pub fn dal() -> Arc<dyn AgentDalTrait> {
    AGENT_DAL.get().cloned().unwrap()
}

/// 初始化 Agent DAL
pub fn init() {
    // 先初始化 DAO，再初始化 DAL
    crate::service::dao::agent::init();
    let _ = AGENT_DAL.set(Arc::new(AgentDal::new(
        agent::dao(),
    )));
}

// ==================== DAL 接口 ====================

/// Agent DAL 接口
#[async_trait::async_trait]
pub trait AgentDalTrait: Send + Sync {
    /// 创建 Agent
    async fn create(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError>;

    /// 根据 ID 查询 Agent
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Agent>, AppError>;

    /// 查询所有 Agent
    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<Agent>, AppError>;

    /// 更新 Agent
    async fn update(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError>;

    /// 删除 Agent
    async fn delete(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError>;

    /// 唤醒 Brain
    ///
    /// 直接使用传入的 brain 赋值给 Agent，不负责创建 brain
    /// Brain 已经持有 ModelProvider，可以从中获取 model_provider_id 更新到 Agent po
    ///
    /// 唤醒完成后将 brain 写入 Agent 的 brain 字段
    /// 如果 model_provider_id 发生变化，自动更新数据库
    async fn wake_brain(&self, ctx: RequestContext, agent: &mut Agent, brain: Brain) -> Result<(), AppError>;
}

/// Agent DAL 实现
pub struct AgentDal {
    agent_dao: Arc<dyn AgentDaoTrait>,
}

impl AgentDal {
    /// 创建 DAL 实例
    pub fn new(agent_dao: Arc<dyn AgentDaoTrait>) -> Self {
        Self { agent_dao }
    }
}

#[async_trait::async_trait]
impl AgentDalTrait for AgentDal {
    async fn create(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError> {
        self.agent_dao.insert(ctx, &agent.po).await
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Agent>, AppError> {
        let opt = self.agent_dao.find_by_id(ctx, id).await?;
        Ok(opt.map(Agent::from_po))
    }

    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<Agent>, AppError> {
        let agents = self.agent_dao.find_all(ctx).await?;
        Ok(agents.into_iter().map(Agent::from_po).collect())
    }

    async fn update(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError> {
        self.agent_dao.update(ctx, &agent.po).await
    }

    async fn delete(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError> {
        self.agent_dao.delete(ctx, &agent.po).await
    }

    async fn wake_brain(&self, ctx: RequestContext, agent: &mut Agent, brain: Brain) -> Result<(), AppError> {
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
            self.update(ctx, agent).await?;
        }

        Ok(())
    }
}
