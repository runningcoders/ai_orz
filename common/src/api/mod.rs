//! Shared API request/response DTOs - these are used by both backend and frontend

use schemars::JsonSchema;
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

/// Unified offset-based pagination parameters.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct PaginationParams {
    /// Limit result count.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Skip count.
    #[serde(default)]
    pub offset: Option<usize>,
}

/// Unified paged query result.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct PagedResult<T> {
    /// Current page items.
    pub items: Vec<T>,
    /// Total count matching query, ignoring pagination.
    pub total: usize,
}

impl<T> PagedResult<T> {
    /// Transform page items while preserving total count.
    pub fn map<U>(self, f: impl FnMut(T) -> U) -> PagedResult<U> {
        PagedResult {
            items: self.items.into_iter().map(f).collect(),
            total: self.total,
        }
    }
}

pub mod a2a;
#[cfg(test)]
mod a2a_test;
pub mod agent;
#[cfg(test)]
mod agent_test;
pub mod artifact;
#[cfg(test)]
mod artifact_test;
pub mod attachment;
#[cfg(test)]
mod attachment_test;
pub mod auth;
pub mod cron_trigger;
pub mod external_agent;
pub mod log_stats;
pub mod mcp_server;
#[cfg(test)]
mod mcp_server_test;
pub mod mcp_tool;
pub mod message;
pub mod message_channel;
pub mod model_provider;
pub mod neural_tools;
pub mod organization;
pub mod project;
#[cfg(test)]
mod project_test;
pub mod seed;
pub mod skill;
#[cfg(test)]
mod skill_test;
pub mod system;
pub mod task;
#[cfg(test)]
mod task_test;
pub mod text_content;
#[cfg(test)]
mod text_content_test;
pub mod tool;
pub mod user;

// Re-exports for convenient import
pub use a2a::*;
pub use agent::*;
pub use artifact::*;
pub use attachment::*;
pub use auth::*;
pub use cron_trigger::*;
pub use external_agent::*;
pub use log_stats::*;
pub use mcp_server::*;
pub use mcp_tool::*;
pub use message::*;
pub use message_channel::*;
pub use model_provider::*;
pub use neural_tools::*;
pub use organization::*;
pub use project::*;
pub use seed::*;
pub use skill::*;
pub use system::*;
pub use task::*;
pub use text_content::*;
pub use tool::*;
pub use user::*;
