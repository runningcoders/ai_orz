//! 知识属性 DAO：写入（旧生效行降级）/ 生效行列表 / 按值召回

use super::{IN_CLAUSE_CHUNK, MemoryDaoSqliteImpl};
use crate::models::memory::KnowledgePropertyPo;
use crate::pkg::RequestContext;
use common::enums::KnowledgeRelationStatus;
use common::enums::ontology::PropertyTargetType;
use common::error::Result;
use common::ontology::normalize;
use common::ontology_property::PropertyFilter;
use sqlx::QueryBuilder;

impl MemoryDaoSqliteImpl {
    /// 属性值写入的第 1 步：降级同槽位的旧生效行（Superseded）
    ///
    /// ⚠️ 必须在 INSERT **之前**执行：`uq_knowledge_property_active` 是
    /// `(target_type, target_id, term_key, ordinal) WHERE status = 1` 的部分唯一索引，
    /// 先插后降会撞唯一约束；两步必须在同一事务内完成，否则中途失败会留下
    /// 「两条生效值」或「零条生效值」的中间态。
    async fn supersede_active_property(
        tx: &mut sqlx::SqliteConnection,
        property: &KnowledgePropertyPo,
    ) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        let superseded = KnowledgeRelationStatus::Superseded as i32;
        let term_key = normalize(&property.term_key);
        sqlx::query!(
            r#"
UPDATE knowledge_property
SET "status" = ?, updated_at = ?
WHERE target_type = ? AND target_id = ? AND term_key = ? AND ordinal = ? AND "status" = 1
"#,
            superseded,
            now,
            property.target_type,
            property.target_id,
            term_key,
            property.ordinal,
        )
        .execute(tx)
        .await?;
        Ok(())
    }

    /// 属性值写入的第 2 步：插入新行（ON CONFLICT(id) 兜底幂等重放）
    async fn insert_property_row(
        tx: &mut sqlx::SqliteConnection,
        property: &KnowledgePropertyPo,
    ) -> Result<()> {
        let status = property.status as i32;
        let term_key = normalize(&property.term_key);
        sqlx::query!(
            r#"
INSERT INTO knowledge_property (
    id, target_type, target_id, term_key, data_type, ordinal,
    value_text, value_num, value_bool, value_ref, "status", created_at, updated_at
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
"#,
            property.id,
            property.target_type,
            property.target_id,
            term_key,
            property.data_type,
            property.ordinal,
            property.value_text,
            property.value_num,
            property.value_bool,
            property.value_ref,
            status,
            property.created_at,
            property.updated_at
        )
        .execute(tx)
        .await?;
        Ok(())
    }

    /// 属性列的 SELECT 清单（三个读方法共用，避免列名漂移时漏改一处）
    const PROPERTY_COLUMNS: &'static str = r#"id, target_type, target_id, term_key, data_type,
       ordinal, value_text, value_num, value_bool, value_ref, "status", created_at, updated_at"#;
}

impl MemoryDaoSqliteImpl {
    pub(super) async fn upsert_property(
        &self,
        ctx: RequestContext,
        property: &KnowledgePropertyPo,
    ) -> Result<()> {
        let pool = self.pool(ctx);
        let mut tx = pool.begin().await?;
        Self::supersede_active_property(&mut tx, property).await?;
        Self::insert_property_row(&mut tx, property).await?;
        tx.commit().await?;
        Ok(())
    }

