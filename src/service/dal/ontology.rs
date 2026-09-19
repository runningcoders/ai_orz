//! Ontology DAL - 本体数据访问层（业务逻辑层）
//!
//! 职责：组合 [`OntologyDao`]（TBox 词表三表）+ [`MemoryDao`]（ABox 图谱聚合），
//! 承担读路径"解释者"三件事（docs/design/ontology_knowledge_sedimentation_design.md §2.7）：
//! - **词表加载**：三表全量 → 内存 [`OntologyLexicon`]（词表量小，全量载入）
//! - **看板聚合**：GROUP BY 词频 + 内存 resolve → Top N 漂移词 / 覆盖率 / 漂移节点数
//! - **认证执行**：写后 certify——resolve 命中置位 `is_published`，Drift 软门禁不拦截
//!
//! 方向纪律（决策 #5）：依赖单向指向被观察者，[`crate::service::dal::MemoryDal`]
//! 零改动零感知；DAL 之间禁止互引，本模块只依赖 DAO 层。

use crate::models::ontology::{OntologyClass, OntologyRelationType, OntologySynonymMapping};
use crate::pkg::RequestContext;
use crate::pkg::stats::OntologyDriftEvent;
use crate::service::dao::memory::MemoryDao;
use crate::service::dao::ontology::{
    OntologyClassQuery, OntologyDao, OntologyDriftQuery, OntologyDriftTermRow,
    OntologyRelationTypeQuery, OntologyStatsDao, OntologySynonymQuery,
};
use async_trait::async_trait;
use common::api::ontology::{
    DriftClassDetail, DriftCoverage, DriftRelationDetail, DriftWordItem, GetDriftDashboardRequest,
    GetDriftDashboardResponse, ListDriftClassDetailsRequest, ListDriftClassDetailsResponse,
    ListDriftRelationDetailsRequest, ListDriftRelationDetailsResponse,
};
use common::api::{PagedResult, PaginationParams};
use common::enums::OntologyStatus;
use common::error::Result;
use common::ontology::{
    LexiconSynonymSummary, LexiconTermSummary, OntologyCertifyReport, OntologyLexicon,
    OntologyLexiconSummary, ResolvedTerm, TermKind, resolve,
};
use std::sync::Arc;

// ==================== Factory + Singleton ====================

static ONTOLOGY_DAL_INSTANCE: std::sync::OnceLock<Arc<dyn OntologyDal>> =
    std::sync::OnceLock::new();

pub fn new(
    ontology_dao: Arc<dyn OntologyDao>,
    memory_dao: Arc<dyn MemoryDao>,
    stats_dao: Arc<dyn OntologyStatsDao<DriftEvent = OntologyDriftEvent>>,
) -> Arc<dyn OntologyDal> {
    Arc::new(OntologyDalImpl {
        ontology_dao,
        memory_dao,
        stats_dao,
    })
}

pub fn init() {
    crate::service::dao::ontology::stats_init();
    let _ = ONTOLOGY_DAL_INSTANCE.set(new(
        crate::service::dao::ontology::dao(),
        crate::service::dao::memory::dao(),
        crate::service::dao::ontology::stats_dao(),
    ));
}

pub fn dal() -> Arc<dyn OntologyDal> {
    ONTOLOGY_DAL_INSTANCE.get().cloned().unwrap()
}

/// 尝试获取 OntologyDal 单例（未初始化返回 `None`）
///
/// 供**可选依赖**场景优雅降级：如提示词词表注入——本体子系统未初始化时
/// 跳过注入即可，不应让可选增强阻断主流程（区别于 [`dal`] 的硬依赖语义）。
pub fn try_dal() -> Option<Arc<dyn OntologyDal>> {
    ONTOLOGY_DAL_INSTANCE.get().cloned()
}

// ==================== 词表视图辅助（Active 全量拉取） ====================

/// 词表视图 / 认证的全量拉取上限（词表量级几十条，1000 已远超需求；
/// `PaginationParams` 无「全量」语义，用大 limit 模拟）
const LEXICON_LOAD_LIMIT: usize = 1000;

