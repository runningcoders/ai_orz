//! 模型提供商管理

use dioxus::prelude::*;

use crate::api::finance::{create_model_provider, delete_model_provider, list_model_providers, test_model_provider_connection};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, ErrorAlert, Loading, SuccessAlert};
use common::api::{CreateModelProviderRequest, ListModelProvidersResponseItem};

#[component]
pub fn FinanceModelProviders() -> Element {
    let mut providers = use_signal(Vec::<ListModelProvidersResponseItem>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut success = use_signal(String::new);
    let mut show_modal = use_signal(|| false);

    // 表单状态
    let mut name = use_signal(String::new);
    let mut provider_type = use_signal("openai".to_string());
    let mut model_name = use_signal(String::new);
    let mut api_key = use_signal(String::new);
    let mut base_url = use_signal(String::new());
    let mut description = use_signal(String::new);
    let mut creating = use_signal(|| false);

    let load = move || {
        loading.set(true);
        spawn(async move {
            match list_model_providers().await {
                Ok(list) => providers.set(list.providers),
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    };

    use_effect(move || { load(); });

    let handle_create = move |_| {
        spawn(async move {
            if name().is_empty() || model_name().is_empty() {
                error.set("名称和模型名称不能为空".to_string());
                return;
            }
            creating.set(true);
            let req = CreateModelProviderRequest {
                name: name(),
                provider_type: provider_type(),
                model_name: model_name(),
                api_key: if api_key().is_empty() { None } else { Some(api_key()) },
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
                    success.set("创建成功".to_string());
                    load();
                    // 自动测试连接
                    spawn(async move {
                        match test_model_provider_connection(&resp.id).await {
                            Ok(_) => success.set("创建成功，连接测试通过".to_string()),
                            Err(e) => error.set(format!("创建成功但测试失败: {}", e)),
                        }
                    });
                }
                Err(e) => error.set(format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    let providers_list = providers.read().clone();

    rsx! {
        div { class: "card",
            ErrorAlert { message: error() }
            SuccessAlert { message: success() }

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
                                rsx! {
                                    tr { key: "{id}",
                                        td { style: "font-weight: 500;", "{p.name}" }
                                        td { span { class: "badge badge-info", "{p.provider_type}" } }
                                        td { class: "text-mono", "{p.model_name}" }
                                        td {
                                            button { class: "btn btn-danger btn-sm",
                                                onclick: move |_| {
                                                    let id = id.clone();
                                                    spawn(async move {
                                                        if let Err(e) = delete_model_provider(&id).await {
                                                            error.set(format!("删除失败: {}", e));
                                                        } else {
                                                            success.set("已删除".to_string());
                                                            load();
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
            div {
                div { class: "form-group",
                    label { class: "form-label", "名称 *" }
                    input { class: "form-input", value: "{name}",
                        oninput: move |e| name.set(e.value()), placeholder: "如：OpenAI 主账号" }
                }
                div { class: "form-group",
                    label { class: "form-label", "类型" }
                    select { class: "form-select", value: "{provider_type}",
                        onchange: move |e| provider_type.set(e.value()),
                        option { value: "openai", "OpenAI" }
                        option { value: "openai_compatible", "OpenAI 兼容" }
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
            },
            footer: rsx! {
                button { class: "btn btn-ghost", onclick: move |_| show_modal.set(false), "取消" }
                button { class: "btn btn-accent", disabled: creating(), onclick: handle_create,
                    if creating() { "创建中..." } else { "创建" }
                }
            }
        }
    }
}
