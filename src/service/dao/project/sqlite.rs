//! Sqlite 实现 Project DAO

use std::sync::Arc;
use std::sync::OnceLock;

use common::enums::project::ProjectStatus;
use crate::error::AppError;
use crate::models::project::ProjectPo;
use crate::pkg::RequestContext;
use super::ProjectDaoTrait;

/// SQLite Project DAO implementation
#[derive(Debug, Clone, Default)]
pub struct SqliteProjectDao;

impl SqliteProjectDao {
    /// Create a new SQLite Project DAO
    pub fn new() -> Self {
        Self
    }
}

/// Global DAO instance for dependency injection
static DAO: OnceLock<Arc<dyn ProjectDaoTrait + Send + Sync>> = OnceLock::new();

/// Initialize the DAO global instance
pub fn init() {
    let dao = SqliteProjectDao::new();
    let _ = DAO.set(Arc::new(dao));
}

/// Get the global DAO instance
pub fn dao() -> Arc<dyn ProjectDaoTrait + Send + Sync> {
    DAO.get().expect("Project DAO not initialized").clone()
}

#[async_trait::async_trait]
impl ProjectDaoTrait for SqliteProjectDao {
    async fn insert(&self, ctx: RequestContext, project: &ProjectPo) -> Result<(), AppError> {
        let pool = ctx.db_pool();
        let status_i32 = project.status as i32;
        sqlx::query!(
            "INSERT INTO projects (id, name, description, workflow, guidance, \"status\", priority, tags, root_user_id, owner_agent_id, start_at, due_at, end_at, created_by, modified_by, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            project.id, project.name, project.description, project.workflow, project.guidance, status_i32, project.priority, project.tags, project.root_user_id, project.owner_agent_id, project.start_at, project.due_at, project.end_at, project.created_by, project.modified_by, project.created_at, project.updated_at
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<ProjectPo>, AppError> {
        let pool = ctx.db_pool();
        let project = sqlx::query_as!(
            ProjectPo,
            "SELECT id, name, description, workflow, guidance, \"status\" as \"status: ProjectStatus\", priority as \"priority: i32\", tags, root_user_id, owner_agent_id, start_at, due_at, end_at, created_by, modified_by, created_at, updated_at FROM projects WHERE id = ? AND \"status\" != 0",
            id
        )
        .fetch_optional(pool)
        .await?;
        Ok(project)
    }

    async fn list_by_root_user(&self, ctx: RequestContext, root_user_id: &str, limit: Option<usize>) -> Result<Vec<ProjectPo>, AppError> {
        let pool = ctx.db_pool();
        let limit = limit.unwrap_or(100);
        let limit_i64 = limit as i64;
        let projects = sqlx::query_as!(
            ProjectPo,
            "SELECT id, name, description, workflow, guidance, \"status\" as \"status: ProjectStatus\", priority as \"priority: i32\", tags, root_user_id, owner_agent_id, start_at, due_at, end_at, created_by, modified_by, created_at, updated_at FROM projects WHERE root_user_id = ? AND \"status\" != 0 ORDER BY priority DESC, created_at DESC LIMIT ?",
            root_user_id, limit_i64
        )
        .fetch_all(pool)
        .await?;
        Ok(projects)
    }

    async fn list_by_root_user_and_status(&self, ctx: RequestContext, root_user_id: &str, status: Vec<ProjectStatus>, limit: Option<usize>) -> Result<Vec<ProjectPo>, AppError> {
        let pool = ctx.db_pool();
        let limit = limit.unwrap_or(100);
        let limit_i64 = limit as i64;
        let s: Vec<i32> = status.iter().map(|x| (*x) as i32).collect();
        let s0 = s.get(0).copied();
        let s1 = s.get(1).copied();
        let s2 = s.get(2).copied();
        let s3 = s.get(3).copied();

        let projects = sqlx::query_as!(
            ProjectPo,
            "SELECT id, name, description, workflow, guidance, \"status\" as \"status: ProjectStatus\", priority as \"priority: i32\", tags, root_user_id, owner_agent_id, start_at, due_at, end_at, created_by, modified_by, created_at, updated_at FROM projects WHERE root_user_id = ? AND \"status\" != 0 AND ((? IS NOT NULL AND \"status\" = ?) OR (? IS NOT NULL AND \"status\" = ?) OR (? IS NOT NULL AND \"status\" = ?) OR (? IS NOT NULL AND \"status\" = ?)) ORDER BY priority DESC, created_at DESC LIMIT ?",
            root_user_id, s0, s0, s1, s1, s2, s2, s3, s3, limit_i64
        )
        .fetch_all(pool)
        .await?;
        Ok(projects)
    }

    async fn update(&self, ctx: RequestContext, project: &ProjectPo) -> Result<(), AppError> {
        let pool = ctx.db_pool();
        let now = common::constants::utils::current_timestamp();
        let status_i32 = project.status as i32;
        sqlx::query!(
            "UPDATE projects SET name = ?, description = ?, workflow = ?, guidance = ?, \"status\" = ?, priority = ?, tags = ?, root_user_id = ?, owner_agent_id = ?, start_at = ?, due_at = ?, end_at = ?, modified_by = ?, updated_at = ? WHERE id = ?",
            project.name, project.description, project.workflow, project.guidance, status_i32, project.priority, project.tags, project.root_user_id, project.owner_agent_id, project.start_at, project.due_at, project.end_at, project.modified_by, now, project.id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn update_status(&self, ctx: RequestContext, id: &str, status: ProjectStatus, modified_by: &str) -> Result<(), AppError> {
        let pool = ctx.db_pool();
        let now = common::constants::utils::current_timestamp();
        let status_i32 = status as i32;
        sqlx::query!(
            "UPDATE projects SET \"status\" = ?, modified_by = ?, updated_at = ? WHERE id = ?",
            status_i32, modified_by, now, id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn count_by_root_user(&self, ctx: RequestContext, root_user_id: &str) -> Result<u64, AppError> {
        let pool = ctx.db_pool();
        let count = sqlx::query!(
            "SELECT COUNT(*) as cnt FROM projects WHERE root_user_id = ? AND \"status\" != 0",
            root_user_id
        )
        .fetch_one(pool)
        .await?;
        Ok(count.cnt as u64)
    }

    async fn count_by_root_user_and_status(&self, ctx: RequestContext, root_user_id: &str, status: ProjectStatus) -> Result<u64, AppError> {
        let pool = ctx.db_pool();
        let status_i32 = status as i32;
        let count = sqlx::query!(
            "SELECT COUNT(*) as cnt FROM projects WHERE root_user_id = ? AND \"status\" = ?",
            root_user_id, status_i32
        )
        .fetch_one(pool)
        .await?;
        Ok(count.cnt as u64)
    }
}
