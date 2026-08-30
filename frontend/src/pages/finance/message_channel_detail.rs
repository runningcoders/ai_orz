//! 消息渠道详情页 - 展示详情 + 启用/禁用 + 测试连接 + 删除 + 飞书配置编辑
//!
//! 二期凭证引用模式：飞书渠道只存凭证引用（lark_credential_id），
//! 凭证本身在身份凭证页（/finance/identity）飞书区块管理；详情页展示集成状态卡（凭证名 + 用户授权徽标 + 身份模式 + 跳身份凭证页）。

use crate::components::hud::HudPanel;
use crate::utils::status::*;
use dioxus::prelude::*;
use dioxus_router::{Link, use_navigator};

use crate::api::finance::{
    delete_message_channel, get_message_channel, test_message_channel, update_message_channel,
    update_message_channel_status,
};
use crate::api::lark_integration::get_lark_integration_status;
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{
    CreateEmailChannelConfig, CreateLarkChannelConfig, CreateMessageChannelConfig,
    CreateSlackChannelConfig, CreateWebhookChannelConfig, CreateWechatChannelConfig,
    LarkCredentialSnapshot, LarkUserAuthSnapshot, UpdateMessageChannelRequest,
    UpdateMessageChannelStatusRequest,
};
use common::enums::{ChannelStatus, ChannelType};

