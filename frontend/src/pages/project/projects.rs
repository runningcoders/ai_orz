//! 项目列表

use dioxus::prelude::*;
use dioxus_router::prelude::*;

use crate::api::project::{create_project, list_projects};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, ErrorAlert, Loading};
use common::api::{CreateProjectRequest, ListProjectsResponseItem};

fn status_badge(status: i32) -> &'static str {
    match status {
        0 => "badge badge-error",
        1 => "badge badge-info",
        2 => "badge badge-success",
        3 => "badge badge-neutral",
        _ => "badge badge-neutral",
    }
}

fn status_text(status: i32) -> &'static str {
    match status {
        0 => "已归档",
        1 => "进行中",
        2 => "已完成",
        3 => "已暂停",
        _ => "未知",
    }
}

#[component]
pub fn ProjectList() -> Element {
    let mut projects = use_signal(Vec::<ListProjectsResponseItem>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut show_modal = use_signal(|| false);
    let mut new_name = use_signal(String::new);
    let mut new_description = use_signal(String::new);
    let mut creating = use_signal(|| false);

    let load = move || {
        loading.set(true);
        spawn(async move {
            match list_projects().await {
                Ok(list) => projects.set(list.projects),
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    };

    use_effect(move || { load(); });

    let handle_create = move |_| {
        spawn(async move {
            if new_name().is_empty() {
                error.set("项目名称不能为空".to_string());
                return;
            }
            creating.set(true);
            let req = CreateProjectRequest {
                name: new_name(),
                description: if new_description().is_empty() { None } else { Some(new_description()) },
                assignee_id: None,
                assignee_type: None,
            };
            match create_project(req).await {
                Ok(_) => {
                    show_modal.set(false);
                    new_name.set(String::new());
                    new_description.set(String::new());
                    load();
                }
                Err(e) => error.set(format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    let projects_list = projects.read().clone();

    rsx! {
        div { class: "card",
            ErrorAlert { message: error() }

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
                                rsx! {
                                    tr { key: "{id}",
                                        td {
                                            Link { to: crate::pages::Route::ProjectDetail { id: id.clone() },
                                                style: "color: var(--color-mistral-orange); text-decoration: none; font-weight: 500;",
                                                "{p.name}"
                                            }
                                        }
                                        td { span { class: "{status_badge(p.status)}", "{status_text(p.status)}" } }
                                        td { class: "text-secondary", "{p.task_count.unwrap_or(0)}" }
                                        td { class: "text-mono text-muted", "{p.created_at}" }
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
            },
            footer: rsx! {
                button { class: "btn btn-ghost", onclick: move |_| show_modal.set(false), "取消" }
                button { class: "btn btn-accent", disabled: creating(), onclick: handle_create,
                    if creating() { "创建中..." } else { "创建" }
                }
            }
        }
    }
}
