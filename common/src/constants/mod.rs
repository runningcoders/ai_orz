//! 常量定义模块
//!
//! 分类存放不同用途的常量和类型：
//! - http_header: HTTP 请求头常量
//! - request_context: 请求上下文定义
//! - utils: 公共工具函数
//!
//! All enums have been moved to `common::enums::*` grouped by business domain

pub mod http_header;
pub mod request_context;
pub mod utils;

pub use request_context::*;
