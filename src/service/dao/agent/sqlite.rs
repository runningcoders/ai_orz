//! AgentDao SQLite 实现

use crate::models::agent::AgentPo;
use crate::pkg::RequestContext;
use crate::pkg::cache::TtlCache;
use crate::pkg::storage::{build_fts5_search_plan, merge_fts5_results, push_fts5_like_conditions};
use crate::service::dao::agent::{AgentDao, AgentQuery, AgentSearch};
use chrono::Utc;
use common::enums::AgentKind;
use common::enums::AgentStatus;
use common::error::Result;
use sqlx::FromRow;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

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

/// 创建一个全新的 Agent DAO 实例（用于测试；默认关闭缓存，避免跨测试库串数据）
pub fn new() -> Arc<dyn AgentDao> {
    Arc::new(AgentDaoSqliteImpl::with_cache(false))
}

/// 获取 AgentDao 单例
pub fn dao() -> Arc<dyn AgentDao> {
    AGENT_DAO.get().cloned().unwrap()
}

/// 初始化单例
///
/// 生产构建开启实体 / 名单缓存；测试构建自动关闭，避免全局缓存与
/// 各测试用例直接改库后的预期冲突。
pub fn init() {
    let _ = AGENT_DAO.set(Arc::new(AgentDaoSqliteImpl::with_cache(!cfg!(test))));
}

// ==================== 缓存 ====================
//
// 只做第一层：Agent 维度（按 id）的实体缓存。
// 「要哪些 Agent」是上层业务语义，ID 名单由上层自己缓存，DAO 不感知。

/// 实体缓存：按 Agent id 单条缓存，写路径主动失效
const ENTITY_TTL: Duration = Duration::from_secs(60);

fn entity_cache() -> &'static TtlCache<String, AgentPo> {
    static CACHE: OnceLock<&'static TtlCache<String, AgentPo>> = OnceLock::new();
    CACHE.get_or_init(|| crate::pkg::cache::register("hr.agent_dao.entities", ENTITY_TTL))
}

/// 写路径失效单个实体
fn invalidate(agent_id: &str) {
    entity_cache().invalidate(&agent_id.to_string());
}

// ==================== 实现 ====================

struct AgentDaoSqliteImpl {
    /// 是否启用实体 / 名单缓存（测试实例关闭，避免共享全局缓存串库）
    cache_enabled: bool,
}

impl AgentDaoSqliteImpl {
    fn with_cache(cache_enabled: bool) -> Self {
        Self { cache_enabled }
    }

    /// 回填实体缓存（Deleted 永不入缓存，保证 `find_by_id` 语义不打折）
    fn fill_entities(items: &[AgentPo]) {
        for po in items {
            if po.status != AgentStatus::Deleted {
                entity_cache().insert(po.id.clone(), po.clone());
            }
        }
    }

    /// 无缓存路径：一次批量查询后按入参 ID 顺序重排
    async fn fetch_ordered(&self, ctx: &RequestContext, ids: &[String]) -> Result<Vec<AgentPo>> {
        let page = self
            .query_page(
                ctx,
                AgentQuery {
                    ids: Some(ids.to_vec()),
                    exclude_status: Some(AgentStatus::Deleted),
                    ..Default::default()
                },
            )
            .await?;
        let found: HashMap<String, AgentPo> = page
            .items
            .into_iter()
            .map(|po| (po.id.clone(), po))
            .collect();
        Ok(ids.iter().filter_map(|id| found.get(id).cloned()).collect())
    }

    /// 未缓存的通用查询（COUNT + LIST），供公开 `query` 与批量补齐共用
    async fn query_page(
        &self,
        ctx: &RequestContext,
        query: AgentQuery,
    ) -> Result<common::api::PagedResult<AgentPo>> {
        let pool = ctx.db_pool();
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

        if self.cache_enabled {
            invalidate(&agent.id);
        }
        Ok(())
    }

    async fn find_by_id(&self, _ctx: RequestContext, id: &str) -> Result<Option<AgentPo>> {
        let key = id.to_string();
        if self.cache_enabled
            && let Some(po) = entity_cache().get(&key)
            && po.status != AgentStatus::Deleted
        {
            return Ok(Some(po));
        }

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

        if self.cache_enabled
            && let Some(po) = &agent
        {
            entity_cache().insert(key, po.clone());
        }
        Ok(agent)
    }

    async fn query(
        &self,
        _ctx: RequestContext,
        query: AgentQuery,
    ) -> Result<common::api::PagedResult<AgentPo>> {
        // 过滤条件千变万化，DAO 不为查询缓存「名单」；只把查回来的实体回填缓存。
        let page = self.query_page(&_ctx, query).await?;
        if self.cache_enabled {
            Self::fill_entities(&page.items);
        }
        Ok(page)
    }

    async fn find_by_ids(&self, _ctx: RequestContext, ids: &[String]) -> Result<Vec<AgentPo>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        if !self.cache_enabled {
            return self.fetch_ordered(&_ctx, ids).await;
        }

        let mut found: HashMap<String, AgentPo> = HashMap::new();
        let mut missing: HashSet<String> = HashSet::new();
        for id in ids {
            match entity_cache().get(id) {
                Some(po) => {
                    found.insert(id.clone(), po);
                }
                None => {
                    missing.insert(id.clone());
                }
            }
        }

        if !missing.is_empty() {
            let page = self
                .query_page(
                    &_ctx,
                    AgentQuery {
                        ids: Some(missing.into_iter().collect()),
                        exclude_status: Some(AgentStatus::Deleted),
                        ..Default::default()
                    },
                )
                .await?;
            Self::fill_entities(&page.items);
            for po in page.items {
                found.insert(po.id.clone(), po);
            }
        }

        Ok(ids.iter().filter_map(|id| found.get(id).cloned()).collect())
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
        let results = merge_fts5_results(
            primary,
            secondary,
            |po| po.id.clone(),
            limit_i64.max(0) as usize,
        );

        if self.cache_enabled {
            for (po, _) in &results {
                if po.status != AgentStatus::Deleted {
                    entity_cache().insert(po.id.clone(), po.clone());
                }
            }
        }
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

        if self.cache_enabled {
            invalidate(&agent.id);
        }
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

        if self.cache_enabled {
            invalidate(&agent.id);
        }
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
