use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use common::error::{Error as NewError, ErrorType};
use serde_json::Error as JsonError;
use sqlx::Error as SqlxError;
use sqlx::migrate::MigrateError;
use std::fmt;

/// 统一 Result 类型
pub type Result<T> = std::result::Result<T, common::error::Error>;

/// 统一错误类型
#[derive(Debug)]
#[allow(dead_code)]
pub enum AppError {
    NotFound(String),
    BadRequest(String),
    Conflict(String),
    PayloadTooLarge(String),
    Internal(String),
    Io(std::io::Error),
    ChannelPushError(String),
    Unsupported(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::not_found(msg) => write!(f, "Not found: {}", msg),
            AppError::bad_request(msg) => write!(f, "Bad request: {}", msg),
            AppError::conflict(msg) => write!(f, "Conflict: {}", msg),
            AppError::PayloadTooLarge(msg) => write!(f, "Payload too large: {}", msg),
            AppError::internal(msg) => write!(f, "Internal error: {}", msg),
            AppError::Io(err) => write!(f, "IO error: {}", err),
            AppError::ChannelPushError(msg) => write!(f, "Channel push error: {}", msg),
            AppError::Unsupported(msg) => write!(f, "Unsupported operation: {}", msg),
        }
    }
}

impl std::error::Error for AppError {}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::internal(err.to_string())
    }
}

impl From<jsonwebtoken::errors::Error> for AppError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        AppError::bad_request(format!("JWT token 无效: {}", err))
    }
}

impl From<SqlxError> for AppError {
    fn from(err: SqlxError) -> Self {
        AppError::internal(format!("数据库错误: {}", err))
    }
}

impl From<MigrateError> for AppError {
    fn from(err: MigrateError) -> Self {
        AppError::internal(format!("数据库迁移错误: {}", err))
    }
}

impl From<bincode::error::EncodeError> for AppError {
    fn from(err: bincode::error::EncodeError) -> Self {
        AppError::internal(format!("向量编码错误: {}", err))
    }
}

impl From<bincode::error::DecodeError> for AppError {
    fn from(err: bincode::error::DecodeError) -> Self {
        AppError::internal(format!("向量解码错误: {}", err))
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Io(err)
    }
}

impl From<JsonError> for AppError {
    fn from(err: JsonError) -> Self {
        AppError::internal(format!("Json error: {}", err))
    }
}

impl From<lancedb::Error> for AppError {
    fn from(err: lancedb::Error) -> Self {
        AppError::internal(format!("LanceDB error: {}", err))
    }
}

/// Convert old AppError to new unified Error.
impl From<AppError> for NewError {
    fn from(err: AppError) -> Self {
        use common::error::ErrorCode;
use common::bail_err;
        match err {
            AppError::not_found(ref msg) => NewError::typed(
                ErrorCode::NotFound,
                ErrorType::Biz,
                msg.clone(),
            ).with_source(err),
            AppError::bad_request(ref msg) => NewError::typed(
                ErrorCode::InvalidRequest,
                ErrorType::Validation,
                msg.clone(),
            ).with_source(err),
            AppError::conflict(ref msg) => NewError::typed(
                ErrorCode::Conflict,
                ErrorType::Biz,
                msg.clone(),
            ).with_source(err),
            AppError::PayloadTooLarge(ref msg) => NewError::typed(
                ErrorCode::PayloadTooLarge,
                ErrorType::Validation,
                msg.clone(),
            ).with_source(err),
            AppError::internal(ref msg) => NewError::typed(
                ErrorCode::Internal,
                ErrorType::System,
                msg.clone(),
            ).with_source(err),
            AppError::Io(err) => NewError::io_error(err.to_string()).with_source(err),
            AppError::ChannelPushError(ref msg) => NewError::typed(
                ErrorCode::ChannelPushFailed,
                ErrorType::Runtime,
                msg.clone(),
            ).with_source(err),
            AppError::Unsupported(ref msg) => NewError::typed(
                ErrorCode::UnsupportedOperation,
                ErrorType::Biz,
                msg.clone(),
            ).with_source(err),
        }
    }
}

impl AppError {
    /// 获取错误码
    pub fn code(&self) -> i32 {
        match self {
            AppError::not_found(_) => 404,
            AppError::bad_request(_) => 400,
            AppError::conflict(_) => 409,
            AppError::PayloadTooLarge(_) => 413,
            AppError::internal(_) => 500,
            AppError::Io(_) => 500,
            AppError::ChannelPushError(_) => 500,
            AppError::Unsupported(_) => 400,
        }
    }

    /// 判断是否是 NotFound 错误
    pub fn is_not_found(&self) -> bool {
        matches!(self, AppError::not_found(_))
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            AppError::not_found(msg) => (StatusCode::NOT_FOUND, 404, msg),
            AppError::bad_request(msg) => (StatusCode::BAD_REQUEST, 400, msg),
            AppError::conflict(msg) => (StatusCode::CONFLICT, 409, msg),
            AppError::PayloadTooLarge(msg) => (StatusCode::PAYLOAD_TOO_LARGE, 413, msg),
            AppError::internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, 500, msg),
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
