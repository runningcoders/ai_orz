//! Ontology DAO SQLite 实现

use crate::models::ontology::{OntologyClassPo, OntologyRelationTypePo, OntologySynonymMappingPo};
use crate::pkg::RequestContext;
use crate::service::dao::ontology::{
    OntologyClassQuery, OntologyDao, OntologyRelationTypeQuery, OntologySynonymQuery,
};
use common::constants::utils::current_timestamp;
use common::error::Result;
use common::ontology::normalize;
use sqlx::QueryBuilder;
use std::sync::OnceLock;

// ==================== 工厂方法 + 单例管理 ====================

static ONTOLOGY_DAO: OnceLock<std::sync::Arc<dyn OntologyDao>> = OnceLock::new();

/// 创建一个全新的 Ontology DAO 实例（用于测试）
pub fn new() -> std::sync::Arc<dyn OntologyDao> {
    std::sync::Arc::new(OntologyDaoSqliteImpl)
}

/// 获取 Ontology DAO 单例
pub fn dao() -> std::sync::Arc<dyn OntologyDao> {
    ONTOLOGY_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = ONTOLOGY_DAO.set(new());
}

// ==================== 错误辅助 ====================

/// UNIQUE 约束违反 → Conflict（SQLite 唯一约束错误码 2067），其余错误原样透传
///
/// 报错清晰要求（plan A5）：提示哪个词在哪个维度冲突，
/// 上层（Domain/handlers）不再需要解析 sqlx 原始 message。
fn map_unique_conflict(err: sqlx::Error, hint: String) -> common::error::Error {
    let is_unique =
        matches!(&err, sqlx::Error::Database(db) if db.code().as_deref() == Some("2067"));
    if is_unique {
        common::error::Error::conflict(hint)
    } else {
        err.into()
    }
}

// ==================== 实现 ====================

struct OntologyDaoSqliteImpl;

// ==================== 实体类（TBox：节点类型） ====================

#[async_trait::async_trait]
impl OntologyDao for OntologyDaoSqliteImpl {
    async fn insert_class(&self, ctx: RequestContext, class: &OntologyClassPo) -> Result<()> {
        // DAO 写入单点归一：term_key 落库前转为规范形，上层任意形态输入都收敛到同一键
        let term_key = normalize(&class.term_key);
        let status = class.status.to_i32();
        if let Err(e) = sqlx::query!(
            "INSERT INTO ontology_classes (id, term_key, display_name, description, required_fields, status, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            class.id,
            term_key,
            class.display_name,
            class.description,
            class.required_fields,
            status,
            class.created_at,
            class.updated_at
        )
        .execute(ctx.db_pool())
        .await
        {
            return Err(map_unique_conflict(
                e,
                format!("实体类 term_key={} 已存在", term_key),
            ));
        }
        Ok(())
    }

    async fn find_class_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<OntologyClassPo>> {
        let class = sqlx::query_as!(
            OntologyClassPo,
            r#"
SELECT id, term_key, display_name, description, required_fields,
       status AS "status: _", created_at, updated_at
FROM ontology_classes WHERE id = ?
            "#,
            id
        )
        .fetch_optional(ctx.db_pool())
        .await?;
        Ok(class)
    }

    async fn find_class_by_term_key(
        &self,
        ctx: RequestContext,
        term_key: &str,
    ) -> Result<Option<OntologyClassPo>> {
        // 键读参数入口归一：DB 中只存规范形，任意形态输入都能命中
        let term_key = normalize(term_key);
        let class = sqlx::query_as!(
            OntologyClassPo,
            r#"
SELECT id, term_key, display_name, description, required_fields,
       status AS "status: _", created_at, updated_at
FROM ontology_classes WHERE term_key = ?
            "#,
            term_key
        )
        .fetch_optional(ctx.db_pool())
        .await?;
        Ok(class)
    }

    async fn query_classes(
        &self,
        ctx: RequestContext,
        query: OntologyClassQuery,
    ) -> Result<common::api::PagedResult<OntologyClassPo>> {
        let pool = ctx.db_pool();

        let mut count_builder =
            QueryBuilder::new("SELECT COUNT(*) FROM ontology_classes WHERE 1=1");
        push_class_filters(&mut count_builder, &query);
        let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;

        let mut list_builder = QueryBuilder::new(
            r#"SELECT id, term_key, display_name, description, required_fields, status, created_at, updated_at FROM ontology_classes WHERE 1=1"#,
        );
        push_class_filters(&mut list_builder, &query);
        list_builder.push(" ORDER BY term_key ASC");
        push_pagination(&mut list_builder, &query.pagination);

        let items = list_builder
            .build_query_as::<OntologyClassPo>()
            .fetch_all(pool)
            .await?;

        Ok(common::api::PagedResult {
            items,
            total: total as usize,
        })
    }

