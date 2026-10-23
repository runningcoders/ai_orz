//! 本体漂移 DAO：词频聚合（节点 / 关系）与按词下钻

use super::{MAX_WORD_FREQ_ROWS, MemoryDaoSqliteImpl};
use crate::pkg::RequestContext;
use crate::service::dao::memory::{DriftClassDetailRow, DriftRelationDetailRow, TermFrequencyRow};
use common::api::{PagedResult, PaginationParams};
use common::error::Result;
use common::ontology::normalize;
use sqlx::{QueryBuilder, Sqlite};

impl MemoryDaoSqliteImpl {
    pub(super) async fn word_freq_relations(
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

    pub(super) async fn word_freq_nodes(
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

    pub(super) async fn detail_relations_by_term(
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

    pub(super) async fn detail_nodes_by_term(
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

    pub(super) async fn publish_nodes_by_type(
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
