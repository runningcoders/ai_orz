//! MCP 服务器管理

use dioxus::prelude::*;
use dioxus_router::Link;

use crate::api::finance::{
    create_mcp_server, delete_mcp_server, list_mcp_servers, sync_mcp_tools,
    update_mcp_server_status,
};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use crate::utils::format_datetime_full as format_timestamp;
use common::api::{
    CreateMcpServerRequest, McpServerConfigDto, McpServerListItem, UpdateMcpServerStatusRequest,
};
use common::enums::{McpServerStatus, McpTransport};
use common::models::{
    CredentialBinding, CredentialEnhancerKind, CredentialKind, CredentialRequirement,
    default_enhancer, enhancer_supports,
};

// ==================== 凭据需求表单纯函数（单测覆盖） ====================

/// 全部凭据类型（kind 下拉选项，serde 值 = 展示键）
fn all_credential_kinds() -> [CredentialKind; 6] {
    [
        CredentialKind::LarkApp,
        CredentialKind::GithubToken,
        CredentialKind::TavilyKey,
        CredentialKind::GenericToken,
        CredentialKind::OAuth,
        CredentialKind::UserPassword,
    ]
}

/// 按 serde 值解析凭据类型
fn kind_from_value(v: &str) -> Option<CredentialKind> {
    all_credential_kinds().into_iter().find(|k| k.as_str() == v)
}

/// 注入点名（binding 的 name / field）
fn binding_name(binding: &CredentialBinding) -> &str {
    match binding {
        CredentialBinding::Env { name }
        | CredentialBinding::Header { name }
        | CredentialBinding::Query { name } => name,
        CredentialBinding::Internal { field } => field,
    }
}

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

/// 规范化：trim platform/field/注入名，空白 Option 归 None
fn normalize_requirements(list: Vec<CredentialRequirement>) -> Vec<CredentialRequirement> {
    list.into_iter()
        .map(|mut r| {
            r.platform = r
                .platform
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty());
            r.field = r
                .field
                .map(|f| f.trim().to_string())
                .filter(|f| !f.is_empty());
            let name = binding_name(&r.binding).trim().to_string();
            r.binding = match r.binding {
                CredentialBinding::Env { .. } => CredentialBinding::Env { name },
                CredentialBinding::Header { .. } => CredentialBinding::Header { name },
                CredentialBinding::Query { .. } => CredentialBinding::Query { name },
                CredentialBinding::Internal { .. } => CredentialBinding::Internal { field: name },
            };
            r
        })
        .collect()
}

/// 前端预校验（后端 `validate_requirements` 的 MCP 简化版；失败返回具体错误文案）
///
/// 五条规则：binding↔transport / 注入名非空 / platform↔kind / field↔enhancer 互斥 /
/// (kind, platform, 注入名) 三元组去重。
fn validate_requirements(
    requirements: &[CredentialRequirement],
    transport: McpTransport,
) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for req in requirements {
        // 1. binding ↔ transport（Env 仅 stdio / Header 仅 streamable_http）
        let binding_matches = matches!(
            (&req.binding, transport),
            (CredentialBinding::Env { .. }, McpTransport::Stdio)
                | (CredentialBinding::Header { .. }, McpTransport::StreamableHttp)
        );
        if !binding_matches {
            return Err("凭据注入点与传输方式不匹配（Stdio 仅支持环境变量注入，StreamableHttp 仅支持请求头注入）".to_string());
        }
        // 2. 注入名非空
        if binding_name(&req.binding).trim().is_empty() {
            return Err("凭据注入点名不能为空".to_string());
        }
        // 3. platform ↔ kind（generic 类必填、专用类必空）
        if req.kind.requires_platform() != req.platform.is_some() {
            return Err(if req.kind.requires_platform() {
                format!("凭据类型 {} 必须填写平台标识", req.kind.as_str())
            } else {
                format!("凭据类型 {} 不适用平台标识，请清空", req.kind.as_str())
            });
        }
        // 4. field ↔ enhancer 互斥
        if req.field.is_some() && req.enhancer.is_some() {
            return Err(format!(
                "凭据类型 {} 的提取字段与增强器互斥，只能二选一",
                req.kind.as_str()
            ));
        }
        // 5. (kind, platform, 注入名) 三元组去重
        let key = (
            req.kind,
            req.platform.clone(),
            binding_name(&req.binding).to_string(),
        );
        if !seen.insert(key) {
            return Err("存在重复的凭据需求（同凭据类型 + 同平台 + 同注入点）".to_string());
        }
    }
    Ok(())
}

/// 该类型是否存在任一受支持增强器（专用 kind 为 false，用于禁用提示区分）
fn has_any_enhancer_support(kind: CredentialKind) -> bool {
    all_enhancers()
        .iter()
        .any(|e| enhancer_supports(kind, *e))
}

