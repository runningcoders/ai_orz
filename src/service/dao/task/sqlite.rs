//! SQLite implementation of Task DAO

use std::sync::Arc;
use std::sync::OnceLock;
use common::enums::{TaskStatus, AssigneeType};
use crate::error::AppError;
use crate::models::task::TaskPo;
use crate::pkg::RequestContext;
use super::{TaskDao, TaskQuery};

// ==================== 工厂方法 + 单例 ====================

/// Global DAO instance for dependency injection
static DAO: OnceLock<Arc<dyn TaskDao + Send + Sync>> = OnceLock::new();

/// 创建一个全新的 Task DAO 实例（用于测试）
pub fn new() -> Arc<dyn TaskDao + Send + Sync> {
    Arc::new(TaskDaoSqliteImpl::new())
}

/// Initialize the DAO global instance
pub fn init() {
    let _ = DAO.set(new());
}

/// Get the global DAO instance
pub fn get_dao() -> &'static Arc<dyn TaskDao + Send + Sync> {
    DAO.get().expect("Task DAO not initialized")
}

/// Create a new DAO instance for dependency injection
pub fn dao() -> Arc<dyn TaskDao + Send + Sync> {
    new()
}

// ==================== 实现 ====================

/// SQLite Task DAO implementation
#[derive(Debug, Clone, Default)]
struct TaskDaoSqliteImpl;