/// 拉取全量**生效**词表条目的分页参数（只取 Active，供提示词注入视图；
/// 认证/看板走 `list_all_*` 含退役全量，语义不同）
pub(crate) fn active_all_pagination() -> PaginationParams {
    PaginationParams {
        limit: Some(LEXICON_LOAD_LIMIT),
        offset: None,
    }
}

/// `active_all_pagination` 大 limit 模拟全量的截断留痕：拉回条目数达到
/// `LEXICON_LOAD_LIMIT` 视为可能被截断（正常词表量级几十条，触发即异常），
/// 打 warn 提醒，避免提示词注入视图静默丢词
pub(crate) fn warn_if_lexicon_truncated(ctx: &RequestContext, what: &str, total: usize) {
    if total >= LEXICON_LOAD_LIMIT {
        log_warn!(
            ctx,
            "ontology_dal",
            "{} 活跃条目数达到拉取上限 {}，结果可能被截断",
            what,
            LEXICON_LOAD_LIMIT
        );
    }
}

// ==================== DAL Trait ====================

#[async_trait]
pub trait OntologyDal: Send + Sync {
    /// 词表全量加载：三表 `list_all` → 内存 [`OntologyLexicon`]
    ///
    /// **含退役词条**（`list_all_*` 刻意不过滤 status）：退役 ≠ 删除，历史图谱
    /// 中的存量引用仍需可解释——退役词留在词表内继续解析为规范命中，看板不会
    /// 把历史数据误报成漂移。这是 DAO 侧「全量词表（含退役；lexicon 加载与认证
    /// 需要全貌）」契约的消费点。
    ///
    /// 词表为空时 resolve 全部返回 Drift——语义正确（无词表 = 无约定 = 全漂移），
    /// 看板会引导管理员完成初始注入。
    async fn load_lexicon(&self, ctx: RequestContext) -> Result<OntologyLexicon>;

    /// 词表注入视图：三表 **Active** 全量 → [`OntologyLexiconSummary`]（design §5.4）
    ///
    /// 消费方：神经技能提示词构建器（prompt builder 词表区块）与 hr Domain 词表
    /// 视图 / 导出——转换逻辑单点在本 DAL。与 [`load_lexicon`] 的区别：
    /// 只取生效词条（退役词条不进提示词），返回展示视图
    /// （term_key + display_name + description）而非归一化解析键集合。
    async fn load_lexicon_summary(&self, ctx: RequestContext) -> Result<OntologyLexiconSummary>;

    /// 漂移看板聚合（SQLite 读路径惰性聚合，"现在时"视角）
    ///
    /// 口径：SQL 词频（归一化 GROUP BY，只计生效行）+ 内存 [`resolve`] 判定，
    /// 无跨表 JOIN（决策 #4）。聚合规则：
    /// - 关系侧：Canonical / ViaSynonym / Drift 三分支计数 → [`DriftCoverage`]，
    ///   `coverage_ratio =（规范命中 + 同义归并）/ 全部关系数`（零关系时为 0.0）
    /// - 节点侧：词表外（Drift）行数累计为 `drift_node_count`
    /// - Top N：两侧漂移词合并，按 count 降序、原文升序（与 SQL 排序口径一致）
    ///   后截取 `top_n`（None = 默认 20）
    ///
    /// `agent_id` 直传词频聚合（None / 空串 = 全组织）。
    async fn get_drift_dashboard(
        &self,
        ctx: RequestContext,
        request: GetDriftDashboardRequest,
    ) -> Result<GetDriftDashboardResponse>;

    /// 漂移词下钻：关系（边）明细——透传 [`MemoryDao::detail_relations_by_term`]
    /// 并把 DAO 瘦投影行转为 [`DriftRelationDetail`] DTO（`PagedResult::map` 保 total）
    async fn list_drift_relation_details(
        &self,
        ctx: RequestContext,
        request: ListDriftRelationDetailsRequest,
    ) -> Result<ListDriftRelationDetailsResponse>;

