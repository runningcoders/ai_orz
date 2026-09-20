//! Memory DAO SQLite 实现
//!
//! 负责：
//! - 短期记忆索引的增删查改（SQLite）
//! - 长期知识图谱节点和关系的增删查改（SQLite）
//! - 记忆追踪文件的写入（每日文件追加）
//! - 原始记忆不可修改不可删除，只能追加

use crate::config;
use crate::models::memory::{
    KnowledgeNodeRelationPo, KnowledgeReferencePo, LongTermKnowledgeNodePo, MemoryTrace,
    MemoryTracePosition, ShortTermMemoryIndexPo,
};
use crate::pkg::RequestContext;
use crate::pkg::paths;
use crate::pkg::storage::{build_fts5_like_clause, build_fts5_search_plan, merge_fts5_results};
use crate::service::dao::memory::{
    DriftClassDetailRow, DriftRelationDetailRow, MemoryDao, MemoryQuery, MemorySearch,
    TermFrequencyRow,
};
use async_trait::async_trait;
use common::api::{PagedResult, PaginationParams};
use common::enums::{KnowledgeRelationStatus, MemoryStatus, MemoryType};
use common::error::{Result, bail_err};
use common::ontology::normalize;
use serde_json;
use sqlx::{FromRow, QueryBuilder, Sqlite, SqlitePool};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

/// SQLite 默认绑定参数上限为 999（SQLITE_MAX_VARIABLE_NUMBER）。
/// `list_relations_batch` 每个节点 ID 占 2 个绑定（source + target 两个 IN 列表），
/// 按 400 个节点分块（800 绑定）留出安全余量；ids IN 单绑定场景同样复用此常量。
pub const IN_CLAUSE_CHUNK: usize = 400;

/// 词频聚合返回行数的防御上限：正常本体词表远小于该值，
/// 防御脏数据（海量不同 node_type / relation_type）导致 dashboard 全量聚合行数爆炸
pub const MAX_WORD_FREQ_ROWS: i64 = 1000;

/// 短期记忆搜索行（PO + fts_rank）
#[derive(FromRow)]
struct ShortTermSearchRow {
    id: String,
    agent_id: String,
    task_id: Option<String>,
    role: String,
    summary: String,
    tags: String,
    trace_ids: String,
    status: MemoryStatus,
    created_at: i64,
    updated_at: i64,
    fts_rank: Option<f32>,
}

/// 知识节点搜索行（PO + fts_rank）
#[derive(FromRow)]
struct KnowledgeNodeSearchRow {
    id: String,
    agent_id: String,
    node_name: String,
    node_description: String,
    node_type: String,
    summary: String,
    tags: String,
    status: MemoryStatus,
    is_published: bool,
    created_at: i64,
    updated_at: i64,
    fts_rank: Option<f32>,
}

// ==================== 工厂方法 + 单例 ====================

static MEMORY_DAO: OnceLock<Arc<dyn super::MemoryDao + Send + Sync>> = OnceLock::new();

/// 创建一个全新的 Memory DAO 实例（用于测试）
pub fn new() -> Arc<dyn super::MemoryDao + Send + Sync> {
    Arc::new(MemoryDaoSqliteImpl::new())
}

/// 获取 Memory DAO 单例
pub fn dao() -> Arc<dyn super::MemoryDao + Send + Sync> {
    MEMORY_DAO.get().cloned().unwrap()
}

/// 初始化 Memory DAO 单例
pub fn init() {
    let _ = MEMORY_DAO.set(new());
}

/// SQLite Memory DAO 实现
#[derive(Default)]
pub struct MemoryDaoSqliteImpl;

impl MemoryDaoSqliteImpl {
    /// 创建新的 DAO 实例
    pub fn new() -> Self {
        MemoryDaoSqliteImpl
    }

    /// 获取 Agent 记忆目录完整路径（用于写入）
    fn agent_memory_dir(&self, agent_id: &str) -> PathBuf {
        let base = config::get().base_data_path();
        paths::agent_memory_dir(&base, agent_id)
    }

    /// 获取连接池从上下文
    fn pool(&self, ctx: RequestContext) -> SqlitePool {
        ctx.db_pool().clone()
    }

    /// Read original memory content by knowledge reference
    ///
    /// Uses date_path (YYYYMMDD.jsonl) + line_number to read the exact JSON line
    pub fn read_memory_reference(&self, reference: &KnowledgeReferencePo) -> Result<String> {
        // Full path: agent memory dir + date file name
        let agent_id = reference
            .knowledge_id
            .split('/')
            .next()
            .unwrap_or(&reference.knowledge_id);
        let agent_dir = self.agent_memory_dir(agent_id);
        let writer = crate::pkg::daily_jsonl::DailyJsonlWriter::new(agent_dir);
        // date_path is just YYYYMMDD.jsonl
        let date = reference.date_path.replace(".jsonl", "");
        let line = writer.read_line(&date, reference.line_number as usize)?;
        // Parse as MemoryTrace and return formatted content for display
        let trace: MemoryTrace = serde_json::from_str(&line)?;
        Ok(trace.input)
    }

