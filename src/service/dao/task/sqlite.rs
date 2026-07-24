//! SQLite implementation of Task DAO

use super::{TaskDao, TaskQuery, TaskSearch};
use common::error::Result;
use crate::models::task::TaskPo;
use crate::pkg::RequestContext;
use common::enums::{AssigneeType, TaskStatus};
use sqlx::FromRow;
use std::sync::Arc;
use std::sync::OnceLock;

// ==================== FTS5 辅助 ====================

use crate::pkg::storage::escape_fts5_keyword;

/// 任务搜索行（PO + fts_rank）
#[derive(FromRow)]
struct TaskSearchRow {
    id: String,
    title: String,
    description: String,
    status: TaskStatus,
    priority: i32,
    tags: String,
    due_at: Option<i64>,
    start_at: Option<i64>,
    end_at: Option<i64>,
    dependencies: Option<String>,
    root_user_id: String,
    assignee_type: AssigneeType,
    assignee_id: String,
    project_id: Option<String>,
    thinking_depth: i64,
    progress: i32,
    created_by: String,
    modified_by: String,
    created_at: i64,
    updated_at: i64,
    fts_rank: Option<f32>,
}

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
    async fn insert(&self, ctx: RequestContext, task: &TaskPo) -> Result<()> {
        let pool = ctx.db_pool();
        let status_i32 = task.status as i32;
        let assignee_type_i32 = task.assignee_type as i32;
        sqlx::query!(
            r#"INSERT INTO tasks(
                id, title, description, "status", priority, tags, due_at, start_at, end_at, dependencies, root_user_id,
                "assignee_type", assignee_id, project_id, thinking_depth, progress, created_by, modified_by, created_at, updated_at
            ) VALUES (
                ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
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
            task.thinking_depth,
            task.progress,
            task.created_by,
            task.modified_by,
            task.created_at,
            task.updated_at
        ).execute(pool).await?;
        Ok(())
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<TaskPo>> {
        let pool = ctx.db_pool();
        let task = sqlx::query_as!(
            TaskPo,
            r#"
SELECT id, title, description, "status" as "status: TaskStatus", priority as "priority: i32", tags, due_at, start_at, end_at, dependencies, root_user_id,
       "assignee_type" as "assignee_type: AssigneeType", assignee_id, project_id, thinking_depth, progress as "progress: i32",
       created_by, modified_by, created_at, updated_at
FROM tasks WHERE id = ? AND "status" != 0
"#,
            id
        )
        .fetch_optional(pool)
        .await?;
        Ok(task)
    }

    async fn query(&self, ctx: RequestContext, query: TaskQuery) -> Result<common::api::PagedResult<TaskPo>> {
        let pool = ctx.db_pool();

        let mut count_builder = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM tasks WHERE 1=1");
        push_query_filters(&mut count_builder, &query);
        let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;

        let mut list_builder = sqlx::QueryBuilder::new(
            r#"SELECT id, title, description, "status", priority, tags, due_at, start_at, end_at, dependencies, root_user_id, "assignee_type", assignee_id, project_id, thinking_depth, progress, created_by, modified_by, created_at, updated_at FROM tasks WHERE 1=1"#,
        );
        push_query_filters(&mut list_builder, &query);

        // 排序
        list_builder.push(" ORDER BY priority DESC, created_at DESC");

        if let Some(limit) = query.pagination.limit {
            list_builder.push(" LIMIT ").push_bind(limit as i64);
        } else if query.pagination.offset.is_some() {
            list_builder.push(" LIMIT -1");
        }
        if let Some(offset) = query.pagination.offset {
            list_builder.push(" OFFSET ").push_bind(offset as i64);
        }

        // 执行查询
        let items = list_builder.build_query_as().fetch_all(pool).await?;

        Ok(common::api::PagedResult {
            items,
            total: total as usize,
        })
    }

    async fn search_tasks(
        &self,
        ctx: RequestContext,
        search: TaskSearch,
    ) -> Result<Vec<(TaskPo, Option<f32>)>> {
        let pool = ctx.db_pool();

        let keyword = search.keyword.unwrap_or_default();
        let limit_i64 = search.filters.pagination.limit.unwrap_or(50) as i64;

        // 空关键词直接返回空结果（FTS5 MATCH 空字符串会报错）
        if keyword.trim().is_empty() {
            return Ok(Vec::new());
        }

        // 转义关键词为 FTS5 短语匹配
        let escaped_keyword = escape_fts5_keyword(&keyword);

        // FTS5 MATCH + JOIN + BM25 排序
        // 注意：MATCH 左侧必须使用完整表名（非别名），否则 SQLite 会将别名解释为列名
        let mut builder = sqlx::QueryBuilder::new(
            r#"SELECT t.id, t.title, t.description, t."status", t.priority, t.tags, t.due_at, t.start_at, t.end_at, t.dependencies, t.root_user_id, t."assignee_type", t.assignee_id, t.project_id, t.thinking_depth, t.progress, t.created_by, t.modified_by, t.created_at, t.updated_at, tasks_fts.rank as fts_rank
FROM tasks_fts
JOIN tasks t ON tasks_fts.rowid = t.rowid
WHERE tasks_fts MATCH "#,
        );
        builder.push_bind(escaped_keyword);
        builder.push(r#" AND t."status" != 0"#);

        // 业务过滤条件
        if let Some(assignee_type) = &search.filters.assignee_type {
            builder.push(r#" AND t."assignee_type" = "#);
            builder.push_bind(*assignee_type as i32);
        }

        if let Some(assignee_id) = &search.filters.assignee_id {
            builder.push(" AND t.assignee_id = ");
            builder.push_bind(assignee_id);
        }

        if let Some(project_id) = &search.filters.project_id {
            builder.push(" AND t.project_id = ");
            builder.push_bind(project_id);
        }

        if let Some(status_list) = &search.filters.status_in {
            if !status_list.is_empty() {
                builder.push(r#" AND t."status" IN ("#);
                let mut separated = builder.separated(", ");
                for s in status_list {
                    separated.push_bind(*s as i32);
                }
                drop(separated);
                builder.push(")");
            }
        }

        builder.push(" ORDER BY tasks_fts.rank LIMIT ");
        builder.push_bind(limit_i64);

        let rows: Vec<TaskSearchRow> = builder.build_query_as().fetch_all(pool).await?;

        let results = rows
            .into_iter()
            .map(|row| {
                let po = TaskPo {
                    id: row.id,
                    title: row.title,
                    description: row.description,
                    status: row.status,
                    priority: row.priority,
                    tags: row.tags,
                    due_at: row.due_at,
                    start_at: row.start_at,
                    end_at: row.end_at,
                    dependencies: row.dependencies,
                    root_user_id: row.root_user_id,
                    assignee_type: row.assignee_type,
                    assignee_id: row.assignee_id,
                    project_id: row.project_id,
                    thinking_depth: row.thinking_depth,
                    progress: row.progress,
                    created_by: row.created_by,
                    modified_by: row.modified_by,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                };
                (po, row.fts_rank)
            })
            .collect();

        Ok(results)
    }

    async fn list_by_assignee(
        &self,
        ctx: RequestContext,
        assignee_type: Option<AssigneeType>,
        assignee_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<TaskPo>> {
        // 语法糖：调用通用查询
        let page = self
            .query(
                ctx,
                TaskQuery {
                    assignee_type,
                    assignee_id: Some(assignee_id.to_string()),
                    pagination: common::api::PaginationParams {
                        limit: Some(limit.unwrap_or(100)),
                        offset: None,
                    },
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    async fn list_by_status(
        &self,
        ctx: RequestContext,
        assignee_type: Option<AssigneeType>,
        assignee_id: &str,
        status: Vec<TaskStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<TaskPo>> {
        // 语法糖：调用通用查询
        let page = self
            .query(
                ctx,
                TaskQuery {
                    assignee_type,
                    assignee_id: Some(assignee_id.to_string()),
                    status_in: Some(status),
                    pagination: common::api::PaginationParams {
                        limit: Some(limit.unwrap_or(100)),
                        offset: None,
                    },
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    async fn update(&self, ctx: RequestContext, task: &TaskPo) -> Result<()> {
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
    thinking_depth = ?,
    progress = ?,
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
            task.thinking_depth,
            task.progress,
            ctx_user_id,
            now,
            task.id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn update_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: TaskStatus,
        modified_by: &str,
    ) -> Result<()> {
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

    async fn count_by_assignee(
        &self,
        ctx: RequestContext,
        assignee_id: &str,
    ) -> Result<u64> {
        let pool = ctx.db_pool();
        let row = sqlx::query!(
            "SELECT COUNT(*) as \"count: i64\" FROM tasks WHERE assignee_id = ? AND \"status\" != 0",
            assignee_id
        )
        .fetch_one(pool)
        .await?;
        Ok(row.count as u64)
    }

    async fn count_by_assignee_and_status(
        &self,
        ctx: RequestContext,
        assignee_id: &str,
        status: TaskStatus,
    ) -> Result<u64> {
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

/// 推送查询过滤条件到 QueryBuilder（COUNT 和 LIST 查询复用）
fn push_query_filters<'args>(
    builder: &mut sqlx::QueryBuilder<'args, sqlx::Sqlite>,
    query: &TaskQuery,
) {
    builder.push(r#" AND "status" != 0"#);
    if let Some(ids) = &query.ids {
        if !ids.is_empty() {
            builder.push(" AND id IN (");
            let mut separated = builder.separated(", ");
            for id in ids {
                separated.push_bind(id.clone());
            }
            drop(separated);
            builder.push(")");
        }
    }
    if let Some(assignee_type) = &query.assignee_type {
        builder
            .push(r#" AND "assignee_type" = "#)
            .push_bind(*assignee_type as i32);
    }
    if let Some(assignee_id) = &query.assignee_id {
        builder.push(" AND assignee_id = ").push_bind(assignee_id.clone());
    }
    if let Some(project_id) = &query.project_id {
        builder.push(" AND project_id = ").push_bind(project_id.clone());
    }
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
}
