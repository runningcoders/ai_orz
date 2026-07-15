//! 任务详情页 - 基本信息、状态流转、进度更新、操作按钮

use dioxus::prelude::*;
use dioxus_router::use_navigator;

use crate::api::{project::*, StatsOptions};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::components::stats::TaskStatsPanel;
use crate::store::toast::use_toast;
use common::api::GetTaskResponse;

fn status_badge(status: i32) -> &'static str {
    match status {
        0 => "badge badge-error",       // 已取消
        1 => "badge badge-warning",     // 待审核
        2 => "badge badge-info",        // 待处理
        3 => "badge badge-primary",     // 进行中
        4 => "badge badge-success",     // 已完成
        5 => "badge badge-neutral",     // 已归档
        _ => "badge badge-neutral",
    }
}

fn status_text(status: i32) -> &'static str {
    match status {
        0 => "已取消",
        1 => "待审核",
        2 => "待处理",
        3 => "进行中",
        4 => "已完成",
        5 => "已归档",
        _ => "未知",
    }
}

fn format_timestamp(ts: Option<i64>) -> String {
    use chrono::{Local, TimeZone};
    ts.map(|t| {
        Local
            .timestamp_opt(t / 1000, 0)
            .single()
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|| t.to_string())
    })
    .unwrap_or_else(|| "—".to_string())
}

fn progress_bar_class(progress: i32) -> &'static str {
    match progress {
        0..=25 => "overview-progress-fill warning",
        26..=50 => "overview-progress-fill primary",
        51..=75 => "overview-progress-fill accent",
        76..=100 => "overview-progress-fill success",
        _ => "overview-progress-fill",
    }
}

