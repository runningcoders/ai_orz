//! Artifact management API DTOs - shared between backend and frontend.

use crate::api::PaginationParams;
use crate::enums::{ArtifactSourceType, FileType};
use ai_orz_macros::Params;
use schemars::JsonSchema;
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
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
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

/// Create Artifact response.
pub type CreateArtifactResponse = ArtifactDetail;

/// Get Artifact request.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetArtifactRequest {
    /// Artifact ID.
    #[param(source = "path")]
    pub artifact_id: String,
}

/// Get Artifact response.
pub type GetArtifactResponse = ArtifactDetail;

/// Delete Artifact request.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteArtifactRequest {
    /// Artifact ID.
    #[param(source = "path")]
    pub artifact_id: String,
}

/// Delete Artifact response.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteArtifactResponse {
    /// Success flag.
    pub success: bool,
}

/// List Artifact request (by project).
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListArtifactsRequest {
    /// Project ID. Required by handler for bounded queries.
    #[param(source = "query")]
    pub project_id: String,
    /// Optional task ID filter.
    #[param(source = "query")]
    pub task_id: Option<String>,
    /// Optional file type filter.
    #[param(source = "query")]
    pub file_type: Option<FileType>,
    /// Optional source type filter.
    #[param(source = "query")]
    pub source_type: Option<ArtifactSourceType>,
    /// Return limit.
    #[param(source = "query")]
    pub limit: Option<usize>,
    /// Skip count.
    #[param(source = "query")]
    pub offset: Option<usize>,
}

/// Artifact 通用查询请求（POST body）
///
/// 支持完整查询条件 + 分页，query 是核心查询能力。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct ArtifactQueryRequest {
    /// 按项目 ID 查询
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// 按任务 ID 查询
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// 按文件类型查询
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_type: Option<FileType>,
    /// 按来源类型查询
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<ArtifactSourceType>,
    /// 分页参数（limit + offset）
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// List Artifact response.
pub type ListArtifactsResponse = Vec<ArtifactDetail>;

/// Artifact detail response.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

/// Get artifact content request (text-based content).
///
/// Used when source_type = GeneratedContent, retrieves the full UTF-8 text content directly.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetArtifactContentRequest {
    /// Artifact ID.
    #[param(source = "path")]
    pub artifact_id: String,
}

/// Get artifact content response (text-based content).
///
/// Used when source_type = GeneratedContent, serves UTF-8 text content directly.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetArtifactContentResponse {
    /// Artifact basic detail.
    pub artifact: ArtifactDetail,
    /// Text content response.
    pub content: ArtifactContentText,
}

/// Text content metadata and payload.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactContentText {
    /// UTF-8 encoded content.
    pub content: String,
    /// Content encoding (always utf-8).
    pub encoding: String,
    /// Content size in bytes.
    pub size: u64,
    /// Last updated timestamp.
    pub updated_at: i64,
}

/// Update artifact request (partial update).
///
/// Supports updating content and/or metadata in a single call.
/// Only fields that are `Some` will be updated. `None` fields are left unchanged.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateArtifactRequest {
    /// Artifact ID.
    #[param(source = "path")]
    pub artifact_id: String,
    /// New content for replacement. `None` to keep current content unchanged.
    /// Only applicable to GeneratedContent artifacts.
    pub content: Option<String>,
    /// New name. `None` to keep current.
    pub name: Option<String>,
    /// New description. `None` to keep current.
    pub description: Option<String>,
    /// New tags. `None` to keep current tags.
    pub tags: Option<Vec<String>>,
    /// Optional optimistic locking: expect current updated_at matches this value.
    /// If mismatch, returns 409 Conflict.
    pub expected_updated_at: Option<i64>,
}

/// Create text artifact params (neural tool: create_text_artifact).
///
/// Agent provides text content directly; the tool handles file creation
/// and artifact metadata registration in one step.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateTextArtifactParams {
    /// Project ID. Required.
    pub project_id: String,
    /// Optional task ID. `None` means project-level artifact.
    pub task_id: Option<String>,
    /// Artifact display name.
    pub name: String,
    /// Optional artifact description.
    pub description: Option<String>,
    /// Text content of the artifact.
    pub content: String,
    /// File name for storage. Defaults to derived from name (with .md extension).
    pub file_name: Option<String>,
    /// MIME type. Defaults to "text/plain".
    pub mime_type: Option<String>,
    /// File type category. Defaults to Document.
    pub file_type: Option<FileType>,
    /// Optional tags.
    pub tags: Option<Vec<String>>,
}

/// Register artifact from file path params (neural tool: register_artifact_from_path).
///
/// Agent provides a file path in its own directory; the tool copies the file
/// to artifact storage and registers metadata. Source file is preserved.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct RegisterArtifactFromPathParams {
    /// Project ID. Required.
    pub project_id: String,
    /// Optional task ID. `None` means project-level artifact.
    pub task_id: Option<String>,
    /// Artifact display name.
    pub name: String,
    /// Optional artifact description.
    pub description: Option<String>,
    /// Source file path, relative to agent's directory.
    pub source_path: String,
    /// File name for artifact storage. Defaults to basename of source_path.
    pub file_name: Option<String>,
    /// MIME type. Defaults to inferred from file extension.
    pub mime_type: Option<String>,
    /// File type category. Defaults to inferred from mime_type.
    pub file_type: Option<FileType>,
    /// Optional tags.
    pub tags: Option<Vec<String>>,
}
