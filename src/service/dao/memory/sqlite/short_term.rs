//! 短期记忆索引 DAO：CRUD + 条件查询 + FTS5 混合搜索

use super::MemoryDaoSqliteImpl;
use crate::models::memory::ShortTermMemoryIndexPo;
use crate::pkg::RequestContext;
use crate::pkg::storage::{build_fts5_like_clause, build_fts5_search_plan, merge_fts5_results};
use crate::service::dao::memory::{MemoryQuery, MemorySearch};
use common::enums::MemoryStatus;
use common::error::{Result, bail_err};
use sqlx::FromRow;

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

impl MemoryDaoSqliteImpl {
    pub(super) async fn create_short_term_index(
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

    pub(super) async fn update_short_term_index(
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

    pub(super) async fn forget_short_term_index(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<()> {
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

    pub(super) async fn get_short_term_index(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<ShortTermMemoryIndexPo>> {
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

    pub(super) async fn list_short_term_by_agent(
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

    pub(super) async fn query_short_term(
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

    pub(super) async fn search_short_term(
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

    pub(super) fn read_memory_content(&self, _index: &ShortTermMemoryIndexPo) -> Result<String> {
        // 原始文件读取由上层业务处理，这里直接返回空字符串占位
        Ok(String::new())
    }
}
