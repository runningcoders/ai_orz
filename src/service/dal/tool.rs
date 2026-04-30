//! Tool DAL 模块
//!
//! 基础工具数据访问层，提供工具查询和管理能力
//! 负责组合 DAO 完成业务级数据操作

use crate::error::AppError;
use crate::models::tool::{Tool, ToolPo, CoreTool};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_tracing::entry::ToolCallEntry;
use crate::service::dao::tool::{ToolDao, ToolQuery, self};
use crate::service::dao::tool_call::{ToolCallDao, self};
use rig::tool::ToolError;
use anyhow::Result;
use serde_json::Value;
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
        tool_call::dao(),
    ));
}

/// 创建 Tool DAL（返回 trait 对象）
pub fn new(
    tool_dao: Arc<dyn ToolDao + Send + Sync>,
    tool_call_dao: Arc<dyn ToolCallDao + Send + Sync>,
) -> Arc<dyn ToolDal> {
    Arc::new(ToolDalImpl {
        tool_dao,
        tool_call_dao,
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

    /// 根据 ID 获取完整工具（PO + CoreTool 实例）
    async fn get_by_id(&self, ctx: &RequestContext, id: String) -> Result<Option<Tool>, AppError>;

    /// 根据名称获取完整工具
    async fn get_by_name(&self, ctx: &RequestContext, name: &str) -> Result<Option<Tool>, AppError>;

    /// 通用综合查询（返回完整 Tool 实体，包含 PO + CoreTool）
    ///
    /// 支持组合查询条件，所有字段都是 Option
    async fn query(&self, ctx: &RequestContext, query: ToolQuery) -> Result<Vec<Tool>, AppError>;

    /// 获取所有启用的工具
    async fn list_enabled(&self, ctx: &RequestContext) -> Result<Vec<Tool>, AppError>;

    /// 获取 Agent 的所有完整工具（每个都是 PO + CoreTool）
    async fn list_tools_for_agent_full(&self, ctx: &RequestContext, agent_id: &str) -> Result<Vec<Tool>, AppError>;

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

    /// 执行工具调用（通过工具 ID）
    /// 自动获取完整工具实体然后执行
    async fn call_tool_by_id(
        &self,
        ctx: &RequestContext,
        tool_id: String,
        args: Value,
    ) -> Result<Value, ToolError>;

    /// 直接执行已获取的工具
    /// 用于上层已经获取工具的场景（避免重复查询）
    async fn call_tool(
        &self,
        ctx: &RequestContext,
        tool: &Tool,
        args: Value,
    ) -> Result<Value, ToolError>;

    /// 手动执行工具并返回完整调用追踪 entry
    /// ToolCallDao 层负责每次调用新建 LoggingDecorator 捕获本次调用信息
    async fn call_manual(
        &self,
        ctx: &RequestContext,
        tool: &Tool,
        args: Value,
    ) -> Result<(Value, ToolCallEntry), ToolError>;

    /// Wrap tools for Rig to use (convert to Box<dyn ToolDyn>)
    fn wrap_for_rig(&self, tools: &[Tool], ctx: RequestContext) -> Vec<Box<dyn rig::tool::ToolDyn>>;
}

// ==================== DAL 实现 ====================

/// Tool DAL 基础实现
pub struct ToolDalImpl {
    tool_dao: Arc<dyn ToolDao + Send + Sync>,
    tool_call_dao: Arc<dyn ToolCallDao + Send + Sync>,
}

#[async_trait::async_trait]
impl ToolDal for ToolDalImpl {
    async fn create_tool(&self, ctx: &RequestContext, po: &ToolPo) -> Result<(), AppError> {
        Ok(self.tool_dao.create_tool(ctx, po).await?)
    }

    async fn update_tool(&self, ctx: &RequestContext, po: &ToolPo) -> Result<(), AppError> {
        Ok(self.tool_dao.update_tool(ctx, po).await?)
    }

    async fn get_by_id(&self, ctx: &RequestContext, id: String) -> Result<Option<Tool>, AppError> {
        let Some(po) = self.tool_dao.get_by_id(ctx, id).await? else {
            return Ok(None);
        };
        let Some(our_tool) = self.tool_call_dao.assemble_core_tool(&po)? else {
            return Ok(None);
        };
        Ok(Some(Tool { po, our_tool }))
    }

    async fn get_by_name(&self, ctx: &RequestContext, name: &str) -> Result<Option<Tool>, AppError> {
        let Some(po) = self.tool_dao.get_by_name(ctx, name).await? else {
            return Ok(None);
        };
        let Some(our_tool) = self.tool_call_dao.assemble_core_tool(&po)? else {
            return Ok(None);
        };
        Ok(Some(Tool { po, our_tool }))
    }

    async fn query(&self, ctx: &RequestContext, query: ToolQuery) -> Result<Vec<Tool>, AppError> {
        let pos = self.tool_dao.query(ctx, query).await?;
        let mut tools = Vec::new();
        for po in pos {
            if let Some(our_tool) = self.tool_call_dao.assemble_core_tool(&po)? {
                tools.push(Tool { po, our_tool });
            }
        }
        Ok(tools)
    }

    async fn list_enabled(&self, ctx: &RequestContext) -> Result<Vec<Tool>, AppError> {
        self.query(ctx, ToolQuery { enabled_only: Some(true), ..Default::default() }).await
    }

    async fn list_tools_for_agent_full(&self, ctx: &RequestContext, agent_id: &str) -> Result<Vec<Tool>, AppError> {
        self.query(ctx, ToolQuery { agent_id: Some(agent_id.to_string()), ..Default::default() }).await
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

    async fn call_tool_by_id(
        &self,
        ctx: &RequestContext,
        tool_id: String,
        args: Value,
    ) -> Result<Value, ToolError> {
        // 获取完整工具
        let tool = self.get_by_id(ctx, tool_id.clone()).await
            .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

        let tool = tool.ok_or_else(|| ToolError::ToolCallError(format!("Tool not found: {}", tool_id).into()))?;

        // 执行工具
        self.call_tool(ctx, &tool, args).await
    }

    async fn call_tool(
        &self,
        ctx: &RequestContext,
        tool: &Tool,
        args: Value,
    ) -> Result<Value, ToolError> {
        // Delegate to call_manual and discard the entry
        self.call_manual(ctx, tool, args).await
            .map(|(value, _)| value)
    }

    async fn call_manual(
        &self,
        ctx: &RequestContext,
        tool: &Tool,
        args: Value,
    ) -> Result<(Value, ToolCallEntry), ToolError> {
        self.tool_call_dao.call_manual(ctx, tool, args).await
    }

    fn wrap_for_rig(&self, tools: &[Tool], ctx: RequestContext) -> Vec<Box<dyn rig::tool::ToolDyn>> {
        self.tool_call_dao.wrap_for_rig(tools, ctx)
    }
}
