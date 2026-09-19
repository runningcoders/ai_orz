//! Ontology DAO 模块（词表三表：实体类 / 关系类型 / 同义映射）
//!
//! 全局词表（无 organization_id，对齐 memory 域「蜂巢共享语言」先例）：
//! - `ontology_classes`：实体类词表（TBox：图谱节点类型）
//! - `ontology_relation_types`：关系类型词表（TBox：图谱边类型）
//! - `ontology_synonym_mappings`：同义映射（旧词/别名 → 规范词）
//!
//! 设计边界（docs/design/ontology_knowledge_sedimentation_design.md §2.2）：
//! - term_key 是语义锚点，跨表/跨环境引用一律用它（唯一约束）；
//! - 词表条目软删除（status 1 正常 / 0 退役，退役 ≠ 删除）；
//! - 同义映射是解释规则非事实数据，物理删除、无退役语义。

use crate::models::ontology::{OntologyClassPo, OntologyRelationTypePo, OntologySynonymMappingPo};
use crate::pkg::RequestContext;
use crate::pkg::stats::{StatAggregation, StatEvent, StatFilter, Stats};
use common::api::PaginationParams;
use common::enums::OntologyStatus;
use common::error::Result;
use common::ontology::TermKind;
use serde_json::Value as JsonValue;

/// 实体类查询参数
#[derive(Debug, Clone, Default)]
pub struct OntologyClassQuery {
    /// 按状态过滤（None = 不过滤，含退役）
    pub status: Option<OntologyStatus>,
    /// 关键词（term_key / display_name / description LIKE 模糊匹配）
    pub keyword: Option<String>,
    pub pagination: PaginationParams,
}

/// 关系类型查询参数
#[derive(Debug, Clone, Default)]
pub struct OntologyRelationTypeQuery {
    /// 按状态过滤（None = 不过滤，含退役）
    pub status: Option<OntologyStatus>,
    /// 关键词（term_key / display_name / description LIKE 模糊匹配）
    pub keyword: Option<String>,
    pub pagination: PaginationParams,
}

/// 同义映射查询参数
#[derive(Debug, Clone, Default)]
pub struct OntologySynonymQuery {
    /// 按映射目标种类过滤
    pub target_kind: Option<TermKind>,
    /// 按目标规范词精确匹配（管理页「查看某规范词的全部别名」）
    pub target_key: Option<String>,
    /// 关键词（raw_term / target_key LIKE 模糊匹配）
    pub keyword: Option<String>,
    pub pagination: PaginationParams,
}

/// Ontology DAO 接口（词表三表读写）
#[async_trait::async_trait]
pub trait OntologyDao: Send + Sync {
    // ==================== 实体类（TBox：节点类型） ====================

    /// 插入实体类词条（term_key 唯一，冲突返回 Conflict）
    async fn insert_class(&self, ctx: RequestContext, class: &OntologyClassPo) -> Result<()>;

    async fn find_class_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<OntologyClassPo>>;

    /// 按语义锚点查实体类（写侧查重 + 认证词表加载）
    async fn find_class_by_term_key(
        &self,
        ctx: RequestContext,
        term_key: &str,
    ) -> Result<Option<OntologyClassPo>>;

    /// 分页组合查询（count 内嵌）
    async fn query_classes(
        &self,
        ctx: RequestContext,
        query: OntologyClassQuery,
    ) -> Result<common::api::PagedResult<OntologyClassPo>>;

    /// 全量实体类（含退役；lexicon 加载与认证需要全貌）
    async fn list_all_classes(&self, ctx: RequestContext) -> Result<Vec<OntologyClassPo>>;

    /// 全量更新（term_key 不可变 —— 语义锚点是历史图谱引用的稳定性来源）
    async fn update_class(&self, ctx: RequestContext, class: &OntologyClassPo) -> Result<()>;

    /// 退役（软删除 status=0；幂等：不存在或已退役均静默成功）
    async fn retire_class(&self, ctx: RequestContext, id: &str) -> Result<()>;

    // ==================== 关系类型（TBox：边类型） ====================

    /// 插入关系类型词条（term_key 唯一，冲突返回 Conflict）
    async fn insert_relation_type(
        &self,
        ctx: RequestContext,
        relation_type: &OntologyRelationTypePo,
    ) -> Result<()>;

    async fn find_relation_type_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<OntologyRelationTypePo>>;

    /// 按语义锚点查关系类型（写侧查重 + 认证词表加载）
    async fn find_relation_type_by_term_key(
        &self,
        ctx: RequestContext,
        term_key: &str,
    ) -> Result<Option<OntologyRelationTypePo>>;

    /// 分页组合查询（count 内嵌）
    async fn query_relation_types(
        &self,
        ctx: RequestContext,
        query: OntologyRelationTypeQuery,
    ) -> Result<common::api::PagedResult<OntologyRelationTypePo>>;

    /// 全量关系类型（含退役；lexicon 加载与认证需要全貌）
    async fn list_all_relation_types(
        &self,
        ctx: RequestContext,
    ) -> Result<Vec<OntologyRelationTypePo>>;

    /// 全量更新（term_key 不可变；inverse_key 指向的是 term_key 而非 id，
    /// 改名时同步更新两端 —— 由 Domain 编排保证，DAO 只做行更新）
    async fn update_relation_type(
        &self,
        ctx: RequestContext,
        relation_type: &OntologyRelationTypePo,
    ) -> Result<()>;

    /// 退役（软删除 status=0；幂等：不存在或已退役均静默成功）
    async fn retire_relation_type(&self, ctx: RequestContext, id: &str) -> Result<()>;

    // ==================== 同义映射 ====================

