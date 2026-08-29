//! Agent 管理列表

use crate::components::hud::HudPanel;
use crate::components::hud::PageHeader;
use dioxus::prelude::*;

use crate::api::finance::list_model_providers;
use crate::api::hr::{
    create_agent, create_external_agent, delete_agent, list_agents, query_agents, search_agents,
    update_agent_status,
};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{
    AgentQueryRequest, CreateAgentRequest, CreateExternalAgentRequest, ListAgentsRequest,
    ListAgentsResponseItem, ListModelProvidersResponseItem, SearchAgentsRequest,
    UpdateAgentStatusRequest,
};
use common::enums::AgentStatus;
use dioxus_router::Link;

/// Agent kind 对应的 badge 样式和标签
fn kind_badge_class(kind: &str) -> &'static str {
    match kind {
        "local" => "badge hud-badge badge-info",
        "cli" => "badge hud-badge badge-accent",
        "remote" => "badge hud-badge badge-success",
        _ => "badge hud-badge badge-ghost",
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

fn agent_status_label(status: i32) -> String {
    match status {
        0 => "已删除".to_string(),
        1 => "面试中".to_string(),
        2 => "待入职".to_string(),
        3 => "已入职".to_string(),
        4 => "已离职".to_string(),
        5 => "待离职".to_string(),
        _ => status.to_string(),
    }
}

fn agent_status_badge_class(status: i32) -> &'static str {
    match status {
        3 => "badge hud-badge badge-success",
        1 => "badge hud-badge badge-warning",
        2 => "badge hud-badge badge-info",
        _ => "badge hud-badge badge-ghost",
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
    let mut new_roles = use_signal(Vec::<String>::new);
    let mut new_roles_input = use_signal(String::new);
    let mut new_capabilities = use_signal(Vec::<String>::new);
    let mut new_capabilities_input = use_signal(String::new);
    let mut new_soul = use_signal(String::new);
    let mut new_model_provider_id = use_signal(String::new);
    let mut new_description = use_signal(String::new);
    let mut creating = use_signal(|| false);

    // ===== 外部 Agent 创建 Modal =====
    let mut show_external_modal = use_signal(|| false);
    let mut ext_kind = use_signal(|| "cli".to_string());
    let mut ext_name = use_signal(String::new);
    let mut ext_roles = use_signal(Vec::<String>::new);
    let mut ext_roles_input = use_signal(String::new);
    let mut ext_capabilities = use_signal(Vec::<String>::new);
    let mut ext_capabilities_input = use_signal(String::new);
    let mut ext_soul = use_signal(String::new);
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

    // 过滤条件
    let mut filter_status = use_signal(|| -1i32);

    // ===== 删除确认对话框 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(String::new);

    // 加载数据（三场景切换：list / query / search）
    let load_data = move || {
        spawn(async move {
            loading.set(true);
            let keyword = search_keyword();
            let status = filter_status();
            let my_id = search_request_id() + 1;
            search_request_id.set(my_id);

            let has_filter = status >= 0;

            // 三场景切换：
            // 无关键词 + 无过滤 → list_agents
            // 无关键词 + 有过滤 → query_agents
            // 有关键词 → search_agents（可同时带过滤条件）
            let result = if keyword.trim().is_empty() && !has_filter {
                list_agents(ListAgentsRequest::default())
                    .await
                    .map(|p| p.items)
            } else if keyword.trim().is_empty() {
                query_agents(&AgentQueryRequest {
                    status: if status >= 0 {
                        Some(AgentStatus::from(status))
                    } else {
                        None
                    },
                    ..Default::default()
                })
                .await
                .map(|p| p.items)
            } else {
                search_agents(&SearchAgentsRequest {
                    keyword: Some(keyword),
                    status: if status >= 0 {
                        Some(AgentStatus::from(status))
                    } else {
                        None
                    },
                    ..Default::default()
                })
                .await
                .map(|p| p.items)
            };

            // 丢弃过期请求的结果
            if search_request_id() != my_id {
                return;
            }

            match result {
                Ok(v) => agents.set(v),
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    };

    // 初始加载
    use_effect(move || {
        load_data();
        spawn(async move {
            if let Ok(resp) = list_model_providers().await {
                model_providers.set(resp.providers)
            }
        });
    });

    // ===== 一键入职处理：面试中→待入职→已入职（后端白名单逐级流转）=====
    let handle_onboard = move |id: String, status: i32| {
        let status = AgentStatus::from(status);
        spawn(async move {
            // 面试中需先转待入职，再转已入职
            if status == AgentStatus::Interviewing
                && let Err(e) = update_agent_status(UpdateAgentStatusRequest {
                    id: id.clone(),
                    status: AgentStatus::PendingOnboard,
                })
                .await
            {
                toast.error(format!("转入待入职失败: {}", e));
                return;
            }
            match update_agent_status(UpdateAgentStatusRequest {
                id: id.clone(),
                status: AgentStatus::Onboarded,
            })
            .await
            {
                Ok(_) => {
                    toast.success("Agent 已正式入职");
                    load_data();
                }
                Err(e) => {
                    toast.error(format!("入职失败: {}", e));
                    load_data();
                }
            }
        });
    };

    // ===== 本地 Agent 创建处理 =====
    let handle_create = move |_| {
        spawn(async move {
            if new_name().is_empty() {
                toast.error("名称不能为空");
                return;
            }
            creating.set(true);
            let req = CreateAgentRequest {
                name: new_name(),
                roles: if new_roles().is_empty() {
                    None
                } else {
                    Some(new_roles())
                },
                description: if new_description().is_empty() {
                    None
                } else {
                    Some(new_description())
                },
                capabilities: if new_capabilities().is_empty() {
                    None
                } else {
                    Some(new_capabilities())
                },
                soul: if new_soul().is_empty() {
                    None
                } else {
                    Some(new_soul())
                },
                model_provider_id: new_model_provider_id(),
            };
            match create_agent(req).await {
                Ok(_) => {
                    show_add_modal.set(false);
                    new_name.set(String::new());
                    new_roles.set(Vec::new());
                    new_roles_input.set(String::new());
                    new_capabilities.set(Vec::new());
                    new_capabilities_input.set(String::new());
                    new_soul.set(String::new());
                    new_model_provider_id.set(String::new());
                    new_description.set(String::new());
                    load_data();
                }
                Err(e) => toast.error(format!("创建失败: {}", e)),
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
                    Some(ext_roles())
                },
                description: if ext_description().is_empty() {
                    None
                } else {
                    Some(ext_description())
                },
                capabilities: if ext_capabilities().is_empty() {
                    None
                } else {
                    Some(ext_capabilities())
                },
                soul: if ext_soul().is_empty() {
                    None
                } else {
                    Some(ext_soul())
                },
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
                    ext_kind.set("cli".to_string());
                    ext_name.set(String::new());
                    ext_roles.set(Vec::new());
                    ext_roles_input.set(String::new());
                    ext_capabilities.set(Vec::new());
                    ext_capabilities_input.set(String::new());
                    ext_soul.set(String::new());
                    ext_description.set(String::new());
                    ext_command.set(String::new());
                    ext_args_str.set(String::new());
                    ext_work_dir.set(String::new());
                    ext_timeout.set("300".to_string());
                    ext_prompt_template.set(String::new());
                    ext_endpoint.set(String::new());
                    ext_agent_name.set(String::new());
                    ext_auth_token.set(String::new());
                    load_data();
                    toast.success("外部 Agent 创建成功");
                }
                Err(e) => toast.error(format!("创建失败: {}", e)),
            }
            ext_creating.set(false);
        });
    };

    let agents_list = agents.read().clone();

    rsx! {
        AppLayout {
            PageHeader {
                eyebrow: Some("HR".to_string()),
                title: "Agent 管理".to_string(),
                actions: Some(rsx!{
                div { class: "flex gap-2 flex-wrap",
                    if !search_keyword().is_empty() || filter_status() >= 0 {
                        button { class: "btn hud-btn btn-ghost",
                            onclick: move |_| {
                                search_keyword.set(String::new());
                                filter_status.set(-1);
                                load_data();
                            },
                            "重置"
                        }
                    }
                    button { class: "btn hud-btn btn-primary",
                        onclick: move |_| show_add_modal.set(true),
                        "+ 本地 Agent"
                    }
                    button { class: "btn hud-btn btn-success",
                        onclick: move |_| show_external_modal.set(true),
                        "+ 外部 Agent"
                    }
                }
                }),
            },

            // 筛选栏（独立卡片）
            HudPanel { signal: Some(true), extra_class: Some("mb-4".to_string()),
                div { class: "card-body",
                    div { class: "flex flex-wrap gap-4 items-end",
                        div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                            label { class: "form-label", "状态" }
                            select {
                                class: "select select-bordered w-full",
                                value: "{filter_status}",
                                onchange: move |e| {
                                    if let Ok(v) = e.value().parse::<i32>() {
                                        filter_status.set(v);
                                    }
                                    load_data();
                                },
                                option { value: "-1", "全部" }
                                option { value: "1", "面试中" }
                                option { value: "2", "待入职" }
                                option { value: "3", "已入职" }
                                option { value: "4", "已离职" }
                                option { value: "5", "待离职" }
                            }
                        }
                        div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                            label { class: "form-label", "搜索" }
                            input {
                                class: "input input-bordered w-full",
                                placeholder: "搜索 Agent...",
                                value: "{search_keyword}",
                                oninput: move |e| {
                                    search_keyword.set(e.value());
                                    let my_id = search_request_id() + 1;
                                    search_request_id.set(my_id);
                                    spawn(async move {
                                        gloo_timers::future::TimeoutFuture::new(300).await;
                                        if search_request_id() != my_id {
                                            return;
                                        }
                                        load_data();
                                    });
                                }
                            }
                        }
                    }
                }
            }

            // 列表卡片
            HudPanel { signal: Some(true),
                div { class: "card-body",
                if loading() {
                    Loading {}
                } else if agents_list.is_empty() {
                    EmptyState { icon: "🤖".to_string(), message: "暂无 Agent，点击上方按钮创建第一个".to_string() }
                } else {
                    div { class: "overflow-x-auto",
                        table { class: "table hud-table table-zebra table-pin-rows",
                            thead { tr {
                                th { "名称" }
                                th { "类型" }
                                th { "角色" }
                                th { "模型 / 执行器" }
                                th { "状态" }
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
                                        let astatus = agent.status;
                                        let id_delete = id.clone();
                                        let id_onboard = id.clone();
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
                                                td { "data-label": "状态",
                                                    span { class: "{agent_status_badge_class(astatus)}",
                                                        "{agent_status_label(astatus)}"
                                                    }
                                                }
                                                td { "data-label": "操作",
                                                    // 入职按钮：仅对面试中/待入职的 Agent 显示
                                                    if astatus == AgentStatus::Interviewing as i32 || astatus == AgentStatus::PendingOnboard as i32 {
                                                        button { class: "btn hud-btn btn-success btn-sm",
                                                            onclick: move |_| handle_onboard(id_onboard.clone(), astatus),
                                                            "入职"
                                                        }
                                                    }
                                                    button { class: "btn hud-btn btn-error btn-sm",
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
                new_roles.set(Vec::new());
                new_roles_input.set(String::new());
                new_capabilities.set(Vec::new());
                new_capabilities_input.set(String::new());
                new_soul.set(String::new());
                new_model_provider_id.set(String::new());
                new_description.set(String::new());
            },
            footer: rsx! {
                button { class: "btn hud-btn btn-ghost", onclick: move |_| show_add_modal.set(false), "取消" }
                button { class: "btn hud-btn btn-primary", disabled: creating(), onclick: handle_create,
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
                        span { class: "label-text font-medium", "角色（多选）" }
                        span { class: "label-text-alt", "用于路由匹配，如前台/Web接待/代码专家 等" }
                    }
                    // 预设角色 chip
                    div { class: "flex flex-wrap gap-2 mb-2",
                        {
                            const PRESET_ROLES: &[(&str, &str)] = &[
                                ("reception", "Web前台接待"),
                                ("feishu_reception", "飞书前台接待"),
                                ("a2a_gateway", "A2A网关"),
                                ("hr_specialist", "人事专员"),
                                ("code_assistant", "代码助手"),
                            ];
                            PRESET_ROLES.iter().map(|(key, label)| {
                                let key_clone = key.to_string();
                                let selected = new_roles().iter().any(|r| r == key);
                                let cls = if selected {
                                    "btn btn-primary btn-sm"
                                } else {
                                    "btn btn-outline btn-sm"
                                };
                                rsx! {
                                    button { class: cls,
                                        onclick: move |_| {
                                            let mut v = new_roles();
                                            if let Some(pos) = v.iter().position(|x| x == key_clone.as_str()) {
                                                v.remove(pos);
                                            } else {
                                                v.push(key_clone.clone());
                                            }
                                            new_roles.set(v);
                                        },
                                        "{label}"
                                    }
                                }
                            })
                        }
                    }
                    // 自定义输入（回车/失焦添加）
                    div { class: "flex flex-wrap gap-2 items-center",
                        if !new_roles().is_empty() {
                            for role in new_roles() {
                                span { class: "badge hud-badge badge-accent badge-lg gap-1",
                                    "{role}",
                                    button { class: "btn hud-btn btn-ghost btn-xs",
                                        onclick: move |_| {
                                            let mut v = new_roles();
                                            if let Some(pos) = v.iter().position(|x| x == &role) {
                                                v.remove(pos);
                                            }
                                            new_roles.set(v);
                                        },
                                        "✕"
                                    }
                                }
                            }
                        }
                        input { class: "input input-bordered input-sm flex-1 min-w-[180px]",
                            value: "{new_roles_input}",
                            placeholder: "自定义角色，回车/逗号添加",
                            oninput: move |e| {
                                let val = e.value();
                                if let Some(comma_pos) = val.find(',') {
                                    let (head, rest) = val.split_at(comma_pos);
                                    let v = head.trim().to_string();
                                    if !v.is_empty() && !new_roles().iter().any(|r| r == v.as_str()) {
                                        let mut arr = new_roles();
                                        arr.push(v);
                                        new_roles.set(arr);
                                    }
                                    new_roles_input.set(rest[1..].trim().to_string());
                                } else {
                                    new_roles_input.set(val);
                                }
                            },
                            onkeydown: move |e| {
                                if e.key() == Key::Enter {
                                    e.prevent_default();
                                    let v = new_roles_input().trim().to_string();
                                    if !v.is_empty() && !new_roles().iter().any(|r| r == v.as_str()) {
                                        let mut arr = new_roles();
                                        arr.push(v);
                                        new_roles.set(arr);
                                    }
                                    new_roles_input.set(String::new());
                                }
                            }
                        }
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "能力关键词（多选，用于弱匹配）" }
                        span { class: "label-text-alt", "如：chat、code_search、task、knowledge 等" }
                    }
                    div { class: "flex flex-wrap gap-2 items-center",
                        if !new_capabilities().is_empty() {
                            for cap in new_capabilities() {
                                span { class: "badge hud-badge badge-success badge-lg gap-1",
                                    "{cap}",
                                    button { class: "btn hud-btn btn-ghost btn-xs",
                                        onclick: move |_| {
                                            let mut v = new_capabilities();
                                            if let Some(pos) = v.iter().position(|x| x == &cap) {
                                                v.remove(pos);
                                            }
                                            new_capabilities.set(v);
                                        },
                                        "✕"
                                    }
                                }
                            }
                        }
                        input { class: "input input-bordered input-sm flex-1 min-w-[180px]",
                            value: "{new_capabilities_input}",
                            placeholder: "自定义能力，回车/逗号添加",
                            oninput: move |e| {
                                let val = e.value();
                                if let Some(comma_pos) = val.find(',') {
                                    let (head, rest) = val.split_at(comma_pos);
                                    let v = head.trim().to_string();
                                    if !v.is_empty() && !new_capabilities().iter().any(|r| r == v.as_str()) {
                                        let mut arr = new_capabilities();
                                        arr.push(v);
                                        new_capabilities.set(arr);
                                    }
                                    new_capabilities_input.set(rest[1..].trim().to_string());
                                } else {
                                    new_capabilities_input.set(val);
                                }
                            },
                            onkeydown: move |e| {
                                if e.key() == Key::Enter {
                                    e.prevent_default();
                                    let v = new_capabilities_input().trim().to_string();
                                    if !v.is_empty() && !new_capabilities().iter().any(|r| r == v.as_str()) {
                                        let mut arr = new_capabilities();
                                        arr.push(v);
                                        new_capabilities.set(arr);
                                    }
                                    new_capabilities_input.set(String::new());
                                }
                            }
                        }
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "灵魂 / 系统提示词" }
                        span { class: "label-text-alt", "Agent 的深层人设 / 世界观 / 行为准则" }
                    }
                    textarea { class: "textarea textarea-bordered w-full", rows: 4,
                        value: "{new_soul}",
                        oninput: move |e| new_soul.set(e.value()),
                        placeholder: "你是一位资深的代码助手，习惯先分析需求再给出结构化建议..."
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "模型提供商" }
                        span { class: "label-text-alt", "可选：暂不选择则 Agent 处于面试中状态，配置对话模型后入职即可用" }
                    }
                    if model_providers.read().iter().filter(|mp| mp.capability.is_agent()).count() == 0 {
                        div { class: "flex flex-col gap-1",
                            input {
                                class: "input input-bordered w-full opacity-60",
                                value: "{new_model_provider_id}",
                                oninput: move |e| new_model_provider_id.set(e.value()),
                                placeholder: "暂无可用对话模型，可稍后在「模型提供商管理」中配置并绑定"
                            }
                            Link {
                                class: "link link-primary link-hover text-xs",
                                to: crate::pages::Route::FinanceModelProviders {},
                                "前往模型提供商管理 →"
                            }
                        }
                    } else {
                        select { class: "select select-bordered w-full", value: "{new_model_provider_id}",
                            onchange: move |e| new_model_provider_id.set(e.value()),
                            option { value: "", "-- 暂不绑定（面试中）--" }
                            for mp in model_providers.read().iter().filter(|mp| mp.capability.is_agent()) {
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
                        oninput: move |e| new_description.set(e.value()), placeholder: "Agent 描述（可选，用于列表展示）" }
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
                ext_roles.set(Vec::new());
                ext_roles_input.set(String::new());
                ext_capabilities.set(Vec::new());
                ext_capabilities_input.set(String::new());
                ext_soul.set(String::new());
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
                button { class: "btn hud-btn btn-ghost", onclick: move |_| show_external_modal.set(false), "取消" }
                button { class: "btn hud-btn btn-success", disabled: ext_creating(), onclick: handle_create_external,
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
                        span { class: "label-text font-medium", "角色（多选）" }
                        span { class: "label-text-alt", "用于路由匹配" }
                    }
                    div { class: "flex flex-wrap gap-2 mb-2",
                        {
                            const PRESET_ROLES: &[(&str, &str)] = &[
                                ("reception", "Web前台接待"),
                                ("feishu_reception", "飞书前台接待"),
                                ("a2a_gateway", "A2A网关"),
                                ("code_assistant", "代码助手"),
                            ];
                            PRESET_ROLES.iter().map(|(key, label)| {
                                let key_clone = key.to_string();
                                let selected = ext_roles().iter().any(|r| r == key);
                                let cls = if selected {
                                    "btn btn-success btn-sm"
                                } else {
                                    "btn btn-outline btn-sm"
                                };
                                rsx! {
                                    button { class: cls,
                                        onclick: move |_| {
                                            let mut v = ext_roles();
                                            if let Some(pos) = v.iter().position(|x| x == key_clone.as_str()) {
                                                v.remove(pos);
                                            } else {
                                                v.push(key_clone.clone());
                                            }
                                            ext_roles.set(v);
                                        },
                                        "{label}"
                                    }
                                }
                            })
                        }
                    }
                    div { class: "flex flex-wrap gap-2 items-center",
                        if !ext_roles().is_empty() {
                            for role in ext_roles() {
                                span { class: "badge hud-badge badge-accent badge-lg gap-1",
                                    "{role}",
                                    button { class: "btn hud-btn btn-ghost btn-xs",
                                        onclick: move |_| {
                                            let mut v = ext_roles();
                                            if let Some(pos) = v.iter().position(|x| x == &role) {
                                                v.remove(pos);
                                            }
                                            ext_roles.set(v);
                                        },
                                        "✕"
                                    }
                                }
                            }
                        }
                        input { class: "input input-bordered input-sm flex-1 min-w-[180px]",
                            value: "{ext_roles_input}",
                            placeholder: "自定义角色，回车/逗号添加",
                            oninput: move |e| {
                                let val = e.value();
                                if let Some(comma_pos) = val.find(',') {
                                    let (head, rest) = val.split_at(comma_pos);
                                    let v = head.trim().to_string();
                                    if !v.is_empty() && !ext_roles().iter().any(|r| r == v.as_str()) {
                                        let mut arr = ext_roles();
                                        arr.push(v);
                                        ext_roles.set(arr);
                                    }
                                    ext_roles_input.set(rest[1..].trim().to_string());
                                } else {
                                    ext_roles_input.set(val);
                                }
                            },
                            onkeydown: move |e| {
                                if e.key() == Key::Enter {
                                    e.prevent_default();
                                    let v = ext_roles_input().trim().to_string();
                                    if !v.is_empty() && !ext_roles().iter().any(|r| r == v.as_str()) {
                                        let mut arr = ext_roles();
                                        arr.push(v);
                                        ext_roles.set(arr);
                                    }
                                    ext_roles_input.set(String::new());
                                }
                            }
                        }
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "能力关键词（多选，用于弱匹配）" }
                    }
                    div { class: "flex flex-wrap gap-2 items-center",
                        if !ext_capabilities().is_empty() {
                            for cap in ext_capabilities() {
                                span { class: "badge hud-badge badge-success badge-lg gap-1",
                                    "{cap}",
                                    button { class: "btn hud-btn btn-ghost btn-xs",
                                        onclick: move |_| {
                                            let mut v = ext_capabilities();
                                            if let Some(pos) = v.iter().position(|x| x == &cap) {
                                                v.remove(pos);
                                            }
                                            ext_capabilities.set(v);
                                        },
                                        "✕"
                                    }
                                }
                            }
                        }
                        input { class: "input input-bordered input-sm flex-1 min-w-[180px]",
                            value: "{ext_capabilities_input}",
                            placeholder: "自定义能力，回车/逗号添加",
                            oninput: move |e| {
                                let val = e.value();
                                if let Some(comma_pos) = val.find(',') {
                                    let (head, rest) = val.split_at(comma_pos);
                                    let v = head.trim().to_string();
                                    if !v.is_empty() && !ext_capabilities().iter().any(|r| r == v.as_str()) {
                                        let mut arr = ext_capabilities();
                                        arr.push(v);
                                        ext_capabilities.set(arr);
                                    }
                                    ext_capabilities_input.set(rest[1..].trim().to_string());
                                } else {
                                    ext_capabilities_input.set(val);
                                }
                            },
                            onkeydown: move |e| {
                                if e.key() == Key::Enter {
                                    e.prevent_default();
                                    let v = ext_capabilities_input().trim().to_string();
                                    if !v.is_empty() && !ext_capabilities().iter().any(|r| r == v.as_str()) {
                                        let mut arr = ext_capabilities();
                                        arr.push(v);
                                        ext_capabilities.set(arr);
                                    }
                                    ext_capabilities_input.set(String::new());
                                }
                            }
                        }
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "灵魂 / 系统提示词" }
                    }
                    textarea { class: "textarea textarea-bordered w-full", rows: 3,
                        value: "{ext_soul}",
                        oninput: move |e| ext_soul.set(e.value()),
                        placeholder: "外部 Agent 的人设 / 行为准则（可选）"
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "描述" }
                    }
                    textarea { class: "textarea textarea-bordered w-full", value: "{ext_description}",
                        oninput: move |e| ext_description.set(e.value()), placeholder: "Agent 描述（可选，用于列表展示）" }
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
                        toast.error(format!("删除失败: {}", e));
                    } else {
                        load_data();
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
