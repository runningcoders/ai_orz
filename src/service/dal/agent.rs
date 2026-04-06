//! Agent DAL 模块

use crate::error::AppError;
use crate::models::agent::Agent;
use crate::models::model_provider::ModelProvider;
use crate::pkg::RequestContext;
use crate::service::dao::agent::AgentDaoTrait;
use crate::service::dao::brain::dao as brain_dao;
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
    /// 如果传入 Some(model_provider) → 更新 Agent po 中的 model_provider_id，然后唤醒 brain
    /// 如果传入 None → 使用 Agent po 中已存储的 model_provider_id 查询 ModelProvider，然后唤醒 brain
    ///
    /// 唤醒完成后将 brain 写入 Agent 的 brain 字段
    fn wake_brain(&self, ctx: RequestContext, agent: &mut Agent, model_provider: Option<&ModelProvider>) -> Result<(), AppError>;
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

    fn wake_brain(&self, ctx: RequestContext, agent: &mut Agent, model_provider: Option<&ModelProvider>) -> Result<(), AppError> {
        // 1. 如果传入了 ModelProvider，更新 model_provider_id
        if let Some(mp) = model_provider {
            agent.po.model_provider_id = mp.po.id.clone();
        }

        // 2. 获取 ModelProvider（如果没传入，用 agent.po.model_provider_id 查询）
        let mp = match model_provider {
            Some(mp) => mp.clone(),
            None => {
                let Some(mp) = crate::service::dal::model_provider_dal().find_by_id(ctx.clone(), &agent.po.model_provider_id)? else {
                    return Err(AppError::NotFound(format!(
                        "ModelProvider {} not found for Agent {}",
                        agent.po.model_provider_id, agent.id()
                    )));
                };
                mp
            }
        };

        // 3. 通过 BrainDao 创建 brain
        let brain = brain_dao().create_brain(&mp.po)?;

        // 4. 写入 agent brain 字段（提取 cortex）
        agent.set_brain(brain.cortex);

        // 5. 如果更新了 model_provider_id，需要更新数据库
        if model_provider.is_some() {
            self.update(ctx, agent)?;
        }

        Ok(())
    }
}
