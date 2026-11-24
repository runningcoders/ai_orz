//! 简版消息气泡组件
//!
//! 支持文本消息、图片附件、文件附件渲染。
//! 不含工具调用卡片、任务卡片等复杂消息类型；完整富消息见
//! [`super::rich_message::RichMessageItem`]（对话页 / 工作台专注态共用）。
//! 适用于 Agent 详情页等只需要「谁说了什么」的极简场景。

use common::api::MessageListItem;
use dioxus::prelude::*;

use crate::components::avatar_bubble::{AvatarBubble, AvatarTone};
use crate::components::collapsible_content::CollapsibleContent;
use crate::components::markdown::MarkdownRenderer;
use crate::store::auth::use_auth_state;
use crate::store::directory::use_directory;
use crate::utils::avatar::avatar_initials;
use crate::utils::file::format_file_size;
use crate::utils::message::{
    MSG_MIXED, MSG_TEXT, involves_user, is_attachment_message, normalize_markdown_type,
    role_avatar, role_class, role_label,
};
use crate::utils::time::format_message_time;

/// 被引用消息的轻量预览（工作台/详情页消息流用）
#[derive(Debug, Clone, PartialEq)]
pub struct ReplyPreview {
    /// 被引用消息 ID：没有话题面板时滚动到该消息
    pub message_id: String,
    /// 消息链根 ID：有话题面板时用它打开整条链
    pub root_id: Option<String>,
    pub sender: String,
    pub text: String,
}

/// 单条消息气泡
///
/// - 显示**真实头像**（Agent / 用户走 `AvatarBubble`，点击可展开资料卡）+ 发送者名字
///   + 角色 + 时间 + 消息内容
/// - `show_receiver` 为真时，在发送者后面拼出接收方（复用正文 @ 提及的 chip 写法），
///   用于群聊场景看清「谁在跟谁聊」
/// - 与当前用户无关的消息（Agent 之间的横向交流）整体降透明度（`message-bystander`）
/// - 附件消息自动渲染图片/文件下载链接
///
/// 发送者名字来自全局 `Directory`（`store/directory.rs`）：按 `from_role` 到对应的
/// Agent / 用户名称表查 `from_id`。查不到时回退 `角色 + 短 ID`，
/// **不会**渲染出 "user"/"agent" 这类英文角色码。
///
/// ⚠️ 系统消息没有可查询的实体，继续用静态字母头像；Agent / 用户用发送者首字，
/// 否则同屏多个 Agent 都会显示成同一个 "A"，分不清谁是谁。
///
/// `avatar_popover` 显式开启点击头像弹资料卡（默认关）：
/// `AvatarBubble` 的浮层用 `position: fixed` + 视口坐标，宿主祖先若带 `clip-path`
/// 会把浮层裁掉、带 `transform` 类定位基准会整体偏移。工作台全屏对话框只多一层
/// `backdrop-filter`（偏移约一个内边距，仍在可视区内）且消息区足够高，可以开启；
/// Agent 详情页外层 `HudPanel` 带 `clip-path`、底部收起态消息区只有 `max-h-24`，
/// 两者都开不得。
#[component]
pub fn MessageBubble(
    msg: MessageListItem,
    #[props(default = true)] show_receiver: bool,
    #[props(default = false)] avatar_popover: bool,
    /// 引用块：由持有完整消息列表的调用方解析后传入
    #[props(default)]
    reply_preview: Option<ReplyPreview>,
    /// 长消息折叠高度（紧凑消息流用小值，全屏对话用大值）
    #[props(default = 280)]
    collapse_height: u32,
    /// 有话题面板的宿主传入：点击引用块打开整条消息链，而不是只滚动
    #[props(default)]
    on_open_thread: Option<EventHandler<String>>,
) -> Element {
    let role = msg.from_role;
    let avatar = role_avatar(role);
    let class = role_class(role);
    let role_name = role_label(role);
    let time = format_message_time(msg.created_at);

    let directory = use_directory();
    let auth = use_auth_state();
    let dir = directory.read();
    let sender = dir.sender_name(&msg);
    let receiver = if show_receiver {
        dir.receiver_mention(&msg)
    } else {
        None
    };
    // 收发双方都不是当前用户 → 旁听消息，视觉弱化（hover 恢复，便于细看）
    let current_user_id = auth.read().user_id.clone();
    let bystander = !involves_user(&msg, &current_user_id);
    // 用户头像卡片的副标题：只有自己才是「当前用户」，别人是组织成员
    let user_subtitle = if msg.from_id == current_user_id {
        "当前用户"
    } else {
        "组织成员"
    };
    let item_class = if bystander {
        format!("message-item {class} message-bystander")
    } else {
        format!("message-item {class}")
    };

    rsx! {
        div { id: "msg-{msg.message_id}", class: "{item_class}", key: "{msg.message_id}",
            // 宿主只提供定位位（flex-none）：`.avatar` 由 AvatarBubble 自己贴在触发层
            if avatar_popover && (role == 0 || role == 1) {
                div { class: "flex-none",
                    AvatarBubble {
                        name: sender.clone(),
                        tone: if role == 0 { AvatarTone::User } else { AvatarTone::Agent },
                        agent_id: (role == 1).then(|| msg.from_id.clone()),
                        user_id: (role == 0).then(|| msg.from_id.clone()),
                        user_subtitle: (role == 0).then(|| user_subtitle.to_string()),
                    }
                }
            } else if role == 0 || role == 1 {
                // 未开启浮层：至少用发送者首字区分不同 Agent（不能退回统一的 "A"）
                div { class: "message-avatar", "{avatar_initials(&sender)}" }
            } else {
                div { class: "message-avatar", "{avatar}" }
            }
            div { class: "message-body",
                div { class: "message-meta",
                    // 修复：这里原先渲染的是 `role_class()` 返回的 "user"/"agent" 英文角色码。
                    // 现在渲染真实发送者名字（Agent 名 / 用户名 / 「系统」）+ 中文角色标签。
                    span { class: "message-sender", "{sender}" }
                    span { class: "message-role", "{role_name}" }
                    // 「发给谁」：与正文 @ 提及同一套写法（同样的 chip 样式与 @ 前缀）
                    if let Some(html) = receiver {
                        span { class: "message-arrow", "→" }
                        span { class: "message-receiver", dangerous_inner_html: "{html}" }
                    }
                    span { class: "message-time", "{time}" }
                }
                // 引用块：让紧凑消息流也能看出「这条在回谁」，点击回到被引用消息
                if let Some(reply) = &reply_preview {
                    button {
                        class: "mb-1 flex w-full items-center gap-1.5 overflow-hidden rounded border-l-2 border-primary/40 bg-base-200/40 px-1.5 py-1 text-left text-[10px] text-base-content/50 transition-colors hover:bg-base-200/70",
                        onclick: {
                            let thread_root = reply
                                .root_id
                                .clone()
                                .unwrap_or_else(|| reply.message_id.clone());
                            let target = format!("msg-{}", reply.message_id);
                            move |_| {
                                if let Some(open) = on_open_thread {
                                    open.call(thread_root.clone());
                                    return;
                                }
                                if let Some(doc) = web_sys::window().and_then(|w| w.document())
                                    && let Some(el) = doc.get_element_by_id(&target)
                                {
                                    el.scroll_into_view();
                                }
                            }
                        },
                        span { class: "flex-shrink-0 text-primary/70", "↩ {reply.sender}" }
                        span { class: "truncate", "{reply.text}" }
                    }
                }
                {render_content(&msg, collapse_height)}
            }
        }
    }
}