    async fn list_all_classes(&self, ctx: RequestContext) -> Result<Vec<OntologyClassPo>> {
        let classes = sqlx::query_as!(
            OntologyClassPo,
            r#"
SELECT id, term_key, display_name, description, required_fields,
       status AS "status: _", created_at, updated_at
FROM ontology_classes ORDER BY term_key ASC
            "#
        )
        .fetch_all(ctx.db_pool())
        .await?;
        Ok(classes)
    }

    async fn update_class(&self, ctx: RequestContext, class: &OntologyClassPo) -> Result<()> {
        // status 不在可更新列内（生命周期只由 insert/retire 驱动，防止退役态被意外复活）
        let now = current_timestamp();
        sqlx::query!(
            r#"
UPDATE ontology_classes
SET display_name = ?, description = ?, required_fields = ?, updated_at = ?
WHERE id = ?
            "#,
            class.display_name,
            class.description,
            class.required_fields,
            now,
            class.id
        )
        .execute(ctx.db_pool())
        .await?;
        Ok(())
    }

    async fn retire_class(&self, ctx: RequestContext, id: &str) -> Result<()> {
        // 软删除 status=0；幂等：不存在或已退役均静默成功（对齐 organization_link::revoke）
        let now = current_timestamp();
        sqlx::query!(
            "UPDATE ontology_classes SET status = 0, updated_at = ? WHERE id = ?",
            now,
            id
        )
        .execute(ctx.db_pool())
        .await?;
        Ok(())
    }

    // ==================== 关系类型（TBox：边类型） ====================

    async fn insert_relation_type(
        &self,
        ctx: RequestContext,
        relation_type: &OntologyRelationTypePo,
    ) -> Result<()> {
        // DAO 写入单点归一（与 insert_class 同口径）
        let term_key = normalize(&relation_type.term_key);
        let status = relation_type.status.to_i32();
        if let Err(e) = sqlx::query!(
            "INSERT INTO ontology_relation_types (id, term_key, display_name, description, domain_classes, range_classes, weight_base, inverse_key, status, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            relation_type.id,
            term_key,
            relation_type.display_name,
            relation_type.description,
            relation_type.domain_classes,
            relation_type.range_classes,
            relation_type.weight_base,
            relation_type.inverse_key,
            status,
            relation_type.created_at,
            relation_type.updated_at
        )
        .execute(ctx.db_pool())
        .await
        {
            return Err(map_unique_conflict(
                e,
                format!("关系类型 term_key={} 已存在", term_key),
            ));
        }
        Ok(())
    }

    async fn find_relation_type_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<OntologyRelationTypePo>> {
        let relation_type = sqlx::query_as!(
            OntologyRelationTypePo,
            r#"
SELECT id, term_key, display_name, description, domain_classes, range_classes,
       weight_base, inverse_key, status AS "status: _", created_at, updated_at
FROM ontology_relation_types WHERE id = ?
            "#,
            id
        )
        .fetch_optional(ctx.db_pool())
        .await?;
        Ok(relation_type)
    }

    async fn find_relation_type_by_term_key(
        &self,
        ctx: RequestContext,
        term_key: &str,
    ) -> Result<Option<OntologyRelationTypePo>> {
        // 键读参数入口归一（与 find_class_by_term_key 同口径）
        let term_key = normalize(term_key);
        let relation_type = sqlx::query_as!(
            OntologyRelationTypePo,
            r#"
SELECT id, term_key, display_name, description, domain_classes, range_classes,
       weight_base, inverse_key, status AS "status: _", created_at, updated_at
FROM ontology_relation_types WHERE term_key = ?
            "#,
            term_key
        )
        .fetch_optional(ctx.db_pool())
        .await?;
        Ok(relation_type)
    }

    async fn query_relation_types(
        &self,
        ctx: RequestContext,
        query: OntologyRelationTypeQuery,
    ) -> Result<common::api::PagedResult<OntologyRelationTypePo>> {
        let pool = ctx.db_pool();

        let mut count_builder =
            QueryBuilder::new("SELECT COUNT(*) FROM ontology_relation_types WHERE 1=1");
        push_relation_type_filters(&mut count_builder, &query);
        let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;

        let mut list_builder = QueryBuilder::new(
            r#"SELECT id, term_key, display_name, description, domain_classes, range_classes, weight_base, inverse_key, status, created_at, updated_at FROM ontology_relation_types WHERE 1=1"#,
        );
        push_relation_type_filters(&mut list_builder, &query);
        list_builder.push(" ORDER BY term_key ASC");
        push_pagination(&mut list_builder, &query.pagination);

        let items = list_builder
            .build_query_as::<OntologyRelationTypePo>()
            .fetch_all(pool)
            .await?;

        Ok(common::api::PagedResult {
            items,
            total: total as usize,
        })
    }

