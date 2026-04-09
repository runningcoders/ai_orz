//! Agent 管理具体方法实现

use crate::error::AppError;
use crate::models::agent::Agent;
use common::constants::RequestContext;
use crate::service::dal::agent::AgentDalTrait;
use crate::service::domain::hr::{AgentManage, HrDomainImpl};

impl AgentManage for HrDomainImpl {
    /// 创建 Agent
    ///
    /// 基础操作：将 Agent 持久化到存储
    fn create_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError> {
        self.agent_dal.create(ctx, agent)
    }

    /// 获取 Agent
    ///
    /// 基础操作：根据 ID 查询 Agent
    fn get_agent(&self, ctx: RequestContext, id: &str) -> Result<Option<Agent>, AppError> {
        self.agent_dal.find_by_id(ctx, id)
    }

    /// 列出所有 Agent
    ///
    /// 基础操作：查询所有有效的 Agent
    fn list_agents(&self, ctx: RequestContext) -> Result<Vec<Agent>, AppError> {
        self.agent_dal.find_all(ctx)
    }

    /// 更新 Agent
    ///
    /// 基础操作：更新 Agent 信息
    fn update_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError> {
        self.agent_dal.update(ctx, agent)
    }

    /// 删除 Agent
    ///
    /// 基础操作：软删除 Agent（标记为已删除）
    fn delete_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError> {
        self.agent_dal.delete(ctx, agent)
    }
}
