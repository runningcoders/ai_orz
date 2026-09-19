//! 预置本体词表（seed）同步到本地词表
//!
//! 解决的问题：seed 内置的预置本体词表只在系统初始化 / 快照导入时注入一次，
//! 后续 seed 升级（新增节点类 / 关系词 / 同义映射）不会流入已运行的实例。
//! 这里提供两个接口把 seed 词表重新灌入本地库。
//!
//! - `GET  /api/v1/system/seed/preset-ontology/preview` — 只读对比，返回缺口清单
//! - `POST /api/v1/system/seed/preset-ontology/sync` — 仅补缺注入，同步返回结果
//!
//! 与预置技能同步的有意差异（design 决策 #15）：词表量级几十条、注入毫秒级，
//! 不走后台任务轮询，直接同步返回。策略固定「仅补缺」（决策 #14）：term_key
//! 物理存在（含退役行）即跳过，不覆盖管理页本地修改——管理页是词表唯一修改入口。
//!
//! 路由挂在 `/system` 段下，已由 `require_role_middleware(Admin)` 统一鉴权。

use crate::pkg::RequestContext;
use crate::service::domain::hr;
use crate::service::domain::system::seed::default;
use ai_orz_macros::generate_http_handler;
use common::api::ontology::{
    PresetOntologySyncItem, PreviewPresetOntologyRequest, PreviewPresetOntologyResponse,
    SyncPresetOntologyRequest, SyncPresetOntologyResponse,
};
use common::error::Result;
use common::ontology::{TermKind, normalize};

/// 预置本体词表同步预览（只读，不写库）
///
/// 返回 seed 词表相对本地词表的缺口清单：实体类 / 关系类型逐条列出
/// （exists 标记本地是否已存在），同义映射以计数呈现
/// （raw→target 映射无显示名，不适合逐条展示）。
#[generate_http_handler]
pub async fn preview_preset_ontology(
    ctx: RequestContext,
    _params: PreviewPresetOntologyRequest,
) -> Result<PreviewPresetOntologyResponse> {
    let snapshot = default::embedded_default_snapshot();
    let gaps = hr::domain()
        .ontology_domain()
        .preview_lexicon_gaps(ctx, &snapshot.ontology)
        .await?;

    // exists 判定直接复用缺口清单（归一化 term_key），与注入跳过逻辑同源
    let items: Vec<PresetOntologySyncItem> = snapshot
        .ontology
        .classes
        .iter()
        .map(|p| PresetOntologySyncItem {
            kind: TermKind::Class,
            term_key: p.term_key.clone(),
            display_name: p.display_name.clone(),
            description: p.description.clone(),
            exists: !gaps.missing_classes.contains(&normalize(&p.term_key)),
        })
        .chain(snapshot.ontology.relation_types.iter().map(|p| {
            PresetOntologySyncItem {
                kind: TermKind::Relation,
                term_key: p.term_key.clone(),
                display_name: p.display_name.clone(),
                description: p.description.clone(),
                exists: !gaps
                    .missing_relation_types
                    .contains(&normalize(&p.term_key)),
            }
        }))
        .collect();

    Ok(PreviewPresetOntologyResponse {
        missing_count: gaps.missing_classes.len()
            + gaps.missing_relation_types.len()
            + gaps.missing_synonyms,
        existing_count: gaps.skipped,
        items,
    })
}

/// 同步预置本体词表到本地词表（仅补缺，同步返回）
///
/// 逐条按 seed 词表注入：归一化 term_key 物理存在（含退役行）即跳过，
/// 只补缺失条目。幂等——对已同步过的实例再次调用，全部条目计入 skipped。
/// 同义映射目标不存在时 fail fast（快照残缺不静默），详见
/// [`crate::service::domain::hr::OntologyDomain::apply_default_lexicon`]。
#[generate_http_handler]
pub async fn sync_preset_ontology(
    ctx: RequestContext,
    _params: SyncPresetOntologyRequest,
) -> Result<SyncPresetOntologyResponse> {
    let snapshot = default::embedded_default_snapshot();
    let total = snapshot.ontology.classes.len()
        + snapshot.ontology.relation_types.len()
        + snapshot.ontology.synonym_mappings.len();
    let report = hr::domain()
        .ontology_domain()
        .apply_default_lexicon(ctx.clone(), &snapshot.ontology)
        .await?;
    log_info!(
        &ctx,
        "sync_preset_ontology",
        inserted_classes = report.inserted_classes.len(),
        inserted_relation_types = report.inserted_relation_types.len(),
        inserted_synonyms = report.inserted_synonyms,
        skipped = report.skipped,
        "预置本体词表同步完成"
    );
    Ok(SyncPresetOntologyResponse {
        created: report.inserted_classes.len()
            + report.inserted_relation_types.len()
            + report.inserted_synonyms,
        skipped: report.skipped,
        total,
    })
}
