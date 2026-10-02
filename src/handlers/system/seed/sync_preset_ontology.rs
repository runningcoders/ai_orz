//! 预置本体词表（seed）同步到本地词表
//!
//! 解决的问题：seed 内置的预置本体词表只在系统初始化 / 快照导入时注入一次，
//! 后续 seed 升级（新增节点类 / 关系词 / 同义映射）不会流入已运行的实例。
//! 这里提供两个接口把 seed 词表重新灌入本地库。
//!
//! - `GET  /api/v1/system/seed/preset-ontology/preview` — 只读对比，逐条返回
//!   影响面（exists / local_* / diff_fields / retired / direction 对比 + 四计数）
//! - `POST /api/v1/system/seed/preset-ontology/sync` — 按策略注入，同步返回结果
//!
//! 与预置技能同步的有意差异（design 决策 #15）：词表量级几十条、注入毫秒级，
//! 不走后台任务轮询，直接同步返回。同步策略用户自选（决策 #14 修订）：缺省
//! 仅补缺——term_key 物理存在（含退役行）即跳过，不覆盖管理页本地修改（与历史
//! 行为一致）；显式传 `Overwrite` 时对已存在非退役条目按 seed 幂等覆写
//! （term_key / id / status / created_at 永不触碰，退役行整体跳过）。
//!
//! 路由挂在 `/system` 段下，已由 `require_role_middleware(Admin)` 统一鉴权。

use crate::pkg::RequestContext;
use crate::service::domain::hr;
use crate::service::domain::system::seed::default;
use ai_orz_macros::generate_http_handler;
use common::api::ontology::{
    PreviewPresetOntologyRequest, PreviewPresetOntologyResponse, SyncPresetOntologyRequest,
    SyncPresetOntologyResponse,
};
use common::error::Result;

/// 预置本体词表同步预览（只读，不写库）
///
/// 委托 domain 编排方法
/// [`crate::service::domain::hr::OntologyDomain::build_preset_sync_preview`]：
/// seed 词表相对本地词表逐条对比（实体类 / 关系类型逐条列出本地现值与差异字段，
/// direction 对比仅关系类型条目携带），同义映射以计数呈现（raw→target 映射
/// 无显示名，不适合逐条展示）。
#[generate_http_handler]
pub async fn preview_preset_ontology(
    ctx: RequestContext,
    _params: PreviewPresetOntologyRequest,
) -> Result<PreviewPresetOntologyResponse> {
    let snapshot = default::embedded_default_snapshot();
    hr::domain()
        .ontology_domain()
        .build_preset_sync_preview(ctx, &snapshot.ontology)
        .await
}

/// 同步预置本体词表到本地词表（按策略，同步返回）
///
/// 策略取自请求体 `strategy`（缺省=仅补缺，与历史行为一致）；`Overwrite` 对已存在
/// 非退役条目按 seed 幂等覆写。幂等——对已同步过的实例再次调用（仅补缺）全部条目
/// 计入 skipped。同义映射目标不存在时 fail fast（快照残缺不静默），详见
/// [`crate::service::domain::hr::OntologyDomain::apply_default_lexicon_with_strategy`]。
#[generate_http_handler]
pub async fn sync_preset_ontology(
    ctx: RequestContext,
    params: SyncPresetOntologyRequest,
) -> Result<SyncPresetOntologyResponse> {
    let snapshot = default::embedded_default_snapshot();
    let total = snapshot.ontology.classes.len()
        + snapshot.ontology.relation_types.len()
        + snapshot.ontology.synonym_mappings.len();
    let report = hr::domain()
        .ontology_domain()
        .apply_default_lexicon_with_strategy(ctx.clone(), &snapshot.ontology, params.strategy)
        .await?;
    log_info!(
        &ctx,
        "sync_preset_ontology",
        strategy = ?params.strategy,
        inserted_classes = report.inserted_classes.len(),
        inserted_relation_types = report.inserted_relation_types.len(),
        inserted_synonyms = report.inserted_synonyms,
        updated_classes = report.updated_classes.len(),
        updated_relation_types = report.updated_relation_types.len(),
        updated_synonyms = report.updated_synonyms,
        skipped = report.skipped,
        "预置本体词表同步完成"
    );
    Ok(SyncPresetOntologyResponse {
        created: report.inserted_classes.len()
            + report.inserted_relation_types.len()
            + report.inserted_synonyms,
        updated: report.updated_classes.len()
            + report.updated_relation_types.len()
            + report.updated_synonyms,
        skipped: report.skipped,
        total,
    })
}
