//! 本体词表管理页（/hr/ontology-lexicon）
//!
//! 五大区块 + 三个辅助 Modal：
//! 1. **词表注入视图**：与神经技能注入共用同一契约的只读快照（关系词 / 实体类 / 同义样例）
//! 2. **漂移看板**：词表覆盖率 / 漂移关系数 / 漂移节点占比 + Top 漂移词（可下钻明细）
//! 3. **实体类词表**：分页列表 + 状态/关键词筛选 + 新增 / 编辑 / 退役（软删除）
//! 4. **关系类型词表**：同上，附域/值域约束、边权重基数、反向关系
//! 5. **同义映射**：分页列表 + 目标种类筛选 + 新增 / 删除（物理删除）
//!
//! 同步 Modal：seed 预置词表 preview → sync（仅补缺策略，不覆盖本地修改）。
//! 下钻 Modal：漂移词的关系（边）明细 + 节点明细。

use common::api::{
    CreateOntologyClassRequest, CreateOntologyRelationTypeRequest, CreateOntologySynonymRequest,
    DriftClassDetail, DriftRelationDetail, GetDriftDashboardRequest, GetDriftDashboardResponse,
    ListDriftClassDetailsRequest, ListDriftRelationDetailsRequest, ListOntologyClassesRequest,
    ListOntologyLexiconResponse, ListOntologyRelationTypesRequest, ListOntologySynonymsRequest,
    OntologyClassItem, OntologyRelationTypeItem, OntologySynonymItem, PaginationParams,
    PreviewPresetOntologyResponse, SyncPresetOntologyRequest, SyncPresetOntologyResponse,
    UpdateOntologyClassRequest, UpdateOntologyRelationTypeRequest,
};
use common::enums::OntologyStatus;
use common::ontology::TermKind;
use dioxus::prelude::*;

use crate::api::hr::{
    create_ontology_class, create_ontology_relation_type, create_ontology_synonym,
    delete_ontology_synonym, get_ontology_drift_dashboard, list_ontology_classes,
    list_ontology_drift_class_details, list_ontology_drift_relation_details, list_ontology_lexicon,
    list_ontology_relation_types, list_ontology_synonyms, retire_ontology_class,
    retire_ontology_relation_type, update_ontology_class, update_ontology_relation_type,
};
use crate::api::seed::{preview_preset_ontology, sync_preset_ontology};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::hud::{HudPanel, HudSection, PageHeader, StatGrid, StatReadout};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use crate::utils::number::{format_compact_count, format_relevance};
use crate::utils::time::format_datetime;

/// 词表列表分页大小
const PAGE_SIZE: usize = 20;
/// 漂移下钻单侧明细条数上限
const DRILL_DETAIL_LIMIT: usize = 20;
/// 看板 Top 漂移词数量
const DASHBOARD_TOP_N: usize = 10;

/// 状态筛选下拉值 → OntologyStatus
fn parse_status_filter(v: &str) -> Option<OntologyStatus> {
    match v {
        "Active" => Some(OntologyStatus::Active),
        "Retired" => Some(OntologyStatus::Retired),
        _ => None,
    }
}

/// 种类筛选下拉值 → TermKind
fn parse_kind_filter(v: &str) -> Option<TermKind> {
    match v {
        "class" => Some(TermKind::Class),
        "relation" => Some(TermKind::Relation),
        _ => None,
    }
}

/// 同义映射表单的种类下拉值 → TermKind（默认关系）
fn parse_term_kind(v: &str) -> TermKind {
    if v == "class" {
        TermKind::Class
    } else {
        TermKind::Relation
    }
}

