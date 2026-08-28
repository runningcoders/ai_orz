//! 后台任务管理页面
//!
//! 功能：
//! - 列表展示所有后台任务（按 started_at 降序）
//! - 按类型/状态筛选 + 客户端分页（每页 20 条）
//! - 使用 use_future 实现 3 秒自动轮询（卸载自动取消，筛选变化自动重启）
//! - 点击任务行弹窗查看详情（result/error）
//! - 清理已完成任务

use crate::api::background_task::{cleanup_tasks, list_tasks};
use crate::components::state::Loading;
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{ListBackgroundTasksRequest, TaskProgressSnapshot, TaskStatus};
use dioxus::prelude::*;

const PAGE_SIZE: usize = 20;
const POLL_INTERVAL_MS: u32 = 3000;

/// 后台任务管理页面
#[component]
pub fn SystemTasks() -> Element {
    let toast = use_toast();
    let mut tasks = use_signal(Vec::<TaskProgressSnapshot>::new);
    let mut loading = use_signal(|| true);
    let mut filter_type = use_signal(|| None::<String>);
    let mut filter_status = use_signal(|| None::<TaskStatus>);
    let mut current_page = use_signal(|| 0usize);
    let mut detail_task = use_signal(|| None::<TaskProgressSnapshot>);
    let mut show_cleanup_confirm = use_signal(|| false);

    // use_future：初始加载 + 持续轮询
    // 读取 filter_type/filter_status 会建立依赖，筛选变化时自动重启 future
    // 组件卸载时自动取消，不会产生多个并行循环
    use_future(move || async move {
        let req = ListBackgroundTasksRequest {
            task_type: filter_type(),
            status: filter_status(),
        };
        // 立即加载一次
        match list_tasks(&req).await {
            Ok(resp) => {
                tasks.set(resp.tasks);
                loading.set(false);
            }
            Err(e) => {
                toast.error(format!("加载任务列表失败: {}", e));
                loading.set(false);
            }
        }
        // 持续轮询
        loop {
            gloo_timers::future::TimeoutFuture::new(POLL_INTERVAL_MS).await;
            let req = ListBackgroundTasksRequest {
                task_type: filter_type(),
                status: filter_status(),
            };
            if let Ok(resp) = list_tasks(&req).await {
                tasks.set(resp.tasks);
            }
        }
    });

    // 客户端分页
    let total = tasks().len();
    let total_pages = total.div_ceil(PAGE_SIZE);
    let page = current_page().min(total_pages.saturating_sub(1));
    let start = page * PAGE_SIZE;
    let end = (start + PAGE_SIZE).min(total);
    let page_tasks: Vec<TaskProgressSnapshot> = tasks()[start..end].to_vec();

    // 统计
    let running_count = tasks()
        .iter()
        .filter(|t| t.status == TaskStatus::Running)
        .count();
    let completed_count = tasks()
        .iter()
        .filter(|t| t.status == TaskStatus::Completed)
        .count();
    let failed_count = tasks()
        .iter()
        .filter(|t| t.status == TaskStatus::Failed)
        .count();

    // 清理已完成任务
    let on_cleanup = move |_| {
        spawn(async move {
            match cleanup_tasks(Some(10)).await {
                Ok(resp) => {
                    toast.success(format!("已清理 {} 个任务", resp.cleaned));
                    show_cleanup_confirm.set(false);
                    // 立即刷新列表
                    let req = ListBackgroundTasksRequest {
                        task_type: filter_type(),
                        status: filter_status(),
                    };
                    if let Ok(resp) = list_tasks(&req).await {
                        tasks.set(resp.tasks);
                    }
                }
                Err(e) => toast.error(format!("清理失败: {}", e)),
            }
        });
    };

    rsx! {
        AppLayout {
            div { class: "card bg-base-100 shadow-md",
                div { class: "card-body",
                    // 标题栏
                    div { class: "flex items-center justify-between",
                        h2 { class: "card-title", "后台任务管理" }
                        div { class: "flex gap-2",
                            button {
                                class: "btn btn-warning btn-sm",
                                onclick: move |_| show_cleanup_confirm.set(true),
                                "清理已完成"
                            }
                        }
                    }

                    // 统计卡片
                    div { class: "stats stats-horizontal shadow mt-4",
                        div { class: "stat",
                            div { class: "stat-title", "运行中" }
                            div { class: "stat-value text-primary", "{running_count}" }
                        }
                        div { class: "stat",
                            div { class: "stat-title", "已完成" }
                            div { class: "stat-value text-success", "{completed_count}" }
                        }
                        div { class: "stat",
                            div { class: "stat-title", "已失败" }
                            div { class: "stat-value text-error", "{failed_count}" }
                        }
                    }

                    // 筛选栏
                    div { class: "flex gap-4 mt-4",
                        select {
                            class: "select select-bordered select-sm",
                            onchange: move |e| {
                                let val = e.value();
                                filter_type.set(if val.is_empty() { None } else { Some(val) });
                                current_page.set(0);
                            },
                            option { value: "", "全部类型" }
                            option { value: "initialize_system", "系统初始化" }
                            option { value: "rebuild_vectors", "向量重建" }
                            option { value: "seed_save", "Seed 导出" }
                            option { value: "seed_load", "Seed 导入" }
                            option { value: "seed_apply_default", "应用默认 Seed" }
                        }
                        select {
                            class: "select select-bordered select-sm",
                            onchange: move |e| {
                                let val = e.value();
                                filter_status.set(match val.as_str() {
                                    "pending" => Some(TaskStatus::Pending),
                                    "running" => Some(TaskStatus::Running),
                                    "completed" => Some(TaskStatus::Completed),
                                    "failed" => Some(TaskStatus::Failed),
                                    _ => None,
                                });
                                current_page.set(0);
                            },
                            option { value: "", "全部状态" }
                            option { value: "pending", "等待中" }
                            option { value: "running", "运行中" }
                            option { value: "completed", "已完成" }
                            option { value: "failed", "已失败" }
                        }
                    }

                    // 任务列表
                    if loading() {
                        div { class: "flex justify-center py-8",
                            Loading { size: "lg" }
                        }
                    } else if page_tasks.is_empty() {
                        div { class: "text-center py-8 text-base-content/50",
                            "暂无后台任务"
                        }
                    } else {
                        div { class: "overflow-x-auto mt-4",
                            table { class: "table table-zebra",
                                thead {
                                    tr {
                                        th { "类型" }
                                        th { "状态" }
                                        th { "进度" }
                                        th { "开始时间" }
                                        th { "耗时" }
                                    }
                                }
                                tbody {
                                    for t in page_tasks.iter() {
                                        {
                                            let task_id = t.task_id.clone();
                                            let task_id_click = task_id.clone();
                                            let status_class = match t.status {
                                                TaskStatus::Pending => "badge badge-ghost",
                                                TaskStatus::Running => "badge badge-primary",
                                                TaskStatus::Completed => "badge badge-success",
                                                TaskStatus::Failed => "badge badge-error",
                                            };
                                            let status_label = match t.status {
                                                TaskStatus::Pending => "等待中",
                                                TaskStatus::Running => "运行中",
                                                TaskStatus::Completed => "已完成",
                                                TaskStatus::Failed => "已失败",
                                            };
                                            let type_label = match t.task_type.as_str() {
                                                "initialize_system" => "系统初始化",
                                                "rebuild_vectors" => "向量重建",
                                                "seed_save" => "Seed 导出",
                                                "seed_load" => "Seed 导入",
                                                "seed_apply_default" => "应用默认 Seed",
                                                _ => t.task_type.as_str(),
                                            };
                                            let started_time = chrono::DateTime::from_timestamp_millis(t.started_at)
                                                .map(|dt| dt.format("%m-%d %H:%M:%S").to_string())
                                                .unwrap_or_else(|| "-".to_string());
                                            let duration = if let Some(finished) = t.finished_at {
                                                finished - t.started_at
                                            } else {
                                                chrono::Utc::now().timestamp_millis() - t.started_at
                                            };
                                            let duration_str = if duration < 1000 {
                                                format!("{}ms", duration)
                                            } else if duration < 60_000 {
                                                format!("{:.1}s", duration as f64 / 1000.0)
                                            } else {
                                                format!("{:.1}m", duration as f64 / 60_000.0)
                                            };

                                            rsx! {
                                                tr {
                                                    class: "cursor-pointer hover",
                                                    key: "{task_id}",
                                                    onclick: move |_| {
                                                        if let Some(t) = tasks().iter().find(|t| t.task_id == task_id_click) {
                                                            detail_task.set(Some(t.clone()));
                                                        }
                                                    },
                                                    td { {type_label} }
                                                    td {
                                                        span { class: "{status_class}", {status_label} }
                                                    }
                                                    td {
                                                        if t.total_steps > 0 {
                                                            "{t.current_step}/{t.total_steps}"
                                                        } else {
                                                            "-"
                                                        }
                                                        br {}
                                                        small { class: "text-base-content/60",
                                                            "{t.step_message}"
                                                        }
                                                    }
                                                    td { {started_time} }
                                                    td { {duration_str} }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // 分页
                        if total_pages > 1 {
                            div { class: "flex justify-center mt-4",
                                div { class: "join",
                                    button {
                                        class: "join-item btn btn-sm",
                                        disabled: page == 0,
                                        onclick: move |_| current_page.set(page.saturating_sub(1)),
                                        "«"
                                    }
                                    button { class: "join-item btn btn-sm", "第 {page + 1} 页 / {total_pages}" }
                                    button {
                                        class: "join-item btn btn-sm",
                                        disabled: page + 1 >= total_pages,
                                        onclick: move |_| current_page.set(page + 1),
                                        "»"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 详情弹窗
            if let Some(detail) = detail_task() {
                div {
                    class: "modal modal-open",
                        onclick: move |_| detail_task.set(None),
                        div {
                            class: "modal-box",
                            onclick: |e| e.stop_propagation(),
                            h3 { class: "font-bold text-lg", "任务详情" }
                            div { class: "py-4 space-y-2",
                                div { class: "flex gap-2",
                                    span { class: "font-semibold", "任务 ID:" }
                                    span { class: "font-mono text-sm", "{detail.task_id}" }
                                }
                                div { class: "flex gap-2",
                                    span { class: "font-semibold", "类型:" }
                                    span { "{detail.task_type}" }
                                }
                                div { class: "flex gap-2",
                                    span { class: "font-semibold", "状态:" }
                                    span { "{detail.status:?}" }
                                }
                                div { class: "flex gap-2",
                                    span { class: "font-semibold", "步骤:" }
                                    span { "{detail.current_step} / {detail.total_steps}" }
                                }
                                div { class: "flex gap-2",
                                    span { class: "font-semibold", "当前描述:" }
                                    span { "{detail.step_message}" }
                                }
                                if let Some(err) = &detail.error {
                                    div { class: "alert alert-error",
                                        span { class: "font-semibold", "错误信息:" }
                                        span { "{err}" }
                                    }
                                }
                                if let Some(result) = &detail.result {
                                    div {
                                        span { class: "font-semibold", "结果:" }
                                        pre { class: "bg-base-200 p-2 rounded mt-1 text-xs overflow-x-auto",
                                            {serde_json::to_string_pretty(result).unwrap_or_default()}
                                        }
                                    }
                                }
                            }
                            div { class: "modal-action",
                                button {
                                    class: "btn",
                                    onclick: move |_| detail_task.set(None),
                                    "关闭"
                                }
                            }
                        }
                    }
            }

            // 清理确认弹窗
            if show_cleanup_confirm() {
                div {
                    class: "modal modal-open",
                        onclick: move |_| show_cleanup_confirm.set(false),
                        div {
                            class: "modal-box",
                            onclick: |e| e.stop_propagation(),
                            h3 { class: "font-bold text-lg", "确认清理" }
                            p { class: "py-4", "将清理已完成的旧任务，每个类型保留最近 10 个。运行中的任务不受影响。" }
                            div { class: "modal-action",
                                button {
                                    class: "btn btn-ghost",
                                    onclick: move |_| show_cleanup_confirm.set(false),
                                    "取消"
                                }
                                button {
                                    class: "btn btn-warning",
                                    onclick: move |_| on_cleanup(()),
                                    "确认清理"
                                }
                            }
                        }
                    }
            }
        }
    }
}
