//! 富消息项（对话页 / 工作台专注态共用）
//!
//! 单条消息的完整展示：头像 + 发送者 + 接收方 + 引用块 + 正文
//! （文本 / 附件 / 工具调用卡 / 任务分配卡）+ hover 复制 / 回复。
//!
//! 两个宿主只在参数上不同，其余渲染逻辑完全一致：
//! - **尺寸**：宽度由外层容器决定；长消息折叠高度由 `collapse_height` 传入
//! - **数据来源**：对话页传当前会话消息，专注态传工作台消息流
//! - **能力开关**：`on_reply`（无回复能力时隐藏回复按钮）、`on_open_thread`、
//!   `avatar_popover`（紧凑且有裁切祖先的宿主关掉资料卡浮层）
//!
//! 工具卡展开状态由本组件自持（按消息 id 作为列表 key），
//! 不再需要宿主维护 `tool_expanded` 集合，会话切换时随组件卸载自然复位。

use common::api::MessageListItem;
use dioxus::prelude::*;

use crate::components::avatar_bubble::{AvatarBubble, AvatarTone, BubbleAlign};
use crate::components::collapsible_content::CollapsibleContent;
use crate::components::markdown::MarkdownRenderer;
use crate::store::auth::use_auth_state;
use crate::store::directory::{ensure_agent_name, use_directory};
use crate::store::toast::{ToastState, use_toast};
use crate::utils::{
    MSG_AUDIO, MSG_IMAGE, MSG_MIXED, MSG_TASK_ASSIGNMENT, MSG_TASK_COMPLETED,
    MSG_TASK_PENDING_REVIEW, MSG_TEXT, MSG_TOOL_CALL_REQUEST, MSG_TOOL_CALL_RESULT, MSG_VIDEO,
    avatar_initials, format_file_size, format_message_time as format_time, involves_user,
    is_attachment_message,
};

/// 被引用消息的预览信息（气泡引用块展示用）
#[derive(Debug, Clone, PartialEq)]
pub struct QuotedPreview {
    /// 被回复消息的发送者名
    pub sender: String,
    /// 被回复内容（截断后的纯文本预览）
    pub content: String,
    /// 被回复消息所属消息链的 root_id（缺失时回退为其自身 ID），
    /// 点击引用块用它打开话题浏览页
    pub root_id: Option<String>,
}

/// 引用预览最多展示的字符数：引用块是一行摘要，不承载全文
pub const REPLY_PREVIEW_CHARS: usize = 80;

/// 消息正文预览：纯附件消息退回附件名，避免引用块出现空白。
///
/// 对话页与专注态共用，保证两处引用块「显示什么」同口径。
pub fn message_preview_text(m: &MessageListItem) -> String {
    if m.content.trim().is_empty() {
        m.file_meta
            .as_ref()
            .map(|f| format!("📎 {}", f.name))
            .unwrap_or_default()
    } else {
        m.content.clone()
    }
}

/// 引用预览截断：超长正文压成一行，避免引用块把消息气泡顶开
pub fn truncate_preview(text: &str) -> String {
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= REPLY_PREVIEW_CHARS {
        return one_line;
    }
    let mut out: String = one_line.chars().take(REPLY_PREVIEW_CHARS).collect();
    out.push('…');
    out
}

/// 气泡的回复 / 引用上下文：引用预览 + 回复回调 + 打开话题回调。
///
/// 回调全部可选：只读宿主（如工作台专注态消息流）不传回复能力，
/// 引用块仍作为预览展示，只是没有可点的去处。
struct ReplyCtx {
    quoted: Option<QuotedPreview>,
    on_reply: Option<EventHandler<()>>,
    on_open_thread: Option<EventHandler<String>>,
}

