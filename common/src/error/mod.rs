//! Shared typed error model for ai_orz.

mod code;
mod macros;
mod types;

#[cfg(feature = "axum-integration")]
mod axum;

pub use code::*;
pub use code::ErrorCode;
pub use crate::{bail_err, define_error_codes, ensure_err, err};
pub use types::{Error, ErrorField, ErrorType, Result};