    async fn list_all_relation_types(
        &self,
        ctx: RequestContext,
    ) -> Result<Vec<OntologyRelationTypePo>> {
        let relation_types = sqlx::query_as!(
            OntologyRelationTypePo,
            r#"
SELECT id, term_key, display_name, description, domain_classes, range_classes,
       weight_base, inverse_key, status AS "status: _", created_at, updated_at
FROM ontology_relation_types ORDER BY term_key ASC
            "#
        )
        .fetch_all(ctx.db_pool())
        .await?;
        Ok(relation_types)
    }

    async fn update_relation_type(
        &self,
        ctx: RequestContext,
        relation_type: &OntologyRelationTypePo,
    ) -> Result<()> {
        // status 不在可更新列内（生命周期只由 insert/retire 驱动，防止退役态被意外复活）
        let now = current_timestamp();
        sqlx::query!(
            r#"
UPDATE ontology_relation_types
SET display_name = ?, description = ?, domain_classes = ?, range_classes = ?,
    weight_base = ?, inverse_key = ?, updated_at = ?
WHERE id = ?
            "#,
            relation_type.display_name,
            relation_type.description,
            relation_type.domain_classes,
            relation_type.range_classes,
            relation_type.weight_base,
            relation_type.inverse_key,
            now,
            relation_type.id
        )
        .execute(ctx.db_pool())
        .await?;
        Ok(())
    }

    async fn retire_relation_type(&self, ctx: RequestContext, id: &str) -> Result<()> {
        // 软删除 status=0；幂等：不存在或已退役均静默成功
        let now = current_timestamp();
        sqlx::query!(
            "UPDATE ontology_relation_types SET status = 0, updated_at = ? WHERE id = ?",
            now,
            id
        )
        .execute(ctx.db_pool())
        .await?;
        Ok(())
    }

    // ==================== 同义映射 ====================

    async fn insert_synonym(
        &self,
        ctx: RequestContext,
        mapping: &OntologySynonymMappingPo,
    ) -> Result<()> {
        // DAO 写入单点归一：raw_term / target_key 都收敛到规范形
        let raw_term = normalize(&mapping.raw_term);
        let target_key = normalize(&mapping.target_key);
        if let Err(e) = sqlx::query!(
            "INSERT INTO ontology_synonym_mappings (id, raw_term, target_kind, target_key, created_at) VALUES (?, ?, ?, ?, ?)",
            mapping.id,
            raw_term,
            mapping.target_kind,
            target_key,
            mapping.created_at
        )
        .execute(ctx.db_pool())
        .await
        {
            return Err(map_unique_conflict(
                e,
                format!(
                    "同义映射 raw_term={} → {} 已存在",
                    raw_term, mapping.target_kind
                ),
            ));
        }
        Ok(())
    }

    async fn find_synonyms_by_raw_term(
        &self,
        ctx: RequestContext,
        raw_term: &str,
    ) -> Result<Vec<OntologySynonymMappingPo>> {
        // 键读参数入口归一（与写入侧同口径）
        let raw_term = normalize(raw_term);
        let mappings = sqlx::query_as!(
            OntologySynonymMappingPo,
            r#"
SELECT id, raw_term, target_kind, target_key, created_at
FROM ontology_synonym_mappings WHERE raw_term = ?
            "#,
            raw_term
        )
        .fetch_all(ctx.db_pool())
        .await?;
        Ok(mappings)
    }

    async fn query_synonyms(
        &self,
        ctx: RequestContext,
        query: OntologySynonymQuery,
    ) -> Result<common::api::PagedResult<OntologySynonymMappingPo>> {
        let pool = ctx.db_pool();

        let mut count_builder =
            QueryBuilder::new("SELECT COUNT(*) FROM ontology_synonym_mappings WHERE 1=1");
        push_synonym_filters(&mut count_builder, &query);
        let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;

        let mut list_builder = QueryBuilder::new(
            r#"SELECT id, raw_term, target_kind, target_key, created_at FROM ontology_synonym_mappings WHERE 1=1"#,
        );
        push_synonym_filters(&mut list_builder, &query);
        list_builder.push(" ORDER BY raw_term ASC");
        push_pagination(&mut list_builder, &query.pagination);

        let items = list_builder
            .build_query_as::<OntologySynonymMappingPo>()
            .fetch_all(pool)
            .await?;

        Ok(common::api::PagedResult {
            items,
            total: total as usize,
        })
    }