impl TaskDaoSqliteImpl {
    /// Create a new SQLite Task DAO
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl TaskDao for TaskDaoSqliteImpl {
    async fn insert(&self, ctx: RequestContext, task: &TaskPo) -> Result<(), AppError> {
        let pool = ctx.db_pool();
        let status_i32 = task.status as i32;
        let assignee_type_i32 = task.assignee_type as i32;
        sqlx::query!(
            r#"INSERT INTO tasks(
                id, title, description, "status", priority, tags, due_at, start_at, end_at, dependencies, root_user_id,
                "assignee_type", assignee_id, project_id, created_by, modified_by, created_at, updated_at
            ) VALUES (
                ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
            )"#,
            task.id,
            task.title,
            task.description,
            status_i32,
            task.priority,
            task.tags,
            task.due_at,
            task.start_at,
            task.end_at,
            task.dependencies,
            task.root_user_id,
            assignee_type_i32,
            task.assignee_id,
            task.project_id,
            task.created_by,
            task.modified_by,
            task.created_at,
            task.updated_at
        ).execute(pool).await?;
        Ok(())
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<TaskPo>, AppError> {
        let pool = ctx.db_pool();
        let task = sqlx::query_as!(
            TaskPo,
            r#"
SELECT id, title, description, "status" as "status: TaskStatus", priority as "priority: i32", tags, due_at, start_at, end_at, dependencies, root_user_id,
       "assignee_type" as "assignee_type: AssigneeType", assignee_id, project_id,
       created_by, modified_by, created_at, updated_at
FROM tasks WHERE id = ? AND "status" != 0
"#,
            id
        )
        .fetch_optional(pool)
        .await?;
        Ok(task)
    }

    async fn query(&self, ctx: RequestContext, query: TaskQuery) -> Result<Vec<TaskPo>, AppError> {
        let mut builder = sqlx::QueryBuilder::new(
            r#"SELECT id, title, description, "status", priority, tags, due_at, start_at, end_at, dependencies, root_user_id, "assignee_type", assignee_id, project_id, created_by, modified_by, created_at, updated_at FROM tasks WHERE 1=1"#
        );

        // 默认软删除过滤
        builder.push(r#" AND "status" != 0"#);

        // 逐个添加查询条件
        if let Some(assignee_type) = &query.assignee_type {
            builder.push(r#" AND "assignee_type" = "#).push_bind(*assignee_type as i32);
        }

        if let Some(assignee_id) = &query.assignee_id {
            builder.push(" AND assignee_id = ").push_bind(assignee_id);
        }

        // 状态 IN 查询
        if let Some(status_list) = &query.status_in {
            if !status_list.is_empty() {
                builder.push(r#" AND "status" IN ("#);
                let mut separated = builder.separated(", ");
                for s in status_list {
                    separated.push_bind(*s as i32);
                }
                drop(separated);
                builder.push(")");
            }
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

    async fn list_by_assignee(&self, ctx: RequestContext, assignee_type: Option<AssigneeType>, assignee_id: &str, limit: Option<usize>) -> Result<Vec<TaskPo>, AppError> {
        // 语法糖：调用通用查询
        self.query(ctx, TaskQuery {
            assignee_type,
            assignee_id: Some(assignee_id.to_string()),
            limit: Some(limit.unwrap_or(100)),
            ..Default::default()
        }).await
    }

    async fn list_by_status(&self, ctx: RequestContext, assignee_type: Option<AssigneeType>, assignee_id: &str, status: Vec<TaskStatus>, limit: Option<usize>) -> Result<Vec<TaskPo>, AppError> {
        // 语法糖：调用通用查询
        self.query(ctx, TaskQuery {
            assignee_type,
            assignee_id: Some(assignee_id.to_string()),
            status_in: Some(status),
            limit: Some(limit.unwrap_or(100)),
            ..Default::default()
        }).await
    }

    async fn update(&self, ctx: RequestContext, task: &TaskPo) -> Result<(), AppError> {
        let pool = ctx.db_pool();
        let ctx_user_id = ctx.user_id.clone().unwrap_or_default();
        let now = common::constants::utils::current_timestamp();
        let status_i32 = task.status as i32;
        let priority_i32 = task.priority;
        let assignee_type_i32 = task.assignee_type as i32;
        sqlx::query!(
            r#"
UPDATE tasks SET
    title = ?,
    description = ?,
    "status" = ?,
    priority = ?,
    tags = ?,
    due_at = ?,
    start_at = ?,
    end_at = ?,
    dependencies = ?,
    "assignee_type" = ?,
    assignee_id = ?,
    project_id = ?,
    modified_by = ?,
    updated_at = ?
WHERE id = ?
"#,
            task.title,
            task.description,
            status_i32,
            priority_i32,
            task.tags,
            task.due_at,
            task.start_at,
            task.end_at,
            task.dependencies,
            assignee_type_i32,
            task.assignee_id,
            task.project_id,
            ctx_user_id,
            now,
            task.id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn update_status(&self, ctx: RequestContext, id: &str, status: TaskStatus, modified_by: &str) -> Result<(), AppError> {
        let pool = ctx.db_pool();
        let now = common::constants::utils::current_timestamp();
        let status_i32 = status as i32;
        sqlx::query!(
            r#"
UPDATE tasks SET "status" = ?, modified_by = ?, updated_at = ? WHERE id = ?
"#,
            status_i32,
            modified_by,
            now,
            id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn count_by_assignee(&self, ctx: RequestContext, assignee_id: &str) -> Result<u64, AppError> {
        let pool = ctx.db_pool();
        let row = sqlx::query!(
            "SELECT COUNT(*) as \"count: i64\" FROM tasks WHERE assignee_id = ? AND \"status\" != 0",
            assignee_id
        )
        .fetch_one(pool)
        .await?;
        Ok(row.count as u64)
    }

    async fn count_by_assignee_and_status(&self, ctx: RequestContext, assignee_id: &str, status: TaskStatus) -> Result<u64, AppError> {
        let pool = ctx.db_pool();
        let status_i32 = status as i32;
        let row = sqlx::query!(
            "SELECT COUNT(*) as \"count: i64\" FROM tasks WHERE assignee_id = ? AND \"status\" = ?",
            assignee_id,
            status_i32
        )
        .fetch_one(pool)
        .await?;
        Ok(row.count as u64)
    }
}
