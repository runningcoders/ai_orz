//! 本体（Ontology）词表管理相关 API DTO
//!
//! DTO 单一事实源（前后端共用），覆盖六组接口契约：
//!
//! 1. **词表 CRUD**：实体类（`ontology_classes`，含 `parent_term_key` 继承链）
//!    与关系类型（`ontology_relation_types`）的新增 / 编辑 / 退役（软删除）。
//! 2. **本体骨架**：实体类继承链 + `ontology_class_relations` 三元组
//!    （主语类 - 关系词 - 宾语类，含基数 / 必填 / 状态）。约束语义：某关系无
//!    三元组 = 不约束；单侧为空用通配符 `*`；子类沿祖先链解析（纯函数、零物化）。
//! 3. **同义映射管理**：`ontology_synonym_mappings` 的增删查。
//! 4. **漂移看板**：Top N 漂移词 / 词表覆盖率 / 漂移节点数 / 属性域覆盖率（SQLite 读路径
//!    惰性聚合，"现在时"）+ 漂移趋势（DuckDB 事件流，"历史时"）+ 明细下钻。
//! 5. **seed 预置词表同步**：preview / sync 对（仅补缺，无覆盖策略——
//!    本体是共享约定，seed 不允许覆盖管理页的本地修改）。
//! 6. **属性有效性统计**：按 `(term_key, node_type)` 聚合取值分布，以**众数**
//!    （占比最高的取值，而非枚举首值）作默认值口径，`mode_ratio > 0.90` 标黄——
//!    识别「某属性在某实体类上事实上已退化成常量」的信号。
//!
//! 词条种类复用 [`crate::ontology::TermKind`]（单一事实源），JSON 形态为
//! 小写 `"class"` / `"relation"`，与统计事件 kind 口径一致。

use super::{PagedResult, PaginationParams};
use crate::enums::OntologyStatus;
use crate::enums::ontology::{Cardinality, PropertyDataType, PropertyTargetType};
use crate::ontology::TermKind;
use crate::ontology_property::ValueRange;
use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ==================== 词表 CRUD：实体类 ====================

/// 实体类条目（列表项 + 创建 / 更新 / 退役的返回体）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OntologyClassItem {
    /// 条目 ID
    pub id: String,
    /// 规范词 key（小写 snake_case，全库唯一；创建后不可改）
    pub term_key: String,
    /// 展示名
    pub display_name: String,
    /// 语义描述
    pub description: String,
    /// 实体必填属性清单
    pub required_fields: Vec<String>,
    /// 父类 term_key（None = 根类）；骨架有效约束沿继承链向上解析
    pub parent_term_key: Option<String>,
    /// 状态（正常 / 退役）
    pub status: OntologyStatus,
    /// 创建时间戳（秒）
    pub created_at: i64,
    /// 更新时间戳（秒）
    pub updated_at: i64,
}

/// 列出实体类请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListOntologyClassesRequest {
    /// 按状态筛选（None = 全部，含退役）
    #[serde(default)]
    #[param(source = "query")]
    pub status: Option<OntologyStatus>,
    /// 关键词（对 term_key / display_name 模糊匹配）
    #[serde(default)]
    #[param(source = "query")]
    pub keyword: Option<String>,
    /// 分页参数
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: PaginationParams,
}

/// 列出实体类响应（分页）
pub type ListOntologyClassesResponse = PagedResult<OntologyClassItem>;

/// 创建实体类请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateOntologyClassRequest {
    /// 规范词 key（小写 snake_case，重复创建报错）
    pub term_key: String,
    /// 展示名
    pub display_name: String,
    /// 语义描述
    pub description: String,
    /// 实体必填属性清单
    #[serde(default)]
    pub required_fields: Vec<String>,
    /// 父类 term_key（None = 根类）
    #[serde(default)]
    pub parent_term_key: Option<String>,
}

/// 创建实体类响应
pub type CreateOntologyClassResponse = OntologyClassItem;

