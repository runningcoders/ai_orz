//! 消息渠道管理

use dioxus::prelude::*;

use crate::api::finance::{delete_message_channel, list_message_channels, update_message_channel_status};
use crate::components::state::{EmptyState, ErrorAlert, Loading};
use common::api::ListMessageChannelsResponseItem;
use common::enums::ChannelStatus;

#[component]
pub fn FinanceMessageChannels() -> Element {
    let mut channels = use_signal(Vec::<ListMessageChannelsResponseItem>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);

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

    let channels_list = channels.read().clone();

    rsx! {
        div { class: "card",
            ErrorAlert { message: error() }
            div { class: "card-header",
                h2 { class: "card-title", "消息渠道管理" }
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
    }
}
