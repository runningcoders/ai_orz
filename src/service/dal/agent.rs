//! Agent DAL 模块

use crate::error::AppError;
use crate::models::agent::AgentPo;
use crate::pkg::RequestContext;
use crate::service::dao::agent::AgentDaoTrait;

/// Agent 业务对象（DAL 层）
///
/// 组合 AgentPo 和其他相关信息，作为业务层的核心对象
/// 后续可扩展：执行环境、权限、配置等字段
#[derive(Debug, Clone)]
pub struct Agent {
    /// 底层持久化对象
    pub po: AgentPo,
    // 后续扩展字段：
    // pub execution_env: ExecutionEnv,
    // pub permissions: Vec<Permission>,
    // pub config: AgentConfig,
}

impl Agent {
    /// 从 Po 创建 Agent
    pub fn from_po(po: AgentPo) -> Self {
        Self { po }
    }

    /// 转换为 Po
    pub fn into_po(self) -> AgentPo {
        self.po
    }

    /// 获取 Agent ID
    pub fn id(&self) -> &str {
        &self.po.id
    }

    /// 获取 Agent 名称
    pub fn name(&self) -> &str {
        &self.po.name
    }
}

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
    fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError>;
}

/// Agent DAL 实现
pub struct AgentDal {
    agent_dao: std::sync::Arc<dyn AgentDaoTrait>,
}

impl AgentDal {
    /// 创建 DAL 实例
    pub fn new(agent_dao: std::sync::Arc<dyn AgentDaoTrait>) -> Self {
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
            .map(|agents| agents.into_iter().map(Agent::from_po).collect())
    }

    fn update(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError> {
        self.agent_dao.update(ctx, &agent.po)
    }

    fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError> {
        self.agent_dao.delete(ctx, id)
    }
}

// ==================== 单例管理 ====================

use std::sync::OnceLock;

static AGENT_DAL: OnceLock<std::sync::Arc<dyn AgentDalTrait>> = OnceLock::new();

/// 获取 Agent DAL 单例
pub fn dal() -> std::sync::Arc<dyn AgentDalTrait> {
    AGENT_DAL.get().cloned().unwrap()
}

/// 初始化 Agent DAL
pub fn init(agent_dao: std::sync::Arc<dyn AgentDaoTrait>) {
    let _ = AGENT_DAL.set(std::sync::Arc::new(AgentDal::new(agent_dao)));
}
