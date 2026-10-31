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
    OntologyClass, OntologyClassPo, OntologyClassRelation, OntologyClassRelationPo,
    OntologyProperty, OntologyPropertyPo, OntologyRelationType, OntologyRelationTypePo,
    OntologySynonymMapping, OntologySynonymMappingPo,
};
use crate::pkg::RequestContext;
use crate::service::dal::ontology::{active_all_pagination, warn_if_lexicon_truncated};
use crate::service::dao::ontology::{
    OntologyClassQuery, OntologyClassRelationQuery, OntologyPropertyQuery,
    OntologyRelationTypeQuery, OntologySynonymQuery,
};
use crate::service::domain::hr::{HrDomainImpl, OntologyDomain};
use common::api::ontology::{
    GetDriftDashboardRequest, GetDriftDashboardResponse, GetOntologySkeletonResponse,
    GetPropertyEffectivenessRequest, GetPropertyEffectivenessResponse,
    ListDriftClassDetailsRequest, ListDriftClassDetailsResponse, ListDriftRelationDetailsRequest,
    ListDriftRelationDetailsResponse, OntologyClassItem, OntologyClassRelationItem,
    OntologyRelationTypeItem, PresetOntologySyncItem, PresetOntologySyncStrategy,
    PreviewPresetOntologyResponse,
};
use common::enums::OntologyStatus;
use common::enums::ontology::{Cardinality, PropertyDataType, PropertyTargetType};
use common::error::{Result, bail_err, err};
use common::ontology::{
    LexiconGapReport, MAX_TERM_KEY_LEN, OntologyCertifyReport, OntologyLexiconApplyReport,
    OntologyLexiconSummary, PresetOntologyClass, PresetOntologyClassRelation,
    PresetOntologyLexicon, PresetOntologyProperty, PresetOntologyRelationType,
    PresetOntologySynonym, TermKind, normalize,
};
use common::ontology_property::parse_applies_to;

/// 校验 JSON 数组字符串合法性（空串 = 空清单，合法）
///
/// `required_fields` / 属性的 `domain_classes` 等 JSON 数组字段共用；
/// models 层 parse 助手对非法 JSON 静默返空，写侧在 Domain 强化校验。
fn validate_json_list(raw: &str, field: &str) -> Result<Vec<String>> {
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(raw)
        .map_err(|e| err!(InvalidRequest, "{} 不是合法的 JSON 数组: {}", field, e))
}

/// 校验属性定义的**结构化字段**（写侧强校验，脏定义不放进词表）
///
/// 与 [`validate_json_list`] 的分工：后者只管「是不是合法 JSON 数组」，
/// 这里管「取值是不是本域承认的枚举」——
///
/// - `data_type` / `cardinality`：必须是 DDL CHECK 里的取值。
///   models 层的 parse 助手会**静默兜底**（非法退 string / single），
///   写侧不能接受这种静默：管理员以为写的是 number，实际落的是 string，
///   之后所有数值属性都写不进 `value_num`，范围查询全废且极难排查。
/// - `applies_to`：逐个校验载体名。common 层的 `parse_applies_to` 会**静默丢弃**
///   拼错的项（结果是「谁都不让挂」），写侧必须把拼错暴露成 400。
/// - `value_range`：必须是数组（枚举白名单）或对象（数值区间 / 带 enum 键）两种形态之一。
fn validate_property_definition(po: &OntologyPropertyPo) -> Result<()> {
    if po.data_type.parse::<PropertyDataType>().is_err() {
        bail_err!(
            InvalidRequest,
            "属性 data_type 取值非法: {}（合法值: {}）",
            po.data_type,
            PropertyDataType::ACCEPTED_VALUES
        );
    }
    if po.cardinality.parse::<Cardinality>().is_err() {
        bail_err!(
            InvalidRequest,
            "属性 cardinality 取值非法: {}（合法值: {}）",
            po.cardinality,
            Cardinality::ACCEPTED_VALUES
        );
    }
    for raw in validate_json_list(&po.applies_to, "applies_to")? {
        if raw.parse::<PropertyTargetType>().is_err() {
            bail_err!(
                InvalidRequest,
                "applies_to 含非法载体: {}（合法值: {}）",
                raw,
                PropertyTargetType::ACCEPTED_VALUES
            );
        }
    }
    // value_range 允许两种形态：数组 = 枚举白名单；对象 = {min,max,enum}
    if !po.value_range.trim().is_empty() {
        match serde_json::from_str::<serde_json::Value>(&po.value_range) {
            Ok(serde_json::Value::Array(_)) | Ok(serde_json::Value::Object(_)) => {}
            _ => bail_err!(
                InvalidRequest,
                "value_range 必须是 JSON 数组（枚举白名单）或对象（{{min,max,enum}}）"
            ),
        }
    }
    Ok(())
}

