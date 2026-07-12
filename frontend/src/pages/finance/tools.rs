//! 工具管理

use dioxus::prelude::*;

use crate::api::finance::{delete_tool, list_tools, update_tool_status};
use crate::components::state::{EmptyState, ErrorAlert, Loading};
use common::api::ListToolsResponseItem;

#[component]
pub fn FinanceTools() -> Element {
    let mut tools = use_signal(Vec::<ListToolsResponseItem>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);

    let load = move || {
        loading.set(true);
        spawn(async move {
            match list_tools().await {
                Ok(list) => tools.set(list.tools),
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    };

    use_effect(move || { load(); });

    let tools_list = tools.read().clone();

    rsx! {
        div { class: "card",
            ErrorAlert { message: error() }

            div { class: "card-header",
                h2 { class: "card-title", "工具管理" }
            }

            if loading() {
                Loading {}
            } else if tools_list.is_empty() {
                EmptyState { icon: "🔧".to_string(), message: "暂无工具".to_string() }
            } else {
                table { class: "table",
                    thead { tr {
                        th { "名称" }
                        th { "协议" }
                        th { "状态" }
                        th { "操作" }
                    }}
                    tbody {
                        for t in tools_list.iter() {
                            {
                                let id = t.id.clone();
                                let status = t.status;
                                rsx! {
                                    tr { key: "{id}",
                                        td { style: "font-weight: 500;", "{t.name}" }
                                        td { span { class: "badge badge-neutral", "{t.protocol}" } }
                                        td {
                                            if status == 1 {
                                                span { class: "badge badge-success", "启用" }
                                            } else {
                                                span { class: "badge badge-error", "禁用" }
                                            }
                                        }
                                        td {
                                            if status == 1 {
                                                button { class: "btn btn-ghost btn-sm",
                                                    onclick: move |_| {
                                                        let id = id.clone();
                                                        spawn(async move {
                                                            if let Err(e) = update_tool_status(&id, 0).await {
                                                                error.set(e);
                                                            } else { load(); }
                                                        });
                                                    },
                                                    "禁用"
                                                }
                                            } else {
                                                button { class: "btn btn-ghost btn-sm",
                                                    onclick: move |_| {
                                                        let id = id.clone();
                                                        spawn(async move {
                                                            if let Err(e) = update_tool_status(&id, 1).await {
                                                                error.set(e);
                                                            } else { load(); }
                                                        });
                                                    },
                                                    "启用"
                                                }
                                            }
                                            button { class: "btn btn-danger btn-sm",
                                                onclick: move |_| {
                                                    let id = id.clone();
                                                    spawn(async move {
                                                        if let Err(e) = delete_tool(&id).await {
                                                            error.set(format!("删除失败: {}", e));
                                                        } else { load(); }
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
