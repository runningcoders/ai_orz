//! AgentDao SQLite 实现

use crate::models::agent::AgentPo;
use crate::pkg::RequestContext;
use crate::pkg::storage::escape_fts5_keyword;
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

        // 空关键词直接返回空结果（FTS5 MATCH 空字符串会报错）
        if keyword.trim().is_empty() {
            return Ok(Vec::new());
        }

        // 转义关键词为 FTS5 短语匹配
        let escaped_keyword = escape_fts5_keyword(&keyword);
        let filters = search.filters;

        // FTS5 MATCH + JOIN + BM25 排序
        // 注意：MATCH 左侧必须使用完整表名（非别名），否则 SQLite 会将别名解释为列名
        // agents 表的 status 字段不是 SQL 关键字，不需要双引号转义
        let mut builder = QueryBuilder::new(
            r#"SELECT m.id, m.name, m.role, m.description, m.soul, m.capabilities, m.runtime_config,
                      m.model_provider_id, m.status, m.kind, m.created_by, m.modified_by, m.created_at, m.updated_at,
                      agents_fts.rank as fts_rank
               FROM agents_fts
               JOIN agents m ON agents_fts.rowid = m.rowid
               WHERE agents_fts MATCH "#,
        );
        builder.push_bind(escaped_keyword);

        // 应用业务过滤条件
        if let Some(ids) = &filters.ids
            && !ids.is_empty()
        {
            builder.push(" AND m.id IN (");
            let mut separated = builder.separated(", ");
            for id in ids {
                separated.push_bind(id);
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
            builder.push(" AND m.created_by = ").push_bind(created_by);
        }

        if let Some(model_provider_id) = &filters.model_provider_id {
            builder
                .push(" AND m.model_provider_id = ")
                .push_bind(model_provider_id);
        }

        // 角色标签过滤（OR 语义，使用 json_each 精确匹配）
        if let Some(roles) = &filters.roles
            && !roles.is_empty()
        {
            builder.push(" AND EXISTS (SELECT 1 FROM json_each(m.role) WHERE json_each.value IN (");
            let mut separated = builder.separated(", ");
            for role in roles {
                separated.push_bind(role);
            }
            separated.push_unseparated("))");
        }

        builder.push(" ORDER BY agents_fts.rank");

        // 搜索场景限制最大返回数量（避免关键词失控返回全量结果）
        // 用户传的 limit 若超过 MAX_SEARCH_RESULTS 则截断，未传则默认 MAX_SEARCH_RESULTS
        let search_limit = std::cmp::min(filters.pagination.limit.unwrap_or(20), 20);
        builder.push(" LIMIT ").push_bind(search_limit as i64);

        if let Some(offset) = filters.pagination.offset {
            builder.push(" OFFSET ").push_bind(offset as i64);
        }

        let rows: Vec<AgentSearchRow> = builder
            .build_query_as::<AgentSearchRow>()
            .fetch_all(_ctx.db_pool())
            .await?;

        let results = rows
            .into_iter()
            .map(|row| {
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
            })
            .collect();

        Ok(results)
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
        let current_timestamp = Utc::now().timestamp();
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
        let current_timestamp = Utc::now().timestamp();
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
