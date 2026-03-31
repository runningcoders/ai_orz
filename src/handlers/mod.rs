//! HTTP Handler 层

use serde::Serialize;

pub mod agent;
pub mod health;
pub mod organization;

pub use agent::{create_agent, delete_agent, get_agent, list_agents, update_agent};

/// 通用 API 响应包装
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub code: i32,
    pub message: String,
    pub data: Option<T>,
}

impl<T> ApiResponse<T> {
    /// 成功响应
    pub fn success(data: T) -> Self {
        Self {
            code: 0,
            message: "success".to_string(),
            data: Some(data),
        }
    }

    /// 成功响应（无数据）
    pub fn ok() -> ApiResponse<()> {
        ApiResponse {
            code: 0,
            message: "success".to_string(),
            data: None,
        }
    }

    /// 错误响应
    pub fn error(code: i32, message: String) -> ApiResponse<()> {
        ApiResponse {
            code,
            message,
            data: None,
        }
    }
}
