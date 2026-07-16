//! 模型提供商详情页

use crate::api::finance::{call_model_provider, delete_model_provider, get_model_provider, switch_embedding_provider, test_model_provider_connection, toggle_model_provider};
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

    // Embedding Provider 切换确认对话框
    let mut show_switch_modal = use_signal(|| false);
    let mut switch_provider_name = use_signal(String::new);
    let mut switch_loading = use_signal(|| false);

    let id_for_effect = id.clone();
    use_effect(move || {
        loading.set(true);
        let id = id_for_effect.clone();
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

    // 切换确认需要克隆 id 以满足 FnMut 要求
    let switch_id = id.clone();
    let handle_switch_confirm = move |_| {
        let reload_id = switch_id.clone();
        // 先克隆 provider 数据，避免借用冲突
        let provider_id = provider_data.read().clone().map(|p| p.id);
        spawn(async move {
            switch_loading.set(true);
            if let Some(pid) = provider_id {
                match switch_embedding_provider(&pid).await {
                    Ok(resp) => {
                        show_switch_modal.set(false);
                        toast.success(&format!("Embedding Provider 已切换为 {}，向量索引重建完成", resp.name));
                        // Reload
                        let stats_options = StatsOptions {
                            with_stats: false,
                            with_model_call_stats: true,
                            stats_interval: None,
                        };
                        match get_model_provider(&reload_id, Some(&stats_options)).await {
                            Ok(provider) => provider_data.set(Some(provider)),
                            Err(e) => toast.error(&e),
                        }
                    }
                    Err(e) => {
                        show_switch_modal.set(false);
                        toast.error(&format!("切换失败: {}", e));
                    }
                }
            }
            switch_loading.set(false);
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
                            {
                                let is_embedding = p.capability.is_embedding();
                                let is_enabled = p.status == 1;
                                let provider_id = p.id.clone();
                                let provider_name = p.name.clone();
                                let reload_id = id.clone();
                                rsx! {
                                    if is_enabled {
                                        button { class: "btn btn-secondary btn-sm",
                                            onclick: {
                                                let pid = provider_id.clone();
                                                let rid = reload_id.clone();
                                                move |_| {
                                                    let pid = pid.clone();
                                                    let rid = rid.clone();
                                                    spawn(async move {
                                                        match toggle_model_provider(&pid, 0).await {
                                                            Ok(()) => {
                                                                toast.success("已禁用");
                                                                let stats_options = StatsOptions {
                                                                    with_stats: false,
                                                                    with_model_call_stats: true,
                                                                    stats_interval: None,
                                                                };
                                                                match get_model_provider(&rid, Some(&stats_options)).await {
                                                                    Ok(provider) => provider_data.set(Some(provider)),
                                                                    Err(e) => toast.error(&e),
                                                                }
                                                            }
                                                            Err(e) => toast.error(&format!("禁用失败: {}", e)),
                                                        }
                                                    });
                                                }
                                            },
                                            "禁用"
                                        }
                                    } else {
                                        button { class: "btn btn-accent btn-sm",
                                            onclick: {
                                                let pid = provider_id.clone();
                                                let pname = provider_name.clone();
                                                let rid = reload_id.clone();
                                                move |_| {
                                                    let pid = pid.clone();
                                                    let pname = pname.clone();
                                                    let rid = rid.clone();
                                                    let is_emb = is_embedding;
                                                    spawn(async move {
                                                        if is_emb {
                                                            match toggle_model_provider(&pid, 1).await {
                                                                Ok(()) => {
                                                                    toast.success("已启用");
                                                                    let stats_options = StatsOptions {
                                                                        with_stats: false,
                                                                        with_model_call_stats: true,
                                                                        stats_interval: None,
                                                                    };
                                                                    match get_model_provider(&rid, Some(&stats_options)).await {
                                                                        Ok(provider) => provider_data.set(Some(provider)),
                                                                        Err(e) => toast.error(&e),
                                                                    }
                                                                }
                                                                Err(e) => {
                                                                    if e.error_code.as_deref() == Some("embedding_provider_switch_required") {
                                                                        switch_provider_name.set(pname);
                                                                        show_switch_modal.set(true);
                                                                    } else {
                                                                        toast.error(&format!("启用失败: {}", e));
                                                                    }
                                                                }
                                                            }
                                                        } else {
                                                            match toggle_model_provider(&pid, 1).await {
                                                                Ok(()) => {
                                                                    toast.success("已启用");
                                                                    let stats_options = StatsOptions {
                                                                        with_stats: false,
                                                                        with_model_call_stats: true,
                                                                        stats_interval: None,
                                                                    };
                                                                    match get_model_provider(&rid, Some(&stats_options)).await {
                                                                        Ok(provider) => provider_data.set(Some(provider)),
                                                                        Err(e) => toast.error(&e),
                                                                    }
                                                                }
                                                                Err(e) => {
                                                                    toast.error(&format!("启用失败: {}", e));
                                                                }
                                                            }
                                                        }
                                                    });
                                                }
                                            },
                                            "启用"
                                        }
                                    }
                                    button { class: "btn btn-secondary btn-sm",
                                        onclick: {
                                            let pid = provider_id.clone();
                                            move |_| {
                                                let pid = pid.clone();
                                                spawn(async move {
                                                    match test_model_provider_connection(&pid).await {
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
                                            let pid = provider_id.clone();
                                            move |_| {
                                                let pid = pid.clone();
                                                spawn(async move {
                                                    if let Err(e) = delete_model_provider(&pid).await {
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
                            div { class: "detail-section",
                                label { class: "form-label", "能力" }
                                div { class: "detail-value",
                                    if p.capability.is_embedding() {
                                        span { class: "badge badge-warning", "embedding" }
                                    } else {
                                        span { class: "badge badge-success", "agent" }
                                    }
                                }
                            }
                            div { class: "detail-section",
                                label { class: "form-label", "状态" }
                                div { class: "detail-value",
                                    if p.status == 1 {
                                        span { class: "badge badge-success", "启用" }
                                    } else {
                                        span { class: "badge badge-secondary", "禁用" }
                                    }
                                }
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

                // Embedding Provider 切换确认对话框
                Modal {
                    title: "切换 Embedding Provider".to_string(),
                    show: show_switch_modal(),
                    on_close: move |_| show_switch_modal.set(false),
                    footer: rsx! {
                        button { class: "btn btn-ghost", onclick: move |_| show_switch_modal.set(false), "取消" }
                        button { class: "btn btn-warning", disabled: switch_loading(), onclick: handle_switch_confirm,
                            if switch_loading() { "切换中..." } else { "确认切换" }
                        }
                    },
                    div {
                        div { class: "form-group",
                            p { style: "margin-bottom: 12px; line-height: 1.6;",
                                "当前已有启用的 Embedding Provider，切换到 "
                                strong { "{switch_provider_name}" }
                                " 将会："
                            }
                            ul { style: "padding-left: 20px; line-height: 1.8; color: var(--text-secondary);",
                                li { "禁用当前的 Embedding Provider" }
                                li { "启用新的 Embedding Provider" }
                                li { "使用新的 Embedding 模型重建所有向量索引" }
                            }
                            p { style: "margin-top: 12px; color: var(--color-warning); font-weight: 500;",
                                "⚠️ 重建向量索引可能需要较长时间，期间搜索功能将受影响"
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
