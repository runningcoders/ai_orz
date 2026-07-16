//! 模型提供商详情页

use crate::api::finance::{call_model_provider, delete_model_provider, get_model_provider, test_model_provider_connection};
use crate::api::StatsOptions;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::components::stats::ModelProviderStatsPanel;
use crate::store::toast::use_toast;
use common::api::GetModelProviderResponse;
use dioxus::prelude::*;
use dioxus_router::Link;

#[component]
pub fn FinanceModelProviderDetail(id: String) -> Element {
    let mut provider_data = use_signal(|| None::<GetModelProviderResponse>);
    let mut loading = use_signal(|| true);
    let toast = use_toast();

    // 调用测试弹窗
    let mut show_test_modal = use_signal(|| false);
    let mut test_prompt = use_signal(|| "你好，请介绍一下自己".to_string());
    let mut test_response = use_signal(String::new);
    let mut test_loading = use_signal(|| false);

    use_effect(move || {
        loading.set(true);
        let id = id.clone();
        spawn(async move {
            let stats_options = StatsOptions {
                with_stats: false,
                with_model_call_stats: true,
                stats_interval: None,
            };
            match get_model_provider(&id, Some(&stats_options)).await {
                Ok(provider) => provider_data.set(Some(provider)),
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    });

    let handle_test_send = move |_| {
        spawn(async move {
            test_loading.set(true);
            if let Some(p) = provider_data.read().clone() {
                match call_model_provider(&p.id, &test_prompt()).await {
                    Ok(resp) => test_response.set(resp.result),
                    Err(e) => toast.error(&format!("调用测试失败: {}", e)),
                }
            }
            test_loading.set(false);
        });
    };

    rsx! {
        div { class: "page-container",
            // 页面头部
            div { class: "page-header",
                h1 { class: "page-title", "模型提供商详情" }
                Link { class: "btn btn-ghost", to: crate::pages::Route::FinanceModelProviders {},
                    "← 返回列表"
                }
            }

            if loading() {
                Loading {}
            } else if let Some(p) = provider_data.read().clone() {
                // 基本信息卡片
                div { class: "card",
                    div { class: "card-header",
                        h2 { class: "card-title", "{p.name}" }
                        div { class: "flex gap-2",
                            button { class: "btn btn-accent btn-sm",
                                onclick: {
                                    let id = p.id.clone();
                                    move |_| {
                                        let id = id.clone();
                                        spawn(async move {
                                            match test_model_provider_connection(&id).await {
                                                Ok(_) => toast.success("连接测试通过"),
                                                Err(e) => toast.error(&format!("连接测试失败: {}", e)),
                                            }
                                        });
                                    }
                                },
                                "测试连接"
                            }
                            button { class: "btn btn-secondary btn-sm",
                                onclick: move |_| {
                                    test_prompt.set("你好，请介绍一下自己".to_string());
                                    test_response.set(String::new());
                                    show_test_modal.set(true);
                                },
                                "调用测试"
                            }
                            button { class: "btn btn-danger btn-sm",
                                onclick: {
                                    let id = p.id.clone();
                                    move |_| {
                                        let id = id.clone();
                                        spawn(async move {
                                            if let Err(e) = delete_model_provider(&id).await {
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
                    div { class: "detail-card-body",
                        div { class: "detail-grid",
                            div { class: "detail-section",
                                label { class: "form-label", "模型名称" }
                                div { class: "detail-value text-mono", "{p.model_name}" }
                            }
                            div { class: "detail-section",
                                label { class: "form-label", "类型" }
                                div { class: "detail-value", span { class: "badge badge-info", "{p.provider_type}" } }
                            }
                            if let Some(base_url) = &p.base_url {
                                div { class: "detail-section",
                                    label { class: "form-label", "Base URL" }
                                    div { class: "detail-value text-mono", "{base_url}" }
                                }
                            }
                            if let Some(description) = &p.description {
                                div { class: "detail-section",
                                    label { class: "form-label", "描述" }
                                    div { class: "detail-text", "{description}" }
                                }
                            }
                            div { class: "detail-section",
                                label { class: "form-label", "提供商 ID" }
                                div { class: "text-mono text-sm", "{p.id}" }
                            }
                        }
                    }
                }

                // 统计面板
                if p.stats.is_some() {
                    ModelProviderStatsPanel { stats: p.stats.clone() }
                }

                // 调用测试弹窗
                Modal {
                    title: "调用测试".to_string(),
                    show: show_test_modal(),
                    on_close: move |_| show_test_modal.set(false),
                    footer: rsx! {
                        button { class: "btn btn-ghost", onclick: move |_| show_test_modal.set(false), "关闭" }
                        button { class: "btn btn-accent", disabled: test_loading(), onclick: handle_test_send,
                            if test_loading() { "发送中..." } else { "发送" }
                        }
                    },
                    div {
                        div { class: "form-group",
                            label { class: "form-label", "Prompt" }
                            textarea { class: "form-textarea", rows: "4", value: "{test_prompt}",
                                oninput: move |e| test_prompt.set(e.value()) }
                        }
                        if !test_response().is_empty() {
                            div { class: "form-group",
                                label { class: "form-label", "响应" }
                                textarea { class: "form-textarea", rows: "6", readonly: true, value: "{test_response}" }
                            }
                        }
                    }
                }
            } else {
                EmptyState { icon: "🧠".to_string(), message: "模型提供商不存在或已被删除".to_string() }
            }
        }
    }
}
