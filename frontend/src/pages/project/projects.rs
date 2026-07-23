//! 项目列表

use dioxus::prelude::*;
use std::collections::HashMap;

use crate::api::project::{create_project, list_project_tasks, list_projects};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use crate::utils::{project_status_badge as status_badge, project_status_text as status_text};
use common::api::{CreateProjectRequest, ListProjectsResponseItem};

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
                owner_agent_id: None,
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
        AppLayout {
        div { class: "card bg-base-100 shadow-md",
            div { class: "card-body",
                div { class: "flex justify-between items-center",
                    h2 { class: "card-title", "项目管理" }
                    button { class: "btn btn-primary", onclick: move |_| show_modal.set(true), "+ 创建项目" }
                }

                if loading() {
                    Loading {}
                } else if projects_list.is_empty() {
                    EmptyState { icon: "📁".to_string(), message: "暂无项目".to_string() }
                } else {
                    div { class: "overflow-x-auto",
                        table { class: "table table-zebra",
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
                                                td { "data-label": "项目名称",
                                                    Link { to: crate::pages::Route::ProjectDetail { id: id.clone() },
                                                        class: "link link-primary",
                                                        "{pname}"
                                                    }
                                                }
                                                td { "data-label": "状态", span { class: "{status_badge(pstatus)}", "{status_text(pstatus)}" } }
                                                td { class: "text-base-content/70", "data-label": "任务数",
                                                    if let Some(count) = task_counts.read().get(&id) {
                                                        "{count}"
                                                    } else {
                                                        "-"
                                                    }
                                                }
                                                td { class: "font-mono text-base-content/70", "data-label": "创建时间", "{pcreated}" }
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
                button { class: "btn btn-primary", disabled: creating(), onclick: handle_create,
                    if creating() { "创建中..." } else { "创建" }
                }
            },
            div { class: "space-y-4",
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "项目名称 *" }
                    }
                    input { class: "input input-bordered w-full", value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()), placeholder: "请输入项目名称" }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "描述" }
                    }
                    textarea { class: "textarea textarea-bordered w-full", value: "{new_description}",
                        oninput: move |e| new_description.set(e.value()), placeholder: "项目描述（可选）" }
                }
            }
        }
        }
    }
}