/// 单条富消息
///
/// 宿主只需按消息链顺序迭代并传入 `msg` + 引用预览 + 能力回调；
/// 发送者名字、接收方 chip、旁听弱化、头像形态、正文渲染分支都在组件内收敛。
#[component]
pub fn RichMessageItem(
    msg: MessageListItem,
    /// 被引用消息预览（由持有完整消息列表的宿主解析后传入）
    #[props(default)]
    quoted: Option<QuotedPreview>,
    /// 回复回调；`None` 时隐藏 hover 操作排里的回复按钮
    #[props(default)]
    on_reply: Option<EventHandler<()>>,
    /// 打开话题回调；`None` 时引用块不可点
    #[props(default)]
    on_open_thread: Option<EventHandler<String>>,
    /// 是否在发送者后面拼出接收方（谁在跟谁聊）
    #[props(default = true)]
    show_receiver: bool,
    /// 头像是否用可展开资料卡的 `AvatarBubble`；
    /// 紧凑且有裁切祖先的宿主（收起态底部横幅）传 false，避免浮层被裁
    #[props(default = true)]
    avatar_popover: bool,
    /// 长消息折叠高度（紧凑消息流用小值，全屏对话用大值）
    #[props(default = 340)]
    collapse_height: u32,
    /// 消息来自当前会话目标 Agent 时的运行时状态（驱动头像警示环）
    #[props(default)]
    agent_status: Option<i32>,
) -> Element {
    // 工具卡展开状态按消息实例持有：列表 key 是 message_id，切换会话即卸载复位
    let expanded = use_signal(|| false);
    let directory = use_directory();
    let auth = use_auth_state();
    let toast = use_toast();

    let is_user = msg.from_role == 0;
    let is_system = msg.from_role == 2;
    if msg.from_role == 1 {
        // 按需补拉名字的去重集合在全局目录里，跨消息共享（同一 Agent 只请求一次）
        ensure_agent_name(directory, msg.from_id.clone());
    }

    let sender_name = directory.read().sender_name(&msg);
    let receiver_html = if show_receiver {
        directory.read().receiver_mention(&msg)
    } else {
        None
    };
    // 收发双方都不是当前用户 → 旁听消息，整体降透明度（hover 恢复）
    let bystander = !involves_user(&msg, &auth.read().user_id);
    // 头像气泡形态：用户头像弹用户卡（右对齐），Agent 头像弹 Agent 卡（左对齐）
    let (bubble_agent_id, bubble_user_id, bubble_align) = if is_user {
        (None, Some(msg.from_id.clone()), BubbleAlign::End)
    } else {
        (Some(msg.from_id.clone()), None, BubbleAlign::Start)
    };
    let item_class = if is_user {
        "chat chat-end"
    } else if bystander {
        "chat chat-start chat-bystander"
    } else {
        "chat chat-start"
    };
    let msg_id = msg.message_id.clone();
    let reply_ctx = ReplyCtx {
        quoted,
        on_reply,
        on_open_thread,
    };

    rsx! {
        div { class: "{item_class}", id: "msg-{msg_id}",
            div { class: "chat-header pr-1 text-sm opacity-70 flex items-center gap-1",
                span { "{sender_name}" }
                // 接收方：与正文 @ 提及同一套 chip 写法（谁在跟谁聊）
                if let Some(html) = receiver_html {
                    span { class: "message-arrow", "→" }
                    span { class: "message-receiver", dangerous_inner_html: "{html}" }
                }
            }
            if is_system {
                div { class: "chat-image avatar",
                    div { class: "w-10 h-10 rounded-full bg-info text-info-content flex items-center justify-center font-bold",
                        "{avatar_initials(&sender_name)}"
                    }
                }
            } else if avatar_popover {
                // 宿主只提供 `.chat-image`：`.avatar` 由 AvatarBubble 自己贴在
                // 触发层（详见组件内的结构约束注释）
                div { class: "chat-image",
                    AvatarBubble {
                        name: sender_name.clone(),
                        tone: if is_user { AvatarTone::User } else { AvatarTone::Agent },
                        agent_id: bubble_agent_id,
                        user_id: bubble_user_id,
                        status: if is_user { None } else { agent_status },
                        align: bubble_align,
                    }
                }
            } else {
                // 不开资料卡：静态首字头像，仍能区分同屏多个 Agent
                div { class: "chat-image",
                    div {
                        class: if is_user {
                            "w-10 h-10 rounded-full bg-primary text-primary-content flex items-center justify-center font-bold"
                        } else {
                            "w-10 h-10 rounded-full bg-secondary text-secondary-content flex items-center justify-center font-bold"
                        },
                        "{avatar_initials(&sender_name)}"
                    }
                }
            }
            {
                let mut expanded = expanded;
                render_message_content(
                    &msg,
                    BubbleView {
                        is_user,
                        is_system,
                        expanded: expanded(),
                        collapse_height,
                        toast,
                    },
                    move || expanded.set(!expanded()),
                    reply_ctx,
                )
            }
        }
    }
}