/// 更新实体类请求
///
/// `term_key` 创建后不可改——改 key 等价于"废弃旧词 + 新增新词"，
/// 应走退役 + 新建流程以保留历史解析链路。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateOntologyClassRequest {
    /// 条目 ID
    pub id: String,
    /// 展示名
    pub display_name: String,
    /// 语义描述
    pub description: String,
    /// 实体必填属性清单
    #[serde(default)]
    pub required_fields: Vec<String>,
    /// 父类 term_key（None = 清空父级 / 根类）
    #[serde(default)]
    pub parent_term_key: Option<String>,
}

/// 更新实体类响应
pub type UpdateOntologyClassResponse = OntologyClassItem;

/// 退役实体类请求（软删除 status=0；历史存量引用仍可解释，不做物理删除）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct RetireOntologyClassRequest {
    /// 条目 ID
    #[param(source = "path")]
    pub id: String,
}

/// 退役实体类响应
pub type RetireOntologyClassResponse = OntologyClassItem;

// ==================== 词表 CRUD：属性定义 ====================
//
// 本体第 4 张词表（TBox）：定义「属性是什么」。实例值落在 `knowledge_property`
// （ABox），见 `crate::ontology_property`。

/// 属性定义条目（列表项 + 创建 / 更新 / 退役的返回体）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OntologyPropertyItem {
    /// 条目 ID
    pub id: String,
    /// 规范词 key（小写 snake_case，全库唯一；创建后不可改）
    pub term_key: String,
    /// 展示名
    pub display_name: String,
    /// 语义描述
    pub description: String,
    /// 适用实体类 term_key 清单（空 = 不限实体类）
    pub domain_classes: Vec<String>,
    /// 允许挂载的载体清单（空 = 节点与边皆可）
    pub applies_to: Vec<PropertyTargetType>,
    /// 值类型（决定属性值落 `knowledge_property` 的哪一列）
    pub data_type: PropertyDataType,
    /// 取值约束（数值区间 / 枚举白名单；全空 = 无约束）
    pub value_range: ValueRange,
    /// 基数（单值 / 多值）
    pub cardinality: Cardinality,
    /// 计量单位（仅 number 型有意义）
    pub unit: Option<String>,
    /// 是否必填
    pub required: bool,
    /// 状态（正常 / 退役）
    pub status: OntologyStatus,
    /// 创建时间戳（秒）
    pub created_at: i64,
    /// 更新时间戳（秒）
    pub updated_at: i64,
}

/// 列出属性定义请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListOntologyPropertiesRequest {
    /// 按状态筛选（None = 全部，含退役）
    #[serde(default)]
    #[param(source = "query")]
    pub status: Option<OntologyStatus>,
    /// 按适用实体类筛选
    #[serde(default)]
    #[param(source = "query")]
    pub domain_class: Option<String>,
    /// 按适用载体筛选（node / relation）
    #[serde(default)]
    #[param(source = "query")]
    pub target_type: Option<PropertyTargetType>,
    /// 关键词（对 term_key / display_name / description 模糊匹配）
    #[serde(default)]
    #[param(source = "query")]
    pub keyword: Option<String>,
    /// 分页参数
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: PaginationParams,
}

/// 列出属性定义响应（分页）
pub type ListOntologyPropertiesResponse = PagedResult<OntologyPropertyItem>;

/// 创建属性定义请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateOntologyPropertyRequest {
    /// 规范词 key（小写 snake_case，重复创建报错）
    pub term_key: String,
    /// 展示名
    pub display_name: String,
    /// 语义描述
    pub description: String,
    /// 适用实体类 term_key 清单（空 = 不限实体类）
    #[serde(default)]
    pub domain_classes: Vec<String>,
    /// 允许挂载的载体清单（空 = 节点与边皆可）
    #[serde(default)]
    pub applies_to: Vec<PropertyTargetType>,
    /// 值类型
    pub data_type: PropertyDataType,
    /// 取值约束（全空 = 无约束）
    #[serde(default)]
    pub value_range: ValueRange,
    /// 基数
    pub cardinality: Cardinality,
    /// 计量单位
    #[serde(default)]
    pub unit: Option<String>,
    /// 是否必填
    #[serde(default)]
    pub required: bool,
}

