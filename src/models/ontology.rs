//! 本体词表实体（3 张全局表）
//!
//! 对应建表语句：`migrations/20260918000002_create_ontology_tables.sql`
//!
//! 分层约定：PO 只在 DAO/DAL 层流转；Domain 层及以上只接触 Entity
//! （`OntologyClass` / `OntologyRelationType` / `OntologySynonymMapping`）。
//! 词表条目是纯数据实体（无搜索匹配等附加信息），Entity 采用与
//! `ModelProvider` 相同的「持 PO」形态，`from_po` / `into_po` 往返无损。

use common::enums::OntologyStatus;
use common::enums::ontology::PropertyTargetType;
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
    /// 父类 term_key（可空 = 根类）；骨架有效约束沿继承链向上解析
    pub parent_term_key: Option<String>,
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
            parent_term_key: None,
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
    /// 关系权重基线，映射到图谱边 `weight` 的校准参考
    pub weight_base: f64,
    /// 逆向关系词（如 contains ↔ contained_by），可空
    pub inverse_key: Option<String>,
    /// 方向二值（"directed" / "undirected"；与 migration CHECK 同口径）
    pub direction: String,
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
        weight_base: f64,
        inverse_key: Option<String>,
        direction: &str,
    ) -> Self {
        let now = common::constants::utils::current_timestamp();
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            term_key: term_key.into(),
            display_name: display_name.into(),
            description: description.into(),
            weight_base,
            inverse_key,
            direction: direction.to_string(),
            status: OntologyStatus::default(),
            created_at: now,
            updated_at: now,
        }
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

// ==================== 骨架三元组（TBox：类-关系-类） ====================

/// 本体骨架三元组 PO（TBox：一条「主语类 → 关系 → 宾语类」约束）
///
/// 替代此前压在 `ontology_relation_types.domain_classes / range_classes` 两个
/// JSON 数组里的骨架信息：独立成行后才能按任意一侧反查、承载边级基数/必填。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct OntologyClassRelationPo {
    /// 唯一 ID
    pub id: String,
    /// 主语实体类 term_key
    pub subject_class_key: String,
    /// 关系类型 term_key
    pub relation_key: String,
    /// 宾语实体类 term_key（`*` 表示不限）
    pub object_class_key: String,
    /// 边基数（"single" / "many"）
    pub cardinality: String,
    /// 是否必填（0 可选 / 1 必填）
    pub required: i32,
    /// 条目状态（1 正常 / 0 退役）
    pub status: OntologyStatus,
    /// 创建时间戳（秒级）
    pub created_at: i64,
    /// 更新时间戳（秒级）
    pub updated_at: i64,
}

impl OntologyClassRelationPo {
    /// 创建三元组（id / status / 时间戳自动填充）
    pub fn new(
        subject_class_key: impl Into<String>,
        relation_key: impl Into<String>,
        object_class_key: impl Into<String>,
        cardinality: impl Into<String>,
        required: bool,
    ) -> Self {
        let now = common::constants::utils::current_timestamp();
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            subject_class_key: subject_class_key.into(),
            relation_key: relation_key.into(),
            object_class_key: object_class_key.into(),
            cardinality: cardinality.into(),
            required: i32::from(required),
            status: OntologyStatus::default(),
            created_at: now,
            updated_at: now,
        }
    }

    /// 是否必填
    pub fn is_required(&self) -> bool {
        self.required != 0
    }
}

/// 骨架三元组 Entity（Domain 层及以上唯一可见形态）
#[derive(Debug, Clone)]
pub struct OntologyClassRelation {
    pub po: OntologyClassRelationPo,
}

impl OntologyClassRelation {
    /// 从 PO 创建业务实体
    pub fn from_po(po: OntologyClassRelationPo) -> Self {
        Self { po }
    }

    /// 转回 PO（DAO 写入用；往返无损）
    pub fn into_po(self) -> OntologyClassRelationPo {
        self.po
    }

    /// 主语实体类 term_key
    pub fn subject_class_key(&self) -> &str {
        &self.po.subject_class_key
    }

    /// 关系类型 term_key
    pub fn relation_key(&self) -> &str {
        &self.po.relation_key
    }

    /// 宾语实体类 term_key
    pub fn object_class_key(&self) -> &str {
        &self.po.object_class_key
    }
}

// ==================== 属性定义词表 ====================

