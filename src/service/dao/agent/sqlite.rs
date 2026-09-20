//! AgentDao SQLite 实现

use crate::models::agent::AgentPo;
use crate::pkg::RequestContext;
use crate::pkg::storage::{build_fts5_search_plan, merge_fts5_results, push_fts5_like_conditions};
use crate::service::dao::agent::{AgentDao, AgentQuery, AgentSearch};
use chrono::Utc;
use common::enums::AgentKind;
use common::enums::AgentStatus;
use common::error::Result;
use sqlx::FromRow;
use std::sync::{Arc, OnceLock};

// ==================== FTS5 辅助 ====================

/// Agent 搜索行（PO + fts_rank）
#[derive(FromRow)]
struct AgentSearchRow {
    id: String,
    name: String,
    role: String,
    description: String,
    soul: String,
    capabilities: String,
    runtime_config: String,
    model_provider_id: String,
    status: AgentStatus,
    kind: common::enums::AgentKind,
    created_by: String,
    modified_by: String,
    created_at: i64,
    updated_at: i64,
    fts_rank: Option<f32>,
}

// ==================== 工厂方法 + 单例 ====================

static AGENT_DAO: OnceLock<Arc<dyn AgentDao>> = OnceLock::new();

/// 创建一个全新的 Agent DAO 实例（用于测试）
pub fn new() -> Arc<dyn AgentDao> {
    Arc::new(AgentDaoSqliteImpl::new())
}

/// 获取 AgentDao 单例
pub fn dao() -> Arc<dyn AgentDao> {
    AGENT_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = AGENT_DAO.set(new());
}

// ==================== 实现 ====================

struct AgentDaoSqliteImpl;

impl AgentDaoSqliteImpl {
    fn new() -> Self {
        Self
    }
}
#[async_trait::async_trait]
impl AgentDao for AgentDaoSqliteImpl {
    async fn insert(&self, _ctx: RequestContext, agent: &AgentPo) -> Result<()> {
        let status = agent.status as i32;
        let kind = agent.kind as i32;
        sqlx::query!(
            "INSERT INTO agents (id, name, role, description, soul, capabilities, model_provider_id, runtime_config, status, kind, created_by, modified_by, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            agent.id,
            agent.name,
            agent.role,
            agent.description,
            agent.soul,
            agent.capabilities,
            agent.model_provider_id,
            agent.runtime_config,
            status,
            kind,
            agent.created_by,
            agent.modified_by,
            agent.created_at,
            agent.updated_at
        )
            .execute(_ctx.db_pool())
            .await?;

        Ok(())
    }

    async fn find_by_id(&self, _ctx: RequestContext, id: &str) -> Result<Option<AgentPo>> {
        let agent = sqlx::query_as!(
            AgentPo,
            r#"
SELECT id, name, role, description, soul, capabilities, runtime_config,
       model_provider_id, status as 'status: AgentStatus', kind as 'kind: AgentKind',
       created_by, modified_by, created_at, updated_at
FROM agents WHERE id = ? AND status <> 0
            "#,
            id
        )
        .fetch_optional(_ctx.db_pool())
        .await?;

        Ok(agent)
    }

    async fn query(
        &self,
        _ctx: RequestContext,
        query: AgentQuery,
    ) -> Result<common::api::PagedResult<AgentPo>> {
        let pool = _ctx.db_pool();
        let mut count_builder = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM agents WHERE 1=1");
        push_query_filters(&mut count_builder, &query);
        let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;

        let mut list_builder = sqlx::QueryBuilder::new(
            r#"SELECT id, name, role, description, soul, capabilities, runtime_config, model_provider_id, status, kind, created_by, modified_by, created_at, updated_at FROM agents WHERE 1=1"#,
        );
        push_query_filters(&mut list_builder, &query);
        list_builder.push(" ORDER BY created_at DESC");

        if let Some(limit) = query.pagination.limit {
            list_builder.push(" LIMIT ").push_bind(limit as i64);
        } else if query.pagination.offset.is_some() {
            list_builder.push(" LIMIT -1");
        }
        if let Some(offset) = query.pagination.offset {
            list_builder.push(" OFFSET ").push_bind(offset as i64);
        }

        let items = list_builder.build_query_as().fetch_all(pool).await?;
        Ok(common::api::PagedResult {
            items,
            total: total as usize,
        })
    }

