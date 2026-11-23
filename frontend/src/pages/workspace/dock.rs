//! 底部横幅（MMORPG 式：左下信息卡 / 中间对话框 / 右下预留，左右通透）
//!
//! 纯渲染函数：对话状态与信号由页面壳传入。视觉与交互与拆分前一致 ——
//! 聚焦与否仅消息区展开高度不同，布局两态相同。

use dioxus::html::HasFileData;
use dioxus::prelude::*;
use wasm_bindgen::JsCast;

use common::api::{
    AgentListItem, GetAgentRequest, GetAgentResponse, GetProjectRequest, GetTaskRequest,
    GetTaskResponse, MessageListItem, RuntimeStatusRequest, RuntimeStatusResponse,
};

use crate::api::hr::{get_agent, get_runtime_status};
use crate::api::project::{get_project, get_task};
use crate::components::avatar_bubble::collapse_avatar_bubble;
use crate::components::chat::{MessageBubble, TypingIndicator};
use crate::components::hud::HudProgress;
use crate::components::mention_picker::{
    MentionCandidate, MentionPickedBar, MentionPicker, MentionState, mention_kinds_for,
    mention_tabs,
};
use crate::components::ring_progress::RingProgress;
use crate::components::state::Loading;
use crate::hooks::use_attachments::{AttachmentComposer, clipboard_files_from_paste};
use crate::utils::mention::{read_caret, restore_caret};
use crate::utils::{
    avatar_initials,
    number::format_compact_count,
    status::{agent_runtime_badge, progress_tone, tag_chip, task_status_badge, task_status_text},
    time::format_time_hm,
};

use super::helpers::{WORKSPACE_CHAT_INPUT_ID, WORKSPACE_CHAT_SCROLL_ID, agent_runtime_label};

/// 消息发送方角色文本（0=用户 / 1=Agent / 2=系统）
fn sender_role_label(role: i32) -> &'static str {
    match role {
        0 => "用户",
        1 => "Agent",
        2 => "系统",
        _ => "未知",
    }
}

/// 思考场景中文标签（后端 `scene` 为英文标识）
fn think_scene_label(scene: &str) -> &str {
    match scene {
        "awaken" => "唤醒",
        "settle" => "沉淀",
        "summary" => "总结",
        "compact" => "记忆压缩",
        "intent-analyze" => "意图分析",
        other => other,
    }
}

/// 把底部消息区滚到最新一条
///
/// 收起态高度不足以滚动（`scrollTop` 恒为 0），展开后若不主动滚动，
/// 会停在当前加载的最早一条上 —— 表现为「一点开就回到第一条」。
pub fn scroll_to_latest() {
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
        return;
    };
    let Some(el) = doc.get_element_by_id(WORKSPACE_CHAT_SCROLL_ID) else {
        return;
    };
    if let Ok(el) = el.dyn_into::<web_sys::HtmlElement>() {
        el.set_scroll_top(el.scroll_height());
    }
}