impl HrDomainImpl {
    /// 校验 JSON 数组字段引用的实体类全部存在（属性 domain_classes 等共用）
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
            self.validate_class_ref(ctx, &key, field).await?;
        }
        Ok(())
    }

    /// 校验单个实体类引用存在且未退役
    async fn validate_class_ref(&self, ctx: &RequestContext, key: &str, field: &str) -> Result<()> {
        // 退役词（Retired）仅保留历史引用可解释，写侧不允许新引用
        match self
            .ontology_dal
            .find_class_by_term_key(ctx.clone(), key)
            .await?
        {
            None => bail_err!(InvalidRequest, "{} 引用的实体类 {} 不存在", field, key),
            Some(class) if class.po.status != OntologyStatus::Active => bail_err!(
                InvalidRequest,
                "{} 引用的实体类 {} 已退役，不允许新引用",
                field,
                key
            ),
            Some(_) => Ok(()),
        }
    }

    /// 校验实体类父级指针：父类存在且 Active、禁止自指与成环
    ///
    /// 环检测沿父链向上逐级查库（词表量级小，最多十几次点查），命中自身即拒绝。
    async fn validate_class_parent(
        &self,
        ctx: &RequestContext,
        class_key: &str,
        parent_key: Option<&str>,
    ) -> Result<()> {
        let Some(parent_key) = parent_key else {
            return Ok(());
        };
        let class_key = normalize(class_key);
        let parent_key = normalize(parent_key);
        if parent_key.is_empty() {
            bail_err!(InvalidRequest, "parent_term_key 不能为空串（根类请留空）");
        }
        if parent_key == class_key {
            bail_err!(InvalidRequest, "实体类不能以自身为父类");
        }
        self.validate_class_ref(ctx, &parent_key, "parent_term_key")
            .await?;

        // 环检测：从父类沿 parent 链上溯，回到自身即成环
        let mut current = Some(parent_key);
        let mut visited = std::collections::HashSet::new();
        while let Some(key) = current {
            if !visited.insert(key.clone()) {
                // 已存在环（历史脏数据），写入新边会加深环，直接拒绝
                bail_err!(InvalidRequest, "实体类继承链存在环，拒绝写入");
            }
            if key == class_key {
                bail_err!(
                    InvalidRequest,
                    "实体类继承链成环：{} 的祖先链回到了自身",
                    class_key
                );
            }
            current = self
                .ontology_dal
                .find_class_by_term_key(ctx.clone(), &key)
                .await?
                .and_then(|class| class.po.parent_term_key)
                .map(|p| normalize(&p));
        }
        Ok(())
    }

    /// 校验骨架三元组三个 key：关系词与两侧实体类必须存在且 Active（`*` 通配放行）
    async fn validate_class_relation_refs(
        &self,
        ctx: &RequestContext,
        relation: &OntologyClassRelationPo,
    ) -> Result<()> {
        let relation_key = normalize(&relation.relation_key);
        if relation_key.is_empty() {
            bail_err!(InvalidRequest, "骨架三元组 relation_key 不能为空");
        }
        match self
            .ontology_dal
            .find_relation_type_by_term_key(ctx.clone(), &relation_key)
            .await?
        {
            None => bail_err!(
                InvalidRequest,
                "骨架三元组引用的关系类型 {} 不存在",
                relation_key
            ),
            Some(entity) if entity.po.status != OntologyStatus::Active => bail_err!(
                InvalidRequest,
                "骨架三元组引用的关系类型 {} 已退役，不允许新引用",
                relation_key
            ),
            Some(_) => {}
        }

        for (key, field) in [
            (&relation.subject_class_key, "subject_class_key"),
            (&relation.object_class_key, "object_class_key"),
        ] {
            if normalize(key) == common::ontology::WILDCARD_CLASS {
                continue;
            }
            self.validate_class_ref(ctx, key, field).await?;
        }

        if relation.cardinality.parse::<Cardinality>().is_err() {
            bail_err!(
                InvalidRequest,
                "骨架三元组 cardinality 取值非法: {}（合法值: {}）",
                relation.cardinality,
                Cardinality::ACCEPTED_VALUES
            );
        }
        Ok(())
    }
}

