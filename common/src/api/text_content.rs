//! Shared simple text content API DTOs.
//!
//! These DTOs are composed by resource-specific APIs (Attachment, Skill,
//! Artifact) for UTF-8 small-text content read/replace endpoints.

use serde::{Deserialize, Serialize};

/// Simple UTF-8 text content response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextContentResponse {
    /// UTF-8 text content.
    pub content: String,
    /// Encoding name. Currently always `utf-8`.
    pub encoding: String,
    /// Content size in bytes.
    pub size: u64,
    /// Resource update timestamp used by optimistic locking.
    pub updated_at: i64,
}

/// Full-replace request for simple UTF-8 text content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateTextContentRequest {
    /// New full text content.
    pub content: String,
    /// Optional optimistic-lock timestamp.
    pub expected_updated_at: Option<i64>,
}
