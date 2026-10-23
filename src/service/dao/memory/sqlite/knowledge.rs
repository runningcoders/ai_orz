//! 长期知识节点 DAO：节点 CRUD / 查询 / FTS5 搜索 + 记忆引用

use super::MemoryDaoSqliteImpl;
use crate::models::memory::{KnowledgeReferencePo, LongTermKnowledgeNodePo, MemoryTrace};
use crate::pkg::RequestContext;
use crate::pkg::storage::{build_fts5_like_clause, build_fts5_search_plan, merge_fts5_results};
use crate::service::dao::memory::{MemoryQuery, MemorySearch};
use common::enums::{MemoryStatus, MemoryType};
use common::error::{Result, bail_err};
use common::ontology::normalize;
use serde_json;
use sqlx::FromRow;

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

impl MemoryDaoSqliteImpl {
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
}

impl MemoryDaoSqliteImpl {
    // ========== 长期知识图谱相关 ==========

    pub(super) async fn save_knowledge_node(
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

    pub(super) async fn update_knowledge_node(
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

    pub(super) async fn batch_save_knowledge_nodes(
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

    pub(super) async fn get_knowledge_node(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<LongTermKnowledgeNodePo>> {
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

    pub(super) async fn list_knowledge_nodes_by_agent(
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

    pub(super) async fn query_knowledge_nodes(
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

    pub(super) async fn search_knowledge_nodes(
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

        // 候选集限定（`MemoryQuery.ids`）：由调用方（如属性精确召回通道）给出
        // 「只在这些 id 里检索」。
        // ⚠️ 少了它，属性召回只能退化成「先语义召回再后置过滤」——语义 top_k
        // 截断会把属性命中的节点挡在外面，命中率被语义排序吃掉。
        let id_filter: Option<&Vec<String>> =
            search.filters.ids.as_ref().filter(|ids| !ids.is_empty());
        let ids_clause = match id_filter {
            Some(ids) => format!(
                " AND m.id IN ({})",
                ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ")
            ),
            None => String::new(),
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
  AND m.status != 0{tags_clause}{ids_clause}
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

            // 绑定候选集参数（如果有）
            if let Some(ids) = id_filter {
                for id in ids {
                    query = query.bind(id);
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
  AND m.status != 0{tags_clause}{ids_clause}
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
            // 绑定候选集参数（如果有）
            if let Some(ids) = id_filter {
                for id in ids {
                    query = query.bind(id);
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

    pub(super) async fn delete_knowledge_node(&self, ctx: RequestContext, id: &str) -> Result<()> {
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

    pub(super) async fn add_knowledge_reference(
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

    pub(super) async fn batch_add_knowledge_references(
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

    pub(super) async fn list_knowledge_references(
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
}
