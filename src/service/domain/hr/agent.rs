//! Agent 管理具体方法实现

use crate::error::AppError;
use crate::models::agent::Agent;
use crate::pkg::RequestContext;
use crate::service::dal::agent::AgentDal;
use crate::service::domain::hr::{AgentManage, HrDomainImpl};

#[async_trait::async_trait]
impl AgentManage for HrDomainImpl {
    /// 创建 Agent
    ///
    /// 基础操作：将 Agent 持久化到存储
    async fn create_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError> {
        self.agent_dal.create(ctx, agent).await
    }

    /// 获取 Agent
    ///
    /// 基础操作：根据 ID 查询 Agent
    async fn get_agent(&self, ctx: RequestContext, id: &str) -> Result<Option<Agent>, AppError> {
        self.agent_dal.find_by_id(ctx, id).await
    }

    /// 列出所有 Agent
    ///
    /// 基础操作：查询所有有效的 Agent
    async fn list_agents(&self, ctx: RequestContext) -> Result<Vec<Agent>, AppError> {
        self.agent_dal.find_all(ctx).await
    }

    /// 更新 Agent
    ///
    /// 基础操作：更新 Agent 信息
    async fn update_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError> {
        self.agent_dal.update(ctx, agent).await
    }

    /// 删除 Agent
    ///
    /// 基础操作：软删除 Agent（标记为已删除）
    async fn delete_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError> {
        self.agent_dal.delete(ctx, agent).await
    }
}