    /// 漂移词下钻：节点明细——透传 [`MemoryDao::detail_nodes_by_term`]
    /// 并把 DAO 瘦投影行转为 [`DriftClassDetail`] DTO
    async fn list_drift_class_details(
        &self,
        ctx: RequestContext,
        request: ListDriftClassDetailsRequest,
    ) -> Result<ListDriftClassDetailsResponse>;

    /// 写后认证（certify 软门禁）：单词条 resolve 判定 + 命中置位
    ///
    /// 流程（design §2.4）：
    /// 1. 词表全量加载 + `resolve(kind, raw_term)` 判定，结论记入报告；
    /// 2. 命中 Canonical / ViaSynonym 且 `kind = Class` → 调
    ///    [`MemoryDao::publish_nodes_by_type`] 按 term_key 幂等置位图谱节点
    ///    `is_published`（边表无该控制位，关系词命中无需置位动作）；
    /// 3. Drift → **无任何写动作**：软门禁不拦截不撤回，`is_published` 保持
    ///    未置位即"待复核"语义（红线 5：published 非可见性，蜂巢全可见不变）。
    ///
    /// 返回 [`OntologyCertifyReport`] 供消费侧打点（`drift_terms()`）与日志；
    /// `agent_id` 由调用方（消费者）从事件携带，认证本身不感知归属。
    async fn certify_memory_term(
        &self,
        ctx: RequestContext,
        kind: TermKind,
        raw_term: &str,
    ) -> Result<OntologyCertifyReport>;

    // ==================== 漂移事件流（DuckDB 时间线视角） ====================
    //
    // 与漂移看板（SQLite「现在时」）互补：透传 OntologyStatsDao 的 DuckDB
    // 事件流查询，回答「漂移如何随时间发生」（趋势 / 审计类消费场景）。

    /// 漂移事件总数（Count 聚合，支持 agent / kind / 时间范围过滤）
    async fn count_drift_events(
        &self,
        ctx: RequestContext,
        query: OntologyDriftQuery,
    ) -> Result<u64>;

    /// 按词元类别计数（relation / class 各自漂移量）
    async fn count_drift_by_kind(
        &self,
        ctx: RequestContext,
        query: OntologyDriftQuery,
    ) -> Result<Vec<(String, u64)>>;

    /// 按 Agent 计数（谁在产生漂移词）
    async fn count_drift_by_agent(
        &self,
        ctx: RequestContext,
        query: OntologyDriftQuery,
    ) -> Result<Vec<(String, u64)>>;

    /// 最近 N 条漂移原文（timestamp 倒序；raw_term 只记原文不解析）
    async fn recent_drift_terms(
        &self,
        ctx: RequestContext,
        limit: i64,
        agent_id: Option<String>,
    ) -> Result<Vec<OntologyDriftTermRow>>;

    // ==================== 词表 CRUD 透传（Domain 数据通道） ====================
    //
    // Domain 层（hr::OntologyManage）的 CRUD 编排需要写路径数据通道；
    // 分层纪律（Domain→DAL→DAO）要求由本 DAL 透传，PO↔Entity 转换在此完成，
    // PO 不出 DAL（CODE_STANDARDS §2 分层红线）。

    // ---- 实体类（TBox：节点类型） ----

    /// 插入实体类（term_key 唯一，冲突返回 Conflict）
    async fn insert_class(&self, ctx: RequestContext, class: &OntologyClass) -> Result<()>;

    /// 按 ID 查实体类
    async fn find_class_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<OntologyClass>>;

    /// 按语义锚点查实体类（写侧查重 / seed 注入补缺判断）
    async fn find_class_by_term_key(
        &self,
        ctx: RequestContext,
        term_key: &str,
    ) -> Result<Option<OntologyClass>>;

    /// 实体类分页组合查询（status / keyword 过滤，count 内嵌）
    async fn query_classes(
        &self,
        ctx: RequestContext,
        query: OntologyClassQuery,
    ) -> Result<PagedResult<OntologyClass>>;

