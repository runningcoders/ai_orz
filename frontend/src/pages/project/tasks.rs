//! 任务管理页面 - 列表视图 + 看板视图

use dioxus::prelude::*;
use dioxus_router::use_navigator;

use crate::api::project::{list_projects, list_tasks};
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use crate::utils::{format_datetime as format_time, progress_bar_class, task_status_badge, task_status_text};
use common::api::{ListProjectsResponseItem, TaskListItem};

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

    let toast = use_toast();
    let navigator = use_navigator();

    // 加载数据
    let mut load_data = move || {
        loading.set(true);
        let pid = filter_project_id();
        let status = if filter_status() >= 0 { Some(filter_status()) } else { None };
        let at = if filter_assignee_type() >= 0 { Some(filter_assignee_type()) } else { None };
        spawn(async move {
            match list_tasks(
                if pid.is_empty() { None } else { Some(&pid) },
                status,
                None,
                at,
            ).await {
                Ok(resp) => tasks.set(resp.tasks),
                Err(e) => toast.error(&e),
            }
            match list_projects().await {
                Ok(list) => projects.set(list.projects),
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
    let pending = tasks_list.iter().filter(|t| t.status == 2 || t.status == 1).count();

    // 看板数据分组
    let board_groups = [
        (1, "待审核"),
        (2, "待处理"),
        (3, "进行中"),
        (4, "已完成"),
        (5, "已归档"),
    ];

    let filtered_tasks_by_status = |status: i32| {
        tasks_list.iter().filter(|t| t.status == status).collect::<Vec<_>>()
    };

    let board_columns: Vec<(i32, &str, Vec<&TaskListItem>)> = board_groups
        .iter()
        .map(|(status, title)| (*status, *title, filtered_tasks_by_status(*status)))
        .filter(|(_, _, group)| !group.is_empty())
        .collect();

    rsx! {
        AppLayout {
        div { class: "page-header",
            h1 { class: "page-title", "任务管理" }
            div { class: "page-header-actions",
                button {
                    class: if matches!(view_mode(), ViewMode::List) { "btn btn-outline active" } else { "btn btn-outline" },
                    onclick: move |_| view_mode.set(ViewMode::List),
                    "列表视图"
                }
                button {
                    class: if matches!(view_mode(), ViewMode::Board) { "btn btn-outline active" } else { "btn btn-outline" },
                    onclick: move |_| view_mode.set(ViewMode::Board),
                    "看板视图"
                }
            }
        }

        // 统计概览
        div { class: "card bg-base-100 shadow-md",
            div { class: "card-header",
                h2 { class: "card-title", "任务概览" }
            }
            div { class: "overview-grid",
                div { class: "overview-item",
                    div { class: "overview-label", "任务总数" }
                    div { class: "overview-stat-value", "{total}" }
                }
                div { class: "overview-item",
                    div { class: "overview-label", "进行中" }
                    div { class: "overview-stat-value primary", "{in_progress}" }
                }
                div { class: "overview-item",
                    div { class: "overview-label", "待处理" }
                    div { class: "overview-stat-value warning", "{pending}" }
                }
                div { class: "overview-item",
                    div { class: "overview-label", "已完成" }
                    div { class: "overview-stat-value success", "{completed}" }
                }
            }
        }

        // 筛选栏
        div { class: "card bg-base-100 shadow-md",
            div { class: "card-header",
                h2 { class: "card-title", "筛选条件" }
            }
            div { class: "filter-row",
                div { class: "filter-item",
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
                div { class: "filter-item",
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
                div { class: "filter-item",
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
            }
        }

        // 视图内容
        if loading() {
            div { class: "card bg-base-100 shadow-md", Loading {} }
        } else if tasks_list.is_empty() {
            div { class: "card bg-base-100 shadow-md", EmptyState { icon: "📋".to_string(), message: "暂无任务".to_string() } }
        } else if matches!(view_mode(), ViewMode::List) {
            // 列表视图
            div { class: "card bg-base-100 shadow-md",
                div { class: "card-header",
                    h2 { class: "card-title", "任务列表" }
                }
                table { class: "table table-zebra",
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
                                            div { class: "progress-cell",
                                                div { class: "progress-bar",
                                                    div { class: "progress-bar-fill", style: "width: {t_progress}%;" }
                                                }
                                                span { class: "text-base-content/70 font-mono progress-text", "{t_progress}%" }
                                            }
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
            // 看板视图
            div { class: "kanban-board",
                for (status, title, group_tasks) in board_columns.iter() {
                    div { class: "kanban-column",
                        div { class: "kanban-column-header",
                            span { class: "{task_status_badge(*status)}", "{title}" }
                            span { class: "kanban-column-count", "{group_tasks.len()}" }
                        }
                        div { class: "kanban-column-content",
                            for t in group_tasks.iter() {
                                {
                                    let tid = t.id.clone();
                                    let t_title = t.title.clone();
                                    let t_progress = t.progress;
                                    let t_priority = t.priority;
                                    let t_tags = t.tags.clone();
                                    rsx! {
                                        div {
                                            key: "{tid}",
                                            class: "kanban-card",
                                            onclick: move |_| {
                                            let _ = navigator.push(format!("/tasks/{}", tid));
                                        },
                                            div { class: "kanban-card-header",
                                                h3 { class: "kanban-card-title", "{t_title}" }
                                                div { class: "kanban-card-meta",
                                                    if t_priority > 0 {
                                                        span { class: "badge badge-warning", "优先级 {t_priority}" }
                                                    }
                                                }
                                            }
                                            if !t_tags.is_empty() {
                                                div { class: "kanban-card-tags",
                                                    for tag in t_tags.iter() {
                                                        span { class: "badge badge-neutral tag-item", "{tag}" }
                                                    }
                                                }
                                            }
                                            div { class: "kanban-card-progress",
                                                div { class: "progress-bar",
                                                    div { class: "{progress_bar_class(t_progress)}", style: "width: {t_progress}%;" }
                                                }
                                                span { class: "text-base-content/70 font-mono", "{t_progress}%" }
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
}
