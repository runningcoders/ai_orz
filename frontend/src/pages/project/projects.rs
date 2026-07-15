//! 项目列表

use dioxus::prelude::*;
use std::collections::HashMap;

use crate::api::project::{create_project, list_project_tasks, list_projects};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::store::toast::use_toast;
use common::api::{CreateProjectRequest, ListProjectsResponseItem};

fn status_badge(status: i32) -> &'static str {
    match status {
        0 => "badge badge-error",       // 已删除
        1 => "badge badge-info",        // 活跃
        2 => "badge badge-warning",     // 待审核
        3 => "badge badge-primary",     // 进行中
        4 => "badge badge-success",     // 已完成
        5 => "badge badge-neutral",     // 已归档
        _ => "badge badge-neutral",
    }
}

fn status_text(status: i32) -> &'static str {
    match status {
        0 => "已删除",
        1 => "活跃",
        2 => "待审核",
        3 => "进行中",
        4 => "已完成",
        5 => "已归档",
        _ => "未知",
    }
}

#[component]
pub fn ProjectList() -> Element {
    let mut projects = use_signal(Vec::<ListProjectsResponseItem>::new);
    let mut task_counts = use_signal(HashMap::<String, usize>::new);
    let mut loading = use_signal(|| true);
    let mut show_modal = use_signal(|| false);
    let mut new_name = use_signal(String::new);
    let mut new_description = use_signal(String::new);
    let mut creating = use_signal(|| false);
    let toast = use_toast();

    use_effect(move || {
        loading.set(true);
        spawn(async move {
            match list_projects().await {
                Ok(list) => {
                    let items = list.projects.clone();
                    projects.set(list.projects);
                    let mut counts = HashMap::new();
                    for p in &items {
                        if let Ok(tasks_resp) = list_project_tasks(&p.id).await {
                            counts.insert(p.id.clone(), tasks_resp.tasks.len());
                        }
                    }
                    task_counts.set(counts);
                }
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    });

    let handle_create = move |_| {
        spawn(async move {
            if new_name().is_empty() {
                toast.error("项目名称不能为空");
                return;
            }
            creating.set(true);
            let req = CreateProjectRequest {
                name: new_name(),
                description: if new_description().is_empty() { None } else { Some(new_description()) },
                priority: None,
                tags: None,
            };
            match create_project(req).await {
                Ok(_) => {
                    show_modal.set(false);
                    new_name.set(String::new());
                    new_description.set(String::new());
                    // Reload
                    match list_projects().await {
                        Ok(list) => projects.set(list.projects),
                        Err(e) => toast.error(&e),
                    }
                }
                Err(e) => toast.error(&format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    let projects_list = projects.read().clone();

    rsx! {
        div { class: "card",
            div { class: "card-header",
                h2 { class: "card-title", "项目管理" }
                button { class: "btn btn-accent", onclick: move |_| show_modal.set(true), "+ 创建项目" }
            }

            if loading() {
                Loading {}
            } else if projects_list.is_empty() {
                EmptyState { icon: "📁".to_string(), message: "暂无项目".to_string() }
            } else {
                table { class: "table",
                    thead { tr {
                        th { "项目名称" }
                        th { "状态" }
                        th { "任务数" }
                        th { "创建时间" }
                    }}
                    tbody {
                        for p in projects_list.iter() {
                            {
                                let id = p.id.clone();
                                let pname = p.name.clone();
                                let pstatus = p.status;
                                let pcreated = p.created_at.clone();
                                rsx! {
                                    tr { key: "{id}",
                                        td {
                                            Link { to: crate::pages::Route::ProjectDetail { id: id.clone() },
                                                class: "detail-back-link",
                                                "{pname}"
                                            }
                                        }
                                        td { span { class: "{status_badge(pstatus)}", "{status_text(pstatus)}" } }
                                        td { class: "text-secondary",
                                            if let Some(count) = task_counts.read().get(&id) {
                                                "{count}"
                                            } else {
                                                "-"
                                            }
                                        }
                                        td { class: "text-mono text-muted", "{pcreated}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Modal {
            title: "创建项目".to_string(),
            show: show_modal(),
            on_close: move |_| {
                show_modal.set(false);
                new_name.set(String::new());
                new_description.set(String::new());
            },
            footer: rsx! {
                button { class: "btn btn-ghost", onclick: move |_| show_modal.set(false), "取消" }
                button { class: "btn btn-accent", disabled: creating(), onclick: handle_create,
                    if creating() { "创建中..." } else { "创建" }
                }
            },
            div {
                div { class: "form-group",
                    label { class: "form-label", "项目名称 *" }
                    input { class: "form-input", value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()), placeholder: "请输入项目名称" }
                }
                div { class: "form-group",
                    label { class: "form-label", "描述" }
                    textarea { class: "form-textarea", value: "{new_description}",
                        oninput: move |e| new_description.set(e.value()), placeholder: "项目描述（可选）" }
                }
            }
        }
    }
}
