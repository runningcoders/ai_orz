//! MCP 服务器编辑 Modal（列表页 + 详情页共用共享组件）
//!
//! 表单结构复用「添加服务器」Modal 与凭据需求动态编辑器（credential_form 共享纯函数）；
//! 编辑态预填现值（config 属类型级声明 redacted-safe 非机密，可安全回显）。
//!
//! 保存 = `PUT /api/v1/finance/mcp-servers/{id}`，name/transport/config 三 Option 全量发送
//! （config 整体替换；凭证由平台运行时注入不落 config，无敏感丢失风险）。
//!
//! 纪律：transport 允许修改（变更时注入点类型联动重置并保留注入名，UI 一行确认提示）；
//! 由父页条件渲染挂载（每次打开重新挂载，signals 从 props 正确初始化）。

use dioxus::prelude::*;

use crate::api::finance::update_mcp_server;
use crate::components::credential_form::{
    available_enhancers, binding_name, enhancer_display, enhancer_from_value, enhancer_to_value,
    has_any_enhancer_support, injection_value_preview, kind_from_value, mcp_transport_scope,
    normalize_requirements, recommended_binding_name, validate_requirements_scoped,
};
use crate::components::modal::Modal;
use crate::store::toast::use_toast;
use common::api::{McpServerConfigDto, UpdateMcpServerRequest};
use common::enums::McpTransport;
use common::models::{CredentialBinding, CredentialKind, CredentialRequirement, enhancer_supports};

/// 编辑态数据快照（McpServerListItem 与 McpServerDetail 字段同构，列表页/详情页两处通用）
#[derive(Clone, PartialEq)]
pub struct McpServerEditSnapshot {
    pub id: String,
    pub name: String,
    pub transport: McpTransport,
    pub config: McpServerConfigDto,
}

/// 按传输方式构造空名注入点（stdio → Env / streamable_http → Header；沿 mcp_servers.rs 同款）
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

