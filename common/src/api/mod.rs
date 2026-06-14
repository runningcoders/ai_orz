//! Shared API request/response DTOs - these are used by both backend and frontend

use serde::{Deserialize, Serialize};

/// Standard API response format for all HTTP responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    /// Response code: 0 means success, non-zero means error
    pub code: i32,
    /// Response message: error message when code != 0
    pub message: String,
    /// Response data: present when code == 0
    pub data: Option<T>,
}

impl<T> ApiResponse<T> {
    /// Successful response with data
    pub fn success(data: T) -> Self {
        Self {
            code: 0,
            message: "success".to_string(),
            data: Some(data),
        }
    }

    /// Successful response without data
    pub fn ok() -> ApiResponse<()> {
        ApiResponse {
            code: 0,
            message: "success".to_string(),
            data: None,
        }
    }

    /// Error response without data
    pub fn error(code: i32, message: String) -> ApiResponse<()> {
        ApiResponse {
            code,
            message,
            data: None,
        }
    }

    /// Check if the response is successful
    pub fn is_success(&self) -> bool {
        self.code == 0
    }
}

/// Empty response for operations that don't return data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmptyResponse {}

pub mod agent;
pub mod auth;
pub mod message_channel;
pub mod model_provider;
pub mod organization;
pub mod tool;
pub mod user;

// Re-exports for convenient import
pub use agent::*;
pub use auth::*;
pub use message_channel::*;
pub use model_provider::*;
pub use organization::*;
pub use tool::*;
pub use user::*;
