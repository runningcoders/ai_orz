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
use crate::pkg::storage::escape_fts5_keyword;
use crate::service::dao::memory::{MemoryDao, MemoryQuery, MemorySearch};
use async_trait::async_trait;
use common::enums::{KnowledgeRelationType, MemoryStatus, MemoryType};
use common::error::{Result, bail_err};
use serde_json;
use sqlx::{FromRow, SqlitePool};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

/// SQLite 默认绑定参数上限为 999（SQLITE_MAX_VARIABLE_NUMBER）。
/// `list_relations_batch` 每个节点 ID 占 2 个绑定（source + target 两个 IN 列表），
/// 按 400 个节点分块（800 绑定）留出安全余量；ids IN 单绑定场景同样复用此常量。
pub const IN_CLAUSE_CHUNK: usize = 400;

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

/// 判断 tags JSON 数组字符串中是否包含 "published" 标签
///
/// tags 形如 `["rust","published"]`，序列化后 "published" 一定带双引号，
/// 因此用 `contains("\"published\"")` 即可快速判断，避免每次反序列化。
pub(crate) fn tags_has_published(tags: &str) -> bool {
    tags.contains("\"published\"")
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

    /// `list_relations_batch` 的单块查询（调用方保证 chunk 长度 ≤ IN_CLAUSE_CHUNK）
    async fn list_relations_batch_chunk(
        &self,
        ctx: RequestContext,
        node_ids: &[String],
    ) -> Result<Vec<KnowledgeNodeRelationPo>> {
        use sqlx::{QueryBuilder, Row};

        let pool = self.pool(ctx);
        let mut builder = QueryBuilder::new(
            r#"SELECT id, source_node_id, target_node_id, relation_type, created_at, updated_at
FROM knowledge_node_relation
WHERE source_node_id IN ("#,
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
        separated.push_unseparated(") ORDER BY created_at ASC");

        let rows = builder.build().fetch_all(&pool).await?;

        let mut result = Vec::new();
        for row in rows {
            let id: String = row.get("id");
            let source_node_id: String = row.get("source_node_id");
            let target_node_id: String = row.get("target_node_id");
            let relation_type_str: String = row.get("relation_type");
            let created_at: i64 = row.get("created_at");
            let updated_at: i64 = row.get("updated_at");
            let relation_type = KnowledgeRelationType::from(relation_type_str);
            result.push(KnowledgeNodeRelationPo {
                id,
                source_node_id,
                target_node_id,
                relation_type,
                created_at,
                updated_at,
            });
        }

        Ok(result)
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
        let mut positions = Vec::with_capacity(traces.len());
        for trace in traces {
            let (date, line_number) = writer.append(trace)?;
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

        builder.push(" ORDER BY created_at DESC");

        if let Some(limit) = &query.limit {
            builder.push(" LIMIT ");
            builder.push_bind(*limit as i64);
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

        // 空关键词直接返回空结果（FTS5 MATCH 空字符串会报错）
        if keyword.trim().is_empty() {
            return Ok(Vec::new());
        }

        // 转义关键词为 FTS5 短语匹配
        let escaped_keyword = escape_fts5_keyword(&keyword);

        // 构建带可选 tags / task_id / agent_id 过滤的 SQL
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

        let mut query = sqlx::query_as::<_, ShortTermSearchRow>(&sql).bind(escaped_keyword);

        // 绑定 agent_id 参数（如果有）
        if has_agent_filter {
            query = query.bind(agent_id);
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

        let results = rows
            .into_iter()
            .map(|row| {
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
            })
            .collect();

        Ok(results)
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
            node.node_type,
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
                node.node_type,
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
            node.node_type,
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
                node.node_type,
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
                    node.node_type,
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
        self.query_knowledge_nodes(
            ctx,
            MemoryQuery {
                agent_id: Some(agent_id.to_string()),
                memory_type: node_type.map(|t| match t {
                    "Trace" => MemoryType::Trace,
                    "ShortTerm" => MemoryType::ShortTerm,
                    "KnowledgeNode" => MemoryType::KnowledgeNode,
                    "Relation" => MemoryType::Relation,
                    _ => MemoryType::All,
                }),
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

        // 构造归属过滤条件：自己的节点 OR（include_shared 时）published 节点
        // 注意：sqlx QueryBuilder 的 push() 不会识别 ? 占位符，必须用 push_bind() 绑定参数
        // 使用冗余字段 is_published 替代 json_each(tags) 加速查询（走部分索引 idx_ltkn_is_published）
        let agent_id = query.agent_id.clone().unwrap_or_default();
        let include_shared = query.include_shared;
        builder.push(" AND ");
        if include_shared && !agent_id.is_empty() {
            builder.push("(agent_id = ");
            builder.push_bind(agent_id);
            builder.push(" OR is_published = 1)");
        } else if agent_id.is_empty() && include_shared {
            builder.push("is_published = 1");
        } else if !agent_id.is_empty() {
            builder.push("agent_id = ");
            builder.push_bind(agent_id);
        } else {
            builder.push("1=1");
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

        builder.push(" ORDER BY updated_at DESC");

        if let Some(limit) = &query.limit {
            builder.push(" LIMIT ");
            builder.push_bind(*limit as i64);
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
        let agent_id = search.filters.agent_id.unwrap_or_default();
        let include_shared = search.filters.include_shared;
        let keyword = search.keyword.unwrap_or_default();
        let limit_i64 = search.filters.limit.unwrap_or(50) as i64;
        let tags = search.filters.tags.clone().unwrap_or_default();

        // 空关键词直接返回空结果（FTS5 MATCH 空字符串会报错）
        if keyword.trim().is_empty() {
            return Ok(Vec::new());
        }

        // 转义关键词为 FTS5 短语匹配
        let escaped_keyword = escape_fts5_keyword(&keyword);

        // 构建带可选 tags 过滤的 SQL
        let has_tags = !tags.is_empty();
        let tags_clause = if has_tags {
            let placeholders = tags.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            format!(
                " AND EXISTS (SELECT 1 FROM json_each(m.tags) WHERE json_each.value IN ({placeholders}))"
            )
        } else {
            String::new()
        };

        // 构造归属过滤条件：自己的节点 OR（include_shared 时）published 节点
        // 使用冗余字段 is_published 替代 json_each(tags) 加速查询（走部分索引 idx_ltkn_is_published）
        // agent_id 为空时（全局搜索）：include_shared 返回所有 published 节点，否则返回空集
        let has_agent_filter = !agent_id.is_empty();
        let (ownership_clause, need_bind_agent) = if has_agent_filter {
            if include_shared {
                ("(m.agent_id = ? OR m.is_published = 1)".to_string(), true)
            } else {
                ("m.agent_id = ?".to_string(), true)
            }
        } else if include_shared {
            // 全局搜索：只返回 published 节点
            ("m.is_published = 1".to_string(), false)
        } else {
            // 无 agent_id 且不包含共享：返回空集
            ("1=0".to_string(), false)
        };

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

        let mut query = sqlx::query_as::<_, KnowledgeNodeSearchRow>(&sql).bind(escaped_keyword);

        // 绑定 agent_id 参数（如果归属条件需要）
        if need_bind_agent {
            query = query.bind(agent_id);
        }

        // 绑定 tags 参数（如果有）
        if has_tags {
            for tag in &tags {
                query = query.bind(tag);
            }
        }

        let rows: Vec<KnowledgeNodeSearchRow> = query.bind(limit_i64).fetch_all(&pool).await?;

        let results = rows
            .into_iter()
            .map(|row| {
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
            })
            .collect();

        Ok(results)
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
        let relation_type_str = relation.relation_type.to_string();

        sqlx::query!(
            r#"
INSERT INTO knowledge_node_relation (
    id, source_node_id, target_node_id, relation_type, created_at, updated_at
) VALUES (?, ?, ?, ?, ?, ?)
"#,
            relation.id,
            relation.source_node_id,
            relation.target_node_id,
            relation_type_str,
            relation.created_at,
            relation.updated_at,
        )
        .execute(&pool)
        .await?;

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
            let relation_type_str = relation.relation_type.to_string();
            sqlx::query!(
                r#"
INSERT INTO knowledge_node_relation (
    id, source_node_id, target_node_id, relation_type, created_at, updated_at
) VALUES (?, ?, ?, ?, ?, ?)
"#,
                relation.id,
                relation.source_node_id,
                relation.target_node_id,
                relation_type_str,
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
        let relation_type_str = relation.relation_type.to_string();

        sqlx::query!(
            r#"
INSERT INTO knowledge_node_relation (
    id, source_node_id, target_node_id, relation_type, created_at, updated_at
) VALUES (?, ?, ?, ?, ?, ?)
ON CONFLICT(id) DO UPDATE SET
    source_node_id = excluded.source_node_id,
    target_node_id = excluded.target_node_id,
    relation_type  = excluded.relation_type,
    updated_at     = excluded.updated_at
"#,
            relation.id,
            relation.source_node_id,
            relation.target_node_id,
            relation_type_str,
            relation.created_at,
            relation.updated_at,
        )
        .execute(&pool)
        .await?;

        Ok(())
    }

    async fn list_outgoing_relations(
        &self,
        ctx: RequestContext,
        source_id: &str,
    ) -> Result<Vec<KnowledgeNodeRelationPo>> {
        let pool = self.pool(ctx);
        // sqlx 不自动映射枚举，需要手动处理
        let rows = sqlx::query!(
            r#"
SELECT id, source_node_id, target_node_id, relation_type, created_at, updated_at
FROM knowledge_node_relation
WHERE source_node_id = ?
ORDER BY created_at ASC
"#,
            source_id
        )
        .fetch_all(&pool)
        .await?;

        let mut result = Vec::new();
        for row in rows {
            let relation_type = KnowledgeRelationType::from(row.relation_type);
            result.push(KnowledgeNodeRelationPo {
                id: row.id,
                source_node_id: row.source_node_id,
                target_node_id: row.target_node_id,
                relation_type,
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
SELECT id, source_node_id, target_node_id, relation_type, created_at, updated_at
FROM knowledge_node_relation
WHERE target_node_id = ?
ORDER BY created_at ASC
"#,
            target_id
        )
        .fetch_all(&pool)
        .await?;

        let mut result = Vec::new();
        for row in rows {
            let relation_type = KnowledgeRelationType::from(row.relation_type);
            result.push(KnowledgeNodeRelationPo {
                id: row.id,
                source_node_id: row.source_node_id,
                target_node_id: row.target_node_id,
                relation_type,
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
SELECT id, source_node_id, target_node_id, relation_type, created_at, updated_at
FROM knowledge_node_relation
WHERE source_node_id = ? OR target_node_id = ?
ORDER BY created_at ASC
"#,
            node_id,
            node_id
        )
        .fetch_all(&pool)
        .await?;

        let mut result = Vec::new();
        for row in rows {
            let relation_type = KnowledgeRelationType::from(row.relation_type);
            result.push(KnowledgeNodeRelationPo {
                id: row.id,
                source_node_id: row.source_node_id,
                target_node_id: row.target_node_id,
                relation_type,
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

    async fn delete_knowledge_relation(
        &self,
        ctx: RequestContext,
        relation_id: &str,
    ) -> Result<()> {
        let pool = self.pool(ctx);

        sqlx::query!(
            r#"DELETE FROM knowledge_node_relation WHERE id = ?"#,
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
        relation_type: KnowledgeRelationType,
    ) -> Result<Vec<KnowledgeNodeRelationPo>> {
        let pool = self.pool(ctx);
        let relation_type_str = relation_type.to_string();
        let rows = sqlx::query!(
            r#"
SELECT id, source_node_id, target_node_id, relation_type, created_at, updated_at
FROM knowledge_node_relation
WHERE source_node_id = ? AND relation_type = ?
ORDER BY created_at ASC
"#,
            source_id,
            relation_type_str
        )
        .fetch_all(&pool)
        .await?;

        let mut result = Vec::new();
        for row in rows {
            let relation_type = KnowledgeRelationType::from(row.relation_type);
            result.push(KnowledgeNodeRelationPo {
                id: row.id,
                source_node_id: row.source_node_id,
                target_node_id: row.target_node_id,
                relation_type,
                created_at: row.created_at,
                updated_at: row.updated_at,
            });
        }

        Ok(result)
    }
}
