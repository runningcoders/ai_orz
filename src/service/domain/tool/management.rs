//! Tool 管理子模块
//!
//! 负责工具的 CRUD、绑定解绑、启用禁用、内置工具同步

use async_trait::async_trait;
use std::fmt::Debug;

use super::ToolDomainError;
use crate::models::tool::{Tool, ToolPo};
use crate::pkg::request_context::RequestContext;
use crate::service::dal::tool::ToolDal;

/// Tool Management trait
#[async_trait]
pub trait ToolManagement: Send + Sync + Debug {
    /// 同步所有内置工具到数据库
    async fn sync_builtin_tools(&self, ctx: &RequestContext) -> Result<Vec<Tool>, ToolDomainError>;

    /// 通用综合查询
    ///
    /// 支持组合查询条件，所有字段都是 Option
    async fn query(
        &self,
        ctx: &RequestContext,
        query: crate::service::dao::tool::ToolQuery,
    ) -> Result<Vec<Tool>, ToolDomainError>;

    /// 获取所有工具列表
    async fn list_tools(&self, ctx: &RequestContext) -> Result<Vec<Tool>, ToolDomainError>;

    /// 获取某个 Agent 绑定的所有工具
    async fn list_agent_tools(&self, ctx: &RequestContext, agent_id: &str) -> Result<Vec<Tool>, ToolDomainError>;

    /// 根据 ID 获取工具
    async fn get_tool(&self, ctx: &RequestContext, tool_id: &str) -> Result<Option<Tool>, ToolDomainError>;

    /// 启用工具
    async fn enable_tool(&self, ctx: &RequestContext, tool_id: &str) -> Result<(), ToolDomainError>;

    /// 禁用工具
    async fn disable_tool(&self, ctx: &RequestContext, tool_id: &str) -> Result<(), ToolDomainError>;

    /// 绑定工具到 Agent
    async fn bind_to_agent(&self, ctx: &RequestContext, agent_id: &str, tool_id: &str) -> Result<(), ToolDomainError>;

    /// 从 Agent 解绑工具
    async fn unbind_from_agent(&self, ctx: &RequestContext, agent_id: &str, tool_id: &str) -> Result<(), ToolDomainError>;

    /// 获取 Agent 绑定的工具 ID 列表
    async fn get_agent_bound_tool_ids(&self, ctx: &RequestContext, agent_id: &str) -> Result<Vec<String>, ToolDomainError>;
}

/// ToolManagement 默认实现
#[derive(Debug, Clone)]
pub struct ToolManagementImpl;

impl ToolManagementImpl {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ToolManagementImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolManagement for ToolManagementImpl {
    async fn sync_builtin_tools(&self, _ctx: &RequestContext) -> Result<Vec<Tool>, ToolDomainError> {
        // TODO: 实现内置工具同步
        // 1. 从 ToolRegistry 获取所有内置工具定义
        // 2. 检查数据库中是否已存在
        // 3. 不存在则插入，存在则更新
        Ok(Vec::new())
    }

    /// 通用综合查询
    ///
    /// Domain 层可以添加业务逻辑：权限校验、数据过滤、业务规则验证
    async fn query(
        &self,
        ctx: &RequestContext,
        query: crate::service::dao::tool::ToolQuery,
    ) -> Result<Vec<Tool>, ToolDomainError> {
        // TODO: 实现通用查询
        // 调用 ToolDal.query()
        Ok(Vec::new())
    }

    /// 获取所有工具列表
    ///
    /// 调用 DAL 层对应方法
    async fn list_tools(&self, ctx: &RequestContext) -> Result<Vec<Tool>, ToolDomainError> {
        // TODO: 调用 ToolDal 对应方法
        Ok(Vec::new())
    }

    /// 获取某个 Agent 绑定的所有工具
    ///
    /// 语法糖：调用通用查询，指定 Agent ID
    async fn list_agent_tools(&self, ctx: &RequestContext, agent_id: &str) -> Result<Vec<Tool>, ToolDomainError> {
        self.query(ctx, crate::service::dao::tool::ToolQuery {
            agent_id: Some(agent_id.to_string()),
            enabled_only: None,
            limit: None,
        }).await
    }

    async fn get_tool(&self, _ctx: &RequestContext, _tool_id: &str) -> Result<Option<Tool>, ToolDomainError> {
        // TODO: 实现工具查询
        // 调用 ToolDal.get_by_id()
        Ok(None)
    }

    async fn enable_tool(&self, _ctx: &RequestContext, _tool_id: &str) -> Result<(), ToolDomainError> {
        // TODO: 实现工具启用
        // 调用 ToolDal.enable()
        Ok(())
    }

    async fn disable_tool(&self, _ctx: &RequestContext, _tool_id: &str) -> Result<(), ToolDomainError> {
        // TODO: 实现工具禁用
        // 调用 ToolDal.disable()
        Ok(())
    }

    async fn bind_to_agent(&self, _ctx: &RequestContext, _agent_id: &str, _tool_id: &str) -> Result<(), ToolDomainError> {
        // TODO: 实现工具绑定到 Agent
        // 调用 ToolDal.bind_to_agent()
        Ok(())
    }

    async fn unbind_from_agent(&self, _ctx: &RequestContext, _agent_id: &str, _tool_id: &str) -> Result<(), ToolDomainError> {
        // TODO: 实现工具解绑
        // 调用 ToolDal.unbind_from_agent()
        Ok(())
    }

    async fn get_agent_bound_tool_ids(&self, _ctx: &RequestContext, _agent_id: &str) -> Result<Vec<String>, ToolDomainError> {
        // TODO: 实现获取 Agent 绑定的工具 ID 列表
        Ok(Vec::new())
    }
}
