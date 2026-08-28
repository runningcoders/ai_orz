//! 任务详情页 - 基本信息、状态流转、进度更新、操作按钮

use dioxus::prelude::*;
use dioxus_router::{Link, use_navigator};

use crate::api::hr::query_agents;
use crate::api::project::*;
use crate::components::markdown::MarkdownRenderer;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::components::stats::TaskStatsPanel;
use crate::components::workspace_graph::{WorkspaceGraph, WorkspaceView};
use crate::layouts::app_layout::AppLayout;
use crate::pages::project::task_edit_modal::{TaskEditModal, TaskEditMode};
use crate::store::toast::use_toast;
use crate::utils::{
    format_timestamp_opt as format_timestamp, progress_bar_class, status::tag_chip,
    task_status_badge as status_badge, task_status_text as status_text,
};
use common::api::{
    AgentListItem, AgentQueryRequest, GetTaskRequest, GetTaskResponse, PaginationParams,
    ProjectListItem, ProjectQueryRequest, TaskListItem, UpdateTaskProgressRequest,
    UpdateTaskStatusRequest,
};
use common::enums::TaskStatus;

#[component]
pub fn TaskDetail(id: String) -> Element {
    // 方案 B：响应式 rid + use_resource，拉取仅在 :id 变化时触发
    let route = dioxus_router::use_route::<crate::pages::Route>();
    let mut rid = use_signal(|| String::new());
    if let crate::pages::Route::TaskDetail { id: route_id } = &route {
        if *rid.peek() != *route_id {
            rid.set(route_id.clone());
        }
    }
    let mut task_res = use_resource(move || {
        let id = rid();
        async move {
            let req = GetTaskRequest {
                id,
                with_stats: Some(true),
                with_model_call_stats: Some(true),
                stats_interval: Some("daily".to_string()),
                with_artifacts: Some(true),
                ..Default::default()
            };
            get_task(req).await
        }
    });

    // 进度更新弹窗
    let mut show_progress_modal = use_signal(|| false);
    let mut new_progress = use_signal(|| 0i32);
    let mut updating_progress = use_signal(|| false);
    let mut show_edit_modal = use_signal(|| false);
    let id_for_edit = id.clone();

    // Tab 切换：0=概览 1=进度与状态 2=关系图
    let mut active_tab = use_signal(|| 0usize);
    // 关系图所需数据：同 project 的 tasks + 全局 agents + 全局 projects
    let mut graph_tasks = use_signal(Vec::<TaskListItem>::new);
    let mut graph_agents = use_signal(Vec::<AgentListItem>::new);
    let mut graph_projects = use_signal(Vec::<ProjectListItem>::new);

    let toast = use_toast();
    let navigator = use_navigator();

    // 关系图数据：依赖 task_res 解析出 project_id / assignee，随 id 变化重新加载
    use_effect(move || {
        if let Some(Ok(t)) = task_res.read().as_ref() {
            let pid_for_graph = t.project_id.clone();
            let assignee_type_for_graph = t.assignee_type;
            let assignee_id_for_graph = t.assignee_id.clone();
            spawn(async move {
                if let Some(pid) = &pid_for_graph {
                    match list_project_tasks(pid).await {
                        Ok(resp) => graph_tasks.set(resp.tasks),
                        Err(e) => toast.error(format!("获取项目任务失败: {}", e)),
                    }
                }
                if assignee_type_for_graph == 1 {
                    let ids = vec![assignee_id_for_graph.clone()];
                    let req = AgentQueryRequest {
                        ids: Some(ids),
                        pagination: PaginationParams::default(),
                        ..Default::default()
                    };
                    match query_agents(&req).await {
                        Ok(page) => {
                            if let Some(a) = page.items.into_iter().next() {
                                graph_agents.set(vec![a]);
                            }
                        }
                        Err(e) => toast.error(format!("获取 Agent 失败: {}", e)),
                    }
                }
                if let Some(pid) = &pid_for_graph {
                    let ids = vec![pid.clone()];
                    let req = ProjectQueryRequest {
                        ids: Some(ids),
                        pagination: PaginationParams::default(),
                        ..Default::default()
                    };
                    match query_projects(&req).await {
                        Ok(page) => {
                            if let Some(p) = page.items.into_iter().next() {
                                graph_projects.set(vec![p]);
                            }
                        }
                        Err(e) => toast.error(format!("获取 Project 失败: {}", e)),
                    }
                }
            });
        }
    });

    // 进度初始值随 task 解析派生
    use_effect(move || {
        if let Some(Ok(t)) = task_res.read().as_ref() {
            new_progress.set(t.progress);
        }
    });

    // 状态切换 - 内联每个按钮的 closure 避免 move 问题
    // 状态 1: 送审
    let id_for_review = id.clone();
    let on_review = move |_| {
        let id_clone = id_for_review.clone();
        spawn(async move {
            let req = UpdateTaskStatusRequest {
                id: id_clone.clone(),
                status: TaskStatus::PendingReview,
            };
            match update_task_status(req).await {
                Ok(_) => {
                    toast.success("任务状态已更新");
                    let req = GetTaskRequest {
                        id: id_clone.clone(),
                        with_stats: Some(true),
                        with_model_call_stats: Some(true),
                        stats_interval: Some("daily".to_string()),
                        ..Default::default()
                    };
                    if let Ok(t) = get_task(req).await {
                        new_progress.set(t.progress);
                        task_res.set(Some(Ok(t)));
                    }
                }
                Err(e) => toast.error(&e),
            }
        });
    };
    // 状态 2: 待处理
    let id_for_pending = id.clone();
    let on_pending = move |_| {
        let id_clone = id_for_pending.clone();
        spawn(async move {
            let req = UpdateTaskStatusRequest {
                id: id_clone.clone(),
                status: TaskStatus::Pending,
            };
            match update_task_status(req).await {
                Ok(_) => {
                    toast.success("任务状态已更新");
                    let req = GetTaskRequest {
                        id: id_clone.clone(),
                        with_stats: Some(true),
                        with_model_call_stats: Some(true),
                        stats_interval: Some("daily".to_string()),
                        ..Default::default()
                    };
                    if let Ok(t) = get_task(req).await {
                        new_progress.set(t.progress);
                        task_res.set(Some(Ok(t)));
                    }
                }
                Err(e) => toast.error(&e),
            }
        });
    };
    // 状态 3: 开始
    let id_for_start = id.clone();
    let on_start = move |_| {
        let id_clone = id_for_start.clone();
        spawn(async move {
            let req = UpdateTaskStatusRequest {
                id: id_clone.clone(),
                status: TaskStatus::InProgress,
            };
            match update_task_status(req).await {
                Ok(_) => {
                    toast.success("任务状态已更新");
                    let req = GetTaskRequest {
                        id: id_clone.clone(),
                        with_stats: Some(true),
                        with_model_call_stats: Some(true),
                        stats_interval: Some("daily".to_string()),
                        ..Default::default()
                    };
                    if let Ok(t) = get_task(req).await {
                        new_progress.set(t.progress);
                        task_res.set(Some(Ok(t)));
                    }
                }
                Err(e) => toast.error(&e),
            }
        });
    };
    // 状态 4: 完成
    let id_for_complete = id.clone();
    let on_complete = move |_| {
        let id_clone = id_for_complete.clone();
        spawn(async move {
            let req = UpdateTaskStatusRequest {
                id: id_clone.clone(),
                status: TaskStatus::Completed,
            };
            match update_task_status(req).await {
                Ok(_) => {
                    toast.success("任务状态已更新");
                    let req = GetTaskRequest {
                        id: id_clone.clone(),
                        with_stats: Some(true),
                        with_model_call_stats: Some(true),
                        stats_interval: Some("daily".to_string()),
                        ..Default::default()
                    };
                    if let Ok(t) = get_task(req).await {
                        new_progress.set(t.progress);
                        task_res.set(Some(Ok(t)));
                    }
                }
                Err(e) => toast.error(&e),
            }
        });
    };
    // 状态 0: 取消
    let id_for_cancel = id.clone();
    let on_cancel = move |_| {
        let id_clone = id_for_cancel.clone();
        spawn(async move {
            let req = UpdateTaskStatusRequest {
                id: id_clone.clone(),
                status: TaskStatus::Cancelled,
            };
            match update_task_status(req).await {
                Ok(_) => {
                    toast.success("任务状态已更新");
                    let req = GetTaskRequest {
                        id: id_clone.clone(),
                        with_stats: Some(true),
                        with_model_call_stats: Some(true),
                        stats_interval: Some("daily".to_string()),
                        ..Default::default()
                    };
                    if let Ok(t) = get_task(req).await {
                        new_progress.set(t.progress);
                        task_res.set(Some(Ok(t)));
                    }
                }
                Err(e) => toast.error(&e),
            }
        });
    };
    // 状态 5: 归档
    let id_for_archive = id.clone();
    let on_archive = move |_| {
        let id_clone = id_for_archive.clone();
        spawn(async move {
            let req = UpdateTaskStatusRequest {
                id: id_clone.clone(),
                status: TaskStatus::Archived,
            };
            match update_task_status(req).await {
                Ok(_) => {
                    toast.success("任务状态已更新");
                    let req = GetTaskRequest {
                        id: id_clone.clone(),
                        with_stats: Some(true),
                        with_model_call_stats: Some(true),
                        stats_interval: Some("daily".to_string()),
                        ..Default::default()
                    };
                    if let Ok(t) = get_task(req).await {
                        new_progress.set(t.progress);
                        task_res.set(Some(Ok(t)));
                    }
                }
                Err(e) => toast.error(&e),
            }
        });
    };

    // 打开进度弹窗
    let open_progress_modal = move |_| {
        if let Some(t) = task_res.read().as_ref().and_then(|r| r.as_ref().ok()) {
            new_progress.set(t.progress);
        }
        show_progress_modal.set(true);
    };

    // 提交进度更新
    let submit_progress = move |_| {
        let id_clone = id.clone();
        let progress_val = new_progress();
        updating_progress.set(true);
        spawn(async move {
            match update_task_progress(UpdateTaskProgressRequest {
                id: id_clone.clone(),
                progress: progress_val,
            })
            .await
            {
                Ok(t) => {
                    toast.success("进度已更新");
                    task_res.set(Some(Ok(t)));
                    show_progress_modal.set(false);
                }
                Err(e) => toast.error(&e),
            }
            updating_progress.set(false);
        });
    };

    // 返回项目（如有关联）
    let back_to_project = move |_| {
        if let Some(t) = task_res.read().as_ref().and_then(|r| r.as_ref().ok()) {
            if let Some(pid) = &t.project_id {
                navigator.push(format!("/projects/{}", pid));
            } else {
                navigator.push("/projects".to_string());
            }
        } else {
            navigator.push("/projects".to_string());
        }
    };

    let tab0_class = if active_tab() == 0 {
        "tab tab-lg tab-active"
    } else {
        "tab tab-lg"
    };
    let tab1_class = if active_tab() == 1 {
        "tab tab-lg tab-active"
    } else {
        "tab tab-lg"
    };
    let tab2_class = if active_tab() == 2 {
        "tab tab-lg tab-active"
    } else {
        "tab tab-lg"
    };
    let tab3_class = if active_tab() == 3 {
        "tab tab-lg tab-active"
    } else {
        "tab tab-lg"
    };

    rsx! {
        AppLayout {
        div { class: "page-header",
            button {
                class: "btn btn-outline btn-sm",
                onclick: back_to_project,
                "← 返回项目"
            }
            button {
                class: "btn btn-primary btn-sm",
                onclick: move |_| show_edit_modal.set(true),
                "✏️ 编辑"
            }
        }
        match task_res.read().as_ref() {
            None => rsx! { div { class: "card bg-base-100 shadow-md", Loading {} } },
            Some(Ok(t)) => {
                let t = t.clone();
                rsx! {
            // Tab 导航
            div { class: "tabs tabs-boxed mb-6",
                button { class: "{tab0_class}", onclick: move |_| active_tab.set(0), "📋 概览" }
                button { class: "{tab1_class}", onclick: move |_| active_tab.set(1), "📊 进度与状态" }
                button { class: "{tab2_class}", onclick: move |_| active_tab.set(2), "🕸️ 关系图" }
                button { class: "{tab3_class}", onclick: move |_| active_tab.set(3), "📦 产物" }
            }

            // Tab 内容
            {match active_tab() {
                0 => rsx! {
                    // === 概览：基本信息 + 标签和依赖 + 统计 ===
                    // 区域 1：基本信息
                    div { class: "card bg-base-100 shadow-md",
                div { class: "card-header",
                    h2 { class: "card-title", "{t.title}" }
                    span { class: "{status_badge(t.status)}", "{status_text(t.status)}" }
                }
                div { class: "detail-grid",
                    div {
                        label { class: "form-label", "描述" }
                        if let Some(desc) = &t.description {
                            if desc.is_empty() {
                                span { class: "text-base-content/70", "暂无描述" }
                            } else {
                                MarkdownRenderer { content: desc.clone(), compact: true }
                            }
                        } else {
                            span { class: "text-base-content/70", "暂无描述" }
                        }
                    }
                    div {
                        label { class: "form-label", "优先级" }
                        span { "{t.priority}" }
                    }
                    div {
                        label { class: "form-label", "分配对象" }
                        {
                            let assignee_type_text = if t.assignee_type == 0 { "用户" } else { "Agent" };
                            rsx! {
                                span { "{assignee_type_text}: {t.assignee_id}" }
                            }
                        }
                    }
                    div {
                        label { class: "form-label", "根用户" }
                        span { class: "font-mono", "{t.root_user_id}" }
                    }
                    if let Some(pid) = &t.project_id {
                        div {
                            label { class: "form-label", "所属项目" }
                            span { class: "font-mono", "{pid}" }
                        }
                    }
                    div {
                        label { class: "form-label", "创建者" }
                        span { class: "font-mono", "{t.created_by}" }
                    }
                    div {
                        label { class: "form-label", "创建时间" }
                        span { class: "font-mono text-base-content/70", "{format_timestamp(Some(t.created_at))}" }
                    }
                    div {
                        label { class: "form-label", "更新时间" }
                        span { class: "font-mono text-base-content/70", "{format_timestamp(Some(t.updated_at))}" }
                    }
                    if let Some(due) = t.due_at {
                        div {
                            label { class: "form-label", "截止时间" }
                            span { class: "font-mono", "{format_timestamp(Some(due))}" }
                        }
                    }
                    if let Some(start) = t.start_at {
                        div {
                            label { class: "form-label", "开始时间" }
                            span { class: "font-mono", "{format_timestamp(Some(start))}" }
                        }
                    }
                    if let Some(end) = t.end_at {
                        div {
                            label { class: "form-label", "结束时间" }
                            span { class: "font-mono", "{format_timestamp(Some(end))}" }
                        }
                    }
                }
            }

            // 区域 1.5：执行计划与结果（Markdown 渲染，Agent 产出）
            if t.execution_plan.as_deref().map(|s| !s.is_empty()).unwrap_or(false)
                || t.execution_result.as_deref().map(|s| !s.is_empty()).unwrap_or(false)
            {
                div { class: "card bg-base-100 shadow-md",
                    div { class: "card-header",
                        h2 { class: "card-title", "规划与执行" }
                    }
                    div { class: "space-y-5",
                        if let Some(plan) = t.execution_plan.as_deref().filter(|s| !s.is_empty()) {
                            div {
                                label { class: "form-label", "执行计划" }
                                MarkdownRenderer { content: plan.to_string() }
                            }
                        }
                        if let Some(result) = t.execution_result.as_deref().filter(|s| !s.is_empty()) {
                            div {
                                label { class: "form-label", "执行结果" }
                                MarkdownRenderer { content: result.to_string() }
                            }
                        }
                    }
                }
            }

            // 区域 2：标签和依赖
            if !t.tags.is_empty() || !t.dependencies.is_empty() {
                div { class: "card bg-base-100 shadow-md",
                    div { class: "card-header",
                        h2 { class: "card-title", "标签与依赖" }
                    }
                    div { class: "detail-card-body",
                        if !t.tags.is_empty() {
                            div { class: "detail-section",
                                label { class: "form-label", "标签" }
                                div { class: "tag-list",
                                    for tag in t.tags.iter() {
                                        span { class: "badge badge-neutral tag-item", "{tag}" }
                                    }
                                }
                            }
                        }
                        if !t.dependencies.is_empty() {
                            div { class: "detail-section",
                                label { class: "form-label", "前置任务" }
                                ul { class: "dependency-list",
                                    for dep in t.dependencies.iter() {
                                        li { class: "font-mono", "{dep}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if t.stats.is_some() || t.model_call_stats.is_some() {
                TaskStatsPanel {
                    stats: t.stats.clone(),
                    model_call_stats: t.model_call_stats.clone(),
                }
            }
                },
                1 => rsx! {
                    // === 进度与状态：进度管理 + 状态流转 ===
                    // 区域 3：进度管理
            div { class: "card bg-base-100 shadow-md",
                div { class: "card-header",
                    h2 { class: "card-title", "进度管理" }
                }
                div { class: "detail-card-body",
                    div { class: "detail-section",
                        div { class: "progress-section",
                            div { class: "overview-progress",
                                div { class: "overview-progress-bar",
                                    div { class: "{progress_bar_class(t.progress)}", style: "width: {t.progress}%;" }
                                }
                                span { class: "overview-progress-text", "{t.progress}%" }
                            }
                        }
                    }
                    div { class: "detail-action-row",
                        button {
                            class: "btn btn-primary",
                            onclick: open_progress_modal,
                            "更新进度"
                        }
                    }
                }
            }

            // 区域 4：状态流转
            div { class: "card bg-base-100 shadow-md",
                div { class: "card-header",
                    h2 { class: "card-title", "状态流转" }
                }
                div { class: "detail-card-body",
                    div { class: "detail-action-row",
                        if t.status != 1 {
                            button {
                                class: "btn btn-warning",
                                onclick: on_review,
                                "送审"
                            }
                        }
                        if t.status != 2 {
                            button {
                                class: "btn btn-info",
                                onclick: on_pending,
                                "待处理"
                            }
                        }
                        if t.status != 3 {
                            button {
                                class: "btn btn-primary",
                                onclick: on_start,
                                "开始"
                            }
                        }
                        if t.status != 4 {
                            button {
                                class: "btn btn-primary",
                                onclick: on_complete,
                                "完成"
                            }
                        }
                        if t.status != 0 && t.status != 5 {
                            button {
                                class: "btn btn-error",
                                onclick: on_cancel,
                                "取消"
                            }
                        }
                        if t.status != 5 {
                            button {
                                class: "btn btn-outline",
                                onclick: on_archive,
                                "归档"
                            }
                        }
                    }
                }
            }

                },
                2 => rsx! {
                    // === 关系图 ===
                    div { class: "card bg-base-100 shadow-md",
                        div { class: "card-header",
                            h2 { class: "card-title", "关系图" }
                        }
                        div { class: "p-4",
                            div { class: "w-full h-[520px]",
                                WorkspaceGraph {
                                    view: WorkspaceView::TaskDetail(t.id.clone()),
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
                3 => rsx! {
                    // === 产物 ===
                    {
                    let arts: Vec<_> = task_res.read().as_ref()
                        .and_then(|r| r.as_ref().ok())
                        .and_then(|t| t.artifacts.clone())
                        .unwrap_or_default();
                        if arts.is_empty() {
                            rsx! { EmptyState { icon: "📦".to_string(), message: "暂无产物".to_string() } }
                        } else {
                            rsx! {
                                div { class: "space-y-3",
                                    for art in arts.iter() {
                                        div { class: "card bg-base-100 shadow-sm",
                                            div { class: "card-body p-4",
                                                div { class: "flex justify-between items-start",
                                                    div {
                                                        h3 { class: "font-semibold", "{art.name}" }
                                                        if !art.description.is_empty() {
                                                            p { class: "text-sm text-base-content/60 mt-1", "{art.description}" }
                                                        }
                                                    }
                                                    Link {
                                                        class: "btn btn-ghost btn-sm",
                                                        to: crate::pages::Route::ProjectArtifactDetail { id: art.id.clone() },
                                                        "查看详情 →"
                                                    }
                                                }
                                                div { class: "flex gap-2 mt-2 flex-wrap",
                                                    span { class: "badge badge-sm", "{format_file_type(art.file_type)}" }
                                                    span { class: "badge badge-sm badge-info", "{art.mime_type}" }
                                                    span { class: "badge badge-sm", "{crate::utils::format_file_size(art.file_size)}" }
                                                    for tag in art.tags.iter() {
                                                        span { class: "{tag_chip()}", "#{tag}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                _ => rsx! { div {} },
            }}

            // 进度更新弹窗
            Modal {
                title: "更新进度".to_string(),
                show: show_progress_modal(),
                on_close: move |_| show_progress_modal.set(false),
                footer: Some(rsx! {
                    div { class: "modal-footer-actions",
                        button {
                            class: "btn btn-outline",
                            disabled: updating_progress(),
                            onclick: move |_| show_progress_modal.set(false),
                            "取消"
                        }
                        button {
                            class: "btn btn-primary",
                            disabled: updating_progress(),
                            onclick: submit_progress,
                            if updating_progress() { "更新中..." } else { "更新" }
                        }
                    }
                }),
                div { class: "modal-body-stack",
                    div {
                        label { class: "form-label", "进度（0-100）" }
                        input {
                            class: "input input-bordered w-full",
                            r#type: "number",
                            min: "0",
                            max: "100",
                            value: "{new_progress}",
                            oninput: move |e| {
                                if let Ok(v) = e.value().parse::<i32>() {
                                    new_progress.set(v.clamp(0, 100));
                                }
                            },
                        }
                    }
                    div {
                        label { class: "form-label", "预览" }
                        div { class: "overview-progress",
                            div { class: "overview-progress-bar",
                                div { class: "{progress_bar_class(new_progress())}", style: "width: {new_progress()}%;" }
                            }
                            span { class: "overview-progress-text", "{new_progress()}%" }
                        }
                    }
                }
            }
                }
            }
            Some(Err(e)) => rsx! {
                div { class: "card bg-base-100 shadow-md", EmptyState { icon: "❓".to_string(), message: format!("加载失败: {}", e) } }
            },
        }
        TaskEditModal {
            mode: TaskEditMode::Edit { task_id: id_for_edit.clone() },
            show: show_edit_modal(),
            on_close: move |_| show_edit_modal.set(false),
            on_success: move |t: GetTaskResponse| {
                task_res.set(Some(Ok(t)));
            },
        }
        }
    }
}

fn format_file_type(t: common::enums::FileType) -> &'static str {
    match t {
        common::enums::FileType::Document => "文档",
        common::enums::FileType::Image => "图片",
        common::enums::FileType::Audio => "音频",
        common::enums::FileType::Video => "视频",
        common::enums::FileType::Binary => "二进制",
    }
}
