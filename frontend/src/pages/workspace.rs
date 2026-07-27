//! 工作台页面（驾驶舱）
//!
//! 三栏布局：左侧 Project 列表浮层 / 中间 Canvas 关系图 / 右侧 Agent 列表浮层
//! 顶部汇总状态条：项目数 / Agent 数 / 运行中项目 / 忙碌 Agent
//! 中间区域通过 WorkspaceView 状态机切换视图：
//! - Global：运行中 Project ↔ Agent 关联（默认）
//! - ProjectDetail：选中 Project 的 Task + Agent
//! - AgentDetail：选中 Agent 的 Task + Project
//! - TaskDetail：选中 Task 的 Project + Agent + 依赖/后继 Task
//!
//! 数据加载策略（渐进式）：
//! - 侧边栏：全量加载 projects + agents（轻量）
//! - 中心图：按视图按需加载 tasks 和关联数据，避免全量加载
//!
//! 底部游戏式对话框：
//! - 未聚焦：单行输入框 + 上方浮动半透明最近消息
//! - 聚焦后：展开为可滚动消息列表 + 输入框
//! - 视图联动：Global=默认对话 / ProjectDetail=Project 对话 / AgentDetail=Agent 对话 / TaskDetail=Task 对话

use dioxus::prelude::*;

use crate::api::hr::query_agents;
use crate::api::message::{load_latest_messages, send_message_to_agent};
use crate::api::project::{list_project_tasks, query_projects, query_tasks};
use crate::components::charts::line_chart::LineChart;
use crate::components::chat::{MessageBubble, TypingIndicator};
use crate::components::workspace_graph::{WorkspaceGraph, WorkspaceView};
use crate::hooks::use_workspace_data::{WorkspaceData, use_workspace_data};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use crate::utils::{build_optimistic_user_msg, replace_tmp_with_real};
use common::api::{
    AgentListItem, AgentQueryRequest, MessageListItem, PaginationParams, ProjectListItem,
    ProjectQueryRequest, SendMessageToAgentParams, TaskListItem, TaskQueryRequest,
};
use common::enums::AssigneeType;
use common::models::TimeSeriesPoint;
use wasm_bindgen::{JsCast, closure::Closure};

/// Project 状态标签
fn project_status_label(status: i32) -> &'static str {
    match status {
        1 => "活跃",
        2 => "待评审",
        3 => "进行中",
        4 => "已完成",
        5 => "已归档",
        _ => "未知",
    }
}

/// Agent 运行时状态标签
fn agent_runtime_label(runtime_state: i32) -> &'static str {
    match runtime_state {
        0 => "空闲",
        1 => "休息中",
        2 => "忙碌",
        _ => "未知",
    }
}

/// Agent 运行时状态颜色 class（用于 badge）
fn agent_runtime_badge_class(runtime_state: i32) -> &'static str {
    match runtime_state {
        0 => "badge badge-success",
        1 => "badge badge-warning",
        2 => "badge badge-error",
        _ => "badge badge-ghost",
    }
}

/// 判断是否为运行中项目（status 1-3：活跃 / 待评审 / 进行中）
fn is_active_project(status: i32) -> bool {
    matches!(status, 1 | 2 | 3)
}

/// 根据视图计算对话上下文（project_id, task_id, to_agent_id）
fn resolve_chat_context(
    view: &WorkspaceView,
    sidebar: &WorkspaceData,
) -> (Option<String>, Option<String>, Option<String>) {
    match view {
        WorkspaceView::Global => (None, None, None),
        WorkspaceView::ProjectDetail(pid) => {
            let to_agent_id = sidebar
                .projects
                .iter()
                .find(|p| &p.id == pid)
                .and_then(|p| p.owner_agent_id.clone());
            (Some(pid.clone()), None, to_agent_id)
        }
        WorkspaceView::AgentDetail(aid) => (None, None, Some(aid.clone())),
        WorkspaceView::TaskDetail(tid) => {
            // 从 graph_tasks 查找 task 的 project_id
            // 注意：这里返回的 project_id 可能不准确，因为 graph_tasks 在 effect 中加载
            // 但 sidebar 可能有 project 列表，暂返回 None 让后端兜底
            (None, Some(tid.clone()), None)
        }
    }
}