/// 创建属性定义响应
pub type CreateOntologyPropertyResponse = OntologyPropertyItem;

/// 更新属性定义请求
///
/// `term_key` 创建后不可改：它是历史属性值的语义锚点，改 key 等价于
/// 「废弃旧属性 + 新增新属性」，应走退役 + 新建流程。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateOntologyPropertyRequest {
    /// 条目 ID
    pub id: String,
    /// 展示名
    pub display_name: String,
    /// 语义描述
    pub description: String,
    /// 适用实体类 term_key 清单
    #[serde(default)]
    pub domain_classes: Vec<String>,
    /// 允许挂载的载体清单
    #[serde(default)]
    pub applies_to: Vec<PropertyTargetType>,
    /// 值类型
    pub data_type: PropertyDataType,
    /// 取值约束
    #[serde(default)]
    pub value_range: ValueRange,
    /// 基数
    pub cardinality: Cardinality,
    /// 计量单位
    #[serde(default)]
    pub unit: Option<String>,
    /// 是否必填
    #[serde(default)]
    pub required: bool,
}

/// 更新属性定义响应
pub type UpdateOntologyPropertyResponse = OntologyPropertyItem;

/// 退役属性定义请求（软删除 status=0）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct RetireOntologyPropertyRequest {
    /// 条目 ID
    #[param(source = "path")]
    pub id: String,
}

/// 退役属性定义响应
pub type RetireOntologyPropertyResponse = OntologyPropertyItem;

// ==================== 词表 CRUD：关系类型 ====================

/// 关系类型条目（列表项 + 创建 / 更新 / 退役的返回体）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OntologyRelationTypeItem {
    /// 条目 ID
    pub id: String,
    /// 规范词 key（小写 snake_case，全库唯一；创建后不可改）
    pub term_key: String,
    /// 展示名
    pub display_name: String,
    /// 语义描述
    pub description: String,
    /// 边权重基数（写入图谱边 `weight` 时的基准值）
    pub weight_base: f64,
    /// 反向关系 key（None = 无对称反向词）
    pub inverse_key: Option<String>,
    /// 方向二值（"directed" / "undirected"）
    pub direction: String,
    /// 状态（正常 / 退役）
    pub status: OntologyStatus,
    /// 创建时间戳（秒）
    pub created_at: i64,
    /// 更新时间戳（秒）
    pub updated_at: i64,
}

/// 列出关系类型请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListOntologyRelationTypesRequest {
    /// 按状态筛选（None = 全部，含退役）
    #[serde(default)]
    #[param(source = "query")]
    pub status: Option<OntologyStatus>,
    /// 关键词（对 term_key / display_name 模糊匹配）
    #[serde(default)]
    #[param(source = "query")]
    pub keyword: Option<String>,
    /// 分页参数
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: PaginationParams,
}

/// 列出关系类型响应（分页）
pub type ListOntologyRelationTypesResponse = PagedResult<OntologyRelationTypeItem>;

/// 创建关系类型请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateOntologyRelationTypeRequest {
    /// 规范词 key（小写 snake_case，重复创建报错）
    pub term_key: String,
    /// 展示名
    pub display_name: String,
    /// 语义描述
    pub description: String,
    /// 边权重基数（None = 默认 1.0）
    #[serde(default)]
    pub weight_base: Option<f64>,
    /// 反向关系 key（None = 无对称反向词）
    #[serde(default)]
    pub inverse_key: Option<String>,
    /// 方向二值（"directed" / "undirected"）；缺省兜底 undirected（存量调用方零破坏）
    #[serde(default)]
    pub direction: Option<String>,
}

/// 创建关系类型响应
pub type CreateOntologyRelationTypeResponse = OntologyRelationTypeItem;

/// 更新关系类型请求（`term_key` 创建后不可改，同实体类约定）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateOntologyRelationTypeRequest {
    /// 条目 ID
    pub id: String,
    /// 展示名
    pub display_name: String,
    /// 语义描述
    pub description: String,
    /// 边权重基数
    pub weight_base: f64,
    /// 反向关系 key（None = 无对称反向词）
    #[serde(default)]
    pub inverse_key: Option<String>,
}

