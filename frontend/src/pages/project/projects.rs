//! 项目列表

use dioxus::prelude::*;
use std::collections::HashMap;

use crate::api::project::{
    create_project, list_project_tasks, list_projects, query_projects, search_projects,
};
use crate::components::hud::{HudPanel, PageHeader};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use crate::utils::{project_status_badge as status_badge, project_status_text as status_text};
use common::api::{
    CreateProjectRequest, ListProjectsRequest, ListProjectsResponseItem, ProjectQueryRequest,
    SearchProjectsRequest,
};

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

    let mut search_keyword = use_signal(String::new);
    let mut search_request_id = use_signal(|| 0u32);
    let mut status_filter = use_signal(|| Option::<i32>::None);

    let reload_projects = move || {
        spawn(async move {
            loading.set(true);
            // 信号在 async 内读取，避免 use_effect 订阅导致每次按键重复触发。
            // 三场景切换：
            // 无关键词 + 无状态筛选 → list_projects
            // 无关键词 + 有状态筛选 → query_projects
            // 有关键词 → search_projects（可同时带状态筛选）
            let keyword = search_keyword();
            let status = status_filter();
            let my_id = search_request_id() + 1;
            search_request_id.set(my_id);
            let result = if keyword.trim().is_empty() && status.is_none() {
                list_projects(ListProjectsRequest::default())
                    .await
                    .map(|p| p.items)
            } else if keyword.trim().is_empty() {
                // 有状态筛选，无关键词 → query_projects
                query_projects(&ProjectQueryRequest {
                    status_in: status.map(|s| vec![common::enums::ProjectStatus::from(s)]),
                    ..Default::default()
                })
                .await
                .map(|p| p.items)
            } else {
                // 有关键词 → search_projects（可同时带状态筛选）
                search_projects(&SearchProjectsRequest {
                    keyword: Some(keyword),
                    status_in: status.map(|s| vec![common::enums::ProjectStatus::from(s)]),
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
                Ok(v) => {
                    let items = v.clone();
                    projects.set(v);
                    // 重新加载 task_counts
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
    };

    use_effect(move || {
        reload_projects();
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
                description: if new_description().is_empty() {
                    None
                } else {
                    Some(new_description())
                },
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
                    reload_projects();
                }
                Err(e) => toast.error(format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    let projects_list = projects.read().clone();

    rsx! {
        AppLayout {
        PageHeader {
            eyebrow: "PROJECT".to_string(),
            title: "项目管理".to_string(),
            actions: Some(rsx! {
                button { class: "btn btn-primary", onclick: move |_| show_modal.set(true), "+ 创建项目" }
            }),
        }

        // 筛选栏（独立卡片）
        HudPanel {
            div { class: "card-body",
                div { class: "flex flex-wrap gap-4 items-end",
                    div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                        label { class: "form-label", "状态" }
                        select {
                            class: "select select-bordered w-full",
                            value: "{status_filter().map(|s| s.to_string()).unwrap_or_default()}",
                            onchange: move |e| {
                                let val = e.value();
                                status_filter.set(if val.is_empty() {
                                    None
                                } else {
                                    val.parse::<i32>().ok()
                                });
                                reload_projects();
                            },
                            option { value: "", "全部状态" }
                            option { value: "1", "Active" }
                            option { value: "2", "PendingReview" }
                            option { value: "3", "InProgress" }
                            option { value: "4", "Completed" }
                            option { value: "5", "Archived" }
                        }
                    }
                    div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                        label { class: "form-label", "搜索" }
                        input {
                            class: "input input-bordered w-full",
                            placeholder: "搜索项目...",
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
                                    reload_projects();
                                });
                            }
                        }
                    }
                    // 清除搜索按钮
                    if !search_keyword().is_empty() || status_filter().is_some() {
                        button {
                            class: "btn btn-ghost btn-sm",
                            onclick: move |_| {
                                search_keyword.set(String::new());
                                status_filter.set(None);
                                reload_projects();
                            },
                            "✕"
                        }
                    }
                }
            }
        }

        // 列表卡片
        HudPanel {
            div { class: "card-body",
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
                                        let pcreated = p.created_at;
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