/// 属性定义词表 PO（TBox：属性类型）
///
/// 补齐本体缺失的属性维度：此前 `OntologyClassPo::required_fields` 只是字段名
/// 字符串数组（无类型、无值域、无基数，且零校验消费者），ABox 也没有属性落点，
/// 只能用 `attribute_of` / `value_of` 两条关系词「用边模拟属性」。
/// 本表定义「属性是什么」，实例值落在通用表 `knowledge_property`（ABox）。
///
/// 取值约束与类型见 [`common::ontology_property`]（校验纯函数层）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct OntologyPropertyPo {
    /// 唯一 ID
    pub id: String,
    /// 语义锚点（snake_case 规范词），跨表/跨环境引用一律用它
    pub term_key: String,
    /// 展示名（管理页与属性编辑器使用）
    pub display_name: String,
    /// 语义说明（这个属性指什么、何时使用）
    pub description: String,
    /// 适用实体类 term_key 清单（JSON 数组字符串；空数组 = 不限定实体类）
    pub domain_classes: String,
    /// 允许挂载的载体类型（JSON 数组字符串，取值子集：`node` / `relation`）
    ///
    /// 泛化属性值表带来的配套约束：同一 term_key 挂节点与挂边可能不是同一语义，
    /// 故在 TBox 侧声明适用载体，写侧据此拦截（判定纯函数
    /// `common::ontology_property::allows_target`）。空清单 = 不限载体。
    pub applies_to: String,
    /// 值类型（"string"/"number"/"boolean"/"date"/"reference"/"enum"；
    /// 与 DDL CHECK 同口径）
    pub data_type: String,
    /// 取值约束（JSON：enum 型为白名单数组 / number 型为 {"min","max"}；
    /// 空串视为无约束）
    pub value_range: String,
    /// 基数（"single" / "many"），决定 ABox 侧 ordinal 槽位用法
    pub cardinality: String,
    /// 计量单位（可空；仅 number 型有意义，如 "ms" / "percent"）
    pub unit: Option<String>,
    /// 是否必填（0 可选 / 1 必填），写后校验判据之一
    pub required: i32,
    /// 条目状态（1 正常 / 0 退役；退役 ≠ 删除，历史存量值仍可解释）
    pub status: OntologyStatus,
    /// 创建时间戳（秒级）
    pub created_at: i64,
    /// 更新时间戳（秒级）
    pub updated_at: i64,
}

impl OntologyPropertyPo {
    /// 创建属性定义词条（id / status / 时间戳自动填充）
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        term_key: impl Into<String>,
        display_name: impl Into<String>,
        description: impl Into<String>,
        domain_classes: impl Into<String>,
        applies_to: impl Into<String>,
        data_type: &str,
        value_range: impl Into<String>,
        cardinality: &str,
        unit: Option<String>,
        required: bool,
    ) -> Self {
        let now = common::constants::utils::current_timestamp();
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            term_key: term_key.into(),
            display_name: display_name.into(),
            description: description.into(),
            domain_classes: domain_classes.into(),
            applies_to: applies_to.into(),
            data_type: data_type.to_string(),
            value_range: value_range.into(),
            cardinality: cardinality.to_string(),
            unit,
            required: i32::from(required),
            status: OntologyStatus::default(),
            created_at: now,
            updated_at: now,
        }
    }

    /// 解析适用实体类清单；空串或非法 JSON 视为不限定
    pub fn parse_domain_classes(&self) -> Vec<String> {
        serde_json::from_str(&self.domain_classes).unwrap_or_default()
    }

    /// 解析适用载体清单；空串 / 非法 JSON → 空清单 = 不限载体
    ///
    /// ⚠️ 数组内拼错的载体名会被静默丢弃，结果是谁都不让挂。
    pub fn applies_to(&self) -> Vec<PropertyTargetType> {
        common::ontology_property::parse_applies_to(&self.applies_to)
    }

    /// 该属性是否允许挂到指定载体（空清单 = 不限）
    pub fn allows_target(&self, target: PropertyTargetType) -> bool {
        common::ontology_property::allows_target(&self.applies_to(), target)
    }

    /// 该属性是否允许挂到指定实体类（`domain_classes`；空清单 = 不限）
    ///
    /// 与 [`Self::allows_target`] 同语义：声明为空 = 未声明 = 不限定；
    /// 非空清单必须命中。判定是纯计算，取数（属性定义 / 节点类型）由调用方完成
    /// ——DAL 写入链路据此做**软门禁**：实体类域不符只记 issue，照常写入。
    pub fn allows_domain_class(&self, class_key: &str) -> bool {
        let classes = self.parse_domain_classes();
        classes.is_empty() || classes.iter().any(|c| c == class_key)
    }

    /// 解析值类型；脏数据兜底退 `string`（与本体软门禁同口径，不 panic 不阻断）
    pub fn data_type(&self) -> common::enums::ontology::PropertyDataType {
        common::enums::ontology::PropertyDataType::parse_or_default(&self.data_type)
    }

    /// 解析基数；脏数据兜底退 `single`（保守取值，避免放开写入量）
    pub fn cardinality(&self) -> common::enums::ontology::Cardinality {
        common::enums::ontology::Cardinality::parse_or_default(&self.cardinality)
    }

    /// 解析取值约束（非法 JSON / 空串 → 无约束）
    pub fn value_range(&self) -> common::ontology_property::ValueRange {
        common::ontology_property::parse_value_range(&self.value_range)
    }

    /// 是否必填
    pub fn is_required(&self) -> bool {
        self.required != 0
    }
}

