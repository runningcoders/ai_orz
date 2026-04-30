//! Sqlite 实现 Project DAO

use std::sync::Arc;
use std::sync::OnceLock;

use common::enums::project::ProjectStatus;
use crate::error::AppError;
use crate::models::project::ProjectPo;
use crate::pkg::RequestContext;
use super::{ProjectDao, ProjectQuery};

// ==================== 工厂方法 + 单例 ====================

/// Global DAO instance for dependency injection
static DAO: OnceLock<Arc<dyn ProjectDao + Send + Sync>> = OnceLock::new();

/// 创建一个全新的 Project DAO 实例（用于测试）
pub fn new() -> Arc<dyn ProjectDao + Send + Sync> {
    Arc::new(ProjectDaoSqliteImpl::new())
}

/// Initialize the DAO global instance
pub fn init() {
    let _ = DAO.set(new());
}

/// Get the global DAO instance
pub fn dao() -> Arc<dyn ProjectDao + Send + Sync> {
    DAO.get().expect("Project DAO not initialized").clone()
}

// ==================== 实现 ====================

/// SQLite Project DAO implementation
#[derive(Debug, Clone, Default)]
struct ProjectDaoSqliteImpl;

impl ProjectDaoSqliteImpl {
    /// Create a new SQLite Project DAO
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl ProjectDao for ProjectDaoSqliteImpl {
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

    async fn query(&self, ctx: RequestContext, query: ProjectQuery) -> Result<Vec<ProjectPo>, AppError> {
        // 使用 sqlx::QueryBuilder 动态构建查询
        let mut builder = sqlx::QueryBuilder::new(
            "SELECT id, name, description, workflow, guidance, \"status\" as \"status\", priority, tags, root_user_id, owner_agent_id, start_at, due_at, end_at, created_by, modified_by, created_at, updated_at FROM projects WHERE 1=1"
        );

        // 默认软删除过滤
        builder.push(" AND \"status\" != 0");

        // 逐个添加查询条件
        if let Some(root_user_id) = &query.root_user_id {
            builder.push(" AND root_user_id = ").push_bind(root_user_id);
        }

        // 状态 IN 查询
        if let Some(status_list) = &query.status_in {
            if !status_list.is_empty() {
                builder.push(" AND \"status\" IN (");
                let mut separated = builder.separated(", ");
                for s in status_list {
                    separated.push_bind(*s as i32);
                }
                drop(separated); // 结束分隔器
                builder.push(")");
            }
            // 如果 status_in 是 Some 但是数组为空，我们不添加任何条件
        }

        // 排序
        builder.push(" ORDER BY priority DESC, created_at DESC");

        // 限制数量
        if let Some(limit) = query.limit {
            builder.push(" LIMIT ").push_bind(limit as i64);
        }

        // 执行查询
        let rows = builder.build_query_as()
            .fetch_all(ctx.db_pool())
            .await?;

        Ok(rows)
    }

    async fn list_by_root_user(&self, ctx: RequestContext, root_user_id: &str, limit: Option<usize>) -> Result<Vec<ProjectPo>, AppError> {
        // 语法糖：调用通用查询
        self.query(ctx, ProjectQuery {
            root_user_id: Some(root_user_id.to_string()),
            limit: Some(limit.unwrap_or(100)),
            ..Default::default()
        }).await
    }

    async fn list_by_root_user_and_status(&self, ctx: RequestContext, root_user_id: &str, status: Vec<ProjectStatus>, limit: Option<usize>) -> Result<Vec<ProjectPo>, AppError> {
        // 语法糖：调用通用查询
        self.query(ctx, ProjectQuery {
            root_user_id: Some(root_user_id.to_string()),
            status_in: Some(status),
            limit: Some(limit.unwrap_or(100)),
            ..Default::default()
        }).await
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
