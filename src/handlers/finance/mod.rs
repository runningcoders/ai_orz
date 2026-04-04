//! Finance (财务/模型管理) Handlers module
//!
//! 财务领域模块 HTTP 接口
//! - Model Provider - 大语言模型提供商管理

pub mod model_provider;

// handler 函数导出供路由使用
pub use self::model_provider::{
    create_model_provider, delete_model_provider, get_model_provider, list_model_providers, update_model_provider,
};