/// 更新关系类型响应
pub type UpdateOntologyRelationTypeResponse = OntologyRelationTypeItem;

/// 退役关系类型请求（软删除 status=0；历史存量边仍可解释）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct RetireOntologyRelationTypeRequest {
    /// 条目 ID
    #[param(source = "path")]
    pub id: String,
}

/// 退役关系类型响应
pub type RetireOntologyRelationTypeResponse = OntologyRelationTypeItem;

// ==================== 本体骨架（类-关系-类三元组） ====================

/// 骨架三元组条目（列表项 + 创建 / 更新的返回体）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OntologyClassRelationItem {
    /// 条目 ID
    pub id: String,
    /// 主语实体类 term_key
    pub subject_class_key: String,
    /// 关系类型 term_key
    pub relation_key: String,
    /// 宾语实体类 term_key（`*` 表示不限）
    pub object_class_key: String,
    /// 边基数（"single" / "many"）
    pub cardinality: String,
    /// 是否必填
    pub required: bool,
    /// 状态（正常 / 退役）
    pub status: OntologyStatus,
    /// 创建时间戳（秒）
    pub created_at: i64,
    /// 更新时间戳（秒）
    pub updated_at: i64,
}

/// 列出骨架三元组请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListOntologyClassRelationsRequest {
    /// 按主语类筛选
    #[serde(default)]
    #[param(source = "query")]
    pub subject_class_key: Option<String>,
    /// 按关系类型筛选
    #[serde(default)]
    #[param(source = "query")]
    pub relation_key: Option<String>,
    /// 按宾语类筛选
    #[serde(default)]
    #[param(source = "query")]
    pub object_class_key: Option<String>,
    /// 按状态筛选（None = 全部，含退役）
    #[serde(default)]
    #[param(source = "query")]
    pub status: Option<OntologyStatus>,
    /// 分页参数
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: PaginationParams,
}

/// 列出骨架三元组响应（分页）
pub type ListOntologyClassRelationsResponse = PagedResult<OntologyClassRelationItem>;

/// 创建骨架三元组请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateOntologyClassRelationRequest {
    /// 主语实体类 term_key（`*` = 不限）
    pub subject_class_key: String,
    /// 关系类型 term_key
    pub relation_key: String,
    /// 宾语实体类 term_key（`*` = 不限）
    pub object_class_key: String,
    /// 边基数（缺省 many）
    #[serde(default)]
    pub cardinality: Option<String>,
    /// 是否必填
    #[serde(default)]
    pub required: bool,
}

/// 创建骨架三元组响应
pub type CreateOntologyClassRelationResponse = OntologyClassRelationItem;

/// 更新骨架三元组请求（允许改三个 key；唯一约束冲突报错）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateOntologyClassRelationRequest {
    /// 条目 ID
    pub id: String,
    /// 主语实体类 term_key
    pub subject_class_key: String,
    /// 关系类型 term_key
    pub relation_key: String,
    /// 宾语实体类 term_key
    pub object_class_key: String,
    /// 边基数（"single" / "many"）
    pub cardinality: String,
    /// 是否必填
    pub required: bool,
}

/// 更新骨架三元组响应
pub type UpdateOntologyClassRelationResponse = OntologyClassRelationItem;

/// 退役骨架三元组请求（软删除 status=0）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct RetireOntologyClassRelationRequest {
    /// 条目 ID
    #[param(source = "path")]
    pub id: String,
}

/// 退役骨架三元组响应
pub type RetireOntologyClassRelationResponse = OntologyClassRelationItem;

/// 读取本体骨架视图请求（空参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetOntologySkeletonRequest {}

