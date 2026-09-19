//! Ontology Domain 单元测试
//!
//! 覆盖 P2-1 验收口径（docs/plan/本体论知识沉淀落地.md）：
//! - 写侧校验：term_key 非空 / JSON 字段合法性 / 引用完整性 / raw_term 归一化
//! - seed 预置注入幂等：重复调用 0 重复行（验收核心）
//! - certify 软门禁：命中置位 `is_published`，Drift 调用成功且零写动作
//! - 词表视图：Active 过滤 + 同义样例映射 + 注入裁剪字段
//! - 看板聚合口径 + P5-2 端到端链：覆盖率计算 / Top 漂移词排序 / 词表修订后漂移愈合

use super::{HrDomain, domain};
use crate::models::memory::{KnowledgeNodeRelationPo, LongTermKnowledgeNodePo};
use crate::models::ontology::{
    OntologyClass, OntologyClassPo, OntologyRelationType, OntologyRelationTypePo,
    OntologySynonymMapping, OntologySynonymMappingPo,
};
use crate::pkg::RequestContext;
use crate::service::dao::ontology::{
    OntologyClassQuery, OntologyRelationTypeQuery, OntologySynonymQuery,
};
use common::api::PaginationParams;
use common::api::ontology::GetDriftDashboardRequest;
use common::enums::KnowledgeRelationStatus;
use common::enums::MemoryStatus;
use common::ontology::{
    PresetOntologyClass, PresetOntologyLexicon, PresetOntologyRelationType, PresetOntologySynonym,
    TermKind,
};
use sqlx::SqlitePool;
use std::sync::Arc;

/// 测试用全量分页参数（词表量级小，limit 100 已覆盖）
fn all_pagination() -> PaginationParams {
    PaginationParams {
        limit: Some(100),
        offset: None,
    }
}

/// 初始化 HR Domain 全部依赖（含本体 DAL 链）
///
/// 初始化顺序：dao -> dal -> domain；其中 `dao::memory` 必须先于
/// `dal::ontology`（OntologyDalImpl 构造依赖 MemoryDao 单例）。
fn init_test_env(pool: SqlitePool) -> (Arc<dyn HrDomain>, RequestContext) {
    // DAO（memory 最先：本体 DAL 组合依赖）
    crate::service::dao::memory::init();
    crate::service::dao::ontology::init();
    crate::service::dao::agent::init();
    crate::service::dao::tool::init();
    crate::service::dao::skill::init();
    crate::service::dao::tool_call::init();
    crate::service::dao::model_provider::init();
    crate::service::dao::cortex::init();
    // 组织（HR Domain 构造依赖）
    crate::service::dao::organization::init();
    crate::service::dao::organization_link::init();
    crate::service::dao::organization_pairing::init();
    crate::service::dao::federation_contract::init();
    // DAL
    crate::service::dal::agent::init();
    crate::service::dal::tool::init();
    crate::service::dal::skill::init();
    crate::service::dal::model_provider::init();
    crate::service::dal::organization::init();
    crate::service::dal::ontology::init();
    super::init();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);
    (domain(), ctx)
}

/// 预置词表 fixture：带书写形变体（大写 / 空白）验证归一化口径
///
/// 2 实体类 + 1 关系类型（引用第一个类）+ 1 同义映射 = 4 条。
fn preset_lexicon() -> PresetOntologyLexicon {
    PresetOntologyLexicon {
        classes: vec![
            PresetOntologyClass {
                term_key: "Agent".to_string(),
                display_name: "智能体".to_string(),
                description: "自主执行单元".to_string(),
                required_fields: vec!["name".to_string()],
            },
            PresetOntologyClass {
                term_key: "document".to_string(),
                display_name: "文档".to_string(),
                description: "知识载体".to_string(),
                required_fields: vec![],
            },
        ],
        relation_types: vec![PresetOntologyRelationType {
            term_key: "contains".to_string(),
            display_name: "包含".to_string(),
            description: "组成关系".to_string(),
            domain_classes: vec!["agent".to_string()],
            range_classes: vec!["document".to_string()],
            weight_base: Some(1.5),
            inverse_key: Some("contained_by".to_string()),
        }],
        synonym_mappings: vec![PresetOntologySynonym {
            raw_term: "  CONTAINS ".to_string(),
            target_kind: TermKind::Relation,
            target_key: "contains".to_string(),
        }],
    }
}

