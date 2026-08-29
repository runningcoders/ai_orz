//! MCP Server 详情页 - 展示详情 + 同步工具 + 启用/禁用 + 删除

use crate::api::finance::{
    delete_mcp_server, get_mcp_server, sync_mcp_tools, update_mcp_server_status,
};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::credential_requirements::CredentialRequirementsTable;
use crate::components::hud::{HudPanel, PageHeader};
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

    let mut syncing = use_signal(|| false);
    let mut toggling = use_signal(|| false);
    let mut show_delete_confirm = use_signal(|| false);

    // M1 修复（方案 B）：响应式 id + use_resource，拉取仅在 :id 变化时触发，
    // 切换本地状态（如弹窗）不再误重拉
    let route = dioxus_router::use_route::<crate::pages::Route>();
    let mut id = use_signal(String::new);
    if let crate::pages::Route::FinanceMcpServerDetail { id: rid } = &route
        && *id.peek() != *rid
    {
        id.set(rid.clone());
    }
    let mut server_res = use_resource(move || {
        let id = id();
        async move { get_mcp_server(&id).await }
    });

    let on_sync = {
        let id = id();
        move |_| {
            let id = id.clone();
            let id_reload = id.clone();
            syncing.set(true);
            spawn(async move {
                match sync_mcp_tools(&id).await {
                    Ok(_) => toast.success("工具同步已触发"),
                    Err(e) => toast.error(format!("同步失败: {}", e)),
                }
                syncing.set(false);
                match get_mcp_server(&id_reload).await {
                    Ok(s) => server_res.set(Some(Ok(s))),
                    Err(e) => toast.error(format!("刷新失败: {}", e)),
                }
            });
        }
    };

    let mut on_toggle = {
        let id = id();
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
                    Err(e) => toast.error(format!("状态更新失败: {}", e)),
                }
                toggling.set(false);
                match get_mcp_server(&id_reload).await {
                    Ok(s) => server_res.set(Some(Ok(s))),
                    Err(e) => toast.error(format!("刷新失败: {}", e)),
                }
            });
        }
    };

    let on_delete = {
        let id = id();
        move |_| {
            let id = id.clone();
            show_delete_confirm.set(false);
            spawn(async move {
                match delete_mcp_server(&id).await {
                    Ok(_) => {
                        toast.success("已删除");
                        let _ = navigator.push("/finance/mcp-servers".to_string());
                    }
                    Err(e) => toast.error(format!("删除失败: {}", e)),
                }
            });
        }
    };

    let server_view = server_res.read();

    rsx! {
        AppLayout {
            PageHeader {
                eyebrow: "FINANCE".to_string(),
                title: "MCP Server 详情".to_string(),
                actions: Some(rsx! {
                    Link { class: "btn hud-btn btn-ghost", to: crate::pages::Route::FinanceMcpServers {}, "← 返回列表" }
                }),
            }
            match server_view.as_ref() {
                None => rsx! { Loading {} },
                Some(Ok(s)) => {
                    let s = s.clone();
                    rsx! {
                        HudPanel {
                            title: "{s.name}".to_string(),
                            eyebrow: "MCP SERVER".to_string(),
                            signal: true,
                            div { class: "card-body",
                                div { class: "flex justify-end mb-4",
                                    div { class: "flex gap-2",
                                        button {
                                            class: "btn hud-btn btn-ghost btn-sm",
                                            disabled: toggling(),
                                            onclick: move |_| on_toggle(if s.status == McpServerStatus::Enabled { McpServerStatus::Disabled } else { McpServerStatus::Enabled }),
                                            if s.status == McpServerStatus::Enabled { "🚫 禁用" } else { "✅ 启用" }
                                        }
                                        button {
                                            class: "btn hud-btn btn-ghost btn-sm",
                                            disabled: syncing(),
                                            onclick: on_sync,
                                            if syncing() { "同步中..." } else { "🔄 同步工具" }
                                        }
                                        button {
                                            class: "btn hud-btn btn-error btn-sm",
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
                        // ===== 凭据需求只读卡片（空列表不渲染）=====
                        if !s.config.credential_requirements.is_empty() {
                            HudPanel {
                                title: "凭据需求".to_string(),
                                eyebrow: "CREDENTIALS".to_string(),
                                div { class: "card-body",
                                    p { class: "text-sm text-base-content/60",
                                        "工具以调用者身份注入以下凭据（类型级声明，不绑定具体凭据实例）" }
                                    CredentialRequirementsTable { requirements: s.config.credential_requirements.clone() }
                                }
                            }
                        }
                    }
                },
                Some(Err(e)) => rsx! {
                    EmptyState { icon: "❓".to_string(), message: format!("加载失败: {}", e) }
                },
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
