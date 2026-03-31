//! 常量定义模块
//!
//! 分类存放不同用途的常量和类型：
//! - status: 各种实体的状态枚举（软删除等）
//! - http_header: HTTP 请求头常量
//! - request_context: 请求上下文定义

pub mod http_header;
pub mod request_context;
pub mod status;

pub use http_header::*;
pub use request_context::*;
pub use status::*;
