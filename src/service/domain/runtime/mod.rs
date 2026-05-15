//! Runtime Domain 模块
//!
//! 负责运行时执行逻辑：
//! - ToolManagement: 工具运行时查询（注意：工具配置管理在 Finance Domain）
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

/// Runtime Domain 主 trait
#[async_trait]
pub trait RuntimeDomain: Send + Sync + Debug {
    /// 获取工具管理子模块
    fn management(&self) -> &dyn ToolManagement;

    /// 获取工具执行子模块
    fn execution(&self) -> &dyn ToolExecution;
}

/// Runtime Domain 默认实现
#[derive(Debug, Clone)]
pub struct RuntimeDomainImpl {
    management: management::ToolManagementImpl,
    execution: execution::ToolExecutionImpl,
}

impl RuntimeDomainImpl {
    /// 创建新的 RuntimeDomain 实例
    pub fn new() -> Self {
        Self {
            management: management::ToolManagementImpl::new(),
            execution: execution::ToolExecutionImpl::new(),
        }
    }
}

impl Default for RuntimeDomainImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RuntimeDomain for RuntimeDomainImpl {
    fn management(&self) -> &dyn ToolManagement {
        &self.management
    }

    fn execution(&self) -> &dyn ToolExecution {
        &self.execution
    }
}

/// Thread-safe singleton instance
static RUNTIME_DOMAIN_INSTANCE: std::sync::OnceLock<RuntimeDomainImpl> = std::sync::OnceLock::new();

/// Get the global RuntimeDomain instance
pub fn instance() -> &'static dyn RuntimeDomain {
    RUNTIME_DOMAIN_INSTANCE.get_or_init(RuntimeDomainImpl::new)
}

/// Get the ToolManagement instance (convenience)
pub fn management() -> &'static dyn ToolManagement {
    instance().management()
}

/// Get the ToolExecution instance (convenience)
pub fn execution() -> &'static dyn ToolExecution {
    instance().execution()
}