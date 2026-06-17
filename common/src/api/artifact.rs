//! Artifact management API DTOs - shared between backend and frontend.

use crate::enums::{ArtifactSourceType, FileType};
use serde::{Deserialize, Serialize};

/// Create Artifact request.
///
/// Batch 3.1 initially supports the `attachment` source mode: provide
/// `attachment_id` to reference an existing Finance Attachment.
///
/// `generated_content` is reserved in the shared DTO contract for the next
/// extension. Its fields are modeled here so callers and frontend code can
/// converge on the same request shape, but the current create handler returns
/// Unsupported until the Project Domain file-write flow is implemented.
///
/// `RemoteUrl` is reserved by `ArtifactSourceType` for future extension and is
/// not accepted by the initial create handler until a URL metadata policy is added.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateArtifactRequest {
    /// Project ID. Required for all artifacts.
    pub project_id: String,
    /// Optional task ID. `None` means project-level artifact.
    pub task_id: Option<String>,
    /// Artifact display name.
    pub name: String,
    /// Optional artifact description.
    pub description: Option<String>,
    /// Artifact source type.
    pub source_type: ArtifactSourceType,
    /// Existing Finance Attachment ID. Required when `source_type = Attachment`.
    pub attachment_id: Option<String>,
    /// Generated text content. Required when `source_type = GeneratedContent`.
    pub content: Option<String>,
    /// Generated content file name. Required when `source_type = GeneratedContent`.
    pub file_name: Option<String>,
    /// Optional generated content MIME type.
    pub mime_type: Option<String>,
    /// Optional file type override/filter category.
    pub file_type: Option<FileType>,
    /// Optional tags.
    pub tags: Option<Vec<String>>,
}

/// Artifact list query.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ListArtifactsQuery {
    /// Project ID. Required by handler for bounded queries.
    pub project_id: String,
    /// Optional task ID filter.
    pub task_id: Option<String>,
    /// Optional file type filter.
    pub file_type: Option<FileType>,
    /// Optional source type filter.
    pub source_type: Option<ArtifactSourceType>,
    /// Return limit.
    pub limit: Option<usize>,
}

/// Artifact detail response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactDetail {
    /// Artifact ID.
    pub id: String,
    /// Project ID.
    pub project_id: String,
    /// Optional task ID.
    pub task_id: Option<String>,
    /// Artifact display name.
    pub name: String,
    /// Artifact description.
    pub description: String,
    /// File type.
    pub file_type: FileType,
    /// Source type.
    pub source_type: ArtifactSourceType,
    /// Logical file path.
    pub file_path: String,
    /// MIME type.
    pub mime_type: String,
    /// File size in bytes.
    pub file_size: u64,
    /// Tags.
    pub tags: Vec<String>,
    /// Artifact status integer.
    pub status: i32,
    /// Creator user ID.
    pub created_by: String,
    /// Last modifier user ID.
    pub modified_by: String,
    /// Creation timestamp.
    pub created_at: i64,
    /// Update timestamp.
    pub updated_at: i64,
}

/// Create Artifact response.
pub type CreateArtifactResponse = ArtifactDetail;

/// Get Artifact response.
pub type GetArtifactResponse = ArtifactDetail;

/// List Artifact response.
pub type ListArtifactsResponse = Vec<ArtifactDetail>;
