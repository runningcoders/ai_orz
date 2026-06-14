use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::Error as JsonError;
use sqlx::Error as SqlxError;
use sqlx::migrate::MigrateError;
use std::fmt;

/// 统一 Result 类型
pub type Result<T> = std::result::Result<T, AppError>;

/// 统一错误类型
#[derive(Debug)]
#[allow(dead_code)]
pub enum AppError {
    NotFound(String),
    BadRequest(String),
    Internal(String),
    Io(std::io::Error),
    ChannelPushError(String),
    Unsupported(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::NotFound(msg) => write!(f, "Not found: {}", msg),
            AppError::BadRequest(msg) => write!(f, "Bad request: {}", msg),
            AppError::Internal(msg) => write!(f, "Internal error: {}", msg),
            AppError::Io(err) => write!(f, "IO error: {}", err),
            AppError::ChannelPushError(msg) => write!(f, "Channel push error: {}", msg),
            AppError::Unsupported(msg) => write!(f, "Unsupported operation: {}", msg),
        }
    }
}

impl std::error::Error for AppError {}

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

impl From<SqlxError> for AppError {
    fn from(err: SqlxError) -> Self {
        AppError::Internal(format!("数据库错误: {}", err))
    }
}

impl From<MigrateError> for AppError {
    fn from(err: MigrateError) -> Self {
        AppError::Internal(format!("数据库迁移错误: {}", err))
    }
}

impl From<bincode::error::EncodeError> for AppError {
    fn from(err: bincode::error::EncodeError) -> Self {
        AppError::Internal(format!("向量编码错误: {}", err))
    }
}

impl From<bincode::error::DecodeError> for AppError {
    fn from(err: bincode::error::DecodeError) -> Self {
        AppError::Internal(format!("向量解码错误: {}", err))
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(err)
    }
}

impl From<JsonError> for AppError {
    fn from(err: JsonError) -> Self {
        AppError::Internal(format!("Json error: {}", err))
    }
}

impl From<lancedb::Error> for AppError {
    fn from(err: lancedb::Error) -> Self {
        AppError::Internal(format!("LanceDB error: {}", err))
    }
}

impl AppError {
    /// 获取错误码
    pub fn code(&self) -> i32 {
        match self {
            AppError::NotFound(_) => 404,
            AppError::BadRequest(_) => 400,
            AppError::Internal(_) => 500,
            AppError::Io(_) => 500,
            AppError::ChannelPushError(_) => 500,
            AppError::Unsupported(_) => 400,
        }
    }

    /// 判断是否是 NotFound 错误
    pub fn is_not_found(&self) -> bool {
        matches!(self, AppError::NotFound(_))
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, 404, msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, 400, msg),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, 500, msg),
            AppError::Io(err) => (StatusCode::INTERNAL_SERVER_ERROR, 500, err.to_string()),
            AppError::ChannelPushError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, 500, msg),
            AppError::Unsupported(msg) => (StatusCode::BAD_REQUEST, 400, msg),
        };

        let body = Json(serde_json::json!({
            "code": code,
            "message": message,
            "data": null
        }));

        (status, body).into_response()
    }
}
