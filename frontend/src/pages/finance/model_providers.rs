//! 模型提供商管理

use dioxus::prelude::*;

use crate::api::finance::{call_model_provider, create_model_provider, delete_model_provider, list_model_providers, test_model_provider_connection};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::store::toast::use_toast;
use common::api::{CreateModelProviderRequest, ListModelProvidersResponseItem};
use common::enums::{ModelCapability, ProviderType};
use dioxus_router::Link;

#[component]
pub fn FinanceModelProviders() -> Element {
    let mut providers = use_signal(Vec::<ListModelProvidersResponseItem>::new);
    let mut loading = use_signal(|| true);
    let toast = use_toast();
    let mut show_modal = use_signal(|| false);

    // 表单状态
    let mut name = use_signal(String::new);
    let mut provider_type = use_signal(|| ProviderType::OpenAI);
    let mut model_name = use_signal(String::new);
    let mut api_key = use_signal(String::new);
    let mut base_url = use_signal(|| String::new());
    let mut description = use_signal(String::new);
    let mut creating = use_signal(|| false);

    // 调用测试状态
    let mut show_test_modal = use_signal(|| false);
    let mut test_provider_id = use_signal(String::new);
    let mut test_prompt = use_signal(|| "你好，请介绍一下自己".to_string());
    let mut test_response = use_signal(String::new);
    let mut test_loading = use_signal(|| false);

    use_effect(move || {
        loading.set(true);
        spawn(async move {
            match list_model_providers().await {
                Ok(list) => providers.set(list.providers),
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    });

    let handle_create = move |_| {
        spawn(async move {
            if name().is_empty() || model_name().is_empty() {
                toast.error("名称和模型名称不能为空");
                return;
            }
            creating.set(true);
            let req = CreateModelProviderRequest {
                name: name(),
                provider_type: provider_type(),
                capability: ModelCapability::Agent,
                model_name: model_name(),
                api_key: api_key(),
                base_url: if base_url().is_empty() { None } else { Some(base_url()) },
                description: if description().is_empty() { None } else { Some(description()) },
            };
            match create_model_provider(req).await {
                Ok(resp) => {
                    show_modal.set(false);
                    name.set(String::new());
                    model_name.set(String::new());
                    api_key.set(String::new());
                    base_url.set(String::new());
                    description.set(String::new());
                    toast.success("创建成功");
                    // Reload
                    match list_model_providers().await {
                        Ok(list) => providers.set(list.providers),
                        Err(e) => toast.error(&e),
                    }
                    // 自动测试连接
                    spawn(async move {
                        match test_model_provider_connection(&resp.id).await {
                            Ok(_) => toast.success("创建成功，连接测试通过"),
                            Err(e) => toast.error(&format!("创建成功但测试失败: {}", e)),
                        }
                    });
                }
                Err(e) => toast.error(&format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    let handle_test_send = move |_| {
        spawn(async move {
            test_loading.set(true);
            match call_model_provider(&test_provider_id(), &test_prompt()).await {
                Ok(resp) => test_response.set(resp.result),
                Err(e) => toast.error(&format!("调用测试失败: {}", e)),
            }
            test_loading.set(false);
        });
    };

    let providers_list = providers.read().clone();

    let provider_type_str = provider_type().to_string();

    rsx! {
        div { class: "card",
            div { class: "card-header",
                h2 { class: "card-title", "模型提供商管理" }
                button { class: "btn btn-accent", onclick: move |_| show_modal.set(true), "+ 添加提供商" }
            }

            if loading() {
                Loading {}
            } else if providers_list.is_empty() {
                EmptyState { icon: "🧠".to_string(), message: "暂无模型提供商".to_string() }
            } else {
                table { class: "table",
                    thead { tr {
                        th { "名称" }
                        th { "类型" }
                        th { "模型" }
                        th { "操作" }
                    }}
                    tbody {
                        for p in providers_list.iter() {
                            {
                                let id = p.id.clone();
                                let pname = p.name.clone();
                                let pmodel = p.model_name.clone();
                                let ptype_str = p.provider_type.to_string();
                                let id_delete = id.clone();
                                let id_test = id.clone();
                                let id_detail = id.clone();
                                rsx! {
                                    tr { key: "{id}",
                                        td { class: "detail-table-value-bold",
                                            Link { to: crate::pages::Route::FinanceModelProviderDetail { id: id_detail.clone() }, "{pname}" }
                                        }
                                        td { span { class: "badge badge-info", "{ptype_str}" } }
                                        td { class: "text-mono", "{pmodel}" }
                                        td {
                                            button { class: "btn btn-sm btn-accent",
                                                onclick: move |_| {
                                                    test_provider_id.set(id_test.clone());
                                                    test_prompt.set("你好，请介绍一下自己".to_string());
                                                    test_response.set(String::new());
                                                    show_test_modal.set(true);
                                                },
                                                "调用测试"
                                            }
                                            button { class: "btn btn-danger btn-sm",
                                                onclick: move |_| {
                                                    let id_delete = id_delete.clone();
                                                    spawn(async move {
                                                        if let Err(e) = delete_model_provider(&id_delete).await {
                                                            toast.error(&format!("删除失败: {}", e));
                                                        } else {
                                                            toast.success("已删除");
                                                            match list_model_providers().await {
                                                                Ok(list) => providers.set(list.providers),
                                                                Err(e) => toast.error(&e),
                                                            }
                                                        }
                                                    });
                                                },
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

        Modal {
            title: "添加模型提供商".to_string(),
            show: show_modal(),
            on_close: move |_| show_modal.set(false),
            footer: rsx! {
                button { class: "btn btn-ghost", onclick: move |_| show_modal.set(false), "取消" }
                button { class: "btn btn-accent", disabled: creating(), onclick: handle_create,
                    if creating() { "创建中..." } else { "创建" }
                }
            },
            div {
                div { class: "form-group",
                    label { class: "form-label", "名称 *" }
                    input { class: "form-input", value: "{name}",
                        oninput: move |e| name.set(e.value()), placeholder: "如：OpenAI 主账号" }
                }
                div { class: "form-group",
                    label { class: "form-label", "类型" }
                    select { class: "form-select", value: "{provider_type_str}",
                        onchange: move |e| {
                            provider_type.set(match e.value().as_str() {
                                "deepseek" => ProviderType::DeepSeek,
                                "qwen" => ProviderType::Qwen,
                                "doubao" => ProviderType::Doubao,
                                "ollama" => ProviderType::Ollama,
                                "custom" => ProviderType::Custom,
                                _ => ProviderType::OpenAI,
                            });
                        },
                        option { value: "openai", "OpenAI" }
                        option { value: "custom", "OpenAI 兼容" }
                        option { value: "deepseek", "DeepSeek" }
                        option { value: "doubao", "豆包" }
                        option { value: "qwen", "通义千问" }
                        option { value: "ollama", "Ollama" }
                    }
                }
                div { class: "form-group",
                    label { class: "form-label", "模型名称 *" }
                    input { class: "form-input", value: "{model_name}",
                        oninput: move |e| model_name.set(e.value()), placeholder: "如：gpt-4o" }
                }
                div { class: "form-group",
                    label { class: "form-label", "API Key" }
                    input { class: "form-input", r#type: "password", value: "{api_key}",
                        oninput: move |e| api_key.set(e.value()), placeholder: "sk-..." }
                }
                div { class: "form-group",
                    label { class: "form-label", "Base URL" }
                    input { class: "form-input", value: "{base_url}",
                        oninput: move |e| base_url.set(e.value()), placeholder: "https://api.openai.com/v1" }
                }
                div { class: "form-group",
                    label { class: "form-label", "描述" }
                    input { class: "form-input", value: "{description}",
                        oninput: move |e| description.set(e.value()), placeholder: "可选" }
                }
            }
        }

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
    }
}
