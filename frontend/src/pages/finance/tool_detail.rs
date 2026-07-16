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
        div { class: "page-container",
            // 页面头部
            div { class: "page-header",
                h1 { class: "page-title", "工具详情" }
                Link { class: "btn btn-ghost", to: crate::pages::Route::FinanceTools {},
                    "← 返回列表"
                }
            }

            if loading() {
                Loading {}
            } else if let Some(t) = tool_data.read().clone() {
                // 基本信息卡片
                div { class: "card",
                    div { class: "card-header",
                        h2 { class: "card-title", "{t.name}" }
                        div { class: "flex gap-2",
                            if t.enabled {
                                button { class: "btn btn-secondary btn-sm",
                                    onclick: {
                                        let id = t.id.clone();
                                        move |_| {
                                            let id = id.clone();
                                            spawn(async move {
                                                if let Err(e) = update_tool_status(&id, 0).await {
                                                    toast.error(&e);
                                                } else {
                                                    toast.success("已禁用");
                                                    // 刷新数据
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
                                button { class: "btn btn-accent btn-sm",
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
                            button { class: "btn btn-danger btn-sm",
                                onclick: {
                                    let id = t.id.clone();
                                    move |_| {
                                        let id = id.clone();
                                        spawn(async move {
                                            if let Err(e) = delete_tool(&id).await {
                                                toast.error(&format!("删除失败: {}", e));
                                            } else {
                                                toast.success("已删除");
                                                // 返回列表页
                                            }
                                        });
                                    }
                                },
                                "删除"
                            }
                        }
                    }
                    div { class: "detail-card-body",
                        div { class: "detail-grid",
                            div { class: "detail-section",
                                label { class: "form-label", "描述" }
                                div { class: "detail-text", "{t.description}" }
                            }
                            div { class: "detail-section",
                                label { class: "form-label", "协议" }
                                div { class: "detail-value", span { class: "badge badge-neutral", "{t.protocol}" } }
                            }
                            div { class: "detail-section",
                                label { class: "form-label", "状态" }
                                div { class: "detail-value",
                                    if t.enabled {
                                        span { class: "badge badge-success", "启用" }
                                    } else {
                                        span { class: "badge badge-error", "禁用" }
                                    }
                                }
                            }
                            div { class: "detail-section",
                                label { class: "form-label", "控制模式" }
                                div { class: "detail-value", "{t.control_mode}" }
                            }
                            if !t.tags.is_empty() {
                                div { class: "detail-section",
                                    label { class: "form-label", "标签" }
                                    div { class: "tag-list",
                                        for tag in t.tags.iter() {
                                            span { class: "badge badge-neutral tag-item", "{tag}" }
                                        }
                                    }
                                }
                            }
                            div { class: "detail-section",
                                label { class: "form-label", "工具 ID" }
                                div { class: "text-mono text-sm", "{t.id}" }
                            }
                        }
                    }
                }

                // 统计面板
                if t.stats.is_some() {
                    ToolStatsPanel { stats: t.stats.clone() }
                }
            } else {
                EmptyState { icon: "🔧".to_string(), message: "工具不存在或已被删除".to_string() }
            }
        }
    }
}
