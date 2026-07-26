use dioxus::prelude::{Key, *};

use crate::api::message::search_messages;
use crate::components::button::Button;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use crate::utils::format_datetime_full as format_timestamp;
use common::api::MessageSearchResult;

fn format_role(role: i32) -> &'static str {
    match role {
        1 => "Agent",
        0 => "User",
        _ => "System",
    }
}

#[component]
pub fn MessageSearch() -> Element {
    let mut keyword = use_signal(String::new);
    let mut results = use_signal(Vec::<MessageSearchResult>::new);
    let mut loading = use_signal(|| false);
    // 修复 L_NEW（对齐 chat.rs L1）：删除未使用的 error signal（从未 set，永远为空）
    let toast = use_toast();

    let mut handle_search = move |_| {
        loading.set(true);
        let kw = keyword().clone();
        spawn(async move {
            match search_messages(&kw, None).await {
                Ok(data) => {
                    let msgs = data.messages;
                    results.set(msgs.clone());
                    if msgs.is_empty() {
                        toast.error("未找到匹配的消息");
                    }
                }
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    };

    rsx! {
        AppLayout {
            div { class: "card bg-base-100 shadow-md",
                h2 { class: "card-title", "消息搜索" }
                div { class: "space-y-4",
                    div { class: "flex gap-2",
                        input {
                            class: "form-input flex-1",
                            value: "{keyword}",
                            oninput: move |e| keyword.set(e.value()),
                            placeholder: "输入关键词搜索消息...",
                            onkeydown: move |evt| {
                                if evt.key() == Key::Enter {
                                    // 修复 L8：handle_search 内部已 spawn，外层 spawn 多余
                                    handle_search(());
                                }
                            }
                        }
                        Button {
                            onclick: move |_| handle_search(()),
                            "搜索"
                        }
                    }
                }
            }

            if loading() {
                Loading {}
            } else if results().is_empty() {
                EmptyState { message: "开始搜索".to_string() }
            } else {
                div { class: "card bg-base-100 shadow-md",
                    h3 { class: "card-title", "搜索结果 ({results().len()})" }
                    table { class: "table w-full",
                        thead {
                            tr {
                                th { "内容" }
                                th { "发送方" }
                                th { "类型" }
                                th { "匹配" }
                                th { "时间" }
                            }
                        }
                        tbody {
                            for msg in &results() {
                                tr { key: "{msg.message_id}",
                                    td { "{msg.content.chars().take(100).collect::<String>()}" }
                                    td {
                                        span { class: if msg.from_role == 1 { "badge badge-accent" } else { "badge badge-primary" },
                                            "{format_role(msg.from_role)}"
                                        }
                                    }
                                    td { "{msg.message_type}" }
                                    td {
                                        span { class: "text-sm text-base-content/70", "{msg.match_type.as_deref().unwrap_or_default()}" }
                                        if msg.vector_distance.is_some() {
                                            span { class: "text-sm text-accent ml-2", "d={msg.vector_distance.unwrap():.4}" }
                                        }
                                    }
                                    td { "{format_timestamp(msg.created_at)}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
