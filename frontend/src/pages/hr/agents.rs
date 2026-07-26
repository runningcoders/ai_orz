//! Agent 管理列表

use dioxus::prelude::*;

use crate::api::finance::list_model_providers;
use crate::api::hr::{
    create_agent, create_external_agent, delete_agent, list_agents, search_agents,
};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
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

    // 修复 HIGH #12：搜索输入框每次按键触发请求，无防抖 + race condition。
    // 引入 search_request_id 机制丢弃过期请求结果。
    let mut search_request_id = use_signal(|| 0u32);

    // ===== 删除确认对话框 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(|| String::new());

    use_effect(move || {
        loading.set(true);
        spawn(async move {
            match list_agents(None, None).await {
                Ok(page) => agents.set(page.items),
                Err(e) => toast.error(&e),
            }
            match list_model_providers().await {
                Ok(resp) => model_providers.set(resp.providers),
                Err(_) => {}
            }
            loading.set(false);
        });
    });

    let mut reload_agents = move || {
        let keyword = search_keyword();
        // 修复 HIGH #12：自增 request_id，结果到达时校验是否为最新请求
        let my_id = search_request_id() + 1;
        search_request_id.set(my_id);
        spawn(async move {
            let result = if keyword.trim().is_empty() {
                list_agents(None, None).await.map(|p| p.items)
            } else {
                search_agents(&keyword).await.map(|r| r.agents)
            };
            // 丢弃过期请求的结果
            if search_request_id() != my_id {
                return;
            }
            match result {
                Ok(v) => agents.set(v),
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
                roles: if new_roles().is_empty() {
                    None
                } else {
                    Some(vec![new_roles()])
                },
                description: if new_description().is_empty() {
                    None
                } else {
                    Some(new_description())
                },
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
                Some(
                    ext_args_str()
                        .split_whitespace()
                        .map(|s| s.to_string())
                        .collect(),
                )
            };

            let req = CreateExternalAgentRequest {
                name: ext_name(),
                roles: if ext_roles().is_empty() {
                    None
                } else {
                    Some(vec![ext_roles()])
                },
                description: if ext_description().is_empty() {
                    None
                } else {
                    Some(ext_description())
                },
                capabilities: None,
                soul: None,
                kind: kind.clone(),
                command: if kind == "cli" {
                    Some(ext_command())
                } else {
                    None
                },
                args: if kind == "cli" { args.clone() } else { None },
                work_dir: if kind == "cli" {
                    Some(ext_work_dir())
                } else {
                    None
                },
                env: None,
                timeout_secs: Some(timeout_secs),
                prompt_template: if kind == "cli" && !ext_prompt_template().is_empty() {
                    Some(ext_prompt_template())
                } else {
                    None
                },
                endpoint: if kind == "remote" {
                    Some(ext_endpoint())
                } else {
                    None
                },
                agent_name: if kind == "remote" {
                    Some(ext_agent_name())
                } else {
                    None
                },
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
        AppLayout {
            div { class: "card bg-base-100 shadow-md",
                div { class: "card-body",
                    div { class: "flex flex-col sm:flex-row justify-between items-start sm:items-center gap-4 mb-4",
                        h2 { class: "card-title", "Agent 管理" }
                    div { class: "flex gap-2 flex-wrap",
                        input { class: "input input-bordered w-full sm:w-auto", value: "{search_keyword}",
                            oninput: move |e| {
                                search_keyword.set(e.value());
                                // 修复 HIGH #12：防抖 300ms 后触发搜索，request_id 机制丢弃过期结果
                                let my_id = search_request_id() + 1;
                                search_request_id.set(my_id);
                                spawn(async move {
                                    gloo_timers::future::TimeoutFuture::new(300).await;
                                    if search_request_id() != my_id { return; }
                                    loading.set(true);
                                    let kw = search_keyword();
                                    let result = if kw.trim().is_empty() {
                                        list_agents(None, None).await.map(|p| p.items)
                                    } else {
                                        search_agents(&kw).await.map(|r| r.agents)
                                    };
                                    if search_request_id() != my_id { return; }
                                    match result {
                                        Ok(v) => agents.set(v),
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
                                    reload_agents();
                                },
                                "重置"
                            }
                        }
                        button { class: "btn btn-primary",
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
                    div { class: "overflow-x-auto",
                        table { class: "table table-zebra table-pin-rows",
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
                                                        class: "link link-primary",
                                                        "{aname}"
                                                    }
                                                }
                                                td { "data-label": "类型",
                                                    span { class: "{kind_badge_class(&akind)}", "{kind_label(&akind)}" }
                                                }
                                                td { class: "text-base-content/70", "data-label": "角色", "{aroles}" }
                                                td { class: "font-mono text-sm", "data-label": "模型/执行器", "{display_value}" }
                                                td { "data-label": "操作",
                                                    button { class: "btn btn-error btn-sm",
                                                        onclick: move |_| {
                                                            pending_delete_id.set(id_delete.clone());
                                                            show_delete_confirm.set(true);
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
                button { class: "btn btn-primary", disabled: creating(), onclick: handle_create,
                    if creating() { "创建中..." } else { "创建" }
                }
            },
            div { class: "space-y-4",
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "Agent 名称 *" }
                    }
                    input { class: "input input-bordered w-full", value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()), placeholder: "请输入 Agent 名称" }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "角色描述" }
                    }
                    input { class: "input input-bordered w-full", value: "{new_roles}",
                        oninput: move |e| new_roles.set(e.value()), placeholder: "如：代码助手" }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "模型提供商 *" }
                    }
                    if model_providers.read().is_empty() {
                        input { class: "input input-bordered w-full", value: "{new_model_provider_id}",
                            oninput: move |e| new_model_provider_id.set(e.value()),
                            placeholder: "请先在财务管理中配置模型提供商" }
                    } else {
                        select { class: "select select-bordered w-full", value: "{new_model_provider_id}",
                            onchange: move |e| new_model_provider_id.set(e.value()),
                            option { value: "", "-- 请选择 --" }
                            for mp in model_providers.read().iter() {
                                option { value: "{mp.id}", "{mp.name} ({mp.model_name})" }
                            }
                        }
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "描述" }
                    }
                    textarea { class: "textarea textarea-bordered w-full", value: "{new_description}",
                        oninput: move |e| new_description.set(e.value()), placeholder: "Agent 描述（可选）" }
                }
            }
        }

        // ===== 外部 Agent 创建弹窗 =====
        Modal {
            title: "创建外部 Agent".to_string(),
            show: show_external_modal(),
            on_close: move |_| {
                // 修复 HIGH #14：之前 on_close 只关闭弹窗不重置表单，
                // 导致用户下次打开仍残留上次填写的数据（状态污染）
                show_external_modal.set(false);
                ext_kind.set("cli".to_string());
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
            },
            footer: rsx! {
                button { class: "btn btn-ghost", onclick: move |_| show_external_modal.set(false), "取消" }
                button { class: "btn btn-success", disabled: ext_creating(), onclick: handle_create_external,
                    if ext_creating() { "创建中..." } else { "创建" }
                }
            },
            div { class: "space-y-4",
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "Agent 类型 *" }
                    }
                    select { class: "select select-bordered w-full", value: "{ext_kind}",
                        onchange: move |e| ext_kind.set(e.value()),
                        option { value: "cli", "CLI 子进程（Codex / Claude Code / Aider 等）" }
                        option { value: "remote", "远程 A2A Agent" }
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "Agent 名称 *" }
                    }
                    input { class: "input input-bordered w-full", value: "{ext_name}",
                        oninput: move |e| ext_name.set(e.value()), placeholder: "请输入 Agent 名称" }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "角色描述" }
                    }
                    input { class: "input input-bordered w-full", value: "{ext_roles}",
                        oninput: move |e| ext_roles.set(e.value()), placeholder: "如：代码助手" }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "描述" }
                    }
                    textarea { class: "textarea textarea-bordered w-full", value: "{ext_description}",
                        oninput: move |e| ext_description.set(e.value()), placeholder: "Agent 描述（可选）" }
                }

                // CLI 配置
                if ext_kind() == "cli" {
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "启动命令 *" }
                        }
                        input { class: "input input-bordered w-full", value: "{ext_command}",
                            oninput: move |e| ext_command.set(e.value()),
                            placeholder: "如：codex、claude、aider" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "命令参数（空格分隔）" }
                        }
                        input { class: "input input-bordered w-full", value: "{ext_args_str}",
                            oninput: move |e| ext_args_str.set(e.value()),
                            placeholder: "如：--auto --yes" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "工作目录 *" }
                        }
                        input { class: "input input-bordered w-full", value: "{ext_work_dir}",
                            oninput: move |e| ext_work_dir.set(e.value()),
                            placeholder: "/path/to/workdir" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "超时时间（秒）" }
                        }
                        input { class: "input input-bordered w-full", value: "{ext_timeout}",
                            oninput: move |e| ext_timeout.set(e.value()),
                            r#type: "number", placeholder: "300" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "自定义 Prompt 模板（可选）" }
                        }
                        textarea { class: "textarea textarea-bordered w-full", value: "{ext_prompt_template}",
                            oninput: move |e| ext_prompt_template.set(e.value()),
                            placeholder: "使用 {{prompt}} 占位符标记 prompt 位置" }
                    }
                }

                // Remote 配置
                if ext_kind() == "remote" {
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "A2A Server 地址 *" }
                        }
                        input { class: "input input-bordered w-full", value: "{ext_endpoint}",
                            oninput: move |e| ext_endpoint.set(e.value()),
                            placeholder: "https://a2a-server.example.com" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "目标 Agent 名称 *" }
                        }
                        input { class: "input input-bordered w-full", value: "{ext_agent_name}",
                            oninput: move |e| ext_agent_name.set(e.value()),
                            placeholder: "目标 Agent 的 ID / 名称" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "认证 Token（可选）" }
                        }
                        input { class: "input input-bordered w-full", value: "{ext_auth_token}",
                            oninput: move |e| ext_auth_token.set(e.value()),
                            placeholder: "Bearer xxx" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "超时时间（秒）" }
                        }
                        input { class: "input input-bordered w-full", value: "{ext_timeout}",
                            oninput: move |e| ext_timeout.set(e.value()),
                            r#type: "number", placeholder: "300" }
                    }
                }
            }
        }

        ConfirmDialog {
            show: show_delete_confirm(),
            title: "确认删除".to_string(),
            message: "确定删除此 Agent？此操作不可撤销。".to_string(),
            on_confirm: move |_| {
                let id = pending_delete_id();
                show_delete_confirm.set(false);
                spawn(async move {
                    if let Err(e) = delete_agent(&id).await {
                        toast.error(&format!("删除失败: {}", e));
                    } else {
                        reload_agents();
                    }
                });
            },
            on_cancel: move |_| {
                show_delete_confirm.set(false);
            }
        }
        }
    }
}