/// 本体骨架视图（管理页「本体骨架」tab 的单次全量返回）
///
/// 只返回活跃词：骨架描述的是「当前生效的本体约定」，退役词与退役三元组
/// 仍保留在库中供历史解析，但不进入当前视图。
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct GetOntologySkeletonResponse {
    /// 活跃实体类（含 parent_term_key）
    pub classes: Vec<OntologyClassItem>,
    /// 活跃关系类型
    pub relation_types: Vec<OntologyRelationTypeItem>,
    /// 活跃骨架三元组
    pub class_relations: Vec<OntologyClassRelationItem>,
}

// ==================== 同义映射管理 ====================

/// 同义映射条目
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OntologySynonymItem {
    /// 条目 ID
    pub id: String,
    /// 原文（归一化后入库：trim + 小写）
    pub raw_term: String,
    /// 目标词条种类
    pub target_kind: TermKind,
    /// 目标规范词 key
    pub target_key: String,
    /// 创建时间戳（秒）
    pub created_at: i64,
}

/// 列出同义映射请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListOntologySynonymsRequest {
    /// 按目标词条种类筛选
    #[serde(default)]
    #[param(source = "query")]
    pub target_kind: Option<TermKind>,
    /// 按目标规范词筛选
    #[serde(default)]
    #[param(source = "query")]
    pub target_key: Option<String>,
    /// 分页参数
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: PaginationParams,
}

/// 列出同义映射响应（分页）
pub type ListOntologySynonymsResponse = PagedResult<OntologySynonymItem>;

/// 创建同义映射请求
///
/// 同一 `(raw_term, target_kind)` 重复创建报错（DB UNIQUE 约束）；
/// 同义目标必须存在于对应词表——目标跨 kind 视为无效映射，解析时按漂移处理。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateOntologySynonymRequest {
    /// 漂移原文（入库前归一化：trim + 小写）
    pub raw_term: String,
    /// 目标词条种类
    pub target_kind: TermKind,
    /// 目标规范词 key
    pub target_key: String,
}

/// 创建同义映射响应
pub type CreateOntologySynonymResponse = OntologySynonymItem;

/// 删除同义映射请求
///
/// 物理删除——映射是解释规则而非事实数据，删除后相关词条自然回落漂移，
/// 下一次解析即生效（惰性，无回填）。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteOntologySynonymRequest {
    /// 条目 ID
    #[param(source = "path")]
    pub id: String,
}

/// 删除同义映射响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteOntologySynonymResponse {
    /// 是否删除成功
    pub success: bool,
}

// ==================== 漂移看板 ====================

/// 漂移看板请求
///
/// 三项静态指标（Top N / 覆盖率 / 漂移节点数）为 SQLite 读路径惰性聚合，
/// 随本体词表进化自动愈合；时间维度趋势走 DuckDB 事件流。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetDriftDashboardRequest {
    /// Top N 漂移词数量上限（None = 默认 20）
    #[serde(default)]
    #[param(source = "query")]
    pub top_n: Option<usize>,
    /// 按 Agent 过滤漂移**词**（None = 全组织）
    ///
    /// ⚠️ 作用范围仅限词频聚合（漂移词 / 漂移节点）；响应里的
    /// `property_domain_coverage` 是**全局口径**，不受本参数影响——属性值表无
    /// agent 列，按 agent 切分会把「该属性在全组织是否已退化成常量」拆碎。
    #[serde(default)]
    #[param(source = "query")]
    pub agent_id: Option<String>,
}

/// 单个漂移词条目
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DriftWordItem {
    /// 归一化后的漂移原文
    pub raw_term: String,
    /// 词条种类
    pub kind: TermKind,
    /// 出现次数（词频）
    pub count: u64,
    /// 独立产出该词的 Agent 数（决策清单"是否多源"信号：多 Agent 独立产出 = 强抽象信号）
    pub agent_count: usize,
}

/// 词表覆盖率（关系维度）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DriftCoverage {
    /// 规范词直接命中的关系数
    pub canonical_relations: u64,
    /// 经同义映射归并的关系数
    pub via_synonym_relations: u64,
    /// 词表外漂移关系数
    pub drift_relations: u64,
    /// 词表覆盖率 =（规范命中 + 同义归并）/ 全部关系数
    pub coverage_ratio: f64,
}