fn copy_to_clipboard(content: &str, toast: &ToastState) {
    if let Some(window) = web_sys::window() {
        let navigator = window.navigator();
        let clipboard = navigator.clipboard();
        let promise = clipboard.write_text(content);
        let toast = *toast;
        wasm_bindgen_futures::spawn_local(async move {
            match wasm_bindgen_futures::JsFuture::from(promise).await {
                Ok(_) => toast.info("已复制到剪贴板"),
                Err(_) => toast.error("复制失败"),
            }
        });
    } else {
        toast.error("剪贴板不可用");
    }
}

/// 单条气泡的静态渲染参数（避免渲染函数参数列表膨胀）
struct BubbleView {
    is_user: bool,
    is_system: bool,
    /// 工具卡当前是否展开
    expanded: bool,
    /// 长消息折叠高度
    collapse_height: u32,
    toast: ToastState,
}

/// 根据消息类型渲染不同内容
fn render_message_content(
    msg: &MessageListItem,
    view: BubbleView,
    toggle_expand: impl FnMut() + 'static,
    reply_ctx: ReplyCtx,
) -> Element {
    let BubbleView {
        is_user,
        is_system,
        expanded,
        collapse_height,
        toast,
    } = view;
    let ReplyCtx {
        quoted,
        on_reply,
        on_open_thread,
    } = reply_ctx;
    // 气泡配色已收敛到 `styles/input.css` 的 `.chat .chat-bubble-{primary,neutral,system}`：
    // 轻量「浅底 + 正文色 + 细边」，替代 DaisyUI 原生高饱和/近黑实底。
    let bubble_class = if is_user {
        "chat-bubble chat-bubble-primary"
    } else if is_system {
        "chat-bubble chat-bubble-system text-xs"
    } else {
        "chat-bubble chat-bubble-neutral"
    };
    let time_class = "chat-footer text-xs opacity-50 mt-1";

    if is_attachment_message(msg.message_type) {
        return render_attachment_message(msg, bubble_class, time_class);
    }

    // 任务完成回执(14) / 任务待验收知会(15) 是动作伴生消息，正文是 Markdown
    // 表格工作单，不是 TaskAssignment 的 JSON 载荷，按普通文本走同一套 Markdown 渲染。
    let message_type =
        if msg.message_type == MSG_TASK_COMPLETED || msg.message_type == MSG_TASK_PENDING_REVIEW {
            MSG_TEXT
        } else {
            msg.message_type
        };

    match message_type {
        MSG_TOOL_CALL_REQUEST | MSG_TOOL_CALL_RESULT => render_tool_call_card(
            &msg.content,
            msg.message_type == MSG_TOOL_CALL_RESULT,
            expanded,
            toggle_expand,
            msg.created_at,
            time_class,
        ),
        MSG_TASK_ASSIGNMENT => {
            render_task_card(&msg.content, msg.created_at, bubble_class, time_class)
        }
        // 批3 混排主线：Mixed=13 与 Text 同路直渲染（引用块+MarkdownRenderer+hover 操作排），
        // 协议资源引用经批1 markdown_protocol+批2 render_resource_chip 自动 chip 化。
        MSG_TEXT | MSG_MIXED => {
            let content = msg.content.clone();
            let toast_copy = toast;
            // 引用块：quote_label 驱动展示，quote_target 供点击回调取链根
            let quote_label = quoted.clone();
            let quote_target = quoted;
            // 有引用时给整块一个最小宽度（见 input.css `.chat-has-quote`）：
            // 否则「在吗」这类极短消息旁边的引用块会被压成 2 字一行
            let wrap_class = if quote_label.is_some() {
                "group relative chat-has-quote"
            } else {
                "group relative"
            };
            rsx! {
                div { class: "{wrap_class}",
                    if let Some(q) = quote_label {
                        button {
                            // 宽度以本条消息为准，见 input.css `.chat .chat-quote`
                            class: "chat-quote mb-1 text-left rounded-lg border border-base-300 bg-base-200/70 px-2 py-1 text-xs text-base-content/70 hover:bg-base-200 transition-colors",
                            title: "查看话题讨论区",
                            onclick: move |_| {
                                if let (Some(open), Some(root)) = (
                                    on_open_thread,
                                    quote_target.as_ref().and_then(|t| t.root_id.clone()),
                                ) {
                                    open.call(root);
                                }
                            },
                            span { class: "font-medium", "↩ {q.sender}" }
                            span { class: "ml-1 opacity-80", "{q.content}" }
                        }
                    }
                    div { class: "{bubble_class} break-words",
                        CollapsibleContent {
                            len: msg.content.chars().count(),
                            max_height: collapse_height,
                            MarkdownRenderer { content: msg.content.clone(), compact: true, resolve_attachment_descriptions: true }
                        }
                    }
                    // hover 操作图标排：复制（+ 回复，宿主具备回复能力时）
                    div { class: "absolute -top-2 -right-2 flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity",
                        button {
                            class: "btn hud-btn btn-ghost btn-xs btn-square bg-base-100 shadow",
                            title: "复制",
                            onclick: move |_| copy_to_clipboard(&content, &toast_copy),
                            "⧉"
                        }
                        if let Some(reply) = on_reply {
                            button {
                                class: "btn hud-btn btn-ghost btn-xs btn-square bg-base-100 shadow",
                                title: "回复",
                                onclick: move |_| reply.call(()),
                                "↩"
                            }
                        }
                    }
                }
                div { class: "{time_class}", "{format_time(msg.created_at)}" }
            }
        }
        _ => rsx! {
            div { class: "{bubble_class} break-words whitespace-pre-wrap", "{msg.content}" }
            div { class: "{time_class}", "{format_time(msg.created_at)}" }
        },
    }
}

