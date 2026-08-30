//! 任务管理页面 - 列表视图 + 看板视图

use dioxus::prelude::*;
use dioxus_router::use_navigator;

use crate::api::project::{list_projects, list_tasks, query_tasks, search_tasks};
use crate::components::hud::{HudPanel, HudProgress, PageHeader, StatGrid, StatReadout};
use crate::components::kanban_canvas::{KanbanCanvas, KanbanColumn, KanbanTask};
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use crate::utils::{format_datetime as format_time, task_status_badge, task_status_text};
use common::api::{
    ListProjectsRequest, ListProjectsResponseItem, ListTasksRequest, SearchTasksRequest,
    TaskListItem, TaskQueryRequest,
};
use common::enums::{AssigneeType, TaskStatus};

#[derive(Debug, Clone, PartialEq)]
pub enum ViewMode {
    List,
    Board,
}

#[component]
pub fn TaskList() -> Element {
    let mut tasks = use_signal(Vec::<TaskListItem>::new);
    let mut projects = use_signal(Vec::<ListProjectsResponseItem>::new);
    let mut loading = use_signal(|| true);
    let mut view_mode = use_signal(|| ViewMode::Board);

    // 筛选状态
    let mut filter_project_id = use_signal(String::new);
    let mut filter_status = use_signal(|| -1i32);
    let mut filter_assignee_type = use_signal(|| -1i32);
    let mut search_keyword = use_signal(String::new);
    let mut search_request_id = use_signal(|| 0u32);

    let toast = use_toast();
    let navigator = use_navigator();

    // 加载数据
    let load_data = move || {
        spawn(async move {
            // 在 spawn 内部读取信号（避免 use_effect 订阅）
            loading.set(true);
            let keyword = search_keyword();
            let project_id = filter_project_id();
            let status = filter_status();
            let assignee_type = filter_assignee_type();
            let my_id = search_request_id() + 1;
            search_request_id.set(my_id);

            let has_filter = !project_id.is_empty() || status >= 0 || assignee_type >= 0;

            // 三场景切换：
            // 无关键词 + 无筛选 → list_tasks
            // 无关键词 + 有筛选 → query_tasks
            // 有关键词 → search_tasks（可同时带筛选）
            let result = if keyword.trim().is_empty() && !has_filter {
                list_tasks(ListTasksRequest::default())
                    .await
                    .map(|p| p.items)
            } else if keyword.trim().is_empty() {
                query_tasks(&TaskQueryRequest {
                    project_id: if project_id.is_empty() {
                        None
                    } else {
                        Some(project_id.clone())
                    },
                    status_in: if status >= 0 {
                        Some(vec![TaskStatus::from_i32(status)])
                    } else {
                        None
                    },
                    assignee_type: if assignee_type >= 0 {
                        Some(AssigneeType::from_i32(assignee_type))
                    } else {
                        None
                    },
                    ..Default::default()
                })
                .await
                .map(|p| p.items)
            } else {
                search_tasks(&SearchTasksRequest {
                    keyword: Some(keyword.clone()),
                    project_id: if project_id.is_empty() {
                        None
                    } else {
                        Some(project_id.clone())
                    },
                    status_in: if status >= 0 {
                        Some(vec![TaskStatus::from_i32(status)])
                    } else {
                        None
                    },
                    assignee_type: if assignee_type >= 0 {
                        Some(AssigneeType::from_i32(assignee_type))
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
                Ok(v) => tasks.set(v),
                Err(e) => toast.error(&e),
            }
            match list_projects(ListProjectsRequest::default()).await {
                Ok(page) => projects.set(page.items),
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    };

    // 初始加载
    use_effect(move || {
        load_data();
    });

    let tasks_list = tasks.read().clone();
    let projects_list = projects.read().clone();

    // 统计数据
    let total = tasks_list.len();
    let completed = tasks_list.iter().filter(|t| t.status == 4).count();
    let in_progress = tasks_list.iter().filter(|t| t.status == 3).count();
    let pending = tasks_list
        .iter()
        .filter(|t| t.status == 2 || t.status == 1)
        .count();

    // 看板数据分组
    let board_groups = [
        (1, "待审核"),
        (2, "待处理"),
        (3, "进行中"),
        (4, "已完成"),
        (5, "已归档"),
    ];

    let filtered_tasks_by_status = |status: i32| {
        tasks_list
            .iter()
            .filter(|t| t.status == status)
            .collect::<Vec<_>>()
    };

    let board_columns: Vec<(i32, &str, Vec<&TaskListItem>)> = board_groups
        .iter()
        .map(|(status, title)| (*status, *title, filtered_tasks_by_status(*status)))
        .filter(|(_, _, group)| !group.is_empty())
        .collect();

    rsx! {
        AppLayout {
        PageHeader {
            eyebrow: "TASKS".to_string(),
            title: "任务管理".to_string(),
            actions: Some(rsx! {
                button {
                    class: if matches!(view_mode(), ViewMode::List) { "btn hud-btn btn-outline active" } else { "btn hud-btn btn-outline" },
                    onclick: move |_| view_mode.set(ViewMode::List),
                    "列表视图"
                }
                button {
                    class: if matches!(view_mode(), ViewMode::Board) { "btn hud-btn btn-outline active" } else { "btn hud-btn btn-outline" },
                    onclick: move |_| view_mode.set(ViewMode::Board),
                    "看板视图"
                }
            }),
        }

        // 统计概览
        HudPanel {
            title: "任务概览".to_string(),
            eyebrow: "OVERVIEW".to_string(),
            StatGrid {
                StatReadout { label: "任务总数".to_string(), value: format!("{}", total) }
                StatReadout { label: "进行中".to_string(), value: format!("{}", in_progress), accent: Some("primary".to_string()) }
                StatReadout { label: "待处理".to_string(), value: format!("{}", pending), accent: Some("warning".to_string()) }
                StatReadout { label: "已完成".to_string(), value: format!("{}", completed), accent: Some("success".to_string()) }
            }
        }

        // 筛选栏
        HudPanel {
            title: "筛选条件".to_string(),
            eyebrow: "FILTERS".to_string(),
            div { class: "flex flex-wrap gap-4 items-end",
                div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                    label { class: "form-label", "项目" }
                    select {
                        class: "input input-bordered w-full",
                        value: "{filter_project_id}",
                        onchange: move |e| {
                            filter_project_id.set(e.value().clone());
                            load_data();
                        },
                        option { value: "", "全部项目" }
                        for p in projects_list.iter() {
                            option { value: "{p.id}", "{p.name}" }
                        }
                    }
                }
                div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                    label { class: "form-label", "状态" }
                    select {
                        class: "input input-bordered w-full",
                        value: "{filter_status}",
                        onchange: move |e| {
                            if let Ok(v) = e.value().parse::<i32>() {
                                filter_status.set(v);
                            }
                            load_data();
                        },
                        option { value: "-1", "全部状态" }
                        option { value: "1", "待审核" }
                        option { value: "2", "待处理" }
                        option { value: "3", "进行中" }
                        option { value: "4", "已完成" }
                        option { value: "5", "已归档" }
                    }
                }
                div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                    label { class: "form-label", "负责人类型" }
                    select {
                        class: "input input-bordered w-full",
                        value: "{filter_assignee_type}",
                        onchange: move |e| {
                            if let Ok(v) = e.value().parse::<i32>() {
                                filter_assignee_type.set(v);
                            }
                            load_data();
                        },
                        option { value: "-1", "全部" }
                        option { value: "0", "用户" }
                        option { value: "1", "Agent" }
                    }
                }
                div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                    label { class: "form-label", "搜索" }
                    input {
                        class: "input input-bordered w-full",
                        placeholder: "搜索任务...",
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

        // 视图内容
        if loading() {
            HudPanel {
                Loading {} },
        } else if tasks_list.is_empty() {
            HudPanel {
                EmptyState { icon: "📋".to_string(),
                message: "暂无任务".to_string() } },
        } else if matches!(view_mode(), ViewMode::List) {
            // 列表视图
            HudPanel {
                title: "任务列表".to_string(),
                eyebrow: "TASKS".to_string(),
                table { class: "table hud-table table-zebra",
                    thead { tr {
                        th { "标题" }
                        th { "状态" }
                        th { "优先级" }
                        th { "进度" }
                        th { "负责人" }
                        th { "项目" }
                        th { "更新时间" }
                    }}
                    tbody {
                        for t in tasks_list.iter() {
                            {
                                let tid = t.id.clone();
                                let t_title = t.title.clone();
                                let t_status = t.status;
                                let t_priority = t.priority;
                                let t_progress = t.progress;
                                let t_assignee_type = t.assignee_type;
                                let t_assignee_id = t.assignee_id.clone();
                                let t_project_id = t.project_id.clone();
                                let t_updated_at = t.updated_at;
                                rsx! {
                                    tr {
                                        key: "{tid}",
                                        class: "table-row-clickable",
                                        onclick: move |_| {
                                            let _ = navigator.push(format!("/tasks/{}", tid));
                                        },
                                        td { "data-label": "标题", "{t_title}" }
                                        td { "data-label": "状态", span { class: "{task_status_badge(t_status)}", "{task_status_text(t_status)}" } }
                                        td { "data-label": "优先级", "{t_priority}" }
                                        td { "data-label": "进度",
                                            HudProgress { value: t_progress, tone: Some("primary".to_string()), show_value: Some(true) }
                                        }
                                        td { "data-label": "负责人",
                                            {
                                                let assignee_type_text = if t_assignee_type == 0 { "用户" } else { "Agent" };
                                                rsx! { "{assignee_type_text}: {t_assignee_id}" }
                                            }
                                        }
                                        td { "data-label": "项目",
                                            if let Some(pid) = &t_project_id {
                                                span { class: "font-mono", "{pid}" }
                                            } else {
                                                span { class: "text-base-content/70", "无" }
                                            }
                                        }
                                        td { "data-label": "更新时间", span { class: "font-mono text-base-content/70", "{format_time(t_updated_at)}" } }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            // 看板视图 - HUD 风格 KanbanCanvas
            {
                let columns: Vec<KanbanColumn> = board_columns.iter().map(|(status, title, group_tasks)| {
                    let color = match status {
                        1 => "#6b7280".to_string(), // 待审核 - 灰
                        2 => "#3b82f6".to_string(), // 待处理 - 蓝
                        3 => "#f59e0b".to_string(), // 进行中 - 黄
                        4 => "#10b981".to_string(), // 已完成 - 绿
                        5 => "#4b5563".to_string(), // 已归档 - 深灰
                        _ => "#fa520f".to_string(),
                    };
                    let tasks: Vec<KanbanTask> = group_tasks.iter().map(|t| KanbanTask {
                        id: t.id.clone(),
                        title: t.title.clone(),
                        progress: t.progress,
                        priority: t.priority,
                        tags: t.tags.clone(),
                    }).collect();
                    KanbanColumn {
                        status: *status,
                        title: title.to_string(),
                        color,
                        tasks,
                    }
                }).collect();
                rsx! {
                    KanbanCanvas {
                        columns,
                        width: 900.0,
                        height: 500.0,
                        on_task_click: None,
                    }
                }
            }
        }
        }
    }
}
