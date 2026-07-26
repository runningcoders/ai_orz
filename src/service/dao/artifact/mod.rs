//! Artifact DAO layer
//! Artifacts are task outputs (reports, audio, video, etc.).

use crate::models::artifact::ArtifactPo;
use crate::pkg::RequestContext;
use async_trait::async_trait;
use common::api::PagedResult;
use common::enums::{ArtifactSourceType, FileType};
use common::error::Result;

/// Artifact 查询参数
#[derive(Debug, Clone, Default)]
pub struct ArtifactQuery {
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub file_type: Option<FileType>,
    pub source_type: Option<ArtifactSourceType>,
    pub pagination: common::api::PaginationParams,
}

/// Artifact DAO trait
#[async_trait]
pub trait ArtifactDao: Send + Sync + std::fmt::Debug {
    /// Insert a new artifact
    async fn insert(&self, ctx: RequestContext, artifact: &ArtifactPo) -> Result<()>;

    /// Find artifact by id, automatically filters deleted artifacts
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<ArtifactPo>>;

    /// 通用查询
    async fn query(
        &self,
        ctx: RequestContext,
        query: ArtifactQuery,
    ) -> Result<PagedResult<ArtifactPo>>;

    /// List all artifacts for a project, automatically filters deleted artifacts
    async fn list_by_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<Vec<ArtifactPo>>;

    /// List all artifacts for a task, automatically filters deleted artifacts
    async fn list_by_task(&self, ctx: RequestContext, task_id: &str) -> Result<Vec<ArtifactPo>>;

    /// Count artifacts for a project
    async fn count_by_project(&self, ctx: RequestContext, project_id: &str) -> Result<u64>;

    /// Count artifacts for a task
    async fn count_by_task(&self, ctx: RequestContext, task_id: &str) -> Result<u64>;

    /// 统计符合查询条件的产物数量（复用 query 的 filter 逻辑，只跑 COUNT 不跑 LIST）
    async fn count(&self, ctx: RequestContext, query: ArtifactQuery) -> Result<u64>;

    /// Update artifact status (soft delete)
    async fn update_status(&self, ctx: RequestContext, id: &str, status: i32) -> Result<()>;

    /// Delete artifact (soft delete)
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// Update an existing artifact full record.
    async fn update(&self, ctx: RequestContext, artifact: &ArtifactPo) -> Result<()>;

    /// Read artifact content from disk (for source_type = GeneratedContent).
    /// Returns the raw bytes, None if artifact does not exist or no file.
    async fn read_content(
        &self,
        ctx: RequestContext,
        artifact: &ArtifactPo,
    ) -> Result<Option<Vec<u8>>>;

    /// Write artifact content to disk (for source_type = GeneratedContent).
    /// Overwrites existing file if it exists.
    async fn write_content(
        &self,
        ctx: RequestContext,
        artifact: &ArtifactPo,
        content: &[u8],
    ) -> Result<()>;
}

pub mod sqlite;
pub use self::sqlite::{dao, init, new};

#[cfg(test)]
pub(crate) mod sqlite_test;
