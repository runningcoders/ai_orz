//! 聊天信息侧栏（ChatSidePanel）
//!
//! 沟通页面右侧可收起的信息面板，按对话模式动态组装 Tab：
//! - 项目对话：总览 / 任务 / 产物 / Agent（负责人）/ 工具
//! - 默认对话：Agent（前台）/ 我（当前用户）/ 工具
//!
//! 面板纯只读：数据加载复用现有项目/任务/产物/Agent/用户 API，
//! 创建与编辑操作仍在跳转各自详情页完成。

use std::collections::HashMap;
use std::time::Duration;

use dioxus::prelude::*;
use dioxus_router::Link;

use crate::api::hr::get_agent;
use crate::api::organization::get_current_user_info;
use crate::api::project::{get_project, get_task, list_project_tasks};
use crate::components::agent_summary::{agent_badge_row, agent_identity_row};
use crate::components::avatar_bubble::AvatarTone;
use crate::components::chat::ToolCallsTab;
use crate::components::hud::{HudPanel, HudProgress};
use crate::components::identity_chip::IdentityChip;
use crate::components::markdown::{MarkdownRenderer, MermaidDiagram};
use crate::components::state::Loading;
use crate::components::stats::AgentStatsPanelCompact;
use crate::store::toast::{ToastState, use_toast};
use crate::utils::time::now_ms;
use crate::utils::{
    avatar_initials, avatar_status_ring, format_file_size,
    format_timestamp_opt as format_timestamp, priority_badge, progress_tone, project_status_badge,
    project_status_text, tag_chip, task_status_badge, task_status_text,
};
use common::api::{
    ArtifactDetail, GetAgentRequest, GetAgentResponse, GetProjectRequest, GetProjectResponse,
    GetTaskRequest, GetTaskResponse, TaskListItem, UserInfoResponse,
};
use common::enums::{ArtifactSourceType, AssigneeType};
use common::models::{AgentStats, ModelCallStats};

/// SSE 消息触发的防抖刷新等待时长（毫秒）
const REFRESH_DEBOUNCE_MS: u64 = 2000;

/// 运行统计的时间窗口（分钟）
///
/// 与工作台顶栏 `RUNTIME_METRICS_WINDOW_MINUTES` 同口径：侧栏这一栏是**运行时**读数，
/// 回答的是「此刻这个 Agent 在不在干活」，因此只看最近 60 分钟。
/// 粒度随之取分钟桶（`stats_interval = minutely`）：60 分钟窗口若用天桶只会得到 1 个点，
/// 用小时桶也只有 1~2 个点，都画不出曲线来。
const RUNTIME_WINDOW_MINUTES: i64 = 60;

/// 产物来源类型中文文案
fn artifact_source_type_text(source_type: ArtifactSourceType) -> &'static str {
    match source_type {
        ArtifactSourceType::Attachment => "附件",
        ArtifactSourceType::GeneratedContent => "生成内容",
        ArtifactSourceType::RemoteUrl => "远程链接",
    }
}

/// 将产物按归属拆分为两组：项目级（task_id=None）/ 任务级（task_id=Some）
fn split_artifacts(artifacts: &[ArtifactDetail]) -> (Vec<&ArtifactDetail>, Vec<&ArtifactDetail>) {
    let mut project_level = Vec::new();
    let mut task_level = Vec::new();
    for a in artifacts {
        if a.task_id.is_some() {
            task_level.push(a);
        } else {
            project_level.push(a);
        }
    }
    (project_level, task_level)
}

/// 任务级产物按 task_id 分组（保持首次出现顺序）
fn group_by_task<'a>(arts: &[&'a ArtifactDetail]) -> Vec<(String, Vec<&'a ArtifactDetail>)> {
    let mut groups: Vec<(String, Vec<&ArtifactDetail>)> = Vec::new();
    for a in arts {
        let Some(tid) = a.task_id.as_deref() else {
            continue;
        };
        if let Some((_, list)) = groups.iter_mut().find(|(t, _)| t == tid) {
            list.push(a);
        } else {
            groups.push((tid.to_string(), vec![a]));
        }
    }
    groups
}

