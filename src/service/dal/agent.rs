//! Agent DAL 模块

use crate::error::AppError;
use crate::models::agent::Agent;
use crate::models::brain::Brain;
use crate::models::model_provider::ModelProvider;
use crate::pkg::RequestContext;
use crate::service::dao::agent::AgentDaoTrait;
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static AGENT_DAL: OnceLock<Arc<dyn AgentDalTrait>> = OnceLock::new();

/// 获取 Agent DAL 单例
pub fn dal() -> Arc<dyn AgentDalTrait> {
    AGENT_DAL.get().cloned().unwrap()
}

/// 初始化 Agent DAL
pub fn init(agent_dao: Arc<dyn AgentDaoTrait>) {
    let _ = AGENT_DAL.set(Arc::new(AgentDal::new(agent_dao)));
}

// ==================== DAL 实现 ====================

/// Agent DAL 接口
pub trait AgentDalTrait: Send + Sync {
    /// 创建 Agent
    fn create(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError>;

    /// 根据 ID 查询 Agent
    fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Agent>, AppError>;

    /// 查询所有 Agent
    fn find_all(&self, ctx: RequestContext) -> Result<Vec<Agent>, AppError>;

    /// 更新 Agent
    fn update(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError>;

    /// 删除 Agent
    fn delete(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError>;

    /// 唤醒 Brain
    ///
    /// 如果传入 Some(model_provider) → 更新 Agent po 中的 model_provider_id
    /// 直接使用传入的 brain 赋值给 Agent，不负责创建 brain
    ///
    /// 唤醒完成后将 brain 写入 Agent 的 brain 字段
    /// 如果更新了 model_provider_id，自动更新数据库
    fn wake_brain(&self, ctx: RequestContext, agent: &mut Agent, model_provider: Option<&ModelProvider>, brain: Brain) -> Result<(), AppError>;
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

impl AgentDalTrait for AgentDal {
    fn create(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError> {
        self.agent_dao.insert(ctx, &agent.po)
    }

    fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Agent>, AppError> {
        self.agent_dao
            .find_by_id(ctx, id)
            .map(|opt| opt.map(Agent::from_po))
    }

    fn find_all(&self, ctx: RequestContext) -> Result<Vec<Agent>, AppError> {
        self.agent_dao
            .find_all(ctx)
            .map(|agents: Vec<_>| agents.into_iter().map(Agent::from_po).collect())
    }

    fn update(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError> {
        self.agent_dao.update(ctx, &agent.po)
    }

    fn delete(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError> {
        self.agent_dao.delete(ctx, &agent.po)
    }

    fn wake_brain(&self, ctx: RequestContext, agent: &mut Agent, model_provider: Option<&ModelProvider>, brain: Brain) -> Result<(), AppError> {
        // 1. 如果传入了 ModelProvider，更新 model_provider_id
        if let Some(mp) = model_provider {
            agent.po.model_provider_id = mp.po.id.clone();
        }

        // 2. 直接使用传入的 brain 赋值给 Agent
        agent.set_brain(brain);

        // 3. 如果我们更新了 model_provider_id，需要更新数据库
        if model_provider.is_some() {
            self.update(ctx, agent)?;
        }

        Ok(())
    }
}