/// trim 后非空才保留
fn non_empty(v: &str) -> Option<String> {
    let t = v.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

/// 逗号分隔清单 → 去空白去空项
fn split_list(v: &str) -> Vec<String> {
    v.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn status_badge(status: OntologyStatus) -> Element {
    match status {
        OntologyStatus::Active => {
            rsx! { span { class: "badge hud-badge badge-success", "启用中" } }
        }
        OntologyStatus::Retired => {
            rsx! { span { class: "badge hud-badge badge-warning", "已退役" } }
        }
    }
}

fn kind_badge(kind: TermKind) -> Element {
    match kind {
        TermKind::Class => rsx! { span { class: "badge orz-tag badge-sm", "实体" } },
        TermKind::Relation => rsx! { span { class: "badge orz-tag badge-sm", "关系" } },
    }
}

#[component]
pub fn HrOntologyLexicon() -> Element {
    let toast = use_toast();

    // ===== 实体类列表 =====
    let mut classes = use_signal(Vec::<OntologyClassItem>::new);
    let mut classes_total = use_signal(|| 0usize);
    let mut classes_loading = use_signal(|| false);
    let mut class_seq = use_signal(|| 0u64);
    let mut class_status_filter = use_signal(String::new);
    let mut class_keyword = use_signal(String::new);
    let mut class_offset = use_signal(|| 0usize);

    // ===== 关系类型列表 =====
    let mut relations = use_signal(Vec::<OntologyRelationTypeItem>::new);
    let mut relations_total = use_signal(|| 0usize);
    let mut relations_loading = use_signal(|| false);
    let mut relation_seq = use_signal(|| 0u64);
    let mut relation_status_filter = use_signal(String::new);
    let mut relation_keyword = use_signal(String::new);
    let mut relation_offset = use_signal(|| 0usize);

    // ===== 同义映射列表 =====
    let mut synonyms = use_signal(Vec::<OntologySynonymItem>::new);
    let mut synonyms_total = use_signal(|| 0usize);
    let mut synonyms_loading = use_signal(|| false);
    let mut synonym_seq = use_signal(|| 0u64);
    let mut synonym_kind_filter = use_signal(String::new);
    let mut synonym_offset = use_signal(|| 0usize);

    // ===== 漂移看板 + 下钻 =====
    let mut dashboard = use_signal(|| None::<GetDriftDashboardResponse>);
    let mut dashboard_loading = use_signal(|| false);
    let mut drill_term = use_signal(|| None::<String>);
    let mut drill_relations = use_signal(Vec::<DriftRelationDetail>::new);
    let mut drill_classes = use_signal(Vec::<DriftClassDetail>::new);
    let mut drill_loading = use_signal(|| false);
    let mut drill_seq = use_signal(|| 0u64);

    // ===== 实体类表单 Modal =====
    let mut show_class_modal = use_signal(|| false);
    let mut class_edit_id = use_signal(|| None::<String>);
    let mut class_form_key = use_signal(String::new);
    let mut class_form_name = use_signal(String::new);
    let mut class_form_desc = use_signal(String::new);
    let mut class_form_fields = use_signal(String::new);
    let mut class_saving = use_signal(|| false);

    // ===== 关系类型表单 Modal =====
    let mut show_relation_modal = use_signal(|| false);
    let mut relation_edit_id = use_signal(|| None::<String>);
    let mut relation_form_key = use_signal(String::new);
    let mut relation_form_name = use_signal(String::new);
    let mut relation_form_desc = use_signal(String::new);
    let mut relation_form_domain = use_signal(String::new);
    let mut relation_form_range = use_signal(String::new);
    let mut relation_form_weight = use_signal(String::new);
    let mut relation_form_inverse = use_signal(String::new);
    let mut relation_saving = use_signal(|| false);

    // ===== 同义映射表单 Modal =====
    let mut show_synonym_modal = use_signal(|| false);
    let mut synonym_form_raw = use_signal(String::new);
    let mut synonym_form_kind = use_signal(|| "relation".to_string());
    let mut synonym_form_target_key = use_signal(String::new);
    let mut synonym_saving = use_signal(|| false);

    // ===== 确认框目标 =====
    let mut class_retire_id = use_signal(|| None::<String>);
    let mut relation_retire_id = use_signal(|| None::<String>);
    let mut synonym_delete_id = use_signal(|| None::<String>);

    // ===== 预置词表同步 Modal =====
    let mut show_sync_modal = use_signal(|| false);
    let mut sync_preview = use_signal(|| None::<PreviewPresetOntologyResponse>);
    let mut sync_preview_loading = use_signal(|| false);
    let mut syncing = use_signal(|| false);
    let mut sync_result = use_signal(|| None::<SyncPresetOntologyResponse>);

    // ===== 词表注入视图 =====
    let mut lexicon_summary = use_signal(|| None::<ListOntologyLexiconResponse>);
    let mut lexicon_loading = use_signal(|| false);

    // ===== 数据加载闭包 =====

    let request_classes = move |offset: usize| {
        list_ontology_classes(ListOntologyClassesRequest {
            status: parse_status_filter(&class_status_filter()),
            keyword: non_empty(&class_keyword()),
            pagination: PaginationParams {
                limit: Some(PAGE_SIZE),
                offset: Some(offset),
            },
        })
    };

    let mut fetch_classes = move |reset_offset: bool| {
        if reset_offset {
            class_offset.set(0);
        }
        class_seq.set(class_seq() + 1);
        let seq = class_seq();
        classes_loading.set(true);
        spawn(async move {
            let offset = class_offset();
            let res = request_classes(offset).await;
            if class_seq() == seq {
                match res {
                    Ok(page) => {
                        classes_total.set(page.total);
                        // 末页条目因写入（退役/删除）缩减后旧 offset 可能越界：
                        // 钳回最后一个非空页并重拉一次，避免落地误导性的空列表
                        if page.items.is_empty() && page.total > 0 && offset >= page.total {
                            class_offset.set((page.total - 1) / PAGE_SIZE * PAGE_SIZE);
                            if let Ok(page2) = request_classes(class_offset()).await
                                && class_seq() == seq
                            {
                                classes.set(page2.items);
                            }
                        } else {
                            classes.set(page.items);
                        }
                    }
                    Err(e) => toast.error(format!("加载实体类失败：{e}")),
                }
                classes_loading.set(false);
            }
        });
    };

    let request_relations = move |offset: usize| {
        list_ontology_relation_types(ListOntologyRelationTypesRequest {
            status: parse_status_filter(&relation_status_filter()),
            keyword: non_empty(&relation_keyword()),
            pagination: PaginationParams {
                limit: Some(PAGE_SIZE),
                offset: Some(offset),
            },
        })
    };

    let mut fetch_relations = move |reset_offset: bool| {
        if reset_offset {
            relation_offset.set(0);
        }
        relation_seq.set(relation_seq() + 1);
        let seq = relation_seq();
        relations_loading.set(true);
        spawn(async move {
            let offset = relation_offset();
            let res = request_relations(offset).await;
            if relation_seq() == seq {
                match res {
                    Ok(page) => {
                        relations_total.set(page.total);
                        // 同 fetch_classes：写入缩减末页后钳回最后一个非空页重拉
                        if page.items.is_empty() && page.total > 0 && offset >= page.total {
                            relation_offset.set((page.total - 1) / PAGE_SIZE * PAGE_SIZE);
                            if let Ok(page2) = request_relations(relation_offset()).await
                                && relation_seq() == seq
                            {
                                relations.set(page2.items);
                            }
                        } else {
                            relations.set(page.items);
                        }
                    }
                    Err(e) => toast.error(format!("加载关系类型失败：{e}")),
                }
                relations_loading.set(false);
            }
        });
    };

    let request_synonyms = move |offset: usize| {
        list_ontology_synonyms(ListOntologySynonymsRequest {
            target_kind: parse_kind_filter(&synonym_kind_filter()),
            target_key: None,
            pagination: PaginationParams {
                limit: Some(PAGE_SIZE),
                offset: Some(offset),
            },
        })
    };

    let mut fetch_synonyms = move |reset_offset: bool| {
        if reset_offset {
            synonym_offset.set(0);
        }
        synonym_seq.set(synonym_seq() + 1);
        let seq = synonym_seq();
        synonyms_loading.set(true);
        spawn(async move {
            let offset = synonym_offset();
            let res = request_synonyms(offset).await;
            if synonym_seq() == seq {
                match res {
                    Ok(page) => {
                        synonyms_total.set(page.total);
                        // 同 fetch_classes：同义映射物理删除缩减末页后钳回最后一个非空页重拉
                        if page.items.is_empty() && page.total > 0 && offset >= page.total {
                            synonym_offset.set((page.total - 1) / PAGE_SIZE * PAGE_SIZE);
                            if let Ok(page2) = request_synonyms(synonym_offset()).await
                                && synonym_seq() == seq
                            {
                                synonyms.set(page2.items);
                            }
                        } else {
                            synonyms.set(page.items);
                        }
                    }
                    Err(e) => toast.error(format!("加载同义映射失败：{e}")),
                }
                synonyms_loading.set(false);
            }
        });
    };

    let mut fetch_dashboard = move || {
        dashboard_loading.set(true);
        spawn(async move {
            match get_ontology_drift_dashboard(GetDriftDashboardRequest {
                top_n: Some(DASHBOARD_TOP_N),
                agent_id: None,
            })
            .await
            {
                Ok(d) => dashboard.set(Some(d)),
                Err(e) => {
                    // 清掉旧数据：失败后展示失败占位，避免残留过期看板误导
                    dashboard.set(None);
                    toast.error(format!("加载漂移看板失败：{e}"));
                }
            }
            dashboard_loading.set(false);
        });
    };

    let fetch_lexicon = move || {
        spawn(async move {
            lexicon_loading.set(true);
            match list_ontology_lexicon().await {
                Ok(s) => lexicon_summary.set(Some(s)),
                Err(e) => {
                    // 清掉旧数据：失败后展示失败占位，避免残留过期词表误导
                    lexicon_summary.set(None);
                    toast.error(format!("加载词表注入视图失败：{e}"));
                }
            }
            lexicon_loading.set(false);
        });
    };

    let mut open_drill = move |term: String| {
        drill_term.set(Some(term.clone()));
        drill_relations.set(Vec::new());
        drill_classes.set(Vec::new());
        drill_seq.set(drill_seq() + 1);
        let seq = drill_seq();
        drill_loading.set(true);
        spawn(async move {
            let rel = list_ontology_drift_relation_details(ListDriftRelationDetailsRequest {
                raw_term: term.clone(),
                agent_id: None,
                pagination: PaginationParams {
                    limit: Some(DRILL_DETAIL_LIMIT),
                    offset: Some(0),
                },
            })
            .await;
            let cls = list_ontology_drift_class_details(ListDriftClassDetailsRequest {
                raw_term: term.clone(),
                agent_id: None,
                pagination: PaginationParams {
                    limit: Some(DRILL_DETAIL_LIMIT),
                    offset: Some(0),
                },
            })
            .await;
            // seq 守卫：快速切换下钻词时只让最新一次请求落地，防止旧响应覆盖新视图
            if drill_seq() == seq {
                match rel {
                    Ok(page) => drill_relations.set(page.items),
                    Err(e) => toast.error(format!("加载关系明细失败：{e}")),
                }
                match cls {
                    Ok(page) => drill_classes.set(page.items),
                    Err(e) => toast.error(format!("加载节点明细失败：{e}")),
                }
                drill_loading.set(false);
            }
        });
    };

    // ===== 预置词表同步 =====

    let mut open_sync_modal = move || {
        show_sync_modal.set(true);
        sync_result.set(None);
        sync_preview.set(None);
        sync_preview_loading.set(true);
        spawn(async move {
            match preview_preset_ontology().await {
                Ok(p) => sync_preview.set(Some(p)),
                Err(e) => toast.error(format!("加载预置词表预览失败：{e}")),
            }
            sync_preview_loading.set(false);
        });
    };

    let mut handle_sync = move || {
        syncing.set(true);
        spawn(async move {
            match sync_preset_ontology(SyncPresetOntologyRequest {}).await {
                Ok(r) => {
                    let msg = format!("同步完成：新建 {} 条，跳过 {} 条", r.created, r.skipped);
                    sync_result.set(Some(r));
                    toast.success(msg);
                    fetch_classes(true);
                    fetch_relations(true);
                    fetch_synonyms(true);
                    fetch_dashboard();
                    fetch_lexicon();
                    // 同步成功后刷新 preview：失败要留痕——modal 会继续显示
                    // 同步前的旧预览，静默吞掉会让用户误读为最新状态
                    match preview_preset_ontology().await {
                        Ok(p) => sync_preview.set(Some(p)),
                        Err(e) => toast.warning(format!("同步成功，但刷新预览失败：{e}")),
                    }
                }
                Err(e) => toast.error(format!("同步失败：{e}")),
            }
            syncing.set(false);
        });
    };

    // ===== 实体类写入 =====

    let mut handle_save_class = move || {
        let display_name = class_form_name().trim().to_string();
        if display_name.is_empty() {
            toast.warning("展示名不能为空");
            return;
        }
        let edit_id = class_edit_id();
        let is_edit = edit_id.is_some();
        if edit_id.is_none() && class_form_key().trim().is_empty() {
            toast.warning("规范词 key 不能为空（小写 snake_case）");
            return;
        }
        let required_fields = split_list(&class_form_fields());
        class_saving.set(true);
        spawn(async move {
            let res = match edit_id {
                Some(id) => {
                    update_ontology_class(UpdateOntologyClassRequest {
                        id,
                        display_name,
                        description: class_form_desc(),
                        required_fields,
                    })
                    .await
                }
                None => {
                    create_ontology_class(CreateOntologyClassRequest {
                        term_key: class_form_key().trim().to_string(),
                        display_name,
                        description: class_form_desc(),
                        required_fields,
                    })
                    .await
                }
            };
            class_saving.set(false);
            match res {
                Ok(_) => {
                    toast.success(if is_edit {
                        "实体类已更新"
                    } else {
                        "实体类已创建"
                    });
                    show_class_modal.set(false);
                    fetch_classes(false);
                    fetch_dashboard();
                    fetch_lexicon();
                }
                Err(e) => toast.error(format!("保存失败：{e}")),
            }
        });
    };

    let mut handle_retire_class = move || {
        if let Some(id) = class_retire_id() {
            class_retire_id.set(None);
            spawn(async move {
                match retire_ontology_class(&id).await {
                    Ok(_) => {
                        toast.success("实体类已退役");
                        fetch_classes(false);
                        fetch_dashboard();
                        fetch_lexicon();
                    }
                    Err(e) => toast.error(format!("退役失败：{e}")),
                }
            });
        }
    };

    // ===== 关系类型写入 =====

    let mut handle_save_relation = move || {
        let display_name = relation_form_name().trim().to_string();
        if display_name.is_empty() {
            toast.warning("展示名不能为空");
            return;
        }
        let edit_id = relation_edit_id();
        let is_edit = edit_id.is_some();
        if edit_id.is_none() && relation_form_key().trim().is_empty() {
            toast.warning("规范词 key 不能为空（小写 snake_case）");
            return;
        }
        let domain_classes = split_list(&relation_form_domain());
        let range_classes = split_list(&relation_form_range());
        let inverse_key = non_empty(&relation_form_inverse());
        let weight_text = relation_form_weight().trim().to_string();
        // 编辑分支权重必填且须可解析：非法值显式拒绝而非静默回退 1.0
        // （创建分支留空传 None 走后端默认，语义不同）
        if is_edit && weight_text.parse::<f64>().is_err() {
            toast.warning("权重必须是合法数字（如 1.0）");
            return;
        }
        relation_saving.set(true);
        spawn(async move {
            let res = match edit_id {
                Some(id) => {
                    let weight_base = weight_text.parse::<f64>().unwrap_or(1.0);
                    update_ontology_relation_type(UpdateOntologyRelationTypeRequest {
                        id,
                        display_name,
                        description: relation_form_desc(),
                        domain_classes,
                        range_classes,
                        weight_base,
                        inverse_key,
                    })
                    .await
                }
                None => {
                    let weight_base = weight_text.parse::<f64>().ok();
                    create_ontology_relation_type(CreateOntologyRelationTypeRequest {
                        term_key: relation_form_key().trim().to_string(),
                        display_name,
                        description: relation_form_desc(),
                        domain_classes,
                        range_classes,
                        weight_base,
                        inverse_key,
                    })
                    .await
                }
            };
            relation_saving.set(false);
            match res {
                Ok(_) => {
                    toast.success(if is_edit {
                        "关系类型已更新"
                    } else {
                        "关系类型已创建"
                    });
                    show_relation_modal.set(false);
                    fetch_relations(false);
                    fetch_dashboard();
                    fetch_lexicon();
                }
                Err(e) => toast.error(format!("保存失败：{e}")),
            }
        });
    };

    let mut handle_retire_relation = move || {
        if let Some(id) = relation_retire_id() {
            relation_retire_id.set(None);
            spawn(async move {
                match retire_ontology_relation_type(&id).await {
                    Ok(_) => {
                        toast.success("关系类型已退役");
                        fetch_relations(false);
                        fetch_dashboard();
                        fetch_lexicon();
                    }
                    Err(e) => toast.error(format!("退役失败：{e}")),
                }
            });
        }
    };

    // ===== 同义映射写入 =====

    let mut handle_save_synonym = move || {
        let raw_term = synonym_form_raw().trim().to_string();
        if raw_term.is_empty() {
            toast.warning("漂移原文不能为空");
            return;
        }
        let target_key = synonym_form_target_key().trim().to_string();
        if target_key.is_empty() {
            toast.warning("目标规范词不能为空");
            return;
        }
        let target_kind = parse_term_kind(&synonym_form_kind());
        synonym_saving.set(true);
        spawn(async move {
            let res = create_ontology_synonym(CreateOntologySynonymRequest {
                raw_term,
                target_kind,
                target_key,
            })
            .await;
            synonym_saving.set(false);
            match res {
                Ok(_) => {
                    toast.success("同义映射已创建");
                    show_synonym_modal.set(false);
                    synonym_form_raw.set(String::new());
                    synonym_form_target_key.set(String::new());
                    fetch_synonyms(false);
                    fetch_dashboard();
                    fetch_lexicon();
                }
                Err(e) => toast.error(format!("创建失败：{e}")),
            }
        });
    };

    let mut handle_delete_synonym = move || {
        if let Some(id) = synonym_delete_id() {
            synonym_delete_id.set(None);
            spawn(async move {
                match delete_ontology_synonym(&id).await {
                    Ok(_) => {
                        toast.success("同义映射已删除");
                        fetch_synonyms(false);
                        fetch_dashboard();
                        fetch_lexicon();
                    }
                    Err(e) => toast.error(format!("删除失败：{e}")),
                }
            });
        }
    };

    // ===== 初始加载（闭包同步段不读信号，effect 仅挂载执行一次） =====

    use_effect(move || {
        fetch_classes(true);
        fetch_relations(true);
        fetch_synonyms(true);
        fetch_dashboard();
        fetch_lexicon();
    });

    rsx! {
        AppLayout {
            HudPanel { signal: Some(true),
                div { class: "card-body",
                    PageHeader {
                        eyebrow: Some("HR".to_string()),
                        title: "本体词表管理".to_string(),
                        actions: Some(rsx! {
                            button { class: "btn hud-btn btn-primary",
                                onclick: move |_| open_sync_modal(),
                                "⟳ 同步预置词表"
                            }
                        }),
                    }

                    // ==================== 词表注入视图 ====================
                    HudSection {
                        eyebrow: Some("LEXICON".to_string()),
                        title: "词表注入视图".to_string(),
                        actions: Some(rsx! {
                            button { class: "btn hud-btn btn-ghost btn-sm",
                                onclick: move |_| fetch_lexicon(),
                                "刷新"
                            }
                        }),
                        if lexicon_loading() {
                            Loading { size: "md" }
                        } else if let Some(s) = lexicon_summary() {
                            if s.is_empty() {
                                EmptyState {
                                    icon: Some("📖".to_string()),
                                    message: "词表为空，Agent 技能暂不注入本体词表，可点击右上角同步预置词表"
                                        .to_string(),
                                }
                            } else {
                                div { class: "space-y-4",
                                    div { class: "flex flex-wrap items-center gap-3 text-sm",
                                        span { class: "badge hud-badge badge-primary", "关系词 {s.relation_types.len()}" }
                                        span { class: "badge hud-badge badge-secondary", "实体类 {s.classes.len()}" }
                                        span { class: "badge hud-badge badge-ghost", "同义样例 {s.synonyms.len()}" }
                                        span { class: "opacity-70", "与神经技能注入共用同一契约，Token 超限时按 关系词 > 实体类 > 同义 顺序裁剪" }
                                    }
                                    div { class: "space-y-1",
                                        div { class: "text-xs font-semibold uppercase tracking-wider opacity-60", "关系类型（注入优先级最高）" }
                                        div { class: "flex flex-wrap gap-2",
                                            for t in s.relation_types {
                                                span { class: "badge hud-badge badge-outline", "{t.term_key} · {t.display_name}" }
                                            }
                                        }
                                    }
                                    div { class: "space-y-1",
                                        div { class: "text-xs font-semibold uppercase tracking-wider opacity-60", "实体类" }
                                        div { class: "flex flex-wrap gap-2",
                                            for t in s.classes {
                                                span { class: "badge hud-badge badge-outline", "{t.term_key} · {t.display_name}" }
                                            }
                                        }
                                    }
                                    div { class: "space-y-1",
                                        div { class: "text-xs font-semibold uppercase tracking-wider opacity-60", "同义映射样例" }
                                        div { class: "flex flex-wrap gap-2",
                                            for x in s.synonyms {
                                                span { class: "badge hud-badge badge-ghost font-mono", "{x.raw_term} → {x.target_key}" }
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            EmptyState {
                                icon: Some("⚠️".to_string()),
                                message: "词表注入视图加载失败，请点击右上角刷新重试".to_string(),
                            }
                        }
                    }

                    // ==================== 漂移看板 ====================
                    HudSection {
                        eyebrow: Some("DRIFT".to_string()),
                        title: "漂移看板".to_string(),
                        actions: Some(rsx! {
                            button { class: "btn hud-btn btn-ghost btn-sm",
                                onclick: move |_| fetch_dashboard(),
                                "刷新"
                            }
                        }),
                        StatGrid {
                            if let Some(d) = dashboard() {
                                StatReadout {
                                    label: "词表覆盖率".to_string(),
                                    value: format_relevance(
                                        d.relation_coverage.coverage_ratio as f32,
                                    ),
                                    icon: Some("🎯".to_string()),
                                    delta: Some(format!(
                                        "规范 {} · 同义 {} · 漂移 {}",
                                        d.relation_coverage.canonical_relations,
                                        d.relation_coverage.via_synonym_relations,
                                        d.relation_coverage.drift_relations
                                    )),
                                }
                                StatReadout {
                                    label: "漂移关系数".to_string(),
                                    value: format_compact_count(
                                        d.relation_coverage.drift_relations,
                                    ),
                                    icon: Some("🌊".to_string()),
                                    delta: Some("词表外关系边总数".to_string()),
                                }
                                StatReadout {
                                    label: "漂移节点".to_string(),
                                    value: format_compact_count(d.drift_node_count),
                                    unit: Some("个".to_string()),
                                    icon: Some("🧩".to_string()),
                                    delta: Some(format!(
                                        "占节点总数 {:.0}%",
                                        if d.total_node_count > 0 {
                                            d.drift_node_count as f64
                                                / d.total_node_count as f64
                                                * 100.0
                                        } else {
                                            0.0
                                        }
                                    )),
                                }
                            } else if dashboard_loading() {
                                Loading { size: "md" }
                            } else {
                                EmptyState {
                                    icon: Some("⚠️".to_string()),
                                    message: "漂移看板加载失败，请点击右上角刷新重试".to_string(),
                                }
                            }
                        }
                        if let Some(d) = dashboard() {
                            if d.top_drift_words.is_empty() {
                                EmptyState {
                                    icon: Some("✨".to_string()),
                                    message: "暂无漂移词，词表覆盖良好".to_string(),
                                }
                            } else {
                                div { class: "overflow-x-auto",
                                    table { class: "table hud-table table-zebra table-pin-rows",
                                        thead {
                                            tr {
                                                th { "漂移原文" }
                                                th { "种类" }
                                                th { "词频" }
                                                th { "Agent 数" }
                                                th { "" }
                                            }
                                        }
                                        tbody {
                                            for w in d.top_drift_words {
                                                tr { key: "{w.raw_term}",
                                                    td { span { class: "font-mono text-sm", "{w.raw_term}" } }
                                                    td { {kind_badge(w.kind)} }
                                                    td { "{format_compact_count(w.count)}" }
                                                    td { "{w.agent_count}" }
                                                    td {
                                                        button { class: "btn hud-btn btn-ghost btn-xs",
                                                            onclick: move |_| open_drill(w.raw_term.clone()),
                                                            "下钻"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // ==================== 实体类词表 ====================
                    HudSection {
                        eyebrow: Some("CLASSES".to_string()),
                        title: "实体类词表".to_string(),
                        actions: Some(rsx! {
                            button { class: "btn hud-btn btn-primary btn-sm",
                                onclick: move |_| {
                                    class_edit_id.set(None);
                                    class_form_key.set(String::new());
                                    class_form_name.set(String::new());
                                    class_form_desc.set(String::new());
                                    class_form_fields.set(String::new());
                                    show_class_modal.set(true);
                                },
                                "+ 新增实体类"
                            }
                        }),
                        div { class: "filter-row",
                            div { class: "filter-item",
                                label { class: "label", span { class: "label-text", "状态" } }
                                select { class: "select select-bordered hud-input w-full",
                                    value: "{class_status_filter}",
                                    onchange: move |e| {
                                        class_status_filter.set(e.value());
                                        fetch_classes(true);
                                    },
                                    option { value: "", "全部" }
                                    option { value: "Active", "启用中" }
                                    option { value: "Retired", "已退役" }
                                }
                            }
                            div { class: "filter-item flex-[2]",
                                label { class: "label", span { class: "label-text", "关键词" } }
                                input { class: "input input-bordered hud-input w-full",
                                    value: "{class_keyword}",
                                    oninput: move |e| class_keyword.set(e.value()),
                                    onkeydown: move |evt| {
                                        if evt.key() == Key::Enter {
                                            fetch_classes(true);
                                        }
                                    },
                                    placeholder: "按规范词 / 展示名模糊匹配，回车查询"
                                }
                            }
                            div { class: "filter-item justify-end",
                                label { class: "label", span { class: "label-text", "操作" } }
                                button { class: "btn hud-btn btn-primary",
                                    onclick: move |_| fetch_classes(true),
                                    "查询"
                                }
                            }
                        }
                        if classes_loading() {
                            Loading { size: "md" }
                        } else if classes.is_empty() {
                            EmptyState { message: "暂无实体类词条，点击右上角新增或同步预置词表".to_string() }
                        } else {
                            div { class: "overflow-x-auto",
                                table { class: "table hud-table table-zebra table-pin-rows",
                                    thead {
                                        tr {
                                            th { "规范词" }
                                            th { "展示名" }
                                            th { "描述" }
                                            th { "必填属性" }
                                            th { "状态" }
                                            th { "更新时间" }
                                            th { "操作" }
                                        }
                                    }
                                    tbody {
                                        for c in classes().iter().cloned() {
                                            {
                                                let edit_item = c.clone();
                                                let retire_id = c.id.clone();
                                                rsx! {
                                                    tr { key: "{c.id}",
                                                        td { span { class: "font-mono text-sm", "{c.term_key}" } }
                                                        td { "{c.display_name}" }
                                                        td {
                                                            div { class: "max-w-[220px] truncate", title: "{c.description}",
                                                                "{c.description}"
                                                            }
                                                        }
                                                        td {
                                                            if c.required_fields.is_empty() {
                                                                span { class: "opacity-50", "-" }
                                                            } else {
                                                                span { class: "font-mono text-xs", "{c.required_fields.join(\", \")}" }
                                                            }
                                                        }
                                                        td { {status_badge(c.status)} }
                                                        td { class: "whitespace-nowrap", "{format_datetime(c.updated_at * 1000)}" }
                                                        td {
                                                            div { class: "flex gap-2 items-center",
                                                                button { class: "btn hud-btn btn-ghost btn-sm",
                                                                    onclick: move |_| {
                                                                        class_edit_id.set(Some(edit_item.id.clone()));
                                                                        class_form_key.set(edit_item.term_key.clone());
                                                                        class_form_name.set(edit_item.display_name.clone());
                                                                        class_form_desc.set(edit_item.description.clone());
                                                                        class_form_fields.set(edit_item.required_fields.join(", "));
                                                                        show_class_modal.set(true);
                                                                    },
                                                                    "编辑"
                                                                }
                                                                button { class: "btn hud-btn btn-error btn-sm",
                                                                    disabled: c.status == OntologyStatus::Retired,
                                                                    onclick: move |_| class_retire_id.set(Some(retire_id.clone())),
                                                                    "退役"
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if classes_total() > 0 {
                            div { class: "flex items-center justify-between mt-3",
                                span { class: "text-sm opacity-70",
                                    "共 {classes_total()} 条"
                                }
                                div { class: "flex gap-2",
                                    button { class: "btn hud-btn btn-ghost btn-sm",
                                        disabled: class_offset() == 0,
                                        onclick: move |_| {
                                            class_offset.set(class_offset().saturating_sub(PAGE_SIZE));
                                            fetch_classes(false);
                                        },
                                        "上一页"
                                    }
                                    button { class: "btn hud-btn btn-ghost btn-sm",
                                        disabled: class_offset() + PAGE_SIZE >= classes_total(),
                                        onclick: move |_| {
                                            class_offset.set(class_offset() + PAGE_SIZE);
                                            fetch_classes(false);
                                        },
                                        "下一页"
                                    }
                                }
                            }
                        }
                    }

                    // ==================== 关系类型词表 ====================
                    HudSection {
                        eyebrow: Some("RELATIONS".to_string()),
                        title: "关系类型词表".to_string(),
                        actions: Some(rsx! {
                            button { class: "btn hud-btn btn-primary btn-sm",
                                onclick: move |_| {
                                    relation_edit_id.set(None);
                                    relation_form_key.set(String::new());
                                    relation_form_name.set(String::new());
                                    relation_form_desc.set(String::new());
                                    relation_form_domain.set(String::new());
                                    relation_form_range.set(String::new());
                                    relation_form_weight.set(String::new());
                                    relation_form_inverse.set(String::new());
                                    show_relation_modal.set(true);
                                },
                                "+ 新增关系类型"
                            }
                        }),
                        div { class: "filter-row",
                            div { class: "filter-item",
                                label { class: "label", span { class: "label-text", "状态" } }
                                select { class: "select select-bordered hud-input w-full",
                                    value: "{relation_status_filter}",
                                    onchange: move |e| {
                                        relation_status_filter.set(e.value());
                                        fetch_relations(true);
                                    },
                                    option { value: "", "全部" }
                                    option { value: "Active", "启用中" }
                                    option { value: "Retired", "已退役" }
                                }
                            }
                            div { class: "filter-item flex-[2]",
                                label { class: "label", span { class: "label-text", "关键词" } }
                                input { class: "input input-bordered hud-input w-full",
                                    value: "{relation_keyword}",
                                    oninput: move |e| relation_keyword.set(e.value()),
                                    onkeydown: move |evt| {
                                        if evt.key() == Key::Enter {
                                            fetch_relations(true);
                                        }
                                    },
                                    placeholder: "按规范词 / 展示名模糊匹配，回车查询"
                                }
                            }
                            div { class: "filter-item justify-end",
                                label { class: "label", span { class: "label-text", "操作" } }
                                button { class: "btn hud-btn btn-primary",
                                    onclick: move |_| fetch_relations(true),
                                    "查询"
                                }
                            }
                        }
                        if relations_loading() {
                            Loading { size: "md" }
                        } else if relations.is_empty() {
                            EmptyState { message: "暂无关系类型词条，点击右上角新增或同步预置词表".to_string() }
                        } else {
                            div { class: "overflow-x-auto",
                                table { class: "table hud-table table-zebra table-pin-rows",
                                    thead {
                                        tr {
                                            th { "规范词" }
                                            th { "展示名" }
                                            th { "描述" }
                                            th { "域 → 值域" }
                                            th { "权重" }
                                            th { "反向词" }
                                            th { "状态" }
                                            th { "更新时间" }
                                            th { "操作" }
                                        }
                                    }
                                    tbody {
                                        for r in relations().iter().cloned() {
                                            {
                                                let edit_item = r.clone();
                                                let retire_id = r.id.clone();
                                                let domain_label = if r.domain_classes.is_empty() {
                                                    "不限".to_string()
                                                } else {
                                                    r.domain_classes.join(", ")
                                                };
                                                let range_label = if r.range_classes.is_empty() {
                                                    "不限".to_string()
                                                } else {
                                                    r.range_classes.join(", ")
                                                };
                                                rsx! {
                                                    tr { key: "{r.id}",
                                                        td { span { class: "font-mono text-sm", "{r.term_key}" } }
                                                        td { "{r.display_name}" }
                                                        td {
                                                            div { class: "max-w-[200px] truncate", title: "{r.description}",
                                                                "{r.description}"
                                                            }
                                                        }
                                                        td {
                                                            div { class: "text-xs whitespace-nowrap",
                                                                span { class: "font-mono", "{domain_label}" }
                                                                span { class: "opacity-50 mx-1", "→" }
                                                                span { class: "font-mono", "{range_label}" }
                                                            }
                                                        }
                                                        td { "{r.weight_base}" }
                                                        td {
                                                            if let Some(inv) = r.inverse_key {
                                                                span { class: "font-mono text-xs", "{inv}" }
                                                            } else {
                                                                span { class: "opacity-50", "-" }
                                                            }
                                                        }
                                                        td { {status_badge(r.status)} }
                                                        td { class: "whitespace-nowrap", "{format_datetime(r.updated_at * 1000)}" }
                                                        td {
                                                            div { class: "flex gap-2 items-center",
                                                                button { class: "btn hud-btn btn-ghost btn-sm",
                                                                    onclick: move |_| {
                                                                        relation_edit_id.set(Some(edit_item.id.clone()));
                                                                        relation_form_key.set(edit_item.term_key.clone());
                                                                        relation_form_name.set(edit_item.display_name.clone());
                                                                        relation_form_desc.set(edit_item.description.clone());
                                                                        relation_form_domain.set(edit_item.domain_classes.join(", "));
                                                                        relation_form_range.set(edit_item.range_classes.join(", "));
                                                                        relation_form_weight.set(format!("{}", edit_item.weight_base));
                                                                        relation_form_inverse.set(edit_item.inverse_key.clone().unwrap_or_default());
                                                                        show_relation_modal.set(true);
                                                                    },
                                                                    "编辑"
                                                                }
                                                                button { class: "btn hud-btn btn-error btn-sm",
                                                                    disabled: r.status == OntologyStatus::Retired,
                                                                    onclick: move |_| relation_retire_id.set(Some(retire_id.clone())),
                                                                    "退役"
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if relations_total() > 0 {
                            div { class: "flex items-center justify-between mt-3",
                                span { class: "text-sm opacity-70",
                                    "共 {relations_total()} 条"
                                }
                                div { class: "flex gap-2",
                                    button { class: "btn hud-btn btn-ghost btn-sm",
                                        disabled: relation_offset() == 0,
                                        onclick: move |_| {
                                            relation_offset.set(relation_offset().saturating_sub(PAGE_SIZE));
                                            fetch_relations(false);
                                        },
                                        "上一页"
                                    }
                                    button { class: "btn hud-btn btn-ghost btn-sm",
                                        disabled: relation_offset() + PAGE_SIZE >= relations_total(),
                                        onclick: move |_| {
                                            relation_offset.set(relation_offset() + PAGE_SIZE);
                                            fetch_relations(false);
                                        },
                                        "下一页"
                                    }
                                }
                            }
                        }
                    }

                    // ==================== 同义映射 ====================
                    HudSection {
                        eyebrow: Some("SYNONYMS".to_string()),
                        title: "同义映射".to_string(),
                        actions: Some(rsx! {
                            button { class: "btn hud-btn btn-primary btn-sm",
                                onclick: move |_| {
                                    synonym_form_raw.set(String::new());
                                    synonym_form_kind.set("relation".to_string());
                                    synonym_form_target_key.set(String::new());
                                    show_synonym_modal.set(true);
                                },
                                "+ 新增同义映射"
                            }
                        }),
                        div { class: "filter-row",
                            div { class: "filter-item",
                                label { class: "label", span { class: "label-text", "目标种类" } }
                                select { class: "select select-bordered hud-input w-full",
                                    value: "{synonym_kind_filter}",
                                    onchange: move |e| {
                                        synonym_kind_filter.set(e.value());
                                        fetch_synonyms(true);
                                    },
                                    option { value: "", "全部" }
                                    option { value: "relation", "关系类型" }
                                    option { value: "class", "实体类" }
                                }
                            }
                        }
                        if synonyms_loading() {
                            Loading { size: "md" }
                        } else if synonyms.is_empty() {
                            EmptyState { message: "暂无同义映射，新增后漂移原文将自动归并到目标规范词".to_string() }
                        } else {
                            div { class: "overflow-x-auto",
                                table { class: "table hud-table table-zebra table-pin-rows",
                                    thead {
                                        tr {
                                            th { "漂移原文" }
                                            th { "" }
                                            th { "目标种类" }
                                            th { "目标规范词" }
                                            th { "创建时间" }
                                            th { "操作" }
                                        }
                                    }
                                    tbody {
                                        for s in synonyms().iter().cloned() {
                                            {
                                                let delete_id = s.id.clone();
                                                rsx! {
                                                    tr { key: "{s.id}",
                                                        td { span { class: "font-mono text-sm", "{s.raw_term}" } }
                                                        td { class: "opacity-50", "→" }
                                                        td { {kind_badge(s.target_kind)} }
                                                        td { span { class: "font-mono text-sm", "{s.target_key}" } }
                                                        td { class: "whitespace-nowrap", "{format_datetime(s.created_at * 1000)}" }
                                                        td {
                                                            button { class: "btn hud-btn btn-error btn-sm",
                                                                onclick: move |_| synonym_delete_id.set(Some(delete_id.clone())),
                                                                "删除"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if synonyms_total() > 0 {
                            div { class: "flex items-center justify-between mt-3",
                                span { class: "text-sm opacity-70",
                                    "共 {synonyms_total()} 条"
                                }
                                div { class: "flex gap-2",
                                    button { class: "btn hud-btn btn-ghost btn-sm",
                                        disabled: synonym_offset() == 0,
                                        onclick: move |_| {
                                            synonym_offset.set(synonym_offset().saturating_sub(PAGE_SIZE));
                                            fetch_synonyms(false);
                                        },
                                        "上一页"
                                    }
                                    button { class: "btn hud-btn btn-ghost btn-sm",
                                        disabled: synonym_offset() + PAGE_SIZE >= synonyms_total(),
                                        onclick: move |_| {
                                            synonym_offset.set(synonym_offset() + PAGE_SIZE);
                                            fetch_synonyms(false);
                                        },
                                        "下一页"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ==================== 实体类 新增/编辑 Modal ====================
            Modal {
                title: if class_edit_id().is_some() { "编辑实体类".to_string() } else { "新增实体类".to_string() },
                show: show_class_modal(),
                on_close: move |_| show_class_modal.set(false),
                footer: rsx! {
                    button { class: "btn hud-btn btn-ghost", onclick: move |_| show_class_modal.set(false), "取消" }
                    button { class: "btn hud-btn btn-primary", disabled: class_saving(),
                        onclick: move |_| handle_save_class(),
                        if class_saving() { "保存中..." } else { "保存" }
                    }
                },
                div { class: "space-y-4",
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "规范词 key *" } }
                        input { class: "input input-bordered hud-input w-full font-mono",
                            value: "{class_form_key}",
                            disabled: class_edit_id().is_some(),
                            oninput: move |e| class_form_key.set(e.value()),
                            placeholder: "小写 snake_case，如：customer_profile"
                        }
                        if class_edit_id().is_some() {
                            span { class: "label-text-alt opacity-60", "创建后不可修改，如需换词请退役后新建" }
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "展示名 *" } }
                        input { class: "input input-bordered hud-input w-full",
                            value: "{class_form_name}",
                            oninput: move |e| class_form_name.set(e.value()),
                            placeholder: "如：客户档案"
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "语义描述" } }
                        textarea { class: "textarea textarea-bordered hud-input w-full",
                            rows: "3",
                            value: "{class_form_desc}",
                            oninput: move |e| class_form_desc.set(e.value()),
                            placeholder: "该实体类的语义描述（会随词表注入提示词，帮助模型理解）"
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "必填属性清单" } }
                        input { class: "input input-bordered hud-input w-full font-mono",
                            value: "{class_form_fields}",
                            oninput: move |e| class_form_fields.set(e.value()),
                            placeholder: "逗号分隔，如：name, contact；留空 = 无必填属性"
                        }
                    }
                }
            }

            // ==================== 关系类型 新增/编辑 Modal ====================
            Modal {
                title: if relation_edit_id().is_some() { "编辑关系类型".to_string() } else { "新增关系类型".to_string() },
                show: show_relation_modal(),
                on_close: move |_| show_relation_modal.set(false),
                footer: rsx! {
                    button { class: "btn hud-btn btn-ghost", onclick: move |_| show_relation_modal.set(false), "取消" }
                    button { class: "btn hud-btn btn-primary", disabled: relation_saving(),
                        onclick: move |_| handle_save_relation(),
                        if relation_saving() { "保存中..." } else { "保存" }
                    }
                },
                div { class: "space-y-4",
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "规范词 key *" } }
                        input { class: "input input-bordered hud-input w-full font-mono",
                            value: "{relation_form_key}",
                            disabled: relation_edit_id().is_some(),
                            oninput: move |e| relation_form_key.set(e.value()),
                            placeholder: "小写 snake_case，如：belongs_to"
                        }
                        if relation_edit_id().is_some() {
                            span { class: "label-text-alt opacity-60", "创建后不可修改，如需换词请退役后新建" }
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "展示名 *" } }
                        input { class: "input input-bordered hud-input w-full",
                            value: "{relation_form_name}",
                            oninput: move |e| relation_form_name.set(e.value()),
                            placeholder: "如：归属"
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "语义描述" } }
                        textarea { class: "textarea textarea-bordered hud-input w-full",
                            rows: "3",
                            value: "{relation_form_desc}",
                            oninput: move |e| relation_form_desc.set(e.value()),
                            placeholder: "该关系的语义描述（会随词表注入提示词，帮助模型理解）"
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "允许的源实体类（域）" } }
                        input { class: "input input-bordered hud-input w-full font-mono",
                            value: "{relation_form_domain}",
                            oninput: move |e| relation_form_domain.set(e.value()),
                            placeholder: "逗号分隔的实体类 key；留空 = 不限"
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "允许的目标实体类（值域）" } }
                        input { class: "input input-bordered hud-input w-full font-mono",
                            value: "{relation_form_range}",
                            oninput: move |e| relation_form_range.set(e.value()),
                            placeholder: "逗号分隔的实体类 key；留空 = 不限"
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "边权重基数" } }
                        input { class: "input input-bordered hud-input w-full",
                            r#type: "number",
                            step: "0.1",
                            min: "0",
                            value: "{relation_form_weight}",
                            oninput: move |e| relation_form_weight.set(e.value()),
                            placeholder: "留空 = 默认 1.0"
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "反向关系 key" } }
                        input { class: "input input-bordered hud-input w-full font-mono",
                            value: "{relation_form_inverse}",
                            oninput: move |e| relation_form_inverse.set(e.value()),
                            placeholder: "如 belongs_to 的反向词 owns；留空 = 无对称反向词"
                        }
                    }
                }
            }

            // ==================== 同义映射 新增 Modal ====================
            Modal {
                title: "新增同义映射".to_string(),
                show: show_synonym_modal(),
                on_close: move |_| show_synonym_modal.set(false),
                footer: rsx! {
                    button { class: "btn hud-btn btn-ghost", onclick: move |_| show_synonym_modal.set(false), "取消" }
                    button { class: "btn hud-btn btn-primary", disabled: synonym_saving(),
                        onclick: move |_| handle_save_synonym(),
                        if synonym_saving() { "保存中..." } else { "保存" }
                    }
                },
                div { class: "space-y-4",
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "漂移原文 *" } }
                        input { class: "input input-bordered hud-input w-full",
                            value: "{synonym_form_raw}",
                            oninput: move |e| synonym_form_raw.set(e.value()),
                            placeholder: "图谱中出现的非规范说法，入库前自动 trim + 小写"
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "目标词条种类" } }
                        select { class: "select select-bordered hud-input w-full",
                            value: "{synonym_form_kind}",
                            onchange: move |e| synonym_form_kind.set(e.value()),
                            option { value: "relation", "关系类型" }
                            option { value: "class", "实体类" }
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "目标规范词 key *" } }
                        input { class: "input input-bordered hud-input w-full font-mono",
                            value: "{synonym_form_target_key}",
                            oninput: move |e| synonym_form_target_key.set(e.value()),
                            placeholder: "必须存在于对应词表，否则视为无效映射"
                        }
                    }
                }
            }

            // ==================== 预置词表同步 Modal ====================
            Modal {
                title: "同步预置词表".to_string(),
                show: show_sync_modal(),
                on_close: move |_| show_sync_modal.set(false),
                width_class: Some("max-w-3xl".to_string()),
                footer: rsx! {
                    button { class: "btn hud-btn btn-ghost", onclick: move |_| show_sync_modal.set(false), "关闭" }
                    button { class: "btn hud-btn btn-primary",
                        disabled: syncing() || sync_preview_loading() || sync_result().is_some(),
                        onclick: move |_| handle_sync(),
                        if syncing() { "同步中..." } else { "执行同步" }
                    }
                },
                if sync_preview_loading() {
                    Loading { size: "md" }
                } else if let Some(p) = sync_preview() {
                    div { class: "space-y-4",
                        div { class: "flex flex-wrap items-center gap-3 text-sm",
                            span { class: "badge hud-badge badge-success", "将新建 {p.missing_count}" }
                            span { class: "badge hud-badge badge-ghost", "已存在 {p.existing_count}" }
                            span { class: "opacity-70", "预置共 {p.items.len()} 条 · 仅补缺，不覆盖已有修改" }
                        }
                        if let Some(r) = sync_result() {
                            div { class: "alert alert-success text-sm",
                                "同步完成：新建 {r.created} 条，跳过 {r.skipped} 条（预置共 {r.total} 条）"
                            }
                        }
                        div { class: "max-h-80 overflow-y-auto space-y-1",
                            for item in p.items {
                                                div { class: "flex flex-wrap items-center gap-3 rounded-lg border border-base-300 px-3 py-2",
                                                    {kind_badge(item.kind)}
                                                    span { class: "font-mono text-sm", "{item.term_key}" }
                                                    span { class: "text-sm flex-1 truncate", "{item.display_name}" }
                                                    if item.exists {
                                                        span { class: "badge badge-ghost badge-sm", "已存在·跳过" }
                                                    } else {
                                                        span { class: "badge badge-success badge-sm", "将新建" }
                                                    }
                                                    div { class: "w-full text-xs opacity-60 truncate", "{item.description}" }
                                                }
                                            }
                        }
                    }
                }
            }

            // ==================== 漂移词下钻 Modal ====================
            Modal {
                title: format!("词条「{}」下钻", drill_term().unwrap_or_default()),
                show: drill_term().is_some(),
                on_close: move |_| drill_term.set(None),
                width_class: Some("max-w-3xl".to_string()),
                if drill_loading() {
                    Loading { size: "md" }
                } else {
                    div { class: "space-y-6",
                        div {
                            h4 { class: "font-semibold mb-2", "关系（边）明细" }
                            if drill_relations().is_empty() {
                                EmptyState { icon: Some("🔗".to_string()), message: "无相关关系边".to_string() }
                            } else {
                                div { class: "overflow-x-auto max-h-64 overflow-y-auto",
                                    table { class: "table hud-table table-zebra",
                                        thead {
                                            tr {
                                                th { "源节点" }
                                                th { "" }
                                                th { "目标节点" }
                                                th { "Agent" }
                                                th { "时间" }
                                            }
                                        }
                                        tbody {
                                            for e in drill_relations().iter().cloned() {
                                                tr { key: "{e.id}",
                                                    td { "{e.source_name}" }
                                                    td { class: "opacity-50", "→" }
                                                    td { "{e.target_name}" }
                                                    td {
                                                        div { class: "font-mono text-xs max-w-[120px] truncate",
                                                            title: "{e.agent_id}", "{e.agent_id}"
                                                        }
                                                    }
                                                    td { class: "whitespace-nowrap", "{format_datetime(e.created_at * 1000)}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        div {
                            h4 { class: "font-semibold mb-2", "节点明细" }
                            if drill_classes().is_empty() {
                                EmptyState { icon: Some("🧩".to_string()), message: "无相关节点".to_string() }
                            } else {
                                div { class: "overflow-x-auto max-h-64 overflow-y-auto",
                                    table { class: "table hud-table table-zebra",
                                        thead {
                                            tr {
                                                th { "节点名称" }
                                                th { "Agent" }
                                                th { "时间" }
                                            }
                                        }
                                        tbody {
                                            for c in drill_classes().iter().cloned() {
                                                tr { key: "{c.id}",
                                                    td { "{c.name}" }
                                                    td {
                                                        div { class: "font-mono text-xs max-w-[120px] truncate",
                                                            title: "{c.agent_id}", "{c.agent_id}"
                                                        }
                                                    }
                                                    td { class: "whitespace-nowrap", "{format_datetime(c.created_at * 1000)}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ==================== 确认框 ====================
            ConfirmDialog {
                show: class_retire_id().is_some(),
                title: "确认退役".to_string(),
                message: "退役后该实体类不再参与解析（历史存量引用仍可解释）。确定退役？".to_string(),
                confirm_text: Some("退役".to_string()),
                confirm_class: Some("btn hud-btn btn-error".to_string()),
                on_confirm: move |_| handle_retire_class(),
                on_cancel: move |_| class_retire_id.set(None),
            }
            ConfirmDialog {
                show: relation_retire_id().is_some(),
                title: "确认退役".to_string(),
                message: "退役后该关系类型不再参与解析（历史存量边仍可解释）。确定退役？".to_string(),
                confirm_text: Some("退役".to_string()),
                confirm_class: Some("btn hud-btn btn-error".to_string()),
                on_confirm: move |_| handle_retire_relation(),
                on_cancel: move |_| relation_retire_id.set(None),
            }
            ConfirmDialog {
                show: synonym_delete_id().is_some(),
                title: "确认删除".to_string(),
                message: "删除后相关漂移原文将不再归并，下一次解析即回落为漂移词。确定删除？".to_string(),
                confirm_text: Some("删除".to_string()),
                confirm_class: Some("btn hud-btn btn-error".to_string()),
                on_confirm: move |_| handle_delete_synonym(),
                on_cancel: move |_| synonym_delete_id.set(None),
            }
        }
    }
}
