//! AgentDao SQLite 实现

use common::error::{Error, Result};
use crate::models::agent::AgentPo;
use crate::pkg::RequestContext;
use crate::service::dao::agent::{AgentDao, AgentQuery, AgentSearch};
use crate::service::dao::memory::sqlite::escape_fts5_keyword;
use chrono::Utc;
use common::enums::AgentStatus;
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
        sqlx::query!(
            "INSERT INTO agents (id, name, role, description, soul, capabilities, model_provider_id, runtime_config, status, created_by, modified_by, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            agent.id,
            agent.name,
            agent.role,
            agent.description,
            agent.soul,
            agent.capabilities,
            agent.model_provider_id,
            agent.runtime_config,
            status,
            agent.created_by,
            agent.modified_by,
            agent.created_at,
            agent.updated_at
        )
            .execute(_ctx.db_pool())
            .await?;

        Ok(())
    }

    async fn find_by_id(
        &self,
        _ctx: RequestContext,
        id: &str,
    ) -> Result<Option<AgentPo>> {
        let agent = sqlx::query_as!(
            AgentPo,
            r#"
SELECT id, name, role, description, soul, capabilities, runtime_config,
       model_provider_id, status as 'status: AgentStatus', created_by, modified_by, created_at, updated_at
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
    ) -> Result<Vec<AgentPo>> {
        let mut builder = sqlx::QueryBuilder::new(
            r#"SELECT id, name, role, description, soul, capabilities, runtime_config, model_provider_id, status, created_by, modified_by, created_at, updated_at FROM agents WHERE 1=1"#,
        );

        // ✅ 按 ID 批量查询（向量搜索的核心过滤）
        if let Some(ids) = &query.ids {
            if !ids.is_empty() {
                builder.push(" AND id IN (");
                let mut separated = builder.separated(", ");
                for id in ids {
                    separated.push_bind(id);
                }
                separated.push_unseparated(")");
            }
        }

        // 状态过滤
        if let Some(status) = &query.status {
            builder.push(" AND status = ").push_bind(*status as i32);
        }

        // 排除状态过滤
        if let Some(exclude_status) = &query.exclude_status {
            builder
                .push(" AND status != ")
                .push_bind(*exclude_status as i32);
        }

        // 创建者过滤
        if let Some(created_by) = &query.created_by {
            builder.push(" AND created_by = ").push_bind(created_by);
        }

        // 模型提供商过滤
        if let Some(model_provider_id) = &query.model_provider_id {
            builder
                .push(" AND model_provider_id = ")
                .push_bind(model_provider_id);
        }

        // 关键词搜索已迁移到 FTS5 全文索引（search_agents 方法）
        // query 方法的 keyword 字段已废弃，仅记录 warn 日志
        if let Some(keyword) = &query.keyword {
            if !keyword.is_empty() {
                log_warn!("keyword in agent query is deprecated, use search_agents for FTS5 full-text search; keyword ignored");
            }
        }

        // 排序
        builder.push(" ORDER BY created_at DESC");

        // 限制数量
        if let Some(limit) = query.limit {
            builder.push(" LIMIT ").push_bind(limit as i64);
        }

        // 执行查询
        let rows = builder.build_query_as().fetch_all(_ctx.db_pool()).await?;

        Ok(rows)
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
                      m.model_provider_id, m.status, m.created_by, m.modified_by, m.created_at, m.updated_at,
                      agents_fts.rank as fts_rank
               FROM agents_fts
               JOIN agents m ON agents_fts.rowid = m.rowid
               WHERE agents_fts MATCH "#,
        );
        builder.push_bind(escaped_keyword);

        // 应用业务过滤条件
        if let Some(ids) = &filters.ids {
            if !ids.is_empty() {
                builder.push(" AND m.id IN (");
                let mut separated = builder.separated(", ");
                for id in ids {
                    separated.push_bind(id);
                }
                separated.push_unseparated(")");
            }
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

        builder.push(" ORDER BY agents_fts.rank");

        if let Some(limit) = filters.limit {
            builder.push(" LIMIT ").push_bind(limit as i64);
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
        self.query(
            _ctx,
            AgentQuery {
                exclude_status: Some(AgentStatus::Deleted),
                ..Default::default()
            },
        )
        .await
    }

    async fn update(&self, _ctx: RequestContext, agent: &AgentPo) -> Result<()> {
        let current_timestamp = Utc::now().timestamp();
        let status = agent.status as i32;
        let uid = _ctx.uid();
        sqlx::query!(
            r#"
UPDATE agents
SET name = ?, role = ?, description = ?, soul = ?, capabilities = ?, runtime_config = ?,
    model_provider_id = ?, status = ?, created_by = ?, modified_by = ?, created_at = ?, updated_at = ?
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
        let uid = _ctx.uid().to_string();
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
}