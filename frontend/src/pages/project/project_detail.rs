//! 项目详情页 - 基本信息、状态管理、任务列表、产物列表

use dioxus::prelude::*;
use dioxus_router::{Link, use_navigator};

use crate::api::hr::query_agents;
use crate::api::project::*;
use crate::components::charts::donut_chart::{DonutChart, DonutSlice};
use crate::components::hud::{HudPanel, HudProgress};
use crate::components::markdown::{MarkdownRenderer, MermaidDiagram};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::components::stats::ProjectStatsPanel;
use crate::components::workspace_graph::{WorkspaceGraph, WorkspaceView};
use crate::layouts::app_layout::AppLayout;
use crate::pages::project::task_edit_modal::{TaskEditModal, TaskEditMode};
use crate::store::toast::use_toast;
use crate::utils::task_status_color;
use crate::utils::{
    progress_tone, project_status_badge, project_status_text, task_status_badge, task_status_text,
};
use common::api::{
    AgentListItem, AgentQueryRequest, ArtifactDetail, CreateArtifactRequest, GetProjectRequest,
    PaginationParams, ProjectListItem, TaskListItem, UpdateProjectRequest,
    UpdateProjectStatusRequest, UpdateTaskStatusRequest,
};
use common::enums::{ArtifactSourceType, ProjectStatus, TaskStatus};

fn artifact_source_type_text(source_type: ArtifactSourceType) -> &'static str {
    match source_type {
        ArtifactSourceType::Attachment => "附件",
        ArtifactSourceType::GeneratedContent => "生成内容",
        ArtifactSourceType::RemoteUrl => "远程链接",
    }
}