/// 属性定义 Entity（Domain 层及以上唯一可见形态）
#[derive(Debug, Clone)]
pub struct OntologyProperty {
    pub po: OntologyPropertyPo,
}

impl OntologyProperty {
    /// 从 PO 创建业务实体
    pub fn from_po(po: OntologyPropertyPo) -> Self {
        Self { po }
    }

    /// 转回 PO（DAO 写入用；往返无损）
    pub fn into_po(self) -> OntologyPropertyPo {
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
    use common::enums::ontology::{Cardinality, PropertyDataType};

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
            1.0,
            Some("contained_by".to_string()),
            "directed",
        );
        let entity = OntologyRelationType::from_po(po.clone());
        assert_eq!(entity.term_key(), "contains");
        assert_eq!(entity.into_po(), po);
        assert_eq!(po.direction, "directed");
    }

    #[test]
    fn class_relation_po_entity_roundtrip_lossless() {
        let po = OntologyClassRelationPo::new("document", "contains", "concept", "many", true);
        let entity = OntologyClassRelation::from_po(po.clone());
        assert_eq!(entity.subject_class_key(), "document");
        assert_eq!(entity.relation_key(), "contains");
        assert_eq!(entity.object_class_key(), "concept");
        assert!(po.is_required());
        assert_eq!(entity.into_po(), po);
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

    #[test]
    fn property_po_entity_roundtrip_lossless() {
        let po = OntologyPropertyPo::new(
            "model_name",
            "模型名",
            "绑定的模型标识",
            r#"["agent"]"#,
            r#"["node"]"#,
            "string",
            "[]",
            "single",
            None,
            false,
        );
        let entity = OntologyProperty::from_po(po.clone());
        assert_eq!(entity.term_key(), "model_name");
        assert_eq!(entity.display_name(), "模型名");
        assert_eq!(entity.status(), OntologyStatus::Active);
        assert_eq!(entity.into_po(), po);
    }

    #[test]
    fn property_required_flag_maps_to_int_column() {
        let required = OntologyPropertyPo::new(
            "a", "A", "d", "[]", "[]", "string", "", "single", None, true,
        );
        assert_eq!(required.required, 1);
        assert!(required.is_required());

        let optional = OntologyPropertyPo::new(
            "b", "B", "d", "[]", "[]", "string", "", "single", None, false,
        );
        assert_eq!(optional.required, 0);
        assert!(!optional.is_required());
        assert_eq!(optional.created_at, optional.updated_at);
    }

    #[test]
    fn property_parse_helpers_degrade_gracefully() {
        // 脏定义一律退化到「最宽松类型 + 最保守基数」，不 panic 不阻断写值链路
        let po = OntologyPropertyPo::new(
            "score",
            "分数",
            "质量分",
            "not-json",
            "not-json",
            "bogus",
            "not-json",
            "bogus",
            Some("percent".to_string()),
            false,
        );
        assert!(po.parse_domain_classes().is_empty());
        assert!(po.applies_to().is_empty());
        assert_eq!(po.data_type(), PropertyDataType::String);
        assert_eq!(po.cardinality(), Cardinality::Single);
        assert!(po.value_range().is_empty());
        assert_eq!(po.unit.as_deref(), Some("percent"));
    }

    #[test]
    fn property_parses_domain_classes_and_value_range() {
        let po = OntologyPropertyPo::new(
            "confidence",
            "置信度",
            "0~1 质量分",
            r#"["concept"]"#,
            r#"["node"]"#,
            "number",
            r#"{"min": 0.0, "max": 1.0}"#,
            "single",
            None,
            true,
        );
        assert_eq!(po.parse_domain_classes(), vec!["concept".to_string()]);
        assert_eq!(po.data_type(), PropertyDataType::Number);
        assert_eq!(po.value_range().max, Some(1.0));
    }

    /// 泛化表配套：属性定义声明适用载体，写侧据此拦截，
    /// 否则同一 term_key 挂节点与挂边的语义会漂移。
    #[test]
    fn property_applies_to_gates_target() {
        let node_only = OntologyPropertyPo::new(
            "model_name",
            "模型名",
            "绑定的模型标识",
            r#"["agent"]"#,
            r#"["node"]"#,
            "string",
            "[]",
            "single",
            None,
            false,
        );
        assert!(node_only.allows_target(PropertyTargetType::Node));
        assert!(!node_only.allows_target(PropertyTargetType::Relation));

        // 空清单 = 未声明 = 不限载体
        let unconstrained = OntologyPropertyPo::new(
            "effective_from",
            "生效时间",
            "起始时间",
            "[]",
            "[]",
            "date",
            "{}",
            "single",
            None,
            false,
        );
        assert!(unconstrained.allows_target(PropertyTargetType::Node));
        assert!(unconstrained.allows_target(PropertyTargetType::Relation));
    }
}