/// 属性域外挂载明细（单个 term_key × node_type 组合）
///
/// 看板「域外 Top N」的下钻入口：谁在什么实体类上越界挂了多少条。
/// `term_key` 无属性定义（TBox 未声明）同样落在这里——归因靠 `node_type` 保留现场。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DomainOutlierItem {
    /// 属性语义锚点（本体 `ontology_properties.term_key`）
    pub term_key: String,
    /// 实际挂载到的实体类（`long_term_knowledge_node.node_type`）
    pub node_type: String,
    /// 该组合的生效属性值条数
    pub count: u64,
}

/// 属性值域覆盖率（L2 阈值告警维度）
///
/// 与 [`DriftCoverage`]（关系维度词表覆盖率）并列：关系维度看「边的词在不在词表」，
/// 本维度看「属性的值有没有挂到声明允许的实体类上」。`domain_classes` 为空视为
/// **不限**（计入域内），与写入期软门禁 `allows_domain_class` 严格同语义。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PropertyDomainCoverage {
    /// 参与统计的生效属性值总数（挂节点、双方 status 均有效）
    pub total_values: u64,
    /// 域内条数（含 `domain_classes` 为空 = 不限的属性）
    pub in_domain: u64,
    /// 域外条数（含 `term_key` 无属性定义的幽灵属性）
    pub out_of_domain: u64,
    /// 覆盖率 = in_domain / total_values（total_values = 0 时为 1.0）
    pub coverage_ratio: f64,
    /// 是否触发 L2 告警（`coverage_ratio < threshold`，等于阈值不告警）
    pub alert: bool,
    /// 本次判定所用阈值（回显，便于看板解释红线来源）
    pub threshold: f64,
    /// 域外明细（count 降序，截断 top_n）
    pub out_of_domain_by_class: Vec<DomainOutlierItem>,
}

/// 漂移看板响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetDriftDashboardResponse {
    /// Top N 漂移词（按词频降序）
    pub top_drift_words: Vec<DriftWordItem>,
    /// 关系维度词表覆盖率
    pub relation_coverage: DriftCoverage,
    /// node_type 词表外的节点数
    pub drift_node_count: u64,
    /// 节点总数
    pub total_node_count: u64,
    /// 属性值域覆盖率（L2 阈值告警维度；只增字段，向后兼容）
    ///
    /// 全局口径：不按请求的 `agent_id` 过滤（属性值表无 agent 列，
    /// 归属需 join 节点；切分会把「属性是否已退化成常量」这个全局判断拆碎）。
    pub property_domain_coverage: PropertyDomainCoverage,
}

// ==================== 属性有效性统计 ====================

/// 属性有效性统计请求
///
/// 按 `(属性 × 实体类)` 分组统计取值分布，用**众数**（占比最高的取值）作为
/// 「默认值」口径——不是枚举首值。原因：`lifecycle_status` 在 fact 上用得最多
/// 的是「现行有效」而不是首个枚举值，按首值算会把方向算反。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetPropertyEffectivenessRequest {
    /// 按属性筛选（None = 全部；入口归一化后与落库规范形比对）
    #[serde(default)]
    #[param(source = "query")]
    pub term_key: Option<String>,
    /// 按实体类筛选（None = 全部；同样归一化后比对）
    #[serde(default)]
    #[param(source = "query")]
    pub node_type: Option<String>,
    /// 只保留 `mode_ratio >= 该值` 的分组（None = 不筛；NaN / 无穷视为不筛）
    #[serde(default)]
    #[param(source = "query")]
    pub min_mode_ratio: Option<f64>,
    /// 返回分组条数上限（None = 默认 100）
    #[serde(default)]
    #[param(source = "query")]
    pub limit: Option<usize>,
}

