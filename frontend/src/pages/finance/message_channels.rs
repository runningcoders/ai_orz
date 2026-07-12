//! 消息渠道管理

use dioxus::prelude::*;

use crate::api::finance::{delete_message_channel, list_message_channels, update_message_channel_status};
use crate::components::state::{EmptyState, ErrorAlert, Loading};
use common::api::ListMessageChannelsResponseItem;

#[component]
pub fn FinanceMessageChannels() -> Element {
    let mut channels = use_signal(Vec::<ListMessageChannelsResponseItem>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);

    let load = move || {
        loading.set(true);
        spawn(async move {
            match list_message_channels().await {
                Ok(list) => channels.set(list.channels),
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    };

    use_effect(move || { load(); });

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
                                rsx! {
                                    tr { key: "{id}",
                                        td { style: "font-weight: 500;", "{c.name}" }
                                        td { span { class: "badge badge-info", "{c.channel_type}" } }
                                        td {
                                            if status == 1 { span { class: "badge badge-success", "启用" } }
                                            else { span { class: "badge badge-error", "禁用" } }
                                        }
                                        td {
                                            if status == 1 {
                                                button { class: "btn btn-ghost btn-sm",
                                                    onclick: move |_| {
                                                        let id = id.clone();
                                                        spawn(async move {
                                                            if let Err(e) = update_message_channel_status(&id, 0).await { error.set(e); } else { load(); }
                                                        });
                                                    }, "禁用"
                                                }
                                            } else {
                                                button { class: "btn btn-ghost btn-sm",
                                                    onclick: move |_| {
                                                        let id = id.clone();
                                                        spawn(async move {
                                                            if let Err(e) = update_message_channel_status(&id, 1).await { error.set(e); } else { load(); }
                                                        });
                                                    }, "启用"
                                                }
                                            }
                                            button { class: "btn btn-danger btn-sm",
                                                onclick: move |_| {
                                                    let id = id.clone();
                                                    spawn(async move {
                                                        if let Err(e) = delete_message_channel(&id).await { error.set(format!("删除失败: {}", e)); } else { load(); }
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
