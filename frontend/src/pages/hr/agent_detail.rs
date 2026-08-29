use crate::api::finance::{list_model_providers, list_tool_tags, query_tools};
use crate::api::hr::*;
use crate::api::message::{load_older_messages, poll_new_messages, send_message_to_agent};
use crate::api::project::{query_projects, query_tasks};
use crate::components::SearchableSelect;
use crate::components::chat::{MessageBubble, TypingIndicator};
use crate::components::hud::HudPanel;
use crate::components::markdown::MarkdownRenderer;
use crate::components::modal::Modal;
use crate::components::relation_graph::{RelationGraph, RelationNodeInfo};
use crate::components::state::{EmptyState, Loading};
use crate::components::stats::AgentStatsPanel;
use crate::components::workspace_graph::{WorkspaceGraph, WorkspaceView};
use crate::layouts::app_layout::AppLayout;
use crate::pages::hr::agent_memory_panel::AgentMemoryPanel;
use crate::pages::hr::knowledge_graph::KnowledgeGraph;
use crate::store::toast::use_toast;
use crate::utils::{
    build_optimistic_user_msg, format_time_hm as format_time, replace_tmp_with_real,
    status::tag_chip,
};
use common::api::{
    AgentListItem, AgentRuntimeConfigInfo, BindToolToAgentRequest, GetAgentRequest,
    InstallSkillPackRequest, InstallSkillToAgentRequest, InstallToolPackRequest,
    ListMessagesRequest, ListModelProvidersResponseItem, ListToolsRequest, MessageListItem,
    PaginationParams, ProjectListItem, ProjectQueryRequest, RuntimeReady, SendMessageToAgentParams,
    SkillListItem, SkillQueryRequest, TaskListItem, TaskQueryRequest, ToolListItem,
    ToolQueryRequest, UnbindToolFromAgentRequest, UninstallSkillFromAgentRequest,
    UninstallSkillPackRequest, UninstallToolPackRequest, UpdateAgentRequest,
    UpdateAgentStatusRequest,
};
use common::enums::{AgentStatus, AssigneeType};
use dioxus::prelude::*;
use dioxus_router::{Link, use_navigator};
use std::collections::HashSet;

/// 构造带统计参数的 GetAgentRequest（4 处 get_agent 调用复用，避免重复 stats 字段字面量）
/// 详情页请求的字段开关。
///
/// 详情页需要展示工具/技能全景（三分组），故显式打开 `with_tools` / `with_skills`；
/// 其余调用方（如聊天侧面板）使用 `..Default::default()`，不会装配全景数据。
fn build_agent_stats_request(id: String) -> GetAgentRequest {
    GetAgentRequest {
        id,
        with_stats: Some(true),
        with_model_call_stats: Some(true),
        stats_time_start: None,
        stats_time_end: None,
        stats_interval: Some("daily".to_string()),
        with_tools: Some(true),
        with_skills: Some(true),
    }
}

fn binding_status_badge_class(is_bound: bool) -> &'static str {
    if is_bound {
        "badge hud-badge badge-success"
    } else {
        "badge hud-badge badge-ghost"
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
        "local" => "本地 Agent".to_string(),
        "cli" => "CLI Agent".to_string(),
        "remote" => "远程 Agent".to_string(),
        _ => kind.to_string(),
    }
}

const STATUS_OPTIONS: &[(i32, &str)] = &[
    (0, "已删除"),
    (1, "面试中"),
    (2, "待入职"),
    (3, "已入职"),
    (4, "已离职"),
    (5, "待离职"),
];

/// 消息流单页条数（双向查询各取 PAGE_SIZE，合并去重后取最新的 PAGE_SIZE 条）
const MSG_PAGE_SIZE: usize = 20;

/// 发送后等待 Agent 回复的轮询节奏：最多 20 次 × 3s ≈ 60s，与 is_typing 超时保护一致
const REPLY_POLL_MAX: usize = 20;
const REPLY_POLL_INTERVAL_MS: u32 = 3_000;

/// 拉取「与指定 Agent 相关」的消息（双向 OR 查询 + 合并去重）
///
/// 后端 `/api/v1/finance/messages` 的 `from_id` 与 `to_id` 是 **AND** 关系，
/// 而「与该 Agent 相关」语义是 `from_id == aid **OR** to_id == aid`，
/// 因此分两次查询（`from_id=aid` / `to_id=aid`）后按 `message_id` 去重、
/// 按 `created_at` 升序合并。这样翻页（before_timestamp）也是准确的，
/// 不像旧实现那样「拉 50 条再客户端过滤」（过滤后可能不足一页，且翻页会漏消息）。
///
/// `before` 为 `None` 时取最新一页；否则取早于该时间戳的一页。
async fn fetch_agent_messages(
    aid: &str,
    before: Option<i64>,
) -> Result<Vec<MessageListItem>, crate::api::ApiError> {
    let base = ListMessagesRequest {
        limit: Some(MSG_PAGE_SIZE),
        before_timestamp: before,
        ..Default::default()
    };

    let from_req = ListMessagesRequest {
        from_id: Some(aid.to_string()),
        ..base.clone()
    };
    let to_req = ListMessagesRequest {
        to_id: Some(aid.to_string()),
        ..base
    };

    // 两次查询顺序 await（同一后端，串行开销可接受，避免引入额外并发依赖）
    let sent = load_older_messages(from_req).await?.messages;
    let received = load_older_messages(to_req).await?.messages;

    let mut merged: Vec<MessageListItem> = Vec::with_capacity(sent.len() + received.len());
    let mut seen: HashSet<String> = HashSet::new();
    for m in sent.into_iter().chain(received) {
        if seen.insert(m.message_id.clone()) {
            merged.push(m);
        }
    }
    merged.sort_by_key(|m| m.created_at);
    // 双向各取 PAGE_SIZE，合并后可能到 2×PAGE_SIZE，只保留最新的 PAGE_SIZE 条
    if merged.len() > MSG_PAGE_SIZE {
        merged.drain(..merged.len() - MSG_PAGE_SIZE);
    }
    Ok(merged)
}

