//! Tool 详情页

use crate::api::finance::{delete_tool, get_tool, update_tool_status};
use crate::api::StatsOptions;
use crate::components::state::{EmptyState, Loading};
use crate::components::stats::ToolStatsPanel;
use crate::store::toast::use_toast;
use common::api::GetToolResponse;
use dioxus::prelude::*;
use dioxus_router::Link;

#[component]
pub fn FinanceToolDetail(id: String) -> Element {
    let mut tool_data = use_signal(|| None::<GetToolResponse>);
    let mut loading = use_signal(|| true);
    let toast = use_toast();

    use_effect(move || {
        loading.set(true);
        let id = id.clone();
        spawn(async move {
            let stats_options = StatsOptions {
                with_stats: true,
                with_model_call_stats: false,
                stats_interval: None,
            };
            match get_tool(&id, Some(&stats_options)).await {
                Ok(tool) => tool_data.set(Some(tool)),
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    });

    rsx! {
        div {
            div { class: "mb-6 flex items-center justify-between",
                div {
                    h1 { class: "text-2xl font-bold", "工具详情" }
                }
                Link { class: "btn btn-ghost", to: crate::pages::Route::FinanceTools {},
                    "← 返回列表"
                }
            }

            if loading() {
                Loading {}
            } else if let Some(t) = tool_data.read().clone() {
                div { class: "card bg-base-100 shadow-md",
                    div { class: "card-body",
                        div { class: "flex justify-between items-center mb-4",
                            h2 { class: "card-title", "{t.name}" }
                            div { class: "flex gap-2",
                                if t.enabled {
                                    button { class: "btn btn-outline btn-sm",
                                        onclick: {
                                            let id = t.id.clone();
                                            move |_| {
                                                let id = id.clone();
                                                spawn(async move {
                                                    if let Err(e) = update_tool_status(&id, 0).await {
                                                        toast.error(&e);
                                                    } else {
                                                        toast.success("已禁用");
                                                        let stats_options = StatsOptions {
                                                            with_stats: true,
                                                            with_model_call_stats: false,
                                                            stats_interval: None,
                                                        };
                                                        if let Ok(tool) = get_tool(&id, Some(&stats_options)).await {
                                                            tool_data.set(Some(tool));
                                                        }
                                                    }
                                                });
                                            }
                                        },
                                        "禁用"
                                    }
                                } else {
                                    button { class: "btn btn-primary btn-sm",
                                        onclick: {
                                            let id = t.id.clone();
                                            move |_| {
                                                let id = id.clone();
                                                spawn(async move {
                                                    if let Err(e) = update_tool_status(&id, 1).await {
                                                        toast.error(&e);
                                                    } else {
                                                        toast.success("已启用");
                                                        let stats_options = StatsOptions {
                                                            with_stats: true,
                                                            with_model_call_stats: false,
                                                            stats_interval: None,
                                                        };
                                                        if let Ok(tool) = get_tool(&id, Some(&stats_options)).await {
                                                            tool_data.set(Some(tool));
                                                        }
                                                    }
                                                });
                                            }
                                        },
                                        "启用"
                                    }
                                }
                                button { class: "btn btn-error btn-sm",
                                    onclick: {
                                        let id = t.id.clone();
                                        move |_| {
                                            let id = id.clone();
                                            spawn(async move {
                                                if let Err(e) = delete_tool(&id).await {
                                                    toast.error(&format!("删除失败: {}", e));
                                                } else {
                                                    toast.success("已删除");
                                                }
                                            });
                                        }
                                    },
                                    "删除"
                                }
                            }
                        }
                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                            div { class: "md:col-span-2",
                                label { class: "label",
                                    span { class: "label-text font-medium", "描述" }
                                }
                                div { "{t.description}" }
                            }
                            div {
                                label { class: "label",
                                    span { class: "label-text font-medium", "协议" }
                                }
                                div { span { class: "badge badge-neutral", "{t.protocol}" } }
                            }
                            div {
                                label { class: "label",
                                    span { class: "label-text font-medium", "状态" }
                                }
                                div {
                                    if t.enabled {
                                        span { class: "badge badge-success", "启用" }
                                    } else {
                                        span { class: "badge badge-error", "禁用" }
                                    }
                                }
                            }
                            div {
                                label { class: "label",
                                    span { class: "label-text font-medium", "控制模式" }
                                }
                                div { "{t.control_mode}" }
                            }
                            if !t.tags.is_empty() {
                                div { class: "md:col-span-2",
                                    label { class: "label",
                                        span { class: "label-text font-medium", "标签" }
                                    }
                                    div { class: "flex flex-wrap gap-2",
                                        for tag in t.tags.iter() {
                                            span { class: "badge badge-neutral", "{tag}" }
                                        }
                                    }
                                }
                            }
                            div {
                                label { class: "label",
                                    span { class: "label-text font-medium", "工具 ID" }
                                }
                                div { class: "font-mono text-sm", "{t.id}" }
                            }
                        }
                    }
                }

                if t.stats.is_some() {
                    ToolStatsPanel { stats: t.stats.clone() }
                }
            } else {
                EmptyState { icon: "🔧".to_string(), message: "工具不存在或已被删除".to_string() }
            }
        }
    }
}
