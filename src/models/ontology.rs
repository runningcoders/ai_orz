//! 本体词表实体（3 张全局表）
//!
//! 对应建表语句：`migrations/20260918000002_create_ontology_tables.sql`
//!
//! 分层约定：PO 只在 DAO/DAL 层流转；Domain 层及以上只接触 Entity
//! （`OntologyClass` / `OntologyRelationType` / `OntologySynonymMapping`）。
//! 词表条目是纯数据实体（无搜索匹配等附加信息），Entity 采用与
//! `ModelProvider` 相同的「持 PO」形态，`from_po` / `into_po` 往返无损。

use common::enums::OntologyStatus;
use common::ontology::TermKind;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

// ==================== 实体类词表 ====================

/// 实体类词表 PO（TBox：图谱节点类型）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct OntologyClassPo {
    /// 唯一 ID
    pub id: String,
    /// 语义锚点（snake_case 规范词），跨表/跨环境引用一律用它
    pub term_key: String,
    /// 展示名（管理页与图谱渲染使用）
    pub display_name: String,
    /// 语义说明（该实体类指什么、何时使用）
    pub description: String,
    /// 实体类必备字段清单（JSON 数组字符串），写后校验的判据之一
    pub required_fields: String,
    /// 条目状态（1 正常 / 0 退役；退役 ≠ 删除，历史存量引用仍可解释）
    pub status: OntologyStatus,
    /// 创建时间戳（秒级）
    pub created_at: i64,
    /// 更新时间戳（秒级）
    pub updated_at: i64,
}

impl OntologyClassPo {
    /// 创建实体类词条（id / status / 时间戳自动填充）
    pub fn new(
        term_key: impl Into<String>,
        display_name: impl Into<String>,
        description: impl Into<String>,
        required_fields: impl Into<String>,
    ) -> Self {
        let now = common::constants::utils::current_timestamp();
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            term_key: term_key.into(),
            display_name: display_name.into(),
            description: description.into(),
            required_fields: required_fields.into(),
            status: OntologyStatus::default(),
            created_at: now,
            updated_at: now,
        }
    }

    /// 解析必备字段清单（JSON 数组）；空串或非法 JSON 视为空清单
    pub fn parse_required_fields(&self) -> Vec<String> {
        serde_json::from_str(&self.required_fields).unwrap_or_default()
    }
}

/// 实体类词表 Entity（Domain 层及以上唯一可见形态）
#[derive(Debug, Clone)]
pub struct OntologyClass {
    pub po: OntologyClassPo,
}

impl OntologyClass {
    /// 从 PO 创建业务实体
    pub fn from_po(po: OntologyClassPo) -> Self {
        Self { po }
    }

    /// 转回 PO（DAO 写入用；往返无损）
    pub fn into_po(self) -> OntologyClassPo {
        self.po
    }

    /// 语义锚点
    pub fn term_key(&self) -> &str {
        &self.po.term_key
    }

    /// 展示名
    pub fn display_name(&self) -> &str {
        &self.po.display_name
    }

    /// 条目状态
    pub fn status(&self) -> OntologyStatus {
        self.po.status
    }
}

// ==================== 关系类型词表 ====================

/// 关系类型词表 PO（TBox：图谱边类型）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct OntologyRelationTypePo {
    /// 唯一 ID
    pub id: String,
    /// 语义锚点（snake_case 规范词），跨表/跨环境引用一律用它
    pub term_key: String,
    /// 展示名（管理页与图谱渲染使用）
    pub display_name: String,
    /// 语义说明（这条关系表达什么、头尾各是什么）
    pub description: String,
    /// 头实体类约束（JSON 数组字符串；空数组 = 不约束）
    pub domain_classes: String,
    /// 尾实体类约束（JSON 数组字符串；空数组 = 不约束）
    pub range_classes: String,
    /// 关系权重基线，映射到图谱边 `weight` 的校准参考
    pub weight_base: f64,
    /// 逆向关系词（如 contains ↔ contained_by），可空
    pub inverse_key: Option<String>,
    /// 条目状态（1 正常 / 0 退役；退役 ≠ 删除，历史存量引用仍可解释）
    pub status: OntologyStatus,
    /// 创建时间戳（秒级）
    pub created_at: i64,
    /// 更新时间戳（秒级）
    pub updated_at: i64,
}

impl OntologyRelationTypePo {
    /// 创建关系类型词条（id / status / 时间戳自动填充）
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        term_key: impl Into<String>,
        display_name: impl Into<String>,
        description: impl Into<String>,
        domain_classes: impl Into<String>,
        range_classes: impl Into<String>,
        weight_base: f64,
        inverse_key: Option<String>,
    ) -> Self {
        let now = common::constants::utils::current_timestamp();
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            term_key: term_key.into(),
            display_name: display_name.into(),
            description: description.into(),
            domain_classes: domain_classes.into(),
            range_classes: range_classes.into(),
            weight_base,
            inverse_key,
            status: OntologyStatus::default(),
            created_at: now,
            updated_at: now,
        }
    }

    /// 解析头实体类约束；空串或非法 JSON 视为不约束
    pub fn parse_domain_classes(&self) -> Vec<String> {
        serde_json::from_str(&self.domain_classes).unwrap_or_default()
    }

    /// 解析尾实体类约束；空串或非法 JSON 视为不约束
    pub fn parse_range_classes(&self) -> Vec<String> {
        serde_json::from_str(&self.range_classes).unwrap_or_default()
    }
}

