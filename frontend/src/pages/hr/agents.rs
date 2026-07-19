//! Agent 管理列表

use dioxus::prelude::*;

use crate::api::finance::list_model_providers;
use crate::api::hr::{create_agent, create_external_agent, delete_agent, list_agents, search_agents};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::store::toast::use_toast;
use common::api::{
    CreateAgentRequest, CreateExternalAgentRequest, ListAgentsResponseItem,
    ListModelProvidersResponseItem,
};

/// Agent kind 对应的 badge 样式和标签
fn kind_badge_class(kind: &str) -> &'static str {
    match kind {
        "local" => "badge badge-info",
        "cli" => "badge badge-accent",
        "remote" => "badge badge-success",
        _ => "badge badge-ghost",
    }
}

fn kind_label(kind: &str) -> String {
    match kind {
        "local" => "本地".to_string(),
        "cli" => "CLI".to_string(),
        "remote" => "远程".to_string(),
        _ => kind.to_string(),
    }
}

#[component]
pub fn HrAgents() -> Element {
    let mut agents = use_signal(Vec::<ListAgentsResponseItem>::new);
    let mut model_providers = use_signal(Vec::<ListModelProvidersResponseItem>::new);
    let mut loading = use_signal(|| true);
    let toast = use_toast();
    let mut search_keyword = use_signal(String::new);

    // ===== 本地 Agent 创建 Modal =====
    let mut show_add_modal = use_signal(|| false);
    let mut new_name = use_signal(String::new);
    let mut new_roles = use_signal(String::new);
    let mut new_model_provider_id = use_signal(String::new);
    let mut new_description = use_signal(String::new);
    let mut creating = use_signal(|| false);

    // ===== 外部 Agent 创建 Modal =====
    let mut show_external_modal = use_signal(|| false);
    let mut ext_kind = use_signal(|| "cli".to_string());
    let mut ext_name = use_signal(String::new);
    let mut ext_roles = use_signal(String::new);
    let mut ext_description = use_signal(String::new);
    // CLI 配置
    let mut ext_command = use_signal(String::new);
    let mut ext_args_str = use_signal(String::new);
    let mut ext_work_dir = use_signal(String::new);
    let mut ext_timeout = use_signal(|| "300".to_string());
    let mut ext_prompt_template = use_signal(String::new);
    // Remote 配置
    let mut ext_endpoint = use_signal(String::new);
    let mut ext_agent_name = use_signal(String::new);
    let mut ext_auth_token = use_signal(String::new);
    let mut ext_creating = use_signal(|| false);

    use_effect(move || {
        loading.set(true);
        spawn(async move {
            match list_agents().await {
                Ok(list) => agents.set(list.agents),
                Err(e) => toast.error(&e),
            }
            match list_model_providers().await {
                Ok(resp) => model_providers.set(resp.providers),
                Err(_) => {}
            }
            loading.set(false);
        });
    });

    let reload_agents = move || {
        let keyword = search_keyword();
        spawn(async move {
            let result = if keyword.trim().is_empty() {
                list_agents().await
            } else {
                search_agents(&keyword).await
            };
            match result {
                Ok(list) => agents.set(list.agents),
                Err(e) => toast.error(&e),
            }
        });
    };

    // ===== 本地 Agent 创建处理 =====
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
                    reload_agents();
                }
                Err(e) => toast.error(&format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    // ===== 外部 Agent 创建处理 =====
    let handle_create_external = move |_| {
        spawn(async move {
            if ext_name().is_empty() {
                toast.error("名称不能为空");
                return;
            }
            let kind = ext_kind();
            let timeout_secs = ext_timeout().parse::<u64>().unwrap_or(300);

            let args = if ext_args_str().trim().is_empty() {
                None
            } else {
                Some(ext_args_str().split_whitespace().map(|s| s.to_string()).collect())
            };

            let req = CreateExternalAgentRequest {
                name: ext_name(),
                roles: if ext_roles().is_empty() { None } else { Some(vec![ext_roles()]) },
                description: if ext_description().is_empty() { None } else { Some(ext_description()) },
                capabilities: None,
                soul: None,
                kind: kind.clone(),
                command: if kind == "cli" { Some(ext_command()) } else { None },
                args: if kind == "cli" { args.clone() } else { None },
                work_dir: if kind == "cli" { Some(ext_work_dir()) } else { None },
                env: None,
                timeout_secs: Some(timeout_secs),
                prompt_template: if kind == "cli" && !ext_prompt_template().is_empty() {
                    Some(ext_prompt_template())
                } else {
                    None
                },
                endpoint: if kind == "remote" { Some(ext_endpoint()) } else { None },
                agent_name: if kind == "remote" { Some(ext_agent_name()) } else { None },
                auth_token: if kind == "remote" && !ext_auth_token().is_empty() {
                    Some(ext_auth_token())
                } else {
                    None
                },
            };

            ext_creating.set(true);
            match create_external_agent(req).await {
                Ok(_) => {
                    show_external_modal.set(false);
                    ext_name.set(String::new());
                    ext_roles.set(String::new());
                    ext_description.set(String::new());
                    ext_command.set(String::new());
                    ext_args_str.set(String::new());
                    ext_work_dir.set(String::new());
                    ext_timeout.set("300".to_string());
                    ext_prompt_template.set(String::new());
                    ext_endpoint.set(String::new());
                    ext_agent_name.set(String::new());
                    ext_auth_token.set(String::new());
                    reload_agents();
                    toast.success("外部 Agent 创建成功");
                }
                Err(e) => toast.error(&format!("创建失败: {}", e)),
            }
            ext_creating.set(false);
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
                    button { class: "btn btn-accent",
                        onclick: move |_| show_add_modal.set(true),
                        "+ 本地 Agent"
                    }
                    button { class: "btn btn-success",
                        onclick: move |_| show_external_modal.set(true),
                        "+ 外部 Agent"
                    }
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
                        th { "类型" }
                        th { "角色" }
                        th { "模型 / 执行器" }
                        th { "操作" }
                    }}
                    tbody {
                        for agent in agents_list.iter() {
                            {
                                let id = agent.id.clone();
                                let aname = agent.name.clone();
                                let aroles = agent.roles.join(", ");
                                let akind = agent.kind.clone();
                                let amp = agent.model_provider_id.clone();
                                let id_delete = id.clone();
                                let display_value = match akind.as_str() {
                                    "local" => amp.clone(),
                                    "cli" => "CLI 子进程".to_string(),
                                    "remote" => "A2A 远程".to_string(),
                                    _ => amp.clone(),
                                };
                                rsx! {
                                    tr { key: "{id}",
                                        td { "data-label": "名称",
                                            Link { to: crate::pages::Route::HrAgentDetail { id: id.clone() },
                                                class: "detail-back-link",
                                                "{aname}"
                                            }
                                        }
                                        td { "data-label": "类型",
                                            span { class: "{kind_badge_class(&akind)}", "{kind_label(&akind)}" }
                                        }
                                        td { class: "text-secondary", "data-label": "角色", "{aroles}" }
                                        td { class: "text-mono", "data-label": "模型/执行器", "{display_value}" }
                                        td { "data-label": "操作",
                                            button { class: "btn btn-danger btn-sm",
                                                onclick: move |_| {
                                                    let id_delete = id_delete.clone();
                                                    spawn(async move {
                                                        if let Err(e) = delete_agent(&id_delete).await {
                                                            toast.error(&format!("删除失败: {}", e));
                                                        } else {
                                                            reload_agents();
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

        // ===== 本地 Agent 创建弹窗 =====
        Modal {
            title: "创建本地 Agent".to_string(),
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

        // ===== 外部 Agent 创建弹窗 =====
        Modal {
            title: "创建外部 Agent".to_string(),
            show: show_external_modal(),
            on_close: move |_| {
                show_external_modal.set(false);
            },
            footer: rsx! {
                button { class: "btn btn-ghost", onclick: move |_| show_external_modal.set(false), "取消" }
                button { class: "btn btn-success", disabled: ext_creating(), onclick: handle_create_external,
                    if ext_creating() { "创建中..." } else { "创建" }
                }
            },
            div {
                div { class: "form-group",
                    label { class: "form-label", "Agent 类型 *" }
                    select { class: "form-select", value: "{ext_kind}",
                        onchange: move |e| ext_kind.set(e.value()),
                        option { value: "cli", "CLI 子进程（Codex / Claude Code / Aider 等）" }
                        option { value: "remote", "远程 A2A Agent" }
                    }
                }
                div { class: "form-group",
                    label { class: "form-label", "Agent 名称 *" }
                    input { class: "form-input", value: "{ext_name}",
                        oninput: move |e| ext_name.set(e.value()), placeholder: "请输入 Agent 名称" }
                }
                div { class: "form-group",
                    label { class: "form-label", "角色描述" }
                    input { class: "form-input", value: "{ext_roles}",
                        oninput: move |e| ext_roles.set(e.value()), placeholder: "如：代码助手" }
                }
                div { class: "form-group",
                    label { class: "form-label", "描述" }
                    textarea { class: "form-textarea", value: "{ext_description}",
                        oninput: move |e| ext_description.set(e.value()), placeholder: "Agent 描述（可选）" }
                }

                // CLI 配置
                if ext_kind() == "cli" {
                    div { class: "form-group",
                        label { class: "form-label", "启动命令 *" }
                        input { class: "form-input", value: "{ext_command}",
                            oninput: move |e| ext_command.set(e.value()),
                            placeholder: "如：codex、claude、aider" }
                    }
                    div { class: "form-group",
                        label { class: "form-label", "命令参数（空格分隔）" }
                        input { class: "form-input", value: "{ext_args_str}",
                            oninput: move |e| ext_args_str.set(e.value()),
                            placeholder: "如：--auto --yes" }
                    }
                    div { class: "form-group",
                        label { class: "form-label", "工作目录 *" }
                        input { class: "form-input", value: "{ext_work_dir}",
                            oninput: move |e| ext_work_dir.set(e.value()),
                            placeholder: "/path/to/workdir" }
                    }
                    div { class: "form-group",
                        label { class: "form-label", "超时时间（秒）" }
                        input { class: "form-input", value: "{ext_timeout}",
                            oninput: move |e| ext_timeout.set(e.value()),
                            r#type: "number", placeholder: "300" }
                    }
                    div { class: "form-group",
                        label { class: "form-label", "自定义 Prompt 模板（可选）" }
                        textarea { class: "form-textarea", value: "{ext_prompt_template}",
                            oninput: move |e| ext_prompt_template.set(e.value()),
                            placeholder: "使用 {{prompt}} 占位符标记 prompt 位置" }
                    }
                }

                // Remote 配置
                if ext_kind() == "remote" {
                    div { class: "form-group",
                        label { class: "form-label", "A2A Server 地址 *" }
                        input { class: "form-input", value: "{ext_endpoint}",
                            oninput: move |e| ext_endpoint.set(e.value()),
                            placeholder: "https://a2a-server.example.com" }
                    }
                    div { class: "form-group",
                        label { class: "form-label", "目标 Agent 名称 *" }
                        input { class: "form-input", value: "{ext_agent_name}",
                            oninput: move |e| ext_agent_name.set(e.value()),
                            placeholder: "目标 Agent 的 ID / 名称" }
                    }
                    div { class: "form-group",
                        label { class: "form-label", "认证 Token（可选）" }
                        input { class: "form-input", value: "{ext_auth_token}",
                            oninput: move |e| ext_auth_token.set(e.value()),
                            placeholder: "Bearer xxx" }
                    }
                    div { class: "form-group",
                        label { class: "form-label", "超时时间（秒）" }
                        input { class: "form-input", value: "{ext_timeout}",
                            oninput: move |e| ext_timeout.set(e.value()),
                            r#type: "number", placeholder: "300" }
                    }
                }
            }
        }
    }
}
