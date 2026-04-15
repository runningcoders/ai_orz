//! Artifact persistent object.
//! Artifacts are task outputs (reports, audio, video, etc.).

use common::enums::FileType;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::types::Json;
use uuid::Uuid;

use crate::models::file::FileMeta;

/// Artifact persistent object.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ArtifactPo {
    /// Unique artifact ID.
    pub id: String,
    /// Associated task ID.
    pub task_id: String,
    /// Artifact display name.
    pub name: String,
    /// Artifact description/summary.
    pub description: String,
    /// File type (document/image/audio/video/binary).
    pub file_type: FileType,
    /// File metadata (path, mime type, size) stored as JSON.
    pub file_meta: Json<FileMeta>,
    /// Status: 0 = deleted (soft delete), 1 = active.
    pub status: i32,
    /// Creator user ID.
    pub created_by: String,
    /// Last modifier user ID.
    pub modified_by: String,
    /// Creation timestamp (milliseconds).
    pub created_at: i64,
    /// Last update timestamp (milliseconds).
    pub updated_at: i64,
}

impl ArtifactPo {
    /// Create a new artifact.
    pub fn new(
        task_id: String,
        name: String,
        description: String,
        file_type: FileType,
        file_meta: FileMeta,
        created_by: String,
    ) -> Self {
        let now = common::constants::utils::current_timestamp_ms();
        Self {
            id: Uuid::now_v7().to_string(),
            task_id,
            name,
            description,
            file_type,
            file_meta: Json(file_meta),
            status: 1,
            created_by: created_by.clone(),
            modified_by: created_by,
            created_at: now,
            updated_at: now,
        }
    }

    /// Mark artifact as deleted (soft delete).
    pub fn mark_deleted(&mut self, modified_by: String) {
        self.status = 0;
        self.modified_by = modified_by;
        self.updated_at = common::constants::utils::current_timestamp_ms();
    }
}
