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
use crate::components::chat::ToolCallsTab;
use crate::components::hud::HudProgress;
use crate::components::markdown::{MarkdownRenderer, MermaidDiagram};
use crate::components::state::Loading;
use crate::store::toast::{ToastState, use_toast};
use crate::utils::{
    agent_lifecycle_badge, agent_lifecycle_text, agent_runtime_badge, format_file_size,
    format_timestamp_opt as format_timestamp, priority_badge, progress_tone, project_status_badge,
    project_status_text, tag_chip, task_status_badge, task_status_text,
};
use common::api::{
    ArtifactDetail, GetAgentRequest, GetAgentResponse, GetProjectRequest, GetProjectResponse,
    GetTaskRequest, GetTaskResponse, TaskListItem, UserInfoResponse,
};
use common::enums::ArtifactSourceType;

/// SSE 消息触发的防抖刷新等待时长（毫秒）
const REFRESH_DEBOUNCE_MS: u64 = 2000;

/// 产物来源类型中文文案
fn artifact_source_type_text(source_type: ArtifactSourceType) -> &'static str {
    match source_type {
        ArtifactSourceType::Attachment => "附件",
        ArtifactSourceType::GeneratedContent => "生成内容",
        ArtifactSourceType::RemoteUrl => "远程链接",
    }
}

/// Agent 运行时状态中文文案
fn agent_runtime_text(state: i32) -> &'static str {
    match state {
        0 => "空闲",
        1 => "休息中",
        2 => "忙碌",
        _ => "未知",
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
/// - `project_id`：选中项目 ID（None 表示默认对话模式）
/// - `reception_agent_id`：前台 Agent ID（默认对话模式的 Agent Tab 数据源）
/// - `refresh_tick`：SSE 消息计数器，变化时防抖 2s 后自动刷新项目数据
/// - `on_close`：收起面板回调
#[component]
pub fn ChatSidePanel(
    project_id: Option<String>,
    reception_agent_id: Option<String>,
    refresh_tick: u64,
    on_close: Callback,
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
                with_task_graph: Some(true),
                with_artifacts: Some(true),
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

    // 手动刷新专用副本与模式判断（project_id 会被 use_effect 闭包移走）
    let is_project_mode = project_id.is_some();
    let pid_for_refresh = project_id.clone();
    let pid_for_tab = project_id.clone();

    // 项目切换 → 立即加载并重置面板状态；refresh_tick 变化 → 防抖刷新
    use_effect(move || {
        let pid = project_id.clone();
        let tick = refresh_tick;
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
    let tool_tab_tick = refresh_tick + manual_tick();

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
                expanded_task_id,
                task_cache,
                loading_task_id,
                toast,
            ),
            2 => artifacts_tab(project_data.as_ref(), &tasks_list),
            3 => match project_data.as_ref().and_then(|p| p.owner_agent_id.clone()) {
                Some(agent_id) => rsx! { AgentInfoTab { agent_id } },
                None => empty_hint("项目未指定负责人"),
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
            0 => match &reception_agent_id {
                Some(agent_id) => rsx! { AgentInfoTab { agent_id: agent_id.clone() } },
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

/// Tab 总览：项目目标、进度汇总、执行计划/结果、任务依赖图
fn overview_tab(p: &GetProjectResponse) -> Element {
    let desc = p.description.clone().filter(|s| !s.is_empty());
    let plan = p.execution_plan.clone().filter(|s| !s.is_empty());
    let result = p.execution_result.clone().filter(|s| !s.is_empty());
    let graph = p.task_graph.clone().filter(|s| !s.is_empty());
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
            if let Some(owner) = p.owner_agent_id.as_deref() {
                div { class: "text-xs text-base-content/60", "负责人：{owner}" }
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

            // 任务依赖图
            if let Some(graph) = graph {
                div {
                    label { class: "form-label", "任务依赖图" }
                    MermaidDiagram { code: graph }
                }
            }
        }
    }
}

/// Tab 任务：任务列表，点击单任务展开详情（懒加载 + 缓存）
fn tasks_tab(
    tasks: &[TaskListItem],
    mut expanded_task_id: Signal<Option<String>>,
    mut task_cache: Signal<HashMap<String, GetTaskResponse>>,
    mut loading_task_id: Signal<Option<String>>,
    toast: ToastState,
) -> Element {
    if tasks.is_empty() {
        return empty_hint("暂无任务");
    }
    rsx! {
        div { class: "space-y-2",
            for t in tasks.iter() {
                {
                    let tid = t.id.clone();
                    let title = t.title.clone();
                    let status = t.status;
                    let progress = t.progress;
                    let assignee = t.assignee_id.clone();
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
                                div { class: "text-xs text-base-content/60 mt-1", "负责人：{assignee}" }
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

/// Tab Agent（两种模式共用）：懒加载 Agent 详情并展示
#[component]
fn AgentInfoTab(agent_id: String) -> Element {
    let mut agent = use_signal(|| None::<GetAgentResponse>);
    let mut failed = use_signal(|| false);
    use_effect(move || {
        let id = agent_id.clone();
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

    if failed() {
        return empty_hint("Agent 信息加载失败");
    }
    let Some(a) = agent().clone() else {
        return loading_placeholder();
    };
    let desc = a.description.clone().filter(|s| !s.is_empty());
    let capabilities = a.capabilities.clone().unwrap_or_default();
    let aid = a.id.clone();
    let kind = a.kind.clone();
    rsx! {
        div { class: "space-y-4",
            div { class: "flex items-center gap-2",
                div { class: "w-10 h-10 rounded-full bg-secondary text-secondary-content flex items-center justify-center font-bold",
                    "{a.name.chars().next().unwrap_or('A')}"
                }
                div { class: "flex-1 min-w-0",
                    div { class: "font-semibold truncate", "{a.name}" }
                    div { class: "text-xs text-base-content/60", "类型：{kind}" }
                }
            }
            div { class: "flex flex-wrap gap-1",
                span { class: "{agent_lifecycle_badge(a.status)}", "{agent_lifecycle_text(a.status)}" }
                span { class: "{agent_runtime_badge(a.runtime_state)}",
                    "{agent_runtime_text(a.runtime_state)}"
                }
                for role in a.roles.iter() {
                    span { key: "{role}", class: "{tag_chip()}", "{role}" }
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
            div { class: "text-xs text-base-content/60", "已绑定工具：{a.tool_list.as_ref().map(|l| l.len()).unwrap_or(0)} 个" }
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
                    "{display.chars().next().unwrap_or('U')}"
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