    /// 全量更新实体类（term_key 不可变 —— 语义锚点是历史图谱引用的稳定性来源）
    async fn update_class(&self, ctx: RequestContext, class: &OntologyClass) -> Result<()>;

    /// 退役实体类（软删除 status=0；幂等：不存在或已退役均静默成功）
    async fn retire_class(&self, ctx: RequestContext, id: &str) -> Result<()>;

    // ---- 关系类型（TBox：边类型） ----

    /// 插入关系类型（term_key 唯一，冲突返回 Conflict）
    async fn insert_relation_type(
        &self,
        ctx: RequestContext,
        relation_type: &OntologyRelationType,
    ) -> Result<()>;

    /// 按 ID 查关系类型
    async fn find_relation_type_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<OntologyRelationType>>;

    /// 按语义锚点查关系类型（写侧查重 / seed 注入补缺判断）
    async fn find_relation_type_by_term_key(
        &self,
        ctx: RequestContext,
        term_key: &str,
    ) -> Result<Option<OntologyRelationType>>;

    /// 关系类型分页组合查询（status / keyword 过滤，count 内嵌）
    async fn query_relation_types(
        &self,
        ctx: RequestContext,
        query: OntologyRelationTypeQuery,
    ) -> Result<PagedResult<OntologyRelationType>>;

    /// 全量更新关系类型（term_key 不可变；inverse_key 指向 term_key 而非 id）
    async fn update_relation_type(
        &self,
        ctx: RequestContext,
        relation_type: &OntologyRelationType,
    ) -> Result<()>;

    /// 退役关系类型（软删除 status=0；幂等）
    async fn retire_relation_type(&self, ctx: RequestContext, id: &str) -> Result<()>;

    // ---- 同义映射 ----

    /// 插入同义映射（UNIQUE(raw_term, target_kind)，冲突返回 Conflict）
    async fn insert_synonym(
        &self,
        ctx: RequestContext,
        mapping: &OntologySynonymMapping,
    ) -> Result<()>;

    /// 按原始词查全部映射（同一 raw 词可分别映射到 class / relation 两类）
    async fn find_synonyms_by_raw_term(
        &self,
        ctx: RequestContext,
        raw_term: &str,
    ) -> Result<Vec<OntologySynonymMapping>>;

    /// 同义映射分页组合查询（count 内嵌）
    async fn query_synonyms(
        &self,
        ctx: RequestContext,
        query: OntologySynonymQuery,
    ) -> Result<PagedResult<OntologySynonymMapping>>;

    /// 删除同义映射（物理删除；幂等；删除后相关词条自然回落漂移）
    async fn delete_synonym(&self, ctx: RequestContext, id: &str) -> Result<()>;
}

// ==================== DAL Impl ====================

pub struct OntologyDalImpl {
    ontology_dao: Arc<dyn OntologyDao>,
    memory_dao: Arc<dyn MemoryDao>,
    stats_dao: Arc<dyn OntologyStatsDao<DriftEvent = OntologyDriftEvent>>,
}

#[async_trait]
impl OntologyDal for OntologyDalImpl {
    async fn load_lexicon(&self, ctx: RequestContext) -> Result<OntologyLexicon> {
        let (classes, relation_types, synonyms) = tokio::try_join!(
            self.ontology_dao.list_all_classes(ctx.clone()),
            self.ontology_dao.list_all_relation_types(ctx.clone()),
            self.ontology_dao.list_all_synonyms(ctx)
        )?;

        let lexicon = OntologyLexicon {
            class_keys: classes
                .into_iter()
                .map(|po| common::ontology::normalize(&po.term_key))
                .collect(),
            relation_keys: relation_types
                .into_iter()
                .map(|po| common::ontology::normalize(&po.term_key))
                .collect(),
            synonyms: synonyms
                .into_iter()
                .filter_map(|po| {
                    // target_kind 解析失败（脏数据）的映射不进词表
                    let kind = po.target_kind.parse().ok()?;
                    Some((
                        (common::ontology::normalize(&po.raw_term), kind),
                        po.target_key,
                    ))
                })
                .collect(),
        };
        Ok(lexicon)
    }

