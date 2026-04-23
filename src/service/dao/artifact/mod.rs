//! Artifact DAO layer
//! Artifacts are task outputs (reports, audio, video, etc.).

use async_trait::async_trait;
use crate::pkg::RequestContext;
use crate::models::artifact::ArtifactPo;
use crate::error::Result;

/// Artifact DAO trait
#[async_trait]
pub trait ArtifactDao: Send + Sync + std::fmt::Debug {
    /// Insert a new artifact
    async fn insert(&self, ctx: RequestContext, artifact: &ArtifactPo) -> Result<()>;

    /// Find artifact by id, automatically filters deleted artifacts
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<ArtifactPo>>;

    /// List all artifacts for a task, automatically filters deleted artifacts
    async fn list_by_task(&self, ctx: RequestContext, task_id: &str) -> Result<Vec<ArtifactPo>>;

    /// Count artifacts for a task
    async fn count_by_task(&self, ctx: RequestContext, task_id: &str) -> Result<i64>;

    /// Update artifact status (soft delete)
    async fn update_status(&self, ctx: RequestContext, id: &str, status: i32) -> Result<()>;

    /// Delete artifact (soft delete)
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;
}

pub mod sqlite;
pub use self::sqlite::{dao, init, new};

#[cfg(test)]
pub(crate) mod sqlite_test;