/// 单个 `(属性 × 实体类)` 组合的有效性条目
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PropertyEffectivenessItem {
    /// 属性语义锚点（`knowledge_property.term_key`）
    pub term_key: String,
    /// 实体类（`long_term_knowledge_node.node_type`）
    pub node_type: String,
    /// 该组合的生效属性值总条数
    pub total: u64,
    /// 不同取值的个数（同一取值跨值载体合并后计数）
    pub distinct_values: u64,
    /// 众数取值（并列时取字典序最小，保证同一份数据反复查询结果一致）
    pub mode_value: String,
    /// 众数出现条数
    pub mode_count: u64,
    /// 众数占比 = `mode_count / total`
    pub mode_ratio: f64,
    /// 非众数占比 = `1 - mode_ratio`（即「真正在变化的值」的占比）
    pub non_default_ratio: f64,
    /// 是否标黄：`mode_ratio > threshold`（严格大于，恰好等于不标）
    pub flagged: bool,
}

/// 属性有效性统计响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetPropertyEffectivenessResponse {
    /// 分组条目（`mode_ratio` 降序 → `term_key` 升序 → `node_type` 升序，按 limit 截断）
    pub items: Vec<PropertyEffectivenessItem>,
    /// 过滤后命中的分组总数（截断前，便于前端提示「还有多少组未展示」）
    pub total_groups: u64,
    /// 本次判定所用阈值（回显，便于看板解释标黄来源）
    pub threshold: f64,
}

/// 漂移趋势请求（DuckDB 事件流聚合，"历史时"视角）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetDriftTrendRequest {
    /// 统计最近 N 天（None = 默认 30）
    #[serde(default)]
    #[param(source = "query")]
    pub days: Option<u32>,
    /// 按 Agent 过滤（None = 全组织）
    #[serde(default)]
    #[param(source = "query")]
    pub agent_id: Option<String>,
}

/// 单个趋势数据点
///
/// `drift_count` 按查询时点的词表重放解析得出——DuckDB 只存原文（不可变
/// 事实），漂移结论会随本体进化变化，趋势是"用当前词表看历史"。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DriftTrendPoint {
    /// 日期（YYYY-MM-DD）
    pub day: String,
    /// 词条种类
    pub kind: TermKind,
    /// 当日沉淀词总数
    pub total_count: u64,
    /// 当日词表外漂移数（重放口径）
    pub drift_count: u64,
}

/// 漂移趋势响应
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct GetDriftTrendResponse {
    /// 趋势数据点（按天升序）
    pub points: Vec<DriftTrendPoint>,
}

/// 漂移词下钻：关系（边）明细请求
///
/// `raw_term` 既可为漂移原文也可为规范词 key——下钻不限于漂移词，
/// 可查看任一词条的图谱实例分布。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListDriftRelationDetailsRequest {
    /// 词条原文（归一化匹配）
    #[param(source = "query")]
    pub raw_term: String,
    /// 按 Agent 过滤（None = 全组织）
    #[serde(default)]
    #[param(source = "query")]
    pub agent_id: Option<String>,
    /// 分页参数
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: PaginationParams,
}

/// 漂移关系（边）明细
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DriftRelationDetail {
    /// 边 ID
    pub id: String,
    /// 产出该边的 Agent
    pub agent_id: String,
    /// 源节点名称
    pub source_name: String,
    /// 目标节点名称
    pub target_name: String,
    /// 创建时间戳（秒）
    pub created_at: i64,
}

/// 漂移关系（边）明细响应（分页）
pub type ListDriftRelationDetailsResponse = PagedResult<DriftRelationDetail>;

/// 漂移词下钻：节点明细请求（`raw_term` 语义同关系下钻）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListDriftClassDetailsRequest {
    /// 词条原文（归一化匹配）
    #[param(source = "query")]
    pub raw_term: String,
    /// 按 Agent 过滤（None = 全组织）
    #[serde(default)]
    #[param(source = "query")]
    pub agent_id: Option<String>,
    /// 分页参数
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: PaginationParams,
}

/// 漂移节点明细
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DriftClassDetail {
    /// 节点 ID
    pub id: String,
    /// 产出该节点的 Agent
    pub agent_id: String,
    /// 节点名称
    pub name: String,
    /// 创建时间戳（秒）
    pub created_at: i64,
}