/// create_class：term_key 空白拒绝 + required_fields 非法 JSON 拒绝 + 合法创建可回读
#[sqlx::test]
async fn test_create_class_validates_key_and_json(pool: SqlitePool) {
    let (hr, ctx) = init_test_env(pool);
    let onto = hr.ontology_domain();

    let empty_key = OntologyClass::from_po(OntologyClassPo::new("   ", "空白", "x", "[]"));
    let err = onto
        .create_class(ctx.clone(), &empty_key)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("term_key"),
        "空白 term_key 应被拒绝，实际: {err}"
    );

    let bad_json =
        OntologyClass::from_po(OntologyClassPo::new("widget", "小部件", "x", "not-json"));
    let err = onto.create_class(ctx.clone(), &bad_json).await.unwrap_err();
    assert!(
        err.to_string().contains("required_fields"),
        "非法 JSON 数组应被拒绝，实际: {err}"
    );

    let ok = OntologyClass::from_po(OntologyClassPo::new(
        "widget",
        "小部件",
        "自建实体类",
        r#"["a","b"]"#,
    ));
    onto.create_class(ctx.clone(), &ok).await.unwrap();
    let found = onto
        .get_class(ctx.clone(), &ok.po.id)
        .await
        .unwrap()
        .expect("合法创建后可回读");
    assert_eq!(found.po.term_key, "widget");
    assert_eq!(found.po.required_fields, r#"["a","b"]"#);
}

/// create_relation_type：引用不存在的实体类拒绝；类就位后成功
#[sqlx::test]
async fn test_create_relation_type_validates_class_refs(pool: SqlitePool) {
    let (hr, ctx) = init_test_env(pool);
    let onto = hr.ontology_domain();

    let bad = OntologyRelationType::from_po(OntologyRelationTypePo::new(
        "depends",
        "依赖",
        "依赖关系",
        r#"["ghost"]"#,
        "[]",
        1.0,
        None,
    ));
    let err = onto
        .create_relation_type(ctx.clone(), &bad)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("ghost"),
        "引用缺失实体类应 fail fast，实际: {err}"
    );

    let class = OntologyClass::from_po(OntologyClassPo::new("agent", "智能体", "x", "[]"));
    onto.create_class(ctx.clone(), &class).await.unwrap();
    let ok = OntologyRelationType::from_po(OntologyRelationTypePo::new(
        "depends",
        "依赖",
        "依赖关系",
        r#"["agent"]"#,
        r#"["agent"]"#,
        1.0,
        None,
    ));
    onto.create_relation_type(ctx.clone(), &ok).await.unwrap();
    let found = onto
        .get_relation_type(ctx.clone(), &ok.po.id)
        .await
        .unwrap()
        .expect("引用完整时创建成功");
    assert_eq!(found.po.term_key, "depends");
}

/// update_class / update_relation_type：目标 id 不存在返回 NotFound（DAO 静默成功语义由 Domain 补齐）
#[sqlx::test]
async fn test_update_missing_entity_returns_not_found(pool: SqlitePool) {
    let (hr, ctx) = init_test_env(pool);
    let onto = hr.ontology_domain();

    let class = OntologyClass::from_po(OntologyClassPo::new("widget", "小部件", "x", "[]"));
    let err = onto.update_class(ctx.clone(), &class).await.unwrap_err();
    assert!(
        err.to_string().contains("不存在"),
        "更新不存在的实体类应报 NotFound，实际: {err}"
    );

    let rt = OntologyRelationType::from_po(OntologyRelationTypePo::new(
        "depends", "依赖", "x", "[]", "[]", 1.0, None,
    ));
    let err = onto
        .update_relation_type(ctx.clone(), &rt)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("不存在"),
        "更新不存在的关系类型应报 NotFound，实际: {err}"
    );
}

