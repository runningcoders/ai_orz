//! HR (Human Resources) Domain 模块
//!
//! 人力资源模块，管理：
//! - Agent - AI 智能体
//! - Employee - 人类员工

pub mod agent;

#[cfg(test)]
mod agent_test;

use crate::error::AppError;
use crate::models::agent::Agent;
use crate::pkg::RequestContext;
use crate::service::dal::agent::AgentDalTrait;
use crate::service::dal::agent as agent_dal;
use std::sync::{Arc, OnceLock};

// ==================== 单例 ====================

static HR_DOMAIN: OnceLock<Arc<dyn HrDomain>> = OnceLock::new();

/// 获取 HR Domain 单例
pub fn domain() -> Arc<dyn HrDomain> {
    HR_DOMAIN.get().cloned().unwrap()
}

/// 初始化 HR Domain
pub fn init() {
    // 先初始化 DAL，DAL 会自动初始化 DAO
    crate::service::dal::agent::init();
    let hr_domain = HrDomainImpl::new(
        agent_dal::dal(),
    );
    let _ = HR_DOMAIN.set(Arc::new(hr_domain));
}

// ==================== 实现 ====================

/// HR Domain 实现
///
/// 聚合所有人力资源子功能实现
pub struct HrDomainImpl {
    agent_dal: Arc<dyn AgentDalTrait>,
}

impl HrDomainImpl {
    /// 创建 Domain 实例
    pub fn new(agent_dal: Arc<dyn AgentDalTrait>) -> Self {
        Self { agent_dal }
    }
}

impl HrDomain for HrDomainImpl {
    fn agent_manage(&self) -> &dyn crate::service::domain::hr::AgentManage {
        self
    }
}

// ==================== traits 定义 ====================

/// HR Domain 总 trait
///
/// 聚合人力资源模块所有子功能 trait
pub trait HrDomain: Send + Sync {
    /// Agent 管理能力
    fn agent_manage(&self) -> &dyn AgentManage;
}

/// Agent 管理 trait
///
/// 定义 Agent 相关的业务接口
#[async_trait::async_trait]
pub trait AgentManage: Send + Sync {
    /// 创建 Agent
    async fn create_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError>;

    /// 获取 Agent
    async fn get_agent(&self, ctx: RequestContext, id: &str) -> Result<Option<Agent>, AppError>;

    /// 列出所有 Agent
    async fn list_agents(&self, ctx: RequestContext) -> Result<Vec<Agent>, AppError>;

    /// 更新 Agent
    async fn update_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError>;

    /// 删除 Agent
    async fn delete_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError>;
}
