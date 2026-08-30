//! 工具调用记录查询页 - 列表 + 详情 Modal

use crate::components::hud::HudPanel;
use dioxus::prelude::*;

use crate::api::finance::{get_tool_call_entry, query_tool_call_entries};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{QueryToolCallEntriesRequest, ToolCallEntryDetail, ToolCallStatusDto};

#[component]
pub fn FinanceToolCallEntries() -> Element {
    let toast = use_toast();

    let mut entries = use_signal(Vec::<ToolCallEntryDetail>::new);
    let mut loading = use_signal(|| true);
    let mut query_call_id = use_signal(String::new);
    let mut query_agent_id = use_signal(String::new);
    let mut query_tool_id = use_signal(String::new);
    let mut query_limit = use_signal(|| "50".to_string());

    // 详情 Modal
    let mut show_detail_modal = use_signal(|| false);
    let mut selected_entry = use_signal(|| Option::<ToolCallEntryDetail>::None);
    let mut detail_loading = use_signal(|| false);

    let mut do_search = move || {
        loading.set(true);
        let params = QueryToolCallEntriesRequest {
            call_id: if query_call_id().trim().is_empty() {
                None
            } else {
                Some(query_call_id().trim().to_string())
            },
            agent_id: if query_agent_id().trim().is_empty() {
                None
            } else {
                Some(query_agent_id().trim().to_string())
            },
            project_id: None,
            task_id: None,
            tool_id: if query_tool_id().trim().is_empty() {
                None
            } else {
                Some(query_tool_id().trim().to_string())
            },
            status: None,
            started_after: None,
            started_before: None,
            limit: query_limit().trim().parse::<usize>().ok(),
        };
        spawn(async move {
            match query_tool_call_entries(&params).await {
                Ok(list) => entries.set(list),
                Err(e) => toast.error(format!("查询失败: {}", e)),
            }
            loading.set(false);
        });
    };

    // 初始加载 - 用 use_effect 触发一次
    use_effect(move || {
        let params = QueryToolCallEntriesRequest {
            call_id: None,
            agent_id: None,
            project_id: None,
            task_id: None,
            tool_id: None,
            status: None,
            started_after: None,
            started_before: None,
            limit: Some(50),
        };
        loading.set(true);
        spawn(async move {
            match query_tool_call_entries(&params).await {
                Ok(list) => entries.set(list),
                Err(e) => toast.error(format!("加载失败: {}", e)),
            }
            loading.set(false);
        });
    });

    let on_search = move |_| {
        do_search();
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

    let entries_list = entries.read().clone();
    let selected = selected_entry.read().clone();

    rsx! {
        AppLayout {
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
                                    th { "操作" }
                                }}
                                tbody {
                                    for e in entries_list.iter() {
                                        {
                                            let call_id = e.call_id.clone();
                                            let tool_name = e.tool_name.clone();
                                            let agent_id = e.agent_id.clone();
                                            let status = e.status;
                                            let duration_ms = e.duration_ms;
                                            let started_at = e.started_at;
                                            rsx! {
                                                tr { key: "{call_id}",
                                                    td { class: "font-mono text-xs truncate max-w-xs", title: "{call_id}", "{call_id}" }
                                                    td { "{tool_name}" }
                                                    td { class: "font-mono text-xs", "{agent_id.as_deref().unwrap_or(\"-\")}" }
                                                    td { span { class: "{status_badge_class(status)}", "{status_text(status)}" } }
                                                    td { class: "font-mono", "{duration_ms}ms" }
                                                    td { class: "font-mono text-xs", "{crate::utils::format_datetime(started_at as i64)}" }
                                                    td { button { class: "btn hud-btn btn-ghost btn-xs", onclick: move |_| on_click_entry(call_id.clone()), "详情" } }
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

            // 详情 Modal
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
                            div { span { class: "text-base-content/60", "状态: " }, "{status_text(e.status)}" }
                            div { span { class: "text-base-content/60", "耗时: " }, span { class: "font-mono", "{e.duration_ms}ms" } }
                            div { span { class: "text-base-content/60", "Agent: " }, span { class: "font-mono", "{e.agent_id.as_deref().unwrap_or(\"-\")}" } }
                            div { span { class: "text-base-content/60", "Task: " }, span { class: "font-mono", "{e.task_id.as_deref().unwrap_or(\"-\")}" } }
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
        }
    }
}

fn status_text(s: ToolCallStatusDto) -> &'static str {
    match s {
        ToolCallStatusDto::Started => "进行中",
        ToolCallStatusDto::Completed => "成功",
        ToolCallStatusDto::Failed => "失败",
    }
}

fn status_badge_class(s: ToolCallStatusDto) -> &'static str {
    match s {
        ToolCallStatusDto::Completed => "badge hud-badge badge-success",
        ToolCallStatusDto::Failed => "badge hud-badge badge-error",
        ToolCallStatusDto::Started => "badge hud-badge badge-info",
    }
}