/// MCP 服务器编辑 Modal（父页条件渲染挂载：每次打开重新挂载，signals 从 props 正确初始化）
#[component]
pub fn McpServerEditModal(
    server: McpServerEditSnapshot,
    on_close: EventHandler<()>,
    on_saved: EventHandler<()>,
) -> Element {
    let toast = use_toast();
    let server_id = server.id.clone();
    let server_name = server.name.clone();

    // ===== 编辑态预填现值（仅首次挂载初始化；父页条件渲染保证每次打开重新挂载）=====
    let init_name = server.name.clone();
    let init_transport_value = match server.transport {
        McpTransport::StreamableHttp => "1".to_string(),
        McpTransport::Stdio => "0".to_string(),
    };
    let init_config_value = match server.transport {
        McpTransport::StreamableHttp => server.config.url.clone().unwrap_or_default(),
        McpTransport::Stdio => server.config.command.clone().unwrap_or_default(),
    };
    let init_requirements = server.config.credential_requirements.clone();

    let mut edit_name = use_signal(move || init_name);
    let mut edit_transport = use_signal(move || init_transport_value);
    let mut edit_config_value = use_signal(move || init_config_value);
    let mut edit_requirements = use_signal(move || init_requirements);
    let mut saving = use_signal(|| false);

    // ===== 保存提交（name/transport/config 三 Option 全量发送；config 整体替换）=====
    let handle_save = move |_| {
        let id = server_id.clone();
        spawn(async move {
            let name = edit_name().trim().to_string();
            if name.is_empty() {
                toast.error("服务器名称不能为空");
                return;
            }
            let transport = match edit_transport().parse::<i32>().unwrap_or(0) {
                1 => McpTransport::StreamableHttp,
                _ => McpTransport::Stdio,
            };
            // 凭据需求预校验（规范化后执行，失败 toast 具体错误不提交）
            let requirements = normalize_requirements(edit_requirements());
            if let Err(e) =
                validate_requirements_scoped(&requirements, mcp_transport_scope(transport))
            {
                toast.error(&e);
                return;
            }
            let mut config = if transport == McpTransport::StreamableHttp {
                McpServerConfigDto {
                    url: if edit_config_value().is_empty() {
                        None
                    } else {
                        Some(edit_config_value())
                    },
                    ..Default::default()
                }
            } else {
                McpServerConfigDto {
                    command: if edit_config_value().is_empty() {
                        None
                    } else {
                        Some(edit_config_value())
                    },
                    ..Default::default()
                }
            };
            config.credential_requirements = requirements;
            saving.set(true);
            let req = UpdateMcpServerRequest {
                id,
                name: Some(name),
                transport: Some(transport),
                config: Some(config),
            };
            match update_mcp_server(req).await {
                Ok(_) => {
                    toast.success("保存成功，下次工具调用即生效");
                    on_saved.call(());
                }
                Err(e) => toast.error(format!("保存失败: {}", e)),
            }
            saving.set(false);
        });
    };

    let edit_transport_value = edit_transport();
    let is_http_transport = edit_transport_value == "1";
    let config_label = if is_http_transport { "URL" } else { "命令" };
    let config_placeholder = if is_http_transport {
        "https://..."
    } else {
        "npx -y @modelcontextprotocol/server-filesystem /tmp"
    };
    let requirements_list = edit_requirements();
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
        let transport = if edit_transport() == "1" {
            McpTransport::StreamableHttp
        } else {
            McpTransport::Stdio
        };
        edit_requirements.write().push(CredentialRequirement {
            kind: CredentialKind::GithubToken,
            platform: None,
            field: None,
            enhancer: None,
            binding: empty_binding(transport),
        });
    };

    rsx! {
        Modal {
            title: format!("编辑 MCP 服务器：{server_name}"),
            show: true,
            on_close: move |_| on_close.call(()),
            footer: rsx! {
                button { class: "btn hud-btn btn-ghost", onclick: move |_| on_close.call(()), "取消" }
                button { class: "btn hud-btn btn-primary", disabled: saving(), onclick: handle_save,
                    if saving() { "保存中..." } else { "保存" }
                }
            },
            div { class: "max-h-[70vh] overflow-y-auto space-y-4 pr-1",
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "服务器名称 *" }
                    }
                    input { class: "input input-bordered w-full", value: "{edit_name}",
                        oninput: move |e| edit_name.set(e.value()), placeholder: "服务器显示名" }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "传输方式" }
                    }
                    select { class: "select select-bordered w-full", value: "{edit_transport_value}",
                        onchange: move |e| {
                            edit_transport.set(e.value());
                            // 传输方式变更：已有条目注入点类型联动重置（保留注入名）
                            let stdio = e.value() != "1";
                            edit_requirements.write().iter_mut().for_each(|r| {
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
                    p { class: "text-xs text-warning mt-1",
                        "修改传输方式将影响启动方式与注入点类型（Stdio → 环境变量 / StreamableHttp → 请求头），请确认后保存。"
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "{config_label}" }
                    }
                    input { class: "input input-bordered w-full", value: "{edit_config_value}",
                        oninput: move |e| edit_config_value.set(e.value()),
                        placeholder: "{config_placeholder}" }
                }

                // ===== 凭据需求动态列表（复用添加 Modal 同款编辑器）=====
                div { class: "form-control w-full",
                    div { class: "flex justify-between items-center",
                        span { class: "label-text font-medium", "凭据需求（可选）" }
                        button { class: "btn hud-btn btn-ghost btn-xs", onclick: on_add_requirement, "＋ 添加" }
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
                                                button { class: "btn hud-btn btn-ghost btn-xs text-error",
                                                    onclick: move |_| {
                                                        edit_requirements.write().remove(idx_remove);
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
                                                            let mut list = edit_requirements.write();
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
                                                                let mut list = edit_requirements.write();
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
                                                            let mut list = edit_requirements.write();
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
                                                            let mut list = edit_requirements.write();
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
                                                        let mut list = edit_requirements.write();
                                                        match &mut list[idx_binding].binding {
                                                            CredentialBinding::Env { name }
                                                            | CredentialBinding::Header { name }
                                                            | CredentialBinding::Query { name } => *name = v,
                                                            CredentialBinding::Internal { field } => *field = v,
                                                        }
                                                    },
                                                    placeholder: "{binding_name_placeholder}" }
                                            }
                                            // ===== 注入值预览 + 惯用名建议（只读示意）=====
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
    }
}
