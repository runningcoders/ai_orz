//! Shared error types.

use std::fmt;
use std::sync::Arc;

use anyhow::{self, Error as AnyhowError};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// High-level class of an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorType {
    /// Input validation failure.
    Validation,
    /// Business-rule failure.
    Biz,
    /// Authentication failure.
    Auth,
    /// Authorization or permission failure.
    Permission,
    /// Database or persistence failure.
    Db,
    /// File-system or IO failure.
    Io,
    /// Third-party dependency failure.
    Third,
    /// Tool execution or tool protocol failure.
    Tool,
    /// Runtime orchestration failure.
    Runtime,
    /// Network request failure.
    Network,
    /// Model (LLM) invocation failure.
    Model,

    /// Configuration failure.
    Config,
    /// System internal error.
    System,
}

/// A single error field (structured context).
///
/// Supports both known structured fields and generic dynamic map for extension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorField {
    /// Tool execution trace reference (if available).
    pub trace_ref: Option<crate::models::ToolCallTraceRef>,
    /// Generic dynamic fields for extension.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl ErrorField {
    /// Create a new empty ErrorField.
    pub fn new() -> Self {
        Self {
            trace_ref: None,
            extra: Map::new(),
        }
    }

    /// Insert a key-value pair into the generic extra map.
    pub fn insert(&mut self, key: String, value: Value) {
        self.extra.insert(key, value);
    }

    /// Get a value by key from the generic extra map.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.extra.get(key)
    }

    /// Set the trace_ref field.
    pub fn set_trace_ref(&mut self, trace_ref: crate::models::ToolCallTraceRef) {
        self.trace_ref = Some(trace_ref);
    }
}

impl Default for ErrorField {
    fn default() -> Self {
        Self::new()
    }
}

impl std::ops::Deref for ErrorField {
    type Target = Map<String, Value>;

    fn deref(&self) -> &Self::Target {
        &self.extra
    }
}

impl std::ops::DerefMut for ErrorField {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.extra
    }
}

/// Unified error type for ai_orz.
///
/// - Pure unit ErrorCode for easy matching
/// - ErrorType for high-level classification
/// - Human-readable message
/// - Optional structured fields for extra context
/// - Optional source error for chain
/// - Serializable by default (source is not serialized)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Error {
    /// Error code (stable, machine-readable).
    pub code: crate::error::ErrorCode,
    /// High-level error type.
    pub error_type: ErrorType,
    /// Human-readable message.
    pub msg: String,
    /// Structured error context (boxed to keep Error small).
    pub field: Option<Box<ErrorField>>,
    /// Underlying source error (not serialized, not deserialized).
    #[serde(skip_serializing)]
    #[serde(skip_deserializing)]
    pub source: Option<Arc<AnyhowError>>,
    /// Optional response headers to attach (HTTP adapter only; key-value string pairs).
    ///
    /// 会话相关接口在 401/会话失效时，通过此字段附 Set-Cookie 清除 JWT。
    /// 纯字符串存储以保证 `Error` 在非 HTTP 编译目标（Dioxus 前端等）下仍可编译。
    /// Boxed 以保持 Error 栈尺寸不触 result_large_err 红线。
    pub extra_response_headers: Box<[(String, String)]>,
}

impl Error {
    /// Create a new error from code and message.
    /// ErrorType is automatically inferred from code.
    pub fn new(code: crate::error::ErrorCode, msg: impl Into<String>) -> Self {
        Self {
            code,
            error_type: code.error_type(),
            msg: msg.into(),
            field: None,
            source: None,
            extra_response_headers: Box::new([]),
        }
    }

    /// Create a new error with explicit error_type override.
    pub fn typed(
        code: crate::error::ErrorCode,
        error_type: ErrorType,
        msg: impl Into<String>,
    ) -> Self {
        Self {
            code,
            error_type,
            msg: msg.into(),
            field: None,
            source: None,
            extra_response_headers: Box::new([]),
        }
    }

    /// Attach a raw (name, value) response header.
    ///
    /// 典型用法：会话接口返回未认证时，附 `Set-Cookie` 清除 JWT，使下一次
    /// 浏览器请求立即携带空 Cookie，避免"后端清库后 JWT 仍被自动携带"。
    /// 非 HTTP 环境下这些字段静默忽略，不影响跨包使用。
    pub fn with_response_header(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        let mut v: Vec<(String, String)> = self.extra_response_headers.into_vec();
        v.push((name.into(), value.into()));
        self.extra_response_headers = v.into_boxed_slice();
        self
    }

