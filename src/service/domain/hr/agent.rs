//! Agent 管理领域逻辑实现

use crate::error::AppError;
use crate::models::agent::Agent;
use crate::pkg::RequestContext;
use crate::service::dal::agent::AgentDalTrait;
use crate::service::domain::hr::{AgentManage, HrDomain};
use std::sync::Arc;

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
    fn agent_manage(&self) -> &dyn AgentManage {
        self
    }
}

impl AgentManage for HrDomainImpl {
    /// 创建 Agent
    ///
    /// 基础操作：将 Agent 持久化到存储
    fn create(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError> {
        self.agent_dal.create(ctx, agent)
    }

    /// 获取 Agent
    ///
    /// 基础操作：根据 ID 查询 Agent
    fn get(&self, ctx: RequestContext, id: &str) -> Result<Option<Agent>, AppError> {
        self.agent_dal.find_by_id(ctx, id)
    }

    /// 列出所有 Agent
    ///
    /// 基础操作：查询所有有效的 Agent
    fn list(&self, ctx: RequestContext) -> Result<Vec<Agent>, AppError> {
        self.agent_dal.find_all(ctx)
    }

    /// 更新 Agent
    ///
    /// 基础操作：更新 Agent 信息
    fn update(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError> {
        self.agent_dal.update(ctx, agent)
    }

    /// 删除 Agent
    ///
    /// 基础操作：软删除 Agent（标记为已删除）
    fn delete(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError> {
        self.agent_dal.delete(ctx, agent)
    }
}
