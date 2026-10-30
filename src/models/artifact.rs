//! Artifact 模块
//!
//! 产物是任务的输出结果（报告、音视频、代码等）
//!
//! 包含：
//! - ArtifactPo - 持久化对象（只在 DAO/DAL 层使用）
//! - Artifact - 业务实体（Domain 层使用）

use std::path::{Component, Path, PathBuf};

use common::enums::{ArtifactSourceType, FileType};
use common::error::{Result, bail_err};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::types::Json;
use uuid::Uuid;

use crate::models::file::FileMeta;
use crate::pkg::paths;

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
    #[allow(clippy::too_many_arguments)]
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

    /// 获取产物实体文件在磁盘上的绝对路径。
    ///
    /// 等价于 [`ArtifactPo::file_abs_path`]，供持有 `Artifact` 的调用方直接使用。
    pub fn file_abs_path(&self, base_data_path: &Path) -> Result<PathBuf> {
        self.po.file_abs_path(base_data_path)
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
    #[allow(clippy::too_many_arguments)]
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

    /// 产物实体文件在磁盘上的绝对路径。
    ///
    /// 布局：`{base_data_path}/artifacts/projects/{project_id}/{artifact_id}/{file_path}`
    ///
    /// ## 这里是路径推导的唯一真相
    ///
    /// 「哪份字节算这个产物的实体文件」必须只有一个答案：DAO 读写内容的落地点、
    /// 以及所有按文件（而非按内存字节）消费产物的下游（例如技能包导入走统一
    /// copy 通路），都要从这里取。分头拼串一旦不一致，症状是「写到 A、读到 B」
    /// 这类静默串档，很难查。
    ///
    /// ## 为什么不放在 service 层做 trait 方法
    ///
    /// 所需入参（`project_id` / `id` / `file_meta.file_path`）全在本对象上，只差
    /// 一个 `base_data_path`——纯拼串、无 IO、无权限概念。放进模型层即可，不必
    /// 让 DAO → DAL → Domain 三级各加一个转发签名。
    ///
    /// ## 只放行 GeneratedContent
    ///
    /// Attachment 型产物没有自己的文件（它指向既有附件），取路径必然是空指向。
    /// 这类产物请直接消费其 `attachment_id`。
    pub fn file_abs_path(&self, base_data_path: &Path) -> Result<PathBuf> {
        if self.source_type != ArtifactSourceType::GeneratedContent {
            bail_err!(
                InvalidRequest,
                "Artifact {} is not stored as a file (source type {:?}); only GeneratedContent artifacts own a file on disk. Artifacts referencing an existing attachment should be consumed via that attachment_id.",
                self.id,
                self.source_type
            );
        }

        // file_path 可来自 Agent / API 入参，必须挡住逃逸出产物存储区的写法。
        //
        // 这里**不能**依赖 `Path::starts_with(artifacts_dir)`：它只做逐组件前缀比较，
        // 不做规范化，因此 `.../<artifact_id>/../../../etc/passwd` 依然以 `artifacts`
        // 开头而顺利通过。改为显式扫描组件，拒绝 ParentDir / RootDir / 盘符前缀。
        let relative = Path::new(&self.file_meta.0.file_path);
        if relative.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            bail_err!(
                InvalidRequest,
                "Invalid artifact file path: path traversal attempt detected"
            );
        }

        Ok(
            paths::artifact_project_dir(base_data_path, &self.project_id)
                .join(&self.id)
                .join(relative),
        )
    }
}

// ==================== EnrichContext 实现 ====================

impl crate::pkg::request_context::EnrichContext for ArtifactPo {
    fn enrich(
        &self,
        builder: crate::pkg::request_context::RequestContextBuilder,
    ) -> crate::pkg::request_context::RequestContextBuilder {
        builder
            .project_id(&self.project_id)
            .try_task_id(self.task_id.as_deref())
    }
}

impl crate::pkg::request_context::EnrichContext for Artifact {
    fn enrich(
        &self,
        builder: crate::pkg::request_context::RequestContextBuilder,
    ) -> crate::pkg::request_context::RequestContextBuilder {
        self.po.enrich(builder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample(source_type: ArtifactSourceType, file_path: &str) -> ArtifactPo {
        ArtifactPo::new_project_with_source_type(
            "proj-1".to_string(),
            "demo".to_string(),
            String::new(),
            FileType::Document,
            FileMeta::new(file_path.to_string(), "text/plain".to_string(), 8),
            source_type,
            "user-1".to_string(),
        )
    }

    fn base() -> PathBuf {
        PathBuf::from("/srv/ai_orz")
    }

    #[test]
    fn file_abs_path_follows_artifact_storage_layout() {
        let po = sample(ArtifactSourceType::GeneratedContent, "report.md");
        assert_eq!(
            po.file_abs_path(&base()).unwrap(),
            base()
                .join("artifacts/projects/proj-1")
                .join(&po.id)
                .join("report.md")
        );
    }

    /// 实体包装与 PO 必须同源：任一侧另起炉灶就会出现「DAO 写 A 处、下游读 B 处」。
    #[test]
    fn file_abs_path_is_shared_by_entity_wrapper() {
        let artifact = Artifact::from_po(sample(ArtifactSourceType::GeneratedContent, "a.md"));
        assert_eq!(
            artifact.file_abs_path(&base()).unwrap(),
            artifact.po.file_abs_path(&base()).unwrap()
        );
    }

    /// Attachment 型产物没有自己的文件，取路径必然是空指向——必须在第一时间拒绝，
    /// 而不是返回一个不存在的文件让下游当「内容为空」处理。
    #[test]
    fn file_abs_path_rejects_non_generated_content() {
        let po = sample(ArtifactSourceType::Attachment, "a.md");
        assert!(po.file_abs_path(&base()).is_err());
    }

    /// `file_name` 可由 Agent / API 传入，必须挡住逃逸出产物存储区的写法。
    #[test]
    fn file_abs_path_rejects_path_traversal() {
        for evil in ["../../../etc/passwd", "/etc/passwd", "sub/../../evil.md"] {
            let po = sample(ArtifactSourceType::GeneratedContent, evil);
            assert!(
                po.file_abs_path(&base()).is_err(),
                "应拒绝穿越出产物存储区的 file_path: {evil}"
            );
        }
    }

    /// 正常的分层文件名要放行（技能包 / 带目录结构的产物依赖此能力）。
    #[test]
    fn file_abs_path_allows_nested_relative_names() {
        let po = sample(ArtifactSourceType::GeneratedContent, "assets/guide.md");
        assert_eq!(
            po.file_abs_path(&base()).unwrap(),
            base()
                .join("artifacts/projects/proj-1")
                .join(&po.id)
                .join("assets/guide.md")
        );
    }
}
