//! 工具调用记录查询页 - 列表 + 详情 Modal + 授权审批（待审批单 / trace 行内快捷审批）
//!
//! ## 授权审批怎么和调用记录挂上
//! 后端在拦截建单时把「触发拦截的工具调用 ID」写进授权单
//! （`AuthorizationDetailDto.call_id` == 本页调用记录的 `call_id`），因此两者是
//! **精确一对一**关联，不靠 (agent, tool, 时间窗) 反推：
//! - 顶部「待审批授权单」区块：审批面主入口（全部 Pending 单 + 通过/拒绝）；
//! - 调用记录表「审批」列：命中待审批单 → 行内快捷通过/拒绝；
//! - 详情 Modal：关联授权单卡片（Pending 可批，Active 可撤销）。

use std::collections::HashMap;

use crate::components::hud::HudPanel;
use dioxus::prelude::*;

use crate::api::finance::{
    decide_tool_authorization, get_tool_call_entry, list_tool_authorizations,
    query_tool_call_entries, revoke_tool_authorization,
};
use crate::components::avatar_bubble::AvatarTone;
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::identity_chip::IdentityChip;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use crate::utils::status::{
    authorization_revocable, authorization_status_badge, authorization_status_text, short_id,
    tool_call_status_badge, tool_call_status_text,
};
use common::api::{
    AuthorizationDecisionRequest, AuthorizationDetailDto, AuthorizationQueryRequest,
    AuthorizationStatusDto, QueryToolCallEntriesRequest, RevokeAuthorizationRequest,
    ToolCallEntryDetail,
};

