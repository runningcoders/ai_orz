//! HR Ontology Handlers module
//!
//! 本体词表管理 HTTP 接口：
//! - 词表 CRUD：实体类 / 关系类型（term_key 创建后不可改，退役 = 软删除）
//! - 同义映射：漂移修复规则（raw_term 归一化入库，UNIQUE(raw_term, target_kind)）
//! - 词表视图：神经技能提示词注入裁剪视图（常驻神经工具）
//! - 漂移看板：Top N 漂移词 / 覆盖率 / 明细下钻
//!
//! 分层：handler 只做 DTO ↔ Entity 转换与调用编排（CODE_STANDARDS §7 双宏），
//! 业务校验（term_key 非空 / 引用完整性）在 Domain 层完成；归一化由 DAO 写入侧单点完成。

pub mod create_class;
pub mod create_relation_type;
pub mod create_synonym;
pub mod delete_synonym;
pub mod get_drift_dashboard;
pub mod list_classes;
pub mod list_drift_class_details;
pub mod list_drift_relation_details;
pub mod list_lexicon;
pub mod list_relation_types;
pub mod list_synonyms;
pub mod retire_class;
pub mod retire_relation_type;
pub mod update_class;
pub mod update_relation_type;

pub use create_class::create_ontology_class_handler;
pub use create_relation_type::create_ontology_relation_type_handler;
pub use create_synonym::create_ontology_synonym_handler;
pub use delete_synonym::delete_ontology_synonym_handler;
pub use get_drift_dashboard::get_ontology_drift_dashboard_handler;
pub use list_classes::list_ontology_classes_handler;
pub use list_drift_class_details::list_ontology_drift_class_details_handler;
pub use list_drift_relation_details::list_ontology_drift_relation_details_handler;
pub use list_lexicon::list_ontology_lexicon_handler;
pub use list_relation_types::list_ontology_relation_types_handler;
pub use list_synonyms::list_ontology_synonyms_handler;
pub use retire_class::retire_ontology_class_handler;
pub use retire_relation_type::retire_ontology_relation_type_handler;
pub use update_class::update_ontology_class_handler;
pub use update_relation_type::update_ontology_relation_type_handler;

use crate::models::ontology::{OntologyClassPo, OntologyRelationTypePo, OntologySynonymMappingPo};
use common::api::ontology::{OntologyClassItem, OntologyRelationTypeItem, OntologySynonymItem};
use common::ontology::TermKind;

/// 实体类 Entity → 列表项 DTO（JSON 数组字符串还原为清单）
pub(super) fn class_item(po: OntologyClassPo) -> OntologyClassItem {
    // 先借调解析器再 move 字段，避免 partial move
    let required_fields = po.parse_required_fields();
    OntologyClassItem {
        id: po.id,
        term_key: po.term_key,
        display_name: po.display_name,
        description: po.description,
        required_fields,
        status: po.status,
        created_at: po.created_at,
        updated_at: po.updated_at,
    }
}

/// 关系类型 Entity → 列表项 DTO（domain/range JSON 数组字符串还原为清单）
pub(super) fn relation_type_item(po: OntologyRelationTypePo) -> OntologyRelationTypeItem {
    // 先借调解析器再 move 字段，避免 partial move
    let domain_classes = po.parse_domain_classes();
    let range_classes = po.parse_range_classes();
    OntologyRelationTypeItem {
        id: po.id,
        term_key: po.term_key,
        display_name: po.display_name,
        description: po.description,
        domain_classes,
        range_classes,
        weight_base: po.weight_base,
        inverse_key: po.inverse_key,
        status: po.status,
        created_at: po.created_at,
        updated_at: po.updated_at,
    }
}

/// 同义映射 Entity → 列表项 DTO
///
/// `target_kind` 写入路径已强校验（domain `create_synonym` / `apply_default_lexicon`），
/// 脏数据理论不可达；兜底 Class 避免 histé 非法值导致接口 500。
pub(super) fn synonym_item(po: OntologySynonymMappingPo) -> OntologySynonymItem {
    // 先借调解析器再 move 字段，避免 partial move
    let target_kind = po.kind().unwrap_or(TermKind::Class);
    OntologySynonymItem {
        id: po.id,
        raw_term: po.raw_term,
        target_kind,
        target_key: po.target_key,
        created_at: po.created_at,
    }
}
