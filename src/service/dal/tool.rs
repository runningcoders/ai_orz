//! Tool DAL 模块
//!
//! 基础工具数据访问层，提供工具查询和管理能力
//! 负责组合 DAO 完成业务级数据操作

use crate::error::AppError;
use crate::models::tool::{Tool, ToolPo};
use crate::pkg::request_context::RequestContext;
use crate::service::dao::tool::{ToolDao, self};
use anyhow::Result;
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static TOOL_DAL: OnceLock<Arc<dyn ToolDal>> = OnceLock::new();

/// 获取 Tool DAL 单例
pub fn dal() -> Arc<dyn ToolDal> {
    TOOL_DAL.get().cloned().unwrap()
}

/// 初始化 Tool DAL（使用全局单例 DAO）
pub fn init() {
    let _ = TOOL_DAL.set(new(
        tool::dao(),
    ));
}

/// 创建 Tool DAL（返回 trait 对象）
pub fn new(
    tool_dao: Arc<dyn ToolDao + Send + Sync>,
) -> Arc<dyn ToolDal> {
    Arc::new(ToolDalImpl {
        tool_dao,
    })
}

// ==================== DAL 接口 ====================

/// Tool DAL 接口
#[async_trait::async_trait]
pub trait ToolDal: Send + Sync {
    /// 创建新工具
    async fn create_tool(&self, ctx: &RequestContext, po: &ToolPo) -> Result<(), AppError>;

    /// 更新现有工具
    async fn update_tool(&self, ctx: &RequestContext, po: &ToolPo) -> Result<(), AppError>;

    /// 根据 ID 获取工具 PO
    async fn get_by_id(&self, ctx: &RequestContext, id: String) -> Result<Option<ToolPo>, AppError>;

    /// 根据 ID 获取完整工具（PO + ToolDyn 实例）
    async fn get_tool_full(&self, ctx: &RequestContext, id: String) -> Result<Option<Tool>, AppError>;

    /// 根据名称获取工具
    async fn get_by_name(&self, ctx: &RequestContext, name: &str) -> Result<Option<ToolPo>, AppError>;

    /// 获取所有启用的工具
    async fn list_enabled(&self, ctx: &RequestContext) -> Result<Vec<ToolPo>, AppError>;

    /// 获取 Agent 的所有完整工具（每个都是 PO + ToolDyn）
    async fn list_tools_for_agent_full(&self, ctx: &RequestContext, agent_id: &str) -> Result<Vec<Tool>, AppError>;

    /// 获取 Agent 的所有工具（仅 PO）
    async fn list_tools_for_agent(&self, ctx: &RequestContext, agent_id: &str) -> Result<Vec<ToolPo>, AppError>;

    /// 添加工具到 Agent
    async fn add_tool_to_agent(
        &self,
        ctx: &RequestContext,
        agent_id: &str,
        tool_id: &str,
        created_by: Option<String>,
    ) -> Result<(), AppError>;

    /// 从 Agent 移除工具
    async fn remove_tool_from_agent(
        &self,
        ctx: &RequestContext,
        agent_id: &str,
        tool_id: &str,
    ) -> Result<(), AppError>;

    /// 同步所有注册的内置工具到数据库
    /// 已存在的工具（按 ID）跳过，避免重复
    /// 返回新增的工具数量
    async fn sync_builtin_tools_to_db(&self, ctx: &RequestContext) -> Result<usize, AppError>;
}

// ==================== DAL 实现 ====================

/// Tool DAL 基础实现
pub struct ToolDalImpl {
    tool_dao: Arc<dyn ToolDao + Send + Sync>,
}

#[async_trait::async_trait]
impl ToolDal for ToolDalImpl {
    async fn create_tool(&self, ctx: &RequestContext, po: &ToolPo) -> Result<(), AppError> {
        Ok(self.tool_dao.create_tool(ctx, po).await?)
    }

    async fn update_tool(&self, ctx: &RequestContext, po: &ToolPo) -> Result<(), AppError> {
        Ok(self.tool_dao.update_tool(ctx, po).await?)
    }

    async fn get_by_id(&self, ctx: &RequestContext, id: String) -> Result<Option<ToolPo>, AppError> {
        Ok(self.tool_dao.get_by_id(ctx, id).await?)
    }

    async fn get_tool_full(&self, ctx: &RequestContext, id: String) -> Result<Option<Tool>, AppError> {
        Ok(self.tool_dao.get_tool_full(ctx, id).await?)
    }

    async fn get_by_name(&self, ctx: &RequestContext, name: &str) -> Result<Option<ToolPo>, AppError> {
        Ok(self.tool_dao.get_by_name(ctx, name).await?)
    }

    async fn list_enabled(&self, ctx: &RequestContext) -> Result<Vec<ToolPo>, AppError> {
        Ok(self.tool_dao.list_enabled(ctx).await?)
    }

    async fn list_tools_for_agent_full(&self, ctx: &RequestContext, agent_id: &str) -> Result<Vec<Tool>, AppError> {
        Ok(self.tool_dao.list_tools_for_agent_full(ctx, agent_id).await?)
    }

    async fn list_tools_for_agent(&self, ctx: &RequestContext, agent_id: &str) -> Result<Vec<ToolPo>, AppError> {
        Ok(self.tool_dao.list_tools_for_agent(ctx, agent_id).await?)
    }

    async fn add_tool_to_agent(
        &self,
        ctx: &RequestContext,
        agent_id: &str,
        tool_id: &str,
        created_by: Option<String>,
    ) -> Result<(), AppError> {
        Ok(self.tool_dao.add_tool_to_agent(ctx, agent_id, tool_id, created_by).await?)
    }

    async fn remove_tool_from_agent(
        &self,
        ctx: &RequestContext,
        agent_id: &str,
        tool_id: &str,
    ) -> Result<(), AppError> {
        Ok(self.tool_dao.remove_tool_from_agent(ctx, agent_id, tool_id).await?)
    }

    async fn sync_builtin_tools_to_db(&self, ctx: &RequestContext) -> Result<usize, AppError> {
        Ok(self.tool_dao.sync_builtin_tools_to_db(ctx).await?)
    }
}