    async fn search_agents(
        &self,
        _ctx: RequestContext,
        search: AgentSearch,
    ) -> Result<Vec<(AgentPo, Option<f32>)>> {
        use sqlx::QueryBuilder;

        let keyword = search.keyword.unwrap_or_default();
        let limit_i64 = std::cmp::min(search.filters.pagination.limit.unwrap_or(20), 20) as i64;

        // 搜索计划：词元按长度路由（>=3 字符走 MATCH 短语 OR 组合，短词元走 LIKE 兜底）
        let Some(plan) = build_fts5_search_plan(&keyword) else {
            return Ok(Vec::new());
        };
        let filters = search.filters;

        // 业务过滤条件拼装（MATCH / LIKE 两条路径共用）
        // 注意：闭包对 QueryBuilder 生命周期高阶量化（HRTB），push_bind 的绑定值必须是
        // 所有权（'static），因此 String 过滤值在绑定点 clone
        let apply_filters = |builder: &mut sqlx::QueryBuilder<'_, sqlx::Sqlite>| {
            if let Some(ids) = &filters.ids
                && !ids.is_empty()
            {
                builder.push(" AND m.id IN (");
                let mut separated = builder.separated(", ");
                for id in ids {
                    separated.push_bind(id.clone());
                }
                separated.push_unseparated(")");
            }

            if let Some(status) = &filters.status {
                builder.push(" AND m.status = ").push_bind(*status as i32);
            }

            if let Some(exclude_status) = &filters.exclude_status {
                builder
                    .push(" AND m.status != ")
                    .push_bind(*exclude_status as i32);
            }

            if let Some(created_by) = &filters.created_by {
                builder
                    .push(" AND m.created_by = ")
                    .push_bind(created_by.clone());
            }

            if let Some(model_provider_id) = &filters.model_provider_id {
                builder
                    .push(" AND m.model_provider_id = ")
                    .push_bind(model_provider_id.clone());
            }

            // 角色标签过滤（OR 语义，使用 json_each 精确匹配）
            if let Some(roles) = &filters.roles
                && !roles.is_empty()
            {
                builder.push(
                    " AND EXISTS (SELECT 1 FROM json_each(m.role) WHERE json_each.value IN (",
                );
                let mut separated = builder.separated(", ");
                for role in roles {
                    separated.push_bind(role.clone());
                }
                separated.push_unseparated("))");
            }
        };

        // 行 → 结果元组映射（两条路径共用）
        let to_result = |row: AgentSearchRow| {
            let po = AgentPo {
                id: row.id,
                name: row.name,
                role: row.role,
                description: row.description,
                soul: row.soul,
                capabilities: row.capabilities,
                runtime_config: row.runtime_config,
                model_provider_id: row.model_provider_id,
                status: row.status,
                kind: row.kind,
                created_by: row.created_by,
                modified_by: row.modified_by,
                created_at: row.created_at,
                updated_at: row.updated_at,
            };
            (po, row.fts_rank)
        };

        let mut primary: Vec<(AgentPo, Option<f32>)> = Vec::new();
        let mut secondary: Vec<(AgentPo, Option<f32>)> = Vec::new();

        // 路径一：MATCH 短语 OR 组合（多关键词任一命中即可），BM25 rank 排序
        // 注意：MATCH 左侧必须使用完整表名（非别名），否则 SQLite 会将别名解释为列名
        // agents 表的 status 字段不是 SQL 关键字，不需要双引号转义
        if let Some(match_expr) = &plan.match_expr {
            let mut builder = QueryBuilder::new(
                r#"SELECT m.id, m.name, m.role, m.description, m.soul, m.capabilities, m.runtime_config,
                      m.model_provider_id, m.status, m.kind, m.created_by, m.modified_by, m.created_at, m.updated_at,
                      agents_fts.rank as fts_rank
               FROM agents_fts
               JOIN agents m ON agents_fts.rowid = m.rowid
               WHERE agents_fts MATCH "#,
            );
            builder.push_bind(match_expr);
            apply_filters(&mut builder);

            builder.push(" ORDER BY agents_fts.rank LIMIT ");
            builder.push_bind(limit_i64);
            if let Some(offset) = filters.pagination.offset {
                builder.push(" OFFSET ").push_bind(offset as i64);
            }

            let rows: Vec<AgentSearchRow> = builder
                .build_query_as::<AgentSearchRow>()
                .fetch_all(_ctx.db_pool())
                .await?;
            primary.extend(rows.into_iter().map(to_result));
        }

