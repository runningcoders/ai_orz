//! HR (Human Resources) Domain 模块
//!
//! 人力资源模块，管理：
//! - Agent - AI 智能体
//! - Employee - 人类员工

pub mod agent;

use crate::error::AppError;
use crate::models::agent::Agent;
use crate::pkg::RequestContext;
use std::sync::{Arc, OnceLock};

// ====================  traits 定义 ====================

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
pub trait AgentManage: Send + Sync {
    /// 创建 Agent
    fn create(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError>;

    /// 获取 Agent
    fn get(&self, ctx: RequestContext, id: &str) -> Result<Option<Agent>, AppError>;

    /// 列出所有 Agent
    fn list(&self, ctx: RequestContext) -> Result<Vec<Agent>, AppError>;

    /// 更新 Agent
    fn update(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError>;

    /// 删除 Agent
    fn delete(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError>;
}

// ==================== 单例 ====================

static HR_DOMAIN: OnceLock<Arc<dyn HrDomain>> = OnceLock::new();

/// 获取 HR Domain 单例
pub fn domain() -> Arc<dyn HrDomain> {
    HR_DOMAIN.get().cloned().unwrap()
}

/// 初始化 HR Domain
pub fn init(agent_dal: Arc<dyn crate::service::dal::agent::AgentDalTrait>) {
    let hr_domain = agent::HrDomainImpl::new(agent_dal);
    let _ = HR_DOMAIN.set(Arc::new(hr_domain));
}