/// 渲染消息内容（文本/图片/文件）
///
/// `is_attachment_message` 分支只服务**历史存量**：早年附件是独立消息
/// （`content` = 附件 id、`file_type` 标记类型）。现在站内发送与渠道入站都把
/// 附件以 `[文件名](attachment:id)` 并进正文（`Mixed`），走下面 Markdown 渲染
/// 分支出图，不再产生新的独立附件消息。
fn render_content(msg: &MessageListItem, collapse_height: u32) -> Element {
    // 通知/伴生类消息（14/15 回执 + 10/11/12 系统通知与 Agent 知会 + 7/8 确认）
    // 正文都是 Markdown，统一归一到 MSG_TEXT 走 MarkdownRenderer。
    // ⚠️ 只归一化**类型局部变量**，不 clone 整条 `MessageListItem`：这个函数在
    // 每条气泡每次重渲染时都会跑，整结构体克隆会拖慢长列表。
    let message_type = normalize_markdown_type(msg.message_type);

    if is_attachment_message(message_type) {
        if let Some(fm) = &msg.file_meta {
            // /file = 原始字节流；/content 是 JSON 文本信封，<img>/<a> 拿到 JSON 会裂开
            let file_url = format!("/api/v1/finance/attachments/{}/file", msg.content);
            if message_type == 1 {
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
    } else if message_type == MSG_TEXT || message_type == MSG_MIXED {
        // 批3 混排主线：Text/Mixed 同路 Markdown 渲染（资源引用自动 chip 化）
        // Text 消息按 Markdown 渲染（紧凑样式）
        rsx! {
            CollapsibleContent {
                len: msg.content.chars().count(),
                max_height: collapse_height,
                span { class: "message-text",
                    MarkdownRenderer { content: msg.content.clone(), compact: true, resolve_attachment_descriptions: true }
                }
            }
        }
    } else {
        // 兜底也走 Markdown：正文默认即 Markdown，未知/其他类型同样富文本渲染，
        // 不退回纯文本（否则 markdown 符号原样显示）。
        rsx! {
            CollapsibleContent {
                len: msg.content.chars().count(),
                max_height: collapse_height,
                span { class: "message-text",
                    MarkdownRenderer { content: msg.content.clone(), compact: true, resolve_attachment_descriptions: true }
                }
            }
        }
    }
}
