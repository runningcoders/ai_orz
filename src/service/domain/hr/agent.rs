//! Agent 管理具体方法实现

use crate::error::AppError;
use crate::models::agent::Agent;
use crate::pkg::RequestContext;
use crate::service::dao::agent::AgentQuery;
use crate::service::dal::agent::AgentDal;
use crate::service::domain::hr::{AgentManage, HrDomainImpl};
use common::enums::AgentStatus;

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

    /// 通用综合查询
    ///
    /// Domain 层可以添加业务逻辑：权限校验、数据过滤、业务规则验证
    async fn query(
        &self,
        ctx: RequestContext,
        query: AgentQuery,
    ) -> Result<Vec<Agent>, AppError> {
        self.agent_dal.query(ctx, query).await
    }

    /// 列出所有 Agent
    ///
    /// 语法糖：调用通用查询，默认排除已删除状态
    async fn list_agents(&self, ctx: RequestContext) -> Result<Vec<Agent>, AppError> {
        self.query(ctx, AgentQuery {
            exclude_status: Some(AgentStatus::Deleted),
            ..Default::default()
        }).await
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
