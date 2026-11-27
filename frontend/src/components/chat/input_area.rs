//! 聊天输入区（对话页 / 工作台横幅 / 专注模式共用）
//!
//! 三处聊天入口本来就是同一个交互：文本输入、@ 提及、附件三路入口、
//! 快捷键发送、忙碌提示。差异只在「紧凑度」「回复引用」「焦点跟踪」这些
//! 场景参数上，绝不是三份实现——本组件是这三处的唯一输入区实现。
//!
//! 职责边界：组件自己订阅输入态 / slash / 拖放 / 提及并把它们写回传入的
//! signal；发送、上传、回复目标解析等业务逻辑由调用方以回调注入。

use dioxus::html::HasFileData;
use dioxus::prelude::*;
use wasm_bindgen::JsCast;

use crate::components::mention_picker::{
    MentionCandidate, MentionPickedBar, MentionPicker, MentionState, MentionTab,
};
use crate::components::state::Loading;
use crate::hooks::use_attachments::{PendingAttachment, clipboard_files_from_paste};
use crate::store::toast::use_toast;
use crate::utils::mention::{read_caret, restore_caret};
use common::api::MessageListItem;

/// 内置 slash 命令：对话页与工作台一律可用，避免两处菜单内容漂移
const SLASH_COMMANDS: [(&str, &str); 2] = [("/clear", "清空对话"), ("/help", "显示帮助")];