/// 聊天信息侧栏主组件
///
/// - `project_id`：选中项目 ID（None 表示默认对话模式）。
///   必须以 Signal 传入：use_effect 依赖其变化触发项目数据加载，
///   普通 prop 非响应式，切换项目后 effect 不会重跑（面板会永远转圈）
/// - `reception_agent_id`：前台 Agent ID（现仅工具调用 Tab 使用；Agent Tab 改用
///   `target_agent_id`，以便与主链路轮询同源）
/// - `target_agent_id`：当前会话目标 Agent ID，由 chat 页统一解析（项目会话 = 项目
///   owner，默认对话 = 前台 Agent）。两个模式的 Agent Tab 都由它定位
/// - `agent_info`：主链路轮询共享的 Agent 详情，Agent Tab 优先消费（id 匹配时零请求）
/// - `refresh_tick`：SSE 消息计数器，变化时防抖 2s 后自动刷新项目数据。
///   同样必须以 Signal 传入才能驱动 use_effect 重跑
/// - `stats_poll_tick`：统计周期刷新计数器（chat 页 3s 轮询每 30s 递增），
///   仅叠加进 Agent 统计 Tab 的刷新驱动，静默期统计不再停摆
/// - `on_close`：收起面板回调
#[component]
pub fn ChatSidePanel(
    project_id: Signal<Option<String>>,
    reception_agent_id: Option<String>,
    refresh_tick: Signal<u64>,
    stats_poll_tick: u64,
    on_close: Callback,
    /// 主链路轮询共享的目标 Agent 详情（chat 页置底状态气泡与轮询同源），
    /// AgentInfoTab 优先消费，无值时保留自身懒加载兜底。
    agent_info: Signal<Option<GetAgentResponse>>,
    /// 当前会话目标 Agent ID：chat 页的单一解析来源（项目会话 = 项目 owner，
    /// 默认对话 = 前台 Agent）。
    ///
    /// Agent Tab 用它定位，不再自行从项目详情 / reception 里各取一份 ——
    /// 「面板展示的 Agent」与「3s 轮询刷新的 Agent」必须是同一个，
    /// 否则会出现展示 A 的身份、刷新 B 的状态这种错位。
    target_agent_id: Signal<Option<String>>,
) -> Element {
    let toast = use_toast();
    let mut project = use_signal(|| None::<GetProjectResponse>);
    let mut tasks = use_signal(Vec::<TaskListItem>::new);
    let mut loading = use_signal(|| false);
    let mut active_tab = use_signal(|| 0usize);
    // 加载代际计数：防抖刷新与项目切换时丢弃过期请求结果
    let mut load_gen = use_signal(|| 0u64);
    let mut prev_project_id = use_signal(|| Option::<String>::None);
    let mut prev_tick = use_signal(|| 0u64);
    // 手动刷新计数：叠加到 refresh_tick 一并下发给工具调用 Tab
    let mut manual_tick = use_signal(|| 0u64);

    // 任务 Tab 展开状态与详情缓存（展开时懒加载，命中缓存不再请求）
    let mut expanded_task_id = use_signal(|| None::<String>);
    let mut task_cache = use_signal(HashMap::<String, GetTaskResponse>::new);
    let loading_task_id = use_signal(|| None::<String>);

    // 加载项目数据：debounce=true 时先等待防抖窗口（SSE 触发），期间被更新的代际直接丢弃
    let mut do_load = move |pid: String, debounce: bool| {
        let my_gen = load_gen() + 1;
        load_gen.set(my_gen);
        loading.set(true);
        spawn(async move {
            if debounce {
                gloo_timers::future::sleep(Duration::from_millis(REFRESH_DEBOUNCE_MS)).await;
                if load_gen() != my_gen {
                    return;
                }
            }
            let req = GetProjectRequest {
                id: pid.clone(),
                with_progress_summary: Some(true),
                with_artifacts: Some(true),
                with_task_graph: Some(true),
                ..Default::default()
            };
            let proj_res = get_project(req).await;
            let tasks_res = list_project_tasks(&pid).await;
            if load_gen() != my_gen {
                return;
            }
            match proj_res {
                Ok(p) => project.set(Some(p)),
                Err(e) => toast.error(format!("加载项目信息失败: {}", e)),
            }
            match tasks_res {
                Ok(r) => tasks.set(r.tasks),
                Err(e) => toast.error(format!("加载任务列表失败: {}", e)),
            }
            loading.set(false);
        });
    };

    // 手动刷新专用副本与模式判断（调用 Signal 读出当前值，渲染时同步订阅）
    let project_id_value = project_id();
    let is_project_mode = project_id_value.is_some();
    let pid_for_refresh = project_id_value.clone();
    let pid_for_tab = project_id_value.clone();

    // 项目切换 → 立即加载并重置面板状态；refresh_tick 变化 → 防抖刷新
    use_effect(move || {
        let pid = project_id();
        let tick = refresh_tick();
        let project_changed = prev_project_id() != pid;
        let tick_changed = prev_tick() != tick;
        // 修复 E2E-1：仅在值真正变化时写回。Signal::set 不做相等去重，
        // 无条件写回本 effect 自己订阅的信号会触发 effect 重跑 → 无限循环卡死主线程
        if project_changed {
            prev_project_id.set(pid.clone());
        }
        if tick_changed {
            prev_tick.set(tick);
        }
        if project_changed {
            active_tab.set(0);
            expanded_task_id.set(None);
            task_cache.set(HashMap::new());
            match pid {
                Some(id) => do_load(id, false),
                None => {
                    project.set(None);
                    tasks.set(Vec::new());
                }
            }
        } else if tick_changed && let Some(id) = pid {
            do_load(id, true);
        }
    });

    // 手动刷新：项目模式重拉项目数据，两种模式均同步刷新工具调用 Tab
    let manual_refresh = move |_| {
        manual_tick.set(manual_tick() + 1);
        if let Some(id) = pid_for_refresh.clone() {
            do_load(id, false);
        }
    };

    let tab_labels: Vec<&'static str> = if is_project_mode {
        vec!["总览", "任务", "产物", "Agent", "工具"]
    } else {
        vec!["Agent", "我", "工具"]
    };

    // 工具调用 Tab 的刷新驱动：SSE tick + 手动刷新计数
    let tool_tab_tick = refresh_tick() + manual_tick();
    // Agent 统计 Tab 的刷新驱动：SSE tick + 手动刷新 + 30s 周期 tick（对齐后端统计落盘节奏）
    let agent_stats_tick = tool_tab_tick + stats_poll_tick;

    let project_data = project().clone();
    let tasks_list = tasks.read().clone();
    let tab = active_tab();

    // Tab 内容分发（模式切换时 active_tab 已由 effect 重置）
    let content: Element = if is_project_mode {
        match tab {
            0 => match &project_data {
                Some(p) => overview_tab(p),
                None => loading_placeholder(),
            },
            1 => tasks_tab(
                &tasks_list,
                project_data.as_ref(),
                expanded_task_id,
                task_cache,
                loading_task_id,
                toast,
            ),
            2 => artifacts_tab(project_data.as_ref(), &tasks_list),
            // 负责人由 target_agent_id 提供（与主链路轮询同源）。
            // 项目详情未就绪时先走加载态，避免闪现「未指定负责人」再跳成 Agent 信息。
            3 => match (project_data.as_ref(), target_agent_id()) {
                (None, _) => loading_placeholder(),
                (Some(_), Some(agent_id)) => rsx! {
                    AgentInfoTab { agent_id, shared_info: agent_info, refresh_tick: agent_stats_tick }
                },
                (Some(_), None) => empty_hint("项目未指定负责人"),
            },
            4 => rsx! {
                ToolCallsTab {
                    project_id: pid_for_tab.clone(),
                    agent_id: None,
                    refresh_tick: tool_tab_tick,
                }
            },
            _ => rsx! {},
        }
    } else {
        match tab {
            0 => match target_agent_id() {
                Some(agent_id) => rsx! {
                    AgentInfoTab {
                        agent_id,
                        shared_info: agent_info,
                        // 与项目模式对齐：叠加 stats_poll_tick，静默期统计也按 30s 节奏刷新
                        refresh_tick: agent_stats_tick,
                    }
                },
                None => empty_hint("暂无前台 Agent"),
            },
            1 => rsx! { UserInfoTab {} },
            2 => rsx! {
                ToolCallsTab {
                    project_id: None,
                    agent_id: reception_agent_id.clone(),
                    refresh_tick: tool_tab_tick,
                }
            },
            _ => rsx! {},
        }
    };

    rsx! {
        div { class: "p-3 border-b border-base-300 flex items-center gap-2",
            h3 { class: "font-semibold text-sm flex-1 truncate", "信息面板" }
            if loading() {
                Loading { size: "xs" }
            }
            button {
                class: "btn hud-btn btn-ghost btn-xs",
                title: "刷新",
                onclick: manual_refresh,
                "⟳"
            }
            button {
                class: "btn hud-btn btn-ghost btn-xs",
                title: "收起面板",
                onclick: move |_| on_close.call(()),
                "✕"
            }
        }
        div { class: "flex flex-wrap gap-2 m-2",
            for (i, label) in tab_labels.iter().enumerate() {
                button {
                    key: "{label}",
                    class: if tab == i { "btn hud-btn btn-xs btn-primary" } else { "btn hud-btn btn-xs btn-ghost" },
                    onclick: move |_| active_tab.set(i),
                    "{label}"
                }
            }
        }
        div { class: "flex-1 overflow-y-auto p-3", {content} }
    }
}