#[component]
pub fn TaskDetail(id: String) -> Element {
    let mut task = use_signal(|| None::<GetTaskResponse>);
    let mut loading = use_signal(|| true);

    // 进度更新弹窗
    let mut show_progress_modal = use_signal(|| false);
    let mut new_progress = use_signal(|| 0i32);
    let mut updating_progress = use_signal(|| false);

    let toast = use_toast();
    let navigator = use_navigator();

    // 初始加载
    let id_for_load = id.clone();
    use_effect(move || {
        loading.set(true);
        let id_clone = id_for_load.clone();
        spawn(async move {
            let stats_options = StatsOptions {
                with_stats: true,
                with_model_call_stats: true,
                stats_interval: Some("daily".to_string()),
            };
            match get_task(&id_clone, Some(&stats_options)).await {
                Ok(t) => {
                    new_progress.set(t.progress);
                    task.set(Some(t));
                }
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    });

    // 状态切换 - 内联每个按钮的 closure 避免 move 问题
    // 状态 1: 送审
    let id_for_review = id.clone();
    let on_review = move |_| {
        let id_clone = id_for_review.clone();
        spawn(async move {
            match update_task_status(&id_clone, 1).await {
                Ok(_) => {
                    toast.success("任务状态已更新");
                    let stats_options = StatsOptions {
                        with_stats: true,
                        with_model_call_stats: true,
                        stats_interval: Some("daily".to_string()),
                    };
                    if let Ok(t) = get_task(&id_clone, Some(&stats_options)).await {
                        new_progress.set(t.progress);
                        task.set(Some(t));
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
            match update_task_status(&id_clone, 2).await {
                Ok(_) => {
                    toast.success("任务状态已更新");
                    let stats_options = StatsOptions {
                        with_stats: true,
                        with_model_call_stats: true,
                        stats_interval: Some("daily".to_string()),
                    };
                    if let Ok(t) = get_task(&id_clone, Some(&stats_options)).await {
                        new_progress.set(t.progress);
                        task.set(Some(t));
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
            match update_task_status(&id_clone, 3).await {
                Ok(_) => {
                    toast.success("任务状态已更新");
                    let stats_options = StatsOptions {
                        with_stats: true,
                        with_model_call_stats: true,
                        stats_interval: Some("daily".to_string()),
                    };
                    if let Ok(t) = get_task(&id_clone, Some(&stats_options)).await {
                        new_progress.set(t.progress);
                        task.set(Some(t));
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
            match update_task_status(&id_clone, 4).await {
                Ok(_) => {
                    toast.success("任务状态已更新");
                    let stats_options = StatsOptions {
                        with_stats: true,
                        with_model_call_stats: true,
                        stats_interval: Some("daily".to_string()),
                    };
                    if let Ok(t) = get_task(&id_clone, Some(&stats_options)).await {
                        new_progress.set(t.progress);
                        task.set(Some(t));
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
            match update_task_status(&id_clone, 0).await {
                Ok(_) => {
                    toast.success("任务状态已更新");
                    let stats_options = StatsOptions {
                        with_stats: true,
                        with_model_call_stats: true,
                        stats_interval: Some("daily".to_string()),
                    };
                    if let Ok(t) = get_task(&id_clone, Some(&stats_options)).await {
                        new_progress.set(t.progress);
                        task.set(Some(t));
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
            match update_task_status(&id_clone, 5).await {
                Ok(_) => {
                    toast.success("任务状态已更新");
                    let stats_options = StatsOptions {
                        with_stats: true,
                        with_model_call_stats: true,
                        stats_interval: Some("daily".to_string()),
                    };
                    if let Ok(t) = get_task(&id_clone, Some(&stats_options)).await {
                        new_progress.set(t.progress);
                        task.set(Some(t));
                    }
                }
                Err(e) => toast.error(&e),
            }
        });
    };

    // 打开进度弹窗
    let open_progress_modal = move |_| {
        if let Some(t) = task.read().as_ref() {
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
            match update_task_progress(&id_clone, progress_val).await {
                Ok(t) => {
                    toast.success("进度已更新");
                    task.set(Some(t));
                    show_progress_modal.set(false);
                }
                Err(e) => toast.error(&e),
            }
            updating_progress.set(false);
        });
    };

    // 返回项目（如有关联）
    let back_to_project = move |_| {
        if let Some(t) = task.read().as_ref() {
            if let Some(pid) = &t.project_id {
                navigator.push(format!("/projects/{}", pid));
            } else {
                navigator.push("/projects".to_string());
            }
        } else {
            navigator.push("/projects".to_string());
        }
    };

    rsx! {
        div { class: "page-header",
            button {
                class: "btn btn-secondary btn-sm",
                onclick: back_to_project,
                "← 返回项目"
            }
        }
        if loading() {
            div { class: "card", Loading {} }
        } else if let Some(t) = task.read().as_ref() {
            // 区域 1：基本信息
            div { class: "card",
                div { class: "card-header",
                    h2 { class: "card-title", "{t.title}" }
                    span { class: "{status_badge(t.status)}", "{status_text(t.status)}" }
                }
                div { class: "detail-grid",
                    div {
                        label { class: "form-label", "描述" }
                        if let Some(desc) = &t.description {
                            if desc.is_empty() {
                                span { class: "text-muted", "暂无描述" }
                            } else {
                                "{desc}"
                            }
                        } else {
                            span { class: "text-muted", "暂无描述" }
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
                        span { class: "text-mono", "{t.root_user_id}" }
                    }
                    if let Some(pid) = &t.project_id {
                        div {
                            label { class: "form-label", "所属项目" }
                            span { class: "text-mono", "{pid}" }
                        }
                    }
                    div {
                        label { class: "form-label", "创建者" }
                        span { class: "text-mono", "{t.created_by}" }
                    }
                    div {
                        label { class: "form-label", "创建时间" }
                        span { class: "text-mono text-muted", "{format_timestamp(Some(t.created_at))}" }
                    }
                    div {
                        label { class: "form-label", "更新时间" }
                        span { class: "text-mono text-muted", "{format_timestamp(Some(t.updated_at))}" }
                    }
                    if let Some(due) = t.due_at {
                        div {
                            label { class: "form-label", "截止时间" }
                            span { class: "text-mono", "{format_timestamp(Some(due))}" }
                        }
                    }
                    if let Some(start) = t.start_at {
                        div {
                            label { class: "form-label", "开始时间" }
                            span { class: "text-mono", "{format_timestamp(Some(start))}" }
                        }
                    }
                    if let Some(end) = t.end_at {
                        div {
                            label { class: "form-label", "结束时间" }
                            span { class: "text-mono", "{format_timestamp(Some(end))}" }
                        }
                    }
                }
            }

            // 区域 2：标签和依赖
            if !t.tags.is_empty() || !t.dependencies.is_empty() {
                div { class: "card",
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
                                        li { class: "text-mono", "{dep}" }
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

            // 区域 3：进度管理
            div { class: "card",
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
            div { class: "card",
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
                                class: "btn btn-accent",
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
                                class: "btn btn-secondary",
                                onclick: on_archive,
                                "归档"
                            }
                        }
                    }
                }
            }

            // 进度更新弹窗
            Modal {
                title: "更新进度".to_string(),
                show: show_progress_modal(),
                on_close: move |_| show_progress_modal.set(false),
                footer: Some(rsx! {
                    div { class: "modal-footer-actions",
                        button {
                            class: "btn btn-secondary",
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
                            class: "form-input",
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
        } else {
            div { class: "card", EmptyState { icon: "❓".to_string(), message: "任务不存在".to_string() } }
        }
    }
}
