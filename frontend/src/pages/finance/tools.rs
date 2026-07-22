//! 工具管理

use dioxus::prelude::*;

use crate::api::finance::{delete_tool, list_tools, update_tool_status};
use crate::components::state::{EmptyState, Loading};
use crate::store::toast::use_toast;
use common::api::ListToolsResponseItem;
use common::enums::ToolStatus;
use dioxus_router::Link;

#[component]
pub fn FinanceTools() -> Element {
    let mut tools = use_signal(Vec::<ListToolsResponseItem>::new);
    let mut loading = use_signal(|| true);
    let toast = use_toast();

    use_effect(move || {
        loading.set(true);
        spawn(async move {
            match list_tools().await {
                Ok(list) => tools.set(list.tools),
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    });

    let tools_list = tools.read().clone();

    rsx! {
        div { class: "card bg-base-100 shadow-md",
            div { class: "card-body",
                div { class: "flex justify-between items-center mb-4",
                    h2 { class: "card-title", "工具管理" }
                }

                if loading() {
                    Loading {}
                } else if tools_list.is_empty() {
                    EmptyState { icon: "🔧".to_string(), message: "暂无工具".to_string() }
                } else {
                    div { class: "overflow-x-auto",
                        table { class: "table table-zebra table-pin-rows",
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
                                        let id_detail = id.clone();
                                        rsx! {
                                            tr { key: "{id}",
                                                td { class: "font-semibold",
                                                    Link { to: crate::pages::Route::FinanceToolDetail { id: id_detail.clone() }, "{name}" }
                                                }
                                                td { span { class: "badge badge-neutral", "{protocol}" } }
                                                td {
                                                    if is_enabled {
                                                        span { class: "badge badge-success", "启用" }
                                                    } else {
                                                        span { class: "badge badge-error", "禁用" }
                                                    }
                                                }
                                                td { class: "flex gap-2 items-center",
                                                    if is_enabled {
                                                        button { class: "btn btn-ghost btn-sm",
                                                            onclick: move |_| {
                                                                let id_disable = id_disable.clone();
                                                                spawn(async move {
                                                                    if let Err(e) = update_tool_status(&id_disable, 0).await {
                                                                        toast.error(&e);
                                                                    } else {
                                                                        match list_tools().await {
                                                                            Ok(list) => tools.set(list.tools),
                                                                            Err(e) => toast.error(&e),
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
                                                                        toast.error(&e);
                                                                    } else {
                                                                        match list_tools().await {
                                                                            Ok(list) => tools.set(list.tools),
                                                                            Err(e) => toast.error(&e),
                                                                        }
                                                                    }
                                                                });
                                                            },
                                                            "启用"
                                                        }
                                                    }
                                                    button { class: "btn btn-error btn-sm",
                                                        onclick: move |_| {
                                                            let id_delete = id_delete.clone();
                                                            spawn(async move {
                                                                if let Err(e) = delete_tool(&id_delete).await {
                                                                    toast.error(&format!("删除失败: {}", e));
                                                                } else {
                                                                    match list_tools().await {
                                                                        Ok(list) => tools.set(list.tools),
                                                                        Err(e) => toast.error(&e),
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
    }
}
