//! Sqlite 实现 Project DAO

use std::sync::Arc;
use std::sync::OnceLock;

use super::{ProjectDao, ProjectQuery, ProjectSearch};
use crate::models::project::ProjectPo;
use crate::pkg::RequestContext;
use crate::pkg::storage::escape_fts5_keyword;
use common::enums::project::ProjectStatus;
use common::error::Result;
use sqlx::FromRow;

// ==================== FTS5 搜索辅助 ====================

/// Project 搜索行（PO + fts_rank）
#[derive(FromRow)]
struct ProjectSearchRow {
    id: String,
    name: String,
    description: String,
    workflow: Option<String>,
    guidance: Option<String>,
    status: i32,
    priority: i32,
    tags: String,
    root_user_id: String,
    owner_agent_id: Option<String>,
    start_at: Option<i64>,
    due_at: Option<i64>,
    end_at: Option<i64>,
    created_by: String,
    modified_by: String,
    created_at: i64,
    updated_at: i64,
    fts_rank: Option<f32>,
}

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
    async fn insert(&self, ctx: RequestContext, project: &ProjectPo) -> Result<()> {
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

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<ProjectPo>> {
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

    async fn query(
        &self,
        ctx: RequestContext,
        query: ProjectQuery,
    ) -> Result<common::api::PagedResult<ProjectPo>> {
        let pool = ctx.db_pool();

        let mut count_builder = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM projects WHERE 1=1");
        push_query_filters(&mut count_builder, &query);
        let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;

        // 使用 sqlx::QueryBuilder 动态构建查询
        let mut list_builder = sqlx::QueryBuilder::new(
            "SELECT id, name, description, workflow, guidance, \"status\" as \"status\", priority, tags, root_user_id, owner_agent_id, start_at, due_at, end_at, created_by, modified_by, created_at, updated_at FROM projects WHERE 1=1",
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

    async fn list_by_root_user(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<ProjectPo>> {
        // 语法糖：调用通用查询
        let page = self
            .query(
                ctx,
                ProjectQuery {
                    root_user_id: Some(root_user_id.to_string()),
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

    async fn list_by_root_user_and_status(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        status: Vec<ProjectStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<ProjectPo>> {
        // 语法糖：调用通用查询
        let page = self
            .query(
                ctx,
                ProjectQuery {
                    root_user_id: Some(root_user_id.to_string()),
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

    async fn update(&self, ctx: RequestContext, project: &ProjectPo) -> Result<()> {
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

    async fn update_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: ProjectStatus,
        modified_by: &str,
    ) -> Result<()> {
        let pool = ctx.db_pool();
        let now = common::constants::utils::current_timestamp();
        let status_i32 = status as i32;
        sqlx::query!(
            "UPDATE projects SET \"status\" = ?, modified_by = ?, updated_at = ? WHERE id = ?",
            status_i32,
            modified_by,
            now,
            id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn count_by_root_user(&self, ctx: RequestContext, root_user_id: &str) -> Result<u64> {
        // 语法糖：调用通用 count
        self.count(
            ctx,
            ProjectQuery {
                root_user_id: Some(root_user_id.to_string()),
                ..Default::default()
            },
        )
        .await
    }

    async fn count_by_root_user_and_status(
        &self,
        ctx: RequestContext,
        root_user_id: &str,
        status: ProjectStatus,
    ) -> Result<u64> {
        // 语法糖：调用通用 count
        self.count(
            ctx,
            ProjectQuery {
                root_user_id: Some(root_user_id.to_string()),
                status_in: Some(vec![status]),
                ..Default::default()
            },
        )
        .await
    }

    async fn count(&self, ctx: RequestContext, query: ProjectQuery) -> Result<u64> {
        let pool = ctx.db_pool();
        let mut count_builder = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM projects WHERE 1=1");
        push_query_filters(&mut count_builder, &query);
        let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;
        Ok(total as u64)
    }

    async fn search_projects(
        &self,
        ctx: RequestContext,
        search: ProjectSearch,
    ) -> Result<Vec<(ProjectPo, Option<f32>)>> {
        let pool = ctx.db_pool();

        // 从 ProjectSearch 提取参数
        let keyword = search.keyword.unwrap_or_default();
        let limit_i64 = search.filters.pagination.limit.unwrap_or(50) as i64;

        // 空关键词直接返回空结果（FTS5 MATCH 空字符串会报错）
        if keyword.trim().is_empty() {
            return Ok(Vec::new());
        }

        let escaped_keyword = escape_fts5_keyword(&keyword);

        // FTS5 MATCH + JOIN + BM25 排序
        // 注意：MATCH 左侧必须使用完整表名（非别名），否则 SQLite 会将别名解释为列名
        let rows: Vec<ProjectSearchRow> = sqlx::query_as(
            r#"
SELECT p.id, p.name, p.description, p.workflow, p.guidance, p."status" as status,
       p.priority, p.tags, p.root_user_id, p.owner_agent_id,
       p.start_at, p.due_at, p.end_at, p.created_by, p.modified_by,
       p.created_at, p.updated_at,
       projects_fts.rank as fts_rank
FROM projects_fts
JOIN projects p ON projects_fts.rowid = p.rowid
WHERE projects_fts MATCH ?
  AND p."status" != 0
ORDER BY projects_fts.rank
LIMIT ?
"#,
        )
        .bind(escaped_keyword)
        .bind(limit_i64)
        .fetch_all(pool)
        .await?;

        let results = rows
            .into_iter()
            .map(|row| {
                let po = ProjectPo {
                    id: row.id,
                    name: row.name,
                    description: row.description,
                    workflow: row.workflow,
                    guidance: row.guidance,
                    status: ProjectStatus::from(row.status),
                    priority: row.priority,
                    tags: row.tags,
                    root_user_id: row.root_user_id,
                    owner_agent_id: row.owner_agent_id,
                    start_at: row.start_at,
                    due_at: row.due_at,
                    end_at: row.end_at,
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
}

/// 推送查询过滤条件到 QueryBuilder（COUNT 和 LIST 查询复用）
fn push_query_filters<'args>(
    builder: &mut sqlx::QueryBuilder<'args, sqlx::Sqlite>,
    query: &ProjectQuery,
) {
    // 默认软删除过滤
    builder.push(" AND \"status\" != 0");
    if let Some(ids) = &query.ids
        && !ids.is_empty()
    {
        builder.push(" AND id IN (");
        let mut separated = builder.separated(", ");
        for id in ids {
            separated.push_bind(id.clone());
        }
        builder.push(")");
    }
    if let Some(root_user_id) = &query.root_user_id {
        builder
            .push(" AND root_user_id = ")
            .push_bind(root_user_id.clone());
    }
    if let Some(owner_agent_id) = &query.owner_agent_id {
        builder
            .push(" AND owner_agent_id = ")
            .push_bind(owner_agent_id.clone());
    }
    if let Some(status_list) = &query.status_in
        && !status_list.is_empty()
    {
        builder.push(" AND \"status\" IN (");
        let mut separated = builder.separated(", ");
        for s in status_list {
            separated.push_bind(*s as i32);
        }
        builder.push(")");
    }
}
