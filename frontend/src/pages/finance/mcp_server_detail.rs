//! MCP Server 详情页 - 展示详情 + 同步工具 + 启用/禁用 + 删除

use crate::api::finance::{
    delete_mcp_server, get_mcp_server, sync_mcp_tools, update_mcp_server_status,
};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{GetMcpServerResponse, UpdateMcpServerStatusRequest};
use common::enums::{McpServerStatus, McpTransport};
use dioxus::prelude::*;
use dioxus_router::{Link, use_navigator};

#[component]
pub fn FinanceMcpServerDetail(id: String) -> Element {
    let toast = use_toast();
    let navigator = use_navigator();

    let mut server = use_signal(|| Option::<GetMcpServerResponse>::None);
    let mut loading = use_signal(|| true);
    let mut syncing = use_signal(|| false);
    let mut toggling = use_signal(|| false);
    let mut show_delete_confirm = use_signal(|| false);

    let id_for_effect = id.clone();
    use_effect(move || {
        loading.set(true);
        let id = id_for_effect.clone();
        spawn(async move {
            match get_mcp_server(&id).await {
                Ok(s) => server.set(Some(s)),
                Err(e) => toast.error(&format!("加载失败: {}", e)),
            }
            loading.set(false);
        });
    });

    let on_sync = {
        let id = id.clone();
        move |_| {
            let id = id.clone();
            let id_reload = id.clone();
            syncing.set(true);
            spawn(async move {
                match sync_mcp_tools(&id).await {
                    Ok(_) => toast.success("工具同步已触发"),
                    Err(e) => toast.error(&format!("同步失败: {}", e)),
                }
                syncing.set(false);
                match get_mcp_server(&id_reload).await {
                    Ok(s) => server.set(Some(s)),
                    Err(e) => toast.error(&format!("刷新失败: {}", e)),
                }
            });
        }
    };

    let mut on_toggle = {
        let id = id.clone();
        move |new_status: McpServerStatus| {
            let id = id.clone();
            let id_reload = id.clone();
            toggling.set(true);
            spawn(async move {
                match update_mcp_server_status(UpdateMcpServerStatusRequest {
                    id,
                    status: new_status,
                })
                .await
                {
                    Ok(_) => {
                        toast.success(if new_status == McpServerStatus::Enabled {
                            "已启用"
                        } else {
                            "已禁用"
                        });
                    }
                    Err(e) => toast.error(&format!("状态更新失败: {}", e)),
                }
                toggling.set(false);
                match get_mcp_server(&id_reload).await {
                    Ok(s) => server.set(Some(s)),
                    Err(e) => toast.error(&format!("刷新失败: {}", e)),
                }
            });
        }
    };

    let on_delete = {
        let id = id.clone();
        move |_| {
            let id = id.clone();
            show_delete_confirm.set(false);
            spawn(async move {
                match delete_mcp_server(&id).await {
                    Ok(_) => {
                        toast.success("已删除");
                        let _ = navigator.push("/finance/mcp-servers".to_string());
                    }
                    Err(e) => toast.error(&format!("删除失败: {}", e)),
                }
            });
        }
    };

    let server_data = server.read().clone();

    rsx! {
        AppLayout {
            div { class: "mb-6 flex items-center justify-between",
                h1 { class: "text-2xl font-bold", "MCP Server 详情" }
                Link { class: "btn btn-ghost", to: crate::pages::Route::FinanceMcpServers {}, "← 返回列表" }
            }
            if loading() {
                Loading {}
            } else if let Some(s) = server_data {
                div { class: "card bg-base-100 shadow-md",
                    div { class: "card-body",
                        div { class: "flex justify-between items-center mb-4",
                            h2 { class: "card-title", "{s.name}" }
                            div { class: "flex gap-2",
                                button {
                                    class: "btn btn-ghost btn-sm",
                                    disabled: toggling(),
                                    onclick: move |_| on_toggle(if s.status == McpServerStatus::Enabled { McpServerStatus::Disabled } else { McpServerStatus::Enabled }),
                                    if s.status == McpServerStatus::Enabled { "🚫 禁用" } else { "✅ 启用" }
                                }
                                button {
                                    class: "btn btn-ghost btn-sm",
                                    disabled: syncing(),
                                    onclick: on_sync,
                                    if syncing() { "同步中..." } else { "🔄 同步工具" }
                                }
                                button {
                                    class: "btn btn-error btn-sm",
                                    onclick: move |_| show_delete_confirm.set(true),
                                    "🗑 删除"
                                }
                            }
                        }
                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                            div {
                                div { class: "text-sm text-base-content/60", "传输方式" }
                                div { class: "font-mono", "{transport_text(s.transport)}" }
                            }
                            div {
                                div { class: "text-sm text-base-content/60", "状态" }
                                div { span { class: "badge", "{status_text(s.status)}" } }
                            }
                            div { class: "md:col-span-2",
                                div { class: "text-sm text-base-content/60 mb-1", "配置" }
                                pre {
                                    class: "font-mono text-sm bg-base-200 p-3 rounded overflow-auto",
                                    style: "white-space: pre-wrap; word-break: break-word;",
                                    "{config_display(&s)}"
                                }
                            }
                            div {
                                div { class: "text-sm text-base-content/60", "创建时间" }
                                div { class: "font-mono", "{crate::utils::format_datetime(s.created_at)}" }
                            }
                            div {
                                div { class: "text-sm text-base-content/60", "更新时间" }
                                div { class: "font-mono", "{crate::utils::format_datetime(s.updated_at)}" }
                            }
                        }
                    }
                }
            } else {
                EmptyState { icon: "❓".to_string(), message: "MCP Server 不存在或已被删除".to_string() }
            }

            ConfirmDialog {
                show: show_delete_confirm(),
                title: "确认删除".to_string(),
                message: "确定删除此 MCP Server？关联工具也会被清理。".to_string(),
                on_confirm: on_delete,
                on_cancel: move |_| show_delete_confirm.set(false),
            }
        }
    }
}

fn transport_text(t: McpTransport) -> &'static str {
    match t {
        McpTransport::Stdio => "Stdio",
        McpTransport::StreamableHttp => "Streamable HTTP",
    }
}

fn status_text(s: McpServerStatus) -> &'static str {
    match s {
        McpServerStatus::Enabled => "启用",
        McpServerStatus::Disabled => "禁用",
        McpServerStatus::Deleted => "已删除",
    }
}

fn config_display(s: &GetMcpServerResponse) -> String {
    let mut parts = Vec::new();
    if let Some(cmd) = &s.config.command {
        parts.push(format!("command: {}", cmd));
    }
    if !s.config.args.is_empty() {
        parts.push(format!("args: {}", s.config.args.join(" ")));
    }
    if let Some(url) = &s.config.url {
        parts.push(format!("url: {}", url));
    }
    if parts.is_empty() {
        "(无配置)".to_string()
    } else {
        parts.join("\n")
    }
}