fn loading_placeholder() -> Element {
    rsx! {
        div { class: "flex items-center justify-center py-12",
            Loading { size: "md" }
            span { class: "ml-2 text-sm text-base-content/60", "加载中..." }
        }
    }
}

fn empty_hint(msg: &str) -> Element {
    rsx! {
        div { class: "text-center py-12 text-base-content/60 text-sm", "{msg}" }
    }
}

/// Tab 总览：项目目标、进度汇总、执行计划/结果
fn overview_tab(p: &GetProjectResponse) -> Element {
    let desc = p.description.clone().filter(|s| !s.is_empty());
    let plan = p.execution_plan.clone().filter(|s| !s.is_empty());
    let result = p.execution_result.clone().filter(|s| !s.is_empty());
    rsx! {
        div { class: "space-y-4",
            // 基础信息：状态 / 优先级 / 标签 / 负责人
            div { class: "flex flex-wrap items-center gap-1",
                span { class: "{project_status_badge(p.status)}", "{project_status_text(p.status)}" }
                span { class: "{priority_badge(p.priority)}", "P{p.priority}" }
                for tag in p.tags.iter() {
                    span { key: "{tag}", class: "{tag_chip()}", "{tag}" }
                }
            }
            // 负责人字段只带 Agent ID，直接插 rsx 会把 ulid 打到界面上；
            // 走身份 chip 换成「头像 + 名字」，点头像展开与聊天页同源的 Agent 信息卡
            if let Some(owner) = p.owner_agent_id.as_deref() {
                div { class: "flex items-center gap-2 min-w-0",
                    span { class: "text-xs text-base-content/60", "负责人" }
                    IdentityChip { id: owner.to_string(), tone: AvatarTone::Agent }
                }
            }

            // 项目目标
            if let Some(d) = desc {
                div {
                    label { class: "form-label", "项目目标" }
                    MarkdownRenderer { content: d, compact: true }
                }
            }

            // 进度汇总
            if let Some(s) = &p.progress_summary {
                div {
                    label { class: "form-label", "整体进度" }
                    HudProgress { value: s.overall_percent as i32, tone: Some(progress_tone(s.overall_percent as i32).to_string()), show_value: Some(false) }
                    div { class: "text-xs text-base-content/60 mt-1",
                        "{s.overall_percent}% · 共 {s.total_tasks} 个任务（完成 {s.completed} / 进行中 {s.in_progress} / 待启动 {s.pending} / 已取消 {s.cancelled}）"
                    }
                }
            }

            // 执行计划 / 执行结果
            if let Some(plan) = plan {
                div {
                    label { class: "form-label", "执行计划" }
                    MarkdownRenderer { content: plan, compact: true }
                }
            }
            if let Some(result) = result {
                div {
                    label { class: "form-label", "执行结果" }
                    MarkdownRenderer { content: result, compact: true }
                }
            }
        }
    }
}

