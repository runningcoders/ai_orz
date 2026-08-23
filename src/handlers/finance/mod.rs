//! Finance (财务/模型管理) Handlers module
//!
//! 财务领域模块 HTTP 接口
//! - Model Provider - 大语言模型提供商管理

pub mod attachment;
pub mod generic_token_integration;
pub mod github_integration;
pub mod lark_integration;
pub mod mcp_server;
pub mod mcp_tool;
pub mod message;
pub mod message_channel;
pub mod model_provider;
pub mod tool;

// handler 函数导出供路由使用