    /// 插入同义映射（UNIQUE(raw_term, target_kind)，冲突返回 Conflict）
    async fn insert_synonym(
        &self,
        ctx: RequestContext,
        mapping: &OntologySynonymMappingPo,
    ) -> Result<()>;

    /// 按原始词查全部映射（同一 raw 词可分别映射到 class / relation 两类）
    async fn find_synonyms_by_raw_term(
        &self,
        ctx: RequestContext,
        raw_term: &str,
    ) -> Result<Vec<OntologySynonymMappingPo>>;

    /// 分页组合查询（count 内嵌）
    async fn query_synonyms(
        &self,
        ctx: RequestContext,
        query: OntologySynonymQuery,
    ) -> Result<common::api::PagedResult<OntologySynonymMappingPo>>;

    /// 全量同义映射（lexicon 加载需要全貌）
    async fn list_all_synonyms(&self, ctx: RequestContext)
    -> Result<Vec<OntologySynonymMappingPo>>;

    /// 物理删除（映射是解释规则非事实数据，无退役语义；幂等）
    async fn delete_synonym(&self, ctx: RequestContext, id: &str) -> Result<()>;
}

/// 本体漂移事件查询参数（DuckDB 时间线视角）
#[derive(Debug, Clone, Default)]
pub struct OntologyDriftQuery {
    /// 按 Agent 过滤（None = 全部）
    pub agent_id: Option<String>,
    /// 按词元类别过滤（"relation" / "class"；None = 全部）
    pub kind: Option<String>,
    /// 时间范围（毫秒时间戳闭区间，None = 全部历史）
    pub time_range: Option<(i64, i64)>,
    /// 通用附加过滤（业务维度之外追加）
    pub filters: Vec<StatFilter>,
    /// 分组维度（"kind" / "agent_id" 等，底层通用查询使用）
    pub group_by: Vec<String>,
    /// 聚合方式（底层通用查询使用；业务语义方法会覆盖此字段）
    pub aggregations: Vec<StatAggregation>,
}

/// 最近漂移词条目（时间线瘦投影）
#[derive(Debug, Clone, serde::Serialize)]
pub struct OntologyDriftTermRow {
    /// 事件毫秒时间戳
    pub timestamp: i64,
    /// 沉淀该词的 Agent
    pub agent_id: String,
    /// 词元类别："relation" | "class"
    pub kind: String,
    /// 词元原文（trim 后，只记原文不解析）
    pub raw_term: String,
}

/// Ontology 统计 DAO 接口（DuckDB 时间线视角）
///
/// 数据来源：`ontology_drift_events`（写侧 [`crate::pkg::stats::OntologyDriftEvent`]）。
/// 与漂移看板（[`crate::service::dal::ontology`]，SQLite 词频「现在时」）互补：
/// 本接口查事件流，回答「漂移如何随时间发生」。
#[async_trait::async_trait]
pub trait OntologyStatsDao: Send + Sync {
    /// 本体漂移事件类型
    type DriftEvent: StatEvent + 'static + Send + Sync;

    /// 获取漂移事件表名（从 Stats 注册表中查询）
    fn drift_table_name(&self, stats: &Stats) -> Option<String> {
        stats.get_table_name::<Self::DriftEvent>()
    }

    /// 底层通用聚合查询（内部使用，不对外暴露业务语义）
    async fn query_drift_events(
        &self,
        ctx: RequestContext,
        query: OntologyDriftQuery,
    ) -> Result<Vec<JsonValue>>;

    /// 漂移事件总数（Count 聚合）
    async fn count_drift_events(
        &self,
        ctx: RequestContext,
        mut query: OntologyDriftQuery,
    ) -> Result<u64> {
        query.aggregations = vec![StatAggregation::Count];
        let rows = self.query_drift_events(ctx, query).await?;
        if rows.is_empty() {
            return Ok(0);
        }
        Ok(rows[0].get("count").and_then(|v| v.as_f64()).unwrap_or(0.0) as u64)
    }

    /// 按词元类别计数（group_by kind；如 relation / class 各自的漂移量）
    async fn count_by_kind(
        &self,
        ctx: RequestContext,
        mut query: OntologyDriftQuery,
    ) -> Result<Vec<(String, u64)>> {
        query.aggregations = vec![StatAggregation::Count];
        query.group_by = vec!["kind".to_string()];
        let rows = self.query_drift_events(ctx, query).await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let kind = row.get("kind")?.as_str()?.to_string();
                let count = row.get("count").and_then(|v| v.as_f64()).unwrap_or(0.0) as u64;
                Some((kind, count))
            })
            .collect())
    }

    /// 按 Agent 计数（group_by agent_id；谁在产生漂移词）
    async fn count_by_agent(
        &self,
        ctx: RequestContext,
        mut query: OntologyDriftQuery,
    ) -> Result<Vec<(String, u64)>> {
        query.aggregations = vec![StatAggregation::Count];
        query.group_by = vec!["agent_id".to_string()];
        let rows = self.query_drift_events(ctx, query).await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let agent_id = row.get("agent_id")?.as_str()?.to_string();
                let count = row.get("count").and_then(|v| v.as_f64()).unwrap_or(0.0) as u64;
                Some((agent_id, count))
            })
            .collect())
    }

    /// 最近 N 条漂移原文（时间线视角，timestamp 倒序）
    async fn recent_raw_terms(
        &self,
        ctx: RequestContext,
        limit: i64,
        agent_id: Option<String>,
    ) -> Result<Vec<OntologyDriftTermRow>>;
}

pub mod sqlite;

#[cfg(test)]
mod sqlite_test;

pub mod stats_duckdb;

#[cfg(test)]
mod stats_duckdb_test;

pub use sqlite::{dao, init, new};

pub use self::stats_duckdb::{stats_dao, stats_init, stats_new};
