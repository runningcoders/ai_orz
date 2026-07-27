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
    /// Structured error context.
    pub field: Option<ErrorField>,
    /// Underlying source error (not serialized, not deserialized).
    #[serde(skip_serializing)]
    #[serde(skip_deserializing)]
    pub source: Option<Arc<AnyhowError>>,
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
        }
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
        self.field = Some(field);
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

    /// Attach tool call trace reference to this error.
    pub fn set_tool_trace(&mut self, trace_ref: serde_json::Value) {
        let mut field = ErrorField::default();
        field.insert("trace_ref".into(), trace_ref);
        self.field = Some(field);
    }

    /// Get reference to structured fields.
    pub fn field(&self) -> Option<&ErrorField> {
        self.field.as_ref()
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

/// Convert rig::tool::ToolError to our Error
#[cfg(feature = "rig-integration")]
impl From<rig::tool::ToolError> for Error {
    fn from(err: rig::tool::ToolError) -> Self {
        Error::new(
            crate::error::ErrorCode::ToolExecutionFailed,
            err.to_string(),
        )
        .with_source(err)
    }
}
