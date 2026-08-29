//! 消息渠道管理
//!
//! 飞书渠道交互约定（二期凭证引用模式）：
//! - 创建时类型选「飞书」：必须选择已绑定的应用凭证（用户级，Finance → Identity 身份凭证页管理）
//! - 无凭证时展示引导条跳转身份凭证页绑定；有凭证时下拉选择（传 lark_credential_id）
//! - 身份模式下拉（自动/应用身份/用户身份，缺省 auto）
//! - 入站监听 toggle 默认开，关闭后仅用于出站推送与 lark_cli 工具身份

use crate::components::hud::PageHeader;
use crate::components::hud::{HudCallout, HudPanel};
use dioxus::prelude::*;
use dioxus_router::Link;

use crate::api::finance::{
    create_message_channel, delete_message_channel, list_message_channels, test_message_channel,
    update_message_channel_status,
};
use crate::api::hr::list_agents;
use crate::api::lark_integration::get_lark_integration_status;
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{
    AgentListItem, CreateEmailChannelConfig, CreateLarkChannelConfig, CreateMessageChannelConfig,
    CreateMessageChannelRequest, CreateSlackChannelConfig, CreateWebhookChannelConfig,
    CreateWechatChannelConfig, LarkCredentialSnapshot, ListAgentsRequest,
    ListMessageChannelsResponseItem, UpdateMessageChannelStatusRequest,
};
use common::enums::{ChannelStatus, ChannelType};

/// 创建表单提交前校验（纯函数，可单测）
///
/// 规则：名称非空；飞书类型下必须选择已绑定的应用凭证。
pub fn validate_create_channel_form(
    name: &str,
    is_lark: bool,
    lark_credential_id: &str,
) -> Result<(), &'static str> {
    if name.trim().is_empty() {
        return Err("渠道名称不能为空");
    }
    if is_lark && lark_credential_id.trim().is_empty() {
        return Err("飞书渠道必须选择应用凭证（请先到设置页绑定飞书应用）");
    }
    Ok(())
}

fn none_if_empty(value: String) -> Option<String> {
    if value.trim().is_empty() {
        None
    } else {
        Some(value)
    }
}

