//! 知识节点关系 DAO：写入（同键旧边降级）/ 批量 / 方向查询 / 端点校验

use super::{IN_CLAUSE_CHUNK, MemoryDaoSqliteImpl};
use crate::models::memory::KnowledgeNodeRelationPo;
use crate::pkg::RequestContext;
use crate::service::dao::memory::MemoryQuery;
use common::enums::{KnowledgeRelationStatus, MemoryStatus};
use common::error::{Result, bail_err};
use common::ontology::normalize;

/// 关系边所有读路径共用的优先级排序键
///
/// 排序意图（从高到低）：
/// 1. 标注过强度的边优先（`weight IS NULL` 升序，NULL 排最后）；
/// 2. 强度越大越前；
/// 3. 最近确认/修正的边优先（修正 = 旧边降级 + 插入新行，`updated_at` 会更新）；
/// 4. 最后按插入顺序（`rowid`）兜底：同秒写入时后插入的边在前，图遍历按
///    `max_breadth` 截断时新边不会被老边饿死，分页次序也确定。
///
/// ⚠️ 下面几处 `sqlx::query!` 是编译期 SQL，无法引用本常量；宏内字面量必须与
/// 这里逐字一致。QueryBuilder 路径统一引用常量，避免多处漂移。
const RELATION_ORDER_BY: &str =
    r#" ORDER BY (weight IS NULL) ASC, weight DESC, updated_at DESC, created_at DESC, rowid DESC"#;

/// 关系边优先级比较（与 [`RELATION_ORDER_BY`] 的前三档同口径）
///
/// PO 不带 `rowid`，跨块合并时由调用方在末尾补 `rowid DESC` 作为最终次序键。
fn relation_priority_cmp(
    a: &KnowledgeNodeRelationPo,
    b: &KnowledgeNodeRelationPo,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    a.weight
        .is_none()
        .cmp(&b.weight.is_none())
        .then_with(|| match (a.weight, b.weight) {
            (Some(x), Some(y)) => y.partial_cmp(&x).unwrap_or(Ordering::Equal),
            _ => Ordering::Equal,
        })
        .then_with(|| b.updated_at.cmp(&a.updated_at))
        .then_with(|| b.created_at.cmp(&a.created_at))
}