/// create_synonym：target_kind 非法拒绝 + target 不存在拒绝 + raw_term 入库归一化
#[sqlx::test]
async fn test_create_synonym_validates_and_normalizes(pool: SqlitePool) {
    let (hr, ctx) = init_test_env(pool);
    let onto = hr.ontology_domain();

    let class = OntologyClass::from_po(OntologyClassPo::new("document", "文档", "x", "[]"));
    onto.create_class(ctx.clone(), &class).await.unwrap();

    let bad_kind =
        OntologySynonymMapping::from_po(OntologySynonymMappingPo::new("对象", "edge", "document"));
    let err = onto
        .create_synonym(ctx.clone(), &bad_kind)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("target_kind"),
        "非法 target_kind 应被拒绝，实际: {err}"
    );

    let bad_target =
        OntologySynonymMapping::from_po(OntologySynonymMappingPo::new("对象", "class", "ghost"));
    let err = onto
        .create_synonym(ctx.clone(), &bad_target)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("ghost"),
        "映射目标不存在应被拒绝，实际: {err}"
    );

    let ok = OntologySynonymMapping::from_po(OntologySynonymMappingPo::new(
        "  OBJECT ",
        "class",
        "document",
    ));
    onto.create_synonym(ctx.clone(), &ok).await.unwrap();

    let page = onto
        .list_synonyms(
            ctx.clone(),
            OntologySynonymQuery {
                target_kind: None,
                target_key: None,
                keyword: None,
                pagination: all_pagination(),
            },
        )
        .await
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(
        page.items[0].po.raw_term, "object",
        "raw_term 入库前统一 trim + 小写"
    );
}

/// apply_default_lexicon 幂等（P2-1 验收核心）：首次全插入，二次 0 重复行全 skipped
#[sqlx::test]
async fn test_apply_default_lexicon_idempotent(pool: SqlitePool) {
    let (hr, ctx) = init_test_env(pool);
    let onto = hr.ontology_domain();
    let preset = preset_lexicon();

    let first = onto
        .apply_default_lexicon(ctx.clone(), &preset)
        .await
        .unwrap();
    assert_eq!(
        first.inserted_classes,
        ["agent", "document"],
        "term_key 归一化后按序插入"
    );
    assert_eq!(first.inserted_relation_types, ["contains"]);
    assert_eq!(first.inserted_synonyms, 1);
    assert_eq!(first.skipped, 0);

    let second = onto
        .apply_default_lexicon(ctx.clone(), &preset)
        .await
        .unwrap();
    assert!(second.inserted_classes.is_empty(), "二次注入不再新增实体类");
    assert!(
        second.inserted_relation_types.is_empty(),
        "二次注入不再新增关系类型"
    );
    assert_eq!(second.inserted_synonyms, 0, "二次注入不再新增同义映射");
    assert_eq!(second.skipped, 4, "三段合计 4 条全部跳过");

    // 库内行数不变（「重复调用 0 重复行」的直接证据）
    let classes = onto
        .list_classes(
            ctx.clone(),
            OntologyClassQuery {
                status: None,
                keyword: None,
                pagination: all_pagination(),
            },
        )
        .await
        .unwrap();
    assert_eq!(classes.total, 2);
    let relations = onto
        .list_relation_types(
            ctx.clone(),
            OntologyRelationTypeQuery {
                status: None,
                keyword: None,
                pagination: all_pagination(),
            },
        )
        .await
        .unwrap();
    assert_eq!(relations.total, 1);
}

/// apply_default_lexicon：预置快照引用不存在的实体类 fail fast（快照残缺不静默）
#[sqlx::test]
async fn test_apply_default_lexicon_rejects_missing_ref(pool: SqlitePool) {
    let (hr, ctx) = init_test_env(pool);
    let onto = hr.ontology_domain();

    let preset = PresetOntologyLexicon {
        classes: vec![],
        relation_types: vec![PresetOntologyRelationType {
            term_key: "depends".to_string(),
            display_name: "依赖".to_string(),
            description: "依赖关系".to_string(),
            domain_classes: vec!["ghost".to_string()],
            range_classes: vec![],
            weight_base: None,
            inverse_key: None,
        }],
        synonym_mappings: vec![],
    };
    let err = onto
        .apply_default_lexicon(ctx.clone(), &preset)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("ghost"),
        "快照残缺应 fail fast，实际: {err}"
    );
}

