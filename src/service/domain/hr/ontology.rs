//! Ontology Domain - 本体词表领域服务
//!
//! 实现 [`OntologyDomain`]（七组 21 方法），职责（docs/design/ontology_knowledge_sedimentation_design.md §2.6）：
//! - **词表 CRUD 编排**：写侧校验（JSON 字段合法性 + term_key 语义锚点非空 +
//!   domain/range 引用完整性（含退役词拦截）+ 同义映射目标存在性）；
//!   归一化由 DAO 写入侧单点完成（common::ontology::normalize），本层不覆写参数
//! - **seed 预置注入**：[`OntologyDomain::apply_default_lexicon`] 仅补缺幂等注入
//! - **写后认证**：[`OntologyDomain::certify_memory_terms`] 逐词条 resolve +
//!   命中置位（软门禁，Drift 零写动作）
//! - **词表视图 / 漂移看板**：神经技能提示词注入裁剪视图 + 惰性聚合透传
//!
//! 分层纪律：本模块只依赖 [`crate::service::dal::ontology::OntologyDal`]
//! （PO↔Entity 转换已在 DAL 完成），不触碰 DAO / PO（CODE_STANDARDS §2 分层红线）。

use crate::models::ontology::{
    OntologyClass, OntologyClassPo, OntologyRelationType, OntologyRelationTypePo,
    OntologySynonymMapping, OntologySynonymMappingPo,
};
use crate::pkg::RequestContext;
use crate::service::dal::ontology::{active_all_pagination, warn_if_lexicon_truncated};
use crate::service::dao::ontology::{
    OntologyClassQuery, OntologyRelationTypeQuery, OntologySynonymQuery,
};
use crate::service::domain::hr::{HrDomainImpl, OntologyDomain};
use common::api::ontology::{
    GetDriftDashboardRequest, GetDriftDashboardResponse, ListDriftClassDetailsRequest,
    ListDriftClassDetailsResponse, ListDriftRelationDetailsRequest,
    ListDriftRelationDetailsResponse,
};
use common::enums::OntologyStatus;
use common::error::{Result, bail_err, err};
use common::ontology::{
    LexiconGapReport, MAX_TERM_KEY_LEN, OntologyCertifyReport, OntologyLexiconApplyReport,
    OntologyLexiconSummary, PresetOntologyClass, PresetOntologyLexicon, PresetOntologyRelationType,
    PresetOntologySynonym, TermKind, normalize,
};

/// 校验 JSON 数组字符串合法性（空串 = 空清单，合法）
///
/// `required_fields` / `domain_classes` / `range_classes` 三处共用；
/// models 层 parse 助手对非法 JSON 静默返空，写侧在 Domain 强化校验。
fn validate_json_list(raw: &str, field: &str) -> Result<Vec<String>> {
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(raw)
        .map_err(|e| err!(InvalidRequest, "{} 不是合法的 JSON 数组: {}", field, e))
}