    pub(super) async fn batch_upsert_properties(
        &self,
        ctx: RequestContext,
        properties: &[KnowledgePropertyPo],
    ) -> Result<()> {
        if properties.is_empty() {
            return Ok(());
        }
        let pool = self.pool(ctx);
        // 整批一个事务：中途失败整批回滚，不留「一半新版一半旧版」的中间态
        let mut tx = pool.begin().await?;
        for property in properties {
            Self::supersede_active_property(&mut tx, property).await?;
            Self::insert_property_row(&mut tx, property).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub(super) async fn list_active_properties(
        &self,
        ctx: RequestContext,
        target_type: PropertyTargetType,
        target_id: &str,
    ) -> Result<Vec<KnowledgePropertyPo>> {
        let pool = self.pool(ctx);
        let target_type = target_type.as_str();
        let active = KnowledgeRelationStatus::Active as i32;
        let sql = format!(
            "SELECT {} FROM knowledge_property \
             WHERE target_type = ? AND target_id = ? AND \"status\" = ? \
             ORDER BY term_key ASC, ordinal ASC",
            Self::PROPERTY_COLUMNS
        );
        let rows = sqlx::query_as::<_, KnowledgePropertyPo>(&sql)
            .bind(target_type)
            .bind(target_id)
            .bind(active)
            .fetch_all(&pool)
            .await?;
        Ok(rows)
    }

    pub(super) async fn list_active_properties_batch(
        &self,
        ctx: RequestContext,
        targets: &[(PropertyTargetType, String)],
    ) -> Result<std::collections::HashMap<(String, String), Vec<KnowledgePropertyPo>>> {
        let mut result: std::collections::HashMap<(String, String), Vec<KnowledgePropertyPo>> =
            std::collections::HashMap::new();
        if targets.is_empty() {
            return Ok(result);
        }

        // SQLite 绑定参数上限 999：每个载体占 2 绑定，按 IN_CLAUSE_CHUNK 分块
        let pool = self.pool(ctx);
        for chunk in targets.chunks(IN_CLAUSE_CHUNK) {
            let mut builder = QueryBuilder::new(format!(
                "SELECT {} FROM knowledge_property WHERE ",
                Self::PROPERTY_COLUMNS
            ));
            // (target_type = ? AND target_id = ?) OR ... —— 不用 row-value IN，
            // SQLite 的 (a,b) IN ((?,?),...) 依赖 3.15+ 的 row value 支持，可移植性更差
            for (i, (target_type, target_id)) in chunk.iter().enumerate() {
                if i > 0 {
                    builder.push(" OR ");
                }
                builder
                    .push("(target_type = ")
                    .push_bind(target_type.as_str())
                    .push(" AND target_id = ")
                    .push_bind(target_id.clone())
                    .push(")");
            }
            builder
                .push(" AND \"status\" = ")
                .push_bind(KnowledgeRelationStatus::Active as i32);
            builder.push(" ORDER BY term_key ASC, ordinal ASC");

            let rows = builder
                .build_query_as::<KnowledgePropertyPo>()
                .fetch_all(&pool)
                .await?;
            for row in rows {
                let key = (row.target_type.clone(), row.target_id.clone());
                result.entry(key).or_default().push(row);
            }
        }
        Ok(result)
    }

    pub(super) async fn supersede_properties(
        &self,
        ctx: RequestContext,
        target_type: PropertyTargetType,
        target_id: &str,
        term_key: &str,
    ) -> Result<u64> {
        let pool = self.pool(ctx);
        let now = chrono::Utc::now().timestamp();
        let superseded = KnowledgeRelationStatus::Superseded as i32;
        let term_key = normalize(term_key);
        let target_type = target_type.as_str();
        let result = sqlx::query!(
            r#"
UPDATE knowledge_property
SET "status" = ?, updated_at = ?
WHERE target_type = ? AND target_id = ? AND term_key = ? AND "status" = 1
"#,
            superseded,
            now,
            target_type,
            target_id,
            term_key
        )
        .execute(&pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub(super) async fn query_properties_by_value(
        &self,
        ctx: RequestContext,
        filter: &PropertyFilter,
    ) -> Result<Vec<KnowledgePropertyPo>> {
        // 运算符与值类型不匹配（如文本用 Gt）→ 空集，不报错：
        // 调用方是搜索过滤链路，「过滤无匹配」比「报错」更符合语义
        if !filter.is_supported() {
            return Ok(Vec::new());
        }

        let pool = self.pool(ctx);
        let (column, value_num, value_text, value_ref): (
            &str,
            Option<f64>,
            Option<String>,
            Option<String>,
        ) = match &filter.value {
            common::ontology_property::PropertyValue::Number(v) => {
                ("value_num", Some(*v), None, None)
            }
            common::ontology_property::PropertyValue::Reference(v) => {
                ("value_ref", None, None, Some(v.clone()))
            }
            other => (
                "value_text",
                None,
                Some(match other {
                    common::ontology_property::PropertyValue::Text(v) => v.clone(),
                    common::ontology_property::PropertyValue::Enum(v) => v.clone(),
                    common::ontology_property::PropertyValue::Date(v) => v.clone(),
                    _ => unreachable!("Number / Reference 已在上一分支处理"),
                }),
                None,
            ),
        };
        let op = match filter.op {
            common::ontology_property::PropertyFilterOp::Eq => "=",
            common::ontology_property::PropertyFilterOp::Gt => ">",
            common::ontology_property::PropertyFilterOp::Gte => ">=",
            common::ontology_property::PropertyFilterOp::Lt => "<",
            common::ontology_property::PropertyFilterOp::Lte => "<=",
        };

        let term_key = normalize(&filter.term_key);
        let sql = format!(
            "SELECT {} FROM knowledge_property \
             WHERE term_key = ? AND target_type = ? AND \"status\" = ? AND {} {} ? \
             ORDER BY target_id ASC, ordinal ASC",
            Self::PROPERTY_COLUMNS,
            column,
            op
        );
        let active = KnowledgeRelationStatus::Active as i32;
        let mut query = sqlx::query_as::<_, KnowledgePropertyPo>(&sql)
            .bind(term_key)
            .bind(filter.target_type.as_str())
            .bind(active);
        // 四列只有一列非 None，依次 bind（None 列不参与，故只会命中一个分支）
        if let Some(v) = value_num {
            query = query.bind(v);
        }
        if let Some(v) = value_text {
            query = query.bind(v);
        }
        if let Some(v) = value_ref {
            query = query.bind(v);
        }
        let rows = query.fetch_all(&pool).await?;
        Ok(rows)
    }
}
