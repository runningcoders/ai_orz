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
//! 底部游戏式横幅（MMORPG 风格，左右通透）：
//! - 左下角：当前对话上下文 / 选中 Agent 信息卡（名称 + 运行状态 + 角色标签 + 简介）
//! - 中间：对话框（消息区 + 输入行 + @ 提及浮层）；聚焦与否仅消息区展开高度不同，布局两态一致
//! - 右下角：预留区（暂放数据刷新）
//! - 视图联动（三种对话场景 + 一种锚定）：
//!   - Global = 默认对话（后端兜底到前台接待）
//!   - ProjectDetail = 项目会话：消息按 project_id 过滤，发给项目 owner（PMO）
//!   - AgentDetail = Agent 私聊：无项目域，发给该 Agent
//!   - TaskDetail = **项目会话锚定到某任务**：对话上下文与 ProjectDetail 完全一致
//!     （project_id + PMO 收件人，消息连续不割裂），额外做两件事 ——
//!     ① 发送时带 task_id：`messages` 表有该列，PMO 的 prompt 会收到 `【关联任务】`
//!     ② 进入视图时默认 @ 该任务（内容层提示，可一键摘除）
//!
//!   语义是「跟 PMO 说，由 PMO 转达给任务执行 Agent」，不对任务 Agent 做微操。
//! - @ 提及范围跟着对话上下文走（见 `MentionState::new(chat_project_id)`）：项目会话
//!   （ProjectDetail / TaskDetail）收窄为项目协作 Agent + 任务；全局 / Agent 私聊没有
//!   项目域，@ 即「全部 Agent + 全部任务 + 全部项目」。

use dioxus::prelude::*;

use common::api::GetTokenStatsRequest;

use crate::api::finance::get_token_stats;
use crate::api::hr::{list_runtime_agents, query_agents};
use crate::api::message::{load_latest_messages, load_older_messages, send_message_to_agent};
use crate::api::project::{list_project_tasks, query_projects, query_tasks};
use crate::components::charts::line_chart::{LineChart, LineChartValueField};
use crate::components::chat::{MessageBubble, TypingIndicator};
use crate::components::mention_picker::{
    MentionCandidate, MentionPickedBar, MentionPicker, MentionState, mention_kinds_for,
    mention_tabs,
};
use crate::components::state::Loading;
use crate::components::workspace_graph::{WorkspaceGraph, WorkspaceView};
use crate::hooks::use_workspace_data::{WorkspaceData, use_workspace_data};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use crate::utils::mention::{MentionKind, read_caret, restore_caret};
use crate::utils::{
    HISTORY_PAGE_SIZE, HISTORY_SCAN_MAX_PAGES, avatar_initials, build_optimistic_user_msg,
    in_project_context, replace_tmp_with_real, request_scope,
    status::{agent_runtime_badge, project_status_badge, tag_chip},
};
use common::api::{
    AgentListItem, AgentQueryRequest, MessageListItem, PaginationParams, ProjectListItem,
    ProjectQueryRequest, RuntimeListRequest, RuntimeListResponse, SendMessageToAgentParams,
    TaskListItem, TaskQueryRequest,
};
use common::enums::AssigneeType;
use common::models::TimeSeriesPoint;
use wasm_bindgen::{JsCast, closure::Closure};

/// Project 状态标签
fn project_status_label(status: i32) -> &'static str {
    match status {
        1 => "进行中",
        2 => "已完成",
        3 => "已归档",
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

/// 判断是否为运行中项目（status 3：进行中；历史值 1/2 已并入进行中）
fn is_active_project(status: i32) -> bool {
    matches!(status, 1..=3)
}

/// 运行中 Agent 过滤按钮的 active class
///
/// 当前过滤值与按钮值匹配时返回 "btn-active"，否则返回空字符串。
/// `None` 表示"全部"按钮。
fn filter_active_class(current: &Option<String>, target: Option<&str>) -> &'static str {
    let matches = match (current.as_deref(), target) {
        (Some(c), Some(t)) => c == t,
        (None, None) => true,
        _ => false,
    };
    if matches { "btn-active" } else { "" }
}

/// 根据视图计算对话上下文（project_id, task_id, to_agent_id）
///
/// `task_anchor` 是 TaskDetail 专用的「(task_id, 标题, project_id)」锚点，由图数据
/// 异步加载后回填 —— 因为任务到项目的归属关系只有 `query_tasks` 拿得到，
/// `WorkspaceData` 里没有。**必须用 task_id 校验**，否则切换任务时旧锚点会造成串台。
fn resolve_chat_context(
    view: &WorkspaceView,
    sidebar: &WorkspaceData,
    task_anchor: Option<&(String, String, Option<String>)>,
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
            // 任务视图 = 项目会话锚定到该任务：project_id 与收件人（项目 owner / PMO）
            // 都与 ProjectDetail 对齐，任务只体现在 task_id 与默认 @ 上。
            // 锚点未就绪时返回 None —— 调用方据此跳过消息加载，等锚点回填后重跑，
            // 避免先拉一次全组织消息再被覆盖（会闪一下别的会话）。
            let pid = task_anchor
                .filter(|(anchor_tid, _, _)| anchor_tid == tid)
                .and_then(|(_, _, pid)| pid.clone());
            let to_agent_id = pid.as_deref().and_then(|pid| {
                sidebar
                    .projects
                    .iter()
                    .find(|p| p.id == pid)
                    .and_then(|p| p.owner_agent_id.clone())
            });
            (pid, Some(tid.clone()), to_agent_id)
        }
    }
}

/// 底部对话框输入框 DOM id：@ 提及需要读写真实光标位置（边界判定 + 插入后复位）
const WORKSPACE_CHAT_INPUT_ID: &str = "workspace-chat-input";

/// 底部对话框消息区 DOM id：上拉加载更早历史需要读 `scrollTop`
const WORKSPACE_CHAT_SCROLL_ID: &str = "workspace-chat-scroll";

