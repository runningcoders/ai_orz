use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// 统一 API 响应格式
#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub code: i32,
    pub message: String,
    pub data: Option<T>,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            code: 0,
            message: "success".to_string(),
            data: Some(data),
        }
    }
}

/// 统一错误类型
#[derive(Debug)]
#[allow(dead_code)]
pub enum AppError {
    NotFound(String),
    BadRequest(String),
    Internal(String),
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
        }));

        (status, body).into_response()
    }
}
