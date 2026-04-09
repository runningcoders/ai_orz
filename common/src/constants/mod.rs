//! Shared constants used by both backend and frontend

use serde::{Deserialize, Serialize};

/// API response wrapper (generic)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    /// Status code (0 = success, non-zero = error)
    pub code: i32,
    /// Response message
    pub message: String,
    /// Response data (None if error)
    pub data: Option<T>,
}

impl<T> ApiResponse<T> {
    /// Create a success response with data
    pub fn success(data: T) -> Self {
        Self {
            code: 0,
            message: "success".to_string(),
            data: Some(data),
        }
    }

    /// Create an error response
    pub fn error(code: i32, message: String) -> Self {
        Self {
            code,
            message,
            data: None,
        }
    }

    /// Check if response is successful
    pub fn is_success(&self) -> bool {
        self.code == 0
    }
}