/// 单条消息是否属于当前对话上下文
///
/// 历史首屏 / 上拉翻页 / SSE 实时推送三条链路**必须同一口径**，否则会出现
/// 「实时推来的看得见、一刷新就没了」。收口成一个函数就是为了杜绝这种漂移。
///
/// - `agent_id = Some`（Agent 私聊）：唯一「按人收窄」的视图 —— 它没有项目域，
///   只能靠 to/from 圈定
/// - 其余按 `project_id` 判定：项目会话（ProjectDetail / TaskDetail）展示项目内
///   **全部**往来（含 Agent 之间的横向交流，用户是旁听者 —— ⚠️ 不可按项目 owner
///   收窄，那正是「刷新即丢失」的成因）；Global（`None`）收敛为「只显示无 project
///   的默认对话」
fn message_in_context(
    msg: &MessageListItem,
    project_id: Option<&str>,
    agent_id: Option<&str>,
) -> bool {
    if let Some(aid) = agent_id {
        return msg.to_id == aid || msg.from_id == aid;
    }
    in_project_context(msg, project_id)
}

/// 批量过滤，复用 [`message_in_context`] 的判定
fn filter_in_context(
    msgs: Vec<MessageListItem>,
    project_id: Option<&str>,
    agent_id: Option<&str>,
) -> Vec<MessageListItem> {
    msgs.into_iter()
        .filter(|m| message_in_context(m, project_id, agent_id))
        .collect()
}