/// 实体类 Entity → DTO（骨架视图组装用）
fn class_to_dto(class: OntologyClass) -> OntologyClassItem {
    let po = class.into_po();
    // 先解析再 move 字段，避免 partial move
    let required_fields = po.parse_required_fields();
    OntologyClassItem {
        id: po.id,
        term_key: po.term_key,
        display_name: po.display_name,
        description: po.description,
        required_fields,
        parent_term_key: po.parent_term_key,
        status: po.status,
        created_at: po.created_at,
        updated_at: po.updated_at,
    }
}

/// 关系类型 Entity → DTO（骨架视图组装用）
fn relation_type_to_dto(relation_type: OntologyRelationType) -> OntologyRelationTypeItem {
    let po = relation_type.into_po();
    OntologyRelationTypeItem {
        id: po.id,
        term_key: po.term_key,
        display_name: po.display_name,
        description: po.description,
        weight_base: po.weight_base,
        inverse_key: po.inverse_key,
        direction: po.direction,
        status: po.status,
        created_at: po.created_at,
        updated_at: po.updated_at,
    }
}

/// 骨架三元组 Entity → DTO（骨架视图组装用）
fn class_relation_to_dto(relation: OntologyClassRelation) -> OntologyClassRelationItem {
    let po = relation.into_po();
    // 先解析再 move 字段，避免 partial move
    let required = po.is_required();
    OntologyClassRelationItem {
        id: po.id,
        subject_class_key: po.subject_class_key,
        relation_key: po.relation_key,
        object_class_key: po.object_class_key,
        cardinality: po.cardinality,
        required,
        status: po.status,
        created_at: po.created_at,
        updated_at: po.updated_at,
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
        self.validate_class_parent(&ctx, &term_key, class.po.parent_term_key.as_deref())
            .await?;
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
        let Some(existing) = self
            .ontology_dal
            .find_class_by_id(ctx.clone(), &class.po.id)
            .await?
        else {
            return Err(err!(NotFound, "实体类 {} 不存在", class.po.id));
        };
        // 父级变更校验（term_key 不可变，用存量 term_key 做自指/环检测）
        self.validate_class_parent(
            &ctx,
            existing.term_key(),
            class.po.parent_term_key.as_deref(),
        )
        .await?;
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
        // term_key 不可变由 DAO UPDATE 语句保证；骨架约束已迁至三元组表，本表不再校验引用
        self.ontology_dal
            .update_relation_type(ctx, relation_type)
            .await
    }

    async fn retire_relation_type(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.ontology_dal.retire_relation_type(ctx, id).await
    }

    // ==================== B2. 本体骨架三元组 ====================

    async fn create_class_relation(
        &self,
        ctx: RequestContext,
        relation: &OntologyClassRelation,
    ) -> Result<()> {
        self.validate_class_relation_refs(&ctx, &relation.po)
            .await?;
        self.ontology_dal.insert_class_relation(ctx, relation).await
    }

    async fn get_class_relation(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<OntologyClassRelation>> {
        self.ontology_dal.find_class_relation_by_id(ctx, id).await
    }

    async fn list_class_relations(
        &self,
        ctx: RequestContext,
        query: OntologyClassRelationQuery,
    ) -> Result<common::api::PagedResult<OntologyClassRelation>> {
        self.ontology_dal.query_class_relations(ctx, query).await
    }

    async fn update_class_relation(
        &self,
        ctx: RequestContext,
        relation: &OntologyClassRelation,
    ) -> Result<()> {
        if self
            .ontology_dal
            .find_class_relation_by_id(ctx.clone(), &relation.po.id)
            .await?
            .is_none()
        {
            return Err(err!(NotFound, "骨架三元组 {} 不存在", relation.po.id));
        }
        self.validate_class_relation_refs(&ctx, &relation.po)
            .await?;
        self.ontology_dal.update_class_relation(ctx, relation).await
    }

    async fn retire_class_relation(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.ontology_dal.retire_class_relation(ctx, id).await
    }

    // ==================== B3. 本体骨架视图 ====================

    async fn get_skeleton(&self, ctx: RequestContext) -> Result<GetOntologySkeletonResponse> {
        // 骨架视图一次性返回三段全貌：词表量级为个位数十词，分页无意义
        let pagination = active_all_pagination();
        let classes = self
            .ontology_dal
            .query_classes(
                ctx.clone(),
                OntologyClassQuery {
                    status: Some(OntologyStatus::Active),
                    keyword: None,
                    pagination: pagination.clone(),
                },
            )
            .await?;
        warn_if_lexicon_truncated(&ctx, "实体类", classes.items.len());

        let relation_types = self
            .ontology_dal
            .query_relation_types(
                ctx.clone(),
                OntologyRelationTypeQuery {
                    status: Some(OntologyStatus::Active),
                    keyword: None,
                    pagination: pagination.clone(),
                },
            )
            .await?;
        warn_if_lexicon_truncated(&ctx, "关系类型", relation_types.items.len());

        let class_relations = self
            .ontology_dal
            .query_class_relations(
                ctx.clone(),
                OntologyClassRelationQuery {
                    status: Some(OntologyStatus::Active),
                    pagination,
                    ..Default::default()
                },
            )
            .await?;
        warn_if_lexicon_truncated(&ctx, "骨架三元组", class_relations.items.len());

        Ok(GetOntologySkeletonResponse {
            classes: classes.items.into_iter().map(class_to_dto).collect(),
            relation_types: relation_types
                .items
                .into_iter()
                .map(relation_type_to_dto)
                .collect(),
            class_relations: class_relations
                .items
                .into_iter()
                .map(class_relation_to_dto)
                .collect(),
        })
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

    // ==================== C4. 属性定义 CRUD ====================

    async fn create_property(
        &self,
        ctx: RequestContext,
        property: &OntologyProperty,
    ) -> Result<()> {
        // 校验基于规范形（与 DAO 落库值同口径）；归一由 DAO 单点完成，此处不覆写参数
        let term_key = normalize(&property.po.term_key);
        if term_key.is_empty() {
            bail_err!(InvalidRequest, "属性 term_key 不能为空");
        }
        if term_key.chars().count() > MAX_TERM_KEY_LEN {
            bail_err!(
                InvalidRequest,
                "属性 term_key 超长（上限 {} 字符）",
                MAX_TERM_KEY_LEN
            );
        }
        self.validate_class_refs(&ctx, &property.po.domain_classes, "domain_classes")
            .await?;
        validate_property_definition(&property.po)?;
        self.ontology_dal.insert_property(ctx, property).await
    }

    async fn get_property(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<OntologyProperty>> {
        self.ontology_dal.find_property_by_id(ctx, id).await
    }

    async fn list_properties(
        &self,
        ctx: RequestContext,
        query: OntologyPropertyQuery,
    ) -> Result<common::api::PagedResult<OntologyProperty>> {
        self.ontology_dal.query_properties(ctx, query).await
    }

    async fn update_property(
        &self,
        ctx: RequestContext,
        property: &OntologyProperty,
    ) -> Result<()> {
        self.validate_class_refs(&ctx, &property.po.domain_classes, "domain_classes")
            .await?;
        validate_property_definition(&property.po)?;
        // DAO update 按 id 行更新、不存在静默成功；NotFound 语义由 Domain 补齐
        if self
            .ontology_dal
            .find_property_by_id(ctx.clone(), &property.po.id)
            .await?
            .is_none()
        {
            return Err(err!(NotFound, "属性 {} 不存在", property.po.id));
        }
        self.ontology_dal.update_property(ctx, property).await
    }

    async fn retire_property(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.ontology_dal.retire_property(ctx, id).await
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
    ///   description / required_fields / parent_term_key / weight_base /
    ///   inverse_key / direction，骨架三元组仅覆写 cardinality / required），
    ///   term_key / id / status / created_at 永不触碰，
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
                    let mut po = OntologyClassPo::new(
                        key.clone(),
                        &p.display_name,
                        &p.description,
                        required_fields,
                    );
                    po.parent_term_key = p.parent_term_key.clone();
                    // 预置快照父级引用不存在 / 成环 = 快照残缺，fail fast 不静默
                    self.validate_class_parent(&ctx, &key, po.parent_term_key.as_deref())
                        .await?;
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
                        po.parent_term_key = p.parent_term_key.clone();
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
                    let po = OntologyRelationTypePo::new(
                        key.clone(),
                        &p.display_name,
                        &p.description,
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
                        // 保留 id / term_key / status / created_at 原值，仅覆写业务字段
                        let mut po = local.po.clone();
                        po.display_name = p.display_name.clone();
                        po.description = p.description.clone();
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

        // B2. 骨架三元组：依赖 classes 与 relation_types 已先行注入；
        //     (subject, relation, object) 天然唯一，存在时仅覆盖式更新边级约束
        for p in &preset.class_relations {
            let subject = normalize(&p.subject_class_key);
            let relation = normalize(&p.relation_key);
            let object = normalize(&p.object_class_key);
            let po = OntologyClassRelationPo::new(
                subject.clone(),
                relation.clone(),
                object.clone(),
                p.cardinality.clone(),
                p.required,
            );
            // 快照残缺 fail fast：三个 key 必须存在且 Active（`*` 通配放行）
            self.validate_class_relation_refs(&ctx, &po).await?;

            let existing = self
                .ontology_dal
                .query_class_relations(
                    ctx.clone(),
                    OntologyClassRelationQuery {
                        subject_class_key: Some(subject.clone()),
                        relation_key: Some(relation.clone()),
                        object_class_key: Some(object.clone()),
                        status: None,
                        pagination: common::api::PaginationParams::default(),
                    },
                )
                .await?
                .items
                .into_iter()
                .next();
            match existing {
                None => {
                    self.ontology_dal
                        .insert_class_relation(ctx.clone(), &OntologyClassRelation::from_po(po))
                        .await?;
                    report.inserted_class_relations += 1;
                }
                Some(local) => {
                    if strategy == PresetOntologySyncStrategy::Overwrite
                        && local.po.status == OntologyStatus::Active
                    {
                        // 三元组以三个 key 为身份，覆写只动边级约束（基数 / 必填）
                        let mut po = local.po.clone();
                        po.cardinality = p.cardinality.clone();
                        po.required = i32::from(p.required);
                        self.ontology_dal
                            .update_class_relation(ctx.clone(), &OntologyClassRelation::from_po(po))
                            .await?;
                        report.updated_class_relations += 1;
                    } else {
                        report.skipped += 1;
                    }
                }
            }
        }

        // C4. 属性定义：与关系类型同策略分流；
        //     注入顺序在 classes 之后（domain_classes 引用必须先存在）
        for p in &preset.properties {
            let key = normalize(&p.term_key);
            let domain_classes = serde_json::to_string(&p.domain_classes).unwrap_or_default();
            let applies_to = serde_json::to_string(&p.applies_to).unwrap_or_default();
            // Null（字段缺省）视为无约束，不落 "null" 字符串
            let value_range = match &p.value_range {
                serde_json::Value::Null => String::new(),
                v => v.to_string(),
            };
            let po = OntologyPropertyPo::new(
                key.clone(),
                &p.display_name,
                &p.description,
                domain_classes.clone(),
                applies_to.clone(),
                &p.data_type,
                value_range.clone(),
                &p.cardinality,
                p.unit.clone(),
                p.required,
            );
            // 快照残缺 fail fast：与关系类型引用校验同口径，不静默落脏定义
            self.validate_class_refs(&ctx, &domain_classes, "domain_classes")
                .await?;
            validate_property_definition(&po)?;

            let existing = self
                .ontology_dal
                .find_property_by_term_key(ctx.clone(), &key)
                .await?;
            match existing {
                None => {
                    self.ontology_dal
                        .insert_property(ctx.clone(), &OntologyProperty::from_po(po))
                        .await?;
                    report.inserted_properties.push(key);
                }
                Some(local) => {
                    if strategy == PresetOntologySyncStrategy::Overwrite
                        && local.po.status == OntologyStatus::Active
                    {
                        // 保留 id / term_key / status / created_at 原值，仅覆写业务字段
                        let mut po = local.po.clone();
                        po.display_name = p.display_name.clone();
                        po.description = p.description.clone();
                        po.domain_classes = domain_classes;
                        po.applies_to = applies_to;
                        po.data_type = p.data_type.clone();
                        po.value_range = value_range;
                        po.cardinality = p.cardinality.clone();
                        po.unit = p.unit.clone();
                        po.required = i32::from(p.required);
                        self.update_property(ctx.clone(), &OntologyProperty::from_po(po))
                            .await?;
                        report.updated_properties.push(key);
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
            if entity.po.parent_term_key != p.parent_term_key {
                diff_fields.push("parent_term_key".to_string());
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
            let mut diff_fields: Vec<String> = Vec::new();
            if entity.po.display_name != p.display_name {
                diff_fields.push("display_name".to_string());
            }
            if entity.po.description != p.description {
                diff_fields.push("description".to_string());
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
                + gaps.missing_synonyms
                + gaps.missing_class_relations,
            existing_count: gaps.skipped,
            overwrite_count,
            retired_count,
        })
    }

    async fn export_lexicon(&self, ctx: RequestContext) -> Result<PresetOntologyLexicon> {
        let (classes, relation_types, class_relations, properties, synonyms) = tokio::try_join!(
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
            self.ontology_dal.query_class_relations(
                ctx.clone(),
                OntologyClassRelationQuery {
                    status: Some(OntologyStatus::Active),
                    pagination: active_all_pagination(),
                    ..Default::default()
                },
            ),
            self.ontology_dal.query_properties(
                ctx.clone(),
                OntologyPropertyQuery {
                    status: Some(OntologyStatus::Active),
                    domain_class: None,
                    target_type: None,
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
        warn_if_lexicon_truncated(&ctx, "骨架三元组", class_relations.items.len());
        warn_if_lexicon_truncated(&ctx, "属性定义", properties.items.len());
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
                        parent_term_key: po.parent_term_key,
                    }
                })
                .collect(),
            relation_types: relation_types
                .items
                .into_iter()
                .map(|r| {
                    let po = r.po;
                    PresetOntologyRelationType {
                        term_key: po.term_key,
                        display_name: po.display_name,
                        description: po.description,
                        weight_base: Some(po.weight_base),
                        inverse_key: po.inverse_key,
                        direction: po.direction,
                    }
                })
                .collect(),
            class_relations: class_relations
                .items
                .into_iter()
                .map(|r| {
                    let po = r.po;
                    // 先解析再 move 字段，避免 partial move
                    let required = po.is_required();
                    PresetOntologyClassRelation {
                        subject_class_key: po.subject_class_key,
                        relation_key: po.relation_key,
                        object_class_key: po.object_class_key,
                        cardinality: po.cardinality,
                        required,
                    }
                })
                .collect(),
            properties: properties
                .items
                .into_iter()
                .map(|p| {
                    let po = p.po;
                    // 先借调解析器再 move 字段，避免 partial move
                    let domain_classes = po.parse_domain_classes();
                    let applies_to = parse_applies_to(&po.applies_to)
                        .into_iter()
                        .map(|t| t.as_str().to_string())
                        .collect();
                    let value_range = po.value_range();
                    let data_type = po.data_type();
                    let cardinality = po.cardinality();
                    let required = po.is_required();
                    PresetOntologyProperty {
                        term_key: po.term_key,
                        display_name: po.display_name,
                        description: po.description,
                        domain_classes,
                        applies_to,
                        data_type: data_type.as_str().to_string(),
                        // 无约束导出为空对象（写回快照时保持「已声明但无限定」的可读性）
                        value_range: serde_json::to_value(value_range).unwrap_or_default(),
                        cardinality: cardinality.as_str().to_string(),
                        unit: po.unit,
                        required,
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

        for p in &preset.properties {
            let key = normalize(&p.term_key);
            if self
                .ontology_dal
                .find_property_by_term_key(ctx.clone(), &key)
                .await?
                .is_some()
            {
                report.skipped += 1;
            } else {
                report.missing_properties.push(key);
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

        for p in &preset.class_relations {
            let subject = normalize(&p.subject_class_key);
            let relation = normalize(&p.relation_key);
            let object = normalize(&p.object_class_key);
            let existing = self
                .ontology_dal
                .query_class_relations(
                    ctx.clone(),
                    OntologyClassRelationQuery {
                        subject_class_key: Some(subject),
                        relation_key: Some(relation),
                        object_class_key: Some(object),
                        status: None,
                        pagination: common::api::PaginationParams::default(),
                    },
                )
                .await?;
            if existing.items.is_empty() {
                report.missing_class_relations += 1;
            } else {
                report.skipped += 1;
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

    async fn get_property_effectiveness(
        &self,
        ctx: RequestContext,
        request: GetPropertyEffectivenessRequest,
    ) -> Result<GetPropertyEffectivenessResponse> {
        // 薄透传：聚合与判定全在 DAL（含纯函数），Domain 不做二次加工
        self.ontology_dal
            .get_property_effectiveness(ctx, request)
            .await
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
