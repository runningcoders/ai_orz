//! MCP 服务器管理

use crate::components::hud::HudPanel;
use crate::components::hud::PageHeader;
use dioxus::prelude::*;
use dioxus_router::Link;

use crate::api::finance::{
    create_mcp_server, delete_mcp_server, list_mcp_servers, sync_mcp_tools,
    update_mcp_server_status,
};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::credential_form::{
    available_enhancers, binding_name, enhancer_display, enhancer_from_value, enhancer_to_value,
    has_any_enhancer_support, injection_value_preview, kind_from_value, mcp_transport_scope,
    normalize_requirements, recommended_binding_name, validate_requirements_scoped,
};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use crate::utils::format_datetime_full as format_timestamp;
use common::api::{
    CreateMcpServerRequest, McpServerConfigDto, McpServerListItem, UpdateMcpServerStatusRequest,
};
use common::enums::{McpServerStatus, McpTransport};
use common::models::{CredentialBinding, CredentialKind, CredentialRequirement, enhancer_supports};

// ==================== MCP 表单本地辅助（共享纯函数见 components/credential_form.rs） ====================

/// 按传输方式构造空名注入点（stdio → Env / streamable_http → Header）
fn empty_binding(transport: McpTransport) -> CredentialBinding {
    match transport {
        McpTransport::Stdio => CredentialBinding::Env {
            name: String::new(),
        },
        McpTransport::StreamableHttp => CredentialBinding::Header {
            name: String::new(),
        },
    }
}