#[component]
pub fn FinanceMessageChannels() -> Element {
    let mut channels = use_signal(Vec::<ListMessageChannelsResponseItem>::new);
    let mut loading = use_signal(|| true);
    let toast = use_toast();
    let mut show_add_modal = use_signal(|| false);

    let mut new_name = use_signal(String::new);
    let mut new_type = use_signal(|| "0".to_string());
    let mut new_webhook_url = use_signal(String::new);
    let mut new_access_token = use_signal(String::new);
    let mut new_secret = use_signal(String::new);
    let mut new_lark_credential_id = use_signal(String::new);
    let mut new_lark_identity_mode = use_signal(String::new);
    let mut new_lark_open_id = use_signal(String::new);
    let mut new_lark_user_name = use_signal(String::new);
    let mut new_agent_id = use_signal(String::new);
    let mut new_listen_inbound = use_signal(|| true);
    // 微信
    let mut new_wechat_app_id = use_signal(String::new);
    let mut new_wechat_app_secret = use_signal(String::new);
    let mut new_wechat_open_id = use_signal(String::new);
    // 邮件
    let mut new_email_smtp_host = use_signal(String::new);
    let mut new_email_smtp_port = use_signal(String::new);
    let mut new_email_username = use_signal(String::new);
    let mut new_email_password = use_signal(String::new);
    let mut new_email_from = use_signal(String::new);
    let mut new_email_to = use_signal(String::new);
    // Slack
    let mut new_slack_bot_token = use_signal(String::new);
    let mut new_slack_channel_id = use_signal(String::new);
    // Webhook 高级
    let mut new_webhook_method = use_signal(String::new);
    let mut new_webhook_body_template = use_signal(String::new);
    let mut creating = use_signal(|| false);

    // Agent 下拉数据
    let mut agents = use_signal(Vec::<AgentListItem>::new);
    // 飞书凭证下拉数据（聚合端点）
    let mut lark_credentials = use_signal(Vec::<LarkCredentialSnapshot>::new);

    // ===== 删除确认对话框 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(String::new);

    use_effect(move || {
        loading.set(true);
        spawn(async move {
            match list_message_channels().await {
                Ok(list) => channels.set(list.channels),
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
        // Agent 下拉列表（绑定 Agent 用）
        spawn(async move {
            if let Ok(page) = list_agents(ListAgentsRequest::default()).await {
                agents.set(page.items);
            }
        });
        // 飞书凭证下拉（飞书渠道创建必选）
        spawn(async move {
            if let Ok(status) = get_lark_integration_status().await {
                lark_credentials.set(status.credentials);
            }
        });
    });

    let handle_create = move |_| {
        spawn(async move {
            let is_lark = new_type() == "0";
            if let Err(msg) =
                validate_create_channel_form(&new_name(), is_lark, &new_lark_credential_id())
            {
                toast.error(msg);
                return;
            }
            creating.set(true);
            let channel_type = ChannelType::from_i32(new_type().parse::<i32>().unwrap_or(0));
            let email_port: Option<u16> = new_email_smtp_port().parse().ok();
            let req = CreateMessageChannelRequest {
                user_id: None,
                agent_id: none_if_empty(new_agent_id()),
                channel_type,
                channel_name: new_name(),
                webhook_url: none_if_empty(new_webhook_url()),
                access_token: none_if_empty(new_access_token()),
                secret: none_if_empty(new_secret()),
                config: Some(CreateMessageChannelConfig {
                    lark: if channel_type == ChannelType::Lark {
                        Some(CreateLarkChannelConfig {
                            credential_id: none_if_empty(new_lark_credential_id()),
                            identity_mode: none_if_empty(new_lark_identity_mode()),
                            open_id: none_if_empty(new_lark_open_id()),
                            user_name: none_if_empty(new_lark_user_name()),
                            listen_inbound: Some(new_listen_inbound()),
                        })
                    } else {
                        None
                    },
                    wechat: if channel_type == ChannelType::Wechat {
                        Some(CreateWechatChannelConfig {
                            app_id: none_if_empty(new_wechat_app_id()),
                            app_secret: none_if_empty(new_wechat_app_secret()),
                            open_id: none_if_empty(new_wechat_open_id()),
                        })
                    } else {
                        None
                    },
                    email: if channel_type == ChannelType::Email {
                        Some(CreateEmailChannelConfig {
                            smtp_host: none_if_empty(new_email_smtp_host()),
                            smtp_port: email_port,
                            username: none_if_empty(new_email_username()),
                            password: none_if_empty(new_email_password()),
                            from_address: none_if_empty(new_email_from()),
                            to_address: none_if_empty(new_email_to()),
                        })
                    } else {
                        None
                    },
                    slack: if channel_type == ChannelType::Slack {
                        Some(CreateSlackChannelConfig {
                            bot_token: none_if_empty(new_slack_bot_token()),
                            channel_id: none_if_empty(new_slack_channel_id()),
                        })
                    } else {
                        None
                    },
                    webhook: if channel_type == ChannelType::Webhook {
                        Some(CreateWebhookChannelConfig {
                            method: none_if_empty(new_webhook_method()),
                            body_template: none_if_empty(new_webhook_body_template()),
                        })
                    } else {
                        None
                    },
                }),
            };
            match create_message_channel(req).await {
                Ok(_) => {
                    show_add_modal.set(false);
                    new_name.set(String::new());
                    new_type.set("0".to_string());
                    new_webhook_url.set(String::new());
                    new_access_token.set(String::new());
                    new_secret.set(String::new());
                    new_lark_credential_id.set(String::new());
                    new_lark_identity_mode.set(String::new());
                    new_lark_open_id.set(String::new());
                    new_lark_user_name.set(String::new());
                    new_agent_id.set(String::new());
                    new_listen_inbound.set(true);
                    new_wechat_app_id.set(String::new());
                    new_wechat_app_secret.set(String::new());
                    new_wechat_open_id.set(String::new());
                    new_email_smtp_host.set(String::new());
                    new_email_smtp_port.set(String::new());
                    new_email_username.set(String::new());
                    new_email_password.set(String::new());
                    new_email_from.set(String::new());
                    new_email_to.set(String::new());
                    new_slack_bot_token.set(String::new());
                    new_slack_channel_id.set(String::new());
                    new_webhook_method.set(String::new());
                    new_webhook_body_template.set(String::new());
                    toast.success("创建成功，建议先运行连接测试");
                    match list_message_channels().await {
                        Ok(list) => channels.set(list.channels),
                        Err(e) => toast.error(&e),
                    }
                }
                Err(e) => toast.error(format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    let channels_list = channels.read().clone();
    let agents_list = agents.read().clone();
    let credentials_list = lark_credentials.read().clone();

    let new_type_value = new_type();
    let is_lark_type = new_type_value == "0";
    let is_wechat_type = new_type_value == "1";
    let is_slack_type = new_type_value == "2";
    let is_email_type = new_type_value == "3";
    let is_webhook_type = new_type_value == "4";
    let no_credentials = is_lark_type && credentials_list.is_empty();
    let credential_value = new_lark_credential_id();
    let identity_mode_value = new_lark_identity_mode();
    let listen_inbound_value = new_listen_inbound();
    let agent_value = new_agent_id();

    rsx! {
        AppLayout {
            HudPanel { signal: Some(true),
                div { class: "card-body",
                    PageHeader {
                        eyebrow: Some("FINANCE".to_string()),
                        title: "消息渠道管理".to_string(),
                        actions: Some(rsx!{
                        button { class: "btn btn-primary", onclick: move |_| show_add_modal.set(true), "+ 创建渠道" }
                        }),
                    },
                    if loading() {
                        Loading {}
                    } else if channels_list.is_empty() {
                        EmptyState { icon: "📡".to_string(), message: "暂无消息渠道".to_string() }
                    } else {
                        div { class: "overflow-x-auto",
                            table { class: "table table-zebra table-pin-rows",
                                thead { tr { th { "名称" }, th { "类型" }, th { "飞书凭证" }, th { "状态" }, th { "操作" } }}
                                tbody {
                                    for c in channels_list.iter() {
                                        {
                                            let id = c.id.clone();
                                            let status = c.status;
                                            let is_active = status == ChannelStatus::Active;
                                            let channel_name = c.channel_name.clone();
                                            let channel_type = c.channel_type;
                                            let is_lark = channel_type == ChannelType::Lark;
                                            let credential_name = c.config.as_ref().and_then(|cfg| cfg.lark.as_ref()).and_then(|l| l.credential_name.clone());
                                            let id_disable = id.clone();
                                            let id_enable = id.clone();
                                            let id_delete = id.clone();
                                            let id_test = id.clone();
                                            rsx! {
                                                tr { key: "{id}",
                                                    td { class: "font-semibold", "{channel_name}" }
                                                    td { span { class: "badge badge-info", "{channel_type}" } }
                                                    td {
                                                        if is_lark {
                                                            if let Some(name) = &credential_name {
                                                                span { class: "badge badge-outline", "{name}" }
                                                            } else {
                                                                span { class: "badge badge-warning", "未绑定凭证" }
                                                            }
                                                        } else {
                                                            span { class: "text-base-content/40 text-sm", "-" }
                                                        }
                                                    }
                                                    td {
                                                        if is_active { span { class: "badge badge-success", "启用" } }
                                                        else { span { class: "badge badge-error", "禁用" } }
                                                    }
                                                    td { class: "flex gap-2 items-center",
                                                        Link {
                                                            class: "btn btn-ghost btn-sm",
                                                            to: crate::pages::Route::FinanceMessageChannelDetail { id: id.clone() },
                                                            "详情"
                                                        }
                                                        if is_active {
                                                            button { class: "btn btn-ghost btn-sm",
                                                                onclick: move |_| {
                                                                    let id_disable = id_disable.clone();
                                                                    spawn(async move {
                                                                        if let Err(e) = update_message_channel_status(UpdateMessageChannelStatusRequest { id: id_disable, status: ChannelStatus::Disabled }).await {
                                                                            toast.error(&e);
                                                                        } else {
                                                                            match list_message_channels().await {
                                                                                Ok(list) => channels.set(list.channels),
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
                                                                        if let Err(e) = update_message_channel_status(UpdateMessageChannelStatusRequest { id: id_enable, status: ChannelStatus::Active }).await {
                                                                            toast.error(&e);
                                                                        } else {
                                                                            match list_message_channels().await {
                                                                                Ok(list) => channels.set(list.channels),
                                                                                Err(e) => toast.error(&e),
                                                                            }
                                                                        }
                                                                    });
                                                                }, "启用"
                                                            }
                                                        }
                                                        button { class: "btn btn-sm btn-primary",
                                                            onclick: move |_| {
                                                                let id_test = id_test.clone();
                                                                spawn(async move {
                                                                    match test_message_channel(&id_test).await {
                                                                        Ok(resp) => {
                                                                            if resp.success {
                                                                                toast.success("连接测试通过");
                                                                            } else {
                                                                                toast.error(format!("连接测试失败: {}", resp.error.unwrap_or_default()));
                                                                            }
                                                                        }
                                                                        Err(e) => toast.error(format!("连接测试失败: {}", e)),
                                                                    }
                                                                });
                                                            }, "连接测试"
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
                title: "创建消息渠道".to_string(),
                show: show_add_modal(),
                on_close: move |_| show_add_modal.set(false),
                footer: rsx! {
                    button { class: "btn btn-ghost", onclick: move |_| show_add_modal.set(false), "取消" }
                    button { class: "btn btn-primary", disabled: creating() || no_credentials, onclick: handle_create,
                        if creating() { "创建中..." } else { "创建" }
                    }
                },
                div { class: "space-y-4",
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "渠道名称 *" }
                        }
                        input { class: "input input-bordered hud-input w-full", value: "{new_name}",
                            oninput: move |e| new_name.set(e.value()), placeholder: "如：飞书接待渠道" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "渠道类型" }
                        }
                        select { class: "select select-bordered hud-input w-full", value: "{new_type_value}",
                            onchange: move |e| new_type.set(e.value()),
                            option { value: "0", "飞书 (Lark)" }
                            option { value: "1", "微信 (Wechat)" }
                            option { value: "2", "Slack" }
                            option { value: "3", "邮件 (Email)" }
                            option { value: "4", "Webhook" }
                        }
                    }

                    // ===== 通用：绑定 Agent + Webhook URL（非飞书渠道） =====
                    if !is_lark_type {
                        div { class: "form-control w-full",
                            label { class: "label",
                                span { class: "label-text", "绑定 Agent" }
                            }
                            select { class: "select select-bordered hud-input w-full", value: "{agent_value}",
                                onchange: move |e| new_agent_id.set(e.value()),
                                option { value: "", "不绑定（用户全局默认渠道）" }
                                for agent in agents_list.iter() {
                                    {
                                        let aid = agent.id.clone();
                                        let aname = agent.name.clone();
                                        rsx! { option { key: "{aid}", value: "{aid}", "{aname}" } }
                                    }
                                }
                            }
                        }
                    }

                    // ===== 飞书配置 =====
                    if is_lark_type {
                        div { class: "divider text-sm font-medium m-0", "飞书应用凭证 *" }
                        if no_credentials {
                            HudCallout { tone: Some("warning".to_string()),
                                span { "尚未绑定飞书应用凭证，请先前往「身份凭证」页完成绑定" }
                                Link { class: "btn btn-sm btn-primary", to: crate::pages::Route::FinanceIdentity {}, "前往绑定" }
                            }
                        } else {
                            div { class: "form-control w-full",
                                label { class: "label",
                                    span { class: "label-text font-medium", "选择凭证 *" }
                                }
                                select { class: "select select-bordered hud-input w-full", value: "{credential_value}",
                                    onchange: move |e| new_lark_credential_id.set(e.value()),
                                    option { value: "", "请选择已绑定的应用凭证" }
                                    for cred in credentials_list.iter() {
                                        {
                                            let cid = cred.credential_id.clone();
                                            let cname = cred.name.clone();
                                            let capp = cred.app_id.clone();
                                            rsx! { option { key: "{cid}", value: "{cid}", "{cname}（{capp}）" } }
                                        }
                                    }
                                }
                                label { class: "label",
                                    span { class: "label-text-alt", "凭证在「财务管理 → 身份凭证」管理，一个凭证可建多条渠道" }
                                }
                            }
                            div { class: "form-control w-full",
                                label { class: "label",
                                    span { class: "label-text font-medium", "身份模式" }
                                }
                                select { class: "select select-bordered hud-input w-full", value: "{identity_mode_value}",
                                    onchange: move |e| new_lark_identity_mode.set(e.value()),
                                    option { value: "", "自动（auto：按能力选择应用/用户身份）" }
                                    option { value: "bot", "应用身份（bot）" }
                                    option { value: "user", "用户身份（user）" }
                                }
                            }
                        }
                        div { class: "divider text-sm font-medium m-0", "用户与路由（可选）" }
                        div { class: "form-control w-full",
                            label { class: "label",
                                span { class: "label-text", "用户 Open ID" }
                            }
                            input { class: "input input-bordered hud-input w-full font-mono", value: "{new_lark_open_id}",
                                oninput: move |e| new_lark_open_id.set(e.value()),
                                placeholder: "ou_xxx，绑定后接收该用户的飞书私信" }
                        }
                        div { class: "form-control w-full",
                            label { class: "label",
                                span { class: "label-text", "用户昵称" }
                            }
                            input { class: "input input-bordered hud-input w-full", value: "{new_lark_user_name}",
                                oninput: move |e| new_lark_user_name.set(e.value()),
                                placeholder: "可选，用于展示" }
                        }
                        div { class: "form-control",
                            label { class: "label cursor-pointer justify-start gap-3",
                                input { class: "toggle toggle-primary", r#type: "checkbox", checked: listen_inbound_value,
                                    onchange: move |_| new_listen_inbound.set(!new_listen_inbound()) }
                                span { class: "label-text",
                                    "入站监听（接收该应用的飞书私信消息；关闭后仅用于出站推送与 lark_cli 工具身份）"
                                }
                            }
                        }
                    }

                    // ===== 微信配置 =====
                    if is_wechat_type {
                        div { class: "divider text-sm font-medium m-0", "微信应用配置" }
                        div { class: "form-control w-full",
                            label { class: "label", span { class: "label-text font-medium", "App ID *" } }
                            input { class: "input input-bordered hud-input w-full", value: "{new_wechat_app_id}",
                                oninput: move |e| new_wechat_app_id.set(e.value()),
                                placeholder: "微信公众号/企业微信 AppID" }
                        }
                        div { class: "form-control w-full",
                            label { class: "label", span { class: "label-text font-medium", "App Secret *" } }
                            input { class: "input input-bordered hud-input w-full", r#type: "password", value: "{new_wechat_app_secret}",
                                oninput: move |e| new_wechat_app_secret.set(e.value()),
                                placeholder: "应用密钥" }
                        }
                        div { class: "form-control w-full",
                            label { class: "label", span { class: "label-text", "用户 Open ID" } }
                            input { class: "input input-bordered hud-input w-full font-mono", value: "{new_wechat_open_id}",
                                oninput: move |e| new_wechat_open_id.set(e.value()),
                                placeholder: "绑定后接收该用户消息" }
                        }
                    }

                    // ===== 邮件配置 =====
                    if is_email_type {
                        div { class: "divider text-sm font-medium m-0", "邮件 SMTP 配置" }
                        div { class: "form-control w-full",
                            label { class: "label", span { class: "label-text font-medium", "SMTP Host *" } }
                            input { class: "input input-bordered hud-input w-full", value: "{new_email_smtp_host}",
                                oninput: move |e| new_email_smtp_host.set(e.value()),
                                placeholder: "smtp.example.com" }
                        }
                        div { class: "form-control w-full",
                            label { class: "label", span { class: "label-text font-medium", "SMTP Port" } }
                            input { class: "input input-bordered hud-input w-full", value: "{new_email_smtp_port}",
                                oninput: move |e| new_email_smtp_port.set(e.value()),
                                placeholder: "465（SSL）/ 587（STARTTLS）" }
                        }
                        div { class: "form-control w-full",
                            label { class: "label", span { class: "label-text font-medium", "用户名 *" } }
                            input { class: "input input-bordered hud-input w-full", value: "{new_email_username}",
                                oninput: move |e| new_email_username.set(e.value()),
                                placeholder: "邮箱账号" }
                        }
                        div { class: "form-control w-full",
                            label { class: "label", span { class: "label-text font-medium", "密码 *" } }
                            input { class: "input input-bordered hud-input w-full", r#type: "password", value: "{new_email_password}",
                                oninput: move |e| new_email_password.set(e.value()),
                                placeholder: "邮箱密码或授权码" }
                        }
                        div { class: "form-control w-full",
                            label { class: "label", span { class: "label-text font-medium", "发件地址 *" } }
                            input { class: "input input-bordered hud-input w-full", value: "{new_email_from}",
                                oninput: move |e| new_email_from.set(e.value()),
                                placeholder: "noreply@example.com" }
                        }
                        div { class: "form-control w-full",
                            label { class: "label", span { class: "label-text font-medium", "收件地址 *" } }
                            input { class: "input input-bordered hud-input w-full", value: "{new_email_to}",
                                oninput: move |e| new_email_to.set(e.value()),
                                placeholder: "接收通知的邮箱" }
                        }
                    }

                    // ===== Slack 配置 =====
                    if is_slack_type {
                        div { class: "divider text-sm font-medium m-0", "Slack 应用配置" }
                        div { class: "form-control w-full",
                            label { class: "label", span { class: "label-text font-medium", "Bot Token *" } }
                            input { class: "input input-bordered hud-input w-full", r#type: "password", value: "{new_slack_bot_token}",
                                oninput: move |e| new_slack_bot_token.set(e.value()),
                                placeholder: "xoxb-xxxx-xxxx-xxxx" }
                        }
                        div { class: "form-control w-full",
                            label { class: "label", span { class: "label-text font-medium", "Channel ID *" } }
                            input { class: "input input-bordered hud-input w-full", value: "{new_slack_channel_id}",
                                oninput: move |e| new_slack_channel_id.set(e.value()),
                                placeholder: "C1234567890" }
                        }
                    }

                    // ===== Webhook 配置 =====
                    if is_webhook_type {
                        div { class: "divider text-sm font-medium m-0", "Webhook 配置" }
                        div { class: "form-control w-full",
                            label { class: "label", span { class: "label-text font-medium", "Webhook URL *" } }
                            input { class: "input input-bordered hud-input w-full", value: "{new_webhook_url}",
                                oninput: move |e| new_webhook_url.set(e.value()),
                                placeholder: "https://..." }
                        }
                        div { class: "form-control w-full",
                            label { class: "label", span { class: "label-text font-medium", "HTTP 方法" } }
                            select { class: "select select-bordered hud-input w-full", value: "{new_webhook_method}",
                                onchange: move |e| new_webhook_method.set(e.value()),
                                option { value: "", "POST（默认）" }
                                option { value: "GET", "GET" }
                                option { value: "POST", "POST" }
                                option { value: "PUT", "PUT" }
                            }
                        }
                        div { class: "form-control w-full",
                            label { class: "label", span { class: "label-text", "请求体模板（JSON，可选）" } }
                            {
                                let ph = r#"{"text": "{{message}}"}"#;
                                rsx! {
                                    textarea { class: "textarea textarea-bordered hud-input w-full font-mono text-xs", rows: 3,
                                        value: "{new_webhook_body_template}",
                                        oninput: move |e| new_webhook_body_template.set(e.value()),
                                        placeholder: "{ph}" }
                                }
                            }
                        }
                        div { class: "form-control w-full",
                            label { class: "label", span { class: "label-text", "Access Token（可选）" } }
                            input { class: "input input-bordered hud-input w-full", value: "{new_access_token}",
                                oninput: move |e| new_access_token.set(e.value()),
                                placeholder: "Bearer token 等，用于认证" }
                        }
                        div { class: "form-control w-full",
                            label { class: "label", span { class: "label-text", "签名密钥（可选）" } }
                            input { class: "input input-bordered hud-input w-full", value: "{new_secret}",
                                oninput: move |e| new_secret.set(e.value()),
                                placeholder: "用于验证 webhook 请求签名" }
                        }
                    }
                }
            }

            ConfirmDialog {
                show: show_delete_confirm(),
                title: "确认删除".to_string(),
                message: "确定删除此消息渠道？此操作不可撤销。若为飞书渠道，其监听连接将一并停止。".to_string(),
                on_confirm: move |_| {
                    let id = pending_delete_id();
                    show_delete_confirm.set(false);
                    spawn(async move {
                        if let Err(e) = delete_message_channel(&id).await {
                            toast.error(format!("删除失败: {}", e));
                        } else {
                            match list_message_channels().await {
                                Ok(list) => channels.set(list.channels),
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
    fn test_validate_empty_name_rejected() {
        let result = validate_create_channel_form("  ", false, "");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_non_lark_requires_name_only() {
        assert!(validate_create_channel_form("webhook渠道", false, "").is_ok());
    }

    #[test]
    fn test_validate_lark_requires_credential() {
        let result = validate_create_channel_form("飞书渠道", true, "  ");
        assert_eq!(
            result.err(),
            Some("飞书渠道必须选择应用凭证（请先到设置页绑定飞书应用）")
        );
    }

    #[test]
    fn test_validate_lark_ok_with_credential() {
        assert!(validate_create_channel_form("飞书渠道", true, "cred-1").is_ok());
    }

    #[test]
    fn test_none_if_empty() {
        assert_eq!(none_if_empty("".to_string()), None);
        assert_eq!(none_if_empty("  ".to_string()), None);
        assert_eq!(none_if_empty("x".to_string()), Some("x".to_string()));
    }
}
