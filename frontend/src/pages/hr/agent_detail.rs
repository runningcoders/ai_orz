use crate::api::finance::{list_model_providers, list_tool_tags, query_tools};
use crate::api::hr::*;
use crate::api::message::{load_latest_messages, send_message_to_agent};
use crate::api::project::{query_projects, query_tasks};
use crate::components::SearchableSelect;
use crate::components::chat::{MessageBubble, TypingIndicator};
use crate::components::modal::Modal;
use crate::components::relation_graph::{RelationGraph, RelationNodeInfo};
use crate::components::state::Loading;
use crate::components::stats::AgentStatsPanel;
use crate::components::workspace_graph::{WorkspaceGraph, WorkspaceView};
use crate::layouts::app_layout::AppLayout;
use crate::pages::hr::agent_memory_panel::AgentMemoryPanel;
use crate::store::toast::use_toast;
use crate::utils::{
    build_optimistic_user_msg, format_time_hm as format_time, replace_tmp_with_real,
};
use common::api::{
    AgentListItem, BindToolToAgentRequest, GetAgentRequest, GetAgentResponse,
    InstallSkillPackRequest, InstallSkillToAgentRequest, InstallToolPackRequest,
    ListModelProvidersResponseItem, ListToolsRequest, MessageListItem, PaginationParams,
    ProjectListItem, ProjectQueryRequest, SendMessageToAgentParams, SkillListItem,
    SkillQueryRequest, TaskListItem, TaskQueryRequest, ToolListItem, ToolQueryRequest,
    UnbindToolFromAgentRequest, UninstallSkillFromAgentRequest, UninstallSkillPackRequest,
    UninstallToolPackRequest, UpdateAgentRequest, UpdateAgentStatusRequest,
};
use common::enums::{AgentStatus, AssigneeType};
use dioxus::prelude::*;
use dioxus_router::{Link, use_navigator};
use std::collections::HashSet;
use wasm_bindgen::{JsCast, closure::Closure};

/// 构造带统计参数的 GetAgentRequest（4 处 get_agent 调用复用，避免重复 stats 字段字面量）
fn build_agent_stats_request(id: String) -> GetAgentRequest {
    GetAgentRequest {
        id,
        with_stats: Some(true),
        with_model_call_stats: Some(true),
        stats_time_start: None,
        stats_time_end: None,
        stats_interval: Some("daily".to_string()),
    }
}

fn binding_status_badge_class(is_bound: bool) -> &'static str {
    if is_bound {
        "badge badge-success"
    } else {
        "badge badge-ghost"
    }
}

fn agent_status_label(status: i32) -> String {
    match status {
        0 => "空闲".to_string(),
        1 => "思考中".to_string(),
        2 => "已入职".to_string(),
        3 => "休息中".to_string(),
        _ => status.to_string(),
    }
}

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
        "local" => "本地 Agent".to_string(),
        "cli" => "CLI Agent".to_string(),
        "remote" => "远程 Agent".to_string(),
        _ => kind.to_string(),
    }
}

const STATUS_OPTIONS: &[(i32, &str)] = &[(0, "空闲"), (1, "思考中"), (2, "已入职"), (3, "休息中")];

