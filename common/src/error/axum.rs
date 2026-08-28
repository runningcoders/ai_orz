//! axum IntoResponse 实现 for common::error::Error

use axum::{
    Json,
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::json;

use crate::error::Error;

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let http_status = self.code.http_status();
        let status = StatusCode::from_u16(http_status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let code_i32 = http_status as i32;
        let error_code_str = self.code.code_str();
        let message = self.msg.clone();

        let body = Json(json!({
            "code": code_i32,
            "error_code": error_code_str,
            "message": message,
            "data": null
        }));

        let mut extra_headers = HeaderMap::new();
        for (name, value) in self.extra_response_headers {
            if let (Ok(n), Ok(v)) = (HeaderName::try_from(name), HeaderValue::try_from(value)) {
                extra_headers.append(n, v);
            }
        }

        let mut response = (status, body).into_response();
        for (name, value) in extra_headers {
            if let Some(name) = name {
                response.headers_mut().append(name, value);
            }
        }
        response
    }
}