impl HrDomainImpl {
    /// 校验 JSON 数组字段引用的实体类全部存在（domain_classes / range_classes 共用）
    ///
    /// 引用的是语义锚点 term_key（非 id）；预置注入与手动 CRUD 共用本方法，
    /// 保证「手动创建」与「seed 注入」两条写入路径的引用完整性口径一致。
    async fn validate_class_refs(
        &self,
        ctx: &RequestContext,
        json: &str,
        field: &str,
    ) -> Result<()> {
        for key in validate_json_list(json, field)? {
            // 退役词（Retired）仅保留历史引用可解释，写侧不允许新引用
            match self
                .ontology_dal
                .find_class_by_term_key(ctx.clone(), &key)
                .await?
            {
                None => bail_err!(InvalidRequest, "{} 引用的实体类 {} 不存在", field, key),
                Some(class) if class.po.status != OntologyStatus::Active => bail_err!(
                    InvalidRequest,
                    "{} 引用的实体类 {} 已退役，不允许新引用",
                    field,
                    key
                ),
                Some(_) => {}
            }
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl OntologyDomain for HrDomainImpl {
    // ==================== A. 实体类 CRUD ====================

    async fn create_class(&self, ctx: RequestContext, class: &OntologyClass) -> Result<()> {
        // 校验基于规范形（与 DAO 落库值同口径）；归一由 DAO 单点完成，此处不覆写参数
        let term_key = normalize(&class.po.term_key);
        if term_key.is_empty() {
            bail_err!(InvalidRequest, "实体类 term_key 不能为空");
        }
        if term_key.chars().count() > MAX_TERM_KEY_LEN {
            bail_err!(
                InvalidRequest,
                "实体类 term_key 超长（上限 {} 字符）",
                MAX_TERM_KEY_LEN
            );
        }
        validate_json_list(&class.po.required_fields, "required_fields")?;
        self.ontology_dal.insert_class(ctx, class).await
    }

    async fn get_class(&self, ctx: RequestContext, id: &str) -> Result<Option<OntologyClass>> {
        self.ontology_dal.find_class_by_id(ctx, id).await
    }

    async fn list_classes(
        &self,
        ctx: RequestContext,
        query: OntologyClassQuery,
    ) -> Result<common::api::PagedResult<OntologyClass>> {
        self.ontology_dal.query_classes(ctx, query).await
    }

    async fn update_class(&self, ctx: RequestContext, class: &OntologyClass) -> Result<()> {
        validate_json_list(&class.po.required_fields, "required_fields")?;
        // DAO update 按 id 行更新、不存在静默成功；NotFound 语义由 Domain 补齐
        if self
            .ontology_dal
            .find_class_by_id(ctx.clone(), &class.po.id)
            .await?
            .is_none()
        {
            return Err(err!(NotFound, "实体类 {} 不存在", class.po.id));
        }
        self.ontology_dal.update_class(ctx, class).await
    }

    async fn retire_class(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.ontology_dal.retire_class(ctx, id).await
    }

    // ==================== B. 关系类型 CRUD ====================

    async fn create_relation_type(
        &self,
        ctx: RequestContext,
        relation_type: &OntologyRelationType,
    ) -> Result<()> {
        // 校验基于规范形（与 DAO 落库值同口径）；归一由 DAO 单点完成，此处不覆写参数
        let term_key = normalize(&relation_type.po.term_key);
        if term_key.is_empty() {
            bail_err!(InvalidRequest, "关系类型 term_key 不能为空");
        }
        if term_key.chars().count() > MAX_TERM_KEY_LEN {
            bail_err!(
                InvalidRequest,
                "关系类型 term_key 超长（上限 {} 字符）",
                MAX_TERM_KEY_LEN
            );
        }
        self.validate_class_refs(&ctx, &relation_type.po.domain_classes, "domain_classes")
            .await?;
        self.validate_class_refs(&ctx, &relation_type.po.range_classes, "range_classes")
            .await?;
        self.ontology_dal
            .insert_relation_type(ctx, relation_type)
            .await
    }

    async fn get_relation_type(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<OntologyRelationType>> {
        self.ontology_dal.find_relation_type_by_id(ctx, id).await
    }

    async fn list_relation_types(
        &self,
        ctx: RequestContext,
        query: OntologyRelationTypeQuery,
    ) -> Result<common::api::PagedResult<OntologyRelationType>> {
        self.ontology_dal.query_relation_types(ctx, query).await
    }

    async fn update_relation_type(
        &self,
        ctx: RequestContext,
        relation_type: &OntologyRelationType,
    ) -> Result<()> {
        if self
            .ontology_dal
            .find_relation_type_by_id(ctx.clone(), &relation_type.po.id)
            .await?
            .is_none()
        {
            return Err(err!(NotFound, "关系类型 {} 不存在", relation_type.po.id));
        }
        // 引用校验同新增（trait 契约；term_key 不可变由 DAO UPDATE 语句保证）
        self.validate_class_refs(&ctx, &relation_type.po.domain_classes, "domain_classes")
            .await?;
        self.validate_class_refs(&ctx, &relation_type.po.range_classes, "range_classes")
            .await?;
        self.ontology_dal
            .update_relation_type(ctx, relation_type)
            .await
    }

    async fn retire_relation_type(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.ontology_dal.retire_relation_type(ctx, id).await
    }

    // ==================== C. 同义映射 ====================

    async fn create_synonym(
        &self,
        ctx: RequestContext,
        mapping: &OntologySynonymMapping,
    ) -> Result<()> {
        let kind = match mapping.po.target_kind.parse::<TermKind>() {
            Ok(kind) => kind,
            Err(_) => bail_err!(InvalidRequest, "target_kind 必须是 class / relation"),
        };
        // target 存在性校验（按 target_kind 分流到对应词表）
        match kind {
            TermKind::Class => {
                if self
                    .ontology_dal
                    .find_class_by_term_key(ctx.clone(), &mapping.po.target_key)
                    .await?
                    .is_none()
                {
                    bail_err!(
                        InvalidRequest,
                        "同义映射目标实体类 {} 不存在",
                        mapping.po.target_key
                    );
                }
            }
            TermKind::Relation => {
                if self
                    .ontology_dal
                    .find_relation_type_by_term_key(ctx.clone(), &mapping.po.target_key)
                    .await?
                    .is_none()
                {
                    bail_err!(
                        InvalidRequest,
                        "同义映射目标关系类型 {} 不存在",
                        mapping.po.target_key
                    );
                }
            }
        }
        // raw_term 校验基于规范形（与 DAO 落库值同口径）；归一由 DAO 单点完成，此处不覆写参数
        let raw_term = normalize(&mapping.po.raw_term);
        if raw_term.is_empty() {
            bail_err!(InvalidRequest, "同义映射 raw_term 不能为空");
        }
        if raw_term.chars().count() > MAX_TERM_KEY_LEN {
            bail_err!(
                InvalidRequest,
                "同义映射 raw_term 超长（上限 {} 字符）",
                MAX_TERM_KEY_LEN
            );
        }
        self.ontology_dal.insert_synonym(ctx, mapping).await
    }

    async fn list_synonyms(
        &self,
        ctx: RequestContext,
        query: OntologySynonymQuery,
    ) -> Result<common::api::PagedResult<OntologySynonymMapping>> {
        self.ontology_dal.query_synonyms(ctx, query).await
    }

    async fn delete_synonym(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.ontology_dal.delete_synonym(ctx, id).await
    }

    // ==================== D. seed 预置注入（仅补缺） ====================

    /// 仅补缺幂等注入（**非原子**）：逐条 find → skip or insert，中途失败会留下
    /// 部分注入结果。这是有意取舍——真正事务化要求 DAO 全套 find/insert 支持事务
    /// executor，改动面大；幂等设计已兜底：失败后重跑（或管理页 sync）会跳过
    /// 已注入条目、补齐缺失条目，最终收敛到完整词表。调用方请勿假设原子性。
    async fn apply_default_lexicon(
        &self,
        ctx: RequestContext,
        preset: &PresetOntologyLexicon,
    ) -> Result<OntologyLexiconApplyReport> {
        let mut report = OntologyLexiconApplyReport::default();

        // A. 实体类：term_key 归一化比对，已存在跳过（seed 不覆盖管理页本地修改）
        for p in &preset.classes {
            let key = normalize(&p.term_key);
            if self
                .ontology_dal
                .find_class_by_term_key(ctx.clone(), &key)
                .await?
                .is_some()
            {
                report.skipped += 1;
                continue;
            }
            let required_fields = serde_json::to_string(&p.required_fields).unwrap_or_default();
            let po = OntologyClassPo::new(
                key.clone(),
                &p.display_name,
                &p.description,
                required_fields,
            );
            self.ontology_dal
                .insert_class(ctx.clone(), &OntologyClass::from_po(po))
                .await?;
            report.inserted_classes.push(key);
        }

        // B. 关系类型：同策略；引用校验（classes 段已先行注入，直接查库验证）
        for p in &preset.relation_types {
            let key = normalize(&p.term_key);
            if self
                .ontology_dal
                .find_relation_type_by_term_key(ctx.clone(), &key)
                .await?
                .is_some()
            {
                report.skipped += 1;
                continue;
            }
            let domain_classes = serde_json::to_string(&p.domain_classes).unwrap_or_default();
            let range_classes = serde_json::to_string(&p.range_classes).unwrap_or_default();
            // 预置快照引用不存在的实体类 = 快照残缺，fail fast 不静默
            self.validate_class_refs(&ctx, &domain_classes, "domain_classes")
                .await?;
            self.validate_class_refs(&ctx, &range_classes, "range_classes")
                .await?;
            let po = OntologyRelationTypePo::new(
                key.clone(),
                &p.display_name,
                &p.description,
                domain_classes,
                range_classes,
                p.weight_base.unwrap_or(1.0),
                p.inverse_key.clone(),
            );
            self.ontology_dal
                .insert_relation_type(ctx.clone(), &OntologyRelationType::from_po(po))
                .await?;
            report.inserted_relation_types.push(key);
        }

        // C. 同义映射：raw_term 归一化 + UNIQUE(raw_term, target_kind) 维度查重
        //    （同一 raw 词可分别映射 class / relation 两类，只按 kind 判定已存在——
        //    同 (raw, kind) 不同 target_key 也不覆盖：仅补缺不改既有映射）
        for p in &preset.synonym_mappings {
            let raw = normalize(&p.raw_term);
            let existing = self
                .ontology_dal
                .find_synonyms_by_raw_term(ctx.clone(), &raw)
                .await?;
            if existing
                .iter()
                .any(|s| s.po.target_kind == p.target_kind.as_str())
            {
                report.skipped += 1;
                continue;
            }
            // 目标存在性校验（快照残缺 fail fast，口径与关系类型引用校验一致）
            let target_key = normalize(&p.target_key);
            match p.target_kind {
                TermKind::Class => {
                    if self
                        .ontology_dal
                        .find_class_by_term_key(ctx.clone(), &target_key)
                        .await?
                        .is_none()
                    {
                        bail_err!(
                            InvalidRequest,
                            "预置同义映射目标实体类 {} 不存在",
                            p.target_key
                        );
                    }
                }
                TermKind::Relation => {
                    if self
                        .ontology_dal
                        .find_relation_type_by_term_key(ctx.clone(), &target_key)
                        .await?
                        .is_none()
                    {
                        bail_err!(
                            InvalidRequest,
                            "预置同义映射目标关系类型 {} 不存在",
                            p.target_key
                        );
                    }
                }
            }
            let po = OntologySynonymMappingPo::new(raw, p.target_kind.as_str(), target_key);
            self.ontology_dal
                .insert_synonym(ctx.clone(), &OntologySynonymMapping::from_po(po))
                .await?;
            report.inserted_synonyms += 1;
        }

        Ok(report)
    }

    async fn export_lexicon(&self, ctx: RequestContext) -> Result<PresetOntologyLexicon> {
        let (classes, relation_types, synonyms) = tokio::try_join!(
            self.ontology_dal.query_classes(
                ctx.clone(),
                OntologyClassQuery {
                    status: Some(OntologyStatus::Active),
                    keyword: None,
                    pagination: active_all_pagination(),
                },
            ),
            self.ontology_dal.query_relation_types(
                ctx.clone(),
                OntologyRelationTypeQuery {
                    status: Some(OntologyStatus::Active),
                    keyword: None,
                    pagination: active_all_pagination(),
                },
            ),
            self.ontology_dal.query_synonyms(
                ctx.clone(),
                OntologySynonymQuery {
                    target_kind: None,
                    target_key: None,
                    keyword: None,
                    pagination: active_all_pagination(),
                },
            ),
        )?;

        warn_if_lexicon_truncated(&ctx, "实体类", classes.items.len());
        warn_if_lexicon_truncated(&ctx, "关系类型", relation_types.items.len());
        warn_if_lexicon_truncated(&ctx, "同义映射", synonyms.items.len());

        Ok(PresetOntologyLexicon {
            classes: classes
                .items
                .into_iter()
                .map(|c| {
                    let po = c.po;
                    // 先借调解析器再 move 字段，避免 partial move
                    let required_fields = po.parse_required_fields();
                    PresetOntologyClass {
                        term_key: po.term_key,
                        display_name: po.display_name,
                        description: po.description,
                        required_fields,
                    }
                })
                .collect(),
            relation_types: relation_types
                .items
                .into_iter()
                .map(|r| {
                    let po = r.po;
                    let domain_classes = po.parse_domain_classes();
                    let range_classes = po.parse_range_classes();
                    PresetOntologyRelationType {
                        term_key: po.term_key,
                        display_name: po.display_name,
                        description: po.description,
                        domain_classes,
                        range_classes,
                        weight_base: Some(po.weight_base),
                        inverse_key: po.inverse_key,
                    }
                })
                .collect(),
            // kind 解析失败 = 兜底脏数据（写路径已校验），导出侧跳过不阻断快照装配
            synonym_mappings: synonyms
                .items
                .into_iter()
                .filter_map(|s| {
                    let po = s.po;
                    let target_kind = po.kind()?;
                    Some(PresetOntologySynonym {
                        raw_term: po.raw_term,
                        target_kind,
                        target_key: po.target_key,
                    })
                })
                .collect(),
        })
    }

    async fn preview_lexicon_gaps(
        &self,
        ctx: RequestContext,
        preset: &PresetOntologyLexicon,
    ) -> Result<LexiconGapReport> {
        let mut report = LexiconGapReport::default();

        // 判定与 apply_default_lexicon 的跳过逻辑同源（归一化 term_key 物理存在
        // 即算已存在，含退役行），保证 preview "将新增" 与 sync 实际结果一致
        for p in &preset.classes {
            let key = normalize(&p.term_key);
            if self
                .ontology_dal
                .find_class_by_term_key(ctx.clone(), &key)
                .await?
                .is_some()
            {
                report.skipped += 1;
            } else {
                report.missing_classes.push(key);
            }
        }

        for p in &preset.relation_types {
            let key = normalize(&p.term_key);
            if self
                .ontology_dal
                .find_relation_type_by_term_key(ctx.clone(), &key)
                .await?
                .is_some()
            {
                report.skipped += 1;
            } else {
                report.missing_relation_types.push(key);
            }
        }

        for p in &preset.synonym_mappings {
            let raw = normalize(&p.raw_term);
            let existing = self
                .ontology_dal
                .find_synonyms_by_raw_term(ctx.clone(), &raw)
                .await?;
            if existing
                .iter()
                .any(|s| s.po.target_kind == p.target_kind.as_str())
            {
                report.skipped += 1;
            } else {
                report.missing_synonyms += 1;
            }
        }

        Ok(report)
    }

    // ==================== E. 写后认证 ====================

    async fn certify_memory_terms(
        &self,
        ctx: RequestContext,
        agent_id: Option<String>,
        terms: Vec<(TermKind, String)>,
    ) -> Result<OntologyCertifyReport> {
        log_info!(
            ctx,
            "certify_memory_terms",
            "agent_id={:?} terms={} 批量写后认证开始",
            agent_id,
            terms.len()
        );
        let mut report = OntologyCertifyReport::default();
        for (kind, raw) in terms {
            // 逐词条走 DAL 认证（词表量小，重复 load_lexicon 成本可忽略；
            // DAL 单词条契约保持不变，不为批量优化加新 DAL 方法）
            let sub = self
                .ontology_dal
                .certify_memory_term(ctx.clone(), kind, &raw)
                .await?;
            report.entries.extend(sub.entries);
        }
        Ok(report)
    }

    // ==================== F. 词表视图（神经技能提示词注入） ====================

    async fn list_lexicon(&self, ctx: RequestContext) -> Result<OntologyLexiconSummary> {
        // 转换逻辑单点下沉 DAL（runtime prompt builder 同源消费），本层只透传
        self.ontology_dal.load_lexicon_summary(ctx).await
    }

    // ==================== G. 漂移看板（薄透传 DAL 惰性聚合） ====================

    async fn get_drift_dashboard(
        &self,
        ctx: RequestContext,
        request: GetDriftDashboardRequest,
    ) -> Result<GetDriftDashboardResponse> {
        self.ontology_dal.get_drift_dashboard(ctx, request).await
    }

    async fn list_drift_relation_details(
        &self,
        ctx: RequestContext,
        request: ListDriftRelationDetailsRequest,
    ) -> Result<ListDriftRelationDetailsResponse> {
        self.ontology_dal
            .list_drift_relation_details(ctx, request)
            .await
    }

    async fn list_drift_class_details(
        &self,
        ctx: RequestContext,
        request: ListDriftClassDetailsRequest,
    ) -> Result<ListDriftClassDetailsResponse> {
        self.ontology_dal
            .list_drift_class_details(ctx, request)
            .await
    }
}