    /// Shortcut: bad request / invalid request (400).
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::new(crate::error::ErrorCode::InvalidRequest, msg)
    }

    /// Shortcut: database error (500).
    pub fn db_error(msg: impl Into<String>) -> Self {
        Self::new(crate::error::ErrorCode::DbQueryFailed, msg)
    }

    /// Shortcut: IO error (500).
    pub fn io_error(msg: impl Into<String>) -> Self {
        Self::new(crate::error::ErrorCode::IoError, msg)
    }

    /// Shortcut: internal/system error (500).
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(crate::error::ErrorCode::Internal, msg)
    }

    /// Shortcut: not found (404).
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new(crate::error::ErrorCode::ResourceNotFound, msg)
    }

    /// Shortcut: conflict (409).
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::new(crate::error::ErrorCode::Conflict, msg)
    }

    /// Shortcut: payload too large (413).
    pub fn payload_too_large(msg: impl Into<String>) -> Self {
        Self::new(crate::error::ErrorCode::PayloadTooLarge, msg)
    }

    /// Shortcut: tool execution failed (500).
    pub fn tool_call_failed(msg: impl Into<String>) -> Self {
        Self::new(crate::error::ErrorCode::ToolExecutionFailed, msg)
    }

    /// Shortcut: unauthorized (401).
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::new(crate::error::ErrorCode::Unauthorized, msg)
    }

    /// Shortcut: forbidden / permission denied (403).
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::new(crate::error::ErrorCode::Forbidden, msg)
    }

    /// Attach structured business context fields.
    pub fn with_field(mut self, field: ErrorField) -> Self {
        self.field = Some(Box::new(field));
        self
    }

    /// Attach an internal source error.
    pub fn with_source<E>(mut self, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        self.source = Some(Arc::new(AnyhowError::new(source)));
        self
    }

    /// Stable string code for external consumers.
    pub fn code(&self) -> &'static str {
        self.code.code_str()
    }

    /// Get the structured error code enum.
    pub fn code_enum(&self) -> crate::error::ErrorCode {
        self.code
    }

    /// Default HTTP status code for HTTP adapters.
    pub fn http_status(&self) -> u16 {
        self.code.http_status()
    }

    /// 是否为"模型调用错误"。
    ///
    /// 这类错误由 cortex 边界（模型 HTTP 调用）生产，错误码以 `Model*` 开头。
    /// 业务层可用此方法快速识别模型类故障。
    pub fn is_model_error(&self) -> bool {
        matches!(
            self.code,
            crate::error::ErrorCode::ModelRateLimited
                | crate::error::ErrorCode::ModelServerError
                | crate::error::ErrorCode::ModelBadRequest
                | crate::error::ErrorCode::ModelAuth
                | crate::error::ErrorCode::ModelContentFiltered
        )
    }

    /// 判断该错误是否可重试（通用复用方法，业务层按需调用自行决定是否重试）。
    ///
    /// 当前仅针对"模型调用错误"给出分类：
    /// - 可重试：限流(429) / 服务端或网关错误(5xx)
    /// - 不可重试：鉴权(401/403) / 请求本身非法(400/422) / 内容过滤
    ///
    /// 业务层（如 AOP 消费者）可调用此方法决定 nack 重试与否，
    /// 例如在限流期间退避重试，或对不可重试错误直接 ack 并通知用户。
    pub fn is_retryable(&self) -> bool {
        matches!(
            self.code,
            crate::error::ErrorCode::ModelRateLimited | crate::error::ErrorCode::ModelServerError
        )
    }

    /// 面向用户的提示文案（统一管理在错误码系统中）。
    ///
    /// 优先使用错误码通过 `message:` 字段定义的用户文案；
    /// 若未定义，则回退到错误本身的详细文本（`self.msg`）。
    pub fn user_message(&self) -> String {
        match self.code.user_message() {
            Some(m) => m.to_string(),
            None => self.msg.clone(),
        }
    }

    /// Attach tool call trace reference to this error.
    pub fn set_tool_trace(&mut self, trace_ref: serde_json::Value) {
        let mut field = ErrorField::default();
        field.insert("trace_ref".into(), trace_ref);
        self.field = Some(Box::new(field));
    }

    /// Get reference to structured fields.
    pub fn field(&self) -> Option<&ErrorField> {
        self.field.as_deref()
    }

    /// Get source error.
    pub fn source(&self) -> Option<&AnyhowError> {
        self.source.as_ref().map(|v| v.as_ref())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code.code_str(), self.msg)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

#[cfg(feature = "sqlx")]
/// Convert sqlx::Error to our Error (maps to DbError)
impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        Error::db_error(err.to_string()).with_source(err)
    }
}

/// Convert std::io::Error to our Error (maps to IoError)
impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::io_error(err.to_string()).with_source(err)
    }
}