/// 统一聊天输入区
///
/// - `input_id` / `file_input_id`：DOM id 由调用方给。光标读写按 id 定位，
///   同一页面里任一时刻只有一个输入区，但仍显式传参避免写死场景。
/// - `compact`：工作台横幅 / 专注态用紧凑样式；对话页用宽松样式。
/// - `agent_state`：Some 时按运行时状态显示忙碌占位文案；None 表示不提示。
/// - `reply_preview` / `on_cancel_reply`：仅对话页有回复引用条。
/// - `focus_selector` / `on_focus` / `on_blur`：工作台需要「焦点在对话框内
///   就不收起横幅」的延时判定；对话页三者都传 None。
#[component]
#[allow(clippy::too_many_arguments)]
pub fn ChatInputArea(
    input_text: Signal<String>,
    pending_attachments: Signal<Vec<PendingAttachment>>,
    drag_hover: Signal<bool>,
    uploading: Signal<bool>,
    on_files: Callback<Vec<dioxus::html::FileData>>,
    on_send: Callback,
    messages: Signal<Vec<MessageListItem>>,
    mention: MentionState,
    tabs: Vec<MentionTab>,
    input_id: &'static str,
    file_input_id: &'static str,
    compact: bool,
    agent_state: Option<Signal<i32>>,
    reply_preview: Option<(String, String)>,
    on_cancel_reply: Option<EventHandler<MouseEvent>>,
    on_remove_mention: Option<EventHandler<String>>,
    focus_selector: Option<&'static str>,
    on_focus: Option<EventHandler<()>>,
    on_blur: Option<EventHandler<()>>,
) -> Element {
    let toast = use_toast();
    let mut input_text = input_text;
    let mut pending_attachments = pending_attachments;
    let mut drag_hover = drag_hover;
    let messages = messages;
    // slash 态内聚在输入区：调用方不再持有 show_slash_menu / selected_index，
    // 也就不会出现一处开菜单、另一处关菜单的交叉。
    let mut show_slash_menu = use_signal(|| false);
    let selected_slash_index = use_signal(|| 0i32);

    let mut input_text_pick = input_text;
    let on_pick_mention = Callback::new(move |_item: MentionCandidate| {
        if let Some((text, caret)) = mention.confirm(&input_text_pick()) {
            input_text_pick.set(text);
            restore_caret(input_id, caret);
        }
    });
    let mut input_text_remove = input_text;
    let remove_mention = on_remove_mention;
    let on_remove_mention = Callback::new(move |key: String| {
        let text = mention.remove_picked(&input_text_remove(), &key);
        input_text_remove.set(text);
        if let Some(handler) = remove_mention {
            handler.call(key);
        }
    });

    // 统一的提交入口：slash 命令在此消费，不把 "/clear" 当消息发出去。
    // 发送按钮与 Alt+回车共用，两个入口行为完全一致。
    let mut submit_text = input_text;
    let mut submit_messages = messages;
    let mut submit_menu = show_slash_menu;
    let submit_index = selected_slash_index;
    let submit_toast = toast;
    let submit = Callback::new(move |_| {
        if submit_menu() {
            let prefix = submit_text().trim_start().to_lowercase();
            let filtered: Vec<(&str, &str)> = SLASH_COMMANDS
                .iter()
                .copied()
                .filter(|(cmd, _)| cmd.starts_with(&prefix))
                .collect();
            if !filtered.is_empty() {
                let idx = submit_index().min(filtered.len() as i32 - 1).max(0) as usize;
                match filtered[idx].0 {
                    "/clear" => {
                        submit_messages.set(Vec::new());
                        submit_text.set(String::new());
                        submit_toast.info("对话已清空");
                    }
                    "/help" => {
                        submit_toast.info("可用指令: /clear - 清空对话, /help - 显示帮助");
                    }
                    _ => {}
                }
                submit_menu.set(false);
                return;
            }
        }
        on_send.call(());
    });

    let filtered: Vec<(&str, &str)> = if show_slash_menu() {
        let prefix = input_text().trim_start().to_lowercase();
        SLASH_COMMANDS
            .iter()
            .copied()
            .filter(|(cmd, _)| cmd.starts_with(&prefix))
            .collect()
    } else {
        Vec::new()
    };

    rsx! {
        div { class: if compact { "relative" } else { "p-3 border-t border-base-300 bg-base-100 relative" },
            if !filtered.is_empty() {
                {
                    let selected = selected_slash_index()
                        .min(filtered.len() as i32 - 1)
                        .max(0) as usize;
                    let input_text = input_text;
                    let menu = show_slash_menu;
                    let index = selected_slash_index;
                    rsx! {
                        div { class: "absolute bottom-full left-3 right-3 mb-1 orz-popover overflow-hidden",
                            for (i, (cmd, desc)) in filtered.iter().enumerate() {
                                {
                                    let cmd = cmd.to_string();
                                    let cmd_clone = cmd.clone();
                                    let desc = *desc;
                                    let is_selected = i == selected;
                                    let mut input_text = input_text;
                                    let mut messages = messages;
                                    let mut menu = menu;
                                    let mut index = index;
                                    rsx! {
                                        div {
                                            class: if is_selected { "px-3 py-2 cursor-pointer bg-primary/10 flex gap-3 items-center" } else { "px-3 py-2 cursor-pointer hover:bg-base-200 flex gap-3 items-center" },
                                            onclick: move |_| {
                                                match cmd_clone.as_str() {
                                                    "/clear" => {
                                                        messages.set(Vec::new());
                                                        input_text.set(String::new());
                                                        toast.info("对话已清空");
                                                    }
                                                    "/help" => {
                                                        toast.info("可用指令: /clear - 清空对话, /help - 显示帮助");
                                                    }
                                                    _ => {}
                                                }
                                                menu.set(false);
                                            },
                                            onmouseenter: move |_| index.set(i as i32),
                                            span { class: "font-mono font-semibold text-primary", "{cmd}" }
                                            span { class: "text-sm text-base-content/60", "{desc}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if mention.is_open() {
                MentionPicker { state: mention, tabs: tabs.clone(), on_pick: on_pick_mention }
            }
            MentionPickedBar { picked: mention.picked(), on_remove: on_remove_mention }
            if let Some((sender, content)) = reply_preview {
                div { class: "flex items-center gap-2 mb-2 rounded-lg border border-base-300 bg-base-200/70 px-2 py-1 text-xs",
                    span { class: "text-base-content/60 shrink-0", "回复" }
                    span { class: "font-medium shrink-0", "{sender}" }
                    span { class: "text-base-content/60 truncate flex-1", "{content}" }
                    button {
                        class: "btn hud-btn btn-ghost btn-xs btn-circle shrink-0",
                        title: "取消回复",
                        onclick: move |e| {
                            if let Some(handler) = on_cancel_reply {
                                handler.call(e);
                            }
                        },
                        "×"
                    }
                }
            }
            if !pending_attachments().is_empty() {
                div { class: if compact { "flex flex-wrap gap-1 px-2 pt-2" } else { "flex flex-wrap gap-2 mb-2" },
                    for att in pending_attachments().iter() {
                        div {
                            class: if compact { "badge orz-tag badge-sm gap-1" } else { "badge orz-tag badge-lg gap-2" },
                            key: "{att.id}",
                            span { "📎" }
                            span { "{att.name}" }
                            button {
                                class: "btn hud-btn btn-ghost btn-xs btn-circle",
                                onclick: {
                                    let id = att.id.clone();
                                    move |_| {
                                        let mut current = pending_attachments.write();
                                        current.retain(|a| a.id != id);
                                    }
                                },
                                "×"
                            }
                        }
                    }
                }
            }
            div { class: if compact {
                "border-t border-base-content/10 p-2 flex items-end gap-2"
            } else {
                "flex items-end gap-2"
            },
                input {
                    r#type: "file",
                    multiple: "true",
                    class: "hidden",
                    id: file_input_id,
                    onchange: move |e| on_files.call(e.files()),
                }
                button {
                    class: "btn hud-btn btn-ghost btn-square",
                    r#type: "button",
                    disabled: uploading(),
                    onclick: move |_| {
                        if let Some(window) = web_sys::window()
                            && let Some(doc) = window.document()
                            && let Some(el) = doc.get_element_by_id(file_input_id)
                        {
                            let _ = el.dyn_into::<web_sys::HtmlElement>().map(|h| h.click());
                        }
                    },
                    if uploading() { Loading { size: "sm" } } else { "📎" }
                }
                textarea {
                    class: {
                        let base = if compact {
                            "textarea textarea-bordered flex-1 bg-transparent resize-none"
                        } else {
                            "textarea textarea-bordered w-full resize-none"
                        };
                        if drag_hover() {
                            format!("{base} border-primary ring-2 ring-primary/40")
                        } else {
                            base.to_string()
                        }
                    },
                    rows: if compact { "1" } else { "2" },
                    id: input_id,
                    value: "{input_text}",
                    // 忙碌时只提示、不阻断：消息照常排队发送。
                    placeholder: if agent_state.map(|s| s() != 0).unwrap_or(false) {
                        "对方正在思考中，你的消息会排队处理…"
                    } else {
                        "输入消息...（Alt+回车发送，@ 提及 Agent / 任务 / 项目）"
                    },
                    onpaste: {
                        let upload = on_files;
                        move |e: dioxus::events::ClipboardEvent| {
                            let files = clipboard_files_from_paste(&e);
                            if files.is_empty() {
                                // 纯文本粘贴照常工作
                                return;
                            }
                            e.prevent_default();
                            upload.call(files);
                        }
                    },
                    ondrop: {
                        let upload = on_files;
                        move |e: dioxus::events::DragEvent| {
                            // 阻止浏览器把拖进来的文件当文档打开
                            e.prevent_default();
                            drag_hover.set(false);
                            let files = e.files();
                            if !files.is_empty() {
                                upload.call(files);
                            }
                        }
                    },
                    ondragover: move |e: dioxus::events::DragEvent| {
                        // HTML5 铁律：不 preventDefault，drop 不会触发
                        e.prevent_default();
                        if !drag_hover() {
                            drag_hover.set(true);
                        }
                    },
                    ondragleave: move |_| {
                        if drag_hover() {
                            drag_hover.set(false);
                        }
                    },
                    onfocus: move |_| {
                        if let Some(handler) = on_focus {
                            handler.call(());
                        }
                    },
                    onblur: move |_| {
                        let Some(selector) = focus_selector else {
                            if let Some(handler) = on_blur {
                                handler.call(());
                            }
                            return;
                        };
                        // 点击对话框内部（消息区等）不收起；焦点真移出才通知调用方
                        gloo_timers::callback::Timeout::new(150, move || {
                            let still_inside = web_sys::window()
                                .and_then(|w| w.document())
                                .and_then(|d| d.active_element())
                                .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
                                .and_then(|el| el.closest(selector).ok().flatten())
                                .is_some();
                            if !still_inside && let Some(handler) = on_blur {
                                handler.call(());
                            }
                        })
                        .forget();
                    },
                    oninput: {
                        let mut selected_slash_index = selected_slash_index;
                        move |e| {
                        let value = e.value();
                        // 光标位置决定 @ 查询边界；读不到就退回末尾（表现为不弹菜单）
                        let caret = read_caret(input_id).unwrap_or(value.len());
                        mention.sync(&value, caret);
                        input_text.set(value.clone());
                        let trimmed = value.trim_start();
                        if trimmed.starts_with('/') && !trimmed.contains(' ') {
                            show_slash_menu.set(true);
                            selected_slash_index.set(0);
                        } else {
                            show_slash_menu.set(false);
                        }
                        }
                    },
                    onkeydown: {
                        let mut input_text = input_text;
                        let mut menu = show_slash_menu;
                        let mut index = selected_slash_index;
                        move |e| {
                            // @ 菜单优先级最高，其次 slash，最后 Alt+回车
                            if mention.is_open() {
                                match e.key() {
                                    Key::ArrowDown => {
                                        e.prevent_default();
                                        mention.move_selection(1);
                                        return;
                                    }
                                    Key::ArrowUp => {
                                        e.prevent_default();
                                        mention.move_selection(-1);
                                        return;
                                    }
                                    Key::Enter if e.modifiers().is_empty() => {
                                        e.prevent_default();
                                        if let Some((text, caret)) = mention.confirm(&input_text()) {
                                            input_text.set(text);
                                            restore_caret(input_id, caret);
                                        } else {
                                            mention.close();
                                        }
                                        return;
                                    }
                                    Key::Escape => {
                                        e.prevent_default();
                                        mention.close();
                                        return;
                                    }
                                    _ => {}
                                }
                            }
                            if menu() {
                                let prefix = input_text().trim_start().to_lowercase();
                                let count = SLASH_COMMANDS
                                    .iter()
                                    .filter(|(cmd, _)| cmd.starts_with(&prefix))
                                    .count();
                                if count > 0 {
                                    match e.key() {
                                        Key::ArrowDown => {
                                            e.prevent_default();
                                            index.set((index() + 1).min(count as i32 - 1));
                                            return;
                                        }
                                        Key::ArrowUp => {
                                            e.prevent_default();
                                            index.set((index() - 1).max(0));
                                            return;
                                        }
                                        Key::Enter if e.modifiers().is_empty() => {
                                            e.prevent_default();
                                            submit.call(());
                                            return;
                                        }
                                        Key::Escape => {
                                            e.prevent_default();
                                            menu.set(false);
                                            return;
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            if e.key() == Key::Enter && e.modifiers().alt() {
                                e.prevent_default();
                                submit.call(());
                            }
                        }
                    },
                }
                button {
                    class: if compact { "btn hud-btn btn-primary btn-sm" } else { "btn hud-btn btn-primary" },
                    onclick: move |_| submit.call(()),
                    disabled: input_text().trim().is_empty() && pending_attachments().is_empty(),
                    "发送"
                }
            }
        }
    }
}