/// Tab 任务：任务依赖图（随项目详情顺带返回）+ 任务列表，点击单任务展开详情（懒加载 + 缓存）
fn tasks_tab(
    tasks: &[TaskListItem],
    project: Option<&GetProjectResponse>,
    mut expanded_task_id: Signal<Option<String>>,
    mut task_cache: Signal<HashMap<String, GetTaskResponse>>,
    mut loading_task_id: Signal<Option<String>>,
    toast: ToastState,
) -> Element {
    let task_graph = project
        .and_then(|p| p.task_graph.clone())
        .filter(|g| !g.is_empty());
    if tasks.is_empty() {
        return empty_hint("暂无任务");
    }
    rsx! {
        // 任务依赖图：位于任务列表上方（与任务管理页同款渲染）
        if let Some(graph) = task_graph {
            HudPanel {
                title: "任务依赖图".to_string(),
                eyebrow: "DEPENDENCIES".to_string(),
                MermaidDiagram { code: graph }
            }
        }
        div { class: "space-y-2",
            for t in tasks.iter() {
                {
                    let tid = t.id.clone();
                    let title = t.title.clone();
                    let status = t.status;
                    let progress = t.progress;
                    let assignee = t.assignee_id.clone();
                    // 分配对象类型（0=用户 1=Agent）→ 头像基调 / 名称目录 / 卡片形态
                    let assignee_tone = AvatarTone::from(AssigneeType::from_i32(t.assignee_type));
                    let is_expanded = expanded_task_id() == Some(tid.clone());
                    let is_loading = loading_task_id() == Some(tid.clone());
                    rsx! {
                        div {
                            key: "{tid}",
                            class: "rounded-lg border border-base-300 bg-base-100",
                            // 列表行：点击切换展开
                            div {
                                class: "p-2 cursor-pointer hover:bg-base-200 rounded-t-lg",
                                onclick: move |_| {
                                    if expanded_task_id() == Some(tid.clone()) {
                                        expanded_task_id.set(None);
                                        return;
                                    }
                                    expanded_task_id.set(Some(tid.clone()));
                                    // 首次展开懒加载详情（缓存命中则跳过）
                                    if !task_cache.read().contains_key(&tid)
                                        && loading_task_id() != Some(tid.clone())
                                    {
                                        loading_task_id.set(Some(tid.clone()));
                                        let tid2 = tid.clone();
                                        spawn(async move {
                                            let req = GetTaskRequest {
                                                id: tid2.clone(),
                                                with_artifacts: Some(true),
                                                ..Default::default()
                                            };
                                            match get_task(req).await {
                                                Ok(t) => {
                                                    task_cache.write().insert(tid2, t);
                                                }
                                                Err(e) => {
                                                    toast.error(format!("加载任务详情失败: {}", e));
                                                }
                                            }
                                            loading_task_id.set(None);
                                        });
                                    }
                                },
                                div { class: "flex items-center gap-2",
                                    span { class: "{task_status_badge(status)}", "{task_status_text(status)}" }
                                    span { class: "font-medium text-sm flex-1 truncate", "{title}" }
                                    span { class: "text-xs text-base-content/60", "{progress}%" }
                                    if is_expanded { "▲" } else { "▼" }
                                }
                                HudProgress { value: progress, tone: Some(progress_tone(progress).to_string()), show_value: Some(false), extra_class: Some("mt-1".to_string()) }
                                div { class: "flex items-center gap-2 min-w-0 mt-1",
                                    span { class: "text-xs text-base-content/60", "负责人" }
                                    IdentityChip { id: assignee, tone: assignee_tone }
                                }
                            }
                            // 展开详情
                            if is_expanded {
                                div { class: "p-2 border-t border-base-300",
                                    if is_loading {
                                        div { class: "flex items-center justify-center py-4",
                                            Loading { size: "sm" }
                                        }
                                    } else if let Some(detail) = task_cache.read().get(&tid) {
                                        {
                                            let detail = detail.clone();
                                            let tid_link = tid.clone();
                                            rsx! {
                                                {task_expanded_content(detail, tid_link)}
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

/// 任务展开内容：描述、时间、执行计划/结果、产物列表
fn task_expanded_content(t: GetTaskResponse, tid: String) -> Element {
    let desc = t.description.clone().filter(|s| !s.is_empty());
    let plan = t.execution_plan.clone().filter(|s| !s.is_empty());
    let result = t.execution_result.clone().filter(|s| !s.is_empty());
    let artifacts = t.artifacts.clone().unwrap_or_default();
    rsx! {
        div { class: "space-y-3 text-sm",
            div { class: "text-xs text-base-content/60",
                "开始：{format_timestamp(t.start_at)} · 截止：{format_timestamp(t.due_at)}"
            }
            if let Some(d) = desc {
                div {
                    label { class: "form-label", "描述" }
                    MarkdownRenderer { content: d, compact: true }
                }
            }
            if let Some(plan) = plan {
                div {
                    label { class: "form-label", "执行计划" }
                    MarkdownRenderer { content: plan, compact: true }
                }
            }
            if let Some(result) = result {
                div {
                    label { class: "form-label", "执行结果" }
                    MarkdownRenderer { content: result, compact: true }
                }
            }
            if !artifacts.is_empty() {
                div {
                    label { class: "form-label", "产物（{artifacts.len()}）" }
                    div { class: "flex flex-wrap gap-1",
                        for a in artifacts.iter() {
                            {
                                let aid = a.id.clone();
                                let aname = a.name.clone();
                                rsx! {
                                    Link {
                                        key: "{aid}",
                                        class: "{tag_chip()}",
                                        to: crate::pages::Route::ProjectArtifactDetail { id: aid },
                                        "{aname}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Link {
                class: "btn hud-btn btn-ghost btn-xs",
                to: crate::pages::Route::TaskDetail { id: tid },
                "在详情页打开 →"
            }
        }
    }
}

/// Tab 产物：项目级产物 + 任务级产物（按任务分组）
fn artifacts_tab(project: Option<&GetProjectResponse>, tasks: &[TaskListItem]) -> Element {
    let Some(p) = project else {
        return loading_placeholder();
    };
    let artifacts = p.artifacts.clone().unwrap_or_default();
    if artifacts.is_empty() {
        return empty_hint("暂无产物");
    }
    let (project_level, task_level) = split_artifacts(&artifacts);
    let groups = group_by_task(&task_level);
    rsx! {
        div { class: "space-y-4",
            // 项目级产物
            div {
                label { class: "form-label", "项目级产物（{project_level.len()}）" }
                if project_level.is_empty() {
                    div { class: "text-xs text-base-content/60", "暂无" }
                } else {
                    div { class: "space-y-1",
                        for a in project_level.iter() {
                            {
                                let a = (*a).clone();
                                rsx! { ArtifactRow { artifact: a } }
                            }
                        }
                    }
                }
            }
            // 任务级产物（按任务分组）
            div {
                label { class: "form-label", "任务级产物（{task_level.len()}）" }
                if groups.is_empty() {
                    div { class: "text-xs text-base-content/60", "暂无" }
                } else {
                    for (tid, arts) in groups.iter() {
                        {
                            let task_title = tasks
                                .iter()
                                .find(|t| &t.id == tid)
                                .map(|t| t.title.clone())
                                .unwrap_or_else(|| {
                                    let truncated: String = tid.chars().take(8).collect();
                                    format!("任务 {}…", truncated)
                                });
                            let arts = arts.iter().map(|a| (*a).clone()).collect::<Vec<_>>();
                            let tid_key = tid.clone();
                            rsx! {
                                div { key: "{tid_key}", class: "mb-2",
                                    div { class: "text-xs font-medium text-base-content/70 mb-1", "📌 {task_title}" }
                                    div { class: "space-y-1",
                                        for a in arts.iter() {
                                            {
                                                let a = a.clone();
                                                rsx! { ArtifactRow { artifact: a } }
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

/// 单行产物卡片（只读，点击跳转产物详情页）
#[component]
fn ArtifactRow(artifact: ArtifactDetail) -> Element {
    let aid = artifact.id.clone();
    let name = artifact.name.clone();
    let source_text = artifact_source_type_text(artifact.source_type);
    let size = format_file_size(artifact.file_size);
    let created = format_timestamp(Some(artifact.created_at));
    rsx! {
        Link {
            class: "flex items-center gap-2 p-2 rounded-lg border border-base-300 bg-base-100 hover:bg-base-200",
            to: crate::pages::Route::ProjectArtifactDetail { id: aid },
            div { class: "flex-1 min-w-0",
                div { class: "text-sm font-medium truncate", "{name}" }
                div { class: "text-xs text-base-content/60", "{size} · {created}" }
            }
            span { class: "badge orz-tag badge-sm", "{source_text}" }
        }
    }
}

/// Tab Agent（两种模式共用）：展示 Agent 详情。
///
/// 数据源优先级：chat 主链路轮询共享的 `shared_info`（id 匹配才消费，
/// 随轮询实时刷新 runtime 徽章）> 组件自身懒加载兜底（面板独立使用 /
/// 共享数据未就绪时）。
///
/// 运行统计（唤醒/Token/趋势）独立拉取：共享轮询请求不带 stats 参数
/// （主链路高频轮询零额外开销），统计仅在 Tab 挂载时按需加载，
/// 并随 `refresh_tick`（SSE/手动刷新）防抖刷新，机制与 ToolCallsTab 一致。
#[component]
fn AgentInfoTab(
    agent_id: String,
    shared_info: Signal<Option<GetAgentResponse>>,
    refresh_tick: u64,
) -> Element {
    let mut agent = use_signal(|| None::<GetAgentResponse>);
    let mut failed = use_signal(|| false);
    // effect 闭包需要 'static 捕获，单独克隆一份，渲染段仍可直接用 agent_id
    let agent_id_for_effect = agent_id.clone();
    use_effect(move || {
        // 共享数据已就绪且 id 匹配：跳过自身请求
        if shared_info()
            .as_ref()
            .is_some_and(|a| a.id == agent_id_for_effect)
        {
            return;
        }
        let id = agent_id_for_effect.clone();
        spawn(async move {
            let req = GetAgentRequest {
                id,
                ..Default::default()
            };
            match get_agent(req).await {
                Ok(a) => agent.set(Some(a)),
                Err(_) => failed.set(true),
            }
        });
    });

    // ---- 运行统计：独立拉取（带 stats fetch-options），防抖刷新与代际丢弃 ----
    // 四元组同源于一次响应：避免切换 Agent 时统计数据与上下文长度错代混用
    let mut stats_pair = use_signal(|| {
        None::<(
            Option<AgentStats>,
            Option<ModelCallStats>,
            Option<u64>,
            Option<u64>,
        )>
    });
    let mut stats_loaded = use_signal(|| false);
    let mut stats_gen = use_signal(|| 0u64);
    let mut prev_stats_tick = use_signal(|| 0u64);
    let agent_id_for_stats = agent_id.clone();

    let mut load_stats = move |debounce: bool| {
        let my_gen = stats_gen() + 1;
        stats_gen.set(my_gen);
        let id = agent_id_for_stats.clone();
        spawn(async move {
            if debounce {
                gloo_timers::future::sleep(Duration::from_millis(REFRESH_DEBOUNCE_MS)).await;
                if stats_gen() != my_gen {
                    return;
                }
            }
            // 运行统计窗口 = 最近 60 分钟（与工作台顶栏同口径），粒度取分钟桶。
            // ⚠️ `stats_interval` 是**白名单字符串**：后端只认 `minutely` / `hourly` / `daily`，
            // 其余取值会被静默忽略并退回 daily（`StatsFetchOptions::interval` 的兜底），
            // 于是又画出一排日期标签 —— 改这个值时要同步核对 `handlers/hr/agent/get_agent.rs`。
            let end_ms = now_ms();
            let req = GetAgentRequest {
                id,
                with_stats: Some(true),
                with_model_call_stats: Some(true),
                stats_time_start: Some(end_ms - RUNTIME_WINDOW_MINUTES * 60_000),
                stats_time_end: Some(end_ms),
                stats_interval: Some("minutely".to_string()),
                ..Default::default()
            };
            let pair = match get_agent(req).await {
                Ok(a) => Some((
                    a.stats.clone(),
                    a.model_call_stats.clone(),
                    a.context_length,
                    a.context_length_threshold,
                )),
                Err(_) => None,
            };
            if stats_gen() != my_gen {
                return;
            }
            if let Some(p) = pair {
                stats_pair.set(Some(p));
            }
            stats_loaded.set(true);
        });
    };

    // 挂载/Agent 切换 → 立即加载；refresh_tick 变化 → 防抖刷新。
    // 守卫结构与 ToolCallsTab 一致：仅在值真正变化时写回 + 调 load，
    // 避免 load 内写 gen 信号触发 effect 重跑 → 无条件 load 的请求循环（同 E2E-1）
    let mut prev_stats_agent = use_signal(String::new);
    // effect 闭包 'static 捕获会 move agent_id，渲染段还要用，单独克隆一份
    let agent_id_for_key = agent_id.clone();
    use_effect(move || {
        let tick = refresh_tick;
        let aid = agent_id_for_key.clone();
        let agent_changed = prev_stats_agent() != aid;
        let tick_changed = prev_stats_tick() != tick;
        if agent_changed {
            prev_stats_agent.set(aid);
        }
        if tick_changed {
            prev_stats_tick.set(tick);
        }
        if agent_changed {
            // 切换 Agent 时丢弃旧统计，避免新请求返回前闪现上一个 Agent 的数据
            stats_pair.set(None);
            stats_loaded.set(false);
            load_stats(false);
        } else if tick_changed {
            load_stats(true);
        }
    });

    if failed() {
        return empty_hint("Agent 信息加载失败");
    }
    // 共享数据优先（id 匹配），否则用自身懒加载结果
    let shared = shared_info().as_ref().filter(|a| a.id == agent_id).cloned();
    let Some(a) = shared.or_else(|| agent().clone()) else {
        return loading_placeholder();
    };
    let desc = a.description.clone().filter(|s| !s.is_empty());
    let capabilities = a.capabilities.clone().unwrap_or_default();
    let aid = a.id.clone();
    let (agent_stats, model_call_stats, context_length, context_length_threshold) =
        stats_pair().unwrap_or((None, None, None, None));
    rsx! {
        div { class: "space-y-4",
            // 身份行 / 徽章行与头像信息气泡（AvatarBubble）共用同一实现，
            // 避免两处展示逐字复制后静默漂移（见 components/agent_summary.rs）
            { agent_identity_row(&a, avatar_status_ring(a.status)) }
            { agent_badge_row(&a) }
            // 运行统计：首次加载完成后渲染（无数据时面板内给出提示）
            if stats_loaded() {
                AgentStatsPanelCompact {
                    stats: agent_stats,
                    model_call_stats,
                    context_length,
                    context_length_threshold,
                    // 阈值缺失时面板需要给出「去配置」入口，指向本条消息所属 Agent 的供应商
                    model_provider_id: a.model_provider_id.clone(),
                    // 读数口径与上方请求窗口同源，避免被读成历史累计
                    window_label: Some(format!("最近 {RUNTIME_WINDOW_MINUTES} 分钟")),
                }
            }
            if let Some(d) = desc {
                div {
                    label { class: "form-label", "简介" }
                    MarkdownRenderer { content: d, compact: true }
                }
            }
            if !capabilities.is_empty() {
                div {
                    label { class: "form-label", "能力" }
                    div { class: "flex flex-wrap gap-1",
                        for c in capabilities.iter() {
                            span { key: "{c}", class: "{tag_chip()}", "{c}" }
                        }
                    }
                }
            }
            Link {
                class: "btn hud-btn btn-ghost btn-xs",
                to: crate::pages::Route::HrAgentDetail { id: aid },
                "在详情页打开 →"
            }
        }
    }
}

/// Tab 我：当前用户信息（只读）+ 跳转设置页
#[component]
fn UserInfoTab() -> Element {
    let mut user = use_signal(|| None::<UserInfoResponse>);
    use_effect(move || {
        spawn(async move {
            if let Ok(resp) = get_current_user_info().await {
                user.set(Some(resp.data));
            }
        });
    });
    let Some(u) = user().clone() else {
        return loading_placeholder();
    };
    let display = u
        .display_name
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| u.username.clone());
    let email = u.email.clone().filter(|s| !s.is_empty());
    let enabled = u.status == 1;
    rsx! {
        div { class: "space-y-4",
            div { class: "flex items-center gap-2",
                div { class: "w-10 h-10 rounded-full bg-primary text-primary-content flex items-center justify-center font-bold",
                    "{avatar_initials(&display)}"
                }
                div { class: "flex-1 min-w-0",
                    div { class: "font-semibold truncate", "{display}" }
                    div { class: "text-xs text-base-content/60", "@{u.username}" }
                }
            }
            div { class: "flex flex-wrap gap-1 items-center",
                span { class: "badge orz-tag badge-sm", "{u.role_name}" }
                span { class: if enabled { "badge hud-badge badge-sm badge-success" } else { "badge hud-badge badge-sm badge-error" },
                    if enabled { "已启用" } else { "已禁用" }
                }
            }
            if let Some(email) = email {
                div { class: "text-sm", "📧 {email}" }
            }
            div { class: "text-xs text-base-content/60",
                "主题等偏好设置请前往设置页调整"
            }
            Link {
                class: "btn hud-btn btn-ghost btn-xs",
                to: crate::pages::Route::Settings {},
                "打开设置 →"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::enums::FileType;

    fn test_artifact(id: &str, task_id: Option<&str>) -> ArtifactDetail {
        ArtifactDetail {
            id: id.to_string(),
            project_id: "p1".to_string(),
            task_id: task_id.map(|s| s.to_string()),
            name: id.to_string(),
            description: String::new(),
            file_type: FileType::Document,
            source_type: ArtifactSourceType::GeneratedContent,
            file_path: String::new(),
            mime_type: String::new(),
            file_size: 0,
            tags: Vec::new(),
            status: 1,
            created_by: String::new(),
            modified_by: String::new(),
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn split_artifacts_empty() {
        let (project_level, task_level) = split_artifacts(&[]);
        assert!(project_level.is_empty());
        assert!(task_level.is_empty());
    }

    #[test]
    fn split_artifacts_mixed() {
        let arts = vec![
            test_artifact("a1", None),
            test_artifact("a2", Some("t1")),
            test_artifact("a3", None),
            test_artifact("a4", Some("t2")),
        ];
        let (project_level, task_level) = split_artifacts(&arts);
        assert_eq!(project_level.len(), 2);
        assert_eq!(task_level.len(), 2);
        assert!(project_level.iter().all(|a| a.task_id.is_none()));
        assert!(task_level.iter().all(|a| a.task_id.is_some()));
    }

    #[test]
    fn group_by_task_keeps_first_seen_order() {
        let arts = [
            test_artifact("a1", Some("t2")),
            test_artifact("a2", Some("t1")),
            test_artifact("a3", Some("t2")),
        ];
        let groups = group_by_task(&arts.iter().collect::<Vec<_>>());
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].0, "t2");
        assert_eq!(groups[0].1.len(), 2);
        assert_eq!(groups[1].0, "t1");
        assert_eq!(groups[1].1.len(), 1);
    }
}