    async fn load_lexicon_summary(&self, ctx: RequestContext) -> Result<OntologyLexiconSummary> {
        let pagination = active_all_pagination();
        let (classes, relation_types, synonyms) = tokio::try_join!(
            self.ontology_dao.query_classes(
                ctx.clone(),
                OntologyClassQuery {
                    status: Some(OntologyStatus::Active),
                    keyword: None,
                    pagination: pagination.clone(),
                },
            ),
            self.ontology_dao.query_relation_types(
                ctx.clone(),
                OntologyRelationTypeQuery {
                    status: Some(OntologyStatus::Active),
                    keyword: None,
                    pagination: pagination.clone(),
                },
            ),
            self.ontology_dao.query_synonyms(
                ctx.clone(),
                OntologySynonymQuery {
                    target_kind: None,
                    target_key: None,
                    keyword: None,
                    pagination,
                },
            ),
        )?;

        warn_if_lexicon_truncated(&ctx, "实体类", classes.items.len());
        warn_if_lexicon_truncated(&ctx, "关系类型", relation_types.items.len());
        warn_if_lexicon_truncated(&ctx, "同义映射", synonyms.items.len());

        Ok(OntologyLexiconSummary {
            // 关系词 > 实体类 > 同义样例：字段顺序即 prompt 注入裁剪优先级（design §3）
            relation_types: relation_types
                .items
                .into_iter()
                .map(|po| LexiconTermSummary {
                    term_key: po.term_key,
                    display_name: po.display_name,
                    description: po.description,
                })
                .collect(),
            classes: classes
                .items
                .into_iter()
                .map(|po| LexiconTermSummary {
                    term_key: po.term_key,
                    display_name: po.display_name,
                    description: po.description,
                })
                .collect(),
            // target_kind 非法属脏数据兜底（写入路径已校验），视图侧跳过不报错
            synonyms: synonyms
                .items
                .into_iter()
                .filter_map(|po| {
                    let target_kind = po.kind()?;
                    Some(LexiconSynonymSummary {
                        raw_term: po.raw_term,
                        target_kind,
                        target_key: po.target_key,
                    })
                })
                .collect(),
        })
    }