/// 渲染附件消息
fn render_attachment_message(
    msg: &MessageListItem,
    bubble_class: &str,
    time_class: &str,
) -> Element {
    let file_meta = match &msg.file_meta {
        Some(fm) => fm,
        None => {
            return rsx! {
                div { class: "{bubble_class} break-words", "{msg.content}" }
                div { class: "{time_class}", "{format_time(msg.created_at)}" }
            };
        }
    };

    let file_path = &msg.content;
    // /file 返回原始字节流（按 mime 给 Content-Type）；/content 是 JSON 文本信封，
    // <img>/<video>/<audio> 拿到 JSON 会裂开，不能用于媒体渲染。
    let file_url = format!("/api/v1/finance/attachments/{}/file", file_path);

    match msg.message_type {
        MSG_IMAGE => rsx! {
            div { class: "{bubble_class} p-1",
                img {
                    src: "{file_url}",
                    class: "max-w-xs rounded-lg",
                    loading: "lazy",
                }
            }
            div { class: "{time_class}", "{format_time(msg.created_at)}" }
        },
        MSG_VIDEO => rsx! {
            div { class: "{bubble_class} p-2 space-y-2",
                video {
                    src: "{file_url}",
                    controls: "true",
                    class: "max-w-xs rounded-lg",
                    preload: "metadata",
                    "您的浏览器不支持视频播放"
                }
                div { class: "flex items-center gap-2 text-sm opacity-80",
                    span { "🎬" }
                    span { "{file_meta.name}" }
                    span { class: "opacity-60", "{format_file_size(file_meta.size)}" }
                }
            }
            div { class: "{time_class}", "{format_time(msg.created_at)}" }
        },
        MSG_AUDIO => rsx! {
            div { class: "{bubble_class} p-2 space-y-2 min-w-[200px]",
                div { class: "flex items-center gap-2 text-sm",
                    span { "🎵" }
                    span { "{file_meta.name}" }
                }
                audio {
                    src: "{file_url}",
                    controls: "true",
                    class: "w-full",
                    preload: "metadata",
                }
                div { class: "text-xs opacity-60", "{format_file_size(file_meta.size)}" }
            }
            div { class: "{time_class}", "{format_time(msg.created_at)}" }
        },
        _ => rsx! {
            a {
                href: "{file_url}",
                class: "{bubble_class} flex items-center gap-3 no-underline hover:opacity-90 transition-opacity",
                div { class: "text-2xl", "📄" }
                div { class: "flex flex-col",
                    span { "{file_meta.name}" }
                    span { class: "text-xs opacity-60", "{format_file_size(file_meta.size)}" }
                }
            }
            div { class: "{time_class}", "{format_time(msg.created_at)}" }
        },
    }
}

