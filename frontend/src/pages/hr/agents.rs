//! Agent 管理列表

use dioxus::prelude::*;

use crate::api::finance::list_model_providers;
use crate::api::hr::{create_agent, delete_agent, list_agents, search_agents};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::store::toast::use_toast;
use common::api::{CreateAgentRequest, ListAgentsResponseItem, ListModelProvidersResponseItem};

#[component]
pub fn HrAgents() -> Element {
    let mut agents = use_signal(Vec::<ListAgentsResponseItem>::new);
    let mut model_providers = use_signal(Vec::<ListModelProvidersResponseItem>::new);
    let mut loading = use_signal(|| true);
    let toast = use_toast();
    let mut show_add_modal = use_signal(|| false);
    let mut new_name = use_signal(String::new);
    let mut new_roles = use_signal(String::new);
    let mut new_model_provider_id = use_signal(String::new);
    let mut new_description = use_signal(String::new);
    let mut creating = use_signal(|| false);
    let mut search_keyword = use_signal(String::new);

    use_effect(move || {
        loading.set(true);
        spawn(async move {
            match list_agents().await {
                Ok(list) => agents.set(list.agents),
                Err(e) => toast.error(&e),
            }
            // 加载模型提供商列表用于下拉选择
            match list_model_providers().await {
                Ok(resp) => model_providers.set(resp.providers),
                Err(_) => {} // 静默失败，不影响主流程
            }
            loading.set(false);
        });
    });

    let handle_create = move |_| {
        spawn(async move {
            if new_name().is_empty() || new_model_provider_id().is_empty() {
                toast.error("名称和模型提供商 ID 不能为空");
                return;
            }
            creating.set(true);
            let req = CreateAgentRequest {
                name: new_name(),
                roles: if new_roles().is_empty() { None } else { Some(vec![new_roles()]) },
                description: if new_description().is_empty() { None } else { Some(new_description()) },
                capabilities: None,
                soul: None,
                model_provider_id: new_model_provider_id(),
            };
            match create_agent(req).await {
                Ok(_) => {
                    show_add_modal.set(false);
                    new_name.set(String::new());
                    new_roles.set(String::new());
                    new_model_provider_id.set(String::new());
                    new_description.set(String::new());
                    // Reload
                    let keyword = search_keyword();
                    let result = if keyword.trim().is_empty() {
                        list_agents().await
                    } else {
                        search_agents(&keyword).await
                    };
                    match result {
                        Ok(list) => agents.set(list.agents),
                        Err(e) => toast.error(&e),
                    }
                }
                Err(e) => toast.error(&format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    let agents_list = agents.read().clone();

    rsx! {
        div { class: "card",
            div { class: "card-header",
                h2 { class: "card-title", "Agent 管理" }
                div { class: "flex gap-2",
                    input { class: "form-input", value: "{search_keyword}",
                        oninput: move |e| {
                            let keyword = e.value();
                            search_keyword.set(keyword.clone());
                            spawn(async move {
                                loading.set(true);
                                let result = if keyword.trim().is_empty() {
                                    list_agents().await
                                } else {
                                    search_agents(&keyword).await
                                };
                                match result {
                                    Ok(list) => agents.set(list.agents),
                                    Err(e) => toast.error(&e),
                                }
                                loading.set(false);
                            });
                        },
                        placeholder: "搜索 Agent..."
                    }
                    if !search_keyword().is_empty() {
                        button { class: "btn btn-ghost",
                            onclick: move |_| {
                                search_keyword.set(String::new());
                                spawn(async move {
                                    loading.set(true);
                                    match list_agents().await {
                                        Ok(list) => agents.set(list.agents),
                                        Err(e) => toast.error(&e),
                                    }
                                    loading.set(false);
                                });
                            },
                            "重置"
                        }
                    }
                    button { class: "btn btn-accent", onclick: move |_| show_add_modal.set(true), "+ 创建 Agent" }
                }
            }

            if loading() {
                Loading {}
            } else if agents_list.is_empty() {
                EmptyState { icon: "🤖".to_string(), message: "暂无 Agent，点击上方按钮创建第一个".to_string() }
            } else {
                table { class: "table",
                    thead { tr {
                        th { "名称" }
                        th { "角色" }
                        th { "模型提供商" }
                        th { "操作" }
                    }}
                    tbody {
                        for agent in agents_list.iter() {
                            {
                                let id = agent.id.clone();
                                let aname = agent.name.clone();
                                let aroles = agent.roles.join(", ");
                                let amp = agent.model_provider_id.clone();
                                let id_delete = id.clone();
                                rsx! {
                                    tr { key: "{id}",
                                        td { "data-label": "名称",
                                            Link { to: crate::pages::Route::HrAgentDetail { id: id.clone() },
                                                class: "detail-back-link",
                                                "{aname}"
                                            }
                                        }
                                        td { class: "text-secondary", "data-label": "角色", "{aroles}" }
                                        td { class: "text-mono", "data-label": "模型提供商", "{amp}" }
                                        td { "data-label": "操作",
                                            button { class: "btn btn-danger btn-sm",
                                                onclick: move |_| {
                                                    let id_delete = id_delete.clone();
                                                    spawn(async move {
                                                        if let Err(e) = delete_agent(&id_delete).await {
                                                            toast.error(&format!("删除失败: {}", e));
                                                        } else {
                                                            let keyword = search_keyword();
                                                            let result = if keyword.trim().is_empty() {
                                                                list_agents().await
                                                            } else {
                                                                search_agents(&keyword).await
                                                            };
                                                            match result {
                                                                Ok(list) => agents.set(list.agents),
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

        // 创建 Agent 弹窗
        Modal {
            title: "创建新 Agent".to_string(),
            show: show_add_modal(),
            on_close: move |_| {
                show_add_modal.set(false);
                new_name.set(String::new());
                new_roles.set(String::new());
                new_model_provider_id.set(String::new());
                new_description.set(String::new());
            },
            footer: rsx! {
                button { class: "btn btn-ghost", onclick: move |_| show_add_modal.set(false), "取消" }
                button { class: "btn btn-accent", disabled: creating(), onclick: handle_create,
                    if creating() { "创建中..." } else { "创建" }
                }
            },
            div {
                div { class: "form-group",
                    label { class: "form-label", "Agent 名称 *" }
                    input { class: "form-input", value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()), placeholder: "请输入 Agent 名称" }
                }
                div { class: "form-group",
                    label { class: "form-label", "角色描述" }
                    input { class: "form-input", value: "{new_roles}",
                        oninput: move |e| new_roles.set(e.value()), placeholder: "如：代码助手" }
                }
                div { class: "form-group",
                    label { class: "form-label", "模型提供商 *" }
                    if model_providers.read().is_empty() {
                        input { class: "form-input", value: "{new_model_provider_id}",
                            oninput: move |e| new_model_provider_id.set(e.value()),
                            placeholder: "请先在财务管理中配置模型提供商" }
                    } else {
                        select { class: "form-select", value: "{new_model_provider_id}",
                            onchange: move |e| new_model_provider_id.set(e.value()),
                            option { value: "", "-- 请选择 --" }
                            for mp in model_providers.read().iter() {
                                option { value: "{mp.id}", "{mp.name} ({mp.model_name})" }
                            }
                        }
                    }
                }
                div { class: "form-group",
                    label { class: "form-label", "描述" }
                    textarea { class: "form-textarea", value: "{new_description}",
                        oninput: move |e| new_description.set(e.value()), placeholder: "Agent 描述（可选）" }
                }
            }
        }
    }
}