    async fn list_all_synonyms(
        &self,
        ctx: RequestContext,
    ) -> Result<Vec<OntologySynonymMappingPo>> {
        let mappings = sqlx::query_as!(
            OntologySynonymMappingPo,
            r#"
SELECT id, raw_term, target_kind, target_key, created_at
FROM ontology_synonym_mappings ORDER BY raw_term ASC
            "#
        )
        .fetch_all(ctx.db_pool())
        .await?;
        Ok(mappings)
    }

    async fn delete_synonym(&self, ctx: RequestContext, id: &str) -> Result<()> {
        // 物理删除；幂等：不存在时静默成功
        sqlx::query!("DELETE FROM ontology_synonym_mappings WHERE id = ?", id)
            .execute(ctx.db_pool())
            .await?;
        Ok(())
    }
}

// ==================== 查询构造辅助 ====================

/// LIKE 模式转义：`\` `%` `_` 前置反斜杠，配合 `ESCAPE '\'` 使用，
/// 防止关键词自带的通配符越过预期的包含匹配语义。
/// pub(super) 供单元测试直接断言转义结果。
pub(super) fn escape_like(keyword: &str) -> String {
    keyword
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// 实体类查询 WHERE 构造（count 与 list 复用同一套，AGENTS §4.9）
fn push_class_filters<'args>(
    builder: &mut QueryBuilder<'args, sqlx::Sqlite>,
    query: &'args OntologyClassQuery,
) {
    if let Some(status) = query.status {
        builder.push(" AND status = ").push_bind(status.to_i32());
    }
    if let Some(keyword) = &query.keyword {
        let pattern = format!("%{}%", escape_like(keyword));
        builder
            .push(" AND (term_key LIKE ")
            .push_bind(pattern.clone())
            .push(" ESCAPE '\\' OR display_name LIKE ")
            .push_bind(pattern.clone())
            .push(" ESCAPE '\\' OR description LIKE ")
            .push_bind(pattern)
            .push(" ESCAPE '\\')");
    }
}

/// 关系类型查询 WHERE 构造（count 与 list 复用同一套）
fn push_relation_type_filters<'args>(
    builder: &mut QueryBuilder<'args, sqlx::Sqlite>,
    query: &'args OntologyRelationTypeQuery,
) {
    if let Some(status) = query.status {
        builder.push(" AND status = ").push_bind(status.to_i32());
    }
    if let Some(keyword) = &query.keyword {
        let pattern = format!("%{}%", escape_like(keyword));
        builder
            .push(" AND (term_key LIKE ")
            .push_bind(pattern.clone())
            .push(" ESCAPE '\\' OR display_name LIKE ")
            .push_bind(pattern.clone())
            .push(" ESCAPE '\\' OR description LIKE ")
            .push_bind(pattern)
            .push(" ESCAPE '\\')");
    }
}

/// 同义映射查询 WHERE 构造（count 与 list 复用同一套）
fn push_synonym_filters<'args>(
    builder: &mut QueryBuilder<'args, sqlx::Sqlite>,
    query: &'args OntologySynonymQuery,
) {
    if let Some(kind) = query.target_kind {
        builder
            .push(" AND target_kind = ")
            .push_bind(kind.as_str().to_string());
    }
    if let Some(target_key) = &query.target_key {
        builder
            .push(" AND target_key = ")
            .push_bind(target_key.clone());
    }
    if let Some(keyword) = &query.keyword {
        let pattern = format!("%{}%", escape_like(keyword));
        builder
            .push(" AND (raw_term LIKE ")
            .push_bind(pattern.clone())
            .push(" ESCAPE '\\' OR target_key LIKE ")
            .push_bind(pattern)
            .push(" ESCAPE '\\')");
    }
}

/// 分页追加（LIMIT 省略时不限条数；有 OFFSET 无 LIMIT 时 SQLite 需显式 LIMIT -1）
fn push_pagination<'args>(
    builder: &mut QueryBuilder<'args, sqlx::Sqlite>,
    pagination: &common::api::PaginationParams,
) {
    if let Some(limit) = pagination.limit {
        builder.push(" LIMIT ").push_bind(limit as i64);
    } else if pagination.offset.is_some() {
        builder.push(" LIMIT -1");
    }
    if let Some(offset) = pagination.offset {
        builder.push(" OFFSET ").push_bind(offset as i64);
    }
}
