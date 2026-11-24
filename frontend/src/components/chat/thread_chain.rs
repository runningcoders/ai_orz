//! 话题消息链（对话页 / 工作台专注态 / 紧凑浮层共用）
//!
//! 只负责「谁在回谁 + 正文」的紧凑渲染；正文走统一 Markdown 渲染器，
//! 长消息由 `CollapsibleContent` 提供展开 / 收起。

use common::api::MessageListItem;
use dioxus::prelude::*;

use super::message_bubble::ReplyPreview;
use crate::components::collapsible_content::CollapsibleContent;
use crate::components::markdown::MarkdownRenderer;
use crate::store::directory::use_directory;
use crate::utils::time::format_time_hm;

#[component]
pub fn ThreadChain(
    items: Vec<(MessageListItem, bool, String)>,
    reply_map: std::collections::HashMap<String, ReplyPreview>,
) -> Element {
    let directory = use_directory();
    let dir = directory.read();
    rsx! {
        div { class: "space-y-1.5",
            for (msg, is_current, content) in items.iter() {
                div {
                    key: "{msg.message_id}",
                    class: if *is_current {
                        "rounded-md border border-primary/30 bg-primary/5 px-2 py-1.5"
                    } else {
                        "rounded-md border border-base-content/10 bg-base-200/20 px-2 py-1.5"
                    },
                    div { class: "flex items-center gap-1.5 text-[10px] text-base-content/45",
                        span {
                            class: if *is_current { "text-primary" } else { "text-base-content/60" },
                            "{dir.sender_name(msg)}"
                        }
                        if let Some(reply) = reply_map.get(&msg.message_id) {
                            span { class: "min-w-0 truncate text-base-content/35",
                                "↩ {reply.sender}"
                            }
                        }
                        span { class: "ml-auto flex-shrink-0 font-mono",
                            "{format_time_hm(msg.created_at)}"
                        }
                        if *is_current {
                            span { class: "flex-shrink-0 rounded bg-primary/15 px-1 text-[9px] text-primary",
                                "当前"
                            }
                        }
                    }
                    CollapsibleContent {
                        len: content.chars().count(),
                        max_height: if *is_current { 140 } else { 96 },
                        class: "mt-1 thread-md",
                        MarkdownRenderer {
                            content: content.clone(),
                            compact: true,
                            resolve_attachment_descriptions: true,
                        }
                    }
                }
            }
        }
    }
}