/// 关系类型词表 Entity（Domain 层及以上唯一可见形态）
#[derive(Debug, Clone)]
pub struct OntologyRelationType {
    pub po: OntologyRelationTypePo,
}

impl OntologyRelationType {
    /// 从 PO 创建业务实体
    pub fn from_po(po: OntologyRelationTypePo) -> Self {
        Self { po }
    }

    /// 转回 PO（DAO 写入用；往返无损）
    pub fn into_po(self) -> OntologyRelationTypePo {
        self.po
    }

    /// 语义锚点
    pub fn term_key(&self) -> &str {
        &self.po.term_key
    }

    /// 展示名
    pub fn display_name(&self) -> &str {
        &self.po.display_name
    }

    /// 条目状态
    pub fn status(&self) -> OntologyStatus {
        self.po.status
    }
}

// ==================== 同义映射 ====================

/// 同义映射 PO（漂移修复手段：旧词/别名 → 规范词）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct OntologySynonymMappingPo {
    /// 唯一 ID
    pub id: String,
    /// 被映射的原始词（Agent 实际写出的写法）
    pub raw_term: String,
    /// 映射目标种类（`class` / `relation`；同一 raw 词可分别映射到两类）
    pub target_kind: String,
    /// 映射目标的语义锚点（term_key）
    pub target_key: String,
    /// 创建时间戳（秒级）
    pub created_at: i64,
}

impl OntologySynonymMappingPo {
    /// 创建同义映射（id / 时间戳自动填充）
    pub fn new(
        raw_term: impl Into<String>,
        target_kind: impl Into<String>,
        target_key: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            raw_term: raw_term.into(),
            target_kind: target_kind.into(),
            target_key: target_key.into(),
            created_at: common::constants::utils::current_timestamp(),
        }
    }

    /// 解析映射目标种类；非法值返回 `None`（写入路径已校验，兜底脏数据）
    pub fn kind(&self) -> Option<TermKind> {
        self.target_kind.parse().ok()
    }
}

/// 同义映射 Entity（Domain 层及以上唯一可见形态）
#[derive(Debug, Clone)]
pub struct OntologySynonymMapping {
    pub po: OntologySynonymMappingPo,
}

impl OntologySynonymMapping {
    /// 从 PO 创建业务实体
    pub fn from_po(po: OntologySynonymMappingPo) -> Self {
        Self { po }
    }

    /// 转回 PO（DAO 写入用；往返无损）
    pub fn into_po(self) -> OntologySynonymMappingPo {
        self.po
    }

    /// 被映射的原始词
    pub fn raw_term(&self) -> &str {
        &self.po.raw_term
    }

    /// 映射目标的语义锚点
    pub fn target_key(&self) -> &str {
        &self.po.target_key
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_po_entity_roundtrip_lossless() {
        let po = OntologyClassPo::new(
            "document",
            "文档",
            "结构化知识载体",
            r#"["title","content"]"#,
        );
        let entity = OntologyClass::from_po(po.clone());
        assert_eq!(entity.term_key(), "document");
        assert_eq!(entity.display_name(), "文档");
        assert_eq!(entity.status(), OntologyStatus::Active);
        assert_eq!(entity.into_po(), po);
    }

    #[test]
    fn relation_po_entity_roundtrip_lossless() {
        let po = OntologyRelationTypePo::new(
            "contains",
            "包含",
            "整体包含部分",
            r#"["document"]"#,
            "".to_string(),
            1.0,
            Some("contained_by".to_string()),
        );
        let entity = OntologyRelationType::from_po(po.clone());
        assert_eq!(entity.term_key(), "contains");
        assert_eq!(entity.into_po(), po);
        assert_eq!(po.parse_domain_classes(), vec!["document".to_string()]);
        assert!(po.parse_range_classes().is_empty());
    }

    #[test]
    fn synonym_po_entity_roundtrip_lossless() {
        let po = OntologySynonymMappingPo::new("包含", "relation", "contains");
        assert_eq!(po.kind(), Some(TermKind::Relation));
        let entity = OntologySynonymMapping::from_po(po.clone());
        assert_eq!(entity.raw_term(), "包含");
        assert_eq!(entity.target_key(), "contains");
        assert_eq!(entity.into_po(), po);
    }

    #[test]
    fn invalid_target_kind_parses_to_none() {
        let po = OntologySynonymMappingPo::new("包含", "edge", "contains");
        assert_eq!(po.kind(), None);
    }

    #[test]
    fn malformed_json_list_parses_to_empty() {
        let po = OntologyClassPo::new("document", "文档", "结构化知识载体", "not-json");
        assert!(po.parse_required_fields().is_empty());
    }

    #[test]
    fn new_fills_defaults() {
        let po = OntologyClassPo::new("document", "文档", "结构化知识载体", "[]");
        assert!(!po.id.is_empty());
        assert_eq!(po.status, OntologyStatus::Active);
        assert!(po.created_at > 0);
        assert_eq!(po.created_at, po.updated_at);
    }
}