/// 可选增强器列表（按 supports 矩阵过滤且排除默认增强器，D11 前端不暴露默认项）
fn available_enhancers(kind: CredentialKind) -> Vec<CredentialEnhancerKind> {
    all_enhancers()
        .into_iter()
        .filter(|e| enhancer_supports(kind, *e) && default_enhancer(kind) != Some(*e))
        .collect()
}

fn all_enhancers() -> [CredentialEnhancerKind; 3] {
    [
        CredentialEnhancerKind::BearerToken,
        CredentialEnhancerKind::BasicAuth,
        CredentialEnhancerKind::AccessToken,
    ]
}

/// 增强器下拉值（与 serde snake_case 值空间一致）
fn enhancer_to_value(e: CredentialEnhancerKind) -> &'static str {
    match e {
        CredentialEnhancerKind::BearerToken => "bearer_token",
        CredentialEnhancerKind::BasicAuth => "basic_auth",
        CredentialEnhancerKind::AccessToken => "access_token",
    }
}

/// 按下拉值解析增强器（"none" → None）
fn enhancer_from_value(v: &str) -> Option<CredentialEnhancerKind> {
    all_enhancers()
        .into_iter()
        .find(|e| enhancer_to_value(*e) == v)
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
            if let Err(e) = validate_requirements(&requirements, transport) {
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
            div { class: "card bg-base-100 shadow-md",
                div { class: "card-body",
                    div { class: "flex justify-between items-center mb-4",
                        h2 { class: "card-title", "MCP 服务器管理" }
                        button { class: "btn btn-primary", onclick: move |_| show_add_modal.set(true), "+ 添加服务器" }
                    }
                if loading() {
                    Loading {}
                } else if servers_list.is_empty() {
                    EmptyState { icon: "🖥️".to_string(), message: "暂无 MCP 服务器".to_string() }
                } else {
                    div { class: "overflow-x-auto",
                        table { class: "table table-zebra table-pin-rows",
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
                                                            option { value: "{enhancer_to_value(*e)}", "{enhancer_to_value(*e)}" }
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

    fn req(
        kind: CredentialKind,
        platform: Option<&str>,
        field: Option<&str>,
        enhancer: Option<CredentialEnhancerKind>,
        binding: CredentialBinding,
    ) -> CredentialRequirement {
        CredentialRequirement {
            kind,
            platform: platform.map(|s| s.to_string()),
            field: field.map(|s| s.to_string()),
            enhancer,
            binding,
        }
    }

    fn env(name: &str) -> CredentialBinding {
        CredentialBinding::Env {
            name: name.to_string(),
        }
    }

    fn header(name: &str) -> CredentialBinding {
        CredentialBinding::Header {
            name: name.to_string(),
        }
    }

    // ===== validate_requirements 五条规则 =====

    #[test]
    fn validate_accepts_valid_requirements() {
        let list = vec![
            req(
                CredentialKind::GithubToken,
                None,
                None,
                None,
                env("GITHUB_TOKEN"),
            ),
            req(
                CredentialKind::GenericToken,
                Some("linear"),
                Some("token"),
                None,
                env("LINEAR_TOKEN"),
            ),
            req(
                CredentialKind::OAuth,
                Some("okta"),
                None,
                Some(CredentialEnhancerKind::BearerToken),
                env("OKTA_TOKEN"),
            ),
        ];
        assert!(validate_requirements(&list, McpTransport::Stdio).is_ok());
    }

    #[test]
    fn validate_rejects_binding_transport_mismatch() {
        // Env binding 用于 streamable_http → 拒绝
        let list = vec![req(
            CredentialKind::GithubToken,
            None,
            None,
            None,
            env("GITHUB_TOKEN"),
        )];
        let err = validate_requirements(&list, McpTransport::StreamableHttp).unwrap_err();
        assert!(err.contains("不匹配"), "unexpected: {err}");
        // Header binding 用于 stdio → 拒绝
        let list = vec![req(
            CredentialKind::GithubToken,
            None,
            None,
            None,
            header("authorization"),
        )];
        assert!(validate_requirements(&list, McpTransport::Stdio).is_err());
    }

    #[test]
    fn validate_rejects_empty_binding_name() {
        let list = vec![req(
            CredentialKind::GithubToken,
            None,
            None,
            None,
            env("   "),
        )];
        let err = validate_requirements(&list, McpTransport::Stdio).unwrap_err();
        assert!(err.contains("注入点名"), "unexpected: {err}");
    }

    #[test]
    fn validate_rejects_platform_kind_mismatch() {
        // generic 类缺 platform → 拒绝
        let list = vec![req(
            CredentialKind::GenericToken,
            None,
            None,
            None,
            env("TOKEN"),
        )];
        let err = validate_requirements(&list, McpTransport::Stdio).unwrap_err();
        assert!(err.contains("必须填写平台标识"), "unexpected: {err}");
        // 专用类带 platform → 拒绝
        let list = vec![req(
            CredentialKind::GithubToken,
            Some("github"),
            None,
            None,
            env("TOKEN"),
        )];
        let err = validate_requirements(&list, McpTransport::Stdio).unwrap_err();
        assert!(err.contains("不适用平台标识"), "unexpected: {err}");
    }

    #[test]
    fn validate_rejects_field_enhancer_conflict() {
        let list = vec![req(
            CredentialKind::GenericToken,
            Some("linear"),
            Some("token"),
            Some(CredentialEnhancerKind::BearerToken),
            env("LINEAR_TOKEN"),
        )];
        let err = validate_requirements(&list, McpTransport::Stdio).unwrap_err();
        assert!(err.contains("互斥"), "unexpected: {err}");
    }

    #[test]
    fn validate_rejects_duplicate_triple() {
        // 同 (kind, platform, 注入名) 三元组 → 拒绝（field 不同也算重复）
        let list = vec![
            req(
                CredentialKind::GenericToken,
                Some("linear"),
                Some("token"),
                None,
                env("LINEAR_TOKEN"),
            ),
            req(
                CredentialKind::GenericToken,
                Some("linear"),
                None,
                Some(CredentialEnhancerKind::BearerToken),
                env("LINEAR_TOKEN"),
            ),
        ];
        let err = validate_requirements(&list, McpTransport::Stdio).unwrap_err();
        assert!(err.contains("重复"), "unexpected: {err}");
    }

    #[test]
    fn validate_allows_same_kind_different_binding_name() {
        let list = vec![
            req(
                CredentialKind::GithubToken,
                None,
                None,
                None,
                env("GITHUB_TOKEN"),
            ),
            req(
                CredentialKind::GithubToken,
                None,
                None,
                None,
                env("GH_ENTERPRISE_TOKEN"),
            ),
        ];
        assert!(validate_requirements(&list, McpTransport::Stdio).is_ok());
    }

    #[test]
    fn validate_empty_list_passes() {
        assert!(validate_requirements(&[], McpTransport::Stdio).is_ok());
    }

    // ===== normalize_requirements =====

    #[test]
    fn normalize_trims_and_drops_empty_options() {
        let list = vec![req(
            CredentialKind::GenericToken,
            Some("  linear  "),
            Some("  "),
            None,
            env("  LINEAR_TOKEN  "),
        )];
        let normalized = normalize_requirements(list);
        assert_eq!(normalized.len(), 1);
        let r = &normalized[0];
        assert_eq!(r.platform.as_deref(), Some("linear"));
        assert_eq!(r.field, None, "空白 field 归 None");
        assert_eq!(binding_name(&r.binding), "LINEAR_TOKEN");
    }

    // ===== 增强器选项矩阵（D11：默认增强器不暴露） =====

    #[test]
    fn available_enhancers_follow_supports_matrix_excluding_defaults() {
        use CredentialEnhancerKind as E;
        // 专用 kind：零可选项
        for kind in [
            CredentialKind::LarkApp,
            CredentialKind::GithubToken,
            CredentialKind::TavilyKey,
        ] {
            assert!(available_enhancers(kind).is_empty(), "{kind:?}");
            assert!(!has_any_enhancer_support(kind), "{kind:?}");
        }
        // generic_token：仅 bearer_token（无默认增强器）
        assert_eq!(
            available_enhancers(CredentialKind::GenericToken),
            vec![E::BearerToken]
        );
        // oauth：bearer_token 可选，access_token 为默认项不暴露
        assert_eq!(
            available_enhancers(CredentialKind::OAuth),
            vec![E::BearerToken]
        );
        assert!(has_any_enhancer_support(CredentialKind::OAuth));
        // user_password：basic_auth 为默认项不暴露 → 空列表但存在支持
        assert!(available_enhancers(CredentialKind::UserPassword).is_empty());
        assert!(has_any_enhancer_support(CredentialKind::UserPassword));
    }

    // ===== 值解析辅助 =====

    #[test]
    fn kind_and_enhancer_value_roundtrip() {
        for kind in all_credential_kinds() {
            assert_eq!(kind_from_value(kind.as_str()), Some(kind));
        }
        assert_eq!(kind_from_value("unknown"), None);
        for e in all_enhancers() {
            assert_eq!(enhancer_from_value(enhancer_to_value(e)), Some(e));
        }
        assert_eq!(enhancer_from_value("none"), None);
    }

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