#[component]
pub fn FinanceMcpServers() -> Element {
    let mut servers = use_signal(Vec::<McpServerListItem>::new);
    let mut loading = use_signal(|| true);
    let toast = use_toast();
    let mut show_add_modal = use_signal(|| false);

    let mut new_name = use_signal(String::new);
    let mut new_transport = use_signal(|| "0".to_string());
    let mut new_config_value = use_signal(String::new);
    let mut new_requirements = use_signal(Vec::<CredentialRequirement>::new);
    let mut creating = use_signal(|| false);

    // ===== 删除确认对话框 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(String::new);

    use_effect(move || {
        loading.set(true);
        spawn(async move {
            match list_mcp_servers().await {
                Ok(list) => servers.set(list.servers),
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    });

    let handle_create = move |_| {
        spawn(async move {
            if new_name().is_empty() {
                toast.error("服务器名称不能为空");
                return;
            }
            let transport = match new_transport().parse::<i32>().unwrap_or(0) {
                1 => McpTransport::StreamableHttp,
                _ => McpTransport::Stdio,
            };
            // 凭据需求预校验（规范化后执行，失败 toast 具体错误不提交）
            let requirements = normalize_requirements(new_requirements());
            if let Err(e) =
                validate_requirements_scoped(&requirements, mcp_transport_scope(transport))
            {
                toast.error(&e);
                return;
            }
            creating.set(true);
            let mut config = if transport == McpTransport::StreamableHttp {
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
            config.credential_requirements = requirements;
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
                    new_requirements.set(Vec::new());
                    toast.success("创建成功");
                    match list_mcp_servers().await {
                        Ok(list) => servers.set(list.servers),
                        Err(e) => toast.error(&e),
                    }
                }
                Err(e) => toast.error(format!("创建失败: {}", e)),
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
    let requirements_list = new_requirements();
    let is_http_transport = new_transport_value == "1";
    let binding_name_label = if is_http_transport {
        "注入请求头名（Header）*"
    } else {
        "注入环境变量名（Env）*"
    };
    let binding_name_placeholder = if is_http_transport {
        "如 authorization"
    } else {
        "如 GITHUB_TOKEN"
    };

    let on_add_requirement = move |_| {
        let transport = if new_transport() == "1" {
            McpTransport::StreamableHttp
        } else {
            McpTransport::Stdio
        };
        new_requirements.write().push(CredentialRequirement {
            kind: CredentialKind::GithubToken,
            platform: None,
            field: None,
            enhancer: None,
            binding: empty_binding(transport),
        });
    };

    rsx! {
        AppLayout {
            HudPanel { signal: Some(true),
                div { class: "card-body",
                    PageHeader {
                        eyebrow: Some("FINANCE".to_string()),
                        title: "MCP 服务器管理".to_string(),
                        actions: Some(rsx!{
                        button { class: "btn btn-primary", onclick: move |_| show_add_modal.set(true), "+ 添加服务器" }
                        }),
                    },
                if loading() {
                    Loading {}
                } else if servers_list.is_empty() {
                    EmptyState { icon: "🖥️".to_string(), message: "暂无 MCP 服务器".to_string() }
                } else {
                    div { class: "overflow-x-auto",
                        table { class: "table hud-table table-zebra table-pin-rows",
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
                                                td { class: "font-semibold", "{name}" }
                                                td { span { class: "badge badge-info", "{transport}" } }
                                                td { span { class: "text-sm text-base-content/70 truncate block max-w-xs", "{config_display}" } }
                                                td {
                                                    if is_enabled {
                                                        span { class: "badge badge-success", "启用" }
                                                    } else {
                                                        span { class: "badge badge-error", "禁用" }
                                                    }
                                                }
                                                td { span { class: "text-sm text-base-content/70 whitespace-nowrap", "{format_timestamp(created_at)}" } }
                                                td { class: "flex gap-2 items-center",
                                                    Link {
                                                        class: "btn btn-ghost btn-sm",
                                                        to: crate::pages::Route::FinanceMcpServerDetail { id: id.clone() },
                                                        "详情"
                                                    }
                                                    if is_enabled {
                                                        button { class: "btn btn-ghost btn-sm",
                                                            onclick: move |_| {
                                                                let id_disable = id_disable.clone();
                                                                spawn(async move {
                                                                    if let Err(e) = update_mcp_server_status(UpdateMcpServerStatusRequest { id: id_disable, status: McpServerStatus::Disabled }).await {
                                                                        toast.error(&e);
                                                                    } else {
                                                                        match list_mcp_servers().await {
                                                                            Ok(list) => servers.set(list.servers),
                                                                            Err(e) => toast.error(&e),
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
                                                                    if let Err(e) = update_mcp_server_status(UpdateMcpServerStatusRequest { id: id_enable, status: McpServerStatus::Enabled }).await {
                                                                        toast.error(&e);
                                                                    } else {
                                                                        match list_mcp_servers().await {
                                                                            Ok(list) => servers.set(list.servers),
                                                                            Err(e) => toast.error(&e),
                                                                        }
                                                                    }
                                                                });
                                                            }, "启用"
                                                        }
                                                    }
                                                    button { class: "btn btn-secondary btn-sm",
                                                        onclick: move |_| {
                                                            let id_sync = id_sync.clone();
                                                            spawn(async move {
                                                                if let Err(e) = sync_mcp_tools(&id_sync).await {
                                                                    toast.error(format!("同步失败: {}", e));
                                                                } else {
                                                                    toast.success("工具同步成功");
                                                                    match list_mcp_servers().await {
                                                                        Ok(list) => servers.set(list.servers),
                                                                        Err(e) => toast.error(&e),
                                                                    }
                                                                }
                                                            });
                                                        }, "同步工具"
                                                    }
                                                    button { class: "btn btn-error btn-sm",
                                                        onclick: move |_| {
                                                            pending_delete_id.set(id_delete.clone());
                                                            show_delete_confirm.set(true);
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
            }
        }

        Modal {
            title: "添加 MCP 服务器".to_string(),
            show: show_add_modal(),
            on_close: move |_| show_add_modal.set(false),
            footer: rsx! {
                button { class: "btn btn-ghost", onclick: move |_| show_add_modal.set(false), "取消" }
                button { class: "btn btn-primary", disabled: creating(), onclick: handle_create,
                    if creating() { "创建中..." } else { "创建" }
                }
            },
            div { class: "max-h-[70vh] overflow-y-auto space-y-4 pr-1",
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "服务器名称 *" }
                    }
                    input { class: "input input-bordered w-full", value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()), placeholder: "如：filesystem-server" }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "传输方式" }
                    }
                    select { class: "select select-bordered w-full", value: "{new_transport_value}",
                        onchange: move |e| {
                            new_transport.set(e.value());
                            // 传输方式变更：已有条目注入点类型联动重置（保留注入名）
                            let stdio = e.value() != "1";
                            new_requirements.write().iter_mut().for_each(|r| {
                                let name = binding_name(&r.binding).to_string();
                                r.binding = if stdio {
                                    CredentialBinding::Env { name }
                                } else {
                                    CredentialBinding::Header { name }
                                };
                            });
                        },
                        option { value: "0", "Stdio" }
                        option { value: "1", "StreamableHttp" }
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "{config_label}" }
                    }
                    input { class: "input input-bordered w-full", value: "{new_config_value}",
                        oninput: move |e| new_config_value.set(e.value()),
                        placeholder: "{config_placeholder}" }
                }

                // ===== 凭据需求动态列表 =====
                div { class: "form-control w-full",
                    div { class: "flex justify-between items-center",
                        span { class: "label-text font-medium", "凭据需求（可选）" }
                        button { class: "btn btn-ghost btn-xs", onclick: on_add_requirement, "＋ 添加" }
                    }
                    p { class: "text-xs text-base-content/60 mt-1",
                        "声明该服务器所需凭据类型与注入点；工具调用时以调用者身份自动注入（类型级声明，不绑定具体凭据实例）。" }
                    if requirements_list.is_empty() {
                        p { class: "text-xs text-base-content/40 mt-2", "暂未声明凭据需求" }
                    } else {
                        div { class: "mt-2 space-y-2",
                            for (idx, req) in requirements_list.iter().enumerate() {
                                {
                                    let kind_value = req.kind.as_str();
                                    let platform_value = req.platform.clone().unwrap_or_default();
                                    let field_value = req.field.clone().unwrap_or_default();
                                    let enhancer_value = req.enhancer.map_or("none", enhancer_to_value);
                                    let enhancer_opts = available_enhancers(req.kind);
                                    let enhancer_disabled = req.field.is_some() || enhancer_opts.is_empty();
                                    let field_disabled = req.enhancer.is_some();
                                    let requires_platform = req.kind.requires_platform();
                                    let binding_name_value = binding_name(&req.binding).to_string();
                                    let preview = injection_value_preview(req);
                                    let name_recommendation = if binding_name_value.is_empty() {
                                        recommended_binding_name(req)
                                    } else {
                                        None
                                    };
                                    let idx_remove = idx;
                                    let idx_kind = idx;
                                    let idx_platform = idx;
                                    let idx_field = idx;
                                    let idx_enhancer = idx;
                                    let idx_binding = idx;

                                    rsx! {
                                        div { class: "border border-base-300 rounded-box p-3 space-y-2", key: "{idx}",
                                            div { class: "flex justify-between items-center",
                                                span { class: "text-xs font-semibold text-base-content/60", "需求 #{idx + 1}" }
                                                button { class: "btn btn-ghost btn-xs text-error",
                                                    onclick: move |_| {
                                                        new_requirements.write().remove(idx_remove);
                                                    }, "✕ 移除" }
                                            }
                                            div { class: "flex gap-2 flex-wrap",
                                                div { class: "form-control flex-1 min-w-[10rem]",
                                                    label { class: "label",
                                                        span { class: "label-text text-xs", "凭据类型 *" }
                                                    }
                                                    select { class: "select select-bordered select-sm w-full", value: "{kind_value}",
                                                        onchange: move |e| {
                                                            let kind = kind_from_value(&e.value()).unwrap_or(CredentialKind::GithubToken);
                                                            let mut list = new_requirements.write();
                                                            let r = &mut list[idx_kind];
                                                            r.kind = kind;
                                                            // 联动重置：专用类清空 platform；不支持当前增强器时清空
                                                            if !kind.requires_platform() {
                                                                r.platform = None;
                                                            }
                                                            if r.enhancer.is_some_and(|en| !enhancer_supports(kind, en)) {
                                                                r.enhancer = None;
                                                            }
                                                        },
                                                        option { value: "lark_app", "lark_app（飞书应用）" }
                                                        option { value: "github_token", "github_token（GitHub 令牌）" }
                                                        option { value: "tavily_key", "tavily_key（Tavily Key）" }
                                                        option { value: "generic_token", "generic_token（通用平台令牌）" }
                                                        option { value: "oauth", "oauth（OAuth 刷新凭据）" }
                                                        option { value: "user_password", "user_password（用户名密码）" }
                                                    }
                                                }
                                                if requires_platform {
                                                    div { class: "form-control flex-1 min-w-[8rem]",
                                                        label { class: "label",
                                                            span { class: "label-text text-xs", "平台标识 *" }
                                                        }
                                                        input { class: "input input-bordered input-sm w-full", value: "{platform_value}",
                                                            oninput: move |e| {
                                                                let v = e.value();
                                                                let mut list = new_requirements.write();
                                                                list[idx_platform].platform =
                                                                    if v.is_empty() { None } else { Some(v) };
                                                            },
                                                            placeholder: "如 linear / notion" }
                                                    }
                                                }
                                            }
                                            div { class: "flex gap-2 flex-wrap",
                                                div { class: "form-control flex-1 min-w-[8rem]",
                                                    label { class: "label",
                                                        span { class: "label-text text-xs", "提取字段（可选）" }
                                                    }
                                                    input { class: "input input-bordered input-sm w-full", value: "{field_value}",
                                                        disabled: field_disabled,
                                                        oninput: move |e| {
                                                            let v = e.value();
                                                            let mut list = new_requirements.write();
                                                            let r = &mut list[idx_field];
                                                            if v.is_empty() {
                                                                r.field = None;
                                                            } else {
                                                                // 互斥防御：填写字段时清空增强器
                                                                r.field = Some(v);
                                                                r.enhancer = None;
                                                            }
                                                        },
                                                        placeholder: "如 token / api_key" }
                                                }
                                                div { class: "form-control flex-1 min-w-[8rem]",
                                                    label { class: "label",
                                                        span { class: "label-text text-xs", "增强器（可选）" }
                                                    }
                                                    select { class: "select select-bordered select-sm w-full", value: "{enhancer_value}",
                                                        disabled: enhancer_disabled,
                                                        onchange: move |e| {
                                                            let enhancer = enhancer_from_value(&e.value());
                                                            let mut list = new_requirements.write();
                                                            let r = &mut list[idx_enhancer];
                                                            r.enhancer = enhancer;
                                                            // 互斥防御：选择增强器时清空提取字段
                                                            if enhancer.is_some() {
                                                                r.field = None;
                                                            }
                                                        },
                                                        option { value: "none", "不使用增强器" }
                                                        for e in enhancer_opts.iter() {
                                                            option { value: "{enhancer_to_value(*e)}", "{enhancer_display(*e)}" }
                                                        }
                                                    }
                                                    if enhancer_disabled {
                                                        span { class: "text-xs text-base-content/50",
                                                            { if req.field.is_some() {
                                                                "已指定提取字段，二选一".to_string()
                                                            } else if has_any_enhancer_support(req.kind) {
                                                                "默认增强器自动生效，无需选择".to_string()
                                                            } else {
                                                                "该凭据类型不适用增强器".to_string()
                                                            } }
                                                        }
                                                    }
                                                }
                                            }
                                            div { class: "form-control w-full",
                                                label { class: "label",
                                                    span { class: "label-text text-xs", "{binding_name_label}" }
                                                }
                                                input { class: "input input-bordered input-sm w-full", value: "{binding_name_value}",
                                                    oninput: move |e| {
                                                        let v = e.value();
                                                        let mut list = new_requirements.write();
                                                        match &mut list[idx_binding].binding {
                                                            CredentialBinding::Env { name }
                                                            | CredentialBinding::Header { name }
                                                            | CredentialBinding::Query { name } => *name = v,
                                                            CredentialBinding::Internal { field } => *field = v,
                                                        }
                                                    },
                                                    placeholder: "{binding_name_placeholder}" }
                                            }
                                            // ===== 注入值预览 + 惯用名建议（只读示意） =====
                                            div { class: "flex gap-2 flex-wrap items-center text-xs text-base-content/50",
                                                span {
                                                    code { class: "bg-base-200 rounded px-1", "{preview}" }
                                                    " ← 注入值形态"
                                                }
                                                if let Some(rec) = name_recommendation {
                                                    span { class: "text-info",
                                                        "建议注入名：{rec}"
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
            message: "确定删除此 MCP 服务器？此操作不可撤销。".to_string(),
            on_confirm: move |_| {
                let id = pending_delete_id();
                show_delete_confirm.set(false);
                spawn(async move {
                    if let Err(e) = delete_mcp_server(&id).await {
                        toast.error(format!("删除失败: {}", e));
                    } else {
                        match list_mcp_servers().await {
                            Ok(list) => servers.set(list.servers),
                            Err(e) => toast.error(&e),
                        }
                    }
                });
            },
            on_cancel: move |_| {
                show_delete_confirm.set(false);
            }
        }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_binding_follows_transport() {
        assert!(matches!(
            empty_binding(McpTransport::Stdio),
            CredentialBinding::Env { .. }
        ));
        assert!(matches!(
            empty_binding(McpTransport::StreamableHttp),
            CredentialBinding::Header { .. }
        ));
    }
}
