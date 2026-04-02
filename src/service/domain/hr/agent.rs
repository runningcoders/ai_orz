//! HR (Human Resources) Domain 模块
//!
//! Domain 层是抽象业务逻辑，关注人力资源（Agent + 员工）的通用行为逻辑
//! 组合多个 DAL 完成业务逻辑，不关心具体的实现细节
//! 人力资源模块管理所有智能体(Agent)和员工(Employee)

use crate::error::AppError;
use crate::models::agent::Agent;
use crate::pkg::RequestContext;
use crate::service::dal::agent::AgentDalTrait;
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static HR_DOMAIN: OnceLock<Arc<HrDomain>> = OnceLock::new();

/// 获取 HR Domain 单例
pub fn domain() -> Arc<HrDomain> {
    HR_DOMAIN.get().cloned().unwrap()
}

/// 初始化 HR Domain
pub fn init(agent_dal: Arc<dyn AgentDalTrait>) {
    let _ = HR_DOMAIN.set(Arc::new(HrDomain::new(agent_dal)));
}

// ==================== Domain 实现 ====================

/// HR Domain 业务逻辑
///
/// 人力资源模块管理：
/// - 智能体(Agent) - AI 智能体
/// - 员工(Employee) - 人类员工
pub struct HrDomain {
    agent_dal: Arc<dyn AgentDalTrait>,
}

impl HrDomain {
    /// 创建 Domain 实例
    pub fn new(agent_dal: Arc<dyn AgentDalTrait>) -> Self {
        Self { agent_dal }
    }

    /// 创建 Agent
    ///
    /// 基础操作：将 Agent 持久化到存储
    pub fn create(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError> {
        self.agent_dal.create(ctx, agent)
    }

    /// 获取 Agent
    ///
    /// 基础操作：根据 ID 查询 Agent
    pub fn get(&self, ctx: RequestContext, id: &str) -> Result<Option<Agent>, AppError> {
        self.agent_dal.find_by_id(ctx, id)
    }

    /// 列出所有 Agent
    ///
    /// 基础操作：查询所有有效的 Agent
    pub fn list(&self, ctx: RequestContext) -> Result<Vec<Agent>, AppError> {
        self.agent_dal.find_all(ctx)
    }

    /// 更新 Agent
    ///
    /// 基础操作：更新 Agent 信息
    pub fn update(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError> {
        self.agent_dal.update(ctx, agent)
    }

    /// 删除 Agent
    ///
    /// 基础操作：软删除 Agent（标记为已删除）
    pub fn delete(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError> {
        self.agent_dal.delete(ctx, agent)
    }
}
