//! 本体（Ontology）词表管理相关 API DTO
//!
//! DTO 单一事实源（前后端共用），覆盖四组接口契约：
//!
//! 1. **词表 CRUD**：实体类（`ontology_classes`）与关系类型
//!    （`ontology_relation_types`）的新增 / 编辑 / 退役（软删除）。
//! 2. **同义映射管理**：`ontology_synonym_mappings` 的增删查。
//! 3. **漂移看板**：Top N 漂移词 / 词表覆盖率 / 漂移节点数（SQLite 读路径
//!    惰性聚合，"现在时"）+ 漂移趋势（DuckDB 事件流，"历史时"）+ 明细下钻。
//! 4. **seed 预置词表同步**：preview / sync 对（仅补缺，无覆盖策略——
//!    本体是共享约定，seed 不允许覆盖管理页的本地修改）。
//!
//! 词条种类复用 [`crate::ontology::TermKind`]（单一事实源），JSON 形态为
//! 小写 `"class"` / `"relation"`，与统计事件 kind 口径一致。

use super::{PagedResult, PaginationParams};
use crate::enums::OntologyStatus;
use crate::ontology::TermKind;
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
    /// 允许的源实体类 key 清单（空 = 不限）
    pub domain_classes: Vec<String>,
    /// 允许的目标实体类 key 清单（空 = 不限）
    pub range_classes: Vec<String>,
    /// 边权重基数（写入图谱边 `weight` 时的基准值）
    pub weight_base: f64,
    /// 反向关系 key（None = 无对称反向词）
    pub inverse_key: Option<String>,
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
    /// 允许的源实体类 key 清单（空 = 不限）
    #[serde(default)]
    pub domain_classes: Vec<String>,
    /// 允许的目标实体类 key 清单（空 = 不限）
    #[serde(default)]
    pub range_classes: Vec<String>,
    /// 边权重基数（None = 默认 1.0）
    #[serde(default)]
    pub weight_base: Option<f64>,
    /// 反向关系 key（None = 无对称反向词）
    #[serde(default)]
    pub inverse_key: Option<String>,
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
    /// 允许的源实体类 key 清单（空 = 不限）
    #[serde(default)]
    pub domain_classes: Vec<String>,
    /// 允许的目标实体类 key 清单（空 = 不限）
    #[serde(default)]
    pub range_classes: Vec<String>,
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
    /// 按 Agent 过滤（None = 全组织）
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
}

/// 同步预置词表请求
///
/// 无策略字段：seed 注入**仅补缺**（term_key 不存在才插入），不做覆盖 /
/// 删除——管理页是词表的唯一修改入口，避免两处写路径打架。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct SyncPresetOntologyRequest {}

/// 同步预置词表响应
///
/// 同步返回（不走后台任务）——词表量级为个位数十条，幂等 upsert 毫秒级。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SyncPresetOntologyResponse {
    /// 新建的词条数量
    pub created: usize,
    /// 跳过的词条数量（term_key 已存在）
    pub skipped: usize,
    /// seed 中预置词条总数
    pub total: usize,
}
