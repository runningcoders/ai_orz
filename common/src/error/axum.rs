//! axum IntoResponse 实现 for common::error::Error

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

use crate::error::{Error, ErrorCode};

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, code, message) = match self.code {
            // 基础错误码映射 HTTP 状态码
            ErrorCode::ResourceNotFound => (StatusCode::NOT_FOUND, 404, self.msg.clone()),
            ErrorCode::InvalidRequest | ErrorCode::UnsupportedOperation => (StatusCode::BAD_REQUEST, 400, self.msg.clone()),
            ErrorCode::ResourceConflict => (StatusCode::CONFLICT, 409, self.msg.clone()),
            ErrorCode::PayloadTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, 413, self.msg.clone()),
            ErrorCode::Internal | ErrorCode::DbQueryFailed | ErrorCode::IoError => (StatusCode::INTERNAL_SERVER_ERROR, 500, self.msg.clone()),
            ErrorCode::ChannelPushFailed => (StatusCode::INTERNAL_SERVER_ERROR, 500, self.msg.clone()),
            // 默认情况
            _ => (StatusCode::INTERNAL_SERVER_ERROR, 500, self.msg.clone()),
        };

        let body = Json(json!({
            "code": code,
            "message": message,
            "data": null
        }));

        (status, body).into_response()
    }
}
