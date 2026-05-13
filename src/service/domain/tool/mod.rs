//! Tool Domain 模块
//!
//! 负责工具的管理与执行：
//! - ToolManagement: 工具 CRUD、绑定解绑、启用禁用、内置工具同步
//! - ToolExecution: 工具执行（单次/批量），返回调用结果和跟踪信息

use async_trait::async_trait;
use std::fmt::Debug;

use crate::error::AppError;
use crate::models::tool::Tool;
use crate::pkg::request_context::RequestContext;

mod management;
mod execution;

pub use management::ToolManagement;
pub use execution::ToolExecution;

/// Tool Domain 主 trait
#[async_trait]
pub trait ToolDomain: Send + Sync + Debug {
    /// 获取工具管理子模块
    fn management(&self) -> &dyn ToolManagement;

    /// 获取工具执行子模块
    fn execution(&self) -> &dyn ToolExecution;
}

/// Tool Domain 默认实现
#[derive(Debug, Clone)]
pub struct ToolDomainImpl {
    management: management::ToolManagementImpl,
    execution: execution::ToolExecutionImpl,
}

impl ToolDomainImpl {
    /// 创建新的 ToolDomain 实例
    pub fn new() -> Self {
        Self {
            management: management::ToolManagementImpl::new(),
            execution: execution::ToolExecutionImpl::new(),
        }
    }
}

impl Default for ToolDomainImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolDomain for ToolDomainImpl {
    fn management(&self) -> &dyn ToolManagement {
        &self.management
    }

    fn execution(&self) -> &dyn ToolExecution {
        &self.execution
    }
}

/// Thread-safe singleton instance
static TOOL_DOMAIN_INSTANCE: std::sync::OnceLock<ToolDomainImpl> = std::sync::OnceLock::new();

/// Get the global ToolDomain instance
pub fn instance() -> &'static dyn ToolDomain {
    TOOL_DOMAIN_INSTANCE.get_or_init(ToolDomainImpl::new)
}

/// Get the ToolManagement instance (convenience)
pub fn management() -> &'static dyn ToolManagement {
    instance().management()
}

/// Get the ToolExecution instance (convenience)
pub fn execution() -> &'static dyn ToolExecution {
    instance().execution()
}