/// 仅在 Agent 私聊视图下才把收件人当作过滤条件
///
/// ⚠️ 项目会话的 `to_agent_id` 也是 `Some`（项目 owner / PMO），直接拿去当过滤器
/// 会把 Agent 之间的横向交流全滤掉 —— 这里显式只在 AgentDetail 生效。
fn context_agent_filter(view: &WorkspaceView, to_agent_id: Option<String>) -> Option<String> {
    match view {
        WorkspaceView::AgentDetail(_) => to_agent_id,
        _ => None,
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
    let chat_messages = use_signal(Vec::<MessageListItem>::new);
    let mut chat_input = use_signal(String::new);
    let mut chat_focused = use_signal(|| false);
    let chat_is_typing = use_signal(|| false);
    let chat_project_id = use_signal(|| Option::<String>::None);
    let chat_task_id = use_signal(|| Option::<String>::None);
    let chat_to_agent_id = use_signal(|| Option::<String>::None);
    // 历史分页：是否还有更早的消息 / 是否正在上拉加载（滚到消息区顶部按
    // `before_timestamp` 续拉，因此「首屏 50 条」只是页大小，不是历史深度上限）
    let mut chat_has_more = use_signal(|| true);
    let chat_loading_older = use_signal(|| false);

    // 任务视图锚点：(task_id, 任务标题, project_id)，由 TaskDetail 的图数据加载回填。
    // 用途：① 把任务视图解析成「项目会话 + 该任务」（`resolve_chat_context`）
    //       ② 进入任务视图时默认 @ 该任务（见下方「任务视图锚定」effect）
    let mut task_anchor = use_signal(|| Option::<(String, String, Option<String>)>::None);
    // 上一次由锚定 effect 自动写入输入框的**完整文本**，用于判断「内容是否仍是我写的」
    // —— 只有输入框为空、或内容与它完全一致时才允许覆盖，绝不碰用户正在输入的内容。
    let auto_mention = use_signal(|| Option::<String>::None);

    // @ 提及：与对话页（pages/message/chat.rs）共用同一套状态机（触发判定 / 候选加载 /
    // 键盘导航 / 已提及胶囊）。候选范围直接跟随对话上下文 chat_project_id：
    // - ProjectDetail / TaskDetail = 项目会话口径：项目协作 Agent + 任务
    // - Global / AgentDetail = 无项目域：组织全量 Agent + 任务 + 项目
    // 也就是说「@ 能选什么」不需要单独一套规则，视图切到哪儿就跟着哪儿。
    let mention = MentionState::new(chat_project_id);

    // 侧边栏红点提示：收到新消息但不在当前视图时，对应 project/agent 亮红点
    let mut project_unread = use_signal(std::collections::HashSet::<String>::new);
    let mut agent_unread = use_signal(std::collections::HashSet::<String>::new);

    // 组织级分钟级 Token 消耗时序（后端 DuckDB 查询，30 秒轮询；顶栏 QPS 曲线用）
    let token_series: Signal<Vec<TimeSeriesPoint>> = use_signal(Vec::new);

    // 运行中 Agent 列表（轮询 runtime-list 接口）
    let runtime_agents = use_signal(RuntimeListResponse::default);
    let mut runtime_filter = use_signal(|| None::<String>);

    // HUD 悬浮面板折叠状态
    let mut project_panel_collapsed = use_signal(|| false);
    let mut agent_panel_collapsed = use_signal(|| false);

    let sidebar = sidebar_signal.read().clone();

    // 运行中 Agent 轮询：5 秒间隔，支持状态过滤
    use_future(move || {
        let mut runtime_agents = runtime_agents;
        let runtime_filter = runtime_filter;
        async move {
            loop {
                let req = RuntimeListRequest {
                    state: runtime_filter(),
                    task_id: None,
                    project_id: None,
                };
                if let Ok(resp) = list_runtime_agents(&req).await {
                    runtime_agents.set(resp);
                }
                gloo_timers::future::TimeoutFuture::new(5000).await;
            }
        }
    });

    // Token 消耗轮询：30 秒间隔，拉最近 60 分钟的分钟级时序（顶栏 QPS 曲线）
    //
    // 注意：统计事件是批次刷盘，最近 1~2 分钟可能尚未落库，曲线末端偏低属预期。
    use_future(move || {
        let mut token_series = token_series;
        async move {
            loop {
                if let Ok(resp) = get_token_stats(GetTokenStatsRequest { minutes: Some(60) }).await
                {
                    token_series.set(resp.points);
                }
                gloo_timers::future::TimeoutFuture::new(30_000).await;
            }
        }
    });

    // 视图变化时按需加载图数据
    use_effect(move || {
        let view = current_view.read().clone();
        let live_view = current_view; // 捕获信号副本，供 spawn 内检测视图是否已切换
        let sidebar_data = sidebar_signal.read().clone();
        let toast = toast;

        spawn(async move {
            graph_loading.set(true);
            let my_view = view.clone();
            // 视图切换守卫：快速切换视图时，旧视图的异步加载可能晚于新视图完成并覆盖
            // 图数据。每次 await 后用 guard!() 检查当前视图是否仍是本任务目标，若已切换
            // 则放弃本次结果，避免过期数据污染当前视图。
            macro_rules! guard {
                () => {
                    if live_view.read().clone() != my_view {
                        graph_loading.set(false);
                        return;
                    }
                };
            }

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
                            guard!();
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
                            guard!();
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
                                    Ok(page) => {
                                        guard!();
                                        graph_agents.set(page.items)
                                    }
                                    Err(e) => toast.error(format!("批量获取 Agent 失败: {}", e)),
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
                        Err(e) => toast.error(format!("获取项目任务失败: {}", e)),
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
                            guard!();
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
                                    Ok(page) => {
                                        guard!();
                                        graph_projects.set(page.items)
                                    }
                                    Err(e) => toast.error(format!("批量获取项目失败: {}", e)),
                                }
                            }
                        }
                        Err(e) => toast.error(format!("获取任务列表失败: {}", e)),
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
                            guard!();
                            if let Some(task) = page.items.into_iter().next() {
                                let pid = task.project_id.clone();
                                let assignee_type = task.assignee_type;
                                let assignee_id = task.assignee_id.clone();

                                // 回填任务锚点：对话上下文据此解析为「项目会话 + 该任务」，
                                // 锚定 effect 据此默认 @ 该任务（task_id 校验防跨任务串台）
                                task_anchor.set(Some((
                                    task.id.clone(),
                                    task.title.clone(),
                                    pid.clone(),
                                )));

                                // 2. 加载同 project 的 tasks（用于依赖 DAG）
                                if let Some(pid) = &pid {
                                    match list_project_tasks(pid).await {
                                        Ok(resp) => {
                                            guard!();
                                            graph_tasks.set(resp.tasks)
                                        }
                                        Err(e) => toast.error(format!("获取项目任务失败: {}", e)),
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
                                            guard!();
                                            if let Some(a) = page.items.into_iter().next() {
                                                graph_agents.set(vec![a]);
                                            }
                                        }
                                        Err(e) => toast.error(format!("获取 Agent 失败: {}", e)),
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
                                            guard!();
                                            if let Some(p) = page.items.into_iter().next() {
                                                graph_projects.set(vec![p]);
                                            }
                                        }
                                        Err(e) => toast.error(format!("获取 Project 失败: {}", e)),
                                    }
                                }
                            }
                        }
                        Err(e) => toast.error(format!("获取 Task 失败: {}", e)),
                    }
                }
            }

            graph_loading.set(false);
        });
    });

    // 视图变化时重新加载对话消息 + 更新对话上下文
    {
        let mut chat_messages = chat_messages;
        let mut chat_project_id = chat_project_id;
        let mut chat_task_id = chat_task_id;
        let mut chat_to_agent_id = chat_to_agent_id;

        use_effect(move || {
            let view = current_view.read().clone();
            let sidebar_data = sidebar_signal.read().clone();
            // 任务锚点（TaskDetail 专用）：随图数据异步回填，回填后本 effect 自动重跑
            let anchor = task_anchor.read().clone();

            // 计算新的对话上下文
            let (pid, tid, aid) = if let Some(data) = &sidebar_data {
                resolve_chat_context(&view, data, anchor.as_ref())
            } else {
                (None, None, None)
            };

            // 任务视图的 project_id 依赖异步锚点：未就绪时先不拉消息，
            // 否则会先闪一次「全组织最近 20 条」再被项目会话覆盖。
            if matches!(view, WorkspaceView::TaskDetail(_)) && pid.is_none() {
                chat_project_id.set(None);
                chat_task_id.set(tid.clone());
                chat_to_agent_id.set(None);
                chat_messages.set(Vec::new());
                chat_has_more.set(false);
                return;
            }

            chat_project_id.set(pid.clone());
            chat_task_id.set(tid.clone());
            chat_to_agent_id.set(aid.clone());

            // 加载首屏历史消息（页大小 HISTORY_PAGE_SIZE，更早的靠上拉续拉）
            let pid_clone = pid.clone();
            // Agent 私聊是唯一「按人收窄」的视图（见 `context_agent_filter`）
            let agent_only = context_agent_filter(&view, aid.clone());
            // 请求侧 project_id（见 `utils::message::request_scope`）：
            // - Global（pid = None，默认对话）→ 哨兵值，后端精确返回 `project_id IS NULL`
            // - Agent 私聊 → 必须保持 None（= 不过滤，另按 to/from 收窄），否则会丢掉
            //   带 project_id 的私聊消息
            // - 项目 / 任务会话 → 项目 id，原样透传
            let req_pid = if matches!(view, WorkspaceView::AgentDetail(_)) {
                None
            } else {
                request_scope(pid.as_deref())
            };
            spawn(async move {
                // 首屏也可能「整页都不可见」：Agent 私聊的可见性收敛在前端
                // （后端 to/from 是「或」关系），一页原始消息有可能被滤到一条不剩，
                // 那时页面既空白、又没有可滚动区域（滚动事件永远不会来）。
                // 因而沿用与上拉一致的策略：整页被滤空就继续往前翻，最多 N 页。
                let mut visible = Vec::new();
                let mut cursor: Option<i64> = None;
                let mut full_page = false;
                for _ in 0..HISTORY_SCAN_MAX_PAGES {
                    let req = common::api::ListMessagesRequest {
                        project_id: req_pid.clone(),
                        before_timestamp: cursor,
                        limit: Some(HISTORY_PAGE_SIZE),
                        ..Default::default()
                    };
                    let resp = match if cursor.is_some() {
                        load_older_messages(req).await
                    } else {
                        load_latest_messages(req).await
                    } {
                        Ok(resp) => resp,
                        Err(_) => break,
                    };
                    // 满页 ⇒ 可能还有更早的（来源是 DESC，已由后端反转为 ASC）
                    full_page = resp.messages.len() >= HISTORY_PAGE_SIZE;
                    let next_cursor = resp.messages.first().map(|m| m.created_at);
                    visible = filter_in_context(
                        resp.messages,
                        pid_clone.as_deref(),
                        agent_only.as_deref(),
                    );
                    if !visible.is_empty() || !full_page {
                        break;
                    }
                    match next_cursor {
                        Some(ts) => cursor = Some(ts),
                        None => {
                            full_page = false;
                            break;
                        }
                    }
                }
                chat_messages.set(visible);
                chat_has_more.set(full_page);
            });
        });
    }

    // 上拉加载更早的历史消息（滚到消息区顶部触发）
    //
    // `before_timestamp` 是**开区间**（后端取 `created_at < before`），所以用当前
    // 首条消息的时间戳做游标不会重复拉回同一条。
    // ⚠️ Agent 私聊拿不到精确的服务端过滤（它的 to/from 是「或」关系，后端按
    // project 过滤帮不上忙），一页原始消息可能被过滤到一条不剩。这时如果直接停手，
    // 用户既看不到新内容、也不会再有滚动事件 —— 所以整页被滤空时继续往前翻，
    // 最多 MAX_SCAN 页，避免长卡顿。
    // （Global 自「默认对话哨兵值」改造后已由后端精确返回，这段兜底对它不再触发。）
    let load_older = move || {
        if !chat_has_more() || chat_loading_older() {
            return;
        }
        let pid = chat_project_id();
        let view = current_view.read().clone();
        let agent_only = context_agent_filter(&view, chat_to_agent_id());
        // 请求侧 project_id：与首屏同口径（Global 用哨兵、Agent 私聊保持 None）
        let req_pid = if matches!(view, WorkspaceView::AgentDetail(_)) {
            None
        } else {
            request_scope(pid.as_deref())
        };
        let Some(mut before) = chat_messages.read().first().map(|m| m.created_at) else {
            return;
        };
        const MAX_SCAN_PAGES: usize = HISTORY_SCAN_MAX_PAGES;

        let mut chat_messages = chat_messages;
        let mut chat_has_more = chat_has_more;
        let mut chat_loading_older = chat_loading_older;
        chat_loading_older.set(true);
        spawn(async move {
            for _ in 0..MAX_SCAN_PAGES {
                let resp = match load_older_messages(common::api::ListMessagesRequest {
                    project_id: req_pid.clone(),
                    before_timestamp: Some(before),
                    limit: Some(HISTORY_PAGE_SIZE),
                    ..Default::default()
                })
                .await
                {
                    Ok(resp) => resp,
                    Err(e) => {
                        toast.error(format!("加载更早消息失败: {e}"));
                        break;
                    }
                };

                let full_page = resp.messages.len() >= HISTORY_PAGE_SIZE;
                // 本页最早的时间戳（ASC 排序）→ 下一页的游标；先取再消费 messages
                let next_before = resp.messages.first().map(|m| m.created_at);
                let older = filter_in_context(resp.messages, pid.as_deref(), agent_only.as_deref());

                let has_visible = !older.is_empty();
                if has_visible {
                    let mut current = chat_messages.write();
                    let mut merged = older;
                    merged.append(&mut current);
                    *current = merged;
                }

                // 不满页 ⇒ 到头了；否则用本页游标继续（整页被滤空时也要继续）
                if !full_page {
                    chat_has_more.set(false);
                    break;
                }
                if has_visible {
                    break;
                }
                match next_before {
                    Some(ts) => before = ts,
                    None => {
                        chat_has_more.set(false);
                        break;
                    }
                }
            }
            chat_loading_older.set(false);
        });
    };

    // 任务视图锚定：进入某任务视图时默认在输入框 @ 该任务（内容层提示，可一键摘除）。
    // 结构化通道由 `task_id` 承担（发送时随消息落库，PMO 的 prompt 带【关联任务】），
    // 这里补的是「人也能一眼看到当前在说哪个任务」。
    //
    // 只在「输入框为空」或「内容与上一次自动写入的完全一致」时才写入 —— 绝不打断用户输入。
    // 读输入用 peek（不订阅），否则每次打字都会重跑本 effect。
    {
        let mut chat_input = chat_input;
        let mut auto_mention = auto_mention;
        use_effect(move || {
            let Some((tid, title, _)) = task_anchor.read().clone() else {
                return;
            };
            // 仅当视图确实停在该任务上才锚定（图数据回填可能滞后于视图切换）
            let is_current =
                matches!(&*current_view.read(), WorkspaceView::TaskDetail(v) if v == &tid);
            if !is_current {
                return;
            }

            let current = chat_input.peek().clone();
            let trimmed = current.trim();
            let is_mine = auto_mention.peek().as_deref().map(str::trim) == Some(trimmed);
            if !trimmed.is_empty() && !is_mine {
                return;
            }

            let item = MentionCandidate {
                kind: MentionKind::Task,
                id: tid.clone(),
                org: None,
                name: title.clone(),
                subtitle: String::new(),
            };
            // 换任务时先摘掉上一轮自动写入的提及，避免残留或重复
            mention.reset_picked();
            let text = format!("{} ", mention.preset(item));
            auto_mention.set(Some(text.clone()));
            chat_input.set(text);
        });
    }

    // SSE 订阅实时消息
    {
        let mut chat_messages = chat_messages;
        let mut project_unread = project_unread;
        let mut agent_unread = agent_unread;

        // SSE 资源：EventSource + Closure 供顶层 use_drop 清理
        struct WsSseResource {
            event_source: web_sys::EventSource,
            on_message: Closure<dyn FnMut(web_sys::MessageEvent)>,
        }
        let mut ws_sse_resource = use_signal(|| Option::<WsSseResource>::None);

        use_effect(move || {
            let on_message = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
                if let Some(data) = event.data().as_string()
                    && let Ok(msg) = serde_json::from_str::<MessageListItem>(&data)
                {
                    let mut msgs = chat_messages.write();
                    // 移除同 content 的乐观消息（统一使用 replace_tmp_with_real）
                    replace_tmp_with_real(&mut msgs, &msg);

                    // 过滤口径与历史首屏 / 上拉翻页共用同一个判定
                    // （`message_in_context`），三处必须一致，否则会出现
                    // 「实时推来的看得到、一刷新就消失」。
                    let cur_pid = chat_project_id.read().clone();
                    // raw_aid 供红点判定（保持「当前对话对象」原义），
                    // cur_aid 是过滤用的（仅 Agent 私聊才收窄，见 context_agent_filter）
                    let raw_aid = chat_to_agent_id.read().clone();
                    let cur_aid =
                        context_agent_filter(&current_view.read().clone(), raw_aid.clone());

                    let matched = message_in_context(&msg, cur_pid.as_deref(), cur_aid.as_deref());

                    if matched {
                        // 去重
                        if !msgs.iter().any(|m| m.message_id == msg.message_id) {
                            msgs.push(msg);
                            // 内存上限保护：超出后丢掉**最旧**的（上拉可再翻回来，
                            // 不是历史深度上限）
                            const MAX_KEEP: usize = 500;
                            if msgs.len() > MAX_KEEP {
                                let drain = msgs.len() - MAX_KEEP;
                                msgs.drain(..drain);
                            }
                        }
                    } else {
                        // 不属于当前视图 → 更新侧边栏红点
                        // Agent 回复消息（from_role=1）才触发红点
                        if msg.from_role == 1 {
                            if let Some(pid) = &msg.project_id
                                && cur_pid.as_deref() != Some(pid)
                            {
                                project_unread.write().insert(pid.clone());
                            }
                            let sender_aid = &msg.from_id;
                            if raw_aid.as_deref() != Some(sender_aid) {
                                agent_unread.write().insert(sender_aid.clone());
                            }
                        }
                    }
                }
            }) as Box<dyn FnMut(_)>);

            if let Ok(es) = web_sys::EventSource::new("/api/v1/finance/messages/sse") {
                es.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
                ws_sse_resource.set(Some(WsSseResource {
                    event_source: es,
                    on_message,
                }));
            }
        });

        use_drop(move || {
            if let Some(res) = ws_sse_resource.take() {
                res.event_source.set_onmessage(None);
                drop(res.on_message);
                res.event_source.close();
            }
        });
    }

    // 发送消息的信号触发器
    let mut send_trigger = use_signal(|| false);

    // 发送消息
    {
        let mut chat_input = chat_input;
        let mut chat_messages = chat_messages;
        let mut chat_is_typing = chat_is_typing;
        let mut send_trigger = send_trigger;
        let mut auto_mention = auto_mention;

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
            // 提及已随正文一起发出，「已提及」记录只服务于本次输入；
            // 锚定记录一并清空，否则下次会自动复用上一次的判定基准
            mention.reset_picked();
            auto_mention.set(None);
            chat_is_typing.set(true);

            {
                let mut chat_is_typing = chat_is_typing;
                // 一次性延时复位：用 callback::Timeout + forget() 泄露安全，
                // 避免 spawn + TimeoutFuture 在组件卸载时 drop 触发 "closure invoked recursively or after being dropped"。
                gloo_timers::callback::Timeout::new(60_000, move || {
                    chat_is_typing.set(false);
                })
                .forget();
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
                        toast.error(format!("发送消息失败: {}", e));
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
            // 修复：原写法 `relative h-full w-full overflow-hidden` 在 `AppLayout` 的
            // `min-h-screen flex flex-col` + main `flex-1` 父容器下，`h-full` 因为父
            // 高度 `min-height:100vh` + `height:auto` 对子级百分比解析为不定（definite
            // size）而坍塌为 0，导致该 div 内部所有 `absolute inset-0` 子元素（关系图
            // 全屏背景 / HUD 面板 / 加载遮罩）被 `overflow-hidden` 裁掉，页面整片空白。
            // 改用 `absolute inset-0`：锚定到 `position:relative` 的 main，绕过百分比
            // 高度解析、必然填满 `100vh - navbar` 区域（main 由 flex-1 保证该高度）。
            // 配套把 WorkspaceGraph 内部根容器也改为 `absolute inset-0` 一致铺满，
            // 避免它再依赖 `h-full`。
            div { class: "absolute inset-0 overflow-hidden",
                // === 关系图全屏背景层（HUD 底层，透明 canvas 透出主题底） ===
                div { class: "absolute inset-0 z-0",
                    WorkspaceGraph {
                        view: current_view.read().clone(),
                        projects: gp.clone(),
                        agents: ga.clone(),
                        tasks: gt.clone(),
                        width: 800.0,
                        height: 600.0,
                        auto_size: true,
                        on_view_change: Some(EventHandler::new(move |new_view: WorkspaceView| {
                            current_view.set(new_view);
                        })),
                    }
                }

                // 图数据加载遮罩
                if loading {
                    div { class: "absolute inset-0 z-20 flex items-center justify-center bg-base-100/40",
                        Loading { size: "lg" }
                    }
                }

                // === 顶部状态栏（玻璃，悬浮顶部） ===
                {sidebar.as_ref().map(|d| {
                    let project_count = d.projects.len();
                    let agent_count = d.agents.len();
                    let active_project_count = d.projects.iter().filter(|p| is_active_project(p.status)).count();
                    let busy_agent_count = d.agents.iter().filter(|a| a.runtime_state == 2).count();

                    // 运行中 Agent 实时状态计数
                    let ra = runtime_agents.read().clone();
                    let idle_n = ra.items.iter().filter(|i| i.state == "idle").count();
                    let busy_n = ra.items.iter().filter(|i| i.state == "busy").count();
                    let rest_n = ra.items.iter().filter(|i| i.state == "resting").count();

                    // Token QPS 迷你图数据（后端分钟级时序，最近 60 分钟）
                    let points = token_series.read().clone();

                    rsx! {
                        div { class: "absolute top-3 left-3 right-3 z-10 hud-glass rounded-xl px-4 py-2 flex items-center gap-4 flex-wrap",
                            // 4 个概览指标
                            div { class: "flex items-center gap-3",
                                div { class: "text-center",
                                    div { class: "text-xs text-base-content/60", "项目" }
                                    div { class: "text-lg font-semibold text-primary", "{project_count}" }
                                }
                                div { class: "text-center",
                                    div { class: "text-xs text-base-content/60", "Agent" }
                                    div { class: "text-lg font-semibold text-info", "{agent_count}" }
                                }
                                div { class: "text-center",
                                    div { class: "text-xs text-base-content/60", "运行中" }
                                    div { class: "text-lg font-semibold text-secondary", "{active_project_count}" }
                                }
                                div { class: "text-center",
                                    div { class: "text-xs text-base-content/60", "忙碌" }
                                    div { class: "text-lg font-semibold text-error", "{busy_agent_count}" }
                                }
                            }
                            div { class: "h-8 w-px bg-base-content/15" }
                            // 运行中 Agent 实时状态
                            div { class: "flex items-center gap-3 text-xs",
                                span { class: "flex items-center gap-1",
                                    span { class: "w-2 h-2 rounded-full bg-success" } "空闲 {idle_n}"
                                }
                                span { class: "flex items-center gap-1",
                                    span { class: "w-2 h-2 rounded-full bg-warning" } "思考 {busy_n}"
                                }
                                span { class: "flex items-center gap-1",
                                    span { class: "w-2 h-2 rounded-full bg-error" } "休息 {rest_n}"
                                }
                            }
                            // 流量迷你图
                            div { class: "ml-auto h-12 flex items-center",
                                if points.is_empty() {
                                    span { class: "text-xs text-base-content/40", "暂无消耗" }
                                } else {
                                    LineChart {
                                        data: points,
                                        width: Some(160.0),
                                        height: Some(48.0),
                                        title: None,
                                        value_label: Some("Token/s".to_string()),
                                        value_field: Some(LineChartValueField::TokensPerSecond),
                                    }
                                }
                            }
                        }
                    }
                })}

                // === 左侧项目面板（玻璃，悬浮左，可折叠） ===
                {sidebar.as_ref().map(|d| {
                    let collapsed = *project_panel_collapsed.read();
                    rsx! {
                        div {
                            class: if collapsed {
                                "absolute left-3 top-24 bottom-40 z-10 w-12 hud-glass rounded-xl flex flex-col items-center py-2"
                            } else {
                                "absolute left-3 top-24 bottom-40 z-10 w-64 hud-glass rounded-xl flex flex-col overflow-hidden"
                            },
                            div { class: "hud-panel-header p-3 border-b border-base-content/10",
                                if !collapsed {
                                    h3 { class: "text-sm font-semibold", "项目列表" }
                                }
                                div { class: "flex items-center gap-1",
                                    button {
                                        class: "hud-collapse-btn text-sm",
                                        onclick: move |_| project_panel_collapsed.set(!collapsed),
                                        if collapsed { "▶" } else { "◀" }
                                    }
                                    if !collapsed {
                                        button {
                                            class: "btn hud-btn btn-ghost btn-xs",
                                            onclick: move |_| { current_view.set(WorkspaceView::Global); },
                                            "全局"
                                        }
                                    }
                                }
                            }
                            if !collapsed {
                                div { class: "flex-1 overflow-y-auto divide-y divide-base-200",
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
                                                        project_unread.write().remove(&pid);
                                                    },
                                                    if has_unread {
                                                        span { class: "hud-streak" }
                                                    }
                                                    div { class: "flex justify-between items-start",
                                                        div { class: "flex items-center gap-1 min-w-0",
                                                            span { class: "text-sm font-medium truncate", "{p.name}" }
                                                        }
                                                        span { class: "{project_status_badge(p.status)} ml-2 flex-shrink-0",
                                                            "{project_status_label(p.status)}"
                                                        }
                                                    }
                                                    if !p.tags.is_empty() {
                                                        div { class: "flex flex-wrap gap-1 mt-1",
                                                            for tag in p.tags.iter().take(2) {
                                                                span { class: "{tag_chip()}", "{tag}" }
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
                    }
                })}

                // === 右侧 Agent 面板（玻璃，悬浮右，可折叠 + 状态过滤） ===
                {sidebar.as_ref().map(|d| {
                    let collapsed = *agent_panel_collapsed.read();
                    let ra_filter = runtime_filter.read().clone();
                    rsx! {
                        div {
                            class: if collapsed {
                                "absolute right-3 top-24 bottom-40 z-10 w-12 hud-glass rounded-xl flex flex-col items-center py-2"
                            } else {
                                "absolute right-3 top-24 bottom-40 z-10 w-64 hud-glass rounded-xl flex flex-col overflow-hidden"
                            },
                            div { class: "hud-panel-header p-3 border-b border-base-content/10",
                                if !collapsed {
                                    h3 { class: "text-sm font-semibold", "Agent 列表" }
                                }
                                div { class: "flex items-center gap-1",
                                    button {
                                        class: "hud-collapse-btn text-sm",
                                        onclick: move |_| agent_panel_collapsed.set(!collapsed),
                                        if collapsed { "◀" } else { "▶" }
                                    }
                                    if !collapsed {
                                        button {
                                            class: "btn hud-btn btn-ghost btn-xs",
                                            onclick: move |_| { current_view.set(WorkspaceView::Global); },
                                            "全局"
                                        }
                                    }
                                }
                            }
                            if !collapsed {
                                // 状态过滤（原独立"运行中 Agent"卡片的过滤迁入）
                                div { class: "flex gap-1 px-3 py-2 border-b border-base-content/10",
                                    button {
                                        class: "btn hud-btn btn-xs join-item {filter_active_class(&ra_filter, None)}",
                                        onclick: move |_| runtime_filter.set(None),
                                        "全部"
                                    }
                                    button {
                                        class: "btn hud-btn btn-xs join-item {filter_active_class(&ra_filter, Some(\"busy\"))}",
                                        onclick: move |_| runtime_filter.set(Some("busy".to_string())),
                                        "思考"
                                    }
                                    button {
                                        class: "btn hud-btn btn-xs join-item {filter_active_class(&ra_filter, Some(\"resting\"))}",
                                        onclick: move |_| runtime_filter.set(Some("resting".to_string())),
                                        "休息"
                                    }
                                }
                                div { class: "flex-1 overflow-y-auto divide-y divide-base-200",
                                    for a in d.agents.iter().filter(|a| match ra_filter.as_deref() {
                                        None => true,
                                        Some("busy") => a.runtime_state == 2,
                                        Some("resting") => a.runtime_state == 1,
                                        _ => true,
                                    }) {
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
                                                        agent_unread.write().remove(&aid);
                                                    },
                                                    if has_unread {
                                                        span { class: "hud-streak" }
                                                    }
                                                    div { class: "flex justify-between items-start",
                                                        div { class: "flex items-center gap-1 min-w-0",
                                                            span { class: "text-sm font-medium truncate", "{a.name}" }
                                                        }
                                                        span { class: "ml-2 flex-shrink-0 {agent_runtime_badge(a.runtime_state)}",
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
                    }
                })}

                // === 底部横幅（MMORPG 式：左下 Agent 卡 / 中间对话框 / 右下预留，左右通透） ===
                {
                    let focused = *chat_focused.read();
                    let msgs = chat_messages.read();
                    let is_typing = *chat_is_typing.read();
                    let input_val = chat_input.read().clone();
                    let view = current_view.read().clone();
                    // 群聊（项目会话）里消息不止「你 ↔ 一个 Agent」两条线，Agent 之间也会
                    // 互相说话 —— 气泡头部拼出接收方（复用正文 @ 提及的 chip 写法），
                    // 配合旁观消息降透明度，一眼看清谁在跟谁聊。
                    // 1:1 视图（默认对话 / Agent 私聊）收件人是显然的，不重复展示。
                    let show_receiver = matches!(
                        view,
                        WorkspaceView::ProjectDetail(_) | WorkspaceView::TaskDetail(_)
                    );
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
                            // 有锚点用真实标题（更可读），否则退回 ID 前 8 位
                            match task_anchor().filter(|(anchor_tid, _, _)| anchor_tid == tid) {
                                Some((_, title, _)) => format!("{} · 项目对话", title),
                                None => format!("任务 {} · 对话", &tid[..tid.len().min(8)]),
                            }
                        }
                    };
                    // 左下角信息卡：AgentDetail 视图展示选中 Agent；其余视图展示对话上下文
                    let selected_agent = match &view {
                        WorkspaceView::AgentDetail(aid) => sidebar
                            .as_ref()
                            .and_then(|d| d.agents.iter().find(|a| &a.id == aid)),
                        _ => None,
                    };

                    // @ 提及浮层：鼠标点选候选（组件已先把高亮对齐到鼠标项）→ 写回文本 + 复位光标
                    let mut mention_input = chat_input;
                    let on_pick_mention = Callback::new(move |_item: MentionCandidate| {
                        if let Some((text, caret)) = mention.confirm(&mention_input()) {
                            mention_input.set(text);
                            restore_caret(WORKSPACE_CHAT_INPUT_ID, caret);
                        }
                    });
                    // 摘掉「已提及」胶囊：同步把正文里的提及语法串删掉
                    let mut mention_remove_input = chat_input;
                    let mut mention_remove_auto = auto_mention;
                    let on_remove_mention = Callback::new(move |key: String| {
                        let text = mention.remove_picked(&mention_remove_input(), &key);
                        mention_remove_input.set(text);
                        // 用户手动摘除后，不再视为「自动写入的内容」，
                        // 否则切任务时会被当成可覆盖的旧锚定
                        mention_remove_auto.set(None);
                    });
                    // 类型 Tab 随上下文变化（项目会话时不再允许 @ 项目本身）
                    let mention_tab_list =
                        mention_tabs(&mention_kinds_for(chat_project_id().as_deref()));

                    rsx! {
                        // 容器整体透明 + pointer-events-none：除三个子区间外可穿透到关系图（左右通透）
                        div { class: "absolute bottom-3 left-3 right-3 z-10 flex items-end gap-3 pointer-events-none",

                            // ---- 左下角：当前对话上下文 / 选中 Agent 信息卡（与左右侧栏同宽对齐） ----
                            div { class: "pointer-events-auto w-64 flex-shrink-0 hud-glass rounded-xl p-3",
                                if let Some(a) = selected_agent {
                                    div { class: "flex items-center gap-2 min-w-0",
                                        div { class: "w-8 h-8 rounded-full bg-primary/15 border border-primary/40 flex items-center justify-center text-xs font-semibold text-primary flex-shrink-0",
                                            "{avatar_initials(&a.name)}"
                                        }
                                        span { class: "text-sm font-medium truncate", "{a.name}" }
                                        span { class: "ml-auto flex-shrink-0 {agent_runtime_badge(a.runtime_state)}",
                                            "{agent_runtime_label(a.runtime_state)}"
                                        }
                                    }
                                    if !a.roles.is_empty() {
                                        div { class: "flex flex-wrap gap-1 mt-1.5",
                                            for role in a.roles.iter().take(3) {
                                                span { class: "{tag_chip()}", "{role}" }
                                            }
                                        }
                                    }
                                    if let Some(desc) = &a.description {
                                        p { class: "text-xs text-base-content/60 mt-1.5 line-clamp-2 leading-relaxed",
                                            "{desc}"
                                        }
                                    }
                                } else {
                                    div { class: "text-sm font-medium", "{chat_title}" }
                                    div { class: "text-xs text-base-content/50 mt-1",
                                        "点击图中项目 / Agent / 任务节点，切换对话上下文"
                                    }
                                }
                            }

                            // ---- 中间：对话框；聚焦与否仅消息区展开高度不同，其余两态完全一致 ----
                            // ⚠️ 这里刻意不加 `overflow-hidden`：@ 提及浮层（`.mention-menu` 用
                            // `bottom: 100%` 悬在输入行上方）属于「溢出父容器」的定位元素，
                            // 一旦父级裁剪就会被整块切掉，菜单永远看不见。圆角改为交给消息区
                            // 自身 `rounded-t-xl` 裁剪（滚动条也随之内收）。
                            div {
                                id: "workspace-chat-dialog",
                                class: "pointer-events-auto relative flex-1 min-w-0 hud-glass rounded-xl border border-base-content/10 flex flex-col",
                                onclick: move |_| {
                                    chat_focused.set(true);
                                    // 点击对话框任意位置即聚焦输入框
                                    if let Some(window) = web_sys::window()
                                        && let Some(doc) = window.document()
                                        && let Some(el) = doc.get_element_by_id(WORKSPACE_CHAT_INPUT_ID) {
                                        let _ = el.dyn_into::<web_sys::HtmlElement>().map(|h| h.focus());
                                    }
                                },
                                // 消息区：未聚焦收起为最近 3 条 + 渐隐蒙版；聚焦展开为可滚动列表
                                // 聚焦态滚到顶 → 上拉续拉更早的历史（页大小 HISTORY_PAGE_SIZE，
                                // 因此「一次拉多少」与「能看到多早」解耦）
                                div {
                                    id: WORKSPACE_CHAT_SCROLL_ID,
                                    class: if focused { "overflow-y-auto p-3 max-h-48 rounded-t-xl" } else { "p-2 max-h-24 overflow-hidden rounded-t-xl" },
                                    style: if focused { "" } else { "opacity: 0.7; mask-image: linear-gradient(to bottom, transparent 0%, black 30%, black 100%); -webkit-mask-image: linear-gradient(to bottom, transparent 0%, black 30%, black 100%);" },
                                    onscroll: move |e| {
                                        if e.scroll_top() == 0.0 {
                                            load_older();
                                        }
                                    },
                                    if msgs.is_empty() && !is_typing {
                                        div { class: "text-center text-sm text-base-content/40 py-2",
                                            "💬 输入消息开始对话"
                                        }
                                    } else {
                                        div { class: "space-y-1",
                                            // 聚焦时渲染全部**已加载**消息（越往上越早，
                                            // 由 load_older 逐页追加）；未聚焦只留最近 3 条
                                            for msg in (if focused { msgs.iter().collect::<Vec<_>>() } else { msgs.iter().rev().take(3).rev().collect::<Vec<_>>() }).into_iter() {
                                                MessageBubble {
                                                    msg: msg.clone(),
                                                    show_receiver,
                                                    key: "{msg.message_id}",
                                                }
                                            }
                                            if is_typing {
                                                TypingIndicator {}
                                            }
                                        }
                                    }
                                }
                                // 输入区：@ 提及浮层与输入行同属一个 relative 容器，
                                // 浮层 `.mention-menu` 的 `bottom: 100%` 即贴着输入行上沿
                                // 向上展开（悬在消息区之上，与对话页观感一致）
                                div { class: "relative",
                                    if mention.is_open() {
                                        MentionPicker {
                                            state: mention,
                                            tabs: mention_tab_list.clone(),
                                            on_pick: on_pick_mention,
                                        }
                                    }
                                    MentionPickedBar {
                                        picked: mention.picked(),
                                        on_remove: on_remove_mention,
                                    }
                                    // 输入行：两态完全一致（无标题切换 / 无收起按钮）
                                    div { class: "border-t border-base-content/10 p-2 flex items-end gap-2",
                                        textarea {
                                            class: "textarea textarea-bordered flex-1 bg-transparent resize-none",
                                            id: WORKSPACE_CHAT_INPUT_ID,
                                            rows: "1",
                                            placeholder: "输入消息（Alt+回车发送，@ 提及 Agent / 任务 / 项目）...",
                                            value: "{input_val}",
                                            onfocus: move |_| chat_focused.set(true),
                                            onblur: move |_| {
                                                let mut chat_focused = chat_focused;
                                                // 点击对话框内部（消息区等）不收起；仅焦点真正移出对话框才收起
                                                gloo_timers::callback::Timeout::new(150, move || {
                                                    let still_inside = web_sys::window()
                                                        .and_then(|w| w.document())
                                                        .and_then(|d| d.active_element())
                                                        .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
                                                        .and_then(|el| el.closest("#workspace-chat-dialog").ok().flatten())
                                                        .is_some();
                                                    if !still_inside {
                                                        chat_focused.set(false);
                                                    }
                                                })
                                                .forget();
                                            },
                                            oninput: move |e| {
                                                let value = e.value();
                                                // 光标位置决定 @ 查询的边界；读不到就退回文本末尾（表现为不弹菜单）
                                                let caret = read_caret(WORKSPACE_CHAT_INPUT_ID)
                                                    .unwrap_or(value.len());
                                                mention.sync(&value, caret);
                                                chat_input.set(value);
                                            },
                                            onkeydown: move |e| {
                                                // @ 菜单开启时优先拦截按键：↑↓ 选择 / Enter 确认 / Esc 取消
                                                if mention.is_open() {
                                                    if e.key() == Key::ArrowDown {
                                                        e.prevent_default();
                                                        mention.move_selection(1);
                                                        return;
                                                    }
                                                    if e.key() == Key::ArrowUp {
                                                        e.prevent_default();
                                                        mention.move_selection(-1);
                                                        return;
                                                    }
                                                    if e.key() == Key::Enter && e.modifiers().is_empty() {
                                                        e.prevent_default();
                                                        if let Some((text, caret)) = mention.confirm(&chat_input()) {
                                                            chat_input.set(text);
                                                            restore_caret(WORKSPACE_CHAT_INPUT_ID, caret);
                                                        } else {
                                                            // 无候选时按 Enter 只收起菜单，不误当换行
                                                            mention.close();
                                                        }
                                                        return;
                                                    }
                                                    if e.key() == Key::Escape {
                                                        e.prevent_default();
                                                        mention.close();
                                                        return;
                                                    }
                                                }
                                                // Alt+回车 发送；普通回车换行（默认行为，不拦截）
                                                if e.key() == Key::Enter && e.modifiers().alt() {
                                                    e.prevent_default();
                                                    send_trigger.set(true);
                                                }
                                            }
                                        }
                                        button {
                                            class: "btn hud-btn btn-primary btn-sm",
                                            onclick: move |_| send_trigger.set(true),
                                            "发送"
                                        }
                                    }
                                }
                            }

                            // ---- 右下角：预留区（后续扩展，暂放数据刷新） ----
                            div { class: "pointer-events-auto w-64 flex-shrink-0 flex items-end justify-end pb-1",
                                button {
                                    class: "btn hud-btn btn-ghost btn-xs",
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
