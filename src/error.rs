use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json;
use std::fmt;

/// 统一错误类型
#[derive(Debug)]
#[allow(dead_code)]
pub enum AppError {
    NotFound(String),
    BadRequest(String),
    Internal(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::NotFound(msg) => write!(f, "Not found: {}", msg),
            AppError::BadRequest(msg) => write!(f, "Bad request: {}", msg),
            AppError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}

impl From<jsonwebtoken::errors::Error> for AppError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        AppError::BadRequest(format!("JWT token 无效: {}", err))
    }
}

impl AppError {
    /// 获取错误码
    pub fn code(&self) -> i32 {
        match self {
            AppError::NotFound(_) => 404,
            AppError::BadRequest(_) => 400,
            AppError::Internal(_) => 500,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, 404, msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, 400, msg),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, 500, msg),
        };

        let body = Json(serde_json::json!({
            "code": code,
            "message": message,
            "data": null
        }));

        (status, body).into_response()
    }
}