impl MemoryDaoSqliteImpl {
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
    ///
    /// 返回 `(rowid, 边)`：跨块拼接后要按同一优先级键重排，而优先级最后一档正是
    /// 插入顺序 `rowid`——PO 里没有这一列，必须随行带出。
    async fn list_relations_batch_chunk(
        &self,
        ctx: RequestContext,
        node_ids: &[String],
    ) -> Result<Vec<(i64, KnowledgeNodeRelationPo)>> {
        use sqlx::QueryBuilder;

        let pool = self.pool(ctx);
        let mut builder = QueryBuilder::new(
            r#"SELECT rowid, id, source_node_id, target_node_id, relation_type, weight, "status", created_at, updated_at
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
        separated.push_unseparated(")) AND \"status\" = 1");
        separated.push_unseparated(RELATION_ORDER_BY);

        let rows = builder.build().fetch_all(&pool).await?;

        use sqlx::Row;
        Ok(rows
            .iter()
            .map(|row| {
                let rowid: i64 = row.get("rowid");
                (rowid, Self::knowledge_relation_from_row(row))
            })
            .collect())
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
        separated.push_unseparated(") AND \"status\" = 1");
        separated.push_unseparated(RELATION_ORDER_BY);

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

    /// 建边端点事务内防线：INSERT 前在同一事务、同一连接校验两端节点均存在且活跃
    ///
    /// handler 层的端点批量预检与本 DAO 落库之间没有事务/外键约束（建表 DDL 无
    /// REFERENCES），预检通过后端点节点可能被并发软删（status → Forgotten），
    /// 若不复查会插出悬空边。本校验与 INSERT 共用同一事务连接：SQLite 单写锁下
    /// 「检查 → 插入 → 提交」之间不存在跨连接窗口——并发删除要么先提交（本校验
    /// 失败、本事务整体回滚、不落任何行），要么后提交（本事务已先完成）。
    /// 错误码与 handler 预检回执同源（SourceNodeNotFound / TargetNodeNotFound）。
    async fn ensure_relation_endpoints_active(
        tx: &mut sqlx::SqliteConnection,
        relation: &KnowledgeNodeRelationPo,
    ) -> Result<()> {
        let active_i32 = MemoryStatus::Active as i32;

        let source_exists = sqlx::query_scalar!(
            r#"
SELECT COUNT(*) AS "count!" FROM long_term_knowledge_node
WHERE id = ? AND status = ?
"#,
            relation.source_node_id,
            active_i32
        )
        .fetch_one(&mut *tx)
        .await?;
        if source_exists == 0 {
            bail_err!(
                SourceNodeNotFound,
                "建边被拒绝：源节点 source_node_id={} 不存在或非活跃状态（target_node_id={}），事务已回滚、未落任何边",
                relation.source_node_id,
                relation.target_node_id
            );
        }

        let target_exists = sqlx::query_scalar!(
            r#"
SELECT COUNT(*) AS "count!" FROM long_term_knowledge_node
WHERE id = ? AND status = ?
"#,
            relation.target_node_id,
            active_i32
        )
        .fetch_one(&mut *tx)
        .await?;
        if target_exists == 0 {
            bail_err!(
                TargetNodeNotFound,
                "建边被拒绝：目标节点 target_node_id={} 不存在或非活跃状态（source_node_id={}），事务已回滚、未落任何边",
                relation.target_node_id,
                relation.source_node_id
            );
        }

        Ok(())
    }
}

impl MemoryDaoSqliteImpl {
    // ========== 知识节点关系相关 ==========

    pub(super) async fn add_knowledge_relation(
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

    pub(super) async fn batch_add_knowledge_relations(
        &self,
        ctx: RequestContext,
        relations: &[KnowledgeNodeRelationPo],
    ) -> Result<()> {
        let pool = self.pool(ctx);
        let mut tx = pool.begin().await?;

        for relation in relations {
            // 端点存在性事务内防线：与下方 INSERT 同事务，任一端点缺失/非活跃
            // 即整体回滚，杜绝「handler 预检通过后被并发删除」的悬空边落库
            Self::ensure_relation_endpoints_active(&mut tx, relation).await?;

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

    pub(super) async fn upsert_knowledge_relation(
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

    pub(super) async fn list_outgoing_relations(
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
ORDER BY (weight IS NULL) ASC, weight DESC, updated_at DESC, created_at DESC, rowid DESC
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

    pub(super) async fn list_incoming_relations(
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
ORDER BY (weight IS NULL) ASC, weight DESC, updated_at DESC, created_at DESC, rowid DESC
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

    pub(super) async fn list_all_relations_for_node(
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
ORDER BY (weight IS NULL) ASC, weight DESC, updated_at DESC, created_at DESC, rowid DESC
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

    pub(super) async fn list_relations_batch(
        &self,
        ctx: RequestContext,
        node_ids: &[String],
    ) -> Result<Vec<KnowledgeNodeRelationPo>> {
        if node_ids.is_empty() {
            return Ok(Vec::new());
        }

        // SQLite 绑定参数上限 999：每 ID 占 2 绑定，按 IN_CLAUSE_CHUNK 分块避免超限
        let mut result: Vec<(i64, KnowledgeNodeRelationPo)> = Vec::new();
        for chunk in node_ids.chunks(IN_CLAUSE_CHUNK) {
            result.extend(self.list_relations_batch_chunk(ctx.clone(), chunk).await?);
        }
        // 跨块拼接后按完整优先级键重排（含 rowid 兜底），保持与单块 SQL 同序；
        // 两端点分属不同块的边会在两块中各命中一次，稳定排序后按 id 去重。
        result.sort_by(|(rowid_a, a), (rowid_b, b)| {
            relation_priority_cmp(a, b).then_with(|| rowid_b.cmp(rowid_a))
        });
        result.dedup_by(|(_, a), (_, b)| a.id == b.id);
        Ok(result.into_iter().map(|(_, rel)| rel).collect())
    }

    pub(super) async fn query_knowledge_relations(
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

    pub(super) async fn delete_knowledge_relation(
        &self,
        ctx: RequestContext,
        relation_id: &str,
    ) -> Result<()> {
        let pool = self.pool(ctx);
        let now = chrono::Utc::now().timestamp();
        let status_i32 = KnowledgeRelationStatus::Deleted as i32;
        // 软删除：标记为 Deleted(2)，行保留支持恢复。
        // 仅降级生效边：Superseded 历史边的版本链语义（"被替换"）不可被覆盖成"被删除"
        let result = sqlx::query!(
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

        // 假成功修复：WHERE 带 status=1 过滤后 0 行受影响 = 边不存在或已非生效
        // （被新版本取代/已删除）——必须显式报错，不能静默 Ok 让调用方以为删掉了。
        if result.rows_affected() == 0 {
            bail_err!(
                ResourceNotFound,
                "关系边 {} 不存在或已非生效状态（可能已被新版本取代或已删除）",
                relation_id
            );
        }

        Ok(())
    }

    pub(super) async fn delete_all_relations_for_node(
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

    pub(super) async fn find_relations_by_type(
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
ORDER BY (weight IS NULL) ASC, weight DESC, updated_at DESC, created_at DESC, rowid DESC
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
}
