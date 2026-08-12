//! 消息渠道详情页 - 展示详情 + 启用/禁用 + 测试连接 + 删除 + 飞书配置编辑
//!
//! 二期凭证引用模式：飞书渠道只存凭证引用（lark_credential_id），
//! 凭证本身在身份凭证页（/finance/identity）飞书区块管理；详情页展示集成状态卡（凭证名 + 用户授权徽标 + 身份模式 + 跳身份凭证页）。

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
    GetMessageChannelResponse, LarkCredentialSnapshot, LarkUserAuthSnapshot,
    UpdateMessageChannelRequest, UpdateMessageChannelStatusRequest,
};
use common::enums::{ChannelStatus, ChannelType};

#[component]
pub fn FinanceMessageChannelDetail(id: String) -> Element {
    let toast = use_toast();
    let navigator = use_navigator();

    let mut channel = use_signal(|| Option::<GetMessageChannelResponse>::None);
    let mut loading = use_signal(|| true);
    let mut toggling = use_signal(|| false);
    let mut testing = use_signal(|| false);
    let mut show_delete_confirm = use_signal(|| false);

    // ===== 飞书配置编辑 =====
    let mut show_edit_modal = use_signal(|| false);
    let mut edit_credential_id = use_signal(String::new);
    let mut edit_identity_mode = use_signal(String::new);
    let mut edit_open_id = use_signal(String::new);
    let mut edit_user_name = use_signal(String::new);
    let mut edit_listen_inbound = use_signal(|| true);
    let mut saving = use_signal(|| false);

    // ===== 飞书集成快照（凭证下拉 + 用户授权徽标） =====
    let mut lark_credentials = use_signal(Vec::<LarkCredentialSnapshot>::new);
    let mut lark_user_auth = use_signal(LarkUserAuthSnapshot::default);

    let id_for_effect = id.clone();
    use_effect(move || {
        loading.set(true);
        let id = id_for_effect.clone();
        spawn(async move {
            match get_message_channel(&id).await {
                Ok(c) => channel.set(Some(c)),
                Err(e) => toast.error(format!("加载失败: {}", e)),
            }
            loading.set(false);
        });
        spawn(async move {
            if let Ok(status) = get_lark_integration_status().await {
                lark_credentials.set(status.credentials);
                lark_user_auth.set(status.user_auth);
            }
        });
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
                            Ok(c) => channel.set(Some(c)),
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

    // 打开编辑弹窗：预填当前凭证引用与路由字段（凭证本体在设置页管理）
    let on_open_edit = {
        let id = id.clone();
        move |_| {
            let current = channel.read().clone();
            if let Some(c) = &current {
                edit_credential_id.set(c.lark_credential_id.clone().unwrap_or_default());
                edit_identity_mode.set(c.lark_identity_mode.clone().unwrap_or_default());
                edit_open_id.set(c.lark_open_id.clone().unwrap_or_default());
                edit_user_name.set(c.lark_user_name.clone().unwrap_or_default());
                edit_listen_inbound.set(c.lark_listen_inbound);
            }
            show_edit_modal.set(true);
            let id = id.clone();
            // 保留引用避免未使用告警
            let _ = id;
        }
    };

    let on_save_edit = {
        let id = id.clone();
        move |_| {
            let id = id.clone();
            saving.set(true);
            spawn(async move {
                let credential_id = edit_credential_id();
                let identity_mode = edit_identity_mode();
                let open_id = edit_open_id();
                let user_name = edit_user_name();
                if credential_id.trim().is_empty() {
                    toast.error("必须选择应用凭证");
                    saving.set(false);
                    return;
                }
                let req = UpdateMessageChannelRequest {
                    id: id.clone(),
                    user_id: None,
                    agent_id: None,
                    channel_type: None,
                    channel_name: None,
                    webhook_url: None,
                    access_token: None,
                    secret: None,
                    lark_credential_id: Some(credential_id),
                    lark_identity_mode: if identity_mode.trim().is_empty() {
                        None
                    } else {
                        Some(identity_mode)
                    },
                    lark_open_id: if open_id.trim().is_empty() {
                        None
                    } else {
                        Some(open_id)
                    },
                    lark_user_name: if user_name.trim().is_empty() {
                        None
                    } else {
                        Some(user_name)
                    },
                    lark_listen_inbound: Some(edit_listen_inbound()),
                    wechat_app_id: None,
                    wechat_app_secret: None,
                    wechat_open_id: None,
                    email_smtp_host: None,
                    email_smtp_port: None,
                    email_username: None,
                    email_password: None,
                    email_from_address: None,
                    email_to_address: None,
                    slack_bot_token: None,
                    slack_channel_id: None,
                    webhook_method: None,
                    webhook_body_template: None,
                };
                match update_message_channel(req).await {
                    Ok(_) => {
                        toast.success("已保存，建议重新运行连接测试");
                        show_edit_modal.set(false);
                        match get_message_channel(&id).await {
                            Ok(c) => channel.set(Some(c)),
                            Err(e) => toast.error(format!("刷新失败: {}", e)),
                        }
                    }
                    Err(e) => toast.error(format!("保存失败: {}", e)),
                }
                saving.set(false);
            });
        }
    };

    let channel_data = channel.read().clone();
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
                Link { class: "btn btn-ghost", to: crate::pages::Route::FinanceMessageChannels {}, "← 返回列表" }
            }
            if loading() {
                Loading {}
            } else if let Some(c) = channel_data {
                div { class: "card bg-base-100 shadow-md",
                    div { class: "card-body",
                        div { class: "flex justify-between items-center mb-4",
                            h2 { class: "card-title", "{c.channel_name}" }
                            div { class: "flex gap-2",
                                if c.channel_type == ChannelType::Lark {
                                    button {
                                        class: "btn btn-ghost btn-sm",
                                        onclick: on_open_edit,
                                        "✏️ 编辑飞书配置"
                                    }
                                }
                                button {
                                    class: "btn btn-ghost btn-sm",
                                    disabled: toggling(),
                                    onclick: move |_| on_toggle(if c.status == ChannelStatus::Active { ChannelStatus::Disabled } else { ChannelStatus::Active }),
                                    if c.status == ChannelStatus::Active { "🚫 禁用" } else { "✅ 启用" }
                                }
                                button {
                                    class: "btn btn-ghost btn-sm",
                                    disabled: testing(),
                                    onclick: on_test,
                                    if testing() { "测试中..." } else { "🔌 测试连接" }
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
                                div { class: "text-sm text-base-content/60", "渠道类型" }
                                div { class: "font-mono", "{channel_type_text(c.channel_type)}" }
                            }
                            div {
                                div { class: "text-sm text-base-content/60", "状态" }
                                div { span { class: "badge", "{status_text(c.status)}" } }
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
                                        if let Some(name) = &c.lark_credential_name {
                                            span { class: "badge badge-outline", "{name}" }
                                        } else {
                                            span { class: "badge badge-warning badge-sm", "未绑定凭证" }
                                        }
                                    }
                                }
                                div {
                                    div { class: "text-sm text-base-content/60", "身份模式" }
                                    div {
                                        span { class: "badge badge-ghost badge-sm", "{identity_mode_text(c.lark_identity_mode.as_deref())}" }
                                    }
                                }
                                div {
                                    div { class: "text-sm text-base-content/60", "用户 Open ID" }
                                    div { class: "font-mono",
                                        if let Some(open_id) = &c.lark_open_id { "{open_id}" } else { "未配置" }
                                    }
                                }
                                div {
                                    div { class: "text-sm text-base-content/60", "用户昵称" }
                                    div {
                                        if let Some(name) = &c.lark_user_name { "{name}" } else { "-" }
                                    }
                                }
                                div {
                                    div { class: "text-sm text-base-content/60", "入站监听" }
                                    div {
                                        if c.lark_listen_inbound {
                                            span { class: "badge badge-success badge-sm", "开启" }
                                        } else {
                                            span { class: "badge badge-ghost badge-sm", "关闭（仅出站/lark_cli）" }
                                        }
                                    }
                                }
                            }
                            if c.channel_type == ChannelType::Lark {
                                // ===== 集成状态卡（互为详情：跳身份凭证页飞书区块） =====
                                div { class: "md:col-span-2 border border-base-300 rounded-lg p-3 flex items-center justify-between gap-3 flex-wrap",
                                    div { class: "flex items-center gap-2 flex-wrap",
                                        span { class: "text-sm text-base-content/60", "飞书集成" }
                                        if let Some(name) = &c.lark_credential_name {
                                            span { class: "badge badge-outline badge-sm", "凭证：{name}" }
                                        } else {
                                            span { class: "badge badge-warning badge-sm", "凭证未绑定" }
                                        }
                                        if user_auth.logged_in {
                                            span { class: "badge badge-success badge-sm",
                                                "用户已授权{user_auth_suffix}"
                                            }
                                        } else {
                                            span { class: "badge badge-ghost badge-sm", "用户未授权" }
                                        }
                                    }
                                    Link { class: "btn btn-ghost btn-sm", to: crate::pages::Route::FinanceIdentity {}, "管理身份凭证 →" }
                                }
                            }
                            div {
                                div { class: "text-sm text-base-content/60", "凭据状态" }
                                div { class: "flex gap-2 flex-wrap",
                                    if c.has_access_token { span { class: "badge badge-success badge-sm", "Access Token" } }
                                    if c.has_secret { span { class: "badge badge-success badge-sm", "Secret" } }
                                    if c.has_config_secret { span { class: "badge badge-success badge-sm", "Config Secret" } }
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
            } else {
                EmptyState { icon: "❓".to_string(), message: "消息渠道不存在或已被删除".to_string() }
            }

            ConfirmDialog {
                show: show_delete_confirm(),
                title: "确认删除".to_string(),
                message: "确定删除此消息渠道？此操作不可撤销。若为飞书渠道，其监听连接将一并停止。".to_string(),
                on_confirm: on_delete,
                on_cancel: move |_| show_delete_confirm.set(false),
            }

            Modal {
                title: "编辑飞书配置".to_string(),
                show: show_edit_modal(),
                on_close: move |_| show_edit_modal.set(false),
                footer: rsx! {
                    button { class: "btn btn-ghost", onclick: move |_| show_edit_modal.set(false), "取消" }
                    button { class: "btn btn-primary", disabled: saving(), onclick: on_save_edit,
                        if saving() { "保存中..." } else { "保存" }
                    }
                },
                div { class: "space-y-4",
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "应用凭证 *" }
                        }
                        select { class: "select select-bordered w-full", value: "{edit_credential_value}",
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
                        select { class: "select select-bordered w-full", value: "{edit_mode_value}",
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
                        input { class: "input input-bordered w-full font-mono", value: "{edit_open_id}",
                            oninput: move |e| edit_open_id.set(e.value()), placeholder: "ou_xxx" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text", "用户昵称" }
                        }
                        input { class: "input input-bordered w-full", value: "{edit_user_name}",
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