/// 底部横幅
///
/// `on_load_older` 由页面壳提供（上拉加载更早历史），本层不碰分页细节。
#[allow(clippy::too_many_arguments)]
pub fn bottom_dock(
    chat_title: String,
    selected_agent: Option<AgentListItem>,
    show_receiver: bool,
    focus: bool,
    on_exit_focus: EventHandler<()>,
    chat_messages: Signal<Vec<MessageListItem>>,
    chat_focused: Signal<bool>,
    chat_is_typing: Signal<bool>,
    chat_input: Signal<String>,
    chat_project_id: Signal<Option<String>>,
    attachments: AttachmentComposer,
    mention: MentionState,
    auto_mention: Signal<Option<String>>,
    send_trigger: Signal<bool>,
    on_load_older: EventHandler<()>,
    // 当前专注的 Agent；Some 时浮层自己开启运行时轮询（无值 = 不在专注态）
    focus_agent_id: Option<String>,
) -> Element {
    // 专注现场：浮层内部闭环 —— 轮询目标由 effect 跟 props 同步，
    // 运行时 / 任务 / 用量 / 项目名四份快照只被本组件订阅，
    // 信息变化只重渲染浮层本身
    let mut active_agent = use_signal(|| Option::<String>::None);
    let focus_runtime = use_signal(|| Option::<RuntimeStatusResponse>::None);
    let focus_task = use_signal(|| Option::<GetTaskResponse>::None);
    let focus_profile = use_signal(|| Option::<GetAgentResponse>::None);
    // 项目名：拿到 project_id 后由浮层自己拉详情解析，不依赖上层传快照
    let focus_project = use_signal(|| Option::<String>::None);
    use_effect(use_reactive!(|focus_agent_id| {
        // props → Signal：future 闭包只看 peek，props 更新不会重启循环
        if active_agent.peek().as_ref() != focus_agent_id.as_ref() {
            active_agent.set(focus_agent_id.clone());
        }
    }));
    // Agent 专注运行时轮询：5 秒一次拉「这个 Agent 此刻在处理什么」。
    // 目标切换时按需重拉任务详情（含产物）；离开专注即清空，
    // 避免预览区残留上一个 Agent 的现场。
    use_future(move || {
        let mut focus_runtime = focus_runtime;
        let mut focus_task = focus_task;
        let mut focus_profile = focus_profile;
        let mut focus_project = focus_project;
        async move {
            // 上一个轮询的 Agent：切换目标时先清空，避免新 Agent 顶着旧现场
            let mut last_agent = Option::<String>::None;
            // 上一个解析过名称的 project_id：项目名很少变，只在 id 变化时拉详情
            let mut last_project = Option::<String>::None;
            // 任务详情 / 用量统计刷新节拍（每 6 拍 = 30 秒）
            let mut task_tick: u32 = 0;
            let mut profile_tick: u32 = 0;
            loop {
                match active_agent.peek().clone() {
                    Some(aid) => {
                        if last_agent.as_deref() != Some(aid.as_str()) {
                            last_agent = Some(aid.clone());
                            task_tick = 0;
                            profile_tick = 0;
                            // 只在确有旧值时才清：`Signal::set` 会无条件通知订阅者，
                            // 首次进入专注时这三个信号本来就是 None，空写只会多一次重渲染
                            if focus_runtime.peek().is_some() {
                                focus_runtime.set(None);
                            }
                            if focus_task.peek().is_some() {
                                focus_task.set(None);
                            }
                            if focus_profile.peek().is_some() {
                                focus_profile.set(None);
                            }
                            if focus_project.peek().is_some() {
                                focus_project.set(None);
                            }
                            last_project = None;
                        }
                        if !crate::utils::page_hidden() {
                            if let Ok(rt) =
                                get_runtime_status(RuntimeStatusRequest { id: aid.clone() }).await
                            {
                                let tid = rt.task_id.clone();
                                // 值没变就不写：`Signal::set` 无条件通知订阅者，
                                // 每 5 秒空写一次会让整页白白重渲染
                                if focus_runtime.peek().as_ref() != Some(&rt) {
                                    focus_runtime.set(Some(rt));
                                }
                                match tid {
                                    Some(tid) => {
                                        let task_changed =
                                            focus_task.peek().as_ref().is_none_or(|t| t.id != tid);
                                        if (task_changed || task_tick.is_multiple_of(6))
                                            && let Ok(task) = get_task(GetTaskRequest {
                                                id: tid,
                                                with_artifacts: Some(true),
                                                ..Default::default()
                                            })
                                            .await
                                        {
                                            focus_task.set(Some(task));
                                        }
                                    }
                                    None => {
                                        if focus_task.peek().is_some() {
                                            focus_task.set(None);
                                        }
                                    }
                                }
                                task_tick = task_tick.wrapping_add(1);
                            }

                            // 项目上下文：project_id 变化时才拉一次项目详情（名称很少变），
                            // 由浮层自己解析，不再依赖上层传「id → 名称」快照
                            let pid = focus_runtime
                                .peek()
                                .as_ref()
                                .and_then(|rt| rt.project_id.clone())
                                .or_else(|| {
                                    focus_task
                                        .peek()
                                        .as_ref()
                                        .and_then(|t| t.project_id.clone())
                                });
                            if pid != last_project {
                                last_project = pid.clone();
                                match pid {
                                    Some(pid) => {
                                        if let Ok(p) = get_project(GetProjectRequest {
                                            id: pid,
                                            ..Default::default()
                                        })
                                        .await
                                        {
                                            focus_project.set(Some(p.name.clone()));
                                        }
                                    }
                                    None => {
                                        if focus_project.peek().is_some() {
                                            focus_project.set(None);
                                        }
                                    }
                                }
                            }

                            // 用量：token 走「近 60 分钟」窗口（与对话页运行统计同口径），
                            // 上下文长度是内存瞬时值。30 秒一次，避免高频拉统计拖慢专注视图。
                            if profile_tick.is_multiple_of(6) {
                                let end_ms = crate::utils::now_ms();
                                // 只要窗口汇总（token / 上下文），不画趋势图：
                                // 不传 interval，走默认粗粒度，避免白拉 60 个时间点
                                let req = GetAgentRequest {
                                    id: aid.clone(),
                                    with_model_call_stats: Some(true),
                                    stats_time_start: Some(end_ms - 60 * 60_000),
                                    stats_time_end: Some(end_ms),
                                    ..Default::default()
                                };
                                if let Ok(profile) = get_agent(req).await {
                                    focus_profile.set(Some(profile));
                                }
                            }
                            profile_tick = profile_tick.wrapping_add(1);
                        }
                    }
                    None => {
                        last_agent = None;
                        last_project = None;
                        task_tick = 0;
                        profile_tick = 0;
                        // 已是 None 就不再写，避免非专注态空刷
                        if focus_runtime.peek().is_some() {
                            focus_runtime.set(None);
                        }
                        if focus_task.peek().is_some() {
                            focus_task.set(None);
                        }
                        if focus_profile.peek().is_some() {
                            focus_profile.set(None);
                        }
                        if focus_project.peek().is_some() {
                            focus_project.set(None);
                        }
                    }
                }
                // 有目标且已处理过 → 5 秒常规轮询；空闲 / 目标刚变化 → 500ms
                // 快速醒来。否则进入专注时要干等一个长节拍才看到现场。
                let delay = {
                    let target = active_agent.peek().clone();
                    if target.is_some() && target == last_agent {
                        5_000u32
                    } else {
                        500u32
                    }
                };
                gloo_timers::future::TimeoutFuture::new(delay).await;
            }
        }
    });

    let focused = *chat_focused.read() || focus;
    let msgs = chat_messages.read();
    let is_typing = *chat_is_typing.read();
    let input_val = chat_input.read().clone();
    let mut chat_focused = chat_focused;
    let mut send_trigger = send_trigger;
    let mut pending_attachments = attachments.pending;
    let mut drag_hover = attachments.drag_hover;
    let uploading = attachments.uploading;
    let on_files = attachments.on_files;

    // 专注预览数据：运行时快照 + 当前处理消息（从已加载会话里按 id 找）+ 任务详情
    let runtime = focus_runtime.read().clone();
    let task = focus_task.read().clone();
    let focus_msg = runtime
        .as_ref()
        .and_then(|rt| rt.current_message_id.as_ref())
        .and_then(|mid| msgs.iter().find(|m| &m.message_id == mid).cloned());
    // 纯附件消息没有正文，退回附件名，避免预览区一片空白
    let focus_msg_preview = focus_msg.as_ref().map(|m| {
        let text = if m.content.trim().is_empty() {
            m.file_meta
                .as_ref()
                .map(|f| format!("📎 {}", f.name))
                .unwrap_or_default()
        } else {
            m.content.clone()
        };
        text.chars().take(220).collect::<String>()
    });
    let runtime_state_label = match runtime.as_ref().map(|r| r.state.as_str()) {
        Some("busy") => "忙碌",
        Some("resting") => "休息",
        Some("idle") => "空闲",
        _ => "离线",
    };

    // 项目名：内部轮询按 project_id 拉过详情后回填，组件不再依赖外部快照
    let focus_project_name = focus_project.read().clone();

    // 用量：token 为近 60 分钟窗口累计，上下文长度为最近一次 LLM 调用的瞬时值。
    // 两者同源于一次 get_agent 响应，避免错代混用。
    let profile = focus_profile.read();
    let token_summary = profile
        .as_ref()
        .and_then(|p| p.model_call_stats.as_ref())
        .and_then(|m| m.token_summary.as_ref());
    let tokens_input = token_summary.map(|t| t.total_tokens_input);
    let tokens_output = token_summary.map(|t| t.total_tokens_output);
    // 缓存命中 / 推理 token 是 input / output 的**子集**，不参与「合计」
    let tokens_cached = token_summary.map(|t| t.total_cached_tokens).unwrap_or(0);
    let tokens_reasoning = token_summary.map(|t| t.total_reasoning_tokens).unwrap_or(0);
    let context_usage = profile.as_ref().and_then(|p| {
        p.context_length
            .map(|len| (len, p.context_length_threshold))
    });

    // @ 提及浮层：鼠标点选候选（组件已先把高亮对齐到鼠标项）→ 写回文本 + 复位光标
    let mut mention_input = chat_input;
    let on_pick_mention = Callback::new(move |_item: MentionCandidate| {
        if let Some((text, caret)) = mention.confirm(&mention_input()) {
            mention_input.set(text);
            restore_caret(WORKSPACE_CHAT_INPUT_ID, caret);
        }
    });
    // 摘掉「已提及」胶囊：同步把正文里的提及语法串删掉
    let mut mention_remove_input = chat_input;
    let mut mention_remove_auto = auto_mention;
    let on_remove_mention = Callback::new(move |key: String| {
        let text = mention.remove_picked(&mention_remove_input(), &key);
        mention_remove_input.set(text);
        // 用户手动摘除后，不再视为「自动写入的内容」，
        // 否则切任务时会被当成可覆盖的旧锚定
        mention_remove_auto.set(None);
    });
    // 类型 Tab 随上下文变化（项目会话时不再允许 @ 项目本身）
    let mention_tab_list = mention_tabs(&mention_kinds_for(chat_project_id().as_deref()));

    rsx! {
        // 聚焦模式：整块铺满场景，左侧聊天 / 右侧预览；普通模式保持底部横幅
        div { class: if focus {
            "absolute inset-0 z-30 p-4 flex items-stretch gap-4 bg-base-100/95 backdrop-blur-sm pointer-events-auto"
        } else {
            "absolute bottom-3 left-3 right-3 z-10 flex items-end gap-3 pointer-events-none"
        },

            // ---- 左下角：当前对话上下文 / 选中 Agent 信息卡（聚焦模式由右侧预览承担） ----
            if !focus {
            div { class: "pointer-events-auto w-64 flex-shrink-0 hud-glass rounded-xl p-3",
                if let Some(a) = &selected_agent {
                    div { class: "flex items-center gap-2 min-w-0",
                        div { class: "w-8 h-8 rounded-full bg-primary/15 border border-primary/40 flex items-center justify-center text-xs font-semibold text-primary flex-shrink-0",
                            "{avatar_initials(&a.name)}"
                        }
                        span { class: "text-sm font-medium truncate", "{a.name}" }
                        span { class: "ml-auto flex-shrink-0 {agent_runtime_badge(a.runtime_state)}",
                            "{agent_runtime_label(a.runtime_state)}"
                        }
                    }
                    if !a.roles.is_empty() {
                        div { class: "flex flex-wrap gap-1 mt-1.5",
                            for role in a.roles.iter().take(3) {
                                span { class: "{tag_chip()}", "{role}" }
                            }
                        }
                    }
                    if let Some(desc) = &a.description {
                        p { class: "text-xs text-base-content/60 mt-1.5 line-clamp-2 leading-relaxed",
                            "{desc}"
                        }
                    }
                } else {
                    div { class: "text-sm font-medium", "{chat_title}" }
                    div { class: "text-xs text-base-content/50 mt-1",
                        "点击图中席位，切换与该 Agent 的对话"
                    }
                }
            }
            }

            // ---- 中间：对话框；聚焦与否仅消息区展开高度不同，其余两态完全一致 ----
            // ⚠️ 这里刻意不加 `overflow-hidden`：@ 提及浮层（`.mention-menu` 用
            // `bottom: 100%` 悬在输入行上方）属于「溢出父容器」的定位元素，
            // 一旦父级裁剪就会被整块切掉，菜单永远看不见。圆角改为交给消息区
            // 自身 `rounded-t-xl` 裁剪（滚动条也随之内收）。
            div {
                id: "workspace-chat-dialog",
                class: if focus {
                    "pointer-events-auto relative flex-1 min-w-0 h-full min-h-0 hud-glass rounded-xl border border-base-content/10 flex flex-col"
                } else {
                    "pointer-events-auto relative flex-1 min-w-0 hud-glass rounded-xl border border-base-content/10 flex flex-col"
                },
                onclick: move |_| {
                    chat_focused.set(true);
                    // 点击对话框任意位置即聚焦输入框
                    if let Some(window) = web_sys::window()
                        && let Some(doc) = window.document()
                        && let Some(el) = doc.get_element_by_id(WORKSPACE_CHAT_INPUT_ID) {
                        let _ = el.dyn_into::<web_sys::HtmlElement>().map(|h| h.focus());
                    }
                },
                // 消息区：未聚焦收起为最近 3 条 + 渐隐蒙版；聚焦展开为可滚动列表
                // 聚焦态滚到顶 → 上拉续拉更早的历史（页大小 HISTORY_PAGE_SIZE，
                // 因此「一次拉多少」与「能看到多早」解耦）
                div {
                    id: WORKSPACE_CHAT_SCROLL_ID,
                    class: if focus {
                        "flex-1 min-h-0 overflow-y-auto p-3 rounded-t-xl"
                    } else if focused {
                        "overflow-y-auto p-3 max-h-48 rounded-t-xl"
                    } else {
                        "p-2 max-h-24 overflow-hidden rounded-t-xl"
                    },
                    style: if focused { "" } else { "opacity: 0.7; mask-image: linear-gradient(to bottom, transparent 0%, black 30%, black 100%); -webkit-mask-image: linear-gradient(to bottom, transparent 0%, black 30%, black 100%);" },
                    onscroll: move |e| {
                        // 头像浮层用展开瞬间的视口锚点（position: fixed），滚动后先收起
                        collapse_avatar_bubble();
                        if e.scroll_top() == 0.0 {
                            on_load_older.call(());
                        }
                    },
                    if msgs.is_empty() && !is_typing {
                        div { class: "text-center text-sm text-base-content/40 py-2",
                            "💬 输入消息开始对话"
                        }
                    } else {
                        div { class: "space-y-1",
                            // 聚焦时渲染全部**已加载**消息（越往上越早，
                            // 由 load_older 逐页追加）；未聚焦只留最近 3 条
                            for msg in (if focused { msgs.iter().collect::<Vec<_>>() } else { msgs.iter().rev().take(3).rev().collect::<Vec<_>>() }).into_iter() {
                                MessageBubble {
                                    msg: msg.clone(),
                                    show_receiver,
                                    // 只有全屏专注的消息区够高、且无裁切祖先，才开资料卡浮层
                                    avatar_popover: focus,
                                    key: "{msg.message_id}",
                                }
                            }
                            if is_typing {
                                TypingIndicator {}
                            }
                        }
                    }
                }
                // 输入区：@ 提及浮层与输入行同属一个 relative 容器，
                // 浮层 `.mention-menu` 的 `bottom: 100%` 即贴着输入行上沿
                // 向上展开（悬在消息区之上，与对话页观感一致）
                div { class: "relative",
                    if mention.is_open() {
                        MentionPicker {
                            state: mention,
                            tabs: mention_tab_list.clone(),
                            on_pick: on_pick_mention,
                        }
                    }
                    MentionPickedBar {
                        picked: mention.picked(),
                        on_remove: on_remove_mention,
                    }
                    // 待发送附件：与对话页同一套胶囊，可逐个摘除
                    if !pending_attachments.read().is_empty() {
                        div { class: "flex flex-wrap gap-1 px-2 pt-2",
                            for att in pending_attachments.read().iter() {
                                div { class: "badge orz-tag badge-sm gap-1", key: "{att.id}",
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
                    // 输入行：两态完全一致（无标题切换 / 无收起按钮）
                    div { class: "border-t border-base-content/10 p-2 flex items-end gap-2",
                        input {
                            r#type: "file",
                            multiple: "true",
                            class: "hidden",
                            id: "workspace-chat-file-input",
                            onchange: move |e| on_files.call(e.files()),
                        }
                        button {
                            class: "btn hud-btn btn-ghost btn-square",
                            r#type: "button",
                            disabled: uploading(),
                            onclick: move |_| {
                                if let Some(window) = web_sys::window()
                                    && let Some(doc) = window.document()
                                    && let Some(el) = doc.get_element_by_id("workspace-chat-file-input")
                                {
                                    let _ = el.dyn_into::<web_sys::HtmlElement>().map(|h| h.click());
                                }
                            },
                            if uploading() {
                                Loading { size: "sm" }
                            } else {
                                "📎"
                            }
                        }
                        textarea {
                            class: if drag_hover() {
                                "textarea textarea-bordered flex-1 bg-transparent resize-none border-primary ring-2 ring-primary/40"
                            } else {
                                "textarea textarea-bordered flex-1 bg-transparent resize-none"
                            },
                            id: WORKSPACE_CHAT_INPUT_ID,
                            rows: "1",
                            placeholder: "输入消息（Alt+回车发送，@ 提及 Agent / 任务 / 项目）...",
                            value: "{input_val}",
                            onpaste: move |e: dioxus::events::ClipboardEvent| {
                                let files = clipboard_files_from_paste(&e);
                                if files.is_empty() {
                                    // 剪贴板里没有文件 → 纯文本粘贴照常
                                    return;
                                }
                                e.prevent_default();
                                on_files.call(files);
                            },
                            ondrop: move |e: dioxus::events::DragEvent| {
                                // 阻止浏览器把拖进来的文件当文档打开
                                e.prevent_default();
                                drag_hover.set(false);
                                let files = e.files();
                                if !files.is_empty() {
                                    on_files.call(files);
                                }
                            },
                            ondragover: move |e: dioxus::events::DragEvent| {
                                // 不 preventDefault，drop 事件不会触发
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
                            onfocus: move |_| chat_focused.set(true),
                            onblur: move |_| {
                                // 点击对话框内部（消息区等）不收起；仅焦点真正移出对话框才收起
                                gloo_timers::callback::Timeout::new(150, move || {
                                    let still_inside = web_sys::window()
                                        .and_then(|w| w.document())
                                        .and_then(|d| d.active_element())
                                        .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
                                        .and_then(|el| el.closest("#workspace-chat-dialog").ok().flatten())
                                        .is_some();
                                    if !still_inside {
                                        chat_focused.set(false);
                                    }
                                })
                                .forget();
                            },
                            oninput: move |e| {
                                let value = e.value();
                                // 光标位置决定 @ 查询的边界；读不到就退回文本末尾（表现为不弹菜单）
                                let caret = read_caret(WORKSPACE_CHAT_INPUT_ID)
                                    .unwrap_or(value.len());
                                mention.sync(&value, caret);
                                mention_input.set(value);
                            },
                            onkeydown: move |e| {
                                // @ 菜单开启时优先拦截按键：↑↓ 选择 / Enter 确认 / Esc 取消
                                if mention.is_open() {
                                    if e.key() == Key::ArrowDown {
                                        e.prevent_default();
                                        mention.move_selection(1);
                                        return;
                                    }
                                    if e.key() == Key::ArrowUp {
                                        e.prevent_default();
                                        mention.move_selection(-1);
                                        return;
                                    }
                                    if e.key() == Key::Enter && e.modifiers().is_empty() {
                                        e.prevent_default();
                                        if let Some((text, caret)) = mention.confirm(&mention_input()) {
                                            mention_input.set(text);
                                            restore_caret(WORKSPACE_CHAT_INPUT_ID, caret);
                                        } else {
                                            // 无候选时按 Enter 只收起菜单，不误当换行
                                            mention.close();
                                        }
                                        return;
                                    }
                                    if e.key() == Key::Escape {
                                        e.prevent_default();
                                        mention.close();
                                        return;
                                    }
                                }
                                // Alt+回车 发送；普通回车换行（默认行为，不拦截）
                                if e.key() == Key::Enter && e.modifiers().alt() {
                                    e.prevent_default();
                                    send_trigger.set(true);
                                }
                            }
                        }
                        button {
                            class: "btn hud-btn btn-primary btn-sm",
                            onclick: move |_| send_trigger.set(true),
                            "发送"
                        }
                    }
                }
            }

            // ---- 聚焦模式右侧：Agent 运行时现场（当前消息 / 任务 / 项目 / 产物） ----
            if focus {
                div { class: "pointer-events-auto w-80 flex-shrink-0 hud-glass rounded-xl border border-base-content/10 flex flex-col overflow-hidden",
                    div { class: "p-3 border-b border-base-content/10 flex items-center gap-2",
                        if let Some(a) = &selected_agent {
                            span { class: "text-sm font-semibold truncate", "{a.name}" }
                            span { class: "ml-auto flex-shrink-0 {agent_runtime_badge(a.runtime_state)}",
                                {runtime
                                    .as_ref()
                                    .map(|_| runtime_state_label.to_string())
                                    .unwrap_or_else(|| agent_runtime_label(a.runtime_state).to_string())}
                            }
                        } else {
                            span { class: "text-sm font-semibold", "预览" }
                        }
                        button {
                            class: "btn hud-btn btn-ghost btn-xs",
                            title: "退出专注",
                            onclick: move |_| on_exit_focus.call(()),
                            "✕"
                        }
                    }
                    div { class: "flex-1 overflow-y-auto p-3 space-y-3",
                        if let Some(a) = &selected_agent {
                            if !a.roles.is_empty() {
                                div { class: "flex flex-wrap gap-1",
                                    for role in a.roles.iter().take(4) {
                                        span { class: "{tag_chip()}", "{role}" }
                                    }
                                }
                            }
                            if let Some(desc) = &a.description {
                                p { class: "text-xs text-base-content/60 leading-relaxed", "{desc}" }
                            }
                        }

                        // ---- 当前处理的消息 ----
                        div { class: "text-xs font-medium text-base-content/60", "当前处理" }
                        div { class: "hud-glass rounded-lg p-3",
                            if let (Some(msg), Some(preview)) = (&focus_msg, &focus_msg_preview) {
                                div { class: "flex items-center gap-1.5 text-[11px] text-base-content/50",
                                    span { "{sender_role_label(msg.from_role)}" }
                                    span { "·" }
                                    span { "{format_time_hm(msg.created_at)}" }
                                }
                                p { class: "text-sm mt-1 whitespace-pre-wrap line-clamp-6", "{preview}" }
                            } else if let Some(rt) = runtime.as_ref() {
                                if rt.state == "busy" {
                                    div { class: "text-sm", "正在处理一条尚未加载到当前会话的消息" }
                                    div { class: "text-xs text-base-content/50 mt-1",
                                        "展开消息区或稍等片刻即可看到正文。"
                                    }
                                } else {
                                    div { class: "text-sm", "当前空闲" }
                                    div { class: "text-xs text-base-content/50 mt-1",
                                        "接到新消息时会自动出现在这里。"
                                    }
                                }
                            } else {
                                div { class: "text-sm text-base-content/50", "正在获取运行态…" }
                            }
                        }

                        // ---- 任务上下文 ----
                        div { class: "text-xs font-medium text-base-content/60", "任务上下文" }
                        if let Some(t) = &task {
                            div { class: "hud-glass rounded-lg p-3 space-y-2",
                                div { class: "flex items-start gap-2",
                                    span { class: "text-sm font-medium flex-1 min-w-0 break-words",
                                        "{t.title}"
                                    }
                                    span { class: "flex-shrink-0 {task_status_badge(t.status)}",
                                        "{task_status_text(t.status)}"
                                    }
                                }
                                if let Some(desc) = &t.description
                                    && !desc.trim().is_empty()
                                {
                                    p { class: "text-xs text-base-content/60 leading-relaxed line-clamp-3",
                                        "{desc}"
                                    }
                                }
                                div { class: "flex items-center gap-2",
                                    HudProgress {
                                        value: t.progress,
                                        tone: Some(progress_tone(t.progress).to_string()),
                                        show_value: Some(true),
                                    }
                                }
                                // 产物：证明「这件事真的在产出东西」
                                if let Some(artifacts) = &t.artifacts
                                    && !artifacts.is_empty()
                                {
                                    div { class: "pt-1",
                                        div { class: "text-[11px] text-base-content/50 mb-1",
                                            "产物 · {artifacts.len()} 项"
                                        }
                                        div { class: "space-y-1",
                                            for art in artifacts.iter().take(6) {
                                                div { class: "text-xs flex items-center gap-1 min-w-0", key: "{art.id}",
                                                    span { "📄" }
                                                    span { class: "truncate", "{art.name}" }
                                                }
                                            }
                                            if artifacts.len() > 6 {
                                                div { class: "text-[11px] text-base-content/40",
                                                    "另有 {artifacts.len() - 6} 项未展开"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } else if runtime
                            .as_ref()
                            .is_none_or(|rt| rt.task_id.is_some())
                        {
                            // 运行态未到 / 任务已挂上但详情未回：都算「加载中」
                            div { class: "hud-glass rounded-lg p-3 text-sm text-base-content/50",
                                "任务详情加载中…"
                            }
                        } else {
                            div { class: "hud-glass rounded-lg p-3",
                                div { class: "text-sm text-base-content/50", "当前没有关联任务" }
                            }
                        }

                        // ---- 用量：token 窗口累计 + 上下文水位（与对话页运行统计同源） ----
                        div { class: "text-xs font-medium text-base-content/60", "用量" }
                        div { class: "hud-glass rounded-lg p-3",
                            if let (Some(input), Some(output)) = (tokens_input, tokens_output) {
                                div { class: "text-[11px] text-base-content/50", "Token（近 60 分钟）" }
                                // 与对话页紧凑统计同口径：输入 / 输出两项固定，
                                // 缓存命中与思考均为其子集，> 0 才入列（0 = 未上报或无推理）。
                                // 不设「合计」：对话页也没有，且加减关系用户可自行判断。
                                div { class: "grid grid-cols-2 gap-1.5 mt-1.5 text-center",
                                    div { class: "bg-base-200/60 rounded px-1 py-1",
                                        div { class: "text-[10px] text-base-content/50", "输入" }
                                        div { class: "font-mono text-sm", "{format_compact_count(input)}" }
                                    }
                                    div { class: "bg-base-200/60 rounded px-1 py-1",
                                        div { class: "text-[10px] text-base-content/50", "输出" }
                                        div { class: "font-mono text-sm", "{format_compact_count(output)}" }
                                    }
                                    if tokens_cached > 0 {
                                        div { class: "bg-base-200/60 rounded px-1 py-1",
                                            div { class: "text-[10px] text-base-content/50", "缓存命中" }
                                            div { class: "font-mono text-sm",
                                                "{format_compact_count(tokens_cached)}"
                                            }
                                        }
                                    }
                                    if tokens_reasoning > 0 {
                                        div { class: "bg-base-200/60 rounded px-1 py-1",
                                            div { class: "text-[10px] text-base-content/50", "思考" }
                                            div { class: "font-mono text-sm",
                                                "{format_compact_count(tokens_reasoning)}"
                                            }
                                        }
                                    }
                                }
                            } else {
                                div { class: "text-xs text-base-content/50", "暂无 Token 统计" }
                            }
                            // 上下文水位：阈值缺失时只能给原始读数（并说明原因）
                            if let Some((len, threshold)) = context_usage {
                                div { class: "flex items-center gap-3 mt-3 pt-2 border-t border-base-content/10",
                                    if let Some(max) = threshold
                                        && max > 0
                                    {
                                        RingProgress {
                                            value: len,
                                            max,
                                            caption: Some("上下文".to_string()),
                                            title: Some(format!(
                                                "上下文 {} / 压缩阈值 {} tokens",
                                                format_compact_count(len),
                                                format_compact_count(max),
                                            )),
                                            size: Some(44),
                                        }
                                    }
                                    div { class: "min-w-0 text-xs text-base-content/60",
                                        div { "上下文长度" }
                                        div { class: "font-mono text-sm text-base-content/80",
                                            "{format_compact_count(len)}"
                                        }
                                        if let Some(max) = threshold
                                            && max > 0
                                        {
                                            div { class: "text-[10px] text-base-content/40",
                                                "压缩阈值 {format_compact_count(max)}"
                                            }
                                        } else {
                                            div { class: "text-[10px] text-base-content/40",
                                                "未配置压缩阈值，暂无百分比"
                                            }
                                        }
                                    }
                                }
                            } else {
                                div { class: "text-[11px] text-base-content/40 mt-2",
                                    "上下文水位：Agent 尚未思考过"
                                }
                            }
                        }

                        // ---- 项目上下文 + 思考快照 ----
                        div { class: "text-xs font-medium text-base-content/60", "运行现场" }
                        div { class: "hud-glass rounded-lg p-3 space-y-1 text-xs text-base-content/60",
                            div { class: "flex items-center gap-1",
                                span { "项目" }
                                span { class: "text-base-content/80 truncate",
                                    {focus_project_name.clone().unwrap_or_else(|| "未关联项目".to_string())}
                                }
                            }
                            if let Some(think) = runtime.as_ref().and_then(|rt| rt.think_runtime.as_ref()) {
                                div { class: "flex items-center gap-1",
                                    span { "场景" }
                                    span { class: "text-base-content/80", "{think_scene_label(&think.scene)}" }
                                }
                                div { class: "flex items-center gap-1",
                                    span { "轮次" }
                                    span { class: "text-base-content/80",
                                        "{think.round} / {think.max_rounds}"
                                    }
                                }
                                div { class: "flex items-center gap-1",
                                    span { "工具调用" }
                                    span { class: "text-base-content/80", "{think.tool_call_count} 次" }
                                }
                                div { class: "flex items-center gap-1",
                                    span { "本次思考 Token" }
                                    span { class: "text-base-content/80", "{think.total_tokens}" }
                                }
                            }
                            div { class: "text-[11px] text-base-content/40 pt-1",
                                "工具调用流与浏览器画面将在这里实时展开。"
                            }
                        }
                    }
                }
            }

            // ---- 右下角：隐形占位（问题 7）----
            // 与左段同宽（w-64 flex-shrink-0），把聊天框顶回下部中间，
            // 三段 flex 分布与 18fa2aa3 之前一致；不带 pointer-events-auto、
            // 无内容无按钮，不截获右下视口浮层点击（不回归问题 6 病灶），
            // 刷新按钮保持退役。
            if !focus {
                div { class: "w-64 flex-shrink-0" }
            }
        }
    }
}

#[cfg(test)]
mod problem7_placeholder_tests {
    /// 问题 7 守卫：底部栏右段隐形占位锚点必须在场。
    /// 生产区切分（split("#[cfg(test)]")）防自匹配：include_str! 全文含测试区，
    /// 测试体内字面量若计入会令锚点计数虚高；切分后仅对生产区断言。
    #[test]
    fn bottom_bar_hidden_placeholder_anchor_present() {
        let src = include_str!("dock.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or("");
        // 占位 div 唯一（与左段信息卡/右侧面板 w-64 区分：占位无其他类无内容）
        assert_eq!(
            src.matches("div { class: \"w-64 flex-shrink-0\" }").count(),
            1
        );
        // 占位注释锚点在场（问题 7 落点标记）
        assert_eq!(
            src.matches("// ---- 右下角：隐形占位（问题 7）----")
                .count(),
            1
        );
        // 硬约束 b：右下不得出现 pointer-events 截获层——占位区不得携带 pointer-events-auto
        assert!(!src.contains("w-64 flex-shrink-0 pointer-events-auto"));
    }
}
