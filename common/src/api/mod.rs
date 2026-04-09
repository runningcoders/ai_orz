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
pub mod organization;
pub mod model_provider;
pub mod user;

// Re-exports for convenient import
pub use agent::*;
pub use auth::*;
pub use organization::*;
pub use model_provider::*;
pub use user::*;
