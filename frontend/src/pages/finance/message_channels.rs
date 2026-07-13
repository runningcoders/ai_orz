//! 消息渠道管理

use dioxus::prelude::*;

use crate::api::finance::{
    create_message_channel, delete_message_channel, list_message_channels,
    test_message_channel, update_message_channel_status,
};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, ErrorAlert, Loading, SuccessAlert};
use common::api::{CreateMessageChannelRequest, ListMessageChannelsResponseItem};
use common::enums::{ChannelStatus, ChannelType};

#[component]
pub fn FinanceMessageChannels() -> Element {
    let mut channels = use_signal(Vec::<ListMessageChannelsResponseItem>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut success = use_signal(String::new);
    let mut show_add_modal = use_signal(|| false);

    // 创建表单状态
    let mut new_name = use_signal(String::new);
    let mut new_type = use_signal(|| "0".to_string());
    let mut new_webhook_url = use_signal(String::new);
    let mut creating = use_signal(|| false);

    use_effect(move || {
        loading.set(true);
        spawn(async move {
            match list_message_channels().await {
                Ok(list) => channels.set(list.channels),
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    });

    let handle_create = move |_| {
        spawn(async move {
            if new_name().is_empty() {
                error.set("渠道名称不能为空".to_string());
                return;
            }
            creating.set(true);
            let channel_type = ChannelType::from_i32(new_type().parse::<i32>().unwrap_or(0));
            let req = CreateMessageChannelRequest {
                user_id: None,
                agent_id: None,
                channel_type,
                channel_name: new_name(),
                webhook_url: if new_webhook_url().is_empty() {
                    None
                } else {
                    Some(new_webhook_url())
                },
                access_token: None,
                secret: None,
                lark_app_id: None,
                lark_app_secret: None,
                lark_encrypt_key: None,
                lark_verification_token: None,
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
            match create_message_channel(req).await {
                Ok(_) => {
                    show_add_modal.set(false);
                    new_name.set(String::new());
                    new_type.set("0".to_string());
                    new_webhook_url.set(String::new());
                    success.set("创建成功".to_string());
                    match list_message_channels().await {
                        Ok(list) => channels.set(list.channels),
                        Err(e) => error.set(e),
                    }
                }
                Err(e) => error.set(format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    let channels_list = channels.read().clone();

    let new_type_value = new_type();

    rsx! {
        div { class: "card",
            ErrorAlert { message: error() }
            SuccessAlert { message: success() }
            div { class: "card-header",
                h2 { class: "card-title", "消息渠道管理" }
                button { class: "btn btn-accent", onclick: move |_| show_add_modal.set(true), "+ 创建渠道" }
            }
            if loading() {
                Loading {}
            } else if channels_list.is_empty() {
                EmptyState { icon: "📡".to_string(), message: "暂无消息渠道".to_string() }
            } else {
                table { class: "table",
                    thead { tr { th { "名称" }, th { "类型" }, th { "状态" }, th { "操作" } }}
                    tbody {
                        for c in channels_list.iter() {
                            {
                                let id = c.id.clone();
                                let status = c.status;
                                let is_active = status == ChannelStatus::Active;
                                let channel_name = c.channel_name.clone();
                                let channel_type = c.channel_type;
                                let id_disable = id.clone();
                                let id_enable = id.clone();
                                let id_delete = id.clone();
                                let id_test = id.clone();
                                rsx! {
                                    tr { key: "{id}",
                                        td { style: "font-weight: 500;", "{channel_name}" }
                                        td { span { class: "badge badge-info", "{channel_type}" } }
                                        td {
                                            if is_active { span { class: "badge badge-success", "启用" } }
                                            else { span { class: "badge badge-error", "禁用" } }
                                        }
                                        td {
                                            if is_active {
                                                button { class: "btn btn-ghost btn-sm",
                                                    onclick: move |_| {
                                                        let id_disable = id_disable.clone();
                                                        spawn(async move {
                                                            if let Err(e) = update_message_channel_status(&id_disable, 2).await {
                                                                error.set(e);
                                                            } else {
                                                                match list_message_channels().await {
                                                                    Ok(list) => channels.set(list.channels),
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
                                                            if let Err(e) = update_message_channel_status(&id_enable, 1).await {
                                                                error.set(e);
                                                            } else {
                                                                match list_message_channels().await {
                                                                    Ok(list) => channels.set(list.channels),
                                                                    Err(e) => error.set(e),
                                                                }
                                                            }
                                                        });
                                                    }, "启用"
                                                }
                                            }
                                            button { class: "btn btn-sm btn-accent",
                                                onclick: move |_| {
                                                    let id_test = id_test.clone();
                                                    spawn(async move {
                                                        match test_message_channel(&id_test).await {
                                                            Ok(resp) => {
                                                                if resp.success {
                                                                    success.set("连接测试通过".to_string());
                                                                } else {
                                                                    error.set(format!("连接测试失败: {}", resp.error.unwrap_or_default()));
                                                                }
                                                            }
                                                            Err(e) => error.set(format!("连接测试失败: {}", e)),
                                                        }
                                                    });
                                                }, "连接测试"
                                            }
                                            button { class: "btn btn-danger btn-sm",
                                                onclick: move |_| {
                                                    let id_delete = id_delete.clone();
                                                    spawn(async move {
                                                        if let Err(e) = delete_message_channel(&id_delete).await {
                                                            error.set(format!("删除失败: {}", e));
                                                        } else {
                                                            match list_message_channels().await {
                                                                Ok(list) => channels.set(list.channels),
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
            title: "创建消息渠道".to_string(),
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
                    label { class: "form-label", "渠道名称 *" }
                    input { class: "form-input", value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()), placeholder: "如：飞书通知群" }
                }
                div { class: "form-group",
                    label { class: "form-label", "渠道类型" }
                    select { class: "form-select", value: "{new_type_value}",
                        onchange: move |e| new_type.set(e.value()),
                        option { value: "0", "飞书 (Lark)" }
                        option { value: "1", "微信 (Wechat)" }
                        option { value: "2", "Slack" }
                        option { value: "3", "邮件 (Email)" }
                        option { value: "4", "Webhook" }
                    }
                }
                div { class: "form-group",
                    label { class: "form-label", "Webhook URL" }
                    input { class: "form-input", value: "{new_webhook_url}",
                        oninput: move |e| new_webhook_url.set(e.value()),
                        placeholder: "https://..." }
                }
            }
        }
    }
}