/// Convert anyhow::Error to our Error (maps to SystemError)
impl From<AnyhowError> for Error {
    fn from(err: AnyhowError) -> Self {
        use crate::error::{ErrorCode, ErrorType};
        let mut e = Error::typed(ErrorCode::Internal, ErrorType::System, err.to_string());
        e.source = Some(Arc::new(err));
        e
    }
}

#[cfg(feature = "tokio-integration")]
/// Convert tokio::task::JoinError to our Error (maps to Internal)
impl From<tokio::task::JoinError> for Error {
    fn from(err: tokio::task::JoinError) -> Self {
        use crate::error::{ErrorCode, ErrorType};
        Error::typed(ErrorCode::Internal, ErrorType::System, err.to_string()).with_source(err)
    }
}

#[cfg(feature = "reqwest-integration")]
/// Convert reqwest::Error to our Error (maps to Network)
use reqwest::Error as ReqwestError;
#[cfg(feature = "reqwest-integration")]
impl From<ReqwestError> for Error {
    fn from(err: ReqwestError) -> Self {
        Error::new(crate::error::ErrorCode::NetworkError, err.to_string()).with_source(err)
    }
}

/// Convert serde_json::Error to our Error (maps to Internal)
impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::internal(err.to_string()).with_source(err)
    }
}

/// Convert sqlx::migrate::MigrateError to our Error (maps to Db)
#[cfg(feature = "sqlx")]
impl From<sqlx::migrate::MigrateError> for Error {
    fn from(err: sqlx::migrate::MigrateError) -> Self {
        use crate::error::{ErrorCode, ErrorType};
        Error::typed(ErrorCode::DbMigrationFailed, ErrorType::Db, err.to_string()).with_source(err)
    }
}

/// Convert bincode::DecodeError to our Error (maps to Internal)
#[cfg(feature = "bincode-integration")]
impl From<bincode::error::DecodeError> for Error {
    fn from(err: bincode::error::DecodeError) -> Self {
        Error::internal(err.to_string()).with_source(err)
    }
}

/// Convert bincode::EncodeError to our Error (maps to Internal)
#[cfg(feature = "bincode-integration")]
impl From<bincode::error::EncodeError> for Error {
    fn from(err: bincode::error::EncodeError) -> Self {
        Error::internal(err.to_string()).with_source(err)
    }
}

/// Convert jsonwebtoken::errors::Error to our Error (maps to Auth)
#[cfg(feature = "jwt-integration")]
impl From<jsonwebtoken::errors::Error> for Error {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        Error::bad_request(err.to_string()).with_source(err)
    }
}

/// Convert base64::DecodeError to our Error (maps to InvalidRequest)
#[cfg(feature = "base64-integration")]
impl From<base64::DecodeError> for Error {
    fn from(err: base64::DecodeError) -> Self {
        Error::bad_request(err.to_string()).with_source(err)
    }
}

#[cfg(feature = "toml-integration")]
/// Convert toml::de::Error to our Error (maps to Config error)
use toml::de::Error as TomlDeError;
#[cfg(feature = "toml-integration")]
impl From<TomlDeError> for Error {
    fn from(err: TomlDeError) -> Self {
        Error::new(crate::error::ErrorCode::ConfigInvalid, err.to_string()).with_source(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;

    #[test]
    fn model_errors_are_identified() {
        for c in [
            ErrorCode::ModelRateLimited,
            ErrorCode::ModelServerError,
            ErrorCode::ModelBadRequest,
            ErrorCode::ModelAuth,
            ErrorCode::ModelContentFiltered,
        ] {
            assert!(
                Error::new(c, "x").is_model_error(),
                "{c:?} should be a model error"
            );
        }
        assert!(!Error::new(ErrorCode::Internal, "x").is_model_error());
    }

    #[test]
    fn only_rate_limit_and_server_are_retryable() {
        assert!(Error::new(ErrorCode::ModelRateLimited, "x").is_retryable());
        assert!(Error::new(ErrorCode::ModelServerError, "x").is_retryable());
        assert!(!Error::new(ErrorCode::ModelBadRequest, "x").is_retryable());
        assert!(!Error::new(ErrorCode::ModelAuth, "x").is_retryable());
        assert!(!Error::new(ErrorCode::ModelContentFiltered, "x").is_retryable());
    }

    #[test]
    fn user_message_uses_code_then_fallback() {
        assert_eq!(
            Error::new(ErrorCode::ModelRateLimited, "detail").user_message(),
            "模型服务请求过于频繁（触发限流），请稍后重试。"
        );
        // 未定义文案的错误码回退到原始详情
        assert_eq!(
            Error::new(ErrorCode::Internal, "boom").user_message(),
            "boom"
        );
    }
}