    async fn get_drift_dashboard(
        &self,
        ctx: RequestContext,
        request: GetDriftDashboardRequest,
    ) -> Result<GetDriftDashboardResponse> {
        let lexicon = self.load_lexicon(ctx.clone()).await?;
        let top_n = request.top_n.unwrap_or(DEFAULT_TOP_N);

        let relation_rows = self
            .memory_dao
            .word_freq_relations(ctx.clone(), request.agent_id.clone())
            .await?;
        let node_rows = self
            .memory_dao
            .word_freq_nodes(ctx.clone(), request.agent_id)
            .await?;

        let mut canonical_relations = 0u64;
        let mut via_synonym_relations = 0u64;
        let mut drift_relations = 0u64;
        let mut total_node_count = 0u64;
        let mut drift_node_count = 0u64;
        let mut drift_words: Vec<DriftWordItem> = Vec::new();

        for row in relation_rows {
            let count = row.count as u64;
            match resolve(&lexicon, TermKind::Relation, &row.term) {
                ResolvedTerm::Canonical { .. } => canonical_relations += count,
                ResolvedTerm::ViaSynonym { .. } => via_synonym_relations += count,
                ResolvedTerm::Drift { raw_term } => {
                    drift_relations += count;
                    drift_words.push(DriftWordItem {
                        raw_term,
                        kind: TermKind::Relation,
                        count,
                        agent_count: row.agent_count.max(0) as usize,
                    });
                }
            }
        }

        for row in node_rows {
            let count = row.count as u64;
            total_node_count += count;
            match resolve(&lexicon, TermKind::Class, &row.term) {
                ResolvedTerm::Canonical { .. } | ResolvedTerm::ViaSynonym { .. } => {}
                ResolvedTerm::Drift { raw_term } => {
                    drift_node_count += count;
                    drift_words.push(DriftWordItem {
                        raw_term,
                        kind: TermKind::Class,
                        count,
                        agent_count: row.agent_count.max(0) as usize,
                    });
                }
            }
        }

        drift_words.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| a.raw_term.cmp(&b.raw_term))
        });
        drift_words.truncate(top_n);

        let total_relations = canonical_relations + via_synonym_relations + drift_relations;
        let coverage_ratio = if total_relations == 0 {
            0.0
        } else {
            (canonical_relations + via_synonym_relations) as f64 / total_relations as f64
        };

        Ok(GetDriftDashboardResponse {
            top_drift_words: drift_words,
            relation_coverage: DriftCoverage {
                canonical_relations,
                via_synonym_relations,
                drift_relations,
                coverage_ratio,
            },
            drift_node_count,
            total_node_count,
        })
    }

    async fn list_drift_relation_details(
        &self,
        ctx: RequestContext,
        request: ListDriftRelationDetailsRequest,
    ) -> Result<ListDriftRelationDetailsResponse> {
        let page = self
            .memory_dao
            .detail_relations_by_term(ctx, &request.raw_term, request.agent_id, request.pagination)
            .await?;
        Ok(page.map(|row| DriftRelationDetail {
            id: row.id,
            agent_id: row.agent_id,
            source_name: row.source_name,
            target_name: row.target_name,
            created_at: row.created_at,
        }))
    }

    async fn list_drift_class_details(
        &self,
        ctx: RequestContext,
        request: ListDriftClassDetailsRequest,
    ) -> Result<ListDriftClassDetailsResponse> {
        let page = self
            .memory_dao
            .detail_nodes_by_term(ctx, &request.raw_term, request.agent_id, request.pagination)
            .await?;
        Ok(page.map(|row| DriftClassDetail {
            id: row.id,
            agent_id: row.agent_id,
            name: row.name,
            created_at: row.created_at,
        }))
    }

    async fn certify_memory_term(
        &self,
        ctx: RequestContext,
        kind: TermKind,
        raw_term: &str,
    ) -> Result<OntologyCertifyReport> {
        let lexicon = self.load_lexicon(ctx.clone()).await?;
        let verdict = resolve(&lexicon, kind, raw_term);

        let mut report = OntologyCertifyReport::default();
        let publish_key = match &verdict {
            ResolvedTerm::Canonical { term_key } => Some(term_key.clone()),
            ResolvedTerm::ViaSynonym { term_key, .. } => Some(term_key.clone()),
            ResolvedTerm::Drift { .. } => None,
        };
        report.record(kind, verdict);

        if let (Some(key), TermKind::Class) = (publish_key, kind) {
            let published = self
                .memory_dao
                .publish_nodes_by_type(ctx.clone(), &key)
                .await?;
            log_info!(
                &ctx,
                "ontology_certify",
                kind = %kind.as_str(),
                term_key = %key,
                published = published,
                "certify 命中词表，按节点类幂等置位 is_published"
            );
        }

        Ok(report)
    }

    async fn count_drift_events(
        &self,
        ctx: RequestContext,
        query: OntologyDriftQuery,
    ) -> Result<u64> {
        self.stats_dao.count_drift_events(ctx, query).await
    }

    async fn count_drift_by_kind(
        &self,
        ctx: RequestContext,
        query: OntologyDriftQuery,
    ) -> Result<Vec<(String, u64)>> {
        self.stats_dao.count_by_kind(ctx, query).await
    }

    async fn count_drift_by_agent(
        &self,
        ctx: RequestContext,
        query: OntologyDriftQuery,
    ) -> Result<Vec<(String, u64)>> {
        self.stats_dao.count_by_agent(ctx, query).await
    }

    async fn recent_drift_terms(
        &self,
        ctx: RequestContext,
        limit: i64,
        agent_id: Option<String>,
    ) -> Result<Vec<OntologyDriftTermRow>> {
        self.stats_dao.recent_raw_terms(ctx, limit, agent_id).await
    }

    async fn insert_class(&self, ctx: RequestContext, class: &OntologyClass) -> Result<()> {
        self.ontology_dao
            .insert_class(ctx, &class.clone().into_po())
            .await
    }

    async fn find_class_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<OntologyClass>> {
        Ok(self
            .ontology_dao
            .find_class_by_id(ctx, id)
            .await?
            .map(OntologyClass::from_po))
    }

    async fn find_class_by_term_key(
        &self,
        ctx: RequestContext,
        term_key: &str,
    ) -> Result<Option<OntologyClass>> {
        Ok(self
            .ontology_dao
            .find_class_by_term_key(ctx, term_key)
            .await?
            .map(OntologyClass::from_po))
    }

    async fn query_classes(
        &self,
        ctx: RequestContext,
        query: OntologyClassQuery,
    ) -> Result<PagedResult<OntologyClass>> {
        Ok(self
            .ontology_dao
            .query_classes(ctx, query)
            .await?
            .map(OntologyClass::from_po))
    }

    async fn update_class(&self, ctx: RequestContext, class: &OntologyClass) -> Result<()> {
        self.ontology_dao
            .update_class(ctx, &class.clone().into_po())
            .await
    }

    async fn retire_class(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.ontology_dao.retire_class(ctx, id).await
    }

    async fn insert_relation_type(
        &self,
        ctx: RequestContext,
        relation_type: &OntologyRelationType,
    ) -> Result<()> {
        self.ontology_dao
            .insert_relation_type(ctx, &relation_type.clone().into_po())
            .await
    }

    async fn find_relation_type_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<OntologyRelationType>> {
        Ok(self
            .ontology_dao
            .find_relation_type_by_id(ctx, id)
            .await?
            .map(OntologyRelationType::from_po))
    }

    async fn find_relation_type_by_term_key(
        &self,
        ctx: RequestContext,
        term_key: &str,
    ) -> Result<Option<OntologyRelationType>> {
        Ok(self
            .ontology_dao
            .find_relation_type_by_term_key(ctx, term_key)
            .await?
            .map(OntologyRelationType::from_po))
    }

    async fn query_relation_types(
        &self,
        ctx: RequestContext,
        query: OntologyRelationTypeQuery,
    ) -> Result<PagedResult<OntologyRelationType>> {
        Ok(self
            .ontology_dao
            .query_relation_types(ctx, query)
            .await?
            .map(OntologyRelationType::from_po))
    }

    async fn update_relation_type(
        &self,
        ctx: RequestContext,
        relation_type: &OntologyRelationType,
    ) -> Result<()> {
        self.ontology_dao
            .update_relation_type(ctx, &relation_type.clone().into_po())
            .await
    }

    async fn retire_relation_type(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.ontology_dao.retire_relation_type(ctx, id).await
    }

    async fn insert_synonym(
        &self,
        ctx: RequestContext,
        mapping: &OntologySynonymMapping,
    ) -> Result<()> {
        self.ontology_dao
            .insert_synonym(ctx, &mapping.clone().into_po())
            .await
    }

    async fn find_synonyms_by_raw_term(
        &self,
        ctx: RequestContext,
        raw_term: &str,
    ) -> Result<Vec<OntologySynonymMapping>> {
        Ok(self
            .ontology_dao
            .find_synonyms_by_raw_term(ctx, raw_term)
            .await?
            .into_iter()
            .map(OntologySynonymMapping::from_po)
            .collect())
    }

    async fn query_synonyms(
        &self,
        ctx: RequestContext,
        query: OntologySynonymQuery,
    ) -> Result<PagedResult<OntologySynonymMapping>> {
        Ok(self
            .ontology_dao
            .query_synonyms(ctx, query)
            .await?
            .map(OntologySynonymMapping::from_po))
    }

    async fn delete_synonym(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.ontology_dao.delete_synonym(ctx, id).await
    }
}

/// Top N 漂移词默认数量（与 `GetDriftDashboardRequest.top_n` 的 None 语义一致）
const DEFAULT_TOP_N: usize = 20;
