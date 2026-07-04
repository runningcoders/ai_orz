//! Artifact 模块
//!
//! 产物是任务的输出结果（报告、音视频、代码等）
//!
//! 包含：
//! - ArtifactPo - 持久化对象（只在 DAO/DAL 层使用）
//! - Artifact - 业务实体（Domain 层使用）

use common::enums::{ArtifactSourceType, FileType};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::types::Json;
use uuid::Uuid;

use crate::models::file::FileMeta;

/// ArtifactPo 持久化对象
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct ArtifactPo {
    /// Unique artifact ID.
    pub id: String,
    /// Associated project ID (required for all artifacts).
    pub project_id: String,
    /// Associated task ID (optional: None = project-level artifact).
    pub task_id: Option<String>,
    /// Artifact display name.
    pub name: String,
    /// Artifact description/summary.
    pub description: String,
    /// File type (document/image/audio/video/binary).
    pub file_type: FileType,
    /// File metadata (path, mime type, size) stored as JSON.
    pub file_meta: Json<FileMeta>,
    /// Source type: attachment, generated content, or reserved future source.
    pub source_type: ArtifactSourceType,
    /// Tags stored as JSON array: ["design", "report", "v1.0"]
    pub tags: String,
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

/// Artifact 业务实体
///
/// 这是 Domain 层返回给上层的类型
#[derive(Debug, Clone)]
pub struct Artifact {
    /// 底层持久化对象
    pub po: ArtifactPo,
}

impl Artifact {
    /// 从 PO 创建 Artifact
    pub fn from_po(po: ArtifactPo) -> Self {
        Self { po }
    }

    /// 创建新的项目级产物
    pub fn new_project(
        project_id: String,
        name: String,
        description: String,
        file_type: FileType,
        file_meta: FileMeta,
        created_by: String,
    ) -> Self {
        Self {
            po: ArtifactPo::new_project(
                project_id,
                name,
                description,
                file_type,
                file_meta,
                created_by,
            ),
        }
    }

    /// 创建新的项目级产物，显式指定来源类型
    pub fn new_project_with_source_type(
        project_id: String,
        name: String,
        description: String,
        file_type: FileType,
        file_meta: FileMeta,
        source_type: ArtifactSourceType,
        created_by: String,
    ) -> Self {
        Self {
            po: ArtifactPo::new_project_with_source_type(
                project_id,
                name,
                description,
                file_type,
                file_meta,
                source_type,
                created_by,
            ),
        }
    }

    /// 创建新的任务级产物
    pub fn new_task(
        project_id: String,
        task_id: String,
        name: String,
        description: String,
        file_type: FileType,
        file_meta: FileMeta,
        created_by: String,
    ) -> Self {
        Self {
            po: ArtifactPo::new_task(
                project_id,
                task_id,
                name,
                description,
                file_type,
                file_meta,
                created_by,
            ),
        }
    }

    /// 创建新的任务级产物，显式指定来源类型
    pub fn new_task_with_source_type(
        project_id: String,
        task_id: String,
        name: String,
        description: String,
        file_type: FileType,
        file_meta: FileMeta,
        source_type: ArtifactSourceType,
        created_by: String,
    ) -> Self {
        Self {
            po: ArtifactPo::new_task_with_source_type(
                project_id,
                task_id,
                name,
                description,
                file_type,
                file_meta,
                source_type,
                created_by,
            ),
        }
    }

    /// 转换为 PO
    pub fn into_po(self) -> ArtifactPo {
        self.po
    }

    /// 获取产物 ID
    pub fn id(&self) -> &str {
        &self.po.id
    }

    /// 获取所属项目 ID
    pub fn project_id(&self) -> &str {
        &self.po.project_id
    }

    /// 获取所属任务 ID
    pub fn task_id(&self) -> Option<&str> {
        self.po.task_id.as_deref()
    }

    /// 判断是否为项目级产物
    pub fn is_project_level(&self) -> bool {
        self.po.task_id.is_none()
    }

    /// 判断是否已删除
    pub fn is_deleted(&self) -> bool {
        self.po.status == 0
    }

    /// 获取标签列表
    pub fn tags(&self) -> Vec<String> {
        self.po.tags()
    }
}

impl ArtifactPo {
    /// Create a new project-level artifact (not associated with any task).
    pub fn new_project(
        project_id: String,
        name: String,
        description: String,
        file_type: FileType,
        file_meta: FileMeta,
        created_by: String,
    ) -> Self {
        Self::new_project_with_source_type(
            project_id,
            name,
            description,
            file_type,
            file_meta,
            ArtifactSourceType::Attachment,
            created_by,
        )
    }

    /// Create a new project-level artifact with explicit source type.
    pub fn new_project_with_source_type(
        project_id: String,
        name: String,
        description: String,
        file_type: FileType,
        file_meta: FileMeta,
        source_type: ArtifactSourceType,
        created_by: String,
    ) -> Self {
        let now = common::constants::utils::current_timestamp_ms();
        Self {
            id: Uuid::now_v7().to_string(),
            project_id,
            task_id: None,
            name,
            description,
            file_type,
            file_meta: Json(file_meta),
            source_type,
            tags: "[]".to_string(),
            status: 1,
            created_by: created_by.clone(),
            modified_by: created_by,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a new task-level artifact.
    pub fn new_task(
        project_id: String,
        task_id: String,
        name: String,
        description: String,
        file_type: FileType,
        file_meta: FileMeta,
        created_by: String,
    ) -> Self {
        Self::new_task_with_source_type(
            project_id,
            task_id,
            name,
            description,
            file_type,
            file_meta,
            ArtifactSourceType::Attachment,
            created_by,
        )
    }

    /// Create a new task-level artifact with explicit source type.
    pub fn new_task_with_source_type(
        project_id: String,
        task_id: String,
        name: String,
        description: String,
        file_type: FileType,
        file_meta: FileMeta,
        source_type: ArtifactSourceType,
        created_by: String,
    ) -> Self {
        let now = common::constants::utils::current_timestamp_ms();
        Self {
            id: Uuid::now_v7().to_string(),
            project_id,
            task_id: Some(task_id),
            name,
            description,
            file_type,
            file_meta: Json(file_meta),
            source_type,
            tags: "[]".to_string(),
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

    /// Get tags parsed as Vec<String>
    pub fn tags(&self) -> Vec<String> {
        serde_json::from_str(&self.tags).unwrap_or_default()
    }

    /// Set tags from Vec<String>
    pub fn set_tags(&mut self, tags: Vec<String>, modified_by: String) {
        self.tags = serde_json::to_string(&tags).unwrap_or_else(|_| "[]".to_string());
        self.modified_by = modified_by;
        self.updated_at = common::constants::utils::current_timestamp_ms();
    }
}

// ==================== EnrichContext 实现 ====================

impl crate::pkg::request_context::EnrichContext for Artifact {
    fn enrich(
        &self,
        builder: crate::pkg::request_context::RequestContextBuilder,
    ) -> crate::pkg::request_context::RequestContextBuilder {
        builder
            .project_id(&self.po.project_id)
            .try_task_id(self.po.task_id.as_deref())
    }
}
