//! Sqlite 实现 Project DAO

use std::sync::Arc;
use std::sync::OnceLock;

use super::{ProjectDao, ProjectQuery, ProjectSearch};
use crate::models::project::ProjectPo;
use crate::pkg::RequestContext;
use crate::pkg::storage::{build_fts5_search_plan, merge_fts5_results, push_fts5_like_conditions};
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
    execution_plan: Option<String>,
    execution_result: Option<String>,
    last_followup_at: Option<i64>,
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
            "INSERT INTO projects (id, name, description, workflow, guidance, \"status\", priority, tags, root_user_id, owner_agent_id, start_at, due_at, end_at, created_by, modified_by, created_at, updated_at, execution_plan, execution_result, last_followup_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            project.id, project.name, project.description, project.workflow, project.guidance, status_i32, project.priority, project.tags, project.root_user_id, project.owner_agent_id, project.start_at, project.due_at, project.end_at, project.created_by, project.modified_by, project.created_at, project.updated_at, project.execution_plan, project.execution_result, project.last_followup_at
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<ProjectPo>> {
        let pool = ctx.db_pool();
        let project = sqlx::query_as!(
            ProjectPo,
            "SELECT id, name, description, workflow, guidance, \"status\" as \"status: ProjectStatus\", priority as \"priority: i32\", tags, root_user_id, owner_agent_id, start_at, due_at, end_at, created_by, modified_by, created_at, updated_at, execution_plan, execution_result, last_followup_at FROM projects WHERE id = ? AND \"status\" != 0",
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
            "SELECT id, name, description, workflow, guidance, \"status\" as \"status\", priority, tags, root_user_id, owner_agent_id, start_at, due_at, end_at, created_by, modified_by, created_at, updated_at, execution_plan, execution_result, last_followup_at FROM projects WHERE 1=1",
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

    async fn list_all_by_status(
        &self,
        ctx: RequestContext,
        status: ProjectStatus,
        limit: Option<usize>,
    ) -> Result<Vec<ProjectPo>> {
        // 语法糖：调用通用查询（不限 root_user_id，用于系统级查询）
        let page = self
            .query(
                ctx,
                ProjectQuery {
                    status_in: Some(vec![status]),
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
        let now = common::constants::utils::current_timestamp_ms();
        let status_i32 = project.status as i32;
        sqlx::query!(
            "UPDATE projects SET name = ?, description = ?, workflow = ?, guidance = ?, \"status\" = ?, priority = ?, tags = ?, root_user_id = ?, owner_agent_id = ?, start_at = ?, due_at = ?, end_at = ?, modified_by = ?, updated_at = ?, execution_plan = ?, execution_result = ?, last_followup_at = ? WHERE id = ?",
            project.name, project.description, project.workflow, project.guidance, status_i32, project.priority, project.tags, project.root_user_id, project.owner_agent_id, project.start_at, project.due_at, project.end_at, project.modified_by, now, project.execution_plan, project.execution_result, project.last_followup_at, project.id
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
        let now = common::constants::utils::current_timestamp_ms();
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
        _ctx: RequestContext,
        search: ProjectSearch,
    ) -> Result<Vec<(ProjectPo, Option<f32>)>> {
        use sqlx::QueryBuilder;

        let pool = _ctx.db_pool();
        let keyword = search.keyword.unwrap_or_default();
        let limit_i64 = std::cmp::min(search.filters.pagination.limit.unwrap_or(20), 20) as i64;

        // 搜索计划：词元按长度路由（>=3 字符走 MATCH 短语 OR 组合，短词元走 LIKE 兜底）
        let Some(plan) = build_fts5_search_plan(&keyword) else {
            return Ok(Vec::new());
        };
        let filters = search.filters;

        // 业务过滤条件拼装（MATCH / LIKE 两条路径共用）
        // ✅ 复用业务过滤条件（修复原 SQL 未应用 root_user_id/owner_agent_id/status_in/ids 的缺陷）
        // 注意：不能直接复用 push_query_filters，因为它的字段引用不带表别名前缀；
        // 闭包对 QueryBuilder 生命周期高阶量化（HRTB），push_bind 的绑定值必须是
        // 所有权（'static），因此 String 过滤值在绑定点 clone
        let apply_filters = |builder: &mut sqlx::QueryBuilder<'_, sqlx::Sqlite>| {
            if let Some(ids) = &filters.ids
                && !ids.is_empty()
            {
                builder.push(" AND p.id IN (");
                let mut separated = builder.separated(", ");
                for id in ids {
                    separated.push_bind(id.clone());
                }
                separated.push_unseparated(")");
            }
            if let Some(root_user_id) = &filters.root_user_id {
                builder
                    .push(" AND p.root_user_id = ")
                    .push_bind(root_user_id.clone());
            }
            if let Some(owner_agent_id) = &filters.owner_agent_id {
                builder
                    .push(" AND p.owner_agent_id = ")
                    .push_bind(owner_agent_id.clone());
            }
            if let Some(status_list) = &filters.status_in
                && !status_list.is_empty()
            {
                builder.push(" AND p.\"status\" IN (");
                let mut separated = builder.separated(", ");
                for s in status_list {
                    separated.push_bind(*s as i32);
                }
                separated.push_unseparated(")");
            }
            // 默认排除软删除
            builder.push(" AND p.\"status\" != 0");
        };

        // 行 → 结果元组映射（两条路径共用）
        let to_result = |row: ProjectSearchRow| {
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
                execution_plan: row.execution_plan,
                execution_result: row.execution_result,
                last_followup_at: row.last_followup_at,
            };
            (po, row.fts_rank)
        };

        let mut primary: Vec<(ProjectPo, Option<f32>)> = Vec::new();
        let mut secondary: Vec<(ProjectPo, Option<f32>)> = Vec::new();

        // 路径一：MATCH 短语 OR 组合（多关键词任一命中即可），BM25 rank 排序
        // 注意：MATCH 左侧必须使用完整表名（非别名），否则 SQLite 会将别名解释为列名
        if let Some(match_expr) = &plan.match_expr {
            let mut builder = QueryBuilder::new(
                r#"SELECT p.id, p.name, p.description, p.workflow, p.guidance, p."status" as status,
                  p.priority, p.tags, p.root_user_id, p.owner_agent_id,
                  p.start_at, p.due_at, p.end_at, p.created_by, p.modified_by,
                  p.created_at, p.updated_at, p.execution_plan, p.execution_result, p.last_followup_at,
                  projects_fts.rank as fts_rank
           FROM projects_fts
           JOIN projects p ON projects_fts.rowid = p.rowid
           WHERE projects_fts MATCH "#,
            );
            builder.push_bind(match_expr);
            apply_filters(&mut builder);

            builder.push(" ORDER BY projects_fts.rank LIMIT ");
            builder.push_bind(limit_i64);
            if let Some(offset) = filters.pagination.offset {
                builder.push(" OFFSET ").push_bind(offset as i64);
            }

            let rows: Vec<ProjectSearchRow> = builder
                .build_query_as::<ProjectSearchRow>()
                .fetch_all(pool)
                .await?;
            primary.extend(rows.into_iter().map(to_result));
        }

        // 路径二：短词元（<3 字符，trigram 无法形成 token）LIKE 兜底，
        // 直接查主表列，BM25 不可用，改按 updated_at 排序
        if !plan.like_patterns.is_empty() {
            let mut builder = QueryBuilder::new(
                r#"SELECT p.id, p.name, p.description, p.workflow, p.guidance, p."status" as status,
              p.priority, p.tags, p.root_user_id, p.owner_agent_id,
              p.start_at, p.due_at, p.end_at, p.created_by, p.modified_by,
              p.created_at, p.updated_at, p.execution_plan, p.execution_result, p.last_followup_at,
              NULL as fts_rank
       FROM projects p
       WHERE "#,
            );
            // 条件与绑定值交错推送：QueryBuilder 里 push 文本中的 ? 不由 push_bind 消费
            builder.push("(");
            push_fts5_like_conditions(
                &mut builder,
                "p",
                &["name", "description", "workflow", "guidance", "tags"],
                &plan.like_patterns,
            );
            builder.push(")");
            apply_filters(&mut builder);

            builder.push(" ORDER BY p.updated_at DESC, p.id DESC LIMIT ");
            builder.push_bind(limit_i64);
            if let Some(offset) = filters.pagination.offset {
                builder.push(" OFFSET ").push_bind(offset as i64);
            }

            let rows: Vec<ProjectSearchRow> = builder
                .build_query_as::<ProjectSearchRow>()
                .fetch_all(pool)
                .await?;
            secondary.extend(rows.into_iter().map(to_result));
        }

        // 合并去重：MATCH 结果（BM25 序）在前，LIKE 结果仅补位
        Ok(merge_fts5_results(
            primary,
            secondary,
            |po| po.id.clone(),
            limit_i64.max(0) as usize,
        ))
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