#[component]
pub fn FinanceMessageChannelDetail(id: String) -> Element {
    // 方案 B：订阅路由并把 id 同步到响应式 rid，use_resource 绑定 rid，
    // 拉取仅在 id 变化时触发；飞书集成状态为全局数据，独立 resource
    let route = dioxus_router::use_route::<crate::pages::Route>();
    let mut rid = use_signal(String::new);
    if let crate::pages::Route::FinanceMessageChannelDetail { id: route_id } = &route
        && *rid.peek() != *route_id
    {
        rid.set(route_id.clone());
    }
    let toast = use_toast();
    let navigator = use_navigator();

    let mut channel_res = use_resource(move || {
        let id = rid();
        async move { get_message_channel(&id).await }
    });
    let mut toggling = use_signal(|| false);
    let mut testing = use_signal(|| false);
    let mut show_delete_confirm = use_signal(|| false);

    // ===== 配置编辑（通用） =====
    let mut show_edit_modal = use_signal(|| false);
    let mut edit_channel_name = use_signal(String::new);
    let mut edit_agent_id = use_signal(String::new);
    let mut edit_credential_id = use_signal(String::new);
    let mut edit_identity_mode = use_signal(String::new);
    let mut edit_open_id = use_signal(String::new);
    let mut edit_user_name = use_signal(String::new);
    let mut edit_listen_inbound = use_signal(|| true);
    // WeChat
    let mut edit_wechat_open_id = use_signal(String::new);
    // Email
    let mut edit_email_smtp_host = use_signal(String::new);
    let mut edit_email_smtp_port = use_signal(|| 587u16);
    let mut edit_email_username = use_signal(String::new);
    let mut edit_email_from_address = use_signal(String::new);
    let mut edit_email_to_address = use_signal(String::new);
    // Slack
    let mut edit_slack_channel_id = use_signal(String::new);
    // Webhook
    let mut edit_webhook_method = use_signal(String::new);
    let mut edit_webhook_body_template = use_signal(String::new);
    let mut saving = use_signal(|| false);

    // ===== 飞书集成快照（凭证下拉 + 用户授权徽标） =====
    let mut lark_credentials = use_signal(Vec::<LarkCredentialSnapshot>::new);
    let mut lark_user_auth = use_signal(LarkUserAuthSnapshot::default);

    // 飞书集成状态为全局数据，独立 resource，结果同步到信号供渲染使用
    let lark_res = use_resource(move || async { get_lark_integration_status().await });
    use_effect(move || {
        if let Some(Ok(status)) = lark_res.read().as_ref() {
            lark_credentials.set(status.credentials.clone());
            lark_user_auth.set(status.user_auth.clone());
        }
    });

    let mut on_toggle = {
        let id = id.clone();
        move |new_status: ChannelStatus| {
            let id = id.clone();
            toggling.set(true);
            spawn(async move {
                match update_message_channel_status(UpdateMessageChannelStatusRequest {
                    id: id.clone(),
                    status: new_status,
                })
                .await
                {
                    Ok(_) => {
                        toast.success(if new_status == ChannelStatus::Active {
                            "已启用"
                        } else {
                            "已禁用"
                        });
                        match get_message_channel(&id).await {
                            Ok(c) => channel_res.set(Some(Ok(c))),
                            Err(e) => toast.error(format!("刷新失败: {}", e)),
                        }
                    }
                    Err(e) => toast.error(&e),
                }
                toggling.set(false);
            });
        }
    };

    let on_test = {
        let id = id.clone();
        move |_| {
            let id = id.clone();
            testing.set(true);
            spawn(async move {
                match test_message_channel(&id).await {
                    Ok(resp) => {
                        if resp.success {
                            toast.success("连接测试通过");
                        } else {
                            toast.error(format!("连接失败: {}", resp.error.unwrap_or_default()));
                        }
                    }
                    Err(e) => toast.error(format!("测试失败: {}", e)),
                }
                testing.set(false);
            });
        }
    };

    let on_delete = {
        let id = id.clone();
        move |_| {
            let id = id.clone();
            show_delete_confirm.set(false);
            spawn(async move {
                match delete_message_channel(&id).await {
                    Ok(_) => {
                        toast.success("已删除");
                        let _ = navigator.push("/finance/message-channels".to_string());
                    }
                    Err(e) => toast.error(format!("删除失败: {}", e)),
                }
            });
        }
    };

    // 打开编辑弹窗：预填当前所有类型字段
    let on_open_edit = {
        move |_| {
            let current = channel_res
                .read()
                .as_ref()
                .and_then(|r| r.as_ref().ok())
                .cloned();
            if let Some(c) = &current {
                edit_channel_name.set(c.channel_name.clone());
                edit_agent_id.set(c.agent_id.clone().unwrap_or_default());
                // 从嵌套 config 读取
                let cfg = c.config.as_ref();
                edit_credential_id.set(
                    cfg.and_then(|c| c.lark.as_ref())
                        .and_then(|l| l.credential_id.clone())
                        .unwrap_or_default(),
                );
                edit_identity_mode.set(
                    cfg.and_then(|c| c.lark.as_ref())
                        .and_then(|l| l.identity_mode.clone())
                        .unwrap_or_default(),
                );
                edit_open_id.set(
                    cfg.and_then(|c| c.lark.as_ref())
                        .and_then(|l| l.open_id.clone())
                        .unwrap_or_default(),
                );
                edit_user_name.set(
                    cfg.and_then(|c| c.lark.as_ref())
                        .and_then(|l| l.user_name.clone())
                        .unwrap_or_default(),
                );
                edit_listen_inbound.set(
                    cfg.and_then(|c| c.lark.as_ref())
                        .map(|l| l.listen_inbound)
                        .unwrap_or(true),
                );
                edit_wechat_open_id.set(
                    cfg.and_then(|c| c.wechat.as_ref())
                        .and_then(|w| w.open_id.clone())
                        .unwrap_or_default(),
                );
                edit_email_smtp_host.set(
                    cfg.and_then(|c| c.email.as_ref())
                        .and_then(|e| e.smtp_host.clone())
                        .unwrap_or_default(),
                );
                edit_email_smtp_port.set(
                    cfg.and_then(|c| c.email.as_ref())
                        .and_then(|e| e.smtp_port)
                        .unwrap_or(587),
                );
                edit_email_username.set(
                    cfg.and_then(|c| c.email.as_ref())
                        .and_then(|e| e.username.clone())
                        .unwrap_or_default(),
                );
                edit_email_from_address.set(
                    cfg.and_then(|c| c.email.as_ref())
                        .and_then(|e| e.from_address.clone())
                        .unwrap_or_default(),
                );
                edit_email_to_address.set(
                    cfg.and_then(|c| c.email.as_ref())
                        .and_then(|e| e.to_address.clone())
                        .unwrap_or_default(),
                );
                edit_slack_channel_id.set(
                    cfg.and_then(|c| c.slack.as_ref())
                        .and_then(|s| s.channel_id.clone())
                        .unwrap_or_default(),
                );
                edit_webhook_method.set(
                    cfg.and_then(|c| c.webhook.as_ref())
                        .and_then(|w| w.method.clone())
                        .unwrap_or_default(),
                );
                edit_webhook_body_template.set(
                    cfg.and_then(|c| c.webhook.as_ref())
                        .and_then(|w| w.body_template.clone())
                        .unwrap_or_default(),
                );
            }
            show_edit_modal.set(true);
        }
    };

    let on_save_edit = {
        let id = id.clone();
        move |_| {
            let id = id.clone();
            let current = channel_res
                .read()
                .as_ref()
                .and_then(|r| r.as_ref().ok())
                .cloned();
            let channel_type = current
                .as_ref()
                .map(|c| c.channel_type)
                .unwrap_or(ChannelType::Lark);
            saving.set(true);
            spawn(async move {
                let channel_name = edit_channel_name();
                let agent_id = edit_agent_id();
                if channel_name.trim().is_empty() {
                    toast.error("渠道名称不能为空");
                    saving.set(false);
                    return;
                }
                let credential_id = edit_credential_id();
                let identity_mode = edit_identity_mode();
                let open_id = edit_open_id();
                let user_name = edit_user_name();
                let wechat_open_id = edit_wechat_open_id();
                let email_smtp_host = edit_email_smtp_host();
                let email_smtp_port = edit_email_smtp_port();
                let email_username = edit_email_username();
                let email_from_address = edit_email_from_address();
                let email_to_address = edit_email_to_address();
                let slack_channel_id = edit_slack_channel_id();
                let webhook_method = edit_webhook_method();
                let webhook_body_template = edit_webhook_body_template();

                let req = UpdateMessageChannelRequest {
                    id: id.clone(),
                    user_id: None,
                    agent_id: if agent_id.trim().is_empty() {
                        None
                    } else {
                        Some(agent_id)
                    },
                    channel_type: None,
                    channel_name: Some(channel_name),
                    webhook_url: None,
                    access_token: None,
                    secret: None,
                    config: Some(CreateMessageChannelConfig {
                        lark: if channel_type == ChannelType::Lark {
                            Some(CreateLarkChannelConfig {
                                credential_id: if credential_id.trim().is_empty() {
                                    None
                                } else {
                                    Some(credential_id)
                                },
                                identity_mode: if identity_mode.trim().is_empty() {
                                    None
                                } else {
                                    Some(identity_mode)
                                },
                                open_id: if open_id.trim().is_empty() {
                                    None
                                } else {
                                    Some(open_id)
                                },
                                user_name: if user_name.trim().is_empty() {
                                    None
                                } else {
                                    Some(user_name)
                                },
                                listen_inbound: Some(edit_listen_inbound()),
                            })
                        } else {
                            None
                        },
                        wechat: if channel_type == ChannelType::Wechat {
                            Some(CreateWechatChannelConfig {
                                app_id: None,
                                app_secret: None,
                                open_id: if wechat_open_id.trim().is_empty() {
                                    None
                                } else {
                                    Some(wechat_open_id)
                                },
                            })
                        } else {
                            None
                        },
                        email: if channel_type == ChannelType::Email {
                            Some(CreateEmailChannelConfig {
                                smtp_host: if email_smtp_host.trim().is_empty() {
                                    None
                                } else {
                                    Some(email_smtp_host)
                                },
                                smtp_port: Some(email_smtp_port),
                                username: if email_username.trim().is_empty() {
                                    None
                                } else {
                                    Some(email_username)
                                },
                                password: None,
                                from_address: if email_from_address.trim().is_empty() {
                                    None
                                } else {
                                    Some(email_from_address)
                                },
                                to_address: if email_to_address.trim().is_empty() {
                                    None
                                } else {
                                    Some(email_to_address)
                                },
                            })
                        } else {
                            None
                        },
                        slack: if channel_type == ChannelType::Slack {
                            Some(CreateSlackChannelConfig {
                                bot_token: None,
                                channel_id: if slack_channel_id.trim().is_empty() {
                                    None
                                } else {
                                    Some(slack_channel_id)
                                },
                            })
                        } else {
                            None
                        },
                        webhook: if channel_type == ChannelType::Webhook {
                            Some(CreateWebhookChannelConfig {
                                method: if webhook_method.trim().is_empty() {
                                    None
                                } else {
                                    Some(webhook_method)
                                },
                                body_template: if webhook_body_template.trim().is_empty() {
                                    None
                                } else {
                                    Some(webhook_body_template)
                                },
                            })
                        } else {
                            None
                        },
                    }),
                };
                match update_message_channel(req).await {
                    Ok(_) => {
                        toast.success("已保存，建议重新运行连接测试");
                        show_edit_modal.set(false);
                        match get_message_channel(&id).await {
                            Ok(c) => channel_res.set(Some(Ok(c))),
                            Err(e) => toast.error(format!("刷新失败: {}", e)),
                        }
                    }
                    Err(e) => toast.error(format!("保存失败: {}", e)),
                }
                saving.set(false);
            });
        }
    };

    let channel_data = channel_res
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned();
    // 加载失败时（Some(Err)）取出错误文案，供下方 else 分支显式展示
    let channel_err_msg = channel_res
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().err())
        .map(|e| format!("加载失败: {}", e));
    let credentials_list = lark_credentials.read().clone();
    let user_auth = lark_user_auth.read().clone();
    let edit_credential_value = edit_credential_id();
    let edit_mode_value = edit_identity_mode();
    let user_auth_suffix = user_auth
        .user_name
        .as_deref()
        .map(|n| format!("（{}）", n))
        .unwrap_or_default();

    rsx! {
        AppLayout {
            div { class: "mb-6 flex items-center justify-between",
                h1 { class: "text-2xl font-bold", "消息渠道详情" }
                Link { class: "btn hud-btn btn-ghost", to: crate::pages::Route::FinanceMessageChannels {}, "← 返回列表" }
            }
            if channel_res.read().as_ref().is_none() {
                Loading {}
            } else if let Some(c) = channel_data.clone() {
                {
                    let cfg = c.config.as_ref();
                    let lark_cfg = cfg.and_then(|c| c.lark.as_ref());
                    let wechat_cfg = cfg.and_then(|c| c.wechat.as_ref());
                    let email_cfg = cfg.and_then(|c| c.email.as_ref());
                    let slack_cfg = cfg.and_then(|c| c.slack.as_ref());
                    let webhook_cfg = cfg.and_then(|c| c.webhook.as_ref());
                    rsx! {
                HudPanel { signal: Some(true),
                    title: Some(c.channel_name.clone()),
                    actions: Some(rsx!{
                        if c.channel_type == ChannelType::Lark {
                            button {
                                class: "btn hud-btn btn-ghost btn-sm",
                                onclick: on_open_edit,
                                "✏️ 编辑飞书配置"
                            }
                        } else {
                            button {
                                class: "btn hud-btn btn-ghost btn-sm",
                                onclick: on_open_edit,
                                "✏️ 编辑"
                            }
                        }
                        button {
                            class: "btn hud-btn btn-ghost btn-sm",
                            disabled: toggling(),
                            onclick: move |_| on_toggle(if c.status == ChannelStatus::Active { ChannelStatus::Disabled } else { ChannelStatus::Active }),
                            if c.status == ChannelStatus::Active { "🚫 禁用" } else { "✅ 启用" }
                        }
                        button {
                            class: "btn hud-btn btn-ghost btn-sm",
                            disabled: testing(),
                            onclick: on_test,
                            if testing() { "测试中..." } else { "🔌 测试连接" }
                        }
                        button {
                            class: "btn hud-btn btn-error btn-sm",
                            onclick: move |_| show_delete_confirm.set(true),
                            "🗑 删除"
                        }
                    }),
                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                            div {
                                div { class: "text-sm text-base-content/60", "渠道类型" }
                                div { class: "font-mono", "{channel_type_text(c.channel_type)}" }
                            }
                            div {
                                div { class: "text-sm text-base-content/60", "状态" }
                                div { span { class: "{channel_status_badge(c.status)}", "{status_text(c.status)}" } }
                            }
                            if let Some(url) = &c.webhook_url {
                                div {
                                    div { class: "text-sm text-base-content/60", "Webhook URL" }
                                    div { class: "font-mono text-sm break-all", "{url}" }
                                }
                            }
                            if let Some(aid) = &c.agent_id {
                                div {
                                    div { class: "text-sm text-base-content/60", "绑定 Agent" }
                                    div { class: "font-mono", "{aid}" }
                                }
                            }
                            if c.channel_type == ChannelType::Lark {
                                div {
                                    div { class: "text-sm text-base-content/60", "应用凭证" }
                                    div {
                                        if let Some(name) = lark_cfg.and_then(|l| l.credential_name.as_deref()) {
                                            span { class: "badge orz-tag badge-sm", "{name}" }
                                        } else {
                                            span { class: "badge hud-badge badge-warning badge-sm", "未绑定凭证" }
                                        }
                                    }
                                }
                                div {
                                    div { class: "text-sm text-base-content/60", "身份模式" }
                                    div {
                                        span { class: "badge orz-tag badge-sm", "{identity_mode_text(lark_cfg.and_then(|l| l.identity_mode.as_deref()))}" }
                                    }
                                }
                                div {
                                    div { class: "text-sm text-base-content/60", "用户 Open ID" }
                                    div { class: "font-mono",
                                        if let Some(open_id) = lark_cfg.and_then(|l| l.open_id.as_ref()) { "{open_id}" } else { "未配置" }
                                    }
                                }
                                div {
                                    div { class: "text-sm text-base-content/60", "用户昵称" }
                                    div {
                                        if let Some(name) = lark_cfg.and_then(|l| l.user_name.as_ref()) { "{name}" } else { "-" }
                                    }
                                }
                                div {
                                    div { class: "text-sm text-base-content/60", "入站监听" }
                                    div {
                                        if lark_cfg.map(|l| l.listen_inbound).unwrap_or(true) {
                                            span { class: "badge hud-badge badge-success badge-sm", "开启" }
                                        } else {
                                            span { class: "badge hud-badge badge-ghost badge-sm", "关闭（仅出站/lark_cli）" }
                                        }
                                    }
                                }
                            }
                            if c.channel_type == ChannelType::Lark {
                                // ===== 集成状态卡（互为详情：跳身份凭证页飞书区块） =====
                                div { class: "md:col-span-2 border border-base-300 rounded-lg p-3 flex items-center justify-between gap-3 flex-wrap",
                                    div { class: "flex items-center gap-2 flex-wrap",
                                        span { class: "text-sm text-base-content/60", "飞书集成" }
                                        if let Some(name) = lark_cfg.and_then(|l| l.credential_name.as_deref()) {
                                            span { class: "badge orz-tag badge-sm", "凭证：{name}" }
                                        } else {
                                            span { class: "badge hud-badge badge-warning badge-sm", "凭证未绑定" }
                                        }
                                        if user_auth.logged_in {
                                            span { class: "{auth_state_badge(\"用户已授权\")}",
                                                "用户已授权{user_auth_suffix}"
                                            }
                                        } else {
                                            span { class: "{auth_state_badge(\"用户未授权\")}", "用户未授权" }
                                        }
                                    }
                                    Link { class: "btn hud-btn btn-ghost btn-sm", to: crate::pages::Route::FinanceIdentity {}, "管理身份凭证 →" }
                                }
                            }
                            if c.channel_type == ChannelType::Wechat {
                                div {
                                    div { class: "text-sm text-base-content/60", "微信 Open ID" }
                                    div { class: "font-mono",
                                        if let Some(id) = wechat_cfg.and_then(|w| w.open_id.as_ref()) { "{id}" } else { "未配置" }
                                    }
                                }
                            }
                            if c.channel_type == ChannelType::Email {
                                div {
                                    div { class: "text-sm text-base-content/60", "SMTP 服务器" }
                                    {
                                        let smtp = match (email_cfg.and_then(|e| e.smtp_host.as_ref()), email_cfg.and_then(|e| e.smtp_port)) {
                                            (Some(h), Some(p)) => format!("{h}:{p}"),
                                            (Some(h), None) => h.clone(),
                                            _ => "未配置".to_string(),
                                        };
                                        rsx! { div { class: "font-mono", "{smtp}" } }
                                    }
                                }
                                div {
                                    div { class: "text-sm text-base-content/60", "用户名" }
                                    div { class: "font-mono",
                                        if let Some(u) = email_cfg.and_then(|e| e.username.as_ref()) { "{u}" } else { "未配置" }
                                    }
                                }
                                div {
                                    div { class: "text-sm text-base-content/60", "发件地址" }
                                    div { class: "font-mono",
                                        if let Some(a) = email_cfg.and_then(|e| e.from_address.as_ref()) { "{a}" } else { "未配置" }
                                    }
                                }
                                div {
                                    div { class: "text-sm text-base-content/60", "收件地址" }
                                    div { class: "font-mono",
                                        if let Some(a) = email_cfg.and_then(|e| e.to_address.as_ref()) { "{a}" } else { "未配置" }
                                    }
                                }
                            }
                            if c.channel_type == ChannelType::Slack {
                                div {
                                    div { class: "text-sm text-base-content/60", "Channel ID" }
                                    div { class: "font-mono",
                                        if let Some(id) = slack_cfg.and_then(|s| s.channel_id.as_ref()) { "{id}" } else { "未配置" }
                                    }
                                }
                            }
                            if c.channel_type == ChannelType::Webhook {
                                if let Some(url) = &c.webhook_url {
                                    div {
                                        div { class: "text-sm text-base-content/60", "Webhook URL" }
                                        div { class: "font-mono text-sm break-all", "{url}" }
                                    }
                                }
                                if let Some(method) = webhook_cfg.and_then(|w| w.method.as_ref()) {
                                    div {
                                        div { class: "text-sm text-base-content/60", "HTTP 方法" }
                                        div { span { class: "badge orz-tag badge-sm", "{method}" } }
                                    }
                                }
                                if let Some(tmpl) = webhook_cfg.and_then(|w| w.body_template.as_ref()) {
                                    div { class: "md:col-span-2",
                                        div { class: "text-sm text-base-content/60", "请求体模板" }
                                        pre { class: "font-mono text-xs bg-base-200 p-2 rounded",
                                            style: "white-space: pre-wrap; word-break: break-word;",
                                            "{tmpl}"
                                        }
                                    }
                                }
                            }
                            div {
                                div { class: "text-sm text-base-content/60", "凭据状态" }
                                div { class: "flex gap-2 flex-wrap",
                                    if c.has_access_token { span { class: "badge hud-badge badge-success badge-sm", "Access Token" } }
                                    if c.has_secret { span { class: "badge hud-badge badge-success badge-sm", "Secret" } }
                                    if c.has_config_secret { span { class: "badge hud-badge badge-success badge-sm", "Config Secret" } }
                                    if !c.has_access_token && !c.has_secret && !c.has_config_secret {
                                        span { class: "text-base-content/50 text-sm", "无凭据" }
                                    }
                                }
                            }
                            if let Some(last_push) = c.last_pushed_at {
                                div {
                                    div { class: "text-sm text-base-content/60", "最后推送" }
                                    div { class: "font-mono", "{crate::utils::format_datetime(last_push)}" }
                                }
                            }
                            if let Some(err) = &c.last_error {
                                div { class: "md:col-span-2",
                                    div { class: "text-sm text-error mb-1", "最后推送错误" }
                                    pre {
                                        class: "font-mono text-xs bg-error/10 p-2 rounded",
                                        style: "white-space: pre-wrap; word-break: break-word;",
                                        "{err}"
                                    }
                                }
                            }
                            div {
                                div { class: "text-sm text-base-content/60", "创建时间" }
                                div { class: "font-mono", "{crate::utils::format_datetime(c.created_at)}" }
                            }
                            div {
                                div { class: "text-sm text-base-content/60", "更新时间" }
                                div { class: "font-mono", "{crate::utils::format_datetime(c.updated_at)}" }
                            }
                        }
                }
            }
            }
            } else {
                EmptyState { icon: "❓".to_string(), message: channel_err_msg.clone().unwrap_or_else(|| "消息渠道不存在或已被删除".to_string()) }
            }

            ConfirmDialog {
                show: show_delete_confirm(),
                title: "确认删除".to_string(),
                message: "确定删除此消息渠道？此操作不可撤销。若为飞书渠道，其监听连接将一并停止。".to_string(),
                on_confirm: on_delete,
                on_cancel: move |_| show_delete_confirm.set(false),
            }

            Modal {
                title: {
                    let ct = channel_data.as_ref().map(|c| c.channel_type).unwrap_or(ChannelType::Lark);
                    format!("编辑 {} 渠道配置", channel_type_text(ct))
                },
                show: show_edit_modal(),
                on_close: move |_| show_edit_modal.set(false),
                footer: rsx! {
                    button { class: "btn hud-btn btn-ghost", onclick: move |_| show_edit_modal.set(false), "取消" }
                    button { class: "btn hud-btn btn-primary", disabled: saving(), onclick: on_save_edit,
                        if saving() { "保存中..." } else { "保存" }
                    }
                },
                {
                    let ct = channel_data.as_ref().map(|c| c.channel_type).unwrap_or(ChannelType::Lark);
                    rsx! {
                        div { class: "space-y-4",
                            div { class: "form-control w-full",
                                label { class: "label",
                                    span { class: "label-text font-medium", "渠道名称 *" }
                                }
                                input { class: "input input-bordered hud-input w-full", value: "{edit_channel_name}",
                                    oninput: move |e| edit_channel_name.set(e.value()), placeholder: "自定义渠道名称" }
                            }
                            div { class: "form-control w-full",
                                label { class: "label",
                                    span { class: "label-text font-medium", "关联 Agent ID" }
                                }
                                input { class: "input input-bordered hud-input w-full font-mono", value: "{edit_agent_id}",
                                    oninput: move |e| edit_agent_id.set(e.value()), placeholder: "留空表示不关联" }
                            }
                            if ct == ChannelType::Lark {
                                div { class: "hud-divider divider", "飞书专属配置" }
                                div { class: "form-control w-full",
                                    label { class: "label",
                                        span { class: "label-text font-medium", "应用凭证 *" }
                                    }
                                    select { class: "select select-bordered hud-input w-full", value: "{edit_credential_value}",
                                        onchange: move |e| edit_credential_id.set(e.value()),
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
                                        span { class: "label-text-alt", "凭证在「财务管理 → 身份凭证」管理；更换凭证将触发监听重建联" }
                                    }
                                }
                                div { class: "form-control w-full",
                                    label { class: "label",
                                        span { class: "label-text font-medium", "身份模式" }
                                    }
                                    select { class: "select select-bordered hud-input w-full", value: "{edit_mode_value}",
                                        onchange: move |e| edit_identity_mode.set(e.value()),
                                        option { value: "", "自动（auto）" }
                                        option { value: "bot", "应用身份（bot）" }
                                        option { value: "user", "用户身份（user）" }
                                    }
                                }
                                div { class: "form-control w-full",
                                    label { class: "label",
                                        span { class: "label-text", "用户 Open ID" }
                                    }
                                    input { class: "input input-bordered hud-input w-full font-mono", value: "{edit_open_id}",
                                        oninput: move |e| edit_open_id.set(e.value()), placeholder: "ou_xxx" }
                                }
                                div { class: "form-control w-full",
                                    label { class: "label",
                                        span { class: "label-text", "用户昵称" }
                                    }
                                    input { class: "input input-bordered hud-input w-full", value: "{edit_user_name}",
                                        oninput: move |e| edit_user_name.set(e.value()), placeholder: "可选" }
                                }
                                div { class: "form-control",
                                    label { class: "label cursor-pointer justify-start gap-3",
                                        input { class: "toggle toggle-primary", r#type: "checkbox", checked: edit_listen_inbound(),
                                            onchange: move |_| edit_listen_inbound.set(!edit_listen_inbound()) }
                                        span { class: "label-text",
                                            "入站监听（接收该应用的飞书私信消息；关闭后仅用于出站推送与 lark_cli 工具身份）"
                                        }
                                    }
                                }
                            }
                            if ct == ChannelType::Wechat {
                                div { class: "hud-divider divider", "微信专属配置" }
                                div { class: "form-control w-full",
                                    label { class: "label",
                                        span { class: "label-text", "微信 Open ID" }
                                    }
                                    input { class: "input input-bordered hud-input w-full font-mono", value: "{edit_wechat_open_id}",
                                        oninput: move |e| edit_wechat_open_id.set(e.value()), placeholder: "openid_xxx" }
                                }
                            }
                            if ct == ChannelType::Email {
                                div { class: "hud-divider divider", "邮件专属配置" }
                                div { class: "form-control w-full",
                                    label { class: "label",
                                        span { class: "label-text font-medium", "SMTP 服务器 *" }
                                    }
                                    input { class: "input input-bordered hud-input w-full", value: "{edit_email_smtp_host}",
                                        oninput: move |e| edit_email_smtp_host.set(e.value()), placeholder: "smtp.example.com" }
                                }
                                div { class: "form-control w-full",
                                    label { class: "label",
                                        span { class: "label-text font-medium", "SMTP 端口 *" }
                                    }
                                    input { class: "input input-bordered hud-input w-full", value: "{edit_email_smtp_port.to_string()}",
                                        oninput: move |e| {
                                            if let Ok(v) = e.value().parse::<u16>() { edit_email_smtp_port.set(v); }
                                        }, placeholder: "587" }
                                }
                                div { class: "form-control w-full",
                                    label { class: "label",
                                        span { class: "label-text", "用户名" }
                                    }
                                    input { class: "input input-bordered hud-input w-full font-mono", value: "{edit_email_username}",
                                        oninput: move |e| edit_email_username.set(e.value()), placeholder: "user@example.com" }
                                }
                                div { class: "form-control w-full",
                                    label { class: "label",
                                        span { class: "label-text", "发件地址" }
                                    }
                                    input { class: "input input-bordered hud-input w-full font-mono", value: "{edit_email_from_address}",
                                        oninput: move |e| edit_email_from_address.set(e.value()), placeholder: "from@example.com" }
                                }
                                div { class: "form-control w-full",
                                    label { class: "label",
                                        span { class: "label-text", "收件地址" }
                                    }
                                    input { class: "input input-bordered hud-input w-full font-mono", value: "{edit_email_to_address}",
                                        oninput: move |e| edit_email_to_address.set(e.value()), placeholder: "to@example.com" }
                                }
                            }
                            if ct == ChannelType::Slack {
                                div { class: "hud-divider divider", "Slack 专属配置" }
                                div { class: "form-control w-full",
                                    label { class: "label",
                                        span { class: "label-text", "Channel ID" }
                                    }
                                    input { class: "input input-bordered hud-input w-full font-mono", value: "{edit_slack_channel_id}",
                                        oninput: move |e| edit_slack_channel_id.set(e.value()), placeholder: "CXXXXXXXXXX" }
                                }
                            }
                            if ct == ChannelType::Webhook {
                                div { class: "hud-divider divider", "Webhook 专属配置" }
                                div { class: "form-control w-full",
                                    label { class: "label",
                                        span { class: "label-text", "HTTP 方法" }
                                    }
                                    select { class: "select select-bordered hud-input w-full", value: "{edit_webhook_method}",
                                        onchange: move |e| edit_webhook_method.set(e.value()),
                                        option { value: "", "选择方法" }
                                        option { value: "POST", "POST" }
                                        option { value: "GET", "GET" }
                                        option { value: "PUT", "PUT" }
                                        option { value: "DELETE", "DELETE" }
                                    }
                                }
                                div { class: "form-control w-full",
                                    label { class: "label",
                                        span { class: "label-text", "请求体模板" }
                                    }
                                    {
                                        let ph = r#"{"content": "{{message}}"}"#;
                                        rsx! {
                                            textarea { class: "textarea textarea-bordered hud-input w-full font-mono text-xs",
                                                value: "{edit_webhook_body_template}",
                                                oninput: move |e| edit_webhook_body_template.set(e.value()),
                                                placeholder: "{ph}" }
                                        }
                                    }
                                    label { class: "label",
                                        span { class: "label-text-alt", "支持 {{message}} 占位符，将被实际内容替换" }
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

fn identity_mode_text(mode: Option<&str>) -> &'static str {
    match mode {
        Some("bot") => "应用身份（bot）",
        Some("user") => "用户身份（user）",
        _ => "自动（auto）",
    }
}

fn channel_type_text(t: ChannelType) -> &'static str {
    match t {
        ChannelType::Lark => "飞书",
        ChannelType::Wechat => "微信",
        ChannelType::Slack => "Slack",
        ChannelType::Email => "邮件",
        ChannelType::Webhook => "Webhook",
        ChannelType::A2aCallback => "A2A 回调",
    }
}

fn status_text(s: ChannelStatus) -> &'static str {
    match s {
        ChannelStatus::Active => "启用",
        ChannelStatus::Disabled => "禁用",
        ChannelStatus::Deleted => "已删除",
    }
}

/// 渠道状态徽章（HUD 皮肤，单一事实源）。取代详情页原本 `class: "badge"` 缺 hud 皮肤的写法。
fn channel_status_badge(s: ChannelStatus) -> &'static str {
    match s {
        ChannelStatus::Active => "badge hud-badge badge-success",
        ChannelStatus::Disabled => "badge hud-badge badge-neutral",
        ChannelStatus::Deleted => "badge hud-badge badge-error",
    }
}
