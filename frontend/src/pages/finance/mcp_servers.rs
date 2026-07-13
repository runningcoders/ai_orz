//! MCP 服务器管理

use dioxus::prelude::*;

use crate::api::finance::{
    create_mcp_server, delete_mcp_server, list_mcp_servers, sync_mcp_tools,
    update_mcp_server_status,
};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, ErrorAlert, Loading, SuccessAlert};
use common::api::{CreateMcpServerRequest, McpServerConfigDto, McpServerListItem};
use common::enums::{McpServerStatus, McpTransport};

fn format_timestamp(ts: i64) -> String {
    let seconds = ts / 1000;
    let day = seconds / 86400;
    let hour = (seconds % 86400) / 3600;
    let minute = (seconds % 3600) / 60;
    let second = seconds % 60;
    format!("{} {:02}:{:02}:{:02}", day, hour, minute, second)
}

#[component]
pub fn FinanceMcpServers() -> Element {
    let mut servers = use_signal(Vec::<McpServerListItem>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut success = use_signal(String::new);
    let mut show_add_modal = use_signal(|| false);

    // 创建表单状态
    let mut new_name = use_signal(String::new);
    let mut new_transport = use_signal(|| "0".to_string());
    let mut new_config_value = use_signal(String::new);
    let mut creating = use_signal(|| false);

    use_effect(move || {
        loading.set(true);
        spawn(async move {
            match list_mcp_servers().await {
                Ok(list) => servers.set(list.servers),
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    });

    let handle_create = move |_| {
        spawn(async move {
            if new_name().is_empty() {
                error.set("服务器名称不能为空".to_string());
                return;
            }
            creating.set(true);
            let transport = match new_transport().parse::<i32>().unwrap_or(0) {
                1 => McpTransport::StreamableHttp,
                _ => McpTransport::Stdio,
            };
            let config = if transport == McpTransport::StreamableHttp {
                McpServerConfigDto {
                    url: if new_config_value().is_empty() {
                        None
                    } else {
                        Some(new_config_value())
                    },
                    ..Default::default()
                }
            } else {
                McpServerConfigDto {
                    command: if new_config_value().is_empty() {
                        None
                    } else {
                        Some(new_config_value())
                    },
                    ..Default::default()
                }
            };
            let req = CreateMcpServerRequest {
                name: new_name(),
                transport,
                config,
            };
            match create_mcp_server(req).await {
                Ok(_) => {
                    show_add_modal.set(false);
                    new_name.set(String::new());
                    new_transport.set("0".to_string());
                    new_config_value.set(String::new());
                    success.set("创建成功".to_string());
                    match list_mcp_servers().await {
                        Ok(list) => servers.set(list.servers),
                        Err(e) => error.set(e),
                    }
                }
                Err(e) => error.set(format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    let servers_list = servers.read().clone();
    let new_transport_value = new_transport();
    let config_label = if new_transport_value == "1" {
        "URL"
    } else {
        "命令"
    };
    let config_placeholder = if new_transport_value == "1" {
        "https://..."
    } else {
        "npx -y @modelcontextprotocol/server-filesystem /tmp"
    };

    rsx! {
        div { class: "card",
            ErrorAlert { message: error() }
            SuccessAlert { message: success() }
            div { class: "card-header",
                h2 { class: "card-title", "MCP 服务器管理" }
                button { class: "btn btn-accent", onclick: move |_| show_add_modal.set(true), "+ 添加服务器" }
            }
            if loading() {
                Loading {}
            } else if servers_list.is_empty() {
                EmptyState { icon: "🖥️".to_string(), message: "暂无 MCP 服务器".to_string() }
            } else {
                table { class: "table",
                    thead { tr {
                        th { "名称" }
                        th { "传输方式" }
                        th { "配置" }
                        th { "状态" }
                        th { "创建时间" }
                        th { "操作" }
                    }}
                    tbody {
                        for s in servers_list.iter() {
                            {
                                let id = s.id.clone();
                                let name = s.name.clone();
                                let transport = s.transport;
                                let status = s.status;
                                let config = s.config.clone();
                                let created_at = s.created_at;
                                let is_enabled = status == McpServerStatus::Enabled;
                                let id_disable = id.clone();
                                let id_enable = id.clone();
                                let id_delete = id.clone();
                                let id_sync = id.clone();
                                let config_display = if transport == McpTransport::StreamableHttp {
                                    config.url.unwrap_or_default()
                                } else {
                                    config.command.unwrap_or_default()
                                };
                                rsx! {
                                    tr { key: "{id}",
                                        td { style: "font-weight: 500;", "{name}" }
                                        td { span { class: "badge badge-info", "{transport}" } }
                                        td { span { class: "text-sm text-muted", "{config_display}" } }
                                        td {
                                            if is_enabled {
                                                span { class: "badge badge-success", "启用" }
                                            } else {
                                                span { class: "badge badge-error", "禁用" }
                                            }
                                        }
                                        td { span { class: "text-sm text-muted", "{format_timestamp(created_at)}" } }
                                        td {
                                            if is_enabled {
                                                button { class: "btn btn-ghost btn-sm",
                                                    onclick: move |_| {
                                                        let id_disable = id_disable.clone();
                                                        spawn(async move {
                                                            if let Err(e) = update_mcp_server_status(&id_disable, 2).await {
                                                                error.set(e);
                                                            } else {
                                                                match list_mcp_servers().await {
                                                                    Ok(list) => servers.set(list.servers),
                                                                    Err(e) => error.set(e),
                                                                }
                                                            }
                                                        });
                                                    }, "禁用"
                                                }
                                            } else {
                                                button { class: "btn btn-ghost btn-sm",
                                                    onclick: move |_| {
                                                        let id_enable = id_enable.clone();
                                                        spawn(async move {
                                                            if let Err(e) = update_mcp_server_status(&id_enable, 1).await {
                                                                error.set(e);
                                                            } else {
                                                                match list_mcp_servers().await {
                                                                    Ok(list) => servers.set(list.servers),
                                                                    Err(e) => error.set(e),
                                                                }
                                                            }
                                                        });
                                                    }, "启用"
                                                }
                                            }
                                            button { class: "btn btn-primary btn-sm",
                                                onclick: move |_| {
                                                    let id_sync = id_sync.clone();
                                                    spawn(async move {
                                                        if let Err(e) = sync_mcp_tools(&id_sync).await {
                                                            error.set(format!("同步失败: {}", e));
                                                        } else {
                                                            success.set("工具同步成功".to_string());
                                                            match list_mcp_servers().await {
                                                                Ok(list) => servers.set(list.servers),
                                                                Err(e) => error.set(e),
                                                            }
                                                        }
                                                    });
                                                }, "同步工具"
                                            }
                                            button { class: "btn btn-danger btn-sm",
                                                onclick: move |_| {
                                                    let id_delete = id_delete.clone();
                                                    spawn(async move {
                                                        if let Err(e) = delete_mcp_server(&id_delete).await {
                                                            error.set(format!("删除失败: {}", e));
                                                        } else {
                                                            match list_mcp_servers().await {
                                                                Ok(list) => servers.set(list.servers),
                                                                Err(e) => error.set(e),
                                                            }
                                                        }
                                                    });
                                                }, "删除"
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
            title: "添加 MCP 服务器".to_string(),
            show: show_add_modal(),
            on_close: move |_| show_add_modal.set(false),
            footer: rsx! {
                button { class: "btn btn-ghost", onclick: move |_| show_add_modal.set(false), "取消" }
                button { class: "btn btn-accent", disabled: creating(), onclick: handle_create,
                    if creating() { "创建中..." } else { "创建" }
                }
            },
            div {
                div { class: "form-group",
                    label { class: "form-label", "服务器名称 *" }
                    input { class: "form-input", value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()), placeholder: "如：filesystem-server" }
                }
                div { class: "form-group",
                    label { class: "form-label", "传输方式" }
                    select { class: "form-select", value: "{new_transport_value}",
                        onchange: move |e| new_transport.set(e.value()),
                        option { value: "0", "Stdio" }
                        option { value: "1", "StreamableHttp" }
                    }
                }
                div { class: "form-group",
                    label { class: "form-label", "{config_label}" }
                    input { class: "form-input", value: "{new_config_value}",
                        oninput: move |e| new_config_value.set(e.value()),
                        placeholder: "{config_placeholder}" }
                }
            }
        }
    }
}
