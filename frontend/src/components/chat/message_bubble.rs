//! 简版消息气泡组件
//!
//! 支持文本消息、图片附件、文件附件渲染。
//! 不含工具调用卡片、任务卡片等复杂消息类型（chat.rs 主对话页保留自己的富版本）。
//! 适用于 Agent 详情页、Workspace 底部对话框等极简场景。

use common::api::MessageListItem;
use dioxus::prelude::*;

use crate::components::markdown::MarkdownRenderer;
use crate::store::directory::use_directory;
use crate::utils::file::format_file_size;
use crate::utils::message::{MSG_TEXT, is_attachment_message, role_avatar, role_class, role_label};
use crate::utils::time::format_time_hm;

/// 单条消息气泡
///
/// - 显示头像 + **发送者名字** + 角色 + 时间 + 消息内容
/// - 附件消息自动渲染图片/文件下载链接
///
/// 发送者名字来自全局 `Directory`（`store/directory.rs`）：按 `from_role` 到对应的
/// Agent / 用户名称表查 `from_id`。查不到时回退 `角色 + 短 ID`，
/// **不会**渲染出 "user"/"agent" 这类英文角色码。
#[component]
pub fn MessageBubble(msg: MessageListItem) -> Element {
    let role = msg.from_role;
    let avatar = role_avatar(role);
    let class = role_class(role);
    let role_name = role_label(role);
    let time = format_time_hm(msg.created_at);

    let directory = use_directory();
    let sender = directory.read().sender_name(&msg);

    rsx! {
        div { class: "message-item {class}", key: "{msg.message_id}",
            div { class: "message-avatar", "{avatar}" }
            div { class: "message-body",
                div { class: "message-meta",
                    // 修复：这里原先渲染的是 `role_class()` 返回的 "user"/"agent" 英文角色码。
                    // 现在渲染真实发送者名字（Agent 名 / 用户名 / 「系统」）+ 中文角色标签。
                    span { class: "message-sender", "{sender}" }
                    span { class: "message-role", "{role_name}" }
                    span { class: "message-time", "{time}" }
                }
                {render_content(&msg)}
            }
        }
    }
}

/// 渲染消息内容（文本/图片/文件）
fn render_content(msg: &MessageListItem) -> Element {
    if is_attachment_message(msg.message_type) {
        if let Some(fm) = &msg.file_meta {
            let file_url = format!("/api/v1/finance/attachments/{}/content", msg.content);
            if msg.message_type == 1 {
                // 图片
                rsx! {
                    div { class: "message-attachment message-attachment-image",
                        img { src: "{file_url}", class: "message-image", loading: "lazy" }
                    }
                }
            } else {
                // 文件
                rsx! {
                    div { class: "message-attachment message-attachment-file",
                        a { href: "{file_url}", class: "attachment-download",
                            div { class: "file-icon", "📄" }
                            div { class: "file-info",
                                span { class: "attachment-name", "{fm.name}" }
                                span { class: "attachment-size", "{format_file_size(fm.size)}" }
                            }
                        }
                    }
                }
            }
        } else {
            rsx! { span { class: "message-text", "[附件]" } }
        }
    } else if msg.message_type == MSG_TEXT {
        // Text 消息按 Markdown 渲染（紧凑样式）
        rsx! {
            span { class: "message-text",
                MarkdownRenderer { content: msg.content.clone(), compact: true }
            }
        }
    } else {
        rsx! { span { class: "message-text", "{msg.content}" } }
    }
}
