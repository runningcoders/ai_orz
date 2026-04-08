//! HTTP Handler 层

use crate::pkg::RequestContext;
use axum::http;
use axum::http::HeaderMap;
use serde::Serialize;

pub mod health;
pub mod hr;
pub mod finance;
pub mod organization;

pub use hr::agent::{create_agent, delete_agent, get_agent, list_agents, update_agent};
pub use finance::model_provider::{
    create_model_provider, delete_model_provider, get_model_provider, list_model_providers, update_model_provider,
};
pub use organization::{
    create_user, delete_organization, delete_user, get_organization, get_user_by_username, initialize_system,
    list_organizations, list_users_by_organization, update_organization, update_user,
};

/// 从 HeaderMap 提取 RequestContext
pub fn extract_ctx(headers: &HeaderMap) -> RequestContext {
    let user_id = headers
        .get("X-User-Id")
        .and_then(|v: &http::HeaderValue| v.to_str().ok())
        .map(|s: &str| s.to_string());
    let user_name = headers
        .get("X-User-Name")
        .and_then(|v: &http::HeaderValue| v.to_str().ok())
        .map(|s: &str| s.to_string());
    RequestContext::new(user_id, user_name)
}

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