    /// 关系边行 → PO（动态 SQL 通用映射；status 按 KnowledgeRelationStatus 解码）
    fn knowledge_relation_from_row(row: &sqlx::sqlite::SqliteRow) -> KnowledgeNodeRelationPo {
        use sqlx::Row;

        KnowledgeNodeRelationPo {
            id: row.get("id"),
            source_node_id: row.get("source_node_id"),
            target_node_id: row.get("target_node_id"),
            relation_type: row.get("relation_type"),
            // 动态 SQL 走 Row::get：显式取 f64 再收窄，避免依赖 f32 的 Decode 实现
            weight: row.get::<Option<f64>, _>("weight").map(|w| w as f32),
            status: row.get("status"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        }
    }

    /// `list_relations_batch` 的单块查询（调用方保证 chunk 长度 ≤ IN_CLAUSE_CHUNK）
    async fn list_relations_batch_chunk(
        &self,
        ctx: RequestContext,
        node_ids: &[String],
    ) -> Result<Vec<KnowledgeNodeRelationPo>> {
        use sqlx::QueryBuilder;

        let pool = self.pool(ctx);
        let mut builder = QueryBuilder::new(
            r#"SELECT id, source_node_id, target_node_id, relation_type, weight, "status", created_at, updated_at
FROM knowledge_node_relation
WHERE (source_node_id IN ("#,
        );

        let mut separated = builder.separated(", ");
        for id in node_ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(") OR target_node_id IN (");

        let mut separated = builder.separated(", ");
        for id in node_ids {
            separated.push_bind(id);
        }
        // 双闭括号：先闭 IN( 再闭外层分组(，保证 status 过滤作用于整个 OR 表达式
        separated.push_unseparated(")) AND \"status\" = 1 ORDER BY created_at ASC");

        let rows = builder.build().fetch_all(&pool).await?;

        let mut result = Vec::new();
        for row in rows {
            // 关系类型原样带出（落库存的就是原文，读取侧不做任何枚举归一）
            result.push(Self::knowledge_relation_from_row(&row));
        }

        Ok(result)
    }

    /// `query_knowledge_relations` 的单块查询（调用方保证 chunk 长度 ≤ IN_CLAUSE_CHUNK）
    async fn query_relations_by_ids_chunk(
        &self,
        ctx: RequestContext,
        ids: &[String],
    ) -> Result<Vec<KnowledgeNodeRelationPo>> {
        use sqlx::QueryBuilder;

        let pool = self.pool(ctx);
        let mut builder = QueryBuilder::new(
            r#"SELECT id, source_node_id, target_node_id, relation_type, weight, "status", created_at, updated_at
FROM knowledge_node_relation
WHERE id IN ("#,
        );

        let mut separated = builder.separated(", ");
        for id in ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(") AND \"status\" = 1 ORDER BY created_at ASC");

        let rows = builder.build().fetch_all(&pool).await?;

        Ok(rows.iter().map(Self::knowledge_relation_from_row).collect())
    }

    /// 把同键 (source, target, relation_type) 的旧生效边降级为 Superseded(0)
    ///
    /// 「版本化替换」的写侧闸门：与 INSERT 同事务执行，配合部分唯一索引
    /// `uq_knowledge_relation_active_edge` 双保险，保证同一有向三元组只有一条
    /// 生效边。旧边只降级不删除 —— 历史版本永久留库，记忆因果链回放依赖它。
    async fn supersede_active_edge(
        tx: &mut sqlx::SqliteConnection,
        relation: &KnowledgeNodeRelationPo,
    ) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        // 匹配键同样归一：与 INSERT 侧写入的规范形同口径，才能命中旧边
        let relation_type = normalize(&relation.relation_type);
        sqlx::query!(
            r#"
UPDATE knowledge_node_relation
SET "status" = 0, updated_at = ?
WHERE source_node_id = ? AND target_node_id = ? AND relation_type = ? AND "status" = 1
"#,
            now,
            relation.source_node_id,
            relation.target_node_id,
            relation_type,
        )
        .execute(tx)
        .await?;

        Ok(())
    }
}

#[async_trait]
impl MemoryDao for MemoryDaoSqliteImpl {
    async fn append_trace(
        &self,
        _ctx: RequestContext,
        trace: &MemoryTrace,
    ) -> Result<MemoryTracePosition> {
        let agent_dir = self.agent_memory_dir(&trace.agent_id);
        let writer = crate::pkg::daily_jsonl::DailyJsonlWriter::new(agent_dir);
        let (date, line_number) = writer.append(trace)?;
        Ok(MemoryTracePosition {
            trace_id: trace.id.clone(),
            date_filename: format!("{date}.jsonl"),
            line_number: line_number as u64,
        })
    }

    async fn batch_append_traces(
        &self,
        _ctx: RequestContext,
        traces: &[MemoryTrace],
    ) -> Result<Vec<MemoryTracePosition>> {
        if traces.is_empty() {
            return Ok(Vec::new());
        }
        let agent_dir = self.agent_memory_dir(&traces[0].agent_id);
        let writer = crate::pkg::daily_jsonl::DailyJsonlWriter::new(agent_dir);
        // 一次数行 + 一次开文件：避免逐条 append 把成本放大成 O(n × 当日行数)
        let (date, first_line_number) = writer.append_batch(traces)?;
        let mut positions = Vec::with_capacity(traces.len());
        for (offset, trace) in traces.iter().enumerate() {
            let line_number = first_line_number + offset;
            positions.push(MemoryTracePosition {
                trace_id: trace.id.clone(),
                date_filename: format!("{date}.jsonl"),
                line_number: line_number as u64,
            });
        }
        Ok(positions)
    }

    async fn create_short_term_index(
        &self,
        ctx: RequestContext,
        index: ShortTermMemoryIndexPo,
    ) -> Result<()> {
        let pool = self.pool(ctx);
        let status_i32 = index.status as i32;
        sqlx::query!(
            r#"
INSERT INTO short_term_memory_index (
    id, agent_id, task_id, role, summary, tags, trace_ids, status, created_at, updated_at
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
"#,
            index.id,
            index.agent_id,
            index.task_id,
            index.role,
            index.summary,
            index.tags,
            index.trace_ids,
            status_i32,
            index.created_at,
            index.updated_at
        )
        .execute(&pool)
        .await?;
        Ok(())
    }

    async fn update_short_term_index(
        &self,
        ctx: RequestContext,
        index: ShortTermMemoryIndexPo,
    ) -> Result<()> {
        let pool = self.pool(ctx);
        let status_i32 = index.status as i32;
        let result = sqlx::query!(
            r#"
UPDATE short_term_memory_index
SET agent_id = ?,
    task_id = ?,
    role = ?,
    summary = ?,
    tags = ?,
    trace_ids = ?,
    status = ?,
    updated_at = ?
WHERE id = ?
"#,
            index.agent_id,
            index.task_id,
            index.role,
            index.summary,
            index.tags,
            index.trace_ids,
            status_i32,
            index.updated_at,
            index.id,
        )
        .execute(&pool)
        .await?;

        if result.rows_affected() == 0 {
            bail_err!(
                ResourceNotFound,
                "short_term_memory_index id={} not found",
                index.id
            );
        }
        Ok(())
    }

    async fn forget_short_term_index(&self, ctx: RequestContext, id: &str) -> Result<()> {
        use common::enums::MemoryStatus;
        let pool = self.pool(ctx);
        let now = chrono::Utc::now().timestamp();
        let status_i32 = MemoryStatus::Forgotten as i32;
        // 软删除：标记为已遗忘，保留数据可恢复
        sqlx::query!(
            r#"
UPDATE short_term_memory_index
SET status = ?, updated_at = ?
WHERE id = ?
"#,
            status_i32,
            now,
            id
        )
        .execute(&pool)
        .await?;

        Ok(())
    }

    async fn get_short_term_index(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<ShortTermMemoryIndexPo>> {
        use common::enums::MemoryStatus;
        let pool = self.pool(ctx);
        let index = sqlx::query_as!(
            ShortTermMemoryIndexPo,
            r#"
SELECT id, agent_id, task_id, role, summary, tags, trace_ids, status AS "status: MemoryStatus", created_at, updated_at
FROM short_term_memory_index
WHERE id = ? AND status != 0
"#,
            id
        )
        .fetch_optional(&pool)
        .await?;

        Ok(index)
    }

    async fn list_short_term_by_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        limit: usize,
    ) -> Result<Vec<ShortTermMemoryIndexPo>> {
        self.query_short_term(
            ctx,
            MemoryQuery {
                agent_id: Some(agent_id.to_string()),
                limit: Some(limit),
                ..Default::default()
            },
        )
        .await
    }

    async fn query_short_term(
        &self,
        ctx: RequestContext,
        query: MemoryQuery,
    ) -> Result<Vec<ShortTermMemoryIndexPo>> {
        use sqlx::QueryBuilder;

        let pool = self.pool(ctx);
        let mut builder = QueryBuilder::new(
            r#"SELECT id, agent_id, task_id, role, summary, tags, trace_ids, status, created_at, updated_at
FROM short_term_memory_index WHERE 1=1"#,
        );

        if let Some(ids) = &query.ids {
            builder.push(" AND id IN (");
            let mut separated = builder.separated(", ");
            for id in ids {
                separated.push_bind(id);
            }
            separated.push_unseparated(")");
        }

        if let Some(agent_id) = &query.agent_id
            && !agent_id.is_empty()
        {
            builder.push(" AND agent_id = ");
            builder.push_bind(agent_id);
        }

        if let Some(status) = &query.status {
            let status_i32 = *status as i32;
            builder.push(" AND status = ");
            builder.push_bind(status_i32);
        }

        if let Some(exclude_status) = &query.exclude_status {
            let exclude_i32 = *exclude_status as i32;
            builder.push(" AND status != ");
            builder.push_bind(exclude_i32);
        } else if query.status.is_none() {
            // 只有当没有明确指定 status 时，才默认排除 Forgotten（0）
            builder.push(" AND status != 0");
        }

        if let Some(keyword) = &query.keyword
            && !keyword.is_empty()
        {
            log_warn!(
                "keyword in query_short_term is deprecated, use search_short_term for FTS5 full-text search; keyword ignored"
            );
        }

        // tag 过滤（OR 语义：包含任一 tag 即可命中）
        if let Some(tags) = &query.tags
            && !tags.is_empty()
        {
            builder.push(" AND EXISTS (SELECT 1 FROM json_each(tags) WHERE json_each.value IN (");
            let mut separated = builder.separated(", ");
            for tag in tags {
                separated.push_bind(tag);
            }
            separated.push_unseparated("))");
        }

        // task_id 过滤（注意力机制：聚焦到特定任务的记忆）
        if let Some(task_id) = &query.task_id {
            builder.push(" AND task_id = ");
            builder.push_bind(task_id);
        }

        // 排序方向：默认最近优先（历史行为），队列类查询显式指定最早优先
        // 排序方向：默认最近优先（历史行为），队列类查询显式指定最早优先
        builder.push(query.order.to_sql());
        // `created_at` 毫秒级可能同值，补 `id` 作次序键：分页（OFFSET）要求排序确定，
        // 否则同值行在不同页查询间相对次序可能变化，导致漏行。
        builder.push(", id ASC");

        // SQLite 语法要求 OFFSET 必须跟在 LIMIT 之后；只给 offset 时用 `LIMIT -1`
        // 表达「不限上限」。两者都不给则维持原有的「无分页子句」行为。
        if query.limit.is_some() || query.offset.is_some() {
            builder.push(" LIMIT ");
            builder.push_bind(query.limit.map(|l| l as i64).unwrap_or(-1));
            if let Some(offset) = query.offset {
                builder.push(" OFFSET ");
                builder.push_bind(offset as i64);
            }
        }

        let indexes = builder
            .build_query_as::<ShortTermMemoryIndexPo>()
            .fetch_all(&pool)
            .await?;

        Ok(indexes)
    }

    async fn search_short_term(
        &self,
        ctx: RequestContext,
        search: MemorySearch,
    ) -> Result<Vec<(ShortTermMemoryIndexPo, Option<f32>)>> {
        let pool = self.pool(ctx);

        // 从 MemorySearch 提取参数
        let agent_id = search.filters.agent_id.unwrap_or_default();
        let keyword = search.keyword.unwrap_or_default();
        let limit_i64 = search.filters.limit.unwrap_or(50) as i64;
        let tags = search.filters.tags.clone().unwrap_or_default();
        let task_id = search.filters.task_id.clone();

        // 搜索计划：词元按长度路由（>=3 字符走 MATCH 短语 OR 组合，短词元走 LIKE 兜底）
        let Some(plan) = build_fts5_search_plan(&keyword) else {
            return Ok(Vec::new());
        };

        // 构建带可选 tags / task_id / agent_id 过滤的 SQL（MATCH / LIKE 两条路径共用）
        let has_tags = !tags.is_empty();
        let tags_clause = if has_tags {
            let placeholders = tags.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            format!(
                " AND EXISTS (SELECT 1 FROM json_each(m.tags) WHERE json_each.value IN ({placeholders}))"
            )
        } else {
            String::new()
        };
        let has_task_id = task_id.is_some();
        let task_id_clause = if has_task_id {
            " AND m.task_id = ?".to_string()
        } else {
            String::new()
        };
        // agent_id 为空时不过滤（全局搜索）
        let has_agent_filter = !agent_id.is_empty();
        let agent_clause = if has_agent_filter {
            " AND m.agent_id = ?"
        } else {
            ""
        };

        // 行 → 结果元组映射（两条路径共用）
        let to_result = |row: ShortTermSearchRow| {
            let po = ShortTermMemoryIndexPo {
                id: row.id,
                agent_id: row.agent_id,
                task_id: row.task_id,
                role: row.role,
                summary: row.summary,
                tags: row.tags,
                trace_ids: row.trace_ids,
                status: row.status,
                created_at: row.created_at,
                updated_at: row.updated_at,
            };
            (po, row.fts_rank)
        };

        let mut primary: Vec<(ShortTermMemoryIndexPo, Option<f32>)> = Vec::new();
        let mut secondary: Vec<(ShortTermMemoryIndexPo, Option<f32>)> = Vec::new();

        // 路径一：MATCH 短语 OR 组合（多关键词任一命中即可），BM25 rank 排序
        if let Some(match_expr) = &plan.match_expr {
            let sql = format!(
                r#"
SELECT m.id, m.agent_id, m.task_id, m.role, m.summary, m.tags, m.trace_ids,
       m.status, m.created_at, m.updated_at,
       short_term_memory_fts.rank as fts_rank
FROM short_term_memory_fts
JOIN short_term_memory_index m ON short_term_memory_fts.rowid = m.rowid
WHERE short_term_memory_fts MATCH ?
  AND m.status != 0{agent_clause}{tags_clause}{task_id_clause}
ORDER BY short_term_memory_fts.rank
LIMIT ?
"#
            );

            let mut query = sqlx::query_as::<_, ShortTermSearchRow>(&sql).bind(match_expr);

            // 绑定 agent_id 参数（如果有）
            if has_agent_filter {
                query = query.bind(&agent_id);
            }

            // 绑定 tags 参数（如果有）
            if has_tags {
                for tag in &tags {
                    query = query.bind(tag);
                }
            }

            // 绑定 task_id 参数（如果有）
            if let Some(tid) = &task_id {
                query = query.bind(tid);
            }

            let rows: Vec<ShortTermSearchRow> = query.bind(limit_i64).fetch_all(&pool).await?;
            primary.extend(rows.into_iter().map(to_result));
        }

        // 路径二：短词元（<3 字符，trigram 无法形成 token）LIKE 兜底，
        // 直接查主表列，BM25 不可用，改按 updated_at 排序
        if !plan.like_patterns.is_empty()
            && let Some((like_clause, like_params)) =
                build_fts5_like_clause("m", &["summary", "tags"], &plan.like_patterns)
        {
            let sql = format!(
                r#"
SELECT m.id, m.agent_id, m.task_id, m.role, m.summary, m.tags, m.trace_ids,
       m.status, m.created_at, m.updated_at,
       NULL as fts_rank
FROM short_term_memory_index m
WHERE ({like_clause})
  AND m.status != 0{agent_clause}{tags_clause}{task_id_clause}
ORDER BY m.updated_at DESC, m.id DESC
LIMIT ?
"#
            );

            let mut query = sqlx::query_as::<_, ShortTermSearchRow>(&sql);
            for pattern in &like_params {
                query = query.bind(pattern);
            }
            if has_agent_filter {
                query = query.bind(&agent_id);
            }
            if has_tags {
                for tag in &tags {
                    query = query.bind(tag);
                }
            }
            if let Some(tid) = &task_id {
                query = query.bind(tid);
            }
            let rows: Vec<ShortTermSearchRow> = query.bind(limit_i64).fetch_all(&pool).await?;
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

    fn read_memory_content(&self, _index: &ShortTermMemoryIndexPo) -> Result<String> {
        // 原始文件读取由上层业务处理，这里直接返回空字符串占位
        Ok(String::new())
    }

    // ========== 长期知识图谱相关 ==========

    async fn save_knowledge_node(
        &self,
        ctx: RequestContext,
        node: &LongTermKnowledgeNodePo,
    ) -> Result<()> {
        let pool = self.pool(ctx);

        // 先试试更新，如果不存在就插入
        // 写入侧单点归一：node_type 落库前转为规范形（trim + 小写），
        // 读取侧直接裸列比对，无需 SQL 内再归一
        let node_type = normalize(&node.node_type);
        let status_i32 = node.status as i32;
        let is_published_i64 = node.is_published as i64;
        let result: sqlx::Result<sqlx::sqlite::SqliteQueryResult> = sqlx::query!(
            r#"
UPDATE long_term_knowledge_node
SET agent_id = ?,
    node_name = ?,
    node_description = ?,
    node_type = ?,
    summary = ?,
    tags = ?,
    status = ?,
    is_published = ?,
    updated_at = ?
WHERE id = ?
"#,
            node.agent_id,
            node.node_name,
            node.node_description,
            node_type,
            node.summary,
            node.tags,
            status_i32,
            is_published_i64,
            node.updated_at,
            node.id,
        )
        .execute(&pool)
        .await;
        let result = result?;
        let rows_affected = result.rows_affected();

        if rows_affected == 0 {
            // 不存在，插入新节点
            // 11 Rust parameters → 11 question marks (all non-Option)

            let status_i32 = node.status as i32;
            let is_published_i64 = node.is_published as i64;
            sqlx::query!(
                r#"
INSERT INTO long_term_knowledge_node (
    id, agent_id, node_name, node_description, node_type, summary, tags, status, is_published, created_at, updated_at
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
"#,
                node.id,
                node.agent_id,
                node.node_name,
                node.node_description,
                node_type,
                node.summary,
                node.tags,
                status_i32,
                is_published_i64,
                node.created_at,
                node.updated_at
            )
            .execute(&pool)
            .await?;
        }

        Ok(())
    }

    async fn update_knowledge_node(
        &self,
        ctx: RequestContext,
        node: &LongTermKnowledgeNodePo,
    ) -> Result<()> {
        let pool = self.pool(ctx);
        // 写入侧单点归一（与 save_knowledge_node 同口径）
        let node_type = normalize(&node.node_type);
        let status_i32 = node.status as i32;
        let is_published_i64 = node.is_published as i64;
        let result = sqlx::query!(
            r#"
UPDATE long_term_knowledge_node
SET agent_id = ?,
    node_name = ?,
    node_description = ?,
    node_type = ?,
    summary = ?,
    tags = ?,
    status = ?,
    is_published = ?,
    updated_at = ?
WHERE id = ?
"#,
            node.agent_id,
            node.node_name,
            node.node_description,
            node_type,
            node.summary,
            node.tags,
            status_i32,
            is_published_i64,
            node.updated_at,
            node.id,
        )
        .execute(&pool)
        .await?;

        if result.rows_affected() == 0 {
            bail_err!(
                ResourceNotFound,
                "long_term_knowledge_node id={} not found",
                node.id
            );
        }
        Ok(())
    }

    async fn batch_save_knowledge_nodes(
        &self,
        ctx: RequestContext,
        nodes: &[LongTermKnowledgeNodePo],
    ) -> Result<()> {
        let pool = self.pool(ctx);
        let mut tx = pool.begin().await?;

        for node in nodes {
            // 写入侧单点归一（与 save_knowledge_node 同口径）
            let node_type = normalize(&node.node_type);
            let status_i32 = node.status as i32;
            let is_published_i64 = node.is_published as i64;
            let result: sqlx::Result<sqlx::sqlite::SqliteQueryResult> = sqlx::query!(
                r#"
UPDATE long_term_knowledge_node
SET agent_id = ?,
    node_name = ?,
    node_description = ?,
    node_type = ?,
    summary = ?,
    tags = ?,
    status = ?,
    is_published = ?,
    updated_at = ?
WHERE id = ?
"#,
                node.agent_id,
                node.node_name,
                node.node_description,
                node_type,
                node.summary,
                node.tags,
                status_i32,
                is_published_i64,
                node.updated_at,
                node.id,
            )
            .execute(&mut *tx)
            .await;
            let result = result?;
            let rows_affected = result.rows_affected();

            if rows_affected == 0 {
                // 不存在，插入新节点
                // 11 Rust parameters → 11 question marks (all non-Option)

                let status_i32 = node.status as i32;
                let is_published_i64 = node.is_published as i64;
                sqlx::query!(
                    r#"
INSERT INTO long_term_knowledge_node (
    id, agent_id, node_name, node_description, node_type, summary, tags, status, is_published, created_at, updated_at
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
"#,
                    node.id,
                    node.agent_id,
                    node.node_name,
                    node.node_description,
                    node_type,
                    node.summary,
                    node.tags,
                    status_i32,
                    is_published_i64,
                    node.created_at,
                    node.updated_at
                )
                .execute(&mut *tx)
                .await?;
            }
        }

        tx.commit().await?;
        Ok(())
    }

    async fn get_knowledge_node(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<LongTermKnowledgeNodePo>> {
        use common::enums::MemoryStatus;
        let pool = self.pool(ctx);
        let node = sqlx::query_as!(
            LongTermKnowledgeNodePo,
            r#"
SELECT id, agent_id, node_name, node_description, node_type, summary, tags, status AS "status: MemoryStatus", is_published AS "is_published: bool", created_at, updated_at
FROM long_term_knowledge_node
WHERE id = ? AND status != 0
"#,
            id
        )
        .fetch_optional(&pool)
        .await?;

        Ok(node)
    }

    async fn list_knowledge_nodes_by_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        node_type: Option<&str>,
        limit: usize,
    ) -> Result<Vec<LongTermKnowledgeNodePo>> {
        // 与 API 侧同口径：非法类型值报错，不静默放行全量。
        let memory_type = match node_type.map(MemoryType::parse) {
            Some(Some(memory_type)) => Some(memory_type),
            Some(None) => bail_err!(
                InvalidRequest,
                "invalid node_type: expected one of short_term/knowledge_node/trace/relation/all"
            ),
            None => None,
        };

        self.query_knowledge_nodes(
            ctx,
            MemoryQuery {
                agent_id: Some(agent_id.to_string()),
                memory_type,
                limit: Some(limit),
                ..Default::default()
            },
        )
        .await
    }

    async fn query_knowledge_nodes(
        &self,
        ctx: RequestContext,
        query: MemoryQuery,
    ) -> Result<Vec<LongTermKnowledgeNodePo>> {
        use sqlx::QueryBuilder;

        let pool = self.pool(ctx);
        let mut builder = QueryBuilder::new(
            r#"SELECT id, agent_id, node_name, node_description, node_type, summary, tags, status, is_published, created_at, updated_at
FROM long_term_knowledge_node WHERE 1=1"#,
        );

        if let Some(ids) = &query.ids {
            builder.push(" AND id IN (");
            let mut separated = builder.separated(", ");
            for id in ids {
                separated.push_bind(id);
            }
            separated.push_unseparated(")");
        }

        // 归属筛选：**显式**指定才过滤（空串 = None = 不过滤）。
        // ⚠️ 知识节点是蜂巢共享资产：默认不施加任何归属门槛，任何 Agent 都能读到全部节点，
        // 否则「全局视图」在私有节点多的库上会退化成空图 / 只剩边没有节点。
        // `published` 只是「重要性 / 影响力」标记，**不是**可见性控制位，查询侧不再读它。
        // 注意：sqlx QueryBuilder 的 push() 不会识别 ? 占位符，必须用 push_bind() 绑定参数
        if let Some(agent_id) = query.agent_id.clone().filter(|s| !s.is_empty()) {
            builder.push(" AND agent_id = ");
            builder.push_bind(agent_id);
        }

        if let Some(status) = &query.status {
            let status_i32 = *status as i32;
            builder.push(" AND status = ");
            builder.push_bind(status_i32);
        }

        if let Some(exclude_status) = &query.exclude_status {
            let exclude_i32 = *exclude_status as i32;
            builder.push(" AND status != ");
            builder.push_bind(exclude_i32);
        } else if query.status.is_none() {
            // 只有当没有明确指定 status 时，才默认排除 Forgotten（0）
            builder.push(" AND status != 0");
        }

        if let Some(node_type) = &query.node_type {
            builder.push(" AND node_type = ");
            builder.push_bind(node_type);
        }

        if let Some(keyword) = &query.keyword
            && !keyword.is_empty()
        {
            log_warn!(
                "keyword in query_knowledge_nodes is deprecated, use search_knowledge_nodes for FTS5 full-text search; keyword ignored"
            );
        }

        // tag 过滤（OR 语义：包含任一 tag 即可命中）
        if let Some(tags) = &query.tags
            && !tags.is_empty()
        {
            builder.push(" AND EXISTS (SELECT 1 FROM json_each(tags) WHERE json_each.value IN (");
            let mut separated = builder.separated(", ");
            for tag in tags {
                separated.push_bind(tag);
            }
            separated.push_unseparated("))");
        }

        // `updated_at` 毫秒级可能同值（批量写入常见），补 `id` 作次序键：
        // 分页（OFFSET）要求排序确定，否则同值行在不同页查询间相对次序可能变化，导致漏行。
        builder.push(" ORDER BY updated_at DESC, id DESC");

        // SQLite 语法要求 OFFSET 必须跟在 LIMIT 之后；只给 offset 时用 `LIMIT -1`
        // 表达「不限上限」。两者都不给则维持原有的「无分页子句」行为。
        if query.limit.is_some() || query.offset.is_some() {
            builder.push(" LIMIT ");
            builder.push_bind(query.limit.map(|l| l as i64).unwrap_or(-1));
            if let Some(offset) = query.offset {
                builder.push(" OFFSET ");
                builder.push_bind(offset as i64);
            }
        }

        let nodes = builder
            .build_query_as::<LongTermKnowledgeNodePo>()
            .fetch_all(&pool)
            .await?;

        Ok(nodes)
    }

    async fn search_knowledge_nodes(
        &self,
        ctx: RequestContext,
        search: MemorySearch,
    ) -> Result<Vec<(LongTermKnowledgeNodePo, Option<f32>)>> {
        let pool = self.pool(ctx);

        // 从 MemorySearch 提取参数
        let agent_filter = search.filters.agent_id.clone().filter(|s| !s.is_empty());
        let keyword = search.keyword.unwrap_or_default();
        let limit_i64 = search.filters.limit.unwrap_or(50) as i64;
        let tags = search.filters.tags.clone().unwrap_or_default();

        // 搜索计划：词元按长度路由（>=3 字符走 MATCH 短语 OR 组合，短词元走 LIKE 兜底）
        let Some(plan) = build_fts5_search_plan(&keyword) else {
            return Ok(Vec::new());
        };

        // 构建带可选 tags 过滤的 SQL（MATCH / LIKE 两条路径共用）
        let has_tags = !tags.is_empty();
        let tags_clause = if has_tags {
            let placeholders = tags.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            format!(
                " AND EXISTS (SELECT 1 FROM json_each(m.tags) WHERE json_each.value IN ({placeholders}))"
            )
        } else {
            String::new()
        };

        // 归属筛选：只有调用方**显式**指定 agent_id 才过滤（空串 = None）。
        // ⚠️ 知识节点是蜂巢共享资产：默认全域可搜，`published` 不再是可见性门槛。
        let ownership_clause = if agent_filter.is_some() {
            "m.agent_id = ?"
        } else {
            "1=1"
        };

        // 行 → 结果元组映射（两条路径共用）
        let to_result = |row: KnowledgeNodeSearchRow| {
            let po = LongTermKnowledgeNodePo {
                id: row.id,
                agent_id: row.agent_id,
                node_name: row.node_name,
                node_description: row.node_description,
                node_type: row.node_type,
                summary: row.summary,
                tags: row.tags,
                status: row.status,
                is_published: row.is_published,
                created_at: row.created_at,
                updated_at: row.updated_at,
            };
            (po, row.fts_rank)
        };

        let mut primary: Vec<(LongTermKnowledgeNodePo, Option<f32>)> = Vec::new();
        let mut secondary: Vec<(LongTermKnowledgeNodePo, Option<f32>)> = Vec::new();

        // 路径一：MATCH 短语 OR 组合（多关键词任一命中即可），BM25 rank 排序
        if let Some(match_expr) = &plan.match_expr {
            let sql = format!(
                r#"
SELECT m.id, m.agent_id, m.node_name, m.node_description, m.node_type, m.summary, m.tags,
       m.status, m.is_published, m.created_at, m.updated_at,
       knowledge_node_fts.rank as fts_rank
FROM knowledge_node_fts
JOIN long_term_knowledge_node m ON knowledge_node_fts.rowid = m.rowid
WHERE knowledge_node_fts MATCH ?
  AND {ownership_clause}
  AND m.status != 0{tags_clause}
ORDER BY knowledge_node_fts.rank
LIMIT ?
"#
            );

            let mut query = sqlx::query_as::<_, KnowledgeNodeSearchRow>(&sql).bind(match_expr);

            // 绑定 agent 归属筛选参数（仅当调用方显式指定时）
            if let Some(agent_id) = &agent_filter {
                query = query.bind(agent_id);
            }

            // 绑定 tags 参数（如果有）
            if has_tags {
                for tag in &tags {
                    query = query.bind(tag);
                }
            }

            let rows: Vec<KnowledgeNodeSearchRow> = query.bind(limit_i64).fetch_all(&pool).await?;
            primary.extend(rows.into_iter().map(to_result));
        }

        // 路径二：短词元（<3 字符，trigram 无法形成 token）LIKE 兜底，
        // 直接查主表列，BM25 不可用，改按 updated_at 排序
        if !plan.like_patterns.is_empty()
            && let Some((like_clause, like_params)) = build_fts5_like_clause(
                "m",
                &["node_name", "summary", "node_description", "tags"],
                &plan.like_patterns,
            )
        {
            let sql = format!(
                r#"
SELECT m.id, m.agent_id, m.node_name, m.node_description, m.node_type, m.summary, m.tags,
       m.status, m.is_published, m.created_at, m.updated_at,
       NULL as fts_rank
FROM long_term_knowledge_node m
WHERE ({like_clause})
  AND {ownership_clause}
  AND m.status != 0{tags_clause}
ORDER BY m.updated_at DESC, m.id DESC
LIMIT ?
"#
            );

            let mut query = sqlx::query_as::<_, KnowledgeNodeSearchRow>(&sql);
            for pattern in &like_params {
                query = query.bind(pattern);
            }
            if let Some(agent_id) = &agent_filter {
                query = query.bind(agent_id);
            }
            if has_tags {
                for tag in &tags {
                    query = query.bind(tag);
                }
            }
            let rows: Vec<KnowledgeNodeSearchRow> = query.bind(limit_i64).fetch_all(&pool).await?;
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

    async fn delete_knowledge_node(&self, ctx: RequestContext, id: &str) -> Result<()> {
        use common::enums::MemoryStatus;
        let pool = self.pool(ctx);
        let now = chrono::Utc::now().timestamp();
        let status_i32 = MemoryStatus::Forgotten as i32;
        // 软删除：标记为已遗忘，保留数据可恢复
        sqlx::query!(
            r#"
UPDATE long_term_knowledge_node
SET status = ?, updated_at = ?
WHERE id = ?
"#,
            status_i32,
            now,
            id
        )
        .execute(&pool)
        .await?;

        Ok(())
    }

    async fn add_knowledge_reference(
        &self,
        ctx: RequestContext,
        reference: &KnowledgeReferencePo,
    ) -> Result<()> {
        let pool = self.pool(ctx);

        sqlx::query!(
            r#"
INSERT INTO knowledge_reference (
    id, knowledge_id, short_term_id, trace_id, date_path, line_number, created_at
) VALUES (?, ?, ?, ?, ?, ?, ?)
"#,
            reference.id,
            reference.knowledge_id,
            reference.short_term_id,
            reference.trace_id,
            reference.date_path,
            reference.line_number,
            reference.created_at,
        )
        .execute(&pool)
        .await?;

        Ok(())
    }

    async fn batch_add_knowledge_references(
        &self,
        ctx: RequestContext,
        references: &[KnowledgeReferencePo],
    ) -> Result<()> {
        let pool = self.pool(ctx);
        let mut tx = pool.begin().await?;

        for reference in references {
            sqlx::query!(
                r#"
INSERT INTO knowledge_reference (
    id, knowledge_id, short_term_id, trace_id, date_path, line_number, created_at
) VALUES (?, ?, ?, ?, ?, ?, ?)
"#,
                reference.id,
                reference.knowledge_id,
                reference.short_term_id,
                reference.trace_id,
                reference.date_path,
                reference.line_number,
                reference.created_at,
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn list_knowledge_references(
        &self,
        ctx: RequestContext,
        knowledge_id: &str,
    ) -> Result<Vec<KnowledgeReferencePo>> {
        let pool = self.pool(ctx);
        let references = sqlx::query_as!(
            KnowledgeReferencePo,
            r#"
SELECT id, knowledge_id, short_term_id, trace_id, date_path, line_number, created_at
FROM knowledge_reference
WHERE knowledge_id = ?
ORDER BY created_at ASC
"#,
            knowledge_id
        )
        .fetch_all(&pool)
        .await?;

        Ok(references)
    }

    // ========== 知识节点关系相关 ==========

    async fn add_knowledge_relation(
        &self,
        ctx: RequestContext,
        relation: &KnowledgeNodeRelationPo,
    ) -> Result<()> {
        let pool = self.pool(ctx);
        let mut tx = pool.begin().await?;

        // 同键旧生效边先降级：部分唯一索引保证 (source, target, type) 仅一条生效，
        // 新版本插入前必须先让位；两步同事务，保证不出现"零生效边"的真空期
        Self::supersede_active_edge(&mut tx, relation).await?;

        // 写入侧单点归一 + 与节点写入同惯例：枚举先落成 i32 再绑定
        let status_i32 = relation.status as i32;
        let relation_type = normalize(&relation.relation_type);
        sqlx::query!(
            r#"
INSERT INTO knowledge_node_relation (
    id, source_node_id, target_node_id, relation_type, weight, "status", created_at, updated_at
) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
"#,
            relation.id,
            relation.source_node_id,
            relation.target_node_id,
            // 归一后落库：词表外的标注不会丢，且与读取侧裸列比对同口径
            relation_type,
            relation.weight,
            status_i32,
            relation.created_at,
            relation.updated_at,
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn batch_add_knowledge_relations(
        &self,
        ctx: RequestContext,
        relations: &[KnowledgeNodeRelationPo],
    ) -> Result<()> {
        let pool = self.pool(ctx);
        let mut tx = pool.begin().await?;

        for relation in relations {
            // 批内同样先降级同键旧生效边再插入，与单条路径语义一致
            Self::supersede_active_edge(&mut tx, relation).await?;

            let status_i32 = relation.status as i32;
            let relation_type = normalize(&relation.relation_type);
            sqlx::query!(
                r#"
INSERT INTO knowledge_node_relation (
    id, source_node_id, target_node_id, relation_type, weight, "status", created_at, updated_at
) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
"#,
                relation.id,
                relation.source_node_id,
                relation.target_node_id,
                relation_type,
                relation.weight,
                status_i32,
                relation.created_at,
                relation.updated_at,
            )
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn upsert_knowledge_relation(
        &self,
        ctx: RequestContext,
        relation: &KnowledgeNodeRelationPo,
    ) -> Result<()> {
        let pool = self.pool(ctx);
        let mut tx = pool.begin().await?;

        Self::supersede_active_edge(&mut tx, relation).await?;

        let status_i32 = relation.status as i32;
        let relation_type = normalize(&relation.relation_type);
        sqlx::query!(
            r#"
INSERT INTO knowledge_node_relation (
    id, source_node_id, target_node_id, relation_type, weight, "status", created_at, updated_at
) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
ON CONFLICT(id) DO UPDATE SET
    source_node_id = excluded.source_node_id,
    target_node_id = excluded.target_node_id,
    relation_type  = excluded.relation_type,
    weight         = excluded.weight,
    "status"       = excluded."status",
    updated_at     = excluded.updated_at
"#,
            relation.id,
            relation.source_node_id,
            relation.target_node_id,
            relation_type,
            relation.weight,
            status_i32,
            relation.created_at,
            relation.updated_at,
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn list_outgoing_relations(
        &self,
        ctx: RequestContext,
        source_id: &str,
    ) -> Result<Vec<KnowledgeNodeRelationPo>> {
        let pool = self.pool(ctx);
        let rows = sqlx::query!(
            r#"
SELECT id, source_node_id, target_node_id, relation_type, weight, "status" as "status: KnowledgeRelationStatus", created_at, updated_at
FROM knowledge_node_relation
WHERE source_node_id = ? AND "status" = 1
ORDER BY created_at ASC
"#,
            source_id
        )
        .fetch_all(&pool)
        .await?;

        let mut result = Vec::new();
        for row in rows {
            result.push(KnowledgeNodeRelationPo {
                id: row.id,
                source_node_id: row.source_node_id,
                target_node_id: row.target_node_id,
                // 关系类型原样带出：落库存的已是写入侧归一后的规范形
                relation_type: row.relation_type,
                // SQLite REAL 映射为 f64，PO 统一用 f32（与 API DTO 的 f32 对齐）
                weight: row.weight.map(|w| w as f32),
                status: row.status,
                created_at: row.created_at,
                updated_at: row.updated_at,
            });
        }

        Ok(result)
    }

    async fn list_incoming_relations(
        &self,
        ctx: RequestContext,
        target_id: &str,
    ) -> Result<Vec<KnowledgeNodeRelationPo>> {
        let pool = self.pool(ctx);
        let rows = sqlx::query!(
            r#"
SELECT id, source_node_id, target_node_id, relation_type, weight, "status" as "status: KnowledgeRelationStatus", created_at, updated_at
FROM knowledge_node_relation
WHERE target_node_id = ? AND "status" = 1
ORDER BY created_at ASC
"#,
            target_id
        )
        .fetch_all(&pool)
        .await?;

        let mut result = Vec::new();
        for row in rows {
            result.push(KnowledgeNodeRelationPo {
                id: row.id,
                source_node_id: row.source_node_id,
                target_node_id: row.target_node_id,
                // 关系类型原样带出：落库存的是原文，读取侧不做任何枚举归一
                relation_type: row.relation_type,
                // SQLite REAL 映射为 f64，PO 统一用 f32（与 API DTO 的 f32 对齐）
                weight: row.weight.map(|w| w as f32),
                status: row.status,
                created_at: row.created_at,
                updated_at: row.updated_at,
            });
        }

        Ok(result)
    }

    async fn list_all_relations_for_node(
        &self,
        ctx: RequestContext,
        node_id: &str,
    ) -> Result<Vec<KnowledgeNodeRelationPo>> {
        let pool = self.pool(ctx);
        let rows = sqlx::query!(
            r#"
SELECT id, source_node_id, target_node_id, relation_type, weight, "status" as "status: KnowledgeRelationStatus", created_at, updated_at
FROM knowledge_node_relation
WHERE (source_node_id = ? OR target_node_id = ?) AND "status" = 1
ORDER BY created_at ASC
"#,
            node_id,
            node_id
        )
        .fetch_all(&pool)
        .await?;

        let mut result = Vec::new();
        for row in rows {
            result.push(KnowledgeNodeRelationPo {
                id: row.id,
                source_node_id: row.source_node_id,
                target_node_id: row.target_node_id,
                // 关系类型原样带出：落库存的是原文，读取侧不做任何枚举归一
                relation_type: row.relation_type,
                // SQLite REAL 映射为 f64，PO 统一用 f32（与 API DTO 的 f32 对齐）
                weight: row.weight.map(|w| w as f32),
                status: row.status,
                created_at: row.created_at,
                updated_at: row.updated_at,
            });
        }

        Ok(result)
    }

    async fn list_relations_batch(
        &self,
        ctx: RequestContext,
        node_ids: &[String],
    ) -> Result<Vec<KnowledgeNodeRelationPo>> {
        if node_ids.is_empty() {
            return Ok(Vec::new());
        }

        // SQLite 绑定参数上限 999：每 ID 占 2 绑定，按 IN_CLAUSE_CHUNK 分块避免超限
        let mut result = Vec::new();
        for chunk in node_ids.chunks(IN_CLAUSE_CHUNK) {
            result.extend(self.list_relations_batch_chunk(ctx.clone(), chunk).await?);
        }
        // 跨块拼接后重排，保持 created_at ASC 的有序契约；
        // 两端点分属不同块的边会在两块中各命中一次，按 id 去重（稳定排序后重复项相邻）
        result.sort_by_key(|rel| rel.created_at);
        result.dedup_by(|a, b| a.id == b.id);
        Ok(result)
    }

    async fn query_knowledge_relations(
        &self,
        ctx: RequestContext,
        query: MemoryQuery,
    ) -> Result<Vec<KnowledgeNodeRelationPo>> {
        let Some(ids) = query.ids.filter(|ids| !ids.is_empty()) else {
            return Ok(Vec::new());
        };

        // SQLite 绑定参数上限 999：按 id 单绑定 IN 查询，分块复用 IN_CLAUSE_CHUNK
        let mut result = Vec::new();
        for chunk in ids.chunks(IN_CLAUSE_CHUNK) {
            result.extend(
                self.query_relations_by_ids_chunk(ctx.clone(), chunk)
                    .await?,
            );
        }
        Ok(result)
    }

    async fn delete_knowledge_relation(
        &self,
        ctx: RequestContext,
        relation_id: &str,
    ) -> Result<()> {
        let pool = self.pool(ctx);
        let now = chrono::Utc::now().timestamp();
        let status_i32 = KnowledgeRelationStatus::Deleted as i32;
        // 软删除：标记为 Deleted(2)，行保留支持恢复。
        // 仅降级生效边：Superseded 历史边的版本链语义（"被替换"）不可被覆盖成"被删除"
        sqlx::query!(
            r#"
UPDATE knowledge_node_relation
SET "status" = ?, updated_at = ?
WHERE id = ? AND "status" = 1
"#,
            status_i32,
            now,
            relation_id
        )
        .execute(&pool)
        .await?;

        Ok(())
    }

    async fn delete_all_relations_for_node(
        &self,
        ctx: RequestContext,
        node_id: &str,
    ) -> Result<()> {
        let pool = self.pool(ctx);
        let mut tx = pool.begin().await?;

        // 删除所有源节点为该节点的关系
        sqlx::query!(
            r#"DELETE FROM knowledge_node_relation WHERE source_node_id = ?"#,
            node_id
        )
        .execute(&mut *tx)
        .await?;

        // 删除所有目标节点为该节点的关系
        sqlx::query!(
            r#"DELETE FROM knowledge_node_relation WHERE target_node_id = ?"#,
            node_id
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn find_relations_by_type(
        &self,
        ctx: RequestContext,
        source_id: &str,
        relation_type: &str,
    ) -> Result<Vec<KnowledgeNodeRelationPo>> {
        let pool = self.pool(ctx);
        let rows = sqlx::query!(
            r#"
SELECT id, source_node_id, target_node_id, relation_type, weight, "status" as "status: KnowledgeRelationStatus", created_at, updated_at
FROM knowledge_node_relation
WHERE source_node_id = ? AND relation_type = ? AND "status" = 1
ORDER BY created_at ASC
"#,
            source_id,
            relation_type
        )
        .fetch_all(&pool)
        .await?;

        let mut result = Vec::new();
        for row in rows {
            result.push(KnowledgeNodeRelationPo {
                id: row.id,
                source_node_id: row.source_node_id,
                target_node_id: row.target_node_id,
                // 关系类型原样带出：落库存的是原文，读取侧不做任何枚举归一
                relation_type: row.relation_type,
                // SQLite REAL 映射为 f64，PO 统一用 f32（与 API DTO 的 f32 对齐）
                weight: row.weight.map(|w| w as f32),
                status: row.status,
                created_at: row.created_at,
                updated_at: row.updated_at,
            });
        }

        Ok(result)
    }

    async fn word_freq_relations(
        &self,
        ctx: RequestContext,
        agent_id: Option<String>,
    ) -> Result<Vec<TermFrequencyRow>> {
        let pool = self.pool(ctx);
        // 写入侧已在 DAO 单点归一（normalize），聚合直接走裸列，无需 SQL 内再归一；
        // 关系表无 agent 列，agent_count 按 LEFT JOIN 源节点归属去重（孤儿边计 0）；
        // agent 过滤同口径取源节点归属（None / 空串 = 全组织）
        let mut builder = QueryBuilder::new(
            r#"
SELECT r.relation_type AS term,
       COUNT(*) AS count,
       COUNT(DISTINCT s.agent_id) AS agent_count
FROM knowledge_node_relation r
LEFT JOIN long_term_knowledge_node s ON s.id = r.source_node_id
WHERE r."status" = 1"#,
        );
        if let Some(agent) = agent_id.as_deref().filter(|s| !s.is_empty()) {
            builder.push(" AND s.agent_id = ");
            builder.push_bind(agent.to_string());
        }
        builder
            .push("\nGROUP BY r.relation_type\nORDER BY count DESC, r.relation_type ASC\nLIMIT ");
        builder.push_bind(MAX_WORD_FREQ_ROWS);
        let rows = builder
            .build_query_as::<TermFrequencyRow>()
            .fetch_all(&pool)
            .await?;
        Ok(rows)
    }

    async fn word_freq_nodes(
        &self,
        ctx: RequestContext,
        agent_id: Option<String>,
    ) -> Result<Vec<TermFrequencyRow>> {
        let pool = self.pool(ctx);
        // 节点自带 agent_id，直接 DISTINCT 去重；agent 过滤（None / 空串 = 全组织）
        let mut builder = QueryBuilder::new(
            r#"
SELECT node_type AS term,
       COUNT(*) AS count,
       COUNT(DISTINCT agent_id) AS agent_count
FROM long_term_knowledge_node
WHERE status != 0"#,
        );
        if let Some(agent) = agent_id.as_deref().filter(|s| !s.is_empty()) {
            builder.push(" AND agent_id = ");
            builder.push_bind(agent.to_string());
        }
        builder.push("\nGROUP BY node_type\nORDER BY count DESC, node_type ASC\nLIMIT ");
        builder.push_bind(MAX_WORD_FREQ_ROWS);
        let rows = builder
            .build_query_as::<TermFrequencyRow>()
            .fetch_all(&pool)
            .await?;
        Ok(rows)
    }

    async fn detail_relations_by_term(
        &self,
        ctx: RequestContext,
        raw_term: &str,
        agent_id: Option<String>,
        pagination: PaginationParams,
    ) -> Result<PagedResult<DriftRelationDetailRow>> {
        let pool = self.pool(ctx);
        // 参数入口归一（normalize）后与落库规范形裸列比对；COUNT 与 LIST 共用同一过滤构造
        let total: i64 = {
            let mut builder = QueryBuilder::new(
                r#"SELECT COUNT(*)
FROM knowledge_node_relation r
LEFT JOIN long_term_knowledge_node s ON s.id = r.source_node_id
WHERE r."status" = 1"#,
            );
            push_drift_relation_filters(&mut builder, raw_term, agent_id.as_deref());
            builder.build_query_scalar().fetch_one(&pool).await?
        };
        // SQLite 语义：负 LIMIT = 无上限（LIMIT -1 OFFSET n 合法，兼容单独 offset）
        let limit_i64 = pagination.limit.map(|v| v as i64).unwrap_or(-1);
        let offset_i64 = pagination.offset.unwrap_or(0) as i64;
        let mut builder = QueryBuilder::new(
            r#"SELECT r.id AS id,
       COALESCE(s.agent_id, '') AS agent_id,
       COALESCE(s.node_name, '') AS source_name,
       COALESCE(t.node_name, '') AS target_name,
       r.created_at AS created_at
FROM knowledge_node_relation r
LEFT JOIN long_term_knowledge_node s ON s.id = r.source_node_id
LEFT JOIN long_term_knowledge_node t ON t.id = r.target_node_id
WHERE r."status" = 1"#,
        );
        push_drift_relation_filters(&mut builder, raw_term, agent_id.as_deref());
        builder.push(" ORDER BY r.created_at DESC, r.id DESC LIMIT ");
        builder.push_bind(limit_i64);
        builder.push(" OFFSET ");
        builder.push_bind(offset_i64);
        let items: Vec<DriftRelationDetailRow> = builder.build_query_as().fetch_all(&pool).await?;
        Ok(PagedResult {
            items,
            total: total as usize,
        })
    }

    async fn detail_nodes_by_term(
        &self,
        ctx: RequestContext,
        raw_term: &str,
        agent_id: Option<String>,
        pagination: PaginationParams,
    ) -> Result<PagedResult<DriftClassDetailRow>> {
        let pool = self.pool(ctx);
        let total: i64 = {
            let mut builder = QueryBuilder::new(
                r#"SELECT COUNT(*) FROM long_term_knowledge_node WHERE status != 0"#,
            );
            push_drift_node_filters(&mut builder, raw_term, agent_id.as_deref());
            builder.build_query_scalar().fetch_one(&pool).await?
        };
        let limit_i64 = pagination.limit.map(|v| v as i64).unwrap_or(-1);
        let offset_i64 = pagination.offset.unwrap_or(0) as i64;
        let mut builder = QueryBuilder::new(
            r#"SELECT id, agent_id, node_name AS name, created_at
FROM long_term_knowledge_node
WHERE status != 0"#,
        );
        push_drift_node_filters(&mut builder, raw_term, agent_id.as_deref());
        builder.push(" ORDER BY updated_at DESC, id DESC LIMIT ");
        builder.push_bind(limit_i64);
        builder.push(" OFFSET ");
        builder.push_bind(offset_i64);
        let items: Vec<DriftClassDetailRow> = builder.build_query_as().fetch_all(&pool).await?;
        Ok(PagedResult {
            items,
            total: total as usize,
        })
    }

    async fn publish_nodes_by_type(
        &self,
        ctx: RequestContext,
        node_type_normalized: &str,
    ) -> Result<u64> {
        let pool = self.pool(ctx);
        let now = chrono::Utc::now().timestamp();
        // 入口兜底归一：无论上游传入何种形态，匹配的都是落库规范形
        let node_type = normalize(node_type_normalized);
        // 幂等置位：已置位（is_published = 1）或已遗忘（status = 0）的行不重复计数
        let result = sqlx::query!(
            r#"
UPDATE long_term_knowledge_node
SET is_published = 1, updated_at = ?
WHERE node_type = ? AND is_published = 0 AND status != 0
"#,
            now,
            node_type
        )
        .execute(&pool)
        .await?;
        Ok(result.rows_affected())
    }
}

/// 关系下钻过滤条件（COUNT 与 LIST 共用）：归一化词匹配 + 可选源节点归属过滤
///
/// 参数在入口经 `common::ontology::normalize` 归一，与写入侧落库的规范形
/// 直接裸列比对（DB 中只存规范形，无需 SQL 内再归一）。
fn push_drift_relation_filters(
    builder: &mut QueryBuilder<'_, Sqlite>,
    raw_term: &str,
    agent_id: Option<&str>,
) {
    builder.push(" AND r.relation_type = ");
    builder.push_bind(normalize(raw_term));
    // None / 空串 = 不过滤（历史调用方表达"全局视图"的方式）
    if let Some(agent) = agent_id.filter(|s| !s.is_empty()) {
        builder.push(" AND s.agent_id = ");
        builder.push_bind(agent.to_string());
    }
}

/// 节点下钻过滤条件（COUNT 与 LIST 共用）：归一化词匹配 + 可选 agent 过滤
fn push_drift_node_filters(
    builder: &mut QueryBuilder<'_, Sqlite>,
    raw_term: &str,
    agent_id: Option<&str>,
) {
    builder.push(" AND node_type = ");
    builder.push_bind(normalize(raw_term));
    if let Some(agent) = agent_id.filter(|s| !s.is_empty()) {
        builder.push(" AND agent_id = ");
        builder.push_bind(agent.to_string());
    }
}