/// 渲染工具调用卡片
fn render_tool_call_card(
    content: &str,
    is_result: bool,
    expanded: bool,
    mut toggle_expand: impl FnMut() + 'static,
    time: i64,
    time_class: &str,
) -> Element {
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(content);

    match parsed {
        Ok(json) => {
            let tool_name = json
                .get("tool_name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let (status_badge, header_class) = if is_result {
                match json.get("is_success").and_then(|v| v.as_bool()) {
                    Some(true) => (
                        "badge hud-badge badge-success",
                        "flex items-center gap-2 px-3 py-2 cursor-pointer hover:bg-base-300/50 transition-colors border-l-4 border-l-success",
                    ),
                    Some(false) => (
                        "badge hud-badge badge-error",
                        "flex items-center gap-2 px-3 py-2 cursor-pointer hover:bg-base-300/50 transition-colors border-l-4 border-l-error",
                    ),
                    None => (
                        "badge hud-badge badge-info",
                        "flex items-center gap-2 px-3 py-2 cursor-pointer hover:bg-base-300/50 transition-colors border-l-4 border-l-info",
                    ),
                }
            } else {
                (
                    "badge hud-badge badge-warning",
                    "flex items-center gap-2 px-3 py-2 cursor-pointer hover:bg-base-300/50 transition-colors border-l-4 border-l-warning",
                )
            };

            let header_icon = if is_result { "⚙️" } else { "🔧" };
            let status_label = if is_result {
                match json.get("is_success").and_then(|v| v.as_bool()) {
                    Some(true) => "执行成功",
                    Some(false) => "执行失败",
                    None => "已执行",
                }
            } else {
                "调用请求"
            };

            let args_str = json
                .get("args")
                .and_then(|v| serde_json::to_string_pretty(v).ok());
            let result_str = json
                .get("result")
                .and_then(|v| serde_json::to_string_pretty(v).ok());
            let error_msg = json
                .get("error_message")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            rsx! {
                div { class: "chat-bubble chat-bubble-neutral p-0 overflow-hidden",
                    div {
                        class: "{header_class}",
                        onclick: move |_| toggle_expand(),
                        span { "{header_icon}" }
                        span { class: "font-medium text-sm", "{tool_name}" }
                        span { class: "{status_badge} badge-sm", "{status_label}" }
                        span { class: "ml-auto text-xs opacity-60",
                            if expanded { "▼" } else { "▶" }
                        }
                    }
                    if expanded {
                        div { class: "px-3 pb-3 space-y-2 text-sm",
                            if let Some(args) = &args_str {
                                div {
                                    div { class: "text-xs font-semibold opacity-60 mb-1", "参数" }
                                    pre { class: "bg-base-200 p-2 rounded text-xs overflow-x-auto", "{args}" }
                                }
                            }
                            if is_result {
                                if let Some(result) = &result_str {
                                    div {
                                        div { class: "text-xs font-semibold opacity-60 mb-1", "结果" }
                                        pre { class: "bg-base-200 p-2 rounded text-xs overflow-x-auto", "{result}" }
                                    }
                                }
                                if let Some(err) = &error_msg {
                                    div {
                                        div { class: "text-xs font-semibold text-error mb-1", "错误" }
                                        div { class: "text-error text-sm", "{err}" }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "{time_class}", "{format_time(time)}" }
            }
        }
        Err(_) => rsx! {
            div { class: "chat-bubble chat-bubble-neutral break-words", "{content}" }
            div { class: "{time_class}", "{format_time(time)}" }
        },
    }
}

/// 渲染任务分配卡片
fn render_task_card(content: &str, time: i64, bubble_class: &str, time_class: &str) -> Element {
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(content);

    match parsed {
        Ok(json) => {
            let title = json
                .get("task_title")
                .and_then(|v| v.as_str())
                .unwrap_or("未知任务")
                .to_string();
            let description = json.get("task_description").and_then(|v| v.as_str());

            rsx! {
                div { class: "{bubble_class} p-0 overflow-hidden",
                    div { class: "flex items-center gap-2 px-3 py-2 border-b border-base-300/30",
                        span { "📋" }
                        span { class: "font-medium", "任务分配" }
                    }
                    div { class: "px-3 py-2 space-y-1",
                        div { class: "font-semibold", "{title}" }
                        if let Some(desc) = description {
                            if !desc.is_empty() {
                                div { class: "text-sm opacity-80", "{desc}" }
                            }
                        }
                    }
                }
                div { class: "{time_class}", "{format_time(time)}" }
            }
        }
        Err(_) => rsx! {
            div { class: "{bubble_class} break-words", "{content}" }
            div { class: "{time_class}", "{format_time(time)}" }
        },
    }
}
