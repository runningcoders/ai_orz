//! Shared typed error model for ai_orz.
//!
//! This is the new unified error model:
//! - ErrorCode: pure unit enum, no associated data
//! - ErrorType: coarse error classification (System, Domain, Tool, etc.)
//! - ErrorField: structured additional context stored as serde_json::Map
//! - Error: main error struct carrying all above, implements Serialize & Error
//! - Result<T>: convenient type alias = std::result::Result<T, Error>

mod code;
mod macros;
mod types;

#[cfg(feature = "axum-integration")]
mod axum;

pub use code::ErrorCode;
pub use code::*;
// Re-export macros from crate root because macros defined at crate root
pub use types::{Error, ErrorField, ErrorType};
pub use {crate::bail_err, crate::define_error_codes, crate::ensure_err, crate::err};

/// Convenient result type for ai_orz with error fixed to `common::error::Error`.
pub type Result<T> = std::result::Result<T, Error>;
