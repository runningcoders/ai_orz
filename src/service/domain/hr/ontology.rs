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
    ListDriftRelationDetailsResponse, PresetOntologySyncItem, PresetOntologySyncStrategy,
    PreviewPresetOntologyResponse,
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

    // ==================== D. seed 预置注入（缺省仅补缺；Overwrite 覆盖式同步见下） ====================

    /// 仅补缺幂等注入（**非原子**）：薄委托 [`Self::apply_default_lexicon_with_strategy`]
    /// 的 OnlyMissing 路径，历史签名与行为零变化；中途失败会留下部分注入结果，幂等
    /// 设计已兜底（失败后重跑或管理页 sync 收敛到完整词表）。调用方请勿假设原子性。
    async fn apply_default_lexicon(
        &self,
        ctx: RequestContext,
        preset: &PresetOntologyLexicon,
    ) -> Result<OntologyLexiconApplyReport> {
        self.apply_default_lexicon_with_strategy(
            ctx,
            preset,
            PresetOntologySyncStrategy::OnlyMissing,
        )
        .await
    }

    /// 按策略注入预置词表（**非原子**；逐条 find → 按策略分流）
    ///
    /// - `OnlyMissing`：term_key / raw_term 已存在（含退役行）即跳过，与历史行为一致；
    /// - `Overwrite`：已存在**非退役**条目按 seed 幂等覆写业务字段（display_name /
    ///   description / required_fields / domain_classes / range_classes / weight_base /
    ///   inverse_key / direction），term_key / id / status / created_at 永不触碰，
    ///   退役行整体跳过（不覆写不复活），用户自拟词（库有 seed 无）不受影响；
    ///   同义映射 (raw, kind) 已存在则覆写 target_key（无退役语义，一律执行动作）。
    ///
    /// `updated_*` 计数 = 执行覆写动作的条目数（非实际字段变化数，TL 口径③）。
    /// 覆写复用 domain 既有 update 原语（NotFound 检查 / 引用校验随原语生效），不新写原语。
    async fn apply_default_lexicon_with_strategy(
        &self,
        ctx: RequestContext,
        preset: &PresetOntologyLexicon,
        strategy: PresetOntologySyncStrategy,
    ) -> Result<OntologyLexiconApplyReport> {
        let mut report = OntologyLexiconApplyReport::default();

        // A. 实体类：不存在新建（两策略同路径）；存在时仅补缺跳过 / 覆盖式幂等覆写
        for p in &preset.classes {
            let key = normalize(&p.term_key);
            let existing = self
                .ontology_dal
                .find_class_by_term_key(ctx.clone(), &key)
                .await?;
            match existing {
                None => {
                    let required_fields =
                        serde_json::to_string(&p.required_fields).unwrap_or_default();
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
                Some(local) => {
                    if strategy == PresetOntologySyncStrategy::Overwrite
                        && local.po.status == OntologyStatus::Active
                    {
                        // 保留 id / term_key / status / created_at 原值，仅覆写业务字段
                        let required_fields =
                            serde_json::to_string(&p.required_fields).unwrap_or_default();
                        let mut po = local.po.clone();
                        po.display_name = p.display_name.clone();
                        po.description = p.description.clone();
                        po.required_fields = required_fields;
                        self.update_class(ctx.clone(), &OntologyClass::from_po(po))
                            .await?;
                        report.updated_classes.push(key);
                    } else {
                        // 仅补缺跳过；覆盖式下退役行整体跳过
                        report.skipped += 1;
                    }
                }
            }
        }

        // B. 关系类型：同策略分流；引用校验（classes 段已先行注入，直接查库验证）
        for p in &preset.relation_types {
            let key = normalize(&p.term_key);
            let existing = self
                .ontology_dal
                .find_relation_type_by_term_key(ctx.clone(), &key)
                .await?;
            match existing {
                None => {
                    let domain_classes =
                        serde_json::to_string(&p.domain_classes).unwrap_or_default();
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
                        p.direction.as_str(),
                    );
                    self.ontology_dal
                        .insert_relation_type(ctx.clone(), &OntologyRelationType::from_po(po))
                        .await?;
                    report.inserted_relation_types.push(key);
                }
                Some(local) => {
                    if strategy == PresetOntologySyncStrategy::Overwrite
                        && local.po.status == OntologyStatus::Active
                    {
                        let domain_classes =
                            serde_json::to_string(&p.domain_classes).unwrap_or_default();
                        let range_classes =
                            serde_json::to_string(&p.range_classes).unwrap_or_default();
                        // 引用校验与新建路径同口径 fail fast
                        self.validate_class_refs(&ctx, &domain_classes, "domain_classes")
                            .await?;
                        self.validate_class_refs(&ctx, &range_classes, "range_classes")
                            .await?;
                        // 保留 id / term_key / status / created_at 原值，仅覆写业务字段
                        let mut po = local.po.clone();
                        po.display_name = p.display_name.clone();
                        po.description = p.description.clone();
                        po.domain_classes = domain_classes;
                        po.range_classes = range_classes;
                        po.weight_base = p.weight_base.unwrap_or(1.0);
                        po.inverse_key = p.inverse_key.clone();
                        po.direction = p.direction.clone();
                        self.update_relation_type(ctx.clone(), &OntologyRelationType::from_po(po))
                            .await?;
                        report.updated_relation_types.push(key);
                    } else {
                        // 仅补缺跳过；覆盖式下退役行整体跳过
                        report.skipped += 1;
                    }
                }
            }
        }

        // C. 同义映射：(raw, kind) 维度判定——不存在新建（目标存在性 fail fast）；
        //    存在时仅补缺跳过 / 覆盖式一律执行覆写动作（归一化 target_key，同值幂等静默）
        for p in &preset.synonym_mappings {
            let raw = normalize(&p.raw_term);
            let existing = self
                .ontology_dal
                .find_synonyms_by_raw_term(ctx.clone(), &raw)
                .await?;
            let matched = existing
                .iter()
                .find(|s| s.po.target_kind == p.target_kind.as_str());
            match matched {
                None => {
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
                Some(local) => {
                    if strategy == PresetOntologySyncStrategy::Overwrite {
                        // 同义映射无退役语义，一律执行覆写动作（计数=动作数，TL 口径③）
                        let target_key = normalize(&p.target_key);
                        self.ontology_dal
                            .update_synonym(ctx.clone(), &local.po.id, &target_key)
                            .await?;
                        report.updated_synonyms += 1;
                    } else {
                        report.skipped += 1;
                    }
                }
            }
        }

        Ok(report)
    }

    /// 预置词表同步预览（只读，不写库）：seed 与本地现值逐条对比
    ///
    /// 缺口计数复用 [`Self::preview_lexicon_gaps`]（判定同源：归一化 term_key 物理
    /// 存在即算已存在，含退役行）；条目细节（local_* / diff_fields / retired /
    /// direction 对比）逐条 find 取本地现值。同义映射维持「以计数呈现」原口径
    /// （raw→target 映射无显示名不逐条展示），其覆盖贡献由 existing_count 反推
    /// （无退役语义，全部计入 overwrite_count）。退役行不参与覆写，diff_fields
    /// 置空（跳过语义由 retired 标记承载）。
    async fn build_preset_sync_preview(
        &self,
        ctx: RequestContext,
        preset: &PresetOntologyLexicon,
    ) -> Result<PreviewPresetOntologyResponse> {
        let gaps = self.preview_lexicon_gaps(ctx.clone(), preset).await?;

        let mut items: Vec<PresetOntologySyncItem> = Vec::new();
        let mut overwrite_count = 0usize;
        let mut retired_count = 0usize;
        let mut existing_classes = 0usize;
        let mut existing_relations = 0usize;

        // A. 实体类：逐条 find 取本地现值（与 gaps 判定同键同库）
        for p in &preset.classes {
            let key = normalize(&p.term_key);
            let local = self
                .ontology_dal
                .find_class_by_term_key(ctx.clone(), &key)
                .await?;
            let Some(entity) = local else {
                items.push(PresetOntologySyncItem {
                    kind: TermKind::Class,
                    term_key: p.term_key.clone(),
                    display_name: p.display_name.clone(),
                    description: p.description.clone(),
                    exists: false,
                    local_display_name: None,
                    local_description: None,
                    diff_fields: Vec::new(),
                    retired: false,
                    seed_direction: None,
                    local_direction: None,
                });
                continue;
            };
            existing_classes += 1;
            let retired = entity.po.status != OntologyStatus::Active;
            if retired {
                retired_count += 1;
            } else {
                overwrite_count += 1;
            }
            let seed_required = serde_json::to_string(&p.required_fields).unwrap_or_default();
            let mut diff_fields: Vec<String> = Vec::new();
            if entity.po.display_name != p.display_name {
                diff_fields.push("display_name".to_string());
            }
            if entity.po.description != p.description {
                diff_fields.push("description".to_string());
            }
            if entity.po.required_fields != seed_required {
                diff_fields.push("required_fields".to_string());
            }
            // 退役行整体跳过不覆写，diff 不进入影响面（retired 标记承载跳过语义）
            if retired {
                diff_fields.clear();
            }
            items.push(PresetOntologySyncItem {
                kind: TermKind::Class,
                term_key: p.term_key.clone(),
                display_name: p.display_name.clone(),
                description: p.description.clone(),
                exists: true,
                local_display_name: Some(entity.po.display_name.clone()),
                local_description: Some(entity.po.description.clone()),
                diff_fields,
                retired,
                seed_direction: None,
                local_direction: None,
            });
        }

        // B. 关系类型：逐条 find 取本地现值 + direction 对比（本专项核心字段）
        for p in &preset.relation_types {
            let key = normalize(&p.term_key);
            let local = self
                .ontology_dal
                .find_relation_type_by_term_key(ctx.clone(), &key)
                .await?;
            let Some(entity) = local else {
                items.push(PresetOntologySyncItem {
                    kind: TermKind::Relation,
                    term_key: p.term_key.clone(),
                    display_name: p.display_name.clone(),
                    description: p.description.clone(),
                    exists: false,
                    local_display_name: None,
                    local_description: None,
                    diff_fields: Vec::new(),
                    retired: false,
                    seed_direction: Some(p.direction.clone()),
                    local_direction: None,
                });
                continue;
            };
            existing_relations += 1;
            let retired = entity.po.status != OntologyStatus::Active;
            if retired {
                retired_count += 1;
            } else {
                overwrite_count += 1;
            }
            let seed_domain = serde_json::to_string(&p.domain_classes).unwrap_or_default();
            let seed_range = serde_json::to_string(&p.range_classes).unwrap_or_default();
            let mut diff_fields: Vec<String> = Vec::new();
            if entity.po.display_name != p.display_name {
                diff_fields.push("display_name".to_string());
            }
            if entity.po.description != p.description {
                diff_fields.push("description".to_string());
            }
            if entity.po.domain_classes != seed_domain {
                diff_fields.push("domain_classes".to_string());
            }
            if entity.po.range_classes != seed_range {
                diff_fields.push("range_classes".to_string());
            }
            if entity.po.weight_base != p.weight_base.unwrap_or(1.0) {
                diff_fields.push("weight_base".to_string());
            }
            if entity.po.inverse_key != p.inverse_key {
                diff_fields.push("inverse_key".to_string());
            }
            if entity.po.direction != p.direction {
                diff_fields.push("direction".to_string());
            }
            // 退役行整体跳过不覆写，diff 不进入影响面（retired 标记承载跳过语义）
            if retired {
                diff_fields.clear();
            }
            items.push(PresetOntologySyncItem {
                kind: TermKind::Relation,
                term_key: p.term_key.clone(),
                display_name: p.display_name.clone(),
                description: p.description.clone(),
                exists: true,
                local_display_name: Some(entity.po.display_name.clone()),
                local_description: Some(entity.po.description.clone()),
                diff_fields,
                retired,
                seed_direction: Some(p.direction.clone()),
                local_direction: Some(entity.po.direction.clone()),
            });
        }

        // 同义映射覆盖贡献：existing_count 反推（无退役语义全覆盖计入）
        let existing_synonyms = gaps
            .skipped
            .saturating_sub(existing_classes)
            .saturating_sub(existing_relations);
        overwrite_count += existing_synonyms;

        Ok(PreviewPresetOntologyResponse {
            items,
            missing_count: gaps.missing_classes.len()
                + gaps.missing_relation_types.len()
                + gaps.missing_synonyms,
            existing_count: gaps.skipped,
            overwrite_count,
            retired_count,
        })
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
                        direction: po.direction,
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
