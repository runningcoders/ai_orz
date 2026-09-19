//! Ontology DAO SQLite 单元测试

use crate::models::ontology::{OntologyClassPo, OntologyRelationTypePo, OntologySynonymMappingPo};
use crate::pkg::RequestContext;
use crate::service::dao::ontology::{self, OntologyClassQuery, OntologySynonymQuery};
use common::api::PaginationParams;
use common::enums::OntologyStatus;
use common::ontology::TermKind;
use sqlx::SqlitePool;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx(user_id, pool)
}

fn create_test_class(term_key: &str, display_name: &str) -> OntologyClassPo {
    OntologyClassPo::new(
        term_key,
        display_name,
        format!("{} 的语义说明", display_name),
        r#"["title"]"#,
    )
}

fn create_test_relation(term_key: &str, display_name: &str) -> OntologyRelationTypePo {
    OntologyRelationTypePo::new(
        term_key,
        display_name,
        format!("{} 的语义说明", display_name),
        r#"["document"]"#,
        String::new(),
        1.0,
        None,
    )
}

// ==================== 实体类 ====================

/// CRUD 往返：插入 → 按 id / term_key 查询 → 更新 → 退役（软删除）
#[sqlx::test]
async fn test_class_crud_roundtrip(pool: SqlitePool) {
    ontology::init();
    let dao = ontology::dao();
    let ctx = new_ctx("test-user", pool);

    let class = create_test_class("document", "文档");
    dao.insert_class(ctx.clone(), &class).await.unwrap();

    let by_id = dao.find_class_by_id(ctx.clone(), &class.id).await.unwrap();
    assert!(by_id.is_some());
    let found = by_id.unwrap();
    assert_eq!(found.term_key, "document");
    assert_eq!(found.display_name, "文档");
    assert_eq!(found.status, OntologyStatus::Active);
    assert_eq!(found.required_fields, r#"["title"]"#);

    let by_term_key = dao
        .find_class_by_term_key(ctx.clone(), "document")
        .await
        .unwrap();
    assert_eq!(by_term_key.unwrap().id, class.id);

    // 更新：term_key 不可变，其余可改
    let mut updated = found.clone();
    updated.display_name = "结构化文档".to_string();
    updated.description = "更新后的说明".to_string();
    updated.required_fields = r#"["title","content"]"#.to_string();
    dao.update_class(ctx.clone(), &updated).await.unwrap();
    let after_update = dao
        .find_class_by_id(ctx.clone(), &class.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after_update.display_name, "结构化文档");
    assert_eq!(after_update.required_fields, r#"["title","content"]"#);

    // 退役：软删除，记录仍在但状态为 Retired
    dao.retire_class(ctx.clone(), &class.id).await.unwrap();
    let retired = dao
        .find_class_by_id(ctx.clone(), &class.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retired.status, OntologyStatus::Retired);
    assert_eq!(retired.term_key, "document");

    // 重复退役幂等
    dao.retire_class(ctx.clone(), &class.id).await.unwrap();
}

/// LIKE 关键词转义：`\` `%` `_` 均前置反斜杠，普通字符不变
#[test]
fn test_escape_like() {
    assert_eq!(super::sqlite::escape_like("doc"), "doc");
    assert_eq!(super::sqlite::escape_like("100%"), "100\\%");
    assert_eq!(super::sqlite::escape_like("under_score"), "under\\_score");
    assert_eq!(super::sqlite::escape_like("back\\slash"), "back\\\\slash");
    assert_eq!(super::sqlite::escape_like("a%_\\b"), "a\\%\\_\\\\b");
}

/// UNIQUE(term_key) 冲突 → Conflict 错误（报错清晰，plan A5）
#[sqlx::test]
async fn test_class_term_key_unique_conflict(pool: SqlitePool) {
    ontology::init();
    let dao = ontology::dao();
    let ctx = new_ctx("test-user", pool);

    dao.insert_class(ctx.clone(), &create_test_class("document", "文档"))
        .await
        .unwrap();

    let err = dao
        .insert_class(ctx.clone(), &create_test_class("document", "另一文档"))
        .await
        .unwrap_err();
    assert!(matches!(err.code, common::error::ErrorCode::Conflict));
}

/// 分页组合查询：keyword / status 过滤 + 分页 + count 内嵌
#[sqlx::test]
async fn test_class_query_filters_and_pagination(pool: SqlitePool) {
    ontology::init();
    let dao = ontology::dao();
    let ctx = new_ctx("test-user", pool);

    for key in ["document", "event", "preference"] {
        dao.insert_class(ctx.clone(), &create_test_class(key, key))
            .await
            .unwrap();
    }
    // 一个退役条目
    let retired = create_test_class("legacy", "遗留");
    dao.insert_class(ctx.clone(), &retired).await.unwrap();
    dao.retire_class(ctx.clone(), &retired.id).await.unwrap();

    // 无过滤：全部 4 条（含退役）
    let all = dao
        .query_classes(ctx.clone(), OntologyClassQuery::default())
        .await
        .unwrap();
    assert_eq!(all.total, 4);

    // keyword 过滤（term_key 命中）
    let by_kw = dao
        .query_classes(
            ctx.clone(),
            OntologyClassQuery {
                keyword: Some("doc".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(by_kw.total, 1);
    assert_eq!(by_kw.items[0].term_key, "document");

    // status 过滤
    let active = dao
        .query_classes(
            ctx.clone(),
            OntologyClassQuery {
                status: Some(OntologyStatus::Active),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(active.total, 3);

    // 分页：limit=2 offset=0 → total=4, items=2
    let page1 = dao
        .query_classes(
            ctx.clone(),
            OntologyClassQuery {
                pagination: PaginationParams {
                    limit: Some(2),
                    offset: Some(0),
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(page1.total, 4);
    assert_eq!(page1.items.len(), 2);

    // 按 term_key 排序的分页衔接
    let page2 = dao
        .query_classes(
            ctx.clone(),
            OntologyClassQuery {
                pagination: PaginationParams {
                    limit: Some(2),
                    offset: Some(2),
                },
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(page2.items.len(), 2);
    assert_ne!(page1.items[0].id, page2.items[0].id);
}

// ==================== 关系类型 ====================

/// CRUD 往返：插入 → 查询 → 更新（weight_base / inverse_key）→ 退役
#[sqlx::test]
async fn test_relation_type_crud_roundtrip(pool: SqlitePool) {
    ontology::init();
    let dao = ontology::dao();
    let ctx = new_ctx("test-user", pool);

    let relation = create_test_relation("contains", "包含");
    dao.insert_relation_type(ctx.clone(), &relation)
        .await
        .unwrap();

    let by_id = dao
        .find_relation_type_by_id(ctx.clone(), &relation.id)
        .await
        .unwrap();
    assert!(by_id.is_some());
    let found = by_id.unwrap();
    assert_eq!(found.term_key, "contains");
    assert_eq!(found.weight_base, 1.0);
    assert_eq!(found.inverse_key, None);
    assert_eq!(found.parse_domain_classes(), vec!["document".to_string()]);

    let by_term_key = dao
        .find_relation_type_by_term_key(ctx.clone(), "contains")
        .await
        .unwrap();
    assert_eq!(by_term_key.unwrap().id, relation.id);

    // 更新：权重 / 逆向关系词 / 约束
    let mut updated = found.clone();
    updated.weight_base = 1.5;
    updated.inverse_key = Some("contained_by".to_string());
    updated.range_classes = r#"["concept"]"#.to_string();
    dao.update_relation_type(ctx.clone(), &updated)
        .await
        .unwrap();
    let after_update = dao
        .find_relation_type_by_id(ctx.clone(), &relation.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after_update.weight_base, 1.5);
    assert_eq!(after_update.inverse_key, Some("contained_by".to_string()));
    assert_eq!(
        after_update.parse_range_classes(),
        vec!["concept".to_string()]
    );

    // 退役 + 幂等
    dao.retire_relation_type(ctx.clone(), &relation.id)
        .await
        .unwrap();
    let retired = dao
        .find_relation_type_by_id(ctx.clone(), &relation.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retired.status, OntologyStatus::Retired);
    dao.retire_relation_type(ctx.clone(), &relation.id)
        .await
        .unwrap();
}

/// UNIQUE(term_key) 冲突 → Conflict 错误
#[sqlx::test]
async fn test_relation_type_term_key_unique_conflict(pool: SqlitePool) {
    ontology::init();
    let dao = ontology::dao();
    let ctx = new_ctx("test-user", pool);

    dao.insert_relation_type(ctx.clone(), &create_test_relation("contains", "包含"))
        .await
        .unwrap();

    let err = dao
        .insert_relation_type(ctx.clone(), &create_test_relation("contains", "重复包含"))
        .await
        .unwrap_err();
    assert!(matches!(err.code, common::error::ErrorCode::Conflict));
}

// ==================== 同义映射 ====================

/// UNIQUE(raw_term, target_kind) 语义：同 Kind 唯一，跨 Kind 可共存（plan A5）
#[sqlx::test]
async fn test_synonym_unique_semantics(pool: SqlitePool) {
    ontology::init();
    let dao = ontology::dao();
    let ctx = new_ctx("test-user", pool);

    let mapping = OntologySynonymMappingPo::new("包含", "relation", "contains");
    dao.insert_synonym(ctx.clone(), &mapping).await.unwrap();

    let by_raw = dao
        .find_synonyms_by_raw_term(ctx.clone(), "包含")
        .await
        .unwrap();
    assert_eq!(by_raw.len(), 1);
    assert_eq!(by_raw[0].target_key, "contains");
    assert_eq!(by_raw[0].kind(), Some(TermKind::Relation));

    // 同 raw_term + 同 target_kind → Conflict
    let dup = OntologySynonymMappingPo::new("包含", "relation", "part_of");
    let err = dao.insert_synonym(ctx.clone(), &dup).await.unwrap_err();
    assert!(matches!(err.code, common::error::ErrorCode::Conflict));

    // 同 raw_term + 不同 target_kind → 成功（同一旧词可分别映射到实体类与关系词）
    let cross_kind = OntologySynonymMappingPo::new("包含", "class", "container");
    dao.insert_synonym(ctx.clone(), &cross_kind).await.unwrap();
    let by_raw = dao
        .find_synonyms_by_raw_term(ctx.clone(), "包含")
        .await
        .unwrap();
    assert_eq!(by_raw.len(), 2);
}

/// 查询过滤 + 物理删除
#[sqlx::test]
async fn test_synonym_query_and_delete(pool: SqlitePool) {
    ontology::init();
    let dao = ontology::dao();
    let ctx = new_ctx("test-user", pool);

    dao.insert_synonym(
        ctx.clone(),
        &OntologySynonymMappingPo::new("包含", "relation", "contains"),
    )
    .await
    .unwrap();
    dao.insert_synonym(
        ctx.clone(),
        &OntologySynonymMappingPo::new("是", "relation", "is_a"),
    )
    .await
    .unwrap();
    dao.insert_synonym(
        ctx.clone(),
        &OntologySynonymMappingPo::new("文档", "class", "document"),
    )
    .await
    .unwrap();

    // kind 过滤
    let relations = dao
        .query_synonyms(
            ctx.clone(),
            OntologySynonymQuery {
                target_kind: Some(TermKind::Relation),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(relations.total, 2);

    let all = dao
        .query_synonyms(ctx.clone(), OntologySynonymQuery::default())
        .await
        .unwrap();
    assert_eq!(all.total, 3);

    // keyword 过滤（raw_term 命中）
    let by_kw = dao
        .query_synonyms(
            ctx.clone(),
            OntologySynonymQuery {
                keyword: Some("包含".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(by_kw.total, 1);
    assert_eq!(by_kw.items[0].target_key, "contains");

    // 物理删除 → 再查为空
    dao.delete_synonym(ctx.clone(), &by_kw.items[0].id)
        .await
        .unwrap();
    let after = dao
        .find_synonyms_by_raw_term(ctx.clone(), "包含")
        .await
        .unwrap();
    assert!(after.is_empty());

    // 删除不存在的 id 幂等
    dao.delete_synonym(ctx.clone(), "not-exists").await.unwrap();
}

// ==================== list_all（lexicon 数据源） ====================

/// 全量词表查询：含退役条目（认证与 lexicon 加载需要全貌）
#[sqlx::test]
async fn test_list_all_includes_retired(pool: SqlitePool) {
    ontology::init();
    let dao = ontology::dao();
    let ctx = new_ctx("test-user", pool);

    dao.insert_class(ctx.clone(), &create_test_class("document", "文档"))
        .await
        .unwrap();
    let retired_class = create_test_class("legacy", "遗留");
    dao.insert_class(ctx.clone(), &retired_class).await.unwrap();
    dao.retire_class(ctx.clone(), &retired_class.id)
        .await
        .unwrap();

    dao.insert_relation_type(ctx.clone(), &create_test_relation("contains", "包含"))
        .await
        .unwrap();

    dao.insert_synonym(
        ctx.clone(),
        &OntologySynonymMappingPo::new("包含", "relation", "contains"),
    )
    .await
    .unwrap();

    let classes = dao.list_all_classes(ctx.clone()).await.unwrap();
    assert_eq!(classes.len(), 2);
    assert!(classes.iter().any(|c| c.status == OntologyStatus::Retired));

    let relations = dao.list_all_relation_types(ctx.clone()).await.unwrap();
    assert_eq!(relations.len(), 1);

    let synonyms = dao.list_all_synonyms(ctx.clone()).await.unwrap();
    assert_eq!(synonyms.len(), 1);
}