#[component]
pub fn Workspace() -> Element {
    let (sidebar_signal, mut refresh) = use_workspace_data();
    let mut current_view = use_signal(|| WorkspaceView::Global);
    let toast = use_toast();

    // 图数据（按视图按需加载）
    let mut graph_projects = use_signal(Vec::<ProjectListItem>::new);
    let mut graph_agents = use_signal(Vec::<AgentListItem>::new);
    let mut graph_tasks = use_signal(Vec::<TaskListItem>::new);
    let mut graph_loading = use_signal(|| false);

    // 对话框状态
    let mut chat_messages = use_signal(Vec::<MessageListItem>::new);
    let mut chat_input = use_signal(String::new);
    let mut chat_focused = use_signal(|| false);
    let mut chat_is_typing = use_signal(|| false);
    let mut chat_project_id = use_signal(|| Option::<String>::None);
    let mut chat_task_id = use_signal(|| Option::<String>::None);
    let mut chat_to_agent_id = use_signal(|| Option::<String>::None);

    // 侧边栏红点提示：收到新消息但不在当前视图时，对应 project/agent 亮红点
    let mut project_unread = use_signal(std::collections::HashSet::<String>::new);
    let mut agent_unread = use_signal(std::collections::HashSet::<String>::new);

    // 消息流量时序数据（前端本地累积，每分钟桶，保留最近 60 分钟）
    let mut msg_flow: Signal<std::collections::HashMap<i64, u64>> =
        use_signal(|| std::collections::HashMap::new());

    let sidebar = sidebar_signal.read().clone();

    // 视图变化时按需加载图数据
    use_effect(move || {
        let view = current_view.read().clone();
        let sidebar_data = sidebar_signal.read().clone();
        let toast = toast.clone();

        spawn(async move {
            graph_loading.set(true);

            match view {
                WorkspaceView::Global => {
                    // Global：并发加载运行中项目的 tasks，推断 Project ↔ Agent 关联
                    let Some(data) = sidebar_data else {
                        graph_loading.set(false);
                        return;
                    };
                    let active_pids: Vec<String> = data
                        .projects
                        .iter()
                        .filter(|p| is_active_project(p.status))
                        .map(|p| p.id.clone())
                        .collect();

                    // 并发加载每个运行中项目的 tasks
                    let mut all_tasks = Vec::new();
                    for pid in &active_pids {
                        if let Ok(resp) = list_project_tasks(pid).await {
                            all_tasks.extend(resp.tasks);
                        }
                    }
                    graph_tasks.set(all_tasks);
                    // Global 视图：projects 和 agents 用侧边栏全量数据
                    graph_projects.set(data.projects.clone());
                    graph_agents.set(data.agents.clone());
                }

                WorkspaceView::ProjectDetail(pid) => {
                    // ProjectDetail：加载该项目 tasks + 批量查询关联 agents
                    let Some(data) = sidebar_data else {
                        graph_loading.set(false);
                        return;
                    };
                    match list_project_tasks(&pid).await {
                        Ok(resp) => {
                            let tasks_vec = resp.tasks;
                            // 批量加载关联 agents（消除 N+1）
                            let assignee_ids: Vec<String> = tasks_vec
                                .iter()
                                .filter(|t| t.assignee_type == 1)
                                .map(|t| t.assignee_id.clone())
                                .collect::<std::collections::HashSet<_>>()
                                .into_iter()
                                .collect();
                            if assignee_ids.is_empty() {
                                graph_agents.set(Vec::new());
                            } else {
                                let req = AgentQueryRequest {
                                    ids: Some(assignee_ids),
                                    pagination: PaginationParams::default(),
                                    ..Default::default()
                                };
                                match query_agents(&req).await {
                                    Ok(page) => graph_agents.set(page.items),
                                    Err(e) => toast.error(&format!("批量获取 Agent 失败: {}", e)),
                                }
                            }
                            graph_tasks.set(tasks_vec);
                            // graph_projects 从侧边栏数据构造
                            graph_projects.set(
                                data.projects
                                    .iter()
                                    .find(|p| p.id == pid)
                                    .cloned()
                                    .map(|p| vec![p])
                                    .unwrap_or_default(),
                            );
                        }
                        Err(e) => toast.error(&format!("获取项目任务失败: {}", e)),
                    }
                }

                WorkspaceView::AgentDetail(aid) => {
                    // AgentDetail：复用 Agent 详情页加载逻辑
                    // 1. 按 agent_id 过滤 tasks
                    let Some(data) = sidebar_data else {
                        graph_loading.set(false);
                        return;
                    };
                    let req = TaskQueryRequest {
                        assignee_id: Some(aid.clone()),
                        assignee_type: Some(AssigneeType::Agent),
                        pagination: PaginationParams::default(),
                        ..Default::default()
                    };
                    match query_tasks(&req).await {
                        Ok(page) => {
                            let tasks = page.items;
                            // 2. 从 tasks 收集 project_ids，批量查询
                            let project_ids: Vec<String> = tasks
                                .iter()
                                .filter_map(|t| t.project_id.clone())
                                .collect::<std::collections::HashSet<_>>()
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
                                    Err(e) => toast.error(&format!("批量获取项目失败: {}", e)),
                                }
                            }
                        }
                        Err(e) => toast.error(&format!("获取任务列表失败: {}", e)),
                    }
                    // 3. graph_agents 从侧边栏数据构造
                    graph_agents.set(
                        data.agents
                            .iter()
                            .find(|a| a.id == aid)
                            .cloned()
                            .map(|a| vec![a])
                            .unwrap_or_default(),
                    );
                }

                WorkspaceView::TaskDetail(tid) => {
                    // TaskDetail：需要获取 task 详情 + 同 project tasks + 关联 agent + project
                    if sidebar_data.is_none() {
                        graph_loading.set(false);
                        return;
                    }
                    // 1. 先获取 task 详情（通过 query_tasks ids）
                    let req = TaskQueryRequest {
                        ids: Some(vec![tid.clone()]),
                        pagination: PaginationParams::default(),
                        ..Default::default()
                    };
                    match query_tasks(&req).await {
                        Ok(page) => {
                            if let Some(task) = page.items.into_iter().next() {
                                let pid = task.project_id.clone();
                                let assignee_type = task.assignee_type;
                                let assignee_id = task.assignee_id.clone();

                                // 2. 加载同 project 的 tasks（用于依赖 DAG）
                                if let Some(pid) = &pid {
                                    match list_project_tasks(pid).await {
                                        Ok(resp) => graph_tasks.set(resp.tasks),
                                        Err(e) => toast.error(&format!("获取项目任务失败: {}", e)),
                                    }
                                }

                                // 3. 批量加载关联 agent
                                if assignee_type == 1 {
                                    let req = AgentQueryRequest {
                                        ids: Some(vec![assignee_id.clone()]),
                                        pagination: PaginationParams::default(),
                                        ..Default::default()
                                    };
                                    match query_agents(&req).await {
                                        Ok(page) => {
                                            if let Some(a) = page.items.into_iter().next() {
                                                graph_agents.set(vec![a]);
                                            }
                                        }
                                        Err(e) => toast.error(&format!("获取 Agent 失败: {}", e)),
                                    }
                                }

                                // 4. 批量加载关联 project
                                if let Some(pid) = &pid {
                                    let req = ProjectQueryRequest {
                                        ids: Some(vec![pid.clone()]),
                                        pagination: PaginationParams::default(),
                                        ..Default::default()
                                    };
                                    match query_projects(&req).await {
                                        Ok(page) => {
                                            if let Some(p) = page.items.into_iter().next() {
                                                graph_projects.set(vec![p]);
                                            }
                                        }
                                        Err(e) => toast.error(&format!("获取 Project 失败: {}", e)),
                                    }
                                }
                            }
                        }
                        Err(e) => toast.error(&format!("获取 Task 失败: {}", e)),
                    }
                }
            }

            graph_loading.set(false);
        });
    });

    // 视图变化时重新加载对话消息 + 更新对话上下文
    {
        let mut chat_messages = chat_messages.clone();
        let mut chat_project_id = chat_project_id.clone();
        let mut chat_task_id = chat_task_id.clone();
        let mut chat_to_agent_id = chat_to_agent_id.clone();
        let sidebar_signal = sidebar_signal.clone();

        use_effect(move || {
            let view = current_view.read().clone();
            let sidebar_data = sidebar_signal.read().clone();

            // 计算新的对话上下文
            let (pid, tid, aid) = if let Some(data) = &sidebar_data {
                resolve_chat_context(&view, data)
            } else {
                (None, None, None)
            };

            chat_project_id.set(pid.clone());
            chat_task_id.set(tid.clone());
            chat_to_agent_id.set(aid.clone());

            // 加载历史消息
            let pid_clone = pid.clone();
            spawn(async move {
                let pid_str = pid_clone.as_deref();
                match load_latest_messages(common::api::ListMessagesRequest {
                    project_id: pid_str.map(|s| s.to_string()),
                    limit: Some(20),
                    ..Default::default()
                }).await {
                    Ok(resp) => {
                        // Agent 视图按 to_id/from_id 过滤；其他视图按 project_id 过滤
                        let filtered = if let Some(aid) = &aid {
                            resp.messages
                                .into_iter()
                                .filter(|m| &m.to_id == aid || &m.from_id == aid)
                                .collect::<Vec<_>>()
                        } else {
                            resp.messages
                        };
                        chat_messages.set(filtered);
                    }
                    Err(_) => chat_messages.set(Vec::new()),
                }
            });
        });
    }

    // SSE 订阅实时消息
    {
        let mut chat_messages = chat_messages.clone();
        let chat_project_id = chat_project_id.clone();
        let chat_to_agent_id = chat_to_agent_id.clone();
        let mut project_unread = project_unread.clone();
        let mut agent_unread = agent_unread.clone();
        let mut msg_flow = msg_flow.clone();

        use_effect(move || {
            let on_message = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
                if let Some(data) = event.data().as_string() {
                    if let Ok(msg) = serde_json::from_str::<MessageListItem>(&data) {
                        // 累计消息流量（按分钟桶，淘汰超过 60 分钟的旧桶）
                        {
                            let mut flow = msg_flow.write();
                            let now_ms = js_sys::Date::now() as i64;
                            let bucket = (now_ms / 60_000) * 60_000;
                            *flow.entry(bucket).or_insert(0) += 1;
                            let cutoff = bucket - 60 * 60_000;
                            flow.retain(|&k, _| k >= cutoff);
                        }

                        let mut msgs = chat_messages.write();
                        // 移除同 content 的乐观消息（统一使用 replace_tmp_with_real）
                        replace_tmp_with_real(&mut msgs, &msg);

                        // 过滤：project_id 匹配 或 agent_id 匹配
                        let cur_pid = chat_project_id.read().clone();
                        let cur_aid = chat_to_agent_id.read().clone();

                        let pid_match = match (&cur_pid, &msg.project_id) {
                            (Some(a), Some(b)) => a == b,
                            (None, None) => true,
                            _ => false,
                        };
                        let aid_match = cur_aid
                            .as_deref()
                            .map(|aid| msg.to_id == aid || msg.from_id == aid)
                            .unwrap_or(false);

                        // Global 视图（pid=None, aid=None）只显示无 project 的消息
                        if pid_match || aid_match {
                            // 去重
                            if !msgs.iter().any(|m| m.message_id == msg.message_id) {
                                msgs.push(msg);
                                // 保留最近 50 条
                                if msgs.len() > 50 {
                                    let drain = msgs.len() - 50;
                                    msgs.drain(..drain);
                                }
                            }
                        } else {
                            // 不属于当前视图 → 更新侧边栏红点
                            // Agent 回复消息（from_role=1）才触发红点
                            if msg.from_role == 1 {
                                if let Some(pid) = &msg.project_id {
                                    if cur_pid.as_deref() != Some(pid) {
                                        project_unread.write().insert(pid.clone());
                                    }
                                }
                                let sender_aid = &msg.from_id;
                                if cur_aid.as_deref() != Some(sender_aid) {
                                    agent_unread.write().insert(sender_aid.clone());
                                }
                            }
                        }
                    }
                }
            }) as Box<dyn FnMut(_)>);

            if let Ok(es) = web_sys::EventSource::new("/api/v1/finance/messages/sse") {
                es.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
                // 清理
                use_drop(move || {
                    es.set_onmessage(None);
                    drop(on_message);
                    es.close();
                });
            }
        });
    }

    // 发送消息的信号触发器
    let mut send_trigger = use_signal(|| false);

    // 发送消息
    {
        let mut chat_input = chat_input.clone();
        let mut chat_project_id = chat_project_id.clone();
        let mut chat_task_id = chat_task_id.clone();
        let mut chat_to_agent_id = chat_to_agent_id.clone();
        let mut chat_messages = chat_messages.clone();
        let mut chat_is_typing = chat_is_typing.clone();
        let mut send_trigger = send_trigger.clone();
        let toast = toast.clone();

        use_effect(move || {
            if !*send_trigger.read() {
                return;
            }
            send_trigger.set(false);

            let text = chat_input.read().trim().to_string();
            if text.is_empty() {
                return;
            }

            let pid = chat_project_id.read().clone();
            let tid = chat_task_id.read().clone();
            let aid = chat_to_agent_id.read().clone();
            let text_snapshot = text.clone();

            chat_input.set(String::new());
            chat_is_typing.set(true);

            {
                let mut chat_is_typing = chat_is_typing.clone();
                spawn(async move {
                    gloo_timers::future::TimeoutFuture::new(60_000).await;
                    chat_is_typing.set(false);
                });
            }

            spawn(async move {
                let req = SendMessageToAgentParams {
                    to_agent_id: aid.clone(),
                    content: text.clone(),
                    project_id: pid.clone(),
                    task_id: tid.clone(),
                    reply_to_id: None,
                    attachment_ids: None,
                };

                match send_message_to_agent(req).await {
                    Ok(_) => {
                        let user_msg = build_optimistic_user_msg(text, pid, tid, aid);
                        chat_messages.write().push(user_msg);
                    }
                    Err(e) => {
                        chat_input.set(text_snapshot);
                        toast.error(&format!("发送消息失败: {}", e));
                        chat_is_typing.set(false);
                    }
                }
            });
        });
    }

    let gp = graph_projects.read().clone();
    let ga = graph_agents.read().clone();
    let gt = graph_tasks.read().clone();
    let loading = *graph_loading.read();

    rsx! {
        AppLayout {
            div { class: "flex flex-col h-full gap-4",
                // === 顶部汇总状态条 ===
                {sidebar.as_ref().map(|d| {
                    let project_count = d.projects.len();
                    let agent_count = d.agents.len();
                    let active_project_count = d.projects.iter().filter(|p| is_active_project(p.status)).count();
                    let busy_agent_count = d.agents.iter().filter(|a| a.runtime_state == 2).count();

                    rsx! {
                        div { class: "grid grid-cols-2 md:grid-cols-4 gap-3",
                            div { class: "stat bg-base-100 rounded-lg shadow-sm",
                                div { class: "stat-title", "项目" }
                                div { class: "stat-value text-primary", "{project_count}" }
                            }
                            div { class: "stat bg-base-100 rounded-lg shadow-sm",
                                div { class: "stat-title", "Agent" }
                                div { class: "stat-value text-info", "{agent_count}" }
                            }
                            div { class: "stat bg-base-100 rounded-lg shadow-sm",
                                div { class: "stat-title", "运行中项目" }
                                div { class: "stat-value text-secondary", "{active_project_count}" }
                            }
                            div { class: "stat bg-base-100 rounded-lg shadow-sm",
                                div { class: "stat-title", "忙碌 Agent" }
                                div { class: "stat-value text-error", "{busy_agent_count}" }
                            }
                        }
                    }
                })}

                // === 三栏布局：左 Project / 中 Canvas / 右 Agent ===
                div { class: "flex gap-4 flex-1 min-h-0",
                    // 左侧 Project 列表浮层
                    {sidebar.as_ref().map(|d| {
                        rsx! {
                            div { class: "w-64 flex-shrink-0 bg-base-100 rounded-lg shadow-md overflow-y-auto",
                                div { class: "p-3 sticky top-0 bg-base-100 border-b border-base-200 z-10",
                                    div { class: "flex justify-between items-center",
                                        h3 { class: "text-sm font-semibold", "项目列表" }
                                        button {
                                            class: "btn btn-ghost btn-xs",
                                            onclick: move |_| { current_view.set(WorkspaceView::Global); },
                                            "全局"
                                        }
                                    }
                                }
                                div { class: "divide-y divide-base-200",
                                    for p in d.projects.iter() {
                                        {
                                            let pid = p.id.clone();
                                            let is_selected = matches!(*current_view.read(), WorkspaceView::ProjectDetail(ref id) if id == &pid);
                                            let has_unread = project_unread.read().contains(&pid);
                                            let item_class = if is_selected {
                                                "relative overflow-hidden w-full text-left p-3 hover:bg-base-200 transition-colors bg-base-200"
                                            } else {
                                                "relative overflow-hidden w-full text-left p-3 hover:bg-base-200 transition-colors"
                                            };
                                            rsx! {
                                                button {
                                                    class: "{item_class}",
                                                    onclick: move |_| {
                                                        current_view.set(WorkspaceView::ProjectDetail(pid.clone()));
                                                        // 点击后清除该 project 的提示
                                                        project_unread.write().remove(&pid);
                                                    },
                                                    if has_unread {
                                                        span { class: "hud-streak" }
                                                    }
                                                    div { class: "flex justify-between items-start",
                                                        div { class: "flex items-center gap-1 min-w-0",
                                                            span { class: "text-sm font-medium truncate", "{p.name}" }
                                                        }
                                                        span { class: "badge badge-xs badge-ghost ml-2 flex-shrink-0",
                                                            "{project_status_label(p.status)}"
                                                        }
                                                    }
                                                    if !p.tags.is_empty() {
                                                        div { class: "flex flex-wrap gap-1 mt-1",
                                                            for tag in p.tags.iter().take(2) {
                                                                span { class: "badge badge-xs badge-ghost", "{tag}" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if d.projects.is_empty() {
                                        div { class: "p-4 text-center text-sm text-base-content/50",
                                            "暂无项目"
                                        }
                                    }
                                }
                            }
                        }
                    })}

                    // 中间 Canvas 关系图
                    div { class: "flex-1 bg-base-100 rounded-lg shadow-md p-4 min-w-0 relative",
                        if loading {
                            div { class: "absolute inset-0 flex items-center justify-center bg-base-100/50 z-10",
                                span { class: "loading loading-spinner loading-lg" }
                            }
                        }
                        WorkspaceGraph {
                            view: current_view.read().clone(),
                            projects: gp.clone(),
                            agents: ga.clone(),
                            tasks: gt.clone(),
                            width: 700.0,
                            height: 500.0,
                            on_view_change: Some(EventHandler::new(move |new_view: WorkspaceView| {
                                current_view.set(new_view);
                            })),
                        }
                        {sidebar.is_none().then(|| rsx! {
                            div { class: "flex items-center justify-center h-full",
                                span { class: "loading loading-spinner loading-lg" }
                            }
                        })}
                    }

                    // 右侧 Agent 列表浮层
                    {sidebar.as_ref().map(|d| {
                        rsx! {
                            div { class: "w-64 flex-shrink-0 bg-base-100 rounded-lg shadow-md overflow-y-auto",
                                div { class: "p-3 sticky top-0 bg-base-100 border-b border-base-200 z-10",
                                    div { class: "flex justify-between items-center",
                                        h3 { class: "text-sm font-semibold", "Agent 列表" }
                                        button {
                                            class: "btn btn-ghost btn-xs",
                                            onclick: move |_| { current_view.set(WorkspaceView::Global); },
                                            "全局"
                                        }
                                    }
                                }
                                div { class: "divide-y divide-base-200",
                                    for a in d.agents.iter() {
                                        {
                                            let aid = a.id.clone();
                                            let is_selected = matches!(*current_view.read(), WorkspaceView::AgentDetail(ref id) if id == &aid);
                                            let has_unread = agent_unread.read().contains(&aid);
                                            let item_class = if is_selected {
                                                "relative overflow-hidden w-full text-left p-3 hover:bg-base-200 transition-colors bg-base-200"
                                            } else {
                                                "relative overflow-hidden w-full text-left p-3 hover:bg-base-200 transition-colors"
                                            };
                                            rsx! {
                                                button {
                                                    class: "{item_class}",
                                                    onclick: move |_| {
                                                        current_view.set(WorkspaceView::AgentDetail(aid.clone()));
                                                        // 点击后清除该 agent 的提示
                                                        agent_unread.write().remove(&aid);
                                                    },
                                                    if has_unread {
                                                        span { class: "hud-streak" }
                                                    }
                                                    div { class: "flex justify-between items-start",
                                                        div { class: "flex items-center gap-1 min-w-0",
                                                            span { class: "text-sm font-medium truncate", "{a.name}" }
                                                        }
                                                        span { class: "badge badge-xs ml-2 flex-shrink-0 {agent_runtime_badge_class(a.runtime_state)}",
                                                            "{agent_runtime_label(a.runtime_state)}"
                                                        }
                                                    }
                                                    div { class: "text-xs text-base-content/60 mt-1",
                                                        "{a.kind}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if d.agents.is_empty() {
                                        div { class: "p-4 text-center text-sm text-base-content/50",
                                            "暂无 Agent"
                                        }
                                    }
                                }
                            }
                        }
                    })}
                }

                // === 消息流量时序图（最近 60 分钟本地累积） ===
                div { class: "bg-base-100 rounded-lg shadow-md p-4",
                    h3 { class: "text-sm font-semibold mb-2", "📈 消息流量（最近 60 分钟）" }
                    {
                        let flow = msg_flow.read();
                        let mut points: Vec<TimeSeriesPoint> = flow.iter()
                            .map(|(&k, &v)| TimeSeriesPoint {
                                interval_start: k,
                                tokens_input: 0,
                                tokens_output: 0,
                                call_count: v,
                            })
                            .collect();
                        points.sort_by_key(|p| p.interval_start);
                        if points.is_empty() {
                            rsx! {
                                div { class: "h-[120px] flex items-center justify-center text-base-content/50",
                                    "暂无消息流量数据"
                                }
                            }
                        } else {
                            rsx! {
                                LineChart {
                                    data: points,
                                    width: 700.0,
                                    height: 150.0,
                                    title: Some("消息量".to_string()),
                                    value_label: Some("条/分钟".to_string()),
                                }
                            }
                        }
                    }
                }

                // === 底部游戏式对话框 ===
                {
                    let focused = *chat_focused.read();
                    let msgs = chat_messages.read();
                    let is_typing = *chat_is_typing.read();
                    let input_val = chat_input.read().clone();
                    let view = current_view.read().clone();
                    let chat_title: String = match &view {
                        WorkspaceView::Global => "默认对话".to_string(),
                        WorkspaceView::ProjectDetail(pid) => {
                            let name = sidebar.as_ref()
                                .and_then(|d| d.projects.iter().find(|p| &p.id == pid))
                                .map(|p| p.name.as_str())
                                .unwrap_or("项目对话");
                            format!("{} · 项目对话", name)
                        }
                        WorkspaceView::AgentDetail(aid) => {
                            let name = sidebar.as_ref()
                                .and_then(|d| d.agents.iter().find(|a| &a.id == aid))
                                .map(|a| a.name.as_str())
                                .unwrap_or("Agent");
                            format!("{} · Agent 对话", name)
                        }
                        WorkspaceView::TaskDetail(tid) => {
                            format!("任务 {} · 对话", &tid[..tid.len().min(8)])
                        }
                    };

                    // 未聚焦：单行 + 浮动半透明最近消息
                    // 聚焦后：可滚动消息列表 + 输入框
                    let container_class = if focused {
                        "bg-base-100 rounded-lg shadow-lg border border-base-300 flex flex-col"
                    } else {
                        "bg-base-100/80 rounded-lg shadow-sm border border-base-200 flex flex-col"
                    };

                    rsx! {
                        div { class: "{container_class}",
                            // 消息显示区
                            div {
                                class: if focused { "flex-1 overflow-y-auto p-3 max-h-48" } else { "p-2 max-h-24 overflow-hidden" },
                                style: if focused { "" } else { "opacity: 0.6; mask-image: linear-gradient(to bottom, transparent 0%, black 30%, black 100%); -webkit-mask-image: linear-gradient(to bottom, transparent 0%, black 30%, black 100%);" },

                                if msgs.is_empty() && !is_typing {
                                    div { class: "text-center text-sm text-base-content/40 py-2",
                                        if focused { "💬 输入消息开始对话" } else { "💬 点击输入框开始对话" }
                                    }
                                } else {
                                    div { class: "space-y-1",
                                        for msg in msgs.iter().rev().take(if focused { 50 } else { 3 }).collect::<Vec<_>>().into_iter().rev() {
                                            MessageBubble { msg: msg.clone(), key: "{msg.message_id}" }
                                        }
                                        if is_typing {
                                            TypingIndicator {}
                                        }
                                    }
                                }
                            }

                            // 输入框区域
                            div { class: "border-t border-base-200 p-2 flex items-center gap-2",
                                if focused {
                                    // 聚焦模式：完整输入框 + 发送按钮
                                    span { class: "text-xs text-base-content/50 flex-shrink-0", "{chat_title}" }
                                    input {
                                        class: "input input-bordered input-sm flex-1",
                                        r#type: "text",
                                        placeholder: "输入消息或指令，Enter 发送...",
                                        value: "{input_val}",
                                        onfocus: move |_| chat_focused.set(true),
                                        onblur: move |_| {
                                            // 延迟失焦，允许点击发送按钮
                                            let mut chat_focused = chat_focused.clone();
                                            spawn(async move {
                                                gloo_timers::future::TimeoutFuture::new(150).await;
                                                chat_focused.set(false);
                                            });
                                        },
                                        oninput: move |e| chat_input.set(e.value()),
                                        onkeydown: move |e| {
                                            if e.key() == Key::Enter && !e.modifiers().shift() {
                                                e.prevent_default();
                                                send_trigger.set(true);
                                            }
                                        }
                                    }
                                    button {
                                        class: "btn btn-primary btn-sm",
                                        onclick: move |_| send_trigger.set(true),
                                        "发送"
                                    }
                                    button {
                                        class: "btn btn-ghost btn-xs",
                                        onclick: move |_| chat_focused.set(false),
                                        "▼"
                                    }
                                } else {
                                    // 未聚焦模式：单行小输入框 + 图例 + 刷新
                                    input {
                                        class: "input input-bordered input-sm flex-1 bg-base-200/50",
                                        r#type: "text",
                                        placeholder: "💬 {chat_title} - 点击输入...",
                                        value: "{input_val}",
                                        onfocus: move |_| chat_focused.set(true),
                                        oninput: move |e| chat_input.set(e.value()),
                                        onkeydown: move |e| {
                                            if e.key() == Key::Enter && !e.modifiers().shift() {
                                                e.prevent_default();
                                                send_trigger.set(true);
                                            }
                                        }
                                    }
                                    // 图例（未聚焦时显示）
                                    div { class: "flex gap-2 text-xs text-base-content/50",
                                        span { class: "flex items-center gap-1",
                                            span { class: "w-2 h-2 rounded-full bg-success" } "空闲"
                                        }
                                        span { class: "flex items-center gap-1",
                                            span { class: "w-2 h-2 rounded-full bg-warning" } "休息"
                                        }
                                        span { class: "flex items-center gap-1",
                                            span { class: "w-2 h-2 rounded-full bg-error" } "忙碌"
                                        }
                                    }
                                    button {
                                        class: "btn btn-ghost btn-xs",
                                        onclick: move |_| { refresh(); toast.info("已刷新数据"); },
                                        "🔄"
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