#[component]
pub fn HrAgentDetail(id: String) -> Element {
    // 方案 B：订阅路由并把 id 同步到响应式 rid，use_resource 绑定 rid，
    // 拉取仅在 id 变化时触发（同变体 /hr/agents/A → /hr/agents/B 也会重拉）
    let route = dioxus_router::use_route::<crate::pages::Route>();
    let mut rid = use_signal(String::new);
    if let crate::pages::Route::HrAgentDetail { id: route_id } = &route
        && *rid.peek() != *route_id
    {
        rid.set(route_id.clone());
    }
    let mut agent_res = use_resource(move || {
        let id = rid();
        async move { get_agent(build_agent_stats_request(id)).await }
    });
    let mut messages = use_signal(Vec::<MessageListItem>::new);
    // 消息流分页状态（后端分页拉取，不依赖 SSE）
    let mut msg_loading = use_signal(|| false);
    let mut msg_loading_more = use_signal(|| false);
    let mut msg_has_more = use_signal(|| true);
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
    let mut edit_roles = use_signal(Vec::<String>::new);
    let mut edit_roles_input = use_signal(String::new);
    let mut edit_description = use_signal(String::new);
    let mut edit_capabilities = use_signal(Vec::<String>::new);
    let mut edit_capabilities_input = use_signal(String::new);
    let mut edit_soul = use_signal(String::new);
    let mut edit_model_provider_id = use_signal(String::new);
    // 运行时配置编辑字段（number input 用 String 承载，提交时 parse）
    let mut edit_max_thinking_rounds = use_signal(String::new);
    let mut edit_intent_analyze_max_rounds = use_signal(String::new);
    let mut edit_summary_max_rounds = use_signal(String::new);
    let mut edit_think_timeout_secs = use_signal(String::new);
    let mut saving_meta = use_signal(|| false);
    let mut model_providers = use_signal(Vec::<ListModelProvidersResponseItem>::new);
    // Tab 切换信号：0=概览 1=工具与技能 2=状态图 3=对话与记忆 4=关系图
    let mut active_tab = use_signal(|| 0usize);
    // 状态切换/入职操作的 Agent ID（必须在顶层声明，不能在渲染分支中调用 use_signal）
    // 克隆一份避免 move 走组件参数 id（后续多处仍使用 id.clone()）
    let agent_id_for_signal = id.clone();
    let agent_id_signal = use_signal(move || agent_id_for_signal);
    // 关系图所需数据：全局 projects + tasks + agents 列表
    let mut graph_projects = use_signal(Vec::<ProjectListItem>::new);
    let mut graph_tasks = use_signal(Vec::<TaskListItem>::new);
    let graph_agents = use_signal(Vec::<AgentListItem>::new);

    let skill_packs_list = skill_packs.read().clone();
    let tool_packs_list = tool_packs.read().clone();

    let rid_for_load = rid;
    let load_data = move || {
        let aid = rid_for_load();
        spawn(async move {
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
            // 消息流：后端分页拉取，只取与本 Agent 相关的信息流
            // （from_id=aid OR to_id=aid，见 fetch_agent_messages 的双向查询说明）
            msg_loading.set(true);
            match fetch_agent_messages(&aid, None).await {
                Ok(page) => {
                    msg_has_more.set(page.len() >= MSG_PAGE_SIZE);
                    messages.set(page);
                }
                Err(e) => toast.error(format!("加载消息失败: {}", e)),
            }
            msg_loading.set(false);
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
        });
    };

    use_effect(move || {
        let _ = rid();
        load_data();
    });

    // graph_agents 随 agent_res 完成而同步（load_data 并发执行，避免读取不到 agent）
    let agent_res_eff = agent_res;
    let mut graph_agents_eff = graph_agents;
    use_effect(move || {
        if let Some(Ok(a)) = agent_res_eff.read().as_ref() {
            graph_agents_eff.set(vec![AgentListItem::from(a)]);
        }
    });

    // === 消息流：后端分页拉取（不使用 SSE）===
    //
    // Agent 详情页的对话定位是「信息流回看」，不需要实时推送：
    // SSE 会常驻一条连接、且让消息列表与实时事件耦合（断连/重连都要处理）。
    // 改为：进入页面拉最新一页，点「加载更早」按 before_timestamp 向前翻页，
    // 发送消息后短时轮询拉取 Agent 回复（见 handle_send）。

    // 上拉加载更早消息。
    // 捕获 `rid`（Signal，Copy）而非 `id`（String）—— 让闭包保持 Copy，
    // 才能在「加载更早」按钮等场景复用。
    let load_more = move |_| {
        if msg_loading_more() || !msg_has_more() {
            return;
        }
        let aid = rid();
        msg_loading_more.set(true);
        spawn(async move {
            let before = messages.read().first().map(|m| m.created_at);
            match fetch_agent_messages(&aid, before).await {
                Ok(mut older) => {
                    if older.is_empty() {
                        msg_has_more.set(false);
                    } else {
                        msg_has_more.set(older.len() >= MSG_PAGE_SIZE);
                        older.extend(messages.read().iter().cloned());
                        messages.set(older);
                    }
                }
                Err(e) => toast.error(format!("加载更早消息失败: {}", e)),
            }
            msg_loading_more.set(false);
        });
    };

    // 拉取本 Agent 的新消息（after_timestamp 增量），追加到列表尾部。
    // 同 load_more：捕获 `rid` 保持闭包 Copy，供「刷新」按钮与发送后轮询共用。
    let poll_new = move || {
        let aid = rid();
        spawn(async move {
            let after = messages.read().iter().map(|m| m.created_at).max();
            let req = ListMessagesRequest {
                limit: Some(MSG_PAGE_SIZE),
                after_timestamp: after,
                ..Default::default()
            };
            let from_req = ListMessagesRequest {
                from_id: Some(aid.clone()),
                ..req.clone()
            };
            let to_req = ListMessagesRequest {
                to_id: Some(aid.clone()),
                ..req
            };
            let mut incoming = Vec::new();
            for r in [from_req, to_req] {
                if let Ok(resp) = poll_new_messages(r).await {
                    incoming.extend(resp.messages);
                }
            }
            if incoming.is_empty() {
                return;
            }
            incoming.sort_by_key(|m| m.created_at);
            let mut current = messages.write();
            let mut changed = false;
            for msg in incoming {
                // 移除同 content 的乐观消息（统一使用 replace_tmp_with_real）
                replace_tmp_with_real(&mut current, &msg);
                if current.iter().any(|m| m.message_id == msg.message_id) {
                    continue;
                }
                current.push(msg);
                changed = true;
            }
            // 收到 Agent 的回复即可停止「思考中」提示
            if changed && current.iter().any(|m| m.from_role == 1 && m.from_id == aid) {
                is_typing.set(false);
            }
        });
    };

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

        // 修复 M6（对齐 chat.rs）：is_typing 超时保护，防止 Agent 失败时永久卡死。
        // 用 `callback::Timeout::new(..).forget()` 而非 `spawn + TimeoutFuture`：
        // 组件卸载时 spawn 的 future 被 drop，TimeoutFuture 底层 Closure::once 在
        // 「已入队未触发」时会被浏览器调用已释放的闭包 → "closure invoked recursively
        // or after being dropped"。forget() 有意泄漏该一次性定时器，规避该竞态。
        gloo_timers::callback::Timeout::new(
            REPLY_POLL_MAX as u32 * REPLY_POLL_INTERVAL_MS,
            move || {
                is_typing.set(false);
            },
        )
        .forget();

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

                    // 无 SSE：发送后短时轮询拉取 Agent 回复（最多 ~60s，与超时保护同步）。
                    // 一旦 `poll_new` 收到该 Agent 的消息就会把 is_typing 置回 false。
                    for _ in 0..REPLY_POLL_MAX {
                        gloo_timers::future::TimeoutFuture::new(REPLY_POLL_INTERVAL_MS).await;
                        if !is_typing() {
                            break;
                        }
                        poll_new();
                    }
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
            {match agent_res.read().as_ref() {
                None => rsx! { Loading {} },
                Some(Ok(a)) => {
                    let a = a.clone();
            let capabilities = a.capabilities.clone().unwrap_or_default();
            let desc = a.description.as_deref().unwrap_or("");
            // ---- Agent 关联全景视图 ----
            // 数据源：get_agent 返回的 tools_overview / skills_overview，
            // 与 runtime 注入逻辑同源（neural → bound → pack 优先级去重，互不相交）
            let tools_overview = a.tools_overview.clone().unwrap_or_default();
            let skills_overview = a.skills_overview.clone().unwrap_or_default();

            // 工具：三分组 + 统计总数
            let all_tool_count = tools_overview.neural_tools.len()
                + tools_overview.bound_tools.len()
                + tools_overview
                    .pack_groups
                    .iter()
                    .map(|g| g.tools.len())
                    .sum::<usize>();

            // 技能：三分组 + 统计总数
            let all_skill_count = skills_overview.neural_skills.len()
                + skills_overview
                    .pack_groups
                    .iter()
                    .map(|g| g.skills.len())
                    .sum::<usize>()
                + skills_overview.standalone_skills.len();
            // Tab 按钮动态 class：避免在 rsx! 格式串中嵌套引号转义
            let tab0_class = if active_tab() == 0 { "tab tab-lg tab-active" } else { "tab tab-lg" };
            let tab1_class = if active_tab() == 1 { "tab tab-lg tab-active" } else { "tab tab-lg" };
            let tab2_class = if active_tab() == 2 { "tab tab-lg tab-active" } else { "tab tab-lg" };
            let tab3_class = if active_tab() == 3 { "tab tab-lg tab-active" } else { "tab tab-lg" };
            let tab4_class = if active_tab() == 4 { "tab tab-lg tab-active" } else { "tab tab-lg" };
            let tab5_class = if active_tab() == 5 { "tab tab-lg tab-active" } else { "tab tab-lg" };
            let tab6_class = if active_tab() == 6 { "tab tab-lg tab-active" } else { "tab tab-lg" };

            rsx! {
                HudPanel {
                    title: "{a.name}".to_string(),
                    eyebrow: "AGENT".to_string(),
                    signal: true,
                    div { class: "card-body",
                        // 返回列表按钮：置于布局顶部（放底部时内容长会滚出视野）
                        div { class: "mb-4",
                            Link { to: "/hr/agents", class: "btn hud-btn btn-ghost btn-sm", "← 返回列表" }
                        }
                        // 顶部标题 + 编辑按钮
                        div { class: "mb-6 flex justify-end items-start",
                            div {
                                if !desc.is_empty() {
                                    MarkdownRenderer { content: desc.to_string(), compact: true }
                                }
                            }
                            button {
                                class: "btn hud-btn btn-ghost btn-sm",
                                onclick: move |_| {
                                    if let Some(a) = agent_res.read().as_ref().and_then(|r| r.as_ref().ok()) {
                                        edit_name.set(a.name.clone());
                                        edit_roles.set(a.roles.clone());
                                        edit_roles_input.set(String::new());
                                        edit_description.set(a.description.clone().unwrap_or_default());
                                        edit_capabilities.set(a.capabilities.clone().unwrap_or_default());
                                        edit_capabilities_input.set(String::new());
                                        edit_soul.set(a.soul.clone().unwrap_or_default());
                                        edit_model_provider_id.set(a.model_provider_id.clone());
                                        // 加载运行时配置现有值（缺失时回退 0）
                                        let rc = a.runtime_config.as_ref();
                                        edit_max_thinking_rounds.set(
                                            rc.map(|r| r.max_thinking_rounds.to_string()).unwrap_or_else(|| "0".to_string()),
                                        );
                                        edit_intent_analyze_max_rounds.set(
                                            rc.map(|r| r.intent_analyze_max_rounds.to_string()).unwrap_or_else(|| "0".to_string()),
                                        );
                                        edit_summary_max_rounds.set(
                                            rc.map(|r| r.summary_max_rounds.to_string()).unwrap_or_else(|| "0".to_string()),
                                        );
                                        edit_think_timeout_secs.set(
                                            rc.map(|r| r.think_timeout_secs.to_string()).unwrap_or_else(|| "0".to_string()),
                                        );
                                        show_edit_modal.set(true);
                                    }
                                },
                                "✏️ 编辑"
                            }
                        }

                        // Tab 导航
                        div { class: "tabs tabs-boxed hud-tabs mb-6",
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
                            button {
                                class: "{tab5_class}",
                                onclick: move |_| active_tab.set(5),
                                "🧠 知识图谱"
                            }
                            button {
                                class: "{tab6_class}",
                                onclick: move |_| active_tab.set(6),
                                "⚡ 运行时"
                            }
                        }

                        // Tab 内容
                        {match active_tab() {
                            0 => rsx! {
                                // === 概览：基本信息 + 核心能力 + 运行时配置 + 运行时参数 + 状态切换 ===
                                // （Agent 统计与趋势图已归入「⚡ 运行时」tab）
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
                                                span { class: "badge hud-badge badge-info", "{cap}" }
                                            }
                                        }
                                    } else {
                                        div { class: "text-sm text-base-content/70", "暂无核心能力" }
                                    }
                                }

                                // 灵魂设定（角色/性格 prompt，Markdown 渲染）
                                if let Some(soul) = a.soul.as_deref().filter(|s| !s.is_empty()) {
                                    div { class: "mb-6",
                                        h3 { class: "text-lg font-semibold mb-3", "灵魂设定" }
                                        MarkdownRenderer { content: soul.to_string() }
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

                                // 运行时参数（思考轮次 / 超时，只读展示）
                                if let Some(rc) = &a.runtime_config {
                                    div { class: "mb-6",
                                        h3 { class: "text-lg font-semibold mb-3", "运行时参数" }
                                        div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4",
                                            div {
                                                span { class: "block text-sm text-base-content/70 mb-1", "最大思考轮次" }
                                                span { class: "text-sm",
                                                    if rc.max_thinking_rounds == 0 {
                                                        span { class: "text-base-content/60", "0（系统默认）" }
                                                    } else {
                                                        "{rc.max_thinking_rounds}"
                                                    }
                                                }
                                            }
                                            div {
                                                span { class: "block text-sm text-base-content/70 mb-1", "意图识别轮次" }
                                                span { class: "text-sm",
                                                    if rc.intent_analyze_max_rounds == 0 {
                                                        span { class: "text-base-content/60", "0（系统默认）" }
                                                    } else {
                                                        "{rc.intent_analyze_max_rounds}"
                                                    }
                                                }
                                            }
                                            div {
                                                span { class: "block text-sm text-base-content/70 mb-1", "总结轮次" }
                                                span { class: "text-sm",
                                                    if rc.summary_max_rounds == 0 {
                                                        span { class: "text-base-content/60", "0（系统默认）" }
                                                    } else {
                                                        "{rc.summary_max_rounds}"
                                                    }
                                                }
                                            }
                                            div {
                                                span { class: "block text-sm text-base-content/70 mb-1", "思考超时" }
                                                span { class: "text-sm",
                                                    if rc.think_timeout_secs == 0 {
                                                        span { class: "text-base-content/60", "0（不限制）" }
                                                    } else {
                                                        "{rc.think_timeout_secs} 秒"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                div { class: "mb-6",
                                    h3 { class: "text-lg font-semibold mb-3", "状态切换" }
                                    // 一键入职：仅对面试中/待入职的 Agent 显示，
                                    // 后端 transition_status 白名单为 Interviewing → PendingOnboard → Onboarded，
                                    // 因此需按当前状态走两步（面试中先转待入职，再转已入职）
                                    if a.status == AgentStatus::Interviewing as i32 || a.status == AgentStatus::PendingOnboard as i32 {
                                        div { class: "flex flex-wrap items-center gap-3 mb-3 p-3 rounded-lg bg-base-200",
                                            button {
                                                class: "btn hud-btn btn-success btn-sm",
                                                onclick: move |_| {
                                                    let aid = agent_id_signal();
                                                    spawn(async move {
                                                        if let Err(e) = update_agent_status(UpdateAgentStatusRequest { id: aid.clone(), status: AgentStatus::PendingOnboard }).await {
                                                            toast.error(format!("转入待入职失败: {}", e));
                                                            return;
                                                        }
                                                        match update_agent_status(UpdateAgentStatusRequest { id: aid.clone(), status: AgentStatus::Onboarded }).await {
                                                            Ok(_) => {
                                                                toast.success("Agent 已正式入职");
                                                                match get_agent(build_agent_stats_request(aid.clone())).await {
                                                                    Ok(a) => agent_res.set(Some(Ok(a))),
                                                                    Err(e) => toast.error(format!("刷新 Agent 失败: {}", e)),
                                                                }
                                                            }
                                                            Err(e) => toast.error(format!("入职失败: {}", e)),
                                                        }
                                                    });
                                                },
                                                "🚀 一键入职"
                                            }
                                            span { class: "text-sm text-base-content/60",
                                                "入职后 Agent 将正式对外提供服务，并自动安装项目管理工具包"
                                            }
                                        }
                                    }
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
                                                                            Ok(a) => agent_res.set(Some(Ok(a))),
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
                                                            class: "badge hud-badge badge-accent gap-1",
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
                                                            class: "badge hud-badge badge-info gap-1",
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

                                    // ===== Agent 已安装技能全景（三分组：神经/技能包/独立）=====
                                    // 与 get_agent skills_overview 同源，不再依赖 list_agent_skills 平面展示
                                    if all_skill_count > 0 {
                                        // -- ① 神经技能 --
                                        if !skills_overview.neural_skills.is_empty() {
                                            div { class: "mb-4",
                                                div { class: "flex items-center gap-2 mb-2",
                                                    h4 { class: "font-semibold text-base", "🧠 神经技能" }
                                                    span { class: "badge hud-badge badge-xs badge-primary badge-outline",
                                                        "{skills_overview.neural_skills.len()}"
                                                    }
                                                }
                                                div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                                                    for skill in skills_overview.neural_skills.iter() {
                                                        {
                                                            let skill_clone = skill.clone();
                                                            let aid = agent_id_signal();
                                                            let skill_id = skill.id.clone();
                                                            let skill_name = skill.name.clone();
                                                            let skill_desc = skill.description.clone();
                                                            let tags = skill.tags.clone();
                                                            rsx! {
                                                                div {
                                                                    class: "hud-panel hud-tone-primary",
                                                                    key: "ns-{skill_id}",
                                                                    div { class: "card-body p-4",
                                                                        div { class: "flex justify-between items-start",
                                                                            span { class: "font-medium", "{skill_name}" }
                                                                            div { class: "flex gap-1",
                                                                                span { class: "badge hud-badge badge-primary badge-xs", "神经" }
                                                                                span { class: "badge hud-badge badge-success", "已安装" }
                                                                            }
                                                                        }
                                                                        if !skill_desc.is_empty() {
                                                                            p { class: "text-sm text-base-content/70 mt-2", "{skill_desc}" }
                                                                        }
                                                                        if !tags.is_empty() {
                                                                            div { class: "flex flex-wrap gap-1 mt-2",
                                                                                for tag in tags.iter() {
                                                                                    span { class: "{tag_chip()}", "{tag}" }
                                                                                }
                                                                            }
                                                                        }
                                                                        div { class: "card-actions justify-end mt-3",
                                                                            button {
                                                                                class: "btn hud-btn btn-error btn-sm",
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
                                                                                                match get_agent(build_agent_stats_request(agent_id)).await {
                                                                                                    Ok(a) => agent_res.set(Some(Ok(a))),
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
                                            }
                                        }

                                        // -- ② 技能包分组 --
                                        for pack in skills_overview.pack_groups.iter() {
                                            if !pack.skills.is_empty() {
                                                div { class: "mb-4",
                                                    key: "skg-{pack.tag}",
                                                    div { class: "flex items-center gap-2 mb-2",
                                                        h4 { class: "font-semibold text-base", "📦 技能包：{pack.tag}" }
                                                        span { class: "badge hud-badge badge-xs badge-accent badge-outline",
                                                            "{pack.skills.len()}"
                                                        }
                                                    }
                                                    div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                                                        for skill in pack.skills.iter() {
                                                            {
                                                                let skill_clone = skill.clone();
                                                                let aid = agent_id_signal();
                                                                let skill_id = skill.id.clone();
                                                                let skill_name = skill.name.clone();
                                                                let skill_desc = skill.description.clone();
                                                                let tags = skill.tags.clone();
                                                                rsx! {
                                                                    div {
                                                                        class: "hud-panel hud-tone-accent",
                                                                        key: "skp-{pack.tag}-{skill_id}",
                                                                        div { class: "card-body p-4",
                                                                            div { class: "flex justify-between items-start",
                                                                                span { class: "font-medium", "{skill_name}" }
                                                                                span { class: "badge hud-badge badge-success", "已安装" }
                                                                            }
                                                                            if !skill_desc.is_empty() {
                                                                                p { class: "text-sm text-base-content/70 mt-2", "{skill_desc}" }
                                                                            }
                                                                            if !tags.is_empty() {
                                                                                div { class: "flex flex-wrap gap-1 mt-2",
                                                                                    for tag in tags.iter() {
                                                                                        span { class: "{tag_chip()}", "{tag}" }
                                                                                    }
                                                                                }
                                                                            }
                                                                            div { class: "card-actions justify-end mt-3",
                                                                                button {
                                                                                    class: "btn hud-btn btn-error btn-sm",
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
                                                                                                    match get_agent(build_agent_stats_request(agent_id)).await {
                                                                                                        Ok(a) => agent_res.set(Some(Ok(a))),
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
                                                }
                                            }
                                        }

                                        // -- ③ 独立技能 --
                                        if !skills_overview.standalone_skills.is_empty() {
                                            div { class: "mb-4",
                                                div { class: "flex items-center gap-2 mb-2",
                                                    h4 { class: "font-semibold text-base", "🆓 独立技能" }
                                                    span { class: "badge hud-badge badge-xs badge-neutral badge-outline",
                                                        "{skills_overview.standalone_skills.len()}"
                                                    }
                                                }
                                                div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                                                    for skill in skills_overview.standalone_skills.iter() {
                                                        {
                                                            let skill_clone = skill.clone();
                                                            let aid = agent_id_signal();
                                                            let skill_id = skill.id.clone();
                                                            let skill_name = skill.name.clone();
                                                            let skill_desc = skill.description.clone();
                                                            let tags = skill.tags.clone();
                                                            rsx! {
                                                                div {
                                                                    class: "hud-panel hud-tone-neutral",
                                                                    key: "st-{skill_id}",
                                                                    div { class: "card-body p-4",
                                                                        div { class: "flex justify-between items-start",
                                                                            span { class: "font-medium", "{skill_name}" }
                                                                            span { class: "badge hud-badge badge-success", "已安装" }
                                                                        }
                                                                        if !skill_desc.is_empty() {
                                                                            p { class: "text-sm text-base-content/70 mt-2", "{skill_desc}" }
                                                                        }
                                                                        if !tags.is_empty() {
                                                                            div { class: "flex flex-wrap gap-1 mt-2",
                                                                                for tag in tags.iter() {
                                                                                    span { class: "{tag_chip()}", "{tag}" }
                                                                                }
                                                                            }
                                                                        }
                                                                        div { class: "card-actions justify-end mt-3",
                                                                            button {
                                                                                class: "btn hud-btn btn-error btn-sm",
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
                                                                                                match get_agent(build_agent_stats_request(agent_id)).await {
                                                                                                    Ok(a) => agent_res.set(Some(Ok(a))),
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
                                                                    Ok(a) => agent_res.set(Some(Ok(a))),
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

                                    // ===== Agent 工具全景（三分组：神经/直接绑定/工具包）=====
                                    // 与 runtime 工具注入同源，互不相交，完整展示 Agent 实际可用的所有工具
                                    if all_tool_count > 0 {
                                        // -- ① 神经工具：天生拥有，无需安装/绑定 --
                                        if !tools_overview.neural_tools.is_empty() {
                                            div { class: "mb-4",
                                                div { class: "flex items-center gap-2 mb-2",
                                                    h4 { class: "font-semibold text-base", "🧠 神经工具（天生可用）" }
                                                    span { class: "badge hud-badge badge-xs badge-primary badge-outline",
                                                        "{tools_overview.neural_tools.len()}"
                                                    }
                                                }
                                                div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                                                    for tool in tools_overview.neural_tools.iter() {
                                                        {
                                                            let tool_id = tool.id.clone();
                                                            let tool_name = tool.name.clone();
                                                            let tool_desc = tool.description.as_deref().unwrap_or("");
                                                            let tags = tool.tags.clone();
                                                            let runtime_ready = tool.runtime_ready.clone();
                                                            let not_ready_title = match &runtime_ready {
                                                                RuntimeReady::NotReady { reason, hint } => {
                                                                    format!("未就绪（{}）：{}", reason, hint)
                                                                }
                                                                _ => String::new(),
                                                            };
                                                            rsx! {
                                                                div {
                                                                    class: "hud-panel hud-tone-primary",
                                                                    key: "nt-{tool_id}",
                                                                    div { class: "card-body p-4",
                                                                        div { class: "flex justify-between items-start",
                                                                            span { class: "font-medium", "{tool_name}" }
                                                                            div { class: "flex gap-1",
                                                                                if !not_ready_title.is_empty() {
                                                                                    span {
                                                                                        class: "badge hud-badge badge-warning badge-outline",
                                                                                        title: "{not_ready_title}",
                                                                                        "未就绪"
                                                                                    }
                                                                                }
                                                                                span { class: "badge hud-badge badge-primary badge-xs", "神经" }
                                                                            }
                                                                        }
                                                                        p { class: "text-sm text-base-content/70 mt-2", "{tool_desc}" }
                                                                        if !tags.is_empty() {
                                                                            div { class: "flex flex-wrap gap-1 mt-2",
                                                                                for tag in tags.iter() {
                                                                                    span { class: "{tag_chip()}", "{tag}" }
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

                                        // -- ② 直接绑定工具：通过 agent_tools 显式绑定，带「解绑」按钮 --
                                        if !tools_overview.bound_tools.is_empty() {
                                            div { class: "mb-4",
                                                div { class: "flex items-center gap-2 mb-2",
                                                    h4 { class: "font-semibold text-base", "🔗 直接绑定" }
                                                    span { class: "badge hud-badge badge-xs badge-success badge-outline",
                                                        "{tools_overview.bound_tools.len()}"
                                                    }
                                                }
                                                div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                                                    for tool in tools_overview.bound_tools.iter() {
                                                        {
                                                            let tool_clone = tool.clone();
                                                            let aid = agent_id_signal();
                                                            let tool_id = tool.id.clone();
                                                            let tool_name = tool.name.clone();
                                                            let tool_desc = tool.description.as_deref().unwrap_or("");
                                                            let tags = tool.tags.clone();
                                                            let runtime_ready = tool.runtime_ready.clone();
                                                            let not_ready_title = match &runtime_ready {
                                                                RuntimeReady::NotReady { reason, hint } => {
                                                                    format!("未就绪（{}）：{}", reason, hint)
                                                                }
                                                                _ => String::new(),
                                                            };
                                                            rsx! {
                                                                div {
                                                                    class: "hud-panel hud-tone-success",
                                                                    key: "bt-{tool_id}",
                                                                    div { class: "card-body p-4",
                                                                        div { class: "flex justify-between items-start",
                                                                            span { class: "font-medium", "{tool_name}" }
                                                                            div { class: "flex gap-1",
                                                                                if !not_ready_title.is_empty() {
                                                                                    span {
                                                                                        class: "badge hud-badge badge-warning badge-outline",
                                                                                        title: "{not_ready_title}",
                                                                                        "未就绪"
                                                                                    }
                                                                                }
                                                                                span { class: "badge hud-badge badge-success", "已绑定" }
                                                                            }
                                                                        }
                                                                        p { class: "text-sm text-base-content/70 mt-2", "{tool_desc}" }
                                                                        if !tags.is_empty() {
                                                                            div { class: "flex flex-wrap gap-1 mt-2",
                                                                                for tag in tags.iter() {
                                                                                    span { class: "{tag_chip()}", "{tag}" }
                                                                                }
                                                                            }
                                                                        }
                                                                        div { class: "card-actions justify-end mt-3",
                                                                            button {
                                                                                class: "btn hud-btn btn-error btn-sm",
                                                                                onclick: move |_| {
                                                                                    let agent_id = aid.clone();
                                                                                    let tid = tool_clone.id.clone();
                                                                                    let tname = tool_clone.name.clone();
                                                                                    spawn(async move {
                                                                                        match unbind_tool_from_agent(UnbindToolFromAgentRequest { agent_id: agent_id.clone(), tool_id: tid.clone() }).await {
                                                                                            Ok(_) => {
                                                                                                toast.success(format!("工具 {} 已解绑", tname));
                                                                                                match get_agent(build_agent_stats_request(agent_id.clone())).await {
                                                                                                    Ok(a) => agent_res.set(Some(Ok(a))),
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
                                            }
                                        }

                                        // -- ③ 工具包分组（按 runtime_config.installed_tags 展开）--
                                        for pack in tools_overview.pack_groups.iter() {
                                            if !pack.tools.is_empty() {
                                                div { class: "mb-4",
                                                    key: "tpg-{pack.tag}",
                                                    div { class: "flex items-center gap-2 mb-2",
                                                        h4 { class: "font-semibold text-base", "📦 工具包：{pack.tag}" }
                                                        span { class: "badge hud-badge badge-xs badge-accent badge-outline",
                                                            "{pack.tools.len()}"
                                                        }
                                                    }
                                                    div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                                                        for tool in pack.tools.iter() {
                                                            {
                                                                let tool_id = tool.id.clone();
                                                                let tool_name = tool.name.clone();
                                                                let tool_desc = tool.description.as_deref().unwrap_or("");
                                                                let tags = tool.tags.clone();
                                                                let runtime_ready = tool.runtime_ready.clone();
                                                                let not_ready_title = match &runtime_ready {
                                                                    RuntimeReady::NotReady { reason, hint } => {
                                                                        format!("未就绪（{}）：{}", reason, hint)
                                                                    }
                                                                    _ => String::new(),
                                                                };
                                                                rsx! {
                                                                    div {
                                                                        class: "hud-panel hud-tone-accent",
                                                                        key: "tp-{pack.tag}-{tool_id}",
                                                                        div { class: "card-body p-4",
                                                                            div { class: "flex justify-between items-start",
                                                                                span { class: "font-medium", "{tool_name}" }
                                                                                div { class: "flex gap-1",
                                                                                    if !not_ready_title.is_empty() {
                                                                                        span {
                                                                                            class: "badge hud-badge badge-warning badge-outline",
                                                                                            title: "{not_ready_title}",
                                                                                            "未就绪"
                                                                                        }
                                                                                    }
                                                                                    span { class: "badge hud-badge badge-accent badge-xs", "来自 {pack.tag}" }
                                                                                }
                                                                            }
                                                                            p { class: "text-sm text-base-content/70 mt-2", "{tool_desc}" }
                                                                            if !tags.is_empty() {
                                                                                div { class: "flex flex-wrap gap-1 mt-2",
                                                                                    for tag in tags.iter() {
                                                                                        span { class: "{tag_chip()}", "{tag}" }
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
                                    } else {
                                        div { class: "text-center py-12",
                                            div { class: "text-5xl mb-4 opacity-30", "🔧" }
                                            div { class: "text-base-content/70", "暂无可用工具" }
                                        }
                                    }
                                }
                            },
                            2 => rsx! {
                                // === 状态图：Agent 与全量可用 Tools（神经+绑定+工具包）的关系图 ===
                                // 数据源：tools_overview，与 runtime 同源，不再依赖 agent_tools INNER JOIN
                                {
                                    let mut all_tool_nodes: Vec<RelationNodeInfo> = Vec::new();
                                    // 神经工具
                                    for t in tools_overview.neural_tools.iter() {
                                        all_tool_nodes.push(RelationNodeInfo::with_kind(
                                            t.id.clone(),
                                            t.name.clone(),
                                            "neural_tool",
                                        ));
                                    }
                                    // 直接绑定
                                    for t in tools_overview.bound_tools.iter() {
                                        all_tool_nodes.push(RelationNodeInfo::with_kind(
                                            t.id.clone(),
                                            t.name.clone(),
                                            "bound_tool",
                                        ));
                                    }
                                    // 工具包工具
                                    for pack in tools_overview.pack_groups.iter() {
                                        for t in pack.tools.iter() {
                                            all_tool_nodes.push(RelationNodeInfo::with_kind(
                                                t.id.clone(),
                                                t.name.clone(),
                                                "pack_tool",
                                            ));
                                        }
                                    }
                                    let navigator = use_navigator();
                                    rsx! {
                                        RelationGraph {
                                            center_id: a.id.clone(),
                                            center_name: a.name.clone(),
                                            center_color: "#fa520f".to_string(),
                                            center_kind: Some("agent".to_string()),
                                            related: all_tool_nodes,
                                            related_color: "#f59e0b".to_string(),
                                            related_label: "工具".to_string(),
                                            on_node_click: Some(EventHandler::new(move |evt: crate::components::relation_graph::NodeClickEvent| {
                                                if evt.is_center {
                                                    // 点击中心 Agent 节点，不跳转（已在当前页）
                                                    return;
                                                }
                                                // 无论何种子类型（neural_tool / bound_tool / pack_tool），都跳转工具详情页
                                                let kind_family = evt.kind.clone().unwrap_or_default();
                                                if kind_family.ends_with("tool") || kind_family == "tool" {
                                                    navigator.push(format!("/finance/tools/{}", evt.id));
                                                }
                                            })),
                                        }
                                    }
                                }
                            },
                            3 => rsx! {
                                // === 对话与记忆（左右分栏）===
                                // 原先是纵向堆叠：消息流一长就把记忆面板挤到屏幕外，
                                // 两者还会互相抢占滚动。改为左右双栏、各自独立滚动，
                                // 互不干扰。窄屏（< lg）回退为上下堆叠。
                                div { class: "grid grid-cols-1 lg:grid-cols-[minmax(0,3fr)_minmax(0,2fr)] gap-4 items-start",

                                    // ---- 左栏：消息流 ----
                                    div { class: "flex flex-col min-h-0 lg:h-[600px]",
                                        div { class: "flex items-center justify-between mb-2",
                                            h3 { class: "text-lg font-semibold", "对话" }
                                            button {
                                                class: "btn hud-btn btn-ghost btn-xs",
                                                title: "拉取最新消息",
                                                onclick: move |_| poll_new(),
                                                "刷新"
                                            }
                                        }
                                        div { class: "agent-chat-messages flex-1 min-h-0 max-h-[65vh] lg:max-h-none",
                                            // 上拉翻页入口（置于顶部：列表按时间正序，早消息在上）
                                            if msg_has_more() {
                                                div { class: "flex justify-center pb-1",
                                                    button {
                                                        class: "btn hud-btn btn-ghost btn-xs",
                                                        disabled: msg_loading_more(),
                                                        onclick: load_more,
                                                        if msg_loading_more() { "加载中…" } else { "加载更早消息" }
                                                    }
                                                }
                                            } else if !messages().is_empty() {
                                                div { class: "text-center text-xs text-base-content/40 pb-1", "已到最早消息" }
                                            }

                                            if msg_loading() {
                                                Loading { size: "sm" }
                                            } else if messages().is_empty() && !is_typing() {
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
                                        div { class: "flex gap-2 mt-3",
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
                                                class: "btn hud-btn btn-primary",
                                                onclick: move |_| handle_send(()),
                                                "发送"
                                            }
                                        }
                                    }

                                    // ---- 右栏：记忆 ----
                                    div { class: "flex flex-col min-h-0 lg:h-[600px]",
                                        h3 { class: "text-lg font-semibold mb-2", "记忆" }
                                        div { class: "flex-1 min-h-0 overflow-y-auto pr-1",
                                            AgentMemoryPanel { agent_id: Some(id.clone()) }
                                        }
                                    }
                                }
                            },
                            4 => rsx! {
                                HudPanel {
                                    title: "关系图".to_string(),
                                    eyebrow: "GRAPH".to_string(),
                                    div { class: "p-4",
                                        div { class: "w-full h-[520px]",
                                            WorkspaceGraph {
                                                view: WorkspaceView::AgentDetail(a.id.clone()),
                                                projects: graph_projects.read().clone(),
                                                agents: graph_agents.read().clone(),
                                                tasks: graph_tasks.read().clone(),
                                                width: 800.0,
                                                height: 500.0,
                                                auto_size: true,
                                            }
                                        }
                                    }
                                }
                            },
                            5 => rsx! {
                                KnowledgeGraph { agent_id: Some(id.clone()) }
                            },
                            6 => rsx! {
                                // === 运行时：统计概览（Agent 统计 + 模型调用趋势）→ 实时状态 + 思考快照 + 取消按钮 ===
                                // 统计数据属运行时数据，统一放在运行时 tab 上半部分
                                if a.stats.is_some() || a.model_call_stats.is_some() {
                                    div { class: "mb-6",
                                        AgentStatsPanel {
                                            stats: a.stats.clone(),
                                            model_call_stats: a.model_call_stats.clone(),
                                        }
                                    }
                                }
                                crate::components::runtime_panel::RuntimePanel {
                                    agent_id: id.clone(),
                                }
                            },
                            _ => rsx! {},
                        }}

                        Modal {
                            title: "编辑 Agent 基本信息".to_string(),
                            show: show_edit_modal(),
                            on_close: move |_| show_edit_modal.set(false),
                            footer: rsx! {
                                button { class: "btn hud-btn btn-ghost", onclick: move |_| show_edit_modal.set(false), "取消" }
                                button {
                                    class: "btn hud-btn btn-primary",
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
                                                .into_iter()
                                                .filter(|s| !s.trim().is_empty())
                                                .map(|s| s.trim().to_string())
                                                .collect();
                                            let capabilities: Vec<String> = edit_capabilities()
                                                .into_iter()
                                                .filter(|s| !s.trim().is_empty())
                                                .map(|s| s.trim().to_string())
                                                .collect();
                                            let soul = if edit_soul().trim().is_empty() { None } else { Some(edit_soul()) };
                                            let mp_id = if edit_model_provider_id().is_empty() { None } else { Some(edit_model_provider_id()) };
                                            // 运行时配置：解析输入框，空字符串/非法值视为 0
                                            let max_thinking_rounds = edit_max_thinking_rounds()
                                                .trim()
                                                .parse::<usize>()
                                                .unwrap_or(0);
                                            let intent_analyze_max_rounds = edit_intent_analyze_max_rounds()
                                                .trim()
                                                .parse::<usize>()
                                                .unwrap_or(0);
                                            let summary_max_rounds = edit_summary_max_rounds()
                                                .trim()
                                                .parse::<usize>()
                                                .unwrap_or(0);
                                            let think_timeout_secs = edit_think_timeout_secs()
                                                .trim()
                                                .parse::<u64>()
                                                .unwrap_or(0);
                                            let req = UpdateAgentRequest {
                                                id: id_for_submit.clone(),
                                                name: Some(name),
                                                roles: Some(roles),
                                                description: Some(edit_description()),
                                                capabilities: Some(capabilities),
                                                soul,
                                                model_provider_id: mp_id,
                                                runtime_config: Some(AgentRuntimeConfigInfo {
                                                    max_thinking_rounds,
                                                    intent_analyze_max_rounds,
                                                    summary_max_rounds,
                                                    think_timeout_secs,
                                                }),
                                            };
                                            saving_meta.set(true);
                                            let id_clone = id_for_submit.clone();
                                            spawn(async move {
                                                match update_agent(req).await {
                                                    Ok(_) => {
                                                        toast.success("Agent 信息已更新");
                                                        show_edit_modal.set(false);
                                                        match get_agent(build_agent_stats_request(id_clone.clone())).await {
                                                            Ok(a) => agent_res.set(Some(Ok(a))),
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
                                    label { class: "label",
                                        span { class: "label-text font-medium", "角色（多选）" }
                                        span { class: "label-text-alt", "用于路由匹配，如前台/Web接待/代码专家 等" }
                                    }
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
                                                let selected = edit_roles().iter().any(|r| r == key);
                                                let cls = if selected {
                                                    "btn btn-primary btn-sm"
                                                } else {
                                                    "btn btn-outline btn-sm"
                                                };
                                                rsx! {
                                                    button { class: cls,
                                                        onclick: move |_| {
                                                            let mut v = edit_roles();
                                                            if let Some(pos) = v.iter().position(|x| x == key_clone.as_str()) {
                                                                v.remove(pos);
                                                            } else {
                                                                v.push(key_clone.clone());
                                                            }
                                                            edit_roles.set(v);
                                                        },
                                                        "{label}"
                                                    }
                                                }
                                            })
                                        }
                                    }
                                    div { class: "flex flex-wrap gap-2 items-center",
                                        if !edit_roles().is_empty() {
                                            for role in edit_roles() {
                                                span { class: "badge hud-badge badge-accent badge-lg gap-1",
                                                    "{role}",
                                                    button { class: "btn hud-btn btn-ghost btn-xs",
                                                        onclick: move |_| {
                                                            let mut v = edit_roles();
                                                            if let Some(pos) = v.iter().position(|x| x == &role) {
                                                                v.remove(pos);
                                                            }
                                                            edit_roles.set(v);
                                                        },
                                                        "✕"
                                                    }
                                                }
                                            }
                                        }
                                        input { class: "input input-bordered input-sm flex-1 min-w-[180px]",
                                            value: "{edit_roles_input}",
                                            placeholder: "自定义角色，回车/逗号添加",
                                            oninput: move |e| {
                                                let val = e.value();
                                                if let Some(comma_pos) = val.find(',') {
                                                    let (head, rest) = val.split_at(comma_pos);
                                                    let v = head.trim().to_string();
                                                    if !v.is_empty() && !edit_roles().iter().any(|r| r == v.as_str()) {
                                                        let mut arr = edit_roles();
                                                        arr.push(v);
                                                        edit_roles.set(arr);
                                                    }
                                                    edit_roles_input.set(rest[1..].trim().to_string());
                                                } else {
                                                    edit_roles_input.set(val);
                                                }
                                            },
                                            onkeydown: move |e| {
                                                if e.key() == Key::Enter {
                                                    e.prevent_default();
                                                    let v = edit_roles_input().trim().to_string();
                                                    if !v.is_empty() && !edit_roles().iter().any(|r| r == v.as_str()) {
                                                        let mut arr = edit_roles();
                                                        arr.push(v);
                                                        edit_roles.set(arr);
                                                    }
                                                    edit_roles_input.set(String::new());
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
                                        if !edit_capabilities().is_empty() {
                                            for cap in edit_capabilities() {
                                                span { class: "badge hud-badge badge-success badge-lg gap-1",
                                                    "{cap}",
                                                    button { class: "btn hud-btn btn-ghost btn-xs",
                                                        onclick: move |_| {
                                                            let mut v = edit_capabilities();
                                                            if let Some(pos) = v.iter().position(|x| x == &cap) {
                                                                v.remove(pos);
                                                            }
                                                            edit_capabilities.set(v);
                                                        },
                                                        "✕"
                                                    }
                                                }
                                            }
                                        }
                                        input { class: "input input-bordered input-sm flex-1 min-w-[180px]",
                                            value: "{edit_capabilities_input}",
                                            placeholder: "自定义能力，回车/逗号添加",
                                            oninput: move |e| {
                                                let val = e.value();
                                                if let Some(comma_pos) = val.find(',') {
                                                    let (head, rest) = val.split_at(comma_pos);
                                                    let v = head.trim().to_string();
                                                    if !v.is_empty() && !edit_capabilities().iter().any(|r| r == v.as_str()) {
                                                        let mut arr = edit_capabilities();
                                                        arr.push(v);
                                                        edit_capabilities.set(arr);
                                                    }
                                                    edit_capabilities_input.set(rest[1..].trim().to_string());
                                                } else {
                                                    edit_capabilities_input.set(val);
                                                }
                                            },
                                            onkeydown: move |e| {
                                                if e.key() == Key::Enter {
                                                    e.prevent_default();
                                                    let v = edit_capabilities_input().trim().to_string();
                                                    if !v.is_empty() && !edit_capabilities().iter().any(|r| r == v.as_str()) {
                                                        let mut arr = edit_capabilities();
                                                        arr.push(v);
                                                        edit_capabilities.set(arr);
                                                    }
                                                    edit_capabilities_input.set(String::new());
                                                }
                                            }
                                        }
                                    }
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
                                // 运行时配置分区
                                div { class: "pt-2 border-t border-base-300" }
                                div { class: "form-control w-full",
                                    label { class: "label",
                                        span { class: "label-text font-medium", "最大思考轮次" }
                                        span { class: "label-text-alt text-base-content/60", "0 = 系统默认" }
                                    }
                                    input { class: "input input-bordered w-full", r#type: "number",
                                        value: "{edit_max_thinking_rounds}",
                                        oninput: move |e| edit_max_thinking_rounds.set(e.value()),
                                        placeholder: "0 = 使用系统配置" }
                                }
                                div { class: "form-control w-full",
                                    label { class: "label",
                                        span { class: "label-text font-medium", "意图识别轮次" }
                                        span { class: "label-text-alt text-base-content/60", "0 = 系统默认" }
                                    }
                                    input { class: "input input-bordered w-full", r#type: "number",
                                        value: "{edit_intent_analyze_max_rounds}",
                                        oninput: move |e| edit_intent_analyze_max_rounds.set(e.value()),
                                        placeholder: "0 = 使用系统配置" }
                                }
                                div { class: "form-control w-full",
                                    label { class: "label",
                                        span { class: "label-text font-medium", "总结轮次" }
                                        span { class: "label-text-alt text-base-content/60", "0 = 系统默认" }
                                    }
                                    input { class: "input input-bordered w-full", r#type: "number",
                                        value: "{edit_summary_max_rounds}",
                                        oninput: move |e| edit_summary_max_rounds.set(e.value()),
                                        placeholder: "0 = 使用系统配置" }
                                }
                                div { class: "form-control w-full",
                                    label { class: "label",
                                        span { class: "label-text font-medium", "思考超时秒数" }
                                        span { class: "label-text-alt text-base-content/60", "0 = 不限制" }
                                    }
                                    input { class: "input input-bordered w-full", r#type: "number",
                                        value: "{edit_think_timeout_secs}",
                                        oninput: move |e| edit_think_timeout_secs.set(e.value()),
                                        placeholder: "0 = 不限制" }
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
                                class: "modal modal-open hud-modal",
                                onclick: move |_| show_skill_pack_uninstall_dialog.set(None),
                                div {
                                    class: "modal-box hud-modal-box",
                                    onclick: move |e| e.stop_propagation(),
                                    h3 { class: "font-bold text-lg mb-2", "卸载技能包" }
                                    p { class: "text-sm text-base-content/70 mb-4",
                                        "即将卸载技能包 [{tag_a}]，请选择卸载方式："
                                    }
                                    div { class: "flex flex-col gap-3",
                                        // 选项 A：仅移除关联
                                        button {
                                            class: "btn hud-btn btn-ghost justify-start text-left",
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
                                            class: "btn hud-btn btn-error btn-outline justify-start text-left",
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
                                            class: "btn hud-btn btn-ghost",
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
            Some(Err(e)) => rsx! {
                EmptyState {
                    icon: "❓".to_string(),
                    message: format!("加载失败: {}", e),
                }
            },
            }}
        }
    }
}
