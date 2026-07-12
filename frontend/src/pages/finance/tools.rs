//! 工具管理

use dioxus::prelude::*;

use crate::api::finance::{delete_tool, list_tools, update_tool_status};
use crate::components::state::{EmptyState, ErrorAlert, Loading};
use common::api::ListToolsResponseItem;
use common::enums::ToolStatus;

#[component]
pub fn FinanceTools() -> Element {
    let mut tools = use_signal(Vec::<ListToolsResponseItem>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);

    use_effect(move || {
        loading.set(true);
        spawn(async move {
            match list_tools().await {
                Ok(list) => tools.set(list.tools),
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    });

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
                                let name = t.name.clone();
                                let protocol = t.protocol;
                                let status = t.status;
                                let is_enabled = status == ToolStatus::Enabled;
                                let id_disable = id.clone();
                                let id_enable = id.clone();
                                let id_delete = id.clone();
                                rsx! {
                                    tr { key: "{id}",
                                        td { style: "font-weight: 500;", "{name}" }
                                        td { span { class: "badge badge-neutral", "{protocol}" } }
                                        td {
                                            if is_enabled {
                                                span { class: "badge badge-success", "启用" }
                                            } else {
                                                span { class: "badge badge-error", "禁用" }
                                            }
                                        }
                                        td {
                                            if is_enabled {
                                                button { class: "btn btn-ghost btn-sm",
                                                    onclick: move |_| {
                                                        let id_disable = id_disable.clone();
                                                        spawn(async move {
                                                            if let Err(e) = update_tool_status(&id_disable, 0).await {
                                                                error.set(e);
                                                            } else {
                                                                match list_tools().await {
                                                                    Ok(list) => tools.set(list.tools),
                                                                    Err(e) => error.set(e),
                                                                }
                                                            }
                                                        });
                                                    },
                                                    "禁用"
                                                }
                                            } else {
                                                button { class: "btn btn-ghost btn-sm",
                                                    onclick: move |_| {
                                                        let id_enable = id_enable.clone();
                                                        spawn(async move {
                                                            if let Err(e) = update_tool_status(&id_enable, 1).await {
                                                                error.set(e);
                                                            } else {
                                                                match list_tools().await {
                                                                    Ok(list) => tools.set(list.tools),
                                                                    Err(e) => error.set(e),
                                                                }
                                                            }
                                                        });
                                                    },
                                                    "启用"
                                                }
                                            }
                                            button { class: "btn btn-danger btn-sm",
                                                onclick: move |_| {
                                                    let id_delete = id_delete.clone();
                                                    spawn(async move {
                                                        if let Err(e) = delete_tool(&id_delete).await {
                                                            error.set(format!("删除失败: {}", e));
                                                        } else {
                                                            match list_tools().await {
                                                                Ok(list) => tools.set(list.tools),
                                                                Err(e) => error.set(e),
                                                            }
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