/// certify 软门禁（A7）：规范词节点置位 published；Drift 词调用成功且零写动作
#[sqlx::test]
async fn test_certify_memory_terms_soft_gate(pool: SqlitePool) {
    let (hr, ctx) = init_test_env(pool.clone());
    let onto = hr.ontology_domain();

    // 词表就位（含书写形变体：入库归一化 "Agent" → "agent"）
    onto.apply_default_lexicon(ctx.clone(), &preset_lexicon())
        .await
        .unwrap();

    // 造图谱节点：node_type 命中规范词（含空白变体）+ node_type 漂移
    let memory = crate::service::dao::memory::dao();
    let mk_node = |id: &str, ty: &str| LongTermKnowledgeNodePo {
        id: id.to_string(),
        agent_id: "cert-agent".to_string(),
        node_name: id.to_string(),
        node_description: String::new(),
        node_type: ty.to_string(),
        summary: String::new(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: 1,
        updated_at: 1,
    };
    memory
        .save_knowledge_node(ctx.clone(), &mk_node("node-canonical", "Agent "))
        .await
        .unwrap();
    memory
        .save_knowledge_node(ctx.clone(), &mk_node("node-drift", "widget"))
        .await
        .unwrap();

    let report = onto
        .certify_memory_terms(
            ctx.clone(),
            Some("cert-agent".to_string()),
            vec![
                (TermKind::Class, "Agent".to_string()),
                (TermKind::Class, "widget".to_string()),
                (TermKind::Relation, "contains".to_string()),
            ],
        )
        .await
        .unwrap();

    assert_eq!(
        report.canonical_count(),
        2,
        "agent 规范命中 + contains 规范命中"
    );
    assert_eq!(report.drift_count(), 1, "widget 词表外判漂移");
    assert_eq!(report.drift_terms(), ["widget"]);
    assert_eq!(
        report.certified_keys(),
        ["agent", "contains"],
        "认证键跳过漂移、去重保序"
    );

    // A7：命中词节点已置位；Drift 词 certify 调用成功且节点保持待复核
    let published = memory
        .get_knowledge_node(ctx.clone(), "node-canonical")
        .await
        .unwrap()
        .expect("节点存在");
    assert!(published.is_published, "规范词命中节点 is_published 置位");
    let drift = memory
        .get_knowledge_node(ctx.clone(), "node-drift")
        .await
        .unwrap()
        .expect("节点存在");
    assert!(!drift.is_published, "漂移词零写动作，保持待复核语义");
}

/// list_lexicon：Active 过滤（退役词不进提示词视图）+ 同义样例字段映射
#[sqlx::test]
async fn test_list_lexicon_active_view(pool: SqlitePool) {
    let (hr, ctx) = init_test_env(pool);
    let onto = hr.ontology_domain();

    onto.apply_default_lexicon(ctx.clone(), &preset_lexicon())
        .await
        .unwrap();

    let view = onto.list_lexicon(ctx.clone()).await.unwrap();
    assert_eq!(view.classes.len(), 2);
    assert_eq!(view.relation_types.len(), 1);
    assert_eq!(view.synonyms.len(), 1);
    assert_eq!(view.synonyms[0].target_kind, TermKind::Relation);
    assert_eq!(
        view.synonyms[0].raw_term, "contains",
        "同义样例带归一化原文"
    );

    // 退役一个实体类 → 视图排除（只取 Active）
    let classes = onto
        .list_classes(
            ctx.clone(),
            OntologyClassQuery {
                status: None,
                keyword: None,
                pagination: all_pagination(),
            },
        )
        .await
        .unwrap();
    let target = classes
        .items
        .iter()
        .find(|e| e.po.term_key == "document")
        .expect("预置实体类存在");
    onto.retire_class(ctx.clone(), &target.po.id).await.unwrap();

    let view = onto.list_lexicon(ctx.clone()).await.unwrap();
    assert_eq!(view.classes.len(), 1, "退役词不进提示词注入视图");
    assert_eq!(view.classes[0].term_key, "agent");
    assert_eq!(view.total_count(), 3);
}

/// get_drift_dashboard 聚合口径 + P5-2 端到端验收链：
/// 注入词表 → 沉淀规范/漂移边与节点 → 看板口径（覆盖率 1/3、Top 漂移词排序）
/// → 二次注入补同义映射 → 历史漂移自动愈合（覆盖率回升 1.0）
#[sqlx::test]
async fn test_drift_dashboard_coverage_lifecycle(pool: SqlitePool) {
    let (hr, ctx) = init_test_env(pool.clone());
    let onto = hr.ontology_domain();
    let memory = crate::service::dao::memory::dao();

    let first = onto
        .apply_default_lexicon(ctx.clone(), &preset_lexicon())
        .await
        .unwrap();
    assert_eq!(first.skipped, 0, "首次注入全量新增");

    let mk_node = |id: &str, ty: &str| LongTermKnowledgeNodePo {
        id: id.to_string(),
        agent_id: "dash-agent".to_string(),
        node_name: id.to_string(),
        node_description: String::new(),
        node_type: ty.to_string(),
        summary: String::new(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: 1,
        updated_at: 1,
    };
    let mk_relation = |id: &str, ty: &str, source: &str, target: &str| KnowledgeNodeRelationPo {
        id: id.to_string(),
        source_node_id: source.to_string(),
        target_node_id: target.to_string(),
        relation_type: ty.to_string(),
        weight: None,
        status: KnowledgeRelationStatus::Active,
        created_at: 1,
        updated_at: 1,
    };
    for (id, ty) in [
        ("dash-node-1", "Agent "),
        ("dash-node-2", "Agent"),
        ("dash-node-3", "widget"),
    ] {
        memory
            .save_knowledge_node(ctx.clone(), &mk_node(id, ty))
            .await
            .unwrap();
    }
    memory
        .add_knowledge_relation(
            ctx.clone(),
            &mk_relation("dash-r1", "Contains ", "dash-node-1", "dash-node-2"),
        )
        .await
        .unwrap();
    memory
        .add_knowledge_relation(
            ctx.clone(),
            &mk_relation("dash-r2", "connected_to", "dash-node-1", "dash-node-2"),
        )
        .await
        .unwrap();
    // r3 与 r2 归一后同键（同 source/target/type）：写入侧归一使大小写变体
    // 命中同一条部分唯一索引，r3 顶替 r2 成为唯一生效边（顶替语义回归锚点）
    memory
        .add_knowledge_relation(
            ctx.clone(),
            &mk_relation("dash-r3", "CONNECTED_TO", "dash-node-1", "dash-node-2"),
        )
        .await
        .unwrap();
    // r4 为独立边：补足「同词 2 条漂移边」的词频聚合口径
    memory
        .add_knowledge_relation(
            ctx.clone(),
            &mk_relation("dash-r4", "connected_to", "dash-node-2", "dash-node-3"),
        )
        .await
        .unwrap();

    // 看板口径一：1 规范边 + 2 漂移边（写入侧已归一，同词异写按独立边聚合）+ 1 漂移节点
    let dashboard = onto
        .get_drift_dashboard(
            ctx.clone(),
            GetDriftDashboardRequest {
                top_n: Some(10),
                agent_id: None,
            },
        )
        .await
        .unwrap();
    let cov = dashboard.relation_coverage;
    assert_eq!(cov.canonical_relations, 1);
    assert_eq!(cov.via_synonym_relations, 0);
    assert_eq!(cov.drift_relations, 2);
    assert!((cov.coverage_ratio - 1.0 / 3.0).abs() < 1e-9);
    assert_eq!(dashboard.total_node_count, 3);
    assert_eq!(dashboard.drift_node_count, 1);
    let words: Vec<(&str, TermKind)> = dashboard
        .top_drift_words
        .iter()
        .map(|w| (w.raw_term.as_str(), w.kind))
        .collect();
    assert_eq!(
        words,
        [
            ("connected_to", TermKind::Relation),
            ("widget", TermKind::Class)
        ],
        "Top 漂移词按词频降序，大小写原文归一合并"
    );

    // 词表修订：补同义映射（connected_to→contains、widget→document）→ 仅新增不重插
    let mut patched = preset_lexicon();
    patched.synonym_mappings.push(PresetOntologySynonym {
        raw_term: "CONNECTED_TO".to_string(),
        target_kind: TermKind::Relation,
        target_key: "contains".to_string(),
    });
    patched.synonym_mappings.push(PresetOntologySynonym {
        raw_term: "widget".to_string(),
        target_kind: TermKind::Class,
        target_key: "document".to_string(),
    });
    let second = onto
        .apply_default_lexicon(ctx.clone(), &patched)
        .await
        .unwrap();
    assert_eq!(second.inserted_synonyms, 2, "幂等注入只补新增同义映射");
    assert_eq!(second.skipped, 4, "已有 4 条全部跳过");

    // 看板口径二：解析以当前词表为参数，历史漂移自动愈合
    let dashboard = onto
        .get_drift_dashboard(
            ctx.clone(),
            GetDriftDashboardRequest {
                top_n: Some(10),
                agent_id: None,
            },
        )
        .await
        .unwrap();
    let cov = dashboard.relation_coverage;
    assert_eq!(cov.canonical_relations, 1);
    assert_eq!(cov.via_synonym_relations, 2, "漂移边经同义映射归一");
    assert_eq!(cov.drift_relations, 0);
    assert!((cov.coverage_ratio - 1.0).abs() < 1e-9, "覆盖率回升至 100%");
    assert_eq!(dashboard.drift_node_count, 0);
    assert!(dashboard.top_drift_words.is_empty());
}
