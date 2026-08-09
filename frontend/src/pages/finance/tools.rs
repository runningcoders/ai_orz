//! 工具管理

use dioxus::prelude::*;

use crate::api::finance::{delete_tool, list_tools, query_tools, search_tools, update_tool_status};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::create_http_tool::CreateHttpToolModal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{
    ListToolsRequest, ListToolsResponseItem, SearchToolsRequest, ToolQueryRequest,
    UpdateToolStatusRequest,
};
use common::enums::{ToolProtocol, ToolStatus};
use dioxus_router::Link;

#[component]
pub fn FinanceTools() -> Element {
    let mut tools = use_signal(Vec::<ListToolsResponseItem>::new);
    let mut loading = use_signal(|| true);
    let toast = use_toast();

    // 搜索状态
    let mut search_keyword = use_signal(String::new);
    let mut search_request_id = use_signal(|| 0u32);

    // 过滤条件
    let mut filter_protocol = use_signal(|| -1i32);
    let mut filter_status = use_signal(|| -1i32);

    // ===== 删除确认对话框 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(String::new);

    // ===== 创建 HTTP 工具弹窗 =====
    let mut show_create = use_signal(|| false);

    // 加载数据（三场景切换：list / query / search）
    let load_data = move || {
        spawn(async move {
            loading.set(true);
            let keyword = search_keyword();
            let protocol = filter_protocol();
            let status = filter_status();
            let my_id = search_request_id() + 1;
            search_request_id.set(my_id);

            let has_filter = protocol >= 0 || status >= 0;

            // 三场景切换：
            // 无关键词 + 无过滤 → list_tools
            // 无关键词 + 有过滤 → query_tools
            // 有关键词 → search_tools（可同时带过滤条件）
            let result = if keyword.trim().is_empty() && !has_filter {
                list_tools(ListToolsRequest::default())
                    .await
                    .map(|p| p.items)
            } else if keyword.trim().is_empty() {
                query_tools(&ToolQueryRequest {
                    protocol: if protocol >= 0 {
                        Some(ToolProtocol::from(protocol))
                    } else {
                        None
                    },
                    status: if status >= 0 {
                        Some(ToolStatus::from(status))
                    } else {
                        None
                    },
                    ..Default::default()
                })
                .await
                .map(|p| p.items)
            } else {
                search_tools(&SearchToolsRequest {
                    keyword: Some(keyword),
                    protocol: if protocol >= 0 {
                        Some(ToolProtocol::from(protocol))
                    } else {
                        None
                    },
                    status: if status >= 0 {
                        Some(ToolStatus::from(status))
                    } else {
                        None
                    },
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
                Ok(v) => tools.set(v),
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    };

    // 初始加载
    use_effect(move || {
        load_data();
    });

    let tools_list = tools.read().clone();

    rsx! {
        AppLayout {
            div { class: "flex justify-between items-center mb-4",
                h2 { class: "card-title", "工具管理" }
                button {
                    class: "btn btn-primary btn-sm",
                    onclick: move |_| show_create.set(true),
                    "+ 创建 HTTP 工具"
                }
            }

            // 筛选栏（独立卡片）
            div { class: "card bg-base-100 shadow-md mb-4",
                div { class: "card-body",
                    div { class: "flex flex-wrap gap-4 items-end",
                        div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                            label { class: "form-label", "协议" }
                            select {
                                class: "select select-bordered w-full",
                                value: "{filter_protocol}",
                                onchange: move |e| {
                                    if let Ok(v) = e.value().parse::<i32>() {
                                        filter_protocol.set(v);
                                    }
                                    load_data();
                                },
                                option { value: "-1", "全部" }
                                option { value: "0", "内置" }
                                option { value: "1", "HTTP" }
                                option { value: "2", "MCP" }
                            }
                        }
                        div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                            label { class: "form-label", "状态" }
                            select {
                                class: "select select-bordered w-full",
                                value: "{filter_status}",
                                onchange: move |e| {
                                    if let Ok(v) = e.value().parse::<i32>() {
                                        filter_status.set(v);
                                    }
                                    load_data();
                                },
                                option { value: "-1", "全部" }
                                option { value: "1", "启用" }
                                option { value: "0", "禁用" }
                            }
                        }
                        div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                            label { class: "form-label", "搜索" }
                            input {
                                class: "input input-bordered w-full",
                                placeholder: "搜索工具...",
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
                                        load_data();
                                    });
                                }
                            }
                        }
                    }
                }
            }

            // 列表卡片
            div { class: "card bg-base-100 shadow-md",
                div { class: "card-body",
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
                                                                    if let Err(e) = update_tool_status(UpdateToolStatusRequest { id: id_disable, status: ToolStatus::Disabled }).await {
                                                                        toast.error(&e);
                                                                    } else {
                                                                        load_data();
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
                                                                    if let Err(e) = update_tool_status(UpdateToolStatusRequest { id: id_enable, status: ToolStatus::Enabled }).await {
                                                                        toast.error(&e);
                                                                    } else {
                                                                        load_data();
                                                                    }
                                                                });
                                                            },
                                                            "启用"
                                                        }
                                                    }
                                                    button { class: "btn btn-error btn-sm",
                                                        onclick: move |_| {
                                                            pending_delete_id.set(id_delete.clone());
                                                            show_delete_confirm.set(true);
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

        ConfirmDialog {
            show: show_delete_confirm(),
            title: "确认删除".to_string(),
            message: "确定删除此工具？此操作不可撤销。".to_string(),
            on_confirm: move |_| {
                let id = pending_delete_id();
                show_delete_confirm.set(false);
                spawn(async move {
                    if let Err(e) = delete_tool(&id).await {
                        toast.error(format!("删除失败: {}", e));
                    } else {
                        load_data();
                    }
                });
            },
            on_cancel: move |_| {
                show_delete_confirm.set(false);
            }
        }

        CreateHttpToolModal {
            show: show_create(),
            on_close: move |_| show_create.set(false),
            on_created: move |_| load_data(),
        }
        }
    }
}