#[component]
pub fn ProjectDetail(id: String) -> Element {
    // 方案 B：响应式 rid + use_resource，拉取仅在 :id 变化时触发
    let route = dioxus_router::use_route::<crate::pages::Route>();
    let mut rid = use_signal(String::new);
    if let crate::pages::Route::ProjectDetail { id: route_id } = &route
        && *rid.peek() != *route_id
    {
        rid.set(route_id.clone());
    }
    let mut project_res = use_resource(move || {
        let id = rid();
        async move {
            let req = GetProjectRequest {
                id,
                with_stats: Some(true),
                with_model_call_stats: Some(true),
                stats_interval: Some("daily".to_string()),
                with_artifacts: Some(true),
                with_task_graph: Some(true),
                ..Default::default()
            };
            get_project(req).await
        }
    });
    let mut tasks = use_signal(Vec::<TaskListItem>::new);
    let mut artifacts = use_signal(Vec::<ArtifactDetail>::new);

    // 产物新增 Modal 状态
    let mut show_artifact_modal = use_signal(|| false);
    let mut new_artifact_name = use_signal(String::new);
    let mut new_artifact_description = use_signal(String::new);
    let toast = use_toast();

    // 任务创建 Modal 状态
    let mut show_task_modal = use_signal(|| false);
    let navigator = use_navigator();

    // 项目编辑 Modal 状态
    let mut show_edit_modal = use_signal(|| false);
    let mut edit_name = use_signal(String::new);
    let mut edit_description = use_signal(String::new);
    let mut edit_priority = use_signal(|| "0".to_string());
    let mut edit_tags = use_signal(String::new);
    let mut saving_meta = use_signal(|| false);
    let id_for_edit = id.clone();

    // Tab 切换：0=概览 1=任务列表 2=产物 3=关系图
    let mut active_tab = use_signal(|| 0usize);
    // 关系图所需数据：全局 projects + agents 列表（tasks 已有）
    let mut graph_projects = use_signal(Vec::<ProjectListItem>::new);
    let mut graph_agents = use_signal(Vec::<AgentListItem>::new);

    // 任务 / 关系图数据随 :id 变化加载（project 本身由上方 use_resource 负责）
    let rid_for_load = rid;
    use_effect(move || {
        let id_clone = rid_for_load();
        spawn(async move {
            match list_project_tasks(&id_clone).await {
                Ok(resp) => {
                    let tasks_vec = resp.tasks;

                    // 批量加载关联 agents（仅本项目任务涉及到的 assignee），消除 N+1
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
                            Err(e) => toast.error(format!("批量获取 Agent 失败: {}", e)),
                        }
                    }

                    tasks.set(tasks_vec);
                }
                Err(e) => toast.error(&e),
            }
        });
    });

    // 由 project resource 派生 artifacts / graph_projects（随 project 解析而更新）
    use_effect(move || {
        if let Some(Ok(p)) = project_res.read().as_ref() {
            if let Some(ref arts) = p.artifacts {
                artifacts.set(arts.clone());
            }
            graph_projects.set(vec![ProjectListItem::from(p)]);
        }
    });

    // 项目状态切换：启动(3)
    let id_for_start = id.clone();
    let start_project = move |_| {
        let id_clone = id_for_start.clone();
        spawn(async move {
            let req = UpdateProjectStatusRequest {
                id: id_clone.clone(),
                status: ProjectStatus::InProgress,
            };
            match update_project_status(req).await {
                Ok(_) => {
                    toast.success("项目已启动");
                    let req = GetProjectRequest {
                        id: id_clone.clone(),
                        with_stats: Some(true),
                        with_model_call_stats: Some(true),
                        stats_interval: Some("daily".to_string()),
                        ..Default::default()
                    };
                    match get_project(req).await {
                        Ok(p) => project_res.set(Some(Ok(p))),
                        Err(e) => toast.error(&e),
                    }
                }
                Err(e) => toast.error(format!("启动失败: {}", e)),
            }
        });
    };

    // 项目状态切换：完成(4)
    let id_for_complete = id.clone();
    let complete_project = move |_| {
        let id_clone = id_for_complete.clone();
        spawn(async move {
            let req = UpdateProjectStatusRequest {
                id: id_clone.clone(),
                status: ProjectStatus::Completed,
            };
            match update_project_status(req).await {
                Ok(_) => {
                    toast.success("项目已完成");
                    let req = GetProjectRequest {
                        id: id_clone.clone(),
                        with_stats: Some(true),
                        with_model_call_stats: Some(true),
                        stats_interval: Some("daily".to_string()),
                        ..Default::default()
                    };
                    match get_project(req).await {
                        Ok(p) => project_res.set(Some(Ok(p))),
                        Err(e) => toast.error(&e),
                    }
                }
                Err(e) => toast.error(format!("完成失败: {}", e)),
            }
        });
    };

    // 项目状态切换：归档(5)
    let id_for_archive = id.clone();
    let archive_project = move |_| {
        let id_clone = id_for_archive.clone();
        spawn(async move {
            let req = UpdateProjectStatusRequest {
                id: id_clone.clone(),
                status: ProjectStatus::Archived,
            };
            match update_project_status(req).await {
                Ok(_) => {
                    toast.success("项目已归档");
                    let req = GetProjectRequest {
                        id: id_clone.clone(),
                        with_stats: Some(true),
                        with_model_call_stats: Some(true),
                        stats_interval: Some("daily".to_string()),
                        ..Default::default()
                    };
                    match get_project(req).await {
                        Ok(p) => project_res.set(Some(Ok(p))),
                        Err(e) => toast.error(&e),
                    }
                }
                Err(e) => toast.error(format!("归档失败: {}", e)),
            }
        });
    };

    // 打开新增产物 Modal
    let open_artifact_modal = move |_| {
        new_artifact_name.set(String::new());
        new_artifact_description.set(String::new());
        show_artifact_modal.set(true);
    };

    // 关闭新增产物 Modal
    let close_artifact_modal = move |_| {
        show_artifact_modal.set(false);
    };

    // 提交新增产物
    let id_for_artifact_create = id.clone();
    let submit_artifact = move |_| {
        let pid = id_for_artifact_create.clone();
        let name = new_artifact_name.read().clone();
        let description = new_artifact_description.read().clone();
        if name.trim().is_empty() {
            toast.error("产物名称不能为空");
            return;
        }
        spawn(async move {
            let req = CreateArtifactRequest {
                project_id: pid.clone(),
                task_id: None,
                name: name.trim().to_string(),
                description: Some(description).filter(|s| !s.trim().is_empty()),
                source_type: ArtifactSourceType::GeneratedContent,
                attachment_id: None,
                content: None,
                file_name: None,
                mime_type: None,
                file_type: None,
                tags: None,
            };
            match create_artifact(req).await {
                Ok(_) => {
                    toast.success("产物已创建");
                    show_artifact_modal.set(false);
                    new_artifact_name.set(String::new());
                    new_artifact_description.set(String::new());
                    match list_artifacts(&pid).await {
                        Ok(list) => artifacts.set(list),
                        Err(e) => toast.error(&e),
                    }
                }
                Err(e) => toast.error(format!("创建产物失败: {}", e)),
            }
        });
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

    let tasks_list = tasks.read().clone();
    let artifacts_list = artifacts.read().clone();

    let overall_progress = if tasks_list.is_empty() {
        0
    } else {
        tasks_list.iter().map(|t| t.progress).sum::<i32>() / tasks_list.len() as i32
    };

    // 按 6 种状态全量统计，构造 DonutChart 数据
    // 顺序：进行中(3) → 待处理(2) → 待审核(1) → 已完成(4) → 已归档(5) → 已取消(0)
    // 把"进行中"放最前让 HUD 主色橙最显眼，"已完成"绿色紧跟其后
    let task_status_counts: [(i32, &str); 6] = [
        (3, "进行中"),
        (2, "待处理"),
        (1, "待审核"),
        (4, "已完成"),
        (5, "已归档"),
        (0, "已取消"),
    ];
    let donut_slices: Vec<DonutSlice> = task_status_counts
        .iter()
        .map(|(status, label)| {
            let count = tasks_list.iter().filter(|t| t.status == *status).count() as u64;
            DonutSlice {
                label: label.to_string(),
                value: count,
                color: task_status_color(*status).to_string(),
            }
        })
        .filter(|s| s.value > 0) // 过滤掉 0 值状态，避免图例冗余
        .collect();

    rsx! {
        AppLayout {
        match project_res.read().as_ref() {
            None => rsx! { HudPanel { Loading {} } },
            Some(Ok(p)) => {
                let p = p.clone();
                rsx! {
            // Tab 导航
            div { class: "tabs tabs-boxed hud-tabs mb-6",
                button { class: "{tab0_class}", onclick: move |_| active_tab.set(0), "📋 概览" }
                button { class: "{tab1_class}", onclick: move |_| active_tab.set(1), "📝 任务列表" }
                button { class: "{tab2_class}", onclick: move |_| active_tab.set(2), "📦 产物" }
                button { class: "{tab3_class}", onclick: move |_| active_tab.set(3), "🕸️ 关系图" }
            }

            // Tab 内容
            {match active_tab() {
                0 => rsx! {
                    // === 概览：基本信息 + 项目概览统计 + 状态管理 + ProjectStatsPanel ===
                    // 区域 1：项目基本信息卡片
                    HudPanel {
                        title: p.name.clone(),
                        div { class: "flex justify-end mb-3",
                            button {
                                class: "btn btn-ghost btn-sm",
                                onclick: move |_| {
                                    if let Some(p) = project_res.read().as_ref().and_then(|r| r.as_ref().ok()).cloned() {
                                        edit_name.set(p.name.clone());
                                        edit_description.set(p.description.clone().unwrap_or_default());
                                        edit_priority.set(p.priority.to_string());
                                        edit_tags.set(p.tags.join(", "));
                                        show_edit_modal.set(true);
                                    }
                                },
                                "✏️ 编辑"
                            }
                        }
                        div { class: "detail-grid",
                            div {
                                label { class: "form-label", "描述" }
                                if let Some(desc) = &p.description {
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
                                label { class: "form-label", "状态" }
                                span { class: "{project_status_badge(p.status)}", "{project_status_text(p.status)}" }
                            }
                            div {
                                label { class: "form-label", "优先级" }
                                span { "{p.priority}" }
                            }
                            div {
                                label { class: "form-label", "标签" }
                                if p.tags.is_empty() {
                                    span { class: "text-base-content/70", "无标签" }
                                } else {
                                    div { class: "tag-list",
                                        for tag in p.tags.iter() {
                                            span { class: "badge badge-neutral tag-item", "{tag}" }
                                        }
                                    }
                                }
                            }
                            div {
                                label { class: "form-label", "创建时间" }
                                span { class: "font-mono text-base-content/70", "{p.created_at}" }
                            }
                        }
                    }

                    // 区域 1.5：执行计划与结果（Markdown 渲染，Agent 产出）
                    if p.workflow.as_deref().map(|s| !s.is_empty()).unwrap_or(false)
                        || p.guidance.as_deref().map(|s| !s.is_empty()).unwrap_or(false)
                        || p.execution_plan.as_deref().map(|s| !s.is_empty()).unwrap_or(false)
                        || p.execution_result.as_deref().map(|s| !s.is_empty()).unwrap_or(false)
                    {
                        HudPanel {
                            title: "规划与执行".to_string(),
                            div { class: "space-y-5",
                                if let Some(wf) = p.workflow.as_deref().filter(|s| !s.is_empty()) {
                                    div {
                                        label { class: "form-label", "运作流程" }
                                        MarkdownRenderer { content: wf.to_string() }
                                    }
                                }
                                if let Some(gd) = p.guidance.as_deref().filter(|s| !s.is_empty()) {
                                    div {
                                        label { class: "form-label", "指导建议" }
                                        MarkdownRenderer { content: gd.to_string() }
                                    }
                                }
                                if let Some(plan) = p.execution_plan.as_deref().filter(|s| !s.is_empty()) {
                                    div {
                                        label { class: "form-label", "执行计划" }
                                        MarkdownRenderer { content: plan.to_string() }
                                    }
                                }
                                if let Some(result) = p.execution_result.as_deref().filter(|s| !s.is_empty()) {
                                    div {
                                        label { class: "form-label", "执行结果" }
                                        MarkdownRenderer { content: result.to_string() }
                                    }
                                }
                            }
                        }
                    }

                    // 区域 1.6：任务依赖图（Mermaid，with_task_graph 按需返回）
                    if let Some(graph) = p.task_graph.as_deref().filter(|s| !s.is_empty()) {
                        HudPanel {
                            title: "任务依赖图".to_string(),
                            MermaidDiagram { code: graph.to_string() }
                        }
                    }

                    // 区域 2：项目概览统计
                    HudPanel {
                        title: "项目概览".to_string(),
                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                            div { class: "min-w-0",
                                div { class: "hud-eyebrow mb-2", "整体进度" }
                                HudProgress { value: overall_progress, tone: Some(progress_tone(overall_progress).to_string()), show_value: Some(true) }
                            }
                            div { class: "min-w-0",
                                div { class: "hud-eyebrow mb-2", "任务状态分布" }
                                if donut_slices.is_empty() {
                                    div { class: "text-base-content/60 text-sm py-8 text-center",
                                        "暂无任务"
                                    }
                                } else {
                                    DonutChart {
                                        data: donut_slices.clone(),
                                        width: Some(240.0),
                                        height: Some(240.0),
                                        center_label: Some("任务总数".to_string()),
                                    }
                                }
                            }
                        }
                    }

                    // 区域 3：状态管理
                    HudPanel {
                        title: "状态管理".to_string(),
                        div { class: "detail-action-row",
                            if p.status != 3 {
                                button { class: "btn btn-primary", onclick: start_project, "启动项目" }
                            }
                            if p.status != 4 {
                                button { class: "btn btn-primary", onclick: complete_project, "完成项目" }
                            }
                            if p.status != 5 {
                                button { class: "btn btn-outline", onclick: archive_project, "归档项目" }
                            }
                        }
                    }

                    if p.stats.is_some() || p.model_call_stats.is_some() {
                        ProjectStatsPanel {
                            stats: p.stats.clone(),
                            model_call_stats: p.model_call_stats.clone(),
                        }
                    }
                },
                1 => rsx! {
                    // === 任务列表 ===
                    HudPanel {
                        title: "任务列表".to_string(),
                        div { class: "flex justify-end mb-3",
                            button {
                                class: "btn btn-primary btn-sm",
                                onclick: move |_| show_task_modal.set(true),
                                "+ 新建任务"
                            }
                        }
                        if tasks_list.is_empty() {
                            EmptyState { icon: "📋".to_string(), message: "暂无任务".to_string() }
                        } else {
                            table { class: "table table-zebra",
                                thead { tr {
                                    th { "标题" }
                                    th { "状态" }
                                    th { "优先级" }
                                    th { "进度" }
                                    th { "操作" }
                                }}
                                tbody {
                                    for t in tasks_list.iter() {
                                        {
                                            let task_id = t.id.clone();
                                            let task_title = t.title.clone();
                                            let task_status = t.status;
                                            let task_priority = t.priority;
                                            let task_progress = t.progress;
                                            let tid_start = task_id.clone();
                                            let tid_complete = task_id.clone();
                                            let pid_start = id.clone();
                                            let pid_complete = id.clone();
                                            rsx! {
                                                tr {
                                                    key: "{task_id}",
                                                    class: "table-row-clickable",
                                                    onclick: {
                                                        let tid = task_id.clone();
                                                        move |_| {
                                                            navigator.push(format!("/tasks/{}", tid));
                                                        }
                                                    },
                                                    td { "data-label": "标题", "{task_title}" }
                                                    td { "data-label": "状态", span { class: "{task_status_badge(task_status)}", "{task_status_text(task_status)}" } }
                                                    td { "data-label": "优先级", "{task_priority}" }
                                                    td { "data-label": "进度",
                                                        HudProgress { value: task_progress, tone: Some("primary".to_string()), show_value: Some(true) }
                                                    }
                                                    td { "data-label": "操作",
                                                        div { class: "action-group",
                                                            if task_status != 3 {
                                                                button { class: "btn btn-outline btn-sm",
                                                                    onclick: move |e: Event<MouseData>| {
                                                                        // 修复 HIGH #8：阻止事件冒泡到 <tr> 的 onclick，
                                                                        // 否则点击"开始"会同时触发状态更新和页面跳转
                                                                        e.stop_propagation();
                                                                        let tid = tid_start.clone();
                                                                        let pid = pid_start.clone();
                                                                        spawn(async move {
                                                                            match update_task_status(UpdateTaskStatusRequest {
                                                                                id: tid.clone(),
                                                                                status: TaskStatus::InProgress,
                                                                            })
                                                                            .await
                                                                            {
                                                                                Ok(_) => {
                                                                                    toast.success("任务已开始");
                                                                                    match list_project_tasks(&pid).await {
                                                                                        Ok(resp) => tasks.set(resp.tasks),
                                                                                        Err(e) => toast.error(&e),
                                                                                    }
                                                                                }
                                                                                Err(e) => toast.error(format!("操作失败: {}", e)),
                                                                            }
                                                                        });
                                                                    },
                                                                    "开始"
                                                                }
                                                            }
                                                            if task_status != 4 {
                                                                button { class: "btn btn-primary btn-sm",
                                                                    onclick: move |e: Event<MouseData>| {
                                                                        // 修复 HIGH #8：同上，阻止冒泡
                                                                        e.stop_propagation();
                                                                        let tid = tid_complete.clone();
                                                                        let pid = pid_complete.clone();
                                                                        spawn(async move {
                                                                            match update_task_status(UpdateTaskStatusRequest {
                                                                                id: tid.clone(),
                                                                                status: TaskStatus::Completed,
                                                                            })
                                                                            .await
                                                                            {
                                                                                Ok(_) => {
                                                                                    toast.success("任务已完成");
                                                                                    match list_project_tasks(&pid).await {
                                                                                        Ok(resp) => tasks.set(resp.tasks),
                                                                                        Err(e) => toast.error(&e),
                                                                                    }
                                                                                }
                                                                                Err(e) => toast.error(format!("操作失败: {}", e)),
                                                                            }
                                                                        });
                                                                    },
                                                                    "完成"
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
                },
                2 => rsx! {
                    // === 产物 ===
                    HudPanel {
                        title: "项目产物".to_string(),
                        div { class: "detail-card-body",
                            div { class: "detail-toolbar",
                                button { class: "btn btn-primary", onclick: open_artifact_modal, "+ 新增产物" }
                            }
                            if artifacts_list.is_empty() {
                                EmptyState { icon: "📦".to_string(), message: "暂无产物".to_string() }
                            } else {
                                table { class: "table table-zebra",
                                    thead { tr {
                                        th { "名称" }
                                        th { "描述" }
                                        th { "来源类型" }
                                        th { "文件大小" }
                                        th { "创建时间" }
                                        th { "操作" }
                                    }}
                                    tbody {
                                        for a in artifacts_list.iter() {
                                            {
                                                let artifact_id = a.id.clone();
                                                let artifact_name = a.name.clone();
                                                let artifact_description = a.description.clone();
                                                let artifact_source_type = a.source_type;
                                                let artifact_file_size = a.file_size;
                                                let artifact_created_at = a.created_at;
                                                let aid_delete = artifact_id.clone();
                                                let pid_refresh = id.clone();
                                                rsx! {
                                                    tr { key: "{artifact_id}",
                                                        td { "data-label": "名称", "{artifact_name}" }
                                                        td { "data-label": "描述",
                                                            if artifact_description.is_empty() {
                                                                span { class: "text-base-content/70", "暂无描述" }
                                                            } else {
                                                                "{artifact_description}"
                                                            }
                                                        }
                                                        td { "data-label": "来源类型", span { class: "badge badge-neutral", "{artifact_source_type_text(artifact_source_type)}" } }
                                                        td { "data-label": "文件大小", "{artifact_file_size}" }
                                                        td { "data-label": "创建时间", span { class: "font-mono text-base-content/70", "{artifact_created_at}" } }
                                                        td { "data-label": "操作",
                                                            div { class: "flex gap-1",
                                                                Link {
                                                                    class: "btn btn-ghost btn-sm",
                                                                    to: crate::pages::Route::ProjectArtifactDetail { id: artifact_id.clone() },
                                                                    "查看"
                                                                }
                                                                button { class: "btn btn-error btn-sm",
                                                                    onclick: move |_| {
                                                                        let aid = aid_delete.clone();
                                                                        let pid = pid_refresh.clone();
                                                                        spawn(async move {
                                                                            match delete_artifact(&aid).await {
                                                                                Ok(_) => {
                                                                                    toast.success("产物已删除");
                                                                                    match list_artifacts(&pid).await {
                                                                                        Ok(list) => artifacts.set(list),
                                                                                        Err(e) => toast.error(&e),
                                                                                    }
                                                                                }
                                                                                Err(e) => toast.error(format!("删除失败: {}", e)),
                                                                            }
                                                                        });
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
                },
                3 => rsx! {
                    // === 关系图 ===
                    HudPanel {
                        title: "关系图".to_string(),
                        div { class: "p-4",
                            div { class: "w-full h-[520px]",
                                WorkspaceGraph {
                                    view: WorkspaceView::ProjectDetail(p.id.clone()),
                                    projects: graph_projects.read().clone(),
                                    agents: graph_agents.read().clone(),
                                    tasks: tasks_list.clone(),
                                    width: 800.0,
                                    height: 500.0,
                                    auto_size: true,
                                }
                            }
                        }
                    }
                },
                _ => rsx! { div {} },
            }}

            // 新增产物 Modal
            Modal {
                title: "新增产物".to_string(),
                show: show_artifact_modal(),
                on_close: close_artifact_modal,
                footer: Some(rsx! {
                    div { class: "modal-footer-actions",
                        button { class: "btn btn-outline", onclick: move |_| { show_artifact_modal.set(false); }, "取消" }
                        button { class: "btn btn-primary", onclick: submit_artifact, "创建" }
                    }
                }),
                div { class: "modal-body-stack",
                    div {
                        label { class: "form-label", "名称" }
                        input {
                            class: "input input-bordered w-full",
                            r#type: "text",
                            placeholder: "请输入产物名称",
                            value: "{new_artifact_name}",
                            oninput: move |e| new_artifact_name.set(e.value().clone()),
                        }
                    }
                    div {
                        label { class: "form-label", "描述" }
                        textarea {
                            class: "input input-bordered w-full",
                            placeholder: "请输入产物描述（可选）",
                            value: "{new_artifact_description}",
                            oninput: move |e| new_artifact_description.set(e.value().clone()),
                            rows: 3,
                        }
                    }
                }
            }

            // 新建任务 Modal
            TaskEditModal {
                mode: TaskEditMode::Create { project_id: Some(id.clone()) },
                show: show_task_modal(),
                on_close: move |_| show_task_modal.set(false),
                on_success: move |_| {
                    show_task_modal.set(false);
                    // 刷新任务列表
                    let pid = id.clone();
                    spawn(async move {
                        if let Ok(resp) = list_project_tasks(&pid).await {
                            tasks.set(resp.tasks);
                        }
                    });
                },
            }
                }
            }
            Some(Err(e)) => rsx! {
                HudPanel {
                    EmptyState { icon: "❓".to_string(),
                    message: format!("加载失败: {}",
                    e) } },
            },
        }

        // 编辑项目 Modal
        Modal {
            title: "编辑项目".to_string(),
            show: show_edit_modal(),
            on_close: move |_| show_edit_modal.set(false),
            footer: Some(rsx! {
                div { class: "modal-footer-actions",
                    button { class: "btn btn-ghost", onclick: move |_| show_edit_modal.set(false), "取消" }
                    button {
                        class: "btn btn-primary",
                        disabled: saving_meta(),
                        onclick: move |_| {
                            let name = edit_name.read().trim().to_string();
                            if name.is_empty() { toast.error("名称不能为空"); return; }
                            let priority: i32 = edit_priority.read().trim().parse().unwrap_or(0);
                            let tags: Vec<String> = edit_tags.read().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                            let desc = edit_description.read().clone();
                            let description = Some(desc).filter(|s| !s.trim().is_empty());
                            let req = UpdateProjectRequest {
                                id: id_for_edit.clone(),
                                name: Some(name),
                                description,
                                priority: Some(priority),
                                tags: Some(tags),
                                execution_plan: None,
                                execution_result: None,
                            };
                            saving_meta.set(true);
                            let id_clone = id_for_edit.clone();
                            spawn(async move {
                                match update_project(req).await {
                                    Ok(_) => {
                                        toast.success("项目已更新");
                                        show_edit_modal.set(false);
                                        let req = GetProjectRequest {
                                            id: id_clone.clone(),
                                            with_stats: Some(true),
                                            with_model_call_stats: Some(true),
                                            stats_interval: Some("daily".to_string()),
                                            ..Default::default()
                                        };
                                        match get_project(req).await {
                                            Ok(p) => project_res.set(Some(Ok(p))),
                                            Err(e) => toast.error(format!("重新加载失败: {}", e)),
                                        }
                                    }
                                    Err(e) => toast.error(format!("更新失败: {}", e)),
                                }
                                saving_meta.set(false);
                            });
                        },
                        if saving_meta() { "保存中..." } else { "保存" }
                    }
                }
            }),
            div { class: "space-y-4",
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "名称 *" } }
                    input { class: "input input-bordered w-full", value: "{edit_name}",
                        oninput: move |e| edit_name.set(e.value().clone()) }
                }
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "描述" } }
                    textarea { class: "textarea textarea-bordered w-full", value: "{edit_description}",
                        oninput: move |e| edit_description.set(e.value().clone()) }
                }
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "优先级（数字，越大越优先）" } }
                    input { class: "input input-bordered w-full", r#type: "number", value: "{edit_priority}",
                        oninput: move |e| edit_priority.set(e.value().clone()) }
                }
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "标签（逗号分隔）" } }
                    input { class: "input input-bordered w-full", value: "{edit_tags}",
                        oninput: move |e| edit_tags.set(e.value().clone()), placeholder: "tag1, tag2" }
                }
            }
        }
        }
    }
}