        // 路径二：短词元（<3 字符，trigram 无法形成 token）LIKE 兜底，
        // 直接查主表列，BM25 不可用，改按 updated_at 排序
        if !plan.like_patterns.is_empty() {
            let mut builder = QueryBuilder::new(
                r#"SELECT m.id, m.name, m.role, m.description, m.soul, m.capabilities, m.runtime_config,
                  m.model_provider_id, m.status, m.kind, m.created_by, m.modified_by, m.created_at, m.updated_at,
                  NULL as fts_rank
           FROM agents m
           WHERE "#,
            );
            // 条件与绑定值交错推送：QueryBuilder 里 push 文本中的 ? 不由 push_bind 消费
            builder.push("(");
            push_fts5_like_conditions(
                &mut builder,
                "m",
                &["name", "role", "description", "capabilities"],
                &plan.like_patterns,
            );
            builder.push(")");
            apply_filters(&mut builder);

            builder.push(" ORDER BY m.updated_at DESC, m.id DESC LIMIT ");
            builder.push_bind(limit_i64);
            if let Some(offset) = filters.pagination.offset {
                builder.push(" OFFSET ").push_bind(offset as i64);
            }

            let rows: Vec<AgentSearchRow> = builder
                .build_query_as::<AgentSearchRow>()
                .fetch_all(_ctx.db_pool())
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

    async fn find_all(&self, _ctx: RequestContext) -> Result<Vec<AgentPo>> {
        // 语法糖：调用通用查询，排除已删除状态
        let page = self
            .query(
                _ctx,
                AgentQuery {
                    exclude_status: Some(AgentStatus::Deleted),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    async fn update(&self, _ctx: RequestContext, agent: &AgentPo) -> Result<()> {
        let current_timestamp = Utc::now().timestamp_millis();
        let status = agent.status as i32;
        let kind = agent.kind as i32;
        let uid = _ctx.caller_id_or_system();
        sqlx::query!(
            r#"
UPDATE agents
SET name = ?, role = ?, description = ?, soul = ?, capabilities = ?, runtime_config = ?,
    model_provider_id = ?, status = ?, kind = ?, created_by = ?, modified_by = ?, created_at = ?, updated_at = ?
WHERE id = ?
            "#,
            agent.name,
            agent.role,
            agent.description,
            agent.soul,
            agent.capabilities,
            agent.runtime_config,
            agent.model_provider_id,
            status,
            kind,
            agent.created_by,
            uid,
            agent.created_at,
            current_timestamp,
            agent.id
        )
            .execute(_ctx.db_pool())
            .await?;

        Ok(())
    }

    async fn delete(&self, _ctx: RequestContext, agent: &AgentPo) -> Result<()> {
        let current_timestamp = Utc::now().timestamp_millis();
        let uid = _ctx.caller_id_or_system();
        sqlx::query!(
            r#"
UPDATE agents SET status = 0, modified_by = ?, updated_at = ? WHERE id = ?
            "#,
            uid,
            current_timestamp,
            agent.id
        )
        .execute(_ctx.db_pool())
        .await?;

        Ok(())
    }

    async fn count(&self, _ctx: RequestContext, query: AgentQuery) -> Result<u64> {
        let pool = _ctx.db_pool();
        let mut count_builder = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM agents WHERE 1=1");
        push_query_filters(&mut count_builder, &query);
        let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;
        Ok(total as u64)
    }
}

/// 推送查询过滤条件到 QueryBuilder（COUNT 和 LIST 查询复用）
fn push_query_filters<'args>(
    builder: &mut sqlx::QueryBuilder<'args, sqlx::Sqlite>,
    query: &AgentQuery,
) {
    if let Some(ids) = &query.ids
        && !ids.is_empty()
    {
        builder.push(" AND id IN (");
        let mut separated = builder.separated(", ");
        for id in ids {
            separated.push_bind(id.clone());
        }
        separated.push_unseparated(")");
    }
    if let Some(status) = &query.status {
        builder.push(" AND status = ").push_bind(*status as i32);
    }
    if let Some(exclude_status) = &query.exclude_status {
        builder
            .push(" AND status != ")
            .push_bind(*exclude_status as i32);
    }
    if let Some(created_by) = &query.created_by {
        builder
            .push(" AND created_by = ")
            .push_bind(created_by.clone());
    }
    if let Some(model_provider_id) = &query.model_provider_id {
        builder
            .push(" AND model_provider_id = ")
            .push_bind(model_provider_id.clone());
    }
    if let Some(roles) = &query.roles
        && !roles.is_empty()
    {
        builder
            .push(" AND EXISTS (SELECT 1 FROM json_each(agents.role) WHERE json_each.value IN (");
        let mut separated = builder.separated(", ");
        for role in roles {
            separated.push_bind(role.clone());
        }
        separated.push_unseparated("))");
    }
    if let Some(keyword) = &query.keyword
        && !keyword.is_empty()
    {
        log_warn!(
            "keyword in agent query is deprecated, use search_agents for FTS5; keyword ignored"
        );
    }
}