#[component]
pub fn HrAgentDetail(id: String) -> Element {
    let mut agent_data = use_signal(|| Option::<GetAgentResponse>::None);
    let mut messages = use_signal(Vec::<MessageListItem>::new);
    let mut is_typing = use_signal(|| false);
    // 修复 H3：为聊天输入框分离独立 signal，避免状态污染
    let mut input_message = use_signal(String::new);
    let toast = use_toast();
    let mut tool_packs = use_signal(Vec::<String>::new);
    let mut tool_tags = use_signal(Vec::<String>::new);
    let mut skill_packs = use_signal(Vec::<String>::new);
    let mut skill_tags = use_signal(Vec::<String>::new);
    // 技能包卸载确认对话框：存当前待卸载的 tag
    let mut show_skill_pack_uninstall_dialog = use_signal(|| None::<String>);
    let mut all_tools = use_signal(Vec::<ToolListItem>::new);
    // 工具搜索动态结果与加载状态（SearchableSelect 动态搜索模式）
    let mut tool_search_results = use_signal(Vec::<ToolListItem>::new);
    let mut tool_search_loading = use_signal(|| false);
    // 单个技能安装：搜索结果、加载状态、已安装技能列表
    let mut skill_search_results = use_signal(Vec::<SkillListItem>::new);
    let mut skill_search_loading = use_signal(|| false);
    let mut installed_skills = use_signal(Vec::<SkillListItem>::new);
    let mut show_edit_modal = use_signal(|| false);
    let mut edit_name = use_signal(String::new);
    let mut edit_roles = use_signal(String::new);
    let mut edit_description = use_signal(String::new);
    let mut edit_capabilities = use_signal(String::new);
    let mut edit_soul = use_signal(String::new);
    let mut edit_model_provider_id = use_signal(String::new);
    let mut saving_meta = use_signal(|| false);
    let mut model_providers = use_signal(Vec::<ListModelProvidersResponseItem>::new);
    // Tab 切换信号：0=概览 1=工具与技能 2=状态图 3=对话与记忆 4=关系图
    let mut active_tab = use_signal(|| 0usize);
    // 关系图所需数据：全局 projects + tasks + agents 列表
    let mut graph_projects = use_signal(Vec::<ProjectListItem>::new);
    let mut graph_tasks = use_signal(Vec::<TaskListItem>::new);
    let mut graph_agents = use_signal(Vec::<AgentListItem>::new);

    let agent_tool_ids = agent_data
        .read()
        .as_ref()
        .map(|a| a.tools.iter().cloned().collect::<HashSet<_>>())
        .unwrap_or_default();

    let skill_packs_list = skill_packs.read().clone();
    let tool_packs_list = tool_packs.read().clone();
    let all_tools_list = all_tools.read().clone();
    let installed_skills_list = installed_skills.read().clone();

    let id_for_load = id.clone();
    let load_data = move || {
        let aid = id_for_load.clone();
        spawn(async move {
            match get_agent(build_agent_stats_request(aid.clone())).await {
                Ok(a) => agent_data.set(Some(a)),
                Err(e) => toast.error(format!("获取 Agent 失败: {}", e)),
            }
            match list_installed_tool_packs(&aid).await {
                Ok(resp) => tool_packs.set(resp.installed_tags),
                Err(e) => toast.error(format!("获取工具包失败: {}", e)),
            }
            match list_tool_tags().await {
                Ok(resp) => tool_tags.set(resp.tags),
                Err(e) => toast.error(format!("获取工具包标签失败: {}", e)),
            }
            match list_installed_skill_packs(&aid).await {
                Ok(resp) => skill_packs.set(resp.skill_packs),
                Err(e) => toast.error(format!("获取技能包失败: {}", e)),
            }
            match list_skill_tags().await {
                Ok(resp) => skill_tags.set(resp.tags),
                Err(e) => toast.error(format!("获取技能包标签失败: {}", e)),
            }
            match list_agent_skills(&aid).await {
                Ok(resp) => installed_skills.set(resp.skills),
                Err(e) => toast.error(format!("获取已安装技能失败: {}", e)),
            }
            match list_tools(ListToolsRequest::default()).await {
                Ok(resp) => all_tools.set(resp.items),
                Err(e) => toast.error(format!("获取工具列表失败: {}", e)),
            }
            match load_latest_messages(common::api::ListMessagesRequest {
                limit: Some(50),
                ..Default::default()
            })
            .await
            {
                Ok(resp) => {
                    // 修复 HIGH #4：之前直接 set 全局消息，显示的是其他 Agent/用户的消息。
                    // 后端 /messages API 不支持 agent_id 过滤，前端按 to_id/from_id 客户端过滤。
                    // 取较多条数（50）后过滤，确保当前 agent 有足够历史。
                    let aid_for_filter = aid.clone();
                    let filtered: Vec<_> = resp
                        .messages
                        .into_iter()
                        .filter(|m| m.to_id == aid_for_filter || m.from_id == aid_for_filter)
                        .take(20)
                        .collect();
                    messages.set(filtered);
                }
                Err(e) => toast.error(format!("加载消息失败: {}", e)),
            }
            match list_model_providers().await {
                Ok(resp) => model_providers.set(resp.providers),
                Err(e) => toast.error(format!("加载模型提供商列表失败: {}", e)),
            }
            // 按需加载关系图数据（避免全量加载）
            // 1. 按 agent_id 过滤 tasks（assignee_type=Agent）
            let req = TaskQueryRequest {
                assignee_id: Some(aid.clone()),
                assignee_type: Some(AssigneeType::Agent),
                pagination: PaginationParams::default(),
                ..Default::default()
            };
            match query_tasks(&req).await {
                Ok(page) => {
                    let tasks = page.items;
                    // 2. 从 tasks 中收集 unique project_ids，批量查询消除 N+1
                    let project_ids: Vec<String> = tasks
                        .iter()
                        .filter_map(|t| t.project_id.clone())
                        .collect::<HashSet<_>>()
                        .into_iter()
                        .collect();
                    graph_tasks.set(tasks);

                    if project_ids.is_empty() {
                        graph_projects.set(Vec::new());
                    } else {
                        let req = ProjectQueryRequest {
                            ids: Some(project_ids),
                            pagination: PaginationParams::default(),
                            ..Default::default()
                        };
                        match query_projects(&req).await {
                            Ok(page) => graph_projects.set(page.items),
                            Err(e) => toast.error(format!("批量获取项目失败: {}", e)),
                        }
                    }
                }
                Err(e) => toast.error(format!("获取任务列表失败: {}", e)),
            }
            // 3. graph_agents 从当前 agent_data 构造（无需 API 调用）
            //    agent_data 已在上方加载完成
            if let Some(a) = agent_data.read().as_ref() {
                graph_agents.set(vec![AgentListItem::from(a)]);
            }
        });
    };

    use_effect(move || {
        load_data();
    });

    let sse_id = id.clone();

    use_effect(move || {
        // 修复 H5：EventSource::new 可能失败，不能 unwrap
        let event_source = match web_sys::EventSource::new("/api/v1/finance/messages/sse") {
            Ok(es) => es,
            Err(_) => {
                toast.error("SSE 连接初始化失败，实时消息将无法接收");
                return;
            }
        };
        let inner_id = sse_id.clone();
        let mut inner_messages = messages;
        let mut inner_is_typing = is_typing;

        let on_message = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
            let data = event.data().as_string().unwrap_or_default();
            let msg: MessageListItem = match serde_json::from_str(&data) {
                Ok(m) => m,
                Err(_) => return,
            };
            if msg.to_id == inner_id || msg.from_id == inner_id {
                let mut current = inner_messages.write();
                // 移除同 content 的乐观消息（统一使用 replace_tmp_with_real）
                replace_tmp_with_real(&mut current, &msg);
                if current.iter().any(|m| m.message_id == msg.message_id) {
                    return;
                }
                current.push(msg);
                inner_is_typing.set(false);
            }
        }) as Box<dyn FnMut(web_sys::MessageEvent)>);
        event_source.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        // 修复 H7：存储 Closure 避免 forget() 泄漏
        let on_message = Some(on_message);

        use_drop(move || {
            event_source.set_onmessage(None);
            drop(on_message);
            event_source.close();
        });
    });

    let id_for_send = id.clone();
    let handle_send = use_callback(move |_: ()| {
        let text = input_message().trim().to_string();
        if text.is_empty() {
            return;
        }
        let aid = id_for_send.clone();

        // 修复 M4（对齐 chat.rs）：保存输入快照，失败时恢复
        let text_snapshot = text.clone();
        input_message.set(String::new());
        is_typing.set(true);

        // 修复 M6（对齐 chat.rs）：is_typing 超时保护，防止 Agent 失败/SSE 断开时永久卡死
        spawn(async move {
            gloo_timers::future::TimeoutFuture::new(60_000).await;
            is_typing.set(false);
        });

        spawn(async move {
            let req = SendMessageToAgentParams {
                to_agent_id: Some(aid.clone()),
                content: text.clone(),
                project_id: None,
                task_id: None,
                reply_to_id: None,
                attachment_ids: None,
            };

            match send_message_to_agent(req).await {
                Ok(_) => {
                    // 构造乐观用户消息（统一使用 build_optimistic_user_msg）
                    let user_msg = build_optimistic_user_msg(text, None, None, Some(aid.clone()));
                    messages.write().push(user_msg);
                }
                Err(e) => {
                    // 修复 M4：失败时恢复用户输入
                    input_message.set(text_snapshot);
                    toast.error(format!("发送消息失败: {}", e));
                    is_typing.set(false);
                }
            }
        });
    });

    rsx! {
        AppLayout {
            {match agent_data.read().as_ref() {
                None => rsx! { Loading {} },
                Some(a) => {
            let capabilities = a.capabilities.clone().unwrap_or_default();
            let desc = a.description.as_deref().unwrap_or("");
            // 已绑定工具（用于卡片网格展示）：从全量工具列表中筛出 agent 已绑定的工具
            let bound_tools: Vec<ToolListItem> = all_tools_list
                .iter()
                .filter(|t| agent_tool_ids.contains(&t.id))
                .cloned()
                .collect();
            let agent_id_signal = use_signal(|| id.clone());
            // Tab 按钮动态 class：避免在 rsx! 格式串中嵌套引号转义
            let tab0_class = if active_tab() == 0 { "tab tab-lg tab-active" } else { "tab tab-lg" };
            let tab1_class = if active_tab() == 1 { "tab tab-lg tab-active" } else { "tab tab-lg" };
            let tab2_class = if active_tab() == 2 { "tab tab-lg tab-active" } else { "tab tab-lg" };
            let tab3_class = if active_tab() == 3 { "tab tab-lg tab-active" } else { "tab tab-lg" };
            let tab4_class = if active_tab() == 4 { "tab tab-lg tab-active" } else { "tab tab-lg" };

            rsx! {
                div { class: "card bg-base-100 shadow-md",
                    div { class: "card-body",
                        // 顶部标题 + 编辑按钮
                        div { class: "mb-6 flex justify-between items-start",
                            div {
                                h2 { class: "card-title", "{a.name}" }
                                p { class: "text-base-content/70 mt-1", "{desc}" }
                            }
                            button {
                                class: "btn btn-ghost btn-sm",
                                onclick: move |_| {
                                    if let Some(a) = agent_data.read().as_ref() {
                                        edit_name.set(a.name.clone());
                                        edit_roles.set(a.roles.join(", "));
                                        edit_description.set(a.description.clone().unwrap_or_default());
                                        edit_capabilities.set(a.capabilities.clone().unwrap_or_default().join(", "));
                                        edit_soul.set(a.soul.clone().unwrap_or_default());
                                        edit_model_provider_id.set(a.model_provider_id.clone());
                                        show_edit_modal.set(true);
                                    }
                                },
                                "✏️ 编辑"
                            }
                        }

                        // Tab 导航
                        div { class: "tabs tabs-boxed mb-6",
                            button {
                                class: "{tab0_class}",
                                onclick: move |_| active_tab.set(0),
                                "📋 概览"
                            }
                            button {
                                class: "{tab1_class}",
                                onclick: move |_| active_tab.set(1),
                                "🔧 工具与技能"
                            }
                            button {
                                class: "{tab2_class}",
                                onclick: move |_| active_tab.set(2),
                                "🎨 状态图"
                            }
                            button {
                                class: "{tab3_class}",
                                onclick: move |_| active_tab.set(3),
                                "💬 对话与记忆"
                            }
                            button {
                                class: "{tab4_class}",
                                onclick: move |_| active_tab.set(4),
                                "🕸️ 关系图"
                            }
                        }

                        // Tab 内容
                        {match active_tab() {
                            0 => rsx! {
                                // === 概览：基本信息 + 核心能力 + 运行时配置 + 状态切换 + 统计 ===
                                div { class: "mb-6",
                                    h3 { class: "text-lg font-semibold mb-3", "基本信息" }
                                    div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                                        div {
                                            span { class: "block text-sm text-base-content/70 mb-1", "ID" }
                                            span { class: "font-mono text-sm", "{a.id}" }
                                        }
                                        div {
                                            span { class: "block text-sm text-base-content/70 mb-1", "类型" }
                                            span { class: "{kind_badge_class(&a.kind)}",
                                                "{kind_label(&a.kind)}"
                                            }
                                        }
                                        div {
                                            span { class: "block text-sm text-base-content/70 mb-1", "状态" }
                                            span { class: "{binding_status_badge_class(a.status != 0)}",
                                                "{agent_status_label(a.status)}"
                                            }
                                        }
                                        if a.kind == "local" {
                                            div {
                                                span { class: "block text-sm text-base-content/70 mb-1", "模型提供商" }
                                                span { class: "font-mono text-sm", "{a.model_provider_id}" }
                                            }
                                        }
                                        div {
                                            span { class: "block text-sm text-base-content/70 mb-1", "创建时间" }
                                            span { class: "text-sm", "{format_time(a.created_at)}" }
                                        }
                                    }
                                }

                                div { class: "mb-6",
                                    h3 { class: "text-lg font-semibold mb-3", "核心能力" }
                                    if !capabilities.is_empty() {
                                        div { class: "flex flex-wrap gap-2",
                                            for cap in capabilities.iter() {
                                                span { class: "badge badge-info", "{cap}" }
                                            }
                                        }
                                    } else {
                                        div { class: "text-sm text-base-content/70", "暂无核心能力" }
                                    }
                                }

                                if a.kind != "local" {
                                    if let Some(ext_cfg) = &a.external_config {
                                        div { class: "mb-6",
                                            h3 { class: "text-lg font-semibold mb-3", "运行时配置" }
                                            if let Some(cli_cfg) = &ext_cfg.cli {
                                                div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                                                    div {
                                                        span { class: "block text-sm text-base-content/70 mb-1", "启动命令" }
                                                        span { class: "font-mono text-sm", "{cli_cfg.command}" }
                                                    }
                                                    if !cli_cfg.args.is_empty() {
                                                        div { class: "md:col-span-2",
                                                            span { class: "block text-sm text-base-content/70 mb-1", "命令参数" }
                                                            span { class: "font-mono text-sm",
                                                                "{cli_cfg.args.join(\" \")}"
                                                            }
                                                        }
                                                    }
                                                    div { class: "md:col-span-2",
                                                        span { class: "block text-sm text-base-content/70 mb-1", "工作目录" }
                                                        span { class: "font-mono text-sm", "{cli_cfg.work_dir}" }
                                                    }
                                                    div {
                                                        span { class: "block text-sm text-base-content/70 mb-1", "超时时间" }
                                                        span { class: "text-sm", "{cli_cfg.timeout_secs} 秒" }
                                                    }
                                                    if let Some(template) = &cli_cfg.prompt_template {
                                                        div { class: "md:col-span-2",
                                                            span { class: "block text-sm text-base-content/70 mb-1", "Prompt 模板" }
                                                            div { class: "p-3 bg-base-200 rounded-lg font-mono text-sm", "{template}" }
                                                        }
                                                    }
                                                }
                                            }
                                            if let Some(remote_cfg) = &ext_cfg.remote {
                                                div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                                                    div { class: "md:col-span-2",
                                                        span { class: "block text-sm text-base-content/70 mb-1", "A2A Server" }
                                                        span { class: "font-mono text-sm", "{remote_cfg.endpoint}" }
                                                    }
                                                    div {
                                                        span { class: "block text-sm text-base-content/70 mb-1", "目标 Agent" }
                                                        span { class: "font-mono text-sm", "{remote_cfg.agent_name}" }
                                                    }
                                                    div {
                                                        span { class: "block text-sm text-base-content/70 mb-1", "超时时间" }
                                                        span { class: "text-sm", "{remote_cfg.timeout_secs} 秒" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                div { class: "mb-6",
                                    h3 { class: "text-lg font-semibold mb-3", "状态切换" }
                                    div { class: "flex flex-wrap gap-2",
                                        for (status, label) in STATUS_OPTIONS {
                                            {
                                                let is_current = a.status == *status;
                                                let btn_class = if is_current { "btn btn-primary btn-sm" } else { "btn btn-ghost btn-sm" };
                                                let target_status_val = *status;
                                                let aid = agent_id_signal();
                                                let label_str = label.to_string();
                                                let label_for_closure = label_str.clone();
                                                rsx! {
                                                    button {
                                                        class: "{btn_class}",
                                                        disabled: is_current,
                                                        onclick: move |_| {
                                                            let agent_id = aid.clone();
                                                            let label_clone = label_for_closure.clone();
                                                            spawn(async move {
                                                                let status_req = UpdateAgentStatusRequest {
                                                                    id: agent_id.clone(),
                                                                    status: AgentStatus::from_i32(target_status_val),
                                                                };
                                                                match update_agent_status(status_req).await {
                                                                    Ok(_) => {
                                                                        toast.success(format!("状态已更新为：{}", label_clone));
                                                                        match get_agent(build_agent_stats_request(agent_id.clone())).await {
                                                                            Ok(a) => agent_data.set(Some(a)),
                                                                            Err(e) => toast.error(format!("刷新 Agent 失败: {}", e)),
                                                                        }
                                                                    }
                                                                    Err(e) => toast.error(format!("状态更新失败: {}", e)),
                                                                }
                                                            });
                                                        },
                                                        if is_current { "{label_str}（当前）" } else { "{label_str}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                if a.stats.is_some() || a.model_call_stats.is_some() {
                                    AgentStatsPanel {
                                        stats: a.stats.clone(),
                                        model_call_stats: a.model_call_stats.clone(),
                                    }
                                }
                            },
                            1 => rsx! {
                                // === 工具与技能：工具包 + 技能包 + 工具绑定 ===
                                div { class: "mb-6",
                                    h3 { class: "text-lg font-semibold mb-3", "工具包" }
                                    div { class: "flex gap-2 items-center mb-4",
                                        div { class: "flex-1",
                                            SearchableSelect {
                                                placeholder: "搜索工具包 tag...".to_string(),
                                                selected: None,
                                                options: tool_tags.read().clone(),
                                                on_select: move |tag: String| {
                                                    let aid = agent_id_signal();
                                                    spawn(async move {
                                                        match install_tool_pack(InstallToolPackRequest {
                                                            agent_id: aid.clone(),
                                                            tag: tag.clone(),
                                                        }).await {
                                                            Ok(_) => {
                                                                toast.success(format!("工具包 [{}] 已安装", tag));
                                                                match list_installed_tool_packs(&aid).await {
                                                                    Ok(resp) => tool_packs.set(resp.installed_tags),
                                                                    Err(e) => toast.error(format!("刷新工具包列表失败: {}", e)),
                                                                }
                                                            }
                                                            Err(e) => toast.error(format!("安装工具包失败: {}", e)),
                                                        }
                                                    });
                                                },
                                                on_search: None,
                                            }
                                        }
                                    }
                                    if !tool_packs_list.is_empty() {
                                        div { class: "flex flex-wrap gap-2",
                                            for tag in tool_packs_list.iter() {
                                                {
                                                    let tag_clone = tag.clone();
                                                    let aid = agent_id_signal();
                                                    rsx! {
                                                        span {
                                                            class: "badge badge-accent gap-1",
                                                            "{tag}"
                                                            button {
                                                                class: "badge-remove",
                                                                onclick: move |_| {
                                                                    let agent_id = aid.clone();
                                                                    let t = tag_clone.clone();
                                                                    spawn(async move {
                                                                        match uninstall_tool_pack(UninstallToolPackRequest { agent_id: agent_id.clone(), tag: t.clone() }).await {
                                                                            Ok(_) => {
                                                                                toast.success(format!("工具包 [{}] 已卸载", t));
                                                                                match list_installed_tool_packs(&agent_id).await {
                                                                                    Ok(resp) => tool_packs.set(resp.installed_tags),
                                                                                    Err(e) => toast.error(format!("刷新工具包列表失败: {}", e)),
                                                                                }
                                                                            }
                                                                            Err(e) => toast.error(format!("卸载工具包失败: {}", e)),
                                                                        }
                                                                    });
                                                                },
                                                                "×"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                div { class: "mb-6",
                                    h3 { class: "text-lg font-semibold mb-3", "技能包" }
                                    div { class: "flex gap-2 items-center mb-4",
                                        div { class: "flex-1",
                                            SearchableSelect {
                                                placeholder: "搜索技能包 tag...".to_string(),
                                                selected: None,
                                                options: skill_tags.read().clone(),
                                                on_select: move |tag: String| {
                                                    let aid = agent_id_signal();
                                                    spawn(async move {
                                                        match install_skill_pack(InstallSkillPackRequest { agent_id: aid.clone(), tag: tag.clone() }).await {
                                                            Ok(_) => {
                                                                toast.success(format!("技能包 [{}] 已安装", tag));
                                                                match list_installed_skill_packs(&aid).await {
                                                                    Ok(resp) => skill_packs.set(resp.skill_packs),
                                                                    Err(e) => toast.error(format!("刷新技能包列表失败: {}", e)),
                                                                }
                                                            }
                                                            Err(e) => toast.error(format!("安装技能包失败: {}", e)),
                                                        }
                                                    });
                                                },
                                                on_search: None,
                                            }
                                        }
                                    }
                                    if !skill_packs_list.is_empty() {
                                        div { class: "flex flex-wrap gap-2",
                                            for tag in skill_packs_list.iter() {
                                                {
                                                    let tag_clone = tag.clone();
                                                    rsx! {
                                                        span {
                                                            class: "badge badge-info gap-1",
                                                            "{tag}"
                                                            button {
                                                                class: "badge-remove",
                                                                onclick: move |_| {
                                                                    show_skill_pack_uninstall_dialog.set(Some(tag_clone.clone()));
                                                                },
                                                                "×"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // === 单个技能安装 ===
                                div { class: "mb-6",
                                    h3 { class: "text-lg font-semibold mb-3", "单个技能安装" }

                                    // 搜索框（动态搜索模式：on_search 回调调用 query_skills）
                                    div { class: "mb-4",
                                        SearchableSelect {
                                            placeholder: "搜索技能名称...".to_string(),
                                            selected: None,
                                            options: skill_search_results.read().iter().map(|s| {
                                                format!("{} ({})", s.name, s.id)
                                            }).collect(),
                                            on_select: move |selection: String| {
                                                // 从 "name (id)" 格式中提取 id
                                                if let Some(id_start) = selection.rfind('(') {
                                                    let skill_id = selection[id_start+1..selection.len()-1].to_string();
                                                    let aid = agent_id_signal();
                                                    spawn(async move {
                                                        match install_skill_to_agent(InstallSkillToAgentRequest {
                                                            agent_id: aid.clone(),
                                                            skill_id: skill_id.clone(),
                                                        }).await {
                                                            Ok(_) => {
                                                                toast.success("技能已安装");
                                                                match list_agent_skills(&aid).await {
                                                                    Ok(resp) => installed_skills.set(resp.skills),
                                                                    Err(e) => toast.error(format!("刷新失败: {}", e)),
                                                                }
                                                            }
                                                            Err(e) => toast.error(format!("安装失败: {}", e)),
                                                        }
                                                    });
                                                }
                                            },
                                            on_search: Some(EventHandler::new(move |keyword: String| {
                                                spawn(async move {
                                                    if keyword.trim().is_empty() {
                                                        skill_search_results.set(Vec::new());
                                                        return;
                                                    }
                                                    skill_search_loading.set(true);
                                                    let req = SkillQueryRequest {
                                                        keyword: Some(keyword),
                                                        ..Default::default()
                                                    };
                                                    match query_skills(&req).await {
                                                        Ok(resp) => skill_search_results.set(resp.items),
                                                        Err(_) => skill_search_results.set(Vec::new()),
                                                    }
                                                    skill_search_loading.set(false);
                                                });
                                            })),
                                            loading: *skill_search_loading.read(),
                                        }
                                    }

                                    // 已安装技能卡片网格（仅展示已安装技能，带「卸载」按钮）
                                    if !installed_skills_list.is_empty() {
                                        div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                                            for skill in installed_skills_list.iter() {
                                                {
                                                    let skill_clone = skill.clone();
                                                    let aid = agent_id_signal();
                                                    let skill_id = skill.id.clone();
                                                    let skill_name = skill.name.clone();
                                                    let skill_desc = skill.description.clone();
                                                    rsx! {
                                                        div {
                                                            class: "card bg-base-200",
                                                            key: "{skill_id}",
                                                            div { class: "card-body p-4",
                                                                div { class: "flex justify-between items-start",
                                                                    span { class: "font-medium", "{skill_name}" }
                                                                    span { class: "badge badge-success", "已安装" }
                                                                }
                                                                if !skill_desc.is_empty() {
                                                                    p { class: "text-sm text-base-content/70 mt-2", "{skill_desc}" }
                                                                }
                                                                div { class: "card-actions justify-end mt-3",
                                                                    button {
                                                                        class: "btn btn-error btn-sm",
                                                                        onclick: move |_| {
                                                                            let agent_id = aid.clone();
                                                                            let sid = skill_clone.id.clone();
                                                                            let sname = skill_clone.name.clone();
                                                                            spawn(async move {
                                                                                match uninstall_skill_from_agent(UninstallSkillFromAgentRequest {
                                                                                    agent_id: agent_id.clone(),
                                                                                    skill_id: sid.clone(),
                                                                                }).await {
                                                                                    Ok(_) => {
                                                                                        toast.success(format!("技能 {} 已卸载", sname));
                                                                                        match list_agent_skills(&agent_id).await {
                                                                                            Ok(resp) => installed_skills.set(resp.skills),
                                                                                            Err(e) => toast.error(format!("刷新失败: {}", e)),
                                                                                        }
                                                                                    }
                                                                                    Err(e) => toast.error(format!("卸载失败: {}", e)),
                                                                                }
                                                                            });
                                                                        },
                                                                        "卸载"
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    } else {
                                        div { class: "text-center py-12",
                                            div { class: "text-5xl mb-4 opacity-30", "🧩" }
                                            div { class: "text-base-content/70", "暂无已安装技能" }
                                        }
                                    }
                                }

                                div { class: "mb-6",
                                    h3 { class: "text-lg font-semibold mb-3", "工具绑定" }

                                    // 搜索框（动态搜索模式：on_search 回调调用 query_tools）
                                    div { class: "mb-4",
                                        SearchableSelect {
                                            placeholder: "搜索工具名称...".to_string(),
                                            selected: None,
                                            options: tool_search_results.read().iter().map(|t| {
                                                format!("{} ({})", t.name, t.id)
                                            }).collect(),
                                            on_select: move |selection: String| {
                                                // 从 "name (id)" 格式中提取 id
                                                if let Some(id_start) = selection.rfind('(') {
                                                    let tool_id = selection[id_start+1..selection.len()-1].to_string();
                                                    let aid = agent_id_signal();
                                                    spawn(async move {
                                                        match bind_tool_to_agent(BindToolToAgentRequest {
                                                            agent_id: aid.clone(),
                                                            tool_id: tool_id.clone(),
                                                        }).await {
                                                            Ok(_) => {
                                                                toast.success("工具已绑定");
                                                                match get_agent(build_agent_stats_request(aid.clone())).await {
                                                                    Ok(a) => agent_data.set(Some(a)),
                                                                    Err(e) => toast.error(format!("刷新 Agent 失败: {}", e)),
                                                                }
                                                            }
                                                            Err(e) => toast.error(format!("绑定失败: {}", e)),
                                                        }
                                                    });
                                                }
                                            },
                                            on_search: Some(EventHandler::new(move |keyword: String| {
                                                spawn(async move {
                                                    if keyword.trim().is_empty() {
                                                        tool_search_results.set(Vec::new());
                                                        return;
                                                    }
                                                    tool_search_loading.set(true);
                                                    let req = ToolQueryRequest {
                                                        keyword: Some(keyword),
                                                        enabled_only: Some(true),
                                                        ..Default::default()
                                                    };
                                                    match query_tools(&req).await {
                                                        Ok(resp) => tool_search_results.set(resp.items),
                                                        Err(_) => tool_search_results.set(Vec::new()),
                                                    }
                                                    tool_search_loading.set(false);
                                                });
                                            })),
                                            loading: *tool_search_loading.read(),
                                        }
                                    }

                                    // 已绑定工具卡片网格（仅展示已绑定工具，带「解绑」按钮）
                                    if !bound_tools.is_empty() {
                                        div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                                            for tool in bound_tools.iter() {
                                                {
                                                    let tool_clone = tool.clone();
                                                    let aid = agent_id_signal();
                                                    let tool_id = tool.id.clone();
                                                    let tool_name = tool.name.clone();
                                                    let tool_desc = tool.description.as_deref().unwrap_or("");
                                                    let tags = tool.tags.clone();
                                                    rsx! {
                                                        div {
                                                            class: "card bg-base-200",
                                                            key: "{tool_id}",
                                                            div { class: "card-body p-4",
                                                                div { class: "flex justify-between items-start",
                                                                    span { class: "font-medium", "{tool_name}" }
                                                                    span { class: "badge badge-success", "已绑定" }
                                                                }
                                                                p { class: "text-sm text-base-content/70 mt-2", "{tool_desc}" }
                                                                if !tags.is_empty() {
                                                                    div { class: "flex flex-wrap gap-1 mt-2",
                                                                        for tag in tags.iter() {
                                                                            span { class: "badge badge-ghost", "{tag}" }
                                                                        }
                                                                    }
                                                                }
                                                                div { class: "card-actions justify-end mt-3",
                                                                    button {
                                                                        class: "btn btn-error btn-sm",
                                                                        onclick: move |_| {
                                                                            let agent_id = aid.clone();
                                                                            let tid = tool_clone.id.clone();
                                                                            let tname = tool_clone.name.clone();
                                                                            spawn(async move {
                                                                                match unbind_tool_from_agent(UnbindToolFromAgentRequest { agent_id: agent_id.clone(), tool_id: tid.clone() }).await {
                                                                                    Ok(_) => {
                                                                                        toast.success(format!("工具 {} 已解绑", tname));
                                                                                        match get_agent(build_agent_stats_request(agent_id.clone())).await {
                                                                                            Ok(a) => agent_data.set(Some(a)),
                                                                                            Err(e) => toast.error(format!("刷新 Agent 失败: {}", e)),
                                                                                        }
                                                                                    }
                                                                                    Err(e) => toast.error(format!("解绑失败: {}", e)),
                                                                                }
                                                                            });
                                                                        },
                                                                        "解绑"
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    } else {
                                        div { class: "text-center py-12",
                                            div { class: "text-5xl mb-4 opacity-30", "🔧" }
                                            div { class: "text-base-content/70", "暂无已绑定工具" }
                                        }
                                    }
                                }
                            },
                            2 => rsx! {
                                // === 状态图：Agent 与绑定 Tools 的关系图 ===
                                {
                                    let bound_tool_infos: Vec<RelationNodeInfo> = all_tools_list.iter()
                                        .filter(|t| agent_tool_ids.contains(&t.id))
                                        .map(|t| RelationNodeInfo::with_kind(
                                            t.id.clone(),
                                            t.name.clone(),
                                            "tool",
                                        ))
                                        .collect();
                                    let navigator = use_navigator();
                                    rsx! {
                                        RelationGraph {
                                            center_id: a.id.clone(),
                                            center_name: a.name.clone(),
                                            center_color: "#fa520f".to_string(),
                                            center_kind: Some("agent".to_string()),
                                            related: bound_tool_infos,
                                            related_color: "#f59e0b".to_string(),
                                            related_label: "工具".to_string(),
                                            on_node_click: Some(EventHandler::new(move |evt: crate::components::relation_graph::NodeClickEvent| {
                                                if evt.is_center {
                                                    // 点击中心 Agent 节点，不跳转（已在当前页）
                                                    return;
                                                }
                                                if evt.kind.as_deref() == Some("tool") {
                                                    navigator.push(format!("/finance/tools/{}", evt.id));
                                                }
                                            })),
                                        }
                                    }
                                }
                            },
                            3 => rsx! {
                                // === 对话与记忆 ===
                                div { class: "mb-6",
                                    h3 { class: "text-lg font-semibold mb-3", "对话" }
                                    div { class: "agent-chat-messages",
                                        if messages().is_empty() && !is_typing() {
                                            div { class: "text-center py-12",
                                                div { class: "text-5xl mb-4 opacity-30", "💬" }
                                                div { class: "text-base-content/70", "暂无对话记录，发送消息开始对话" }
                                            }
                                        } else {
                                            for msg in messages().iter().cloned() {
                                                MessageBubble { msg: msg.clone(), key: "{msg.message_id}" }
                                            }
                                            if is_typing() {
                                                TypingIndicator {}
                                            }
                                        }
                                    }
                                    div { class: "flex gap-2 mt-4",
                                        input {
                                            class: "input input-bordered flex-1",
                                            r#type: "text",
                                            placeholder: "输入消息...",
                                            value: input_message,
                                            oninput: move |e| input_message.set(e.value().clone()),
                                            onkeydown: move |e| {
                                                if e.key() == Key::Enter {
                                                    e.prevent_default();
                                                    handle_send(());
                                                }
                                            },
                                        }
                                        button {
                                            class: "btn btn-primary",
                                            onclick: move |_| handle_send(()),
                                            "发送"
                                        }
                                    }
                                }

                                div { class: "mb-6",
                                    h3 { class: "text-lg font-semibold mb-3", "记忆" }
                                    AgentMemoryPanel { agent_id: Some(id.clone()) }
                                }
                            },
                            4 => rsx! {
                                div { class: "card bg-base-100 shadow-md",
                                    div { class: "card-header",
                                        h2 { class: "card-title", "关系图" }
                                    }
                                    div { class: "p-4",
                                        WorkspaceGraph {
                                            view: WorkspaceView::AgentDetail(a.id.clone()),
                                            projects: graph_projects.read().clone(),
                                            agents: graph_agents.read().clone(),
                                            tasks: graph_tasks.read().clone(),
                                            width: 800.0,
                                            height: 500.0,
                                        }
                                    }
                                }
                            },
                            _ => rsx! {},
                        }}

                        // 返回列表按钮
                        div { class: "card-actions mt-6",
                            Link { to: "/hr/agents", class: "btn btn-ghost", "返回列表" }
                        }

                        Modal {
                            title: "编辑 Agent 基本信息".to_string(),
                            show: show_edit_modal(),
                            on_close: move |_| show_edit_modal.set(false),
                            footer: rsx! {
                                button { class: "btn btn-ghost", onclick: move |_| show_edit_modal.set(false), "取消" }
                                button {
                                    class: "btn btn-primary",
                                    disabled: saving_meta(),
                                    onclick: {
                                        let id_for_submit = id.clone();
                                        move |_| {
                                            let name = edit_name().trim().to_string();
                                            if name.is_empty() {
                                                toast.error("名称不能为空");
                                                return;
                                            }
                                            let roles: Vec<String> = edit_roles()
                                                .split(',')
                                                .map(|s| s.trim().to_string())
                                                .filter(|s| !s.is_empty())
                                                .collect();
                                            let capabilities: Vec<String> = edit_capabilities()
                                                .split(',')
                                                .map(|s| s.trim().to_string())
                                                .filter(|s| !s.is_empty())
                                                .collect();
                                            let soul = if edit_soul().trim().is_empty() { None } else { Some(edit_soul()) };
                                            let mp_id = if edit_model_provider_id().is_empty() { None } else { Some(edit_model_provider_id()) };
                                            let req = UpdateAgentRequest {
                                                id: id_for_submit.clone(),
                                                name: Some(name),
                                                roles: Some(roles),
                                                description: Some(edit_description()),
                                                capabilities: Some(capabilities),
                                                soul,
                                                model_provider_id: mp_id,
                                            };
                                            saving_meta.set(true);
                                            let id_clone = id_for_submit.clone();
                                            spawn(async move {
                                                match update_agent(req).await {
                                                    Ok(_) => {
                                                        toast.success("Agent 信息已更新");
                                                        show_edit_modal.set(false);
                                                        match get_agent(build_agent_stats_request(id_clone.clone())).await {
                                                            Ok(a) => agent_data.set(Some(a)),
                                                            Err(e) => toast.error(format!("重新加载失败: {}", e)),
                                                        }
                                                    }
                                                    Err(e) => toast.error(format!("更新失败: {}", e)),
                                                }
                                                saving_meta.set(false);
                                            });
                                        }
                                    },
                                    if saving_meta() { "保存中..." } else { "保存" }
                                }
                            },
                            div { class: "space-y-4",
                                div { class: "form-control w-full",
                                    label { class: "label", span { class: "label-text font-medium", "名称 *" } }
                                    input { class: "input input-bordered w-full", value: "{edit_name}",
                                        oninput: move |e| edit_name.set(e.value()), placeholder: "Agent 名称" }
                                }
                                div { class: "form-control w-full",
                                    label { class: "label", span { class: "label-text font-medium", "描述" } }
                                    textarea { class: "textarea textarea-bordered w-full", value: "{edit_description}",
                                        oninput: move |e| edit_description.set(e.value()) }
                                }
                                div { class: "form-control w-full",
                                    label { class: "label", span { class: "label-text font-medium", "角色（逗号分隔）" } }
                                    input { class: "input input-bordered w-full", value: "{edit_roles}",
                                        oninput: move |e| edit_roles.set(e.value()), placeholder: "assistant, coder" }
                                }
                                div { class: "form-control w-full",
                                    label { class: "label", span { class: "label-text font-medium", "能力（逗号分隔）" } }
                                    input { class: "input input-bordered w-full", value: "{edit_capabilities}",
                                        oninput: move |e| edit_capabilities.set(e.value()), placeholder: "text, vision" }
                                }
                                div { class: "form-control w-full",
                                    label { class: "label", span { class: "label-text font-medium", "灵魂提示词 (Soul)" } }
                                    textarea { class: "textarea textarea-bordered w-full", value: "{edit_soul}",
                                        oninput: move |e| edit_soul.set(e.value()), rows: "4" }
                                }
                                div { class: "form-control w-full",
                                    label { class: "label", span { class: "label-text font-medium", "模型提供商" } }
                                    select {
                                        class: "select select-bordered w-full",
                                        value: "{edit_model_provider_id}",
                                        onchange: move |e| edit_model_provider_id.set(e.value()),
                                        option { value: "", "（不绑定）" }
                                        for p in model_providers.read().iter() {
                                            option { value: "{p.id}", "{p.name}" }
                                        }
                                    }
                                }
                            }
                        }

                        // 技能包卸载确认对话框
                        if let Some(tag) = show_skill_pack_uninstall_dialog.read().as_ref() {
                            {
                            let tag_a = tag.clone();
                            let tag_b = tag.clone();
                            rsx! {
                            div {
                                class: "modal modal-open",
                                onclick: move |_| show_skill_pack_uninstall_dialog.set(None),
                                div {
                                    class: "modal-box",
                                    onclick: move |e| e.stop_propagation(),
                                    h3 { class: "font-bold text-lg mb-2", "卸载技能包" }
                                    p { class: "text-sm text-base-content/70 mb-4",
                                        "即将卸载技能包 [{tag_a}]，请选择卸载方式："
                                    }
                                    div { class: "flex flex-col gap-3",
                                        // 选项 A：仅移除关联
                                        button {
                                            class: "btn btn-ghost justify-start text-left",
                                            onclick: move |_| {
                                                let aid = agent_id_signal();
                                                let t = tag_a.clone();
                                                show_skill_pack_uninstall_dialog.set(None);
                                                spawn(async move {
                                                    match uninstall_skill_pack(UninstallSkillPackRequest { agent_id: aid.clone(), tag: t.clone(), delete_copies: Some(false) }).await {
                                                        Ok(_) => {
                                                            toast.success(format!("技能包 [{}] 已卸载（保留副本）", t));
                                                            match list_installed_skill_packs(&aid).await {
                                                                Ok(resp) => skill_packs.set(resp.skill_packs),
                                                                Err(e) => toast.error(format!("刷新失败: {}", e)),
                                                            }
                                                        }
                                                        Err(e) => toast.error(format!("卸载失败: {}", e)),
                                                    }
                                                });
                                            },
                                            div {
                                                p { class: "font-medium", "仅移除关联" }
                                                p { class: "text-xs text-base-content/50", "移除 tag 标记，保留 Agent 侧技能副本" }
                                            }
                                        }
                                        // 选项 B：同时删除副本
                                        button {
                                            class: "btn btn-error btn-outline justify-start text-left",
                                            onclick: move |_| {
                                                let aid = agent_id_signal();
                                                let t = tag_b.clone();
                                                show_skill_pack_uninstall_dialog.set(None);
                                                spawn(async move {
                                                    match uninstall_skill_pack(UninstallSkillPackRequest { agent_id: aid.clone(), tag: t.clone(), delete_copies: Some(true) }).await {
                                                        Ok(_) => {
                                                            toast.success(format!("技能包 [{}] 已卸载（含副本删除）", t));
                                                            match list_installed_skill_packs(&aid).await {
                                                                Ok(resp) => skill_packs.set(resp.skill_packs),
                                                                Err(e) => toast.error(format!("刷新失败: {}", e)),
                                                            }
                                                        }
                                                        Err(e) => toast.error(format!("卸载失败: {}", e)),
                                                    }
                                                });
                                            },
                                            div {
                                                p { class: "font-medium", "移除关联 + 删除副本" }
                                                p { class: "text-xs text-error/70",
                                                    "⚠ Agent 技能可能已经进化（修改过内容），删除后无法恢复"
                                                }
                                            }
                                        }
                                    }
                                    div { class: "modal-action",
                                        button {
                                            class: "btn btn-ghost",
                                            onclick: move |_| show_skill_pack_uninstall_dialog.set(None),
                                            "取消"
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
            }}
        }
    }
}
