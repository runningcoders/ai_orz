//! Agent 管理列表

use crate::components::hud::HudPanel;
use crate::components::hud::PageHeader;
use dioxus::prelude::*;

use crate::api::hr::{
    create_external_agent, delete_agent, list_agents, query_agents, search_agents,
    select_agent_career, update_agent_status,
};
use crate::api::seed::{get_task_progress, preview_preset_agents, sync_preset_agents};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::pages::hr::create_agent_modal::CreateAgentModal;
use crate::pages::hr::onboard_modal::OnboardModal;
use crate::store::toast::use_toast;
use crate::utils::status::{agent_lifecycle_badge, agent_lifecycle_text};
use common::api::seed::{
    PresetAgentSyncStrategy, SyncPresetAgentsRequest, SyncPresetAgentsResponse,
};
use common::api::{
    AgentQueryRequest, CreateExternalAgentRequest, ListAgentsRequest, ListAgentsResponseItem,
    PreviewPresetAgentsResponse, SearchAgentsRequest, SelectAgentCareerRequest,
    UpdateAgentStatusRequest,
};
use common::enums::AgentStatus;
use dioxus_router::Link;

/// Agent kind 对应的 badge 样式和标签
fn kind_badge_class(kind: &str) -> &'static str {
    // Agent 来源类型（local/cli/remote）是「类别标签」，统一走中性 orz-tag chip
    match kind {
        "local" => "badge orz-tag badge-sm",
        "cli" => "badge orz-tag badge-sm",
        "remote" => "badge orz-tag badge-sm",
        _ => "badge orz-tag badge-sm",
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

// Agent 生命周期状态文案/徽章统一走 `utils::status` 的 SSOT（agent_lifecycle_text /
// agent_lifecycle_badge），不再本地复制，避免与详情页、聊天侧栏的视觉口径漂移。

#[component]
pub fn HrAgents() -> Element {
    let mut agents = use_signal(Vec::<ListAgentsResponseItem>::new);
    let mut loading = use_signal(|| true);
    let toast = use_toast();
    let mut search_keyword = use_signal(String::new);

    // ===== 本地 Agent 创建 Modal（独立组件 CreateAgentModal，条件渲染）=====
    let mut show_add_modal = use_signal(|| false);
    // 入职弹窗（选包）：记录待入职的 Agent ID，非空即展示
    let mut onboard_agent_id = use_signal(|| None::<String>);

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
    });

    // ===== 生命周期推进：状态只是结果，动作发生在「边」上 =====
    // 初创 → 职业选择（按职业/能力匹配个人能力）
    // 面试中 → 通过面试（转入待入职，无副作用）
    // 待入职 → 入职（弹窗选包，安装组织要求的包）
    let handle_onboard = move |id: String, status: i32| {
        let status = AgentStatus::from(status);
        spawn(async move {
            match status {
                AgentStatus::Incubating => {
                    match select_agent_career(SelectAgentCareerRequest { id: id.clone() }).await {
                        Ok(_) => {
                            toast.success("职业生涯选择完成，已进入面试环节");
                            load_data();
                        }
                        Err(e) => toast.error(format!("职业选择失败: {}", e)),
                    }
                }
                AgentStatus::Interviewing => {
                    match update_agent_status(UpdateAgentStatusRequest {
                        id: id.clone(),
                        status: AgentStatus::PendingOnboard,
                        packs: None,
                    })
                    .await
                    {
                        Ok(_) => {
                            toast.success("已通过面试，转入待入职");
                            load_data();
                        }
                        Err(e) => toast.error(format!("转入待入职失败: {}", e)),
                    }
                }
                AgentStatus::PendingOnboard => {
                    // 入职需要选包 → 交给弹窗，不在这里直接提交
                    onboard_agent_id.set(Some(id));
                }
                _ => {}
            }
        });
    };

    // 稳定的关闭回调：关闭创建弹窗 + 刷新列表（use_callback 保证父重渲染时不重建，
    // 从而 CreateAgentModal 不会被无谓重渲染，切断「重渲染打断输入」卡死）
    let on_close_create = use_callback(move |_| {
        show_add_modal.set(false);
        load_data();
    });

    // 入职弹窗关闭：清空待入职 ID（卸载弹窗）+ 刷新列表
    let on_close_onboard = use_callback(move |_| {
        onboard_agent_id.set(None);
        load_data();
    });

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

    // ===== 预置 Agent 同步 Modal（seed 默认 Agent 的补缺 / 恢复误删 / 覆盖重置）=====
    let mut show_sync_modal = use_signal(|| false);
    let mut sync_preview = use_signal(|| None::<PreviewPresetAgentsResponse>);
    let mut sync_strategy = use_signal(|| PresetAgentSyncStrategy::Overwrite);
    let mut sync_loading = use_signal(|| false);
    let mut syncing = use_signal(|| false);
    let mut sync_progress = use_signal(String::new);

    // 打开弹窗即拉取预览（seed vs Agent 库逐项对比）
    let open_sync_modal = move |_| {
        show_sync_modal.set(true);
        sync_preview.set(None);
        sync_strategy.set(PresetAgentSyncStrategy::Overwrite);
        sync_progress.set(String::new());
        sync_loading.set(true);
        spawn(async move {
            match preview_preset_agents().await {
                Ok(v) => sync_preview.set(Some(v)),
                Err(e) => toast.error(format!("加载预置 Agent 清单失败: {}", e)),
            }
            sync_loading.set(false);
        });
    };

    // 确认同步：提交后台任务 → 轮询进度 → 完成后 toast 结果并刷新列表
    let confirm_sync = move |_| {
        let req = SyncPresetAgentsRequest {
            strategy: sync_strategy(),
        };
        syncing.set(true);
        sync_progress.set("正在提交同步任务...".to_string());
        spawn(async move {
            let task_id = match sync_preset_agents(req).await {
                Ok(r) => r.task_id,
                Err(e) => {
                    toast.error(format!("提交同步任务失败: {}", e));
                    syncing.set(false);
                    return;
                }
            };

            loop {
                gloo_timers::future::TimeoutFuture::new(500).await;
                match get_task_progress(&task_id).await {
                    Ok(p) => {
                        sync_progress.set(p.step_message.clone());
                        match p.status {
                            common::api::TaskStatus::Completed => {
                                let resp: Option<SyncPresetAgentsResponse> =
                                    p.result.and_then(|v| serde_json::from_value(v).ok());
                                match resp {
                                    Some(r) => toast.success(format!(
                                        "同步完成：新增 {}、恢复 {}、覆盖 {}、跳过 {}（seed 共 {} 项）",
                                        r.created, r.restored, r.updated, r.skipped, r.total
                                    )),
                                    None => toast.success("同步完成"),
                                }
                                show_sync_modal.set(false);
                                load_data();
                                break;
                            }
                            common::api::TaskStatus::Failed => {
                                toast.error(format!(
                                    "同步失败: {}",
                                    p.error.unwrap_or_else(|| "未知错误".to_string())
                                ));
                                break;
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        toast.error(format!("查询同步进度失败: {}", e));
                        break;
                    }
                }
            }
            syncing.set(false);
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
                    button { class: "btn hud-btn btn-ghost",
                        onclick: open_sync_modal,
                        "⟳ 同步预置 Agent"
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
                                option { value: "6", "初创" }
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
                                                    span { class: "{agent_lifecycle_badge(astatus)}",
                                                        "{agent_lifecycle_text(astatus)}"
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


        // ===== 本地 Agent 创建弹窗（独立组件，条件渲染：关闭即卸载重置）=====
        {show_add_modal().then(|| rsx! {
            CreateAgentModal { on_close: on_close_create }
        })}

        // ===== 入职弹窗（选包）：待入职 Agent 点击「入职」后弹出 =====
        {onboard_agent_id().map(|aid| rsx! {
            OnboardModal { key: "{aid}", agent_id: aid, on_close: on_close_onboard }
        })}

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
                                    "btn hud-btn btn-success btn-sm"
                                } else {
                                    "btn hud-btn btn-outline btn-sm"
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
                                span { class: "badge orz-tag badge-lg gap-1",
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
                                span { class: "badge orz-tag badge-lg gap-1",
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

        // 同步预置 Agent 弹窗（策略二选一 + 影响清单）
        Modal {
            title: "同步预置 Agent".to_string(),
            show: show_sync_modal(),
            width_class: Some("max-w-2xl".to_string()),
            on_close: move |_| show_sync_modal.set(false),
            footer: rsx! {
                button { class: "btn hud-btn btn-ghost", onclick: move |_| show_sync_modal.set(false), "取消" }
                button { class: "btn hud-btn btn-primary", disabled: syncing() || sync_loading(),
                    onclick: confirm_sync,
                    if syncing() { "同步中..." } else { "确认同步" }
                }
            },
            div { class: "space-y-4",
                // 策略选择（二选一）
                div { class: "grid gap-2",
                    div {
                        class: if sync_strategy() == PresetAgentSyncStrategy::Overwrite {
                            "card cursor-pointer border-2 border-primary bg-base-200 transition-colors"
                        } else {
                            "card cursor-pointer border border-base-300 bg-base-200 transition-colors"
                        },
                        onclick: move |_| sync_strategy.set(PresetAgentSyncStrategy::Overwrite),
                        div { class: "card-body p-3",
                            div { class: "font-semibold", "1 · 用 seed 覆盖重置" }
                            div { class: "text-xs text-base-content/70",
                                "在用的同 ID Agent 会把名称、角色、人设等基础信息覆写回默认值（生命周期状态、已装能力包与模型绑定不动）；缺失的自动创建；被误删的自动恢复"
                            }
                        }
                    }
                    div {
                        class: if sync_strategy() == PresetAgentSyncStrategy::OnlyMissing {
                            "card cursor-pointer border-2 border-primary bg-base-200 transition-colors"
                        } else {
                            "card cursor-pointer border border-base-300 bg-base-200 transition-colors"
                        },
                        onclick: move |_| sync_strategy.set(PresetAgentSyncStrategy::OnlyMissing),
                        div { class: "card-body p-3",
                            div { class: "font-semibold", "2 · 保留本地，仅补缺" }
                            div { class: "text-xs text-base-content/70",
                                "在用的 Agent 原样保留，只创建缺失的并恢复被误删的"
                            }
                        }
                    }
                }

                // 影响清单
                if sync_loading() {
                    Loading {}
                } else if let Some(preview) = sync_preview() {
                    {
                        // 汇总文案在 rsx 外计算：嵌套 if 表达式放进 format! 会破坏 rsx 解析
                        let overwrite = sync_strategy() == PresetAgentSyncStrategy::Overwrite;
                        let summary = format!(
                            "seed 共 {} 项：缺失 {} 项将新增，误删 {} 项将恢复，在用 {} 项{}",
                            preview.items.len(),
                            preview.missing_count,
                            preview.deleted_count,
                            preview.existing_count,
                            if overwrite { "将覆盖身份信息" } else { "将保留" }
                        );
                        rsx! {
                            div {
                                div { class: "text-sm mb-2", "{summary}" }
                                div { class: "max-h-56 overflow-y-auto",
                                    for item in preview.items.iter() {
                                        div { class: "flex items-center justify-between gap-2 py-1.5 border-b border-base-300 last:border-0",
                                            div { class: "min-w-0 flex-1",
                                                div { class: "text-sm font-medium truncate", "{item.name}" }
                                                div { class: "text-xs text-base-content/50 font-mono truncate", "{item.id}" }
                                            }
                                            div { class: "flex items-center gap-1 shrink-0",
                                                if item.exists {
                                                    // 模型绑定不在同步范围内：展示本地现状（保留）
                                                    if let Some(ref provider) = item.local_provider_name {
                                                        span { class: "badge orz-tag badge-sm", "模型 {provider} · 保留" }
                                                    } else {
                                                        span { class: "badge orz-tag badge-sm text-warning", "未配模型" }
                                                    }
                                                }
                                                if item.deleted {
                                                    span { class: "badge orz-tag badge-sm text-error", "误删待恢复" }
                                                } else if item.exists {
                                                    if overwrite {
                                                        span { class: "badge orz-tag badge-sm", "将覆盖" }
                                                    } else {
                                                        span { class: "badge orz-tag badge-sm", "将保留" }
                                                    }
                                                } else {
                                                    span { class: "badge orz-tag badge-sm", "将新增" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // 同步进度（后台任务执行中显示）
                if syncing() {
                    div { class: "flex items-center gap-2 text-sm text-base-content/70",
                        span { class: "loading loading-spinner loading-sm" }
                        span { "{sync_progress}" }
                    }
                }

                // 风险提示
                div { class: "text-xs text-base-content/50",
                    "注意：同步只处理基础身份信息，不改模型绑定——已有/恢复的 Agent 保留本地配置的模型；新建的 Agent 未绑模型、停留在初创态，请到 Agent 详情配置对话模型后完成职业匹配与入职。恢复的 Agent 会自动进修补齐能力包。"
                }
            }
        }
        }
    }
}
