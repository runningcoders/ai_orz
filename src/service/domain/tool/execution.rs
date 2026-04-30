//! Tool 执行子模块
//!
//! 负责工具的执行逻辑，支持单次和批量执行

use async_trait::async_trait;
use std::fmt::Debug;

use super::ToolDomainError;
use crate::models::tool::Tool;
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_tracing::entry::ToolCallEntry;

/// 工具执行结果
#[derive(Debug, Clone)]
pub struct ToolExecutionResult {
    /// 调用请求 ID
    pub request_id: String,
    /// 工具 ID
    pub tool_id: String,
    /// 工具名称
    pub tool_name: String,
    /// 是否成功
    pub success: bool,
    /// 结果 JSON（成功时有效）
    pub result: Option<String>,
    /// 错误信息（失败时有效）
    pub error: Option<String>,
    /// 调用耗时（毫秒）
    pub duration_ms: u64,
    /// 完整调用跟踪条目
    pub call_entry: ToolCallEntry,
}

/// Tool Execution trait
#[async_trait]
pub trait ToolExecution: Send + Sync + Debug {
    /// 执行单个工具
    async fn call_tool(
        &self,
        ctx: &RequestContext,
        tool_id: &str,
        request_id: &str,
        params: &str,
    ) -> Result<ToolExecutionResult, ToolDomainError>;

    /// 批量执行多个工具
    async fn batch_call_tools(
        &self,
        ctx: &RequestContext,
        calls: Vec<(String, String, String)>, // (tool_id, request_id, params)
    ) -> Result<Vec<ToolExecutionResult>, ToolDomainError>;
}

/// ToolExecution 默认实现
#[derive(Debug, Clone)]
pub struct ToolExecutionImpl;

impl ToolExecutionImpl {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ToolExecutionImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolExecution for ToolExecutionImpl {
    async fn call_tool(
        &self,
        _ctx: &RequestContext,
        _tool_id: &str,
        _request_id: &str,
        _params: &str,
    ) -> Result<ToolExecutionResult, ToolDomainError> {
        // TODO: 实现工具执行逻辑
        // 1. 从 ToolDal 获取工具实体
        // 2. 验证工具是否启用
        // 3. 通过 ToolCallDao.call_manual() 执行工具
        // 4. 构造返回结果
        Err(ToolDomainError::Internal("Not implemented".to_string()))
    }

    async fn batch_call_tools(
        &self,
        _ctx: &RequestContext,
        _calls: Vec<(String, String, String)>,
    ) -> Result<Vec<ToolExecutionResult>, ToolDomainError> {
        // TODO: 实现批量工具执行
        // 可以考虑并行执行
        Ok(Vec::new())
    }
}
