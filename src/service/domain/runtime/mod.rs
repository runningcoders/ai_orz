//! Runtime Domain 模块
//!
//! 【定位】运行时执行层 - 只负责动态执行逻辑，不负责任何静态配置管理
//!
//! 包含子模块：
//! - ToolExecution: 工具实际执行（单次/批量）

use async_trait::async_trait;
use std::fmt::Debug;

use crate::error::AppError;
use crate::pkg::request_context::RequestContext;

mod tool_execution;

pub use tool_execution::ToolExecution;

/// Runtime Domain 主 trait
#[async_trait]
pub trait RuntimeDomain: Send + Sync + Debug {
    /// 获取工具执行子模块
    fn tool_execution(&self) -> &dyn ToolExecution;
}

/// Runtime Domain 默认实现
#[derive(Debug, Clone)]
pub struct RuntimeDomainImpl {
    tool_execution: tool_execution::ToolExecutionImpl,
}

impl RuntimeDomainImpl {
    /// 创建新的 RuntimeDomain 实例
    pub fn new() -> Self {
        Self {
            tool_execution: tool_execution::ToolExecutionImpl::new(),
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
    fn tool_execution(&self) -> &dyn ToolExecution {
        &self.tool_execution
    }
}

/// Thread-safe singleton instance
static RUNTIME_DOMAIN_INSTANCE: std::sync::OnceLock<RuntimeDomainImpl> = std::sync::OnceLock::new();

/// Get the global RuntimeDomain instance
pub fn instance() -> &'static dyn RuntimeDomain {
    RUNTIME_DOMAIN_INSTANCE.get_or_init(RuntimeDomainImpl::new)
}

/// Get the ToolExecution instance (convenience)
pub fn tool_execution() -> &'static dyn ToolExecution {
    instance().tool_execution()
}