#[component]
pub fn FinanceToolCallEntries() -> Element {
    let toast = use_toast();

    let mut entries = use_signal(Vec::<ToolCallEntryDetail>::new);
    let mut loading = use_signal(|| true);
    let mut query_call_id = use_signal(String::new);
    let mut query_agent_id = use_signal(String::new);
    let mut query_tool_id = use_signal(String::new);
    let mut query_limit = use_signal(|| "50".to_string());

    // 授权单：审批面数据源（待审批区块 / 调用记录「审批」列 / 详情卡片都由此派生）
    let mut auths = use_signal(Vec::<AuthorizationDetailDto>::new);
    let mut auths_loading = use_signal(|| true);
    // 正在提交的授权单 ID：Some 时禁用全部审批按钮（防重复决策）
    let mut deciding = use_signal(|| Option::<String>::None);

    // 详情 Modal
    let mut show_detail_modal = use_signal(|| false);
    let mut selected_entry = use_signal(|| Option::<ToolCallEntryDetail>::None);
    let mut detail_loading = use_signal(|| false);

    // 撤销二次确认（影响面已生效，走确认弹窗）
    let mut show_revoke_confirm = use_signal(|| false);
    let mut revoke_target = use_signal(|| Option::<String>::None);

    // ---------- 数据加载 ----------
    // 两个闭包只捕获 Copy 的 Signal / ToastState ⇒ 闭包自身 Copy，可被多个事件处理器复用
    let mut load_entries = move |params: QueryToolCallEntriesRequest| {
        loading.set(true);
        spawn(async move {
            match query_tool_call_entries(&params).await {
                Ok(list) => entries.set(list),
                Err(e) => toast.error(format!("加载调用记录失败: {e}")),
            }
            loading.set(false);
        });
    };
    let mut load_auths = move || {
        auths_loading.set(true);
        spawn(async move {
            match list_tool_authorizations(&AuthorizationQueryRequest::default()).await {
                Ok(list) => auths.set(list),
                Err(e) => toast.error(format!("加载授权单失败: {e}")),
            }
            auths_loading.set(false);
        });
    };

    // 首屏：两张表都用默认条件加载。
    // ⚠️ effect 内**不读任何筛选信号**，否则每敲一个字都会重跑 effect 发一次请求
    use_effect(move || {
        load_entries(QueryToolCallEntriesRequest {
            call_id: None,
            agent_id: None,
            project_id: None,
            task_id: None,
            tool_id: None,
            status: None,
            started_after: None,
            started_before: None,
            limit: Some(50),
        });
        load_auths();
    });

    // ---------- 审批动作 ----------
    // 通过 / 拒绝：平台直批通道（user ctx），后端 domain 二次校验身份与单据状态
    let mut do_decide = move |(auth_id, approve): (String, bool)| {
        if deciding().is_some() {
            return;
        }
        deciding.set(Some(auth_id.clone()));
        spawn(async move {
            let req = AuthorizationDecisionRequest {
                authorization_id: auth_id.clone(),
                decision: if approve { "Approve" } else { "Reject" }.to_string(),
                // scope 省略 = 沿用建单签名的最窄授权（次数上限与 TTL 由后端按规则幂等性给）
                scope: None,
                evidence_class: None,
                evidence_message_id: None,
                mediator_agent_id: None,
            };
            match decide_tool_authorization(&req).await {
                Ok(resp) => {
                    toast.success(format!(
                        "授权单 {} 已{}",
                        short_id(&auth_id),
                        if approve { "批准" } else { "拒绝" }
                    ));
                    // 就地更新状态：调用记录本身没变，表内审批态由 auths 派生；
                    // 需要完整字段（TTL / 剩余次数）时点「刷新」重新同步
                    apply_status_locally(auths, &auth_id, resp.status, resp.grant_id);
                }
                Err(e) => toast.error(format!("审批失败: {e}")),
            }
            deciding.set(None);
        });
    };
    let on_decide = EventHandler::new(move |args: (String, bool)| do_decide(args));

    // 撤销（已生效授权即时失效）
    let mut do_revoke = move |auth_id: String| {
        if deciding().is_some() {
            return;
        }
        deciding.set(Some(auth_id.clone()));
        spawn(async move {
            let req = RevokeAuthorizationRequest {
                authorization_id: auth_id.clone(),
                reason: None,
            };
            match revoke_tool_authorization(&req).await {
                Ok(resp) => {
                    toast.success(format!("授权单 {} 已撤销", short_id(&auth_id)));
                    apply_status_locally(auths, &auth_id, resp.status, resp.grant_id);
                }
                Err(e) => toast.error(format!("撤销失败: {e}")),
            }
            deciding.set(None);
        });
    };

    let on_search = move |_| {
        load_entries(build_trace_query(
            query_call_id(),
            query_agent_id(),
            query_tool_id(),
            query_limit(),
        ));
    };

    let mut on_click_entry = move |call_id: String| {
        show_detail_modal.set(true);
        selected_entry.set(None);
        detail_loading.set(true);
        spawn(async move {
            match get_tool_call_entry(&call_id).await {
                Ok(resp) => selected_entry.set(Some(resp)),
                Err(e) => {
                    toast.error(format!("加载详情失败: {}", e));
                    show_detail_modal.set(false);
                }
            }
            detail_loading.set(false);
        });
    };

    // ---------- 渲染期派生数据 ----------
    let entries_list = entries.read().clone();
    let auths_list = auths.read().clone();
    let by_call = index_auths_by_call(&auths_list);
    let pending_count = auths_list
        .iter()
        .filter(|a| a.status == AuthorizationStatusDto::Pending)
        .count();
    let selected = selected_entry.read().clone();
    let selected_auth = selected
        .as_ref()
        .and_then(|e| by_call.get(e.call_id.as_str()).cloned());
    let is_deciding = deciding().is_some();

    rsx! {
        AppLayout {
            // ① 待审批授权单（审批面主入口）
            HudPanel { signal: Some(true), title: Some("待审批授权单".to_string()),
                extra_class: Some("mb-4".to_string()),
                actions: Some(rsx! {
                    div { class: "flex items-center gap-2",
                        if pending_count > 0 {
                            span { class: "badge hud-badge badge-sm badge-warning", "{pending_count}" }
                        }
                        button {
                            class: "btn hud-btn btn-ghost btn-xs",
                            onclick: move |_| load_auths(),
                            "刷新"
                        }
                    }
                }),
                div { class: "card-body",
                    if auths_loading() {
                        Loading {}
                    } else if pending_count == 0 {
                        EmptyState {
                            icon: "🛡️".to_string(),
                            message: "当前没有待审批的授权单".to_string()
                        }
                    } else {
                        div { class: "overflow-x-auto",
                            table { class: "table hud-table table-zebra table-xs",
                                thead { tr {
                                    th { "授权单" }
                                    th { "Agent" }
                                    th { "工具" }
                                    th { "受限命令签名" }
                                    th { "命中规则" }
                                    th { "关联调用" }
                                    th { "建单时间" }
                                    th { "操作" }
                                }}
                                tbody {
                                    for a in auths_list.iter().filter(|a| a.status == AuthorizationStatusDto::Pending) {
                                        {
                                            let auth_id = a.authorization_id.clone();
                                            let pass_id = a.authorization_id.clone();
                                            let reject_id = a.authorization_id.clone();
                                            let agent_id = a.agent_id.clone();
                                            let tool_id = a.tool_id.clone();
                                            let signature = a.command_signature.clone();
                                            let rule = a.blocking_rule.clone();
                                            let call_id = a.call_id.clone();
                                            let requested_at = a.requested_at_ms;
                                            rsx! {
                                                tr { key: "{auth_id}",
                                                    td { class: "font-mono text-xs", title: "{auth_id}", "{short_id(&auth_id)}" }
                                                    td { IdentityChip { id: agent_id, tone: AvatarTone::Agent } }
                                                    td { span { class: "font-mono text-xs", "{tool_id}" } }
                                                    td { class: "font-mono text-xs max-w-xs truncate", title: "{signature}", "{signature}" }
                                                    td { span { class: "text-xs", "{rule}" } }
                                                    td { class: "font-mono text-xs",
                                                        if let Some(cid) = call_id {
                                                            span { title: "{cid}", "{short_id(&cid)}" }
                                                        } else {
                                                            span { class: "text-base-content/40", "主动建单" }
                                                        }
                                                    }
                                                    td { class: "font-mono text-xs", "{crate::utils::format_datetime(requested_at)}" }
                                                    td {
                                                        div { class: "flex items-center gap-1",
                                                            button {
                                                                class: "btn hud-btn btn-xs btn-success",
                                                                disabled: is_deciding,
                                                                onclick: move |_| on_decide.call((pass_id.clone(), true)),
                                                                "通过"
                                                            }
                                                            button {
                                                                class: "btn hud-btn btn-xs btn-error",
                                                                disabled: is_deciding,
                                                                onclick: move |_| on_decide.call((reject_id.clone(), false)),
                                                                "拒绝"
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
                }
            }

            // ② 工具调用记录
            HudPanel { signal: Some(true),
                title: Some("工具调用记录".to_string()),
                div { class: "card-body",
                    // 查询表单
                    div { class: "grid grid-cols-1 md:grid-cols-4 gap-4 mb-4",
                        div { class: "form-control",
                            label { class: "label", span { class: "label-text text-sm", "Call ID" } }
                            input { class: "input input-bordered input-sm w-full", value: "{query_call_id}",
                                oninput: move |e| query_call_id.set(e.value()), placeholder: "精确匹配" }
                        }
                        div { class: "form-control",
                            label { class: "label", span { class: "label-text text-sm", "Agent ID" } }
                            input { class: "input input-bordered input-sm w-full", value: "{query_agent_id}",
                                oninput: move |e| query_agent_id.set(e.value()) }
                        }
                        div { class: "form-control",
                            label { class: "label", span { class: "label-text text-sm", "Tool ID" } }
                            input { class: "input input-bordered input-sm w-full", value: "{query_tool_id}",
                                oninput: move |e| query_tool_id.set(e.value()) }
                        }
                        div { class: "form-control",
                            label { class: "label", span { class: "label-text text-sm", "Limit" } }
                            input { class: "input input-bordered input-sm w-full", r#type: "number", value: "{query_limit}",
                                oninput: move |e| query_limit.set(e.value()) }
                        }
                    }
                    div { class: "flex justify-end mb-4",
                        button { class: "btn hud-btn btn-primary btn-sm", onclick: on_search, "🔍 查询" }
                    }
                    if loading() {
                        Loading {}
                    } else if entries_list.is_empty() {
                        EmptyState { icon: "🔍".to_string(), message: "无匹配记录".to_string() }
                    } else {
                        div { class: "overflow-x-auto",
                            table { class: "table hud-table table-zebra table-xs",
                                thead { tr {
                                    th { "Call ID" }
                                    th { "工具" }
                                    th { "Agent" }
                                    th { "状态" }
                                    th { "耗时" }
                                    th { "开始时间" }
                                    th { "审批" }
                                    th { "操作" }
                                }}
                                tbody {
                                    for e in entries_list.iter() {
                                        {
                                            let call_id = e.call_id.clone();
                                            let detail_call_id = e.call_id.clone();
                                            let tool_name = e.tool_name.clone();
                                            let agent_id = e.agent_id.clone();
                                            let status = e.status;
                                            let duration_ms = e.duration_ms;
                                            let started_at = e.started_at;
                                            let linked = by_call.get(&call_id).cloned();
                                            rsx! {
                                                tr { key: "{call_id}",
                                                    td { class: "font-mono text-xs truncate", title: "{call_id}", "{call_id}" }
                                                    td { "{tool_name}" }
                                                    td {
                                                        if let Some(aid) = agent_id {
                                                            IdentityChip { id: aid, tone: AvatarTone::Agent }
                                                        } else {
                                                            span { class: "font-mono text-xs", "-" }
                                                        }
                                                    }
                                                    td { span { class: "{tool_call_status_badge(status)}", "{tool_call_status_text(status)}" } }
                                                    td { class: "font-mono", "{duration_ms}ms" }
                                                    td { class: "font-mono text-xs", "{crate::utils::format_datetime(started_at as i64)}" }
                                                    td { { approval_cell(linked, is_deciding, on_decide) } }
                                                    td {
                                                        button {
                                                            class: "btn hud-btn btn-ghost btn-xs",
                                                            onclick: move |_| on_click_entry(detail_call_id.clone()),
                                                            "详情"
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
            }

            // ③ 详情 Modal
            Modal {
                title: "工具调用详情".to_string(),
                show: show_detail_modal(),
                on_close: move |_| show_detail_modal.set(false),
                footer: rsx! { button { class: "btn hud-btn btn-ghost", onclick: move |_| show_detail_modal.set(false), "关闭" } },
                if detail_loading() {
                    Loading {}
                } else if let Some(e) = selected {
                    div { class: "space-y-3",
                        div { class: "grid grid-cols-2 gap-2 text-sm",
                            div { span { class: "text-base-content/60", "Call ID: " }, span { class: "font-mono", "{e.call_id}" } }
                            div { span { class: "text-base-content/60", "工具: " }, "{e.tool_name}" }
                            div { span { class: "text-base-content/60", "状态: " }, "{tool_call_status_text(e.status)}" }
                            div { span { class: "text-base-content/60", "耗时: " }, span { class: "font-mono", "{e.duration_ms}ms" } }
                            div { class: "flex items-center gap-1",
                                span { class: "text-base-content/60", "Agent: " },
                                if let Some(aid) = e.agent_id.clone() {
                                    IdentityChip { id: aid, tone: AvatarTone::Agent }
                                } else {
                                    span { class: "font-mono", "-" }
                                }
                            }
                            div { span { class: "text-base-content/60", "Task: " }, span { class: "font-mono", "{e.task_id.as_deref().unwrap_or(\"-\")}" } }
                        }

                        // 关联授权单：本调用被拦截而建的审批单（精确按 call_id 关联）
                        if let Some(a) = selected_auth {
                            {
                                let auth_id = a.authorization_id.clone();
                                let approve_id = a.authorization_id.clone();
                                let reject_id = a.authorization_id.clone();
                                let revoke_id = a.authorization_id.clone();
                                let status = a.status;
                                let rule = a.blocking_rule.clone();
                                let tool_id = a.tool_id.clone();
                                let signature = a.command_signature.clone();
                                let requested_at = a.requested_at_ms;
                                let expires_at = a.expires_at_ms;
                                let remaining = a.remaining_uses;
                                rsx! {
                                    div { class: "rounded-lg border border-warning/40 bg-warning/5 p-3 space-y-2",
                                        div { class: "flex items-center justify-between gap-2 flex-wrap",
                                            div { class: "flex items-center gap-2",
                                                span { class: "text-sm font-semibold", "关联授权审批单" }
                                                span { class: "{authorization_status_badge(status)}", "{authorization_status_text(status)}" }
                                            }
                                            div { class: "flex items-center gap-1",
                                                if status == AuthorizationStatusDto::Pending {
                                                    button {
                                                        class: "btn hud-btn btn-success btn-xs",
                                                        disabled: is_deciding,
                                                        onclick: move |_| on_decide.call((approve_id.clone(), true)),
                                                        "批准"
                                                    }
                                                    button {
                                                        class: "btn hud-btn btn-error btn-xs",
                                                        disabled: is_deciding,
                                                        onclick: move |_| on_decide.call((reject_id.clone(), false)),
                                                        "拒绝"
                                                    }
                                                } else if authorization_revocable(status) {
                                                    button {
                                                        class: "btn hud-btn btn-warning btn-xs",
                                                        disabled: is_deciding,
                                                        onclick: move |_| {
                                                            revoke_target.set(Some(revoke_id.clone()));
                                                            show_revoke_confirm.set(true);
                                                        },
                                                        "撤销授权"
                                                    }
                                                }
                                            }
                                        }
                                        div { class: "grid grid-cols-2 gap-2 text-xs",
                                            div {
                                                span { class: "text-base-content/60", "授权单: " }
                                                span { class: "font-mono break-all", "{auth_id}" }
                                            }
                                            div { span { class: "text-base-content/60", "命中规则: " }, "{rule}" }
                                            div { span { class: "text-base-content/60", "工具: " }, span { class: "font-mono", "{tool_id}" } }
                                            div { span { class: "text-base-content/60", "建单时间: " }, span { class: "font-mono", "{crate::utils::format_datetime(requested_at)}" } }
                                            div { class: "col-span-2",
                                                span { class: "text-base-content/60", "受限命令签名: " }
                                                span { class: "font-mono break-all", "{signature}" }
                                            }
                                            if status == AuthorizationStatusDto::Active {
                                                div {
                                                    span { class: "text-base-content/60", "剩余次数: " }
                                                    span { class: "font-mono", "{auth_remaining_text(remaining)}" }
                                                }
                                                div {
                                                    span { class: "text-base-content/60", "过期时间: " }
                                                    span { class: "font-mono", "{crate::utils::format_timestamp_opt(expires_at)}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        div {
                            div { class: "text-sm text-base-content/60 mb-1", "Input" }
                            pre { class: "font-mono text-xs bg-base-200 p-2 rounded max-h-48 overflow-auto",
                                style: "white-space: pre-wrap; word-break: break-word;",
                                "{serde_json::to_string_pretty(&e.input).unwrap_or_default()}" }
                        }
                        if let Some(out) = &e.output {
                            div {
                                div { class: "text-sm text-base-content/60 mb-1", "Output" }
                                pre { class: "font-mono text-xs bg-base-200 p-2 rounded max-h-48 overflow-auto",
                                    style: "white-space: pre-wrap; word-break: break-word;",
                                    "{serde_json::to_string_pretty(out).unwrap_or_default()}" }
                            }
                        }
                        if let Some(err) = &e.error {
                            div {
                                div { class: "text-sm text-error mb-1", "Error" }
                                pre { class: "font-mono text-xs bg-error/10 p-2 rounded",
                                    style: "white-space: pre-wrap; word-break: break-word;",
                                    "{err}" }
                            }
                        }
                    }
                } else {
                    EmptyState { icon: "📭".to_string(), message: "无数据".to_string() }
                }
            }

            // 撤销二次确认
            ConfirmDialog {
                show: show_revoke_confirm(),
                title: "确认撤销授权".to_string(),
                message: format!(
                    "确定撤销授权单 {}？撤销即时生效，该命令签名的拦截将恢复默认裁决。",
                    revoke_target().as_deref().unwrap_or("-")
                ),
                confirm_text: Some("撤销".to_string()),
                confirm_class: Some("btn hud-btn btn-error".to_string()),
                on_confirm: move |_| {
                    show_revoke_confirm.set(false);
                    if let Some(id) = revoke_target() {
                        do_revoke(id);
                    }
                },
                on_cancel: move |_| show_revoke_confirm.set(false),
            }
        }
    }
}

// ==================== 纯函数辅助（不依赖组件作用域） ====================

/// 读取筛选信号构造调用记录查询参数（空串 = 不过滤，与后端「省略即不过滤」语义对齐）
fn build_trace_query(
    call_id: String,
    agent_id: String,
    tool_id: String,
    limit: String,
) -> QueryToolCallEntriesRequest {
    fn non_empty(s: String) -> Option<String> {
        let t = s.trim().to_string();
        (!t.is_empty()).then_some(t)
    }
    QueryToolCallEntriesRequest {
        call_id: non_empty(call_id),
        agent_id: non_empty(agent_id),
        project_id: None,
        task_id: None,
        tool_id: non_empty(tool_id),
        status: None,
        started_after: None,
        started_before: None,
        limit: limit.trim().parse::<usize>().ok(),
    }
}

/// 按 `call_id` 归集授权单，供调用记录行 / 详情卡片 O(1) 反查。
///
/// 同一调用正常只产生一张单（同签名重复建单由 domain 拒绝）；万一重复，
/// **待审批态优先**——那张才是当前需要动作的。
fn index_auths_by_call(
    auths: &[AuthorizationDetailDto],
) -> HashMap<String, AuthorizationDetailDto> {
    let mut map: HashMap<String, AuthorizationDetailDto> = HashMap::new();
    for a in auths {
        let Some(call_id) = a.call_id.as_deref() else {
            continue;
        };
        let keep_existing = map
            .get(call_id)
            .is_some_and(|prev: &AuthorizationDetailDto| {
                prev.status == AuthorizationStatusDto::Pending
            });
        if !keep_existing {
            map.insert(call_id.to_string(), a.clone());
        }
    }
    map
}

/// 审批 / 撤销之后就地把列表内该单状态改掉（免额外往返；完整字段靠「刷新」同步）
fn apply_status_locally(
    mut list: Signal<Vec<AuthorizationDetailDto>>,
    authorization_id: &str,
    status: AuthorizationStatusDto,
    grant_id: Option<String>,
) {
    let mut items = list.write();
    if let Some(item) = items
        .iter_mut()
        .find(|a| a.authorization_id == authorization_id)
    {
        item.status = status;
        if grant_id.is_some() {
            item.grant_id = grant_id;
        }
    }
}

/// 调用记录行的「审批」单元格。
///
/// - 命中待审批单 → 快捷「通过 / 拒绝」（审批主路径，不弹确认）
/// - 命中已决策单 → 状态徽章
/// - 无关联（普通成功调用 / 被 Deny 硬拦）→ 占位符
fn approval_cell(
    linked: Option<AuthorizationDetailDto>,
    deciding: bool,
    on_decide: EventHandler<(String, bool)>,
) -> Element {
    let Some(a) = linked else {
        return rsx! { span { class: "text-base-content/40 text-xs", "—" } };
    };
    if a.status != AuthorizationStatusDto::Pending {
        return rsx! {
            span { class: "{authorization_status_badge(a.status)}", "{authorization_status_text(a.status)}" }
        };
    }
    let pass_id = a.authorization_id.clone();
    let reject_id = a.authorization_id.clone();
    rsx! {
        div { class: "flex items-center gap-1",
            button {
                class: "btn hud-btn btn-xs btn-success",
                disabled: deciding,
                onclick: move |_| on_decide.call((pass_id.clone(), true)),
                "通过"
            }
            button {
                class: "btn hud-btn btn-xs btn-error",
                disabled: deciding,
                onclick: move |_| on_decide.call((reject_id.clone(), false)),
                "拒绝"
            }
        }
    }
}

/// 剩余次数文案（None = 授权期内不限次）
fn auth_remaining_text(remaining: Option<u32>) -> String {
    match remaining {
        Some(n) => n.to_string(),
        None => "不限".to_string(),
    }
}