/// 漂移节点明细响应（分页）
pub type ListDriftClassDetailsResponse = PagedResult<DriftClassDetail>;

// ==================== 词表注入视图（神经技能 / 管理页共用） ====================

/// 词表注入视图请求（无参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListOntologyLexiconRequest {}

/// 词表注入视图响应
///
/// 直接复用 [`crate::ontology::OntologyLexiconSummary`]（纯函数层单一事实源）：
/// 三段分列，字段顺序即 Token 裁剪优先级（关系词 > 实体类 > 同义样例）。
pub type ListOntologyLexiconResponse = crate::ontology::OntologyLexiconSummary;

// ==================== seed 预置词表同步 ====================

/// 预置词表同步预览请求（无参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct PreviewPresetOntologyRequest {}

/// 单个预置词表条目的同步预览项
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PresetOntologySyncItem {
    /// 词条种类
    pub kind: TermKind,
    /// 规范词 key（同步以 term_key 为匹配键）
    pub term_key: String,
    /// seed 中的展示名
    pub display_name: String,
    /// seed 中的语义描述
    pub description: String,
    /// 库内是否已存在（仅补缺策略下存在即跳过）
    pub exists: bool,
    /// 库内当前展示名（不存在为 None）
    pub local_display_name: Option<String>,
    /// 库内当前语义描述（不存在为 None）
    pub local_description: Option<String>,
    /// 与 seed 不一致的字段名清单（覆盖策略下将按 seed 覆写）
    pub diff_fields: Vec<String>,
    /// 库内是否为退役行（覆盖策略下整体跳过）
    pub retired: bool,
    /// seed 中关系方向（仅关系类型条目携带；实体类/同义映射为 None）
    pub seed_direction: Option<String>,
    /// 库内当前关系方向（仅已存在的关系类型条目携带）
    pub local_direction: Option<String>,
}

/// 预置词表同步预览响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PreviewPresetOntologyResponse {
    /// 逐词条的对比结果
    pub items: Vec<PresetOntologySyncItem>,
    /// 库内缺失的数量（同步时会被新建）
    pub missing_count: usize,
    /// 库内已存在的数量（仅补缺策略下直接跳过）
    pub existing_count: usize,
    /// 覆盖策略下将执行覆写动作的条目数（已存在非退役条目一律幂等覆写；与 sync 响应 updated 同口径）
    pub overwrite_count: usize,
    /// 库内退役行数（覆盖策略下整体跳过不覆写；同时计入 existing_count）
    pub retired_count: usize,
}

/// 预置词表同步策略
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub enum PresetOntologySyncStrategy {
    /// 仅补缺：term_key 不存在才插入，已存在条目（含退役行）一律跳过
    #[default]
    OnlyMissing,
    /// 覆盖重置：对已存在非退役条目按 seed 幂等覆写（term_key / id / status / created_at 永不触碰），退役行整体跳过
    Overwrite,
}

/// 同步预置词表请求
///
/// `strategy` 缺省为**仅补缺**（term_key 不存在才插入，已存在条目跳过），
/// 与历史行为一致；显式传 `Overwrite` 时对已存在非退役条目按 seed 幂等
/// 覆写，退役行整体跳过。管理页仍是词表条目增删的唯一修改入口，同步只
/// 承载 seed 预置词表的注入与对齐。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct SyncPresetOntologyRequest {
    /// 同步策略（缺省=仅补缺）
    #[serde(default)]
    pub strategy: PresetOntologySyncStrategy,
}

/// 同步预置词表响应
///
/// 同步返回（不走后台任务）——词表量级为个位数十条，幂等 upsert 毫秒级。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SyncPresetOntologyResponse {
    /// 新建的词条数量
    pub created: usize,
    /// 执行覆写动作的条目数量（覆盖策略下命中非退役条目即计，非实际字段变化数）
    pub updated: usize,
    /// 跳过的词条数量（仅补缺下 term_key 已存在；覆盖下为退役行）
    pub skipped: usize,
    /// seed 中预置词条总数
    pub total: usize,
}
