//! Shared error types.

use std::fmt;

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
    /// Configuration failure.
    Config,
    /// System internal error.
    System,
}

/// A single error field (structured context).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorField(pub Map<String, Value>);

impl ErrorField {
    /// Create a new empty ErrorField.
    pub fn new() -> Self {
        Self(Map::new())
    }
    
    /// Insert a key-value pair.
    pub fn insert(&mut self, key: String, value: Value) {
        self.0.insert(key, value);
    }
    
    /// Get a value by key.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
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
        &self.0
    }
}

impl std::ops::DerefMut for ErrorField {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
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
#[derive(Debug, Serialize, Deserialize)]
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
    pub source: Option<anyhow::Error>,
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
        self.source = Some(anyhow::Error::new(source));
        self
    }

    /// Stable string code for external consumers.
    pub fn code(&self) -> &'static str {
        self.code.code_str()
    }

    /// Default HTTP status code for HTTP adapters.
    pub fn http_status(&self) -> u16 {
        self.code.http_status()
    }

    /// Get reference to structured fields.
    pub fn field(&self) -> Option<&ErrorField> {
        self.field.as_ref()
    }

    /// Get source error.
    pub fn source(&self) -> Option<&anyhow::Error> {
        self.source.as_ref()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code.code_str(), self.msg)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|e| e.as_ref() as &dyn std::error::Error)
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
impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        use crate::error::{ErrorCode, ErrorType};
        let mut e = Error::typed(
            ErrorCode::Internal,
            ErrorType::System,
            err.to_string(),
        );
        e.source = Some(err.into());
        e
    }
}

/// Convert serde_json::Error to our Error (maps to Internal)
impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
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

/// Convert base64::EncodeError to our Error (maps to InvalidRequest)
#[cfg(feature = "base64-integration")]
impl From<base64::EncodeError> for Error {
    fn from(err: base64::EncodeError) -> Self {
        Error::bad_request(err.to_string()).with_source(err)
    }
}

/// Convert sqlx::migrate::MigrateError to our Error (maps to Db error)
#[cfg(feature = "sqlx")]
impl From<sqlx::migrate::MigrateError> for Error {
    fn from(err: sqlx::migrate::MigrateError) -> Self {
        Error::db_error(err.to_string()).with_source(err)
    }
}

/// Convert reqwest::Error to our Error (maps to Third-party error)
#[cfg(feature = "reqwest-integration")]
impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Error::new(crate::error::ErrorCode::ThirdPartyError, err.to_string()).with_source(err)
    }
}

/// Convert toml::de::Error to our Error (maps to Config error)
#[cfg(feature = "toml-integration")]
impl From<toml::de::Error> for Error {
    fn from(err: toml::de::Error) -> Self {
        Error::new(crate::error::ErrorCode::ConfigError, err.to_string()).with_source(err)
    }
}

/// Convert rig::tool::ToolError to our Error
#[cfg(feature = "rig-integration")]
impl From<rig::tool::ToolError> for Error {
    fn from(err: rig::tool::ToolError) -> Self {
        Error::new(crate::error::ErrorCode::ToolExecutionFailed, err.to_string()).with_source(err)
    }
}

/// Convert tokio::task::JoinError to our Error
impl From<tokio::task::JoinError> for Error {
    fn from(err: tokio::task::JoinError) -> Self {
        Error::internal(err.to_string()).with_source(err)
    }
}

/// Convert rig::ToolExecutionError to our Error
#[cfg(feature = "rig-integration")]
impl From<rig::tool::ToolExecutionError> for Error {
    fn from(err: rig::tool::ToolExecutionError) -> Self {
        Error::new(crate::error::ErrorCode::ToolExecutionFailed, err.to_string()).with_source(err)
    }
}

/// Convert rig::ToolError to our Error
#[cfg(feature = "rig-integration")]
impl From<rig::tool::ToolError> for Error {
    fn from(err: rig::tool::ToolError) -> Self {
        Error::tool_execution_failed(err.to_string()).with_source(err)
    }
}

/// Shared result type for ai_orz.
pub type Result<T> = std::result::Result<T, Error>;