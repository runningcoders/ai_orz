//! 消息渠道管理

use dioxus::prelude::*;
use dioxus_router::Link;

use crate::api::finance::{
    create_message_channel, delete_message_channel, list_message_channels, test_message_channel,
    update_message_channel_status,
};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{CreateMessageChannelRequest, ListMessageChannelsResponseItem};
use common::enums::{ChannelStatus, ChannelType};

#[component]
pub fn FinanceMessageChannels() -> Element {
    let mut channels = use_signal(Vec::<ListMessageChannelsResponseItem>::new);
    let mut loading = use_signal(|| true);
    let toast = use_toast();
    let mut show_add_modal = use_signal(|| false);

    let mut new_name = use_signal(String::new);
    let mut new_type = use_signal(|| "0".to_string());
    let mut new_webhook_url = use_signal(String::new);
    let mut new_lark_open_id = use_signal(String::new);
    let mut new_lark_user_name = use_signal(String::new);
    let mut new_agent_id = use_signal(String::new);
    let mut creating = use_signal(|| false);

    // ===== 删除确认对话框 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(|| String::new());

    use_effect(move || {
        loading.set(true);
        spawn(async move {
            match list_message_channels().await {
                Ok(list) => channels.set(list.channels),
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    });

    let handle_create = move |_| {
        spawn(async move {
            if new_name().is_empty() {
                toast.error("渠道名称不能为空");
                return;
            }
            creating.set(true);
            let channel_type = ChannelType::from_i32(new_type().parse::<i32>().unwrap_or(0));
            let req = CreateMessageChannelRequest {
                user_id: None,
                agent_id: if new_agent_id().is_empty() {
                    None
                } else {
                    Some(new_agent_id())
                },
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
                lark_open_id: if new_lark_open_id().is_empty() {
                    None
                } else {
                    Some(new_lark_open_id())
                },
                lark_user_name: if new_lark_user_name().is_empty() {
                    None
                } else {
                    Some(new_lark_user_name())
                },
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
                    new_lark_open_id.set(String::new());
                    new_lark_user_name.set(String::new());
                    new_agent_id.set(String::new());
                    toast.success("创建成功");
                    match list_message_channels().await {
                        Ok(list) => channels.set(list.channels),
                        Err(e) => toast.error(&e),
                    }
                }
                Err(e) => toast.error(&format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    let channels_list = channels.read().clone();

    let new_type_value = new_type();

    rsx! {
        AppLayout {
            div { class: "card bg-base-100 shadow-md",
                div { class: "card-body",
                    div { class: "flex justify-between items-center mb-4",
                        h2 { class: "card-title", "消息渠道管理" }
                        button { class: "btn btn-primary", onclick: move |_| show_add_modal.set(true), "+ 创建渠道" }
                    }
                    if loading() {
                        Loading {}
                    } else if channels_list.is_empty() {
                        EmptyState { icon: "📡".to_string(), message: "暂无消息渠道".to_string() }
                    } else {
                        div { class: "overflow-x-auto",
                            table { class: "table table-zebra table-pin-rows",
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
                                                    td { class: "font-semibold", "{channel_name}" }
                                                    td { span { class: "badge badge-info", "{channel_type}" } }
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
                                                                        if let Err(e) = update_message_channel_status(&id_disable, 2).await {
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
                                                                        if let Err(e) = update_message_channel_status(&id_enable, 1).await {
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
                                                                                toast.error(&format!("连接测试失败: {}", resp.error.unwrap_or_default()));
                                                                            }
                                                                        }
                                                                        Err(e) => toast.error(&format!("连接测试失败: {}", e)),
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
                    button { class: "btn btn-primary", disabled: creating(), onclick: handle_create,
                        if creating() { "创建中..." } else { "创建" }
                    }
                },
                div { class: "space-y-4",
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "渠道名称 *" }
                        }
                        input { class: "input input-bordered w-full", value: "{new_name}",
                            oninput: move |e| new_name.set(e.value()), placeholder: "如：飞书接待渠道" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "渠道类型" }
                        }
                        select { class: "select select-bordered w-full", value: "{new_type_value}",
                            onchange: move |e| new_type.set(e.value()),
                            option { value: "0", "飞书 (Lark)" }
                            option { value: "1", "微信 (Wechat)" }
                            option { value: "2", "Slack" }
                            option { value: "3", "邮件 (Email)" }
                            option { value: "4", "Webhook" }
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "绑定 Agent ID" }
                        }
                        input { class: "input input-bordered w-full", value: "{new_agent_id}",
                            oninput: move |e| new_agent_id.set(e.value()),
                            placeholder: "可选，绑定后消息自动路由到该 Agent" }
                    }
                    if new_type_value == "0" {
                        div { class: "form-control w-full",
                            label { class: "label",
                                span { class: "label-text font-medium", "飞书用户 Open ID" }
                            }
                            input { class: "input input-bordered w-full", value: "{new_lark_open_id}",
                                oninput: move |e| new_lark_open_id.set(e.value()),
                                placeholder: "ou_xxx，飞书用户的唯一标识" }
                        }
                        div { class: "form-control w-full",
                            label { class: "label",
                                span { class: "label-text font-medium", "飞书用户昵称" }
                            }
                            input { class: "input input-bordered w-full", value: "{new_lark_user_name}",
                                oninput: move |e| new_lark_user_name.set(e.value()),
                                placeholder: "可选，用于展示" }
                        }
                    }
                    if new_type_value == "4" {
                        div { class: "form-control w-full",
                            label { class: "label",
                                span { class: "label-text font-medium", "Webhook URL" }
                            }
                            input { class: "input input-bordered w-full", value: "{new_webhook_url}",
                                oninput: move |e| new_webhook_url.set(e.value()),
                                placeholder: "https://..." }
                        }
                    }
                }
            }

            ConfirmDialog {
                show: show_delete_confirm(),
                title: "确认删除".to_string(),
                message: "确定删除此消息渠道？此操作不可撤销。".to_string(),
                on_confirm: move |_| {
                    let id = pending_delete_id();
                    show_delete_confirm.set(false);
                    spawn(async move {
                        if let Err(e) = delete_message_channel(&id).await {
                            toast.error(&format!("删除失败: {}", e));
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
