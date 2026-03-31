//! Agent DAL 模块

use crate::error::AppError;
use crate::models::agent::Agent;
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
}
