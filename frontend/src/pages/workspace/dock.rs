//! 底部横幅（MMORPG 式：左下信息卡 / 中间对话框 / 右下预留，左右通透）
//!
//! 纯渲染函数：对话状态与信号由页面壳传入。
//! 紧凑态只保留消息区；专注态左侧是主信息区（主消息链 / 话题 / 详情），
//! 右侧是总览 + 可拉伸的实时事件中心。

use dioxus::prelude::*;
use wasm_bindgen::JsCast;

use common::api::{
    AgentListItem, GetAgentRequest, GetAgentResponse, GetProjectRequest, GetProjectResponse,
    GetTaskRequest, GetTaskResponse, MessageListItem, RuntimeStatusRequest, RuntimeStatusResponse,
};

use crate::api::hr::{get_agent, get_runtime_status};
use crate::api::project::{get_project, get_task};
use crate::components::avatar_bubble::collapse_avatar_bubble;
use crate::components::chat::{
    AgentTracePanel, ChatInputArea, QuotedPreview, ReplyPreview, RichMessageItem, ThreadChain,
    TypingIndicator, message_preview_text, truncate_preview,
};
use crate::components::hud::HudProgress;
use crate::components::markdown::MarkdownRenderer;
use crate::components::mention_picker::{MentionState, mention_kinds_for, mention_tabs};
use crate::components::ring_progress::RingProgress;
use crate::hooks::use_attachments::AttachmentComposer;
use crate::pages::workspace::live_center::{AgentLiveCenter, LiveDetail, LiveDetailText};
use crate::store::directory::use_directory;
use crate::utils::{
    avatar_initials,
    number::format_compact_count,
    status::{
        agent_runtime_badge, priority_badge, progress_tone, project_status_badge,
        project_status_text, tag_chip, task_status_badge, task_status_text,
    },
    time::format_datetime,
};

use super::helpers::{WORKSPACE_CHAT_INPUT_ID, WORKSPACE_CHAT_SCROLL_ID, agent_runtime_label};

/// 专注态左侧主区当前展示的详情页
#[derive(Clone, PartialEq)]
pub(crate) enum FocusLeftDetail {
    /// 任务完整上下文
    Task,
    /// 项目完整上下文
    Project,
    /// 实时事件中心里某条工具/事件的完整载荷
    LiveText(LiveDetailText),
    /// 完整思考轨迹
    LiveTrace,
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

/// 详情页里的一组「小标题 + 卡片」
#[component]
fn DetailSection(title: String, children: Element) -> Element {
    rsx! {
        section { class: "space-y-1",
            div { class: "text-xs font-medium text-base-content/60", "{title}" }
            div { class: "hud-glass rounded-lg p-3 text-sm leading-relaxed", {children} }
        }
    }
}

/// 详情页里的键值对
fn detail_meta(label: &str, value: String) -> Element {
    rsx! {
        div { class: "min-w-0",
            div { class: "text-[10px] text-base-content/40", "{label}" }
            div { class: "text-xs text-base-content/75 break-words", "{value}" }
        }
    }
}

/// 任务完整上下文：右栏只留摘要，长内容统一铺在左侧
fn task_detail_body(t: &GetTaskResponse, project_name: Option<&str>) -> Element {
    rsx! {
        div { class: "space-y-4",
            div { class: "flex flex-wrap items-center gap-2",
                span { class: "{task_status_badge(t.status)}", "{task_status_text(t.status)}" }
                span { class: "{priority_badge(t.priority)}", "P{t.priority}" }
                span { class: "text-[11px] text-base-content/45", "进度 {t.progress}%" }
                for tag in t.tags.iter() {
                    span { class: "{tag_chip()}", "{tag}" }
                }
            }
            div { class: "grid grid-cols-2 gap-x-6 gap-y-3",
                {detail_meta("任务 ID", t.id.clone())}
                {detail_meta(
                    "所属项目",
                    project_name
                        .map(str::to_string)
                        .or_else(|| t.project_id.clone())
                        .unwrap_or_else(|| "—".to_string()),
                )}
                {detail_meta("分配对象", t.assignee_id.clone())}
                {detail_meta("优先级", t.priority.to_string())}
                {detail_meta("创建者", t.created_by.clone())}
                {detail_meta(
                    "截止时间",
                    t.due_at
                        .map(format_datetime)
                        .unwrap_or_else(|| "—".to_string()),
                )}
            }
            if let Some(desc) = &t.description
                && !desc.trim().is_empty()
            {
                DetailSection { title: "任务描述",
                    MarkdownRenderer { content: desc.clone() }
                }
            }
            if let Some(plan) = &t.execution_plan
                && !plan.trim().is_empty()
            {
                DetailSection { title: "执行计划",
                    MarkdownRenderer { content: plan.clone() }
                }
            }
            if let Some(result) = &t.execution_result
                && !result.trim().is_empty()
            {
                DetailSection { title: "执行结果",
                    MarkdownRenderer { content: result.clone() }
                }
            }
            if !t.acceptance_criteria.trim().is_empty() {
                DetailSection { title: "验收标准",
                    MarkdownRenderer { content: t.acceptance_criteria.clone() }
                }
            }
            if let Some(artifacts) = &t.artifacts
                && !artifacts.is_empty()
            {
                DetailSection { title: format!("交付产物 · {} 项", artifacts.len()),
                    div { class: "space-y-2",
                        for art in artifacts.iter() {
                            div { class: "flex items-start gap-2", key: "{art.id}",
                                span { "📄" }
                                div { class: "min-w-0",
                                    div { class: "text-xs text-base-content/80", "{art.name}" }
                                    if !art.description.is_empty() {
                                        div { class: "text-[11px] text-base-content/45",
                                            "{art.description}"
                                        }
                                    }
                                    div { class: "font-mono text-[10px] text-base-content/30 break-all",
                                        "{art.file_path}"
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

/// 项目完整上下文：描述 / 运作流程 / 指导建议可能很长，放到左侧大宽页
fn project_detail_body(p: &GetProjectResponse) -> Element {
    rsx! {
        div { class: "space-y-4",
            div { class: "flex flex-wrap items-center gap-2",
                span { class: "{project_status_badge(p.status)}", "{project_status_text(p.status)}" }
                span { class: "{priority_badge(p.priority)}", "P{p.priority}" }
                for tag in p.tags.iter() {
                    span { class: "{tag_chip()}", "{tag}" }
                }
            }
            div { class: "grid grid-cols-2 gap-x-6 gap-y-3",
                {detail_meta("项目 ID", p.id.clone())}
                {detail_meta(
                    "负责人",
                    p.owner_agent_id.clone().unwrap_or_else(|| "—".to_string()),
                )}
                {detail_meta("根用户", p.root_user_id.clone())}
                {detail_meta(
                    "开始",
                    p.start_at
                        .map(format_datetime)
                        .unwrap_or_else(|| "—".to_string()),
                )}
                {detail_meta(
                    "截止",
                    p.due_at
                        .map(format_datetime)
                        .unwrap_or_else(|| "—".to_string()),
                )}
                {detail_meta("创建", format_datetime(p.created_at))}
            }
            if let Some(desc) = &p.description
                && !desc.trim().is_empty()
            {
                DetailSection { title: "项目描述",
                    MarkdownRenderer { content: desc.clone() }
                }
            }
            if let Some(workflow) = &p.workflow
                && !workflow.trim().is_empty()
            {
                DetailSection { title: "运作流程",
                    MarkdownRenderer { content: workflow.clone() }
                }
            }
            if let Some(guidance) = &p.guidance
                && !guidance.trim().is_empty()
            {
                DetailSection { title: "指导建议",
                    MarkdownRenderer { content: guidance.clone() }
                }
            }
        }
    }
}

/// 消息列表：专注态与展开态共用的滚动容器。
///
/// ⚠️ 必须独立成组件：右侧现场轮询（runtime / task / profile / project）
/// 每 5s 写 signal，若列表与现场共处一个渲染作用域，任何一次写入都会
/// 重建消息 DOM、`scrollTop` 归零 —— 表现为「一抖回到最上边」。
/// 独立订阅后，本组件只读消息相关 signal，现场变化不再波及滚动位置。
#[component]
fn ChatMessageList(
    focus: bool,
    focused: bool,
    chat_messages: Signal<Vec<MessageListItem>>,
    chat_is_typing: Signal<bool>,
    show_receiver: bool,
    on_load_older: EventHandler<()>,
    thread_root: Signal<Option<String>>,
    left_detail: Signal<Option<FocusLeftDetail>>,
) -> Element {
    // 滚动跟随下沉到消息列表本体：use_effect 在 DOM 提交后执行，
    // 不靠页面壳的 16ms 定时（专注态几十上百条富消息渲染不完，
    // scroll_height() 拿到的是中间值 → 视觉上停在顶部/中途）。
    // ⚠️ 只跟随「用户本就在底部」：上拉加载历史会在头部前置追加，
    // 若无条件滚底，刚拉到的历史会被立刻冲掉。
    let mut at_bottom = use_signal(|| true);
    use_effect(move || {
        // 依赖「末条 id」而非消息数：切会话时两边都是满页（数量相同）也能滚底；
        // 上拉历史只前置追加、末条不变，此处不触发，位置自然保留。
        let tail_id = chat_messages().last().map(|m| m.message_id.clone());
        let typing = *chat_is_typing.read();
        if !at_bottom() {
            return;
        }
        let _ = (tail_id, typing);
        spawn(async move {
            scroll_to_latest();
        });
    });

    let msgs = chat_messages.read();
    let is_typing = *chat_is_typing.read();
    let directory = use_directory();
    let dir = directory.read();
    // 一次建索引，避免每条消息都线性扫全表（专注态可能加载几百条历史）
    let msg_by_id: std::collections::HashMap<&str, &MessageListItem> =
        msgs.iter().map(|m| (m.message_id.as_str(), m)).collect();
    let reply_map: std::collections::HashMap<String, ReplyPreview> = msgs
        .iter()
        .filter_map(|m| {
            let target = msg_by_id.get(m.reply_to_id.as_deref()?)?;
            let text = truncate_preview(&message_preview_text(target));
            Some((
                m.message_id.clone(),
                ReplyPreview {
                    message_id: target.message_id.clone(),
                    root_id: target
                        .root_id
                        .clone()
                        .or_else(|| Some(target.message_id.clone())),
                    sender: dir.sender_name(target),
                    text,
                },
            ))
        })
        .collect();
    let mut thread_root = thread_root;
    let mut left_detail = left_detail;
    let on_open_thread: Option<EventHandler<String>> = Some(Callback::new(move |root: String| {
        left_detail.set(None);
        thread_root.set(Some(root));
    }));

    rsx! {
        div {
            id: WORKSPACE_CHAT_SCROLL_ID,
            class: if focus {
                "h-full overflow-y-auto p-3 rounded-t-xl"
            } else if focused {
                "overflow-y-auto p-3 max-h-48 rounded-t-xl"
            } else {
                "p-2 max-h-24 overflow-hidden rounded-t-xl"
            },
            style: if focused {
                "opacity: 1; mask-image: none; -webkit-mask-image: none;"
            } else {
                "opacity: 0.7; mask-image: linear-gradient(to bottom, transparent 0%, black 30%, black 100%); -webkit-mask-image: linear-gradient(to bottom, transparent 0%, black 30%, black 100%);"
            },
            onscroll: move |_| {
                collapse_avatar_bubble();
                let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
                    return;
                };
                let Some(el) = doc
                    .get_element_by_id(WORKSPACE_CHAT_SCROLL_ID)
                    .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
                else {
                    return;
                };
                let top = el.scroll_top();
                // 留 40px 容差：消息卡片展开/收起会让 max 抖动，
                // 不加容差会导致「明明在底部」被判成离开底部。
                let max_scroll = el.scroll_height() - el.client_height();
                let bottom = top >= max_scroll - 40;
                if at_bottom() != bottom {
                    at_bottom.set(bottom);
                }
                if top == 0 {
                    on_load_older.call(());
                }
            },
            if msgs.is_empty() && !is_typing {
                div { class: "text-center text-sm text-base-content/40 py-2",
                    "💬 输入消息开始对话"
                }
            } else {
                div { class: "space-y-1",
                    for msg in (if focused { msgs.iter().collect::<Vec<_>>() } else { msgs.iter().rev().take(3).rev().collect::<Vec<_>>() }).into_iter() {
                        RichMessageItem {
                            key: "{msg.message_id}",
                            msg: msg.clone(),
                            show_receiver,
                            avatar_popover: focus,
                            collapse_height: if focus { 300 } else if focused { 180 } else { 96 },
                            quoted: reply_map.get(&msg.message_id).map(|r| QuotedPreview {
                                sender: r.sender.clone(),
                                content: r.text.clone(),
                                root_id: r.root_id.clone(),
                            }),
                            on_open_thread,
                        }
                    }
                    if is_typing {
                        TypingIndicator {}
                    }
                }
            }
        }
    }
}

/// 话题浮层：话题链 + 紧凑浮层，独立于消息列表订阅。
///
/// ⚠️ 只读 `thread_root` + 消息快照，不读 runtime —— 去掉了
/// 「现场正在处理的消息 id → 消息链定位」这条交叉边。
#[component]
fn FocusThreadOverlay(
    focus: bool,
    chat_messages: Signal<Vec<MessageListItem>>,
    thread_root: Signal<Option<String>>,
) -> Element {
    let msgs = chat_messages.read();
    let thread_now = thread_root();
    let directory = use_directory();
    let dir = directory.read();
    let msg_by_id: std::collections::HashMap<&str, &MessageListItem> =
        msgs.iter().map(|m| (m.message_id.as_str(), m)).collect();
    let reply_map: std::collections::HashMap<String, ReplyPreview> = msgs
        .iter()
        .filter_map(|m| {
            let target = msg_by_id.get(m.reply_to_id.as_deref()?)?;
            let text = truncate_preview(&message_preview_text(target));
            Some((
                m.message_id.clone(),
                ReplyPreview {
                    message_id: target.message_id.clone(),
                    root_id: target
                        .root_id
                        .clone()
                        .or_else(|| Some(target.message_id.clone())),
                    sender: dir.sender_name(target),
                    text,
                },
            ))
        })
        .collect();

    // 话题根固定保留，再取尾部最多 7 条；当前消息一律按话题末条标记，
    // 不再依赖现场 runtime 的「正在处理消息 id」。
    let thread_items: Vec<MessageListItem> = thread_now
        .as_ref()
        .map(|root| {
            msgs.iter()
                .filter(|m| m.message_id == *root || m.root_id.as_deref() == Some(root.as_str()))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let thread_mode = !thread_items.is_empty();
    let thread_chain_items: Vec<(MessageListItem, bool, String)> = thread_items
        .iter()
        .enumerate()
        .map(|(idx, m)| {
            let is_current = idx + 1 == thread_items.len();
            (m.clone(), is_current, message_preview_text(m))
        })
        .collect();
    let compact_chain: Vec<(MessageListItem, bool, String)> = if thread_chain_items.len() <= 8 {
        thread_chain_items.clone()
    } else {
        let mut items = vec![thread_chain_items[0].clone()];
        items.extend_from_slice(&thread_chain_items[thread_chain_items.len() - 7..]);
        items
    };
    let panel_title = format!("话题 · {} 条", thread_items.len());

    rsx! {
        if focus && thread_mode {
            div {
                class: "absolute inset-0 z-10 flex flex-col rounded-t-xl bg-base-100/95 backdrop-blur-md",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center gap-2 border-b border-base-content/10 px-3 py-2",
                    button {
                        class: "btn hud-btn btn-ghost btn-xs gap-1",
                        onclick: move |_| thread_root.set(None),
                        "← 返回"
                    }
                    span { class: "text-sm font-medium", "{panel_title}" }
                    span { class: "ml-auto text-[10px] text-base-content/40", "话题链" }
                }
                div { class: "flex-1 min-h-0 overflow-y-auto p-3",
                    div { class: "hud-glass rounded-lg p-2",
                        ThreadChain {
                            items: thread_chain_items.clone(),
                            reply_map: reply_map.clone(),
                        }
                    }
                }
            }
        }
        if !focus && thread_mode {
            div {
                class: "absolute inset-x-2 bottom-full z-30 mb-2 max-h-[55vh] overflow-y-auto rounded-xl border border-base-content/10 bg-base-100/95 p-3 shadow-[0_18px_50px_-18px_rgba(0,0,0,0.75)] backdrop-blur-md",
                onclick: move |e| e.stop_propagation(),
                div { class: "flex items-center text-xs font-medium text-base-content/60",
                    span { "{panel_title}" }
                    button {
                        class: "ml-auto text-[10px] text-base-content/40 transition-colors hover:text-primary",
                        onclick: move |_| thread_root.set(None),
                        "关闭话题"
                    }
                }
                div { class: "mt-1.5",
                    ThreadChain { items: compact_chain, reply_map }
                }
            }
        }
    }
}

/// 现场快照句柄：由页面壳创建成一组，传给现场面板（写）与详情浮层（读）。
/// `bottom_dock` 自身只持有句柄、不 `.read()`，轮询写入不会触发页面壳重渲染。
#[derive(Clone, Copy, PartialEq)]
struct SceneSnapshots {
    runtime: Signal<Option<RuntimeStatusResponse>>,
    task: Signal<Option<GetTaskResponse>>,
    profile: Signal<Option<GetAgentResponse>>,
    project: Signal<Option<GetProjectResponse>>,
}

/// 左侧详情浮层：任务 / 项目 / 实时详情。只读现场快照 + 详情槽，
/// 不订阅消息列表 —— 开合详情不影响消息区滚动位置。
#[component]
fn FocusDetailOverlay(
    scene: SceneSnapshots,
    selected_agent: Option<AgentListItem>,
    left_detail: Signal<Option<FocusLeftDetail>>,
) -> Element {
    let detail_now = left_detail();
    let Some(detail) = detail_now else {
        return rsx! {};
    };
    let task = scene.task.read();
    let project = scene.project.read();
    let project_name = project.as_ref().map(|p| p.name.clone());
    let (title, subtitle) = match &detail {
        FocusLeftDetail::Task => (
            "任务详情".to_string(),
            task.as_ref().map(|t| t.title.clone()).unwrap_or_default(),
        ),
        FocusLeftDetail::Project => (
            "项目详情".to_string(),
            project_name.clone().unwrap_or_default(),
        ),
        FocusLeftDetail::LiveText(t) => (t.title.clone(), t.subtitle.clone()),
        FocusLeftDetail::LiveTrace => (
            "完整轨迹".to_string(),
            selected_agent
                .as_ref()
                .map(|a| a.name.clone())
                .unwrap_or_default(),
        ),
    };

    rsx! {
        div {
            class: "absolute inset-0 z-20 flex flex-col rounded-t-xl bg-base-100/95 backdrop-blur-md",
            onclick: move |e| e.stop_propagation(),
            div { class: "flex items-center gap-2 border-b border-base-content/10 px-3 py-2",
                button {
                    class: "btn hud-btn btn-ghost btn-xs gap-1",
                    onclick: move |_| left_detail.set(None),
                    "← 返回"
                }
                div { class: "min-w-0",
                    div { class: "text-sm font-medium truncate", "{title}" }
                    if !subtitle.is_empty() {
                        div { class: "text-[10px] text-base-content/40 truncate", "{subtitle}" }
                    }
                }
            }
            div { class: "flex-1 min-h-0 overflow-y-auto p-4",
                if let FocusLeftDetail::Task = detail {
                    if let Some(t) = task.as_ref() {
                        {task_detail_body(t, project_name.as_deref())}
                    } else {
                        div { class: "text-sm text-base-content/45",
                            "任务详情尚未加载完成，请稍后重试。"
                        }
                    }
                }
                if let FocusLeftDetail::Project = detail {
                    if let Some(p) = project.as_ref() {
                        {project_detail_body(p)}
                    } else {
                        div { class: "text-sm text-base-content/45",
                            "项目详情尚未加载完成，请稍后重试。"
                        }
                    }
                }
                if let FocusLeftDetail::LiveText(ref t) = detail {
                    div { class: "hud-glass rounded-lg p-4",
                        pre { class: "whitespace-pre-wrap break-all font-mono text-xs leading-relaxed text-base-content/75",
                            "{t.body}"
                        }
                    }
                }
                if let FocusLeftDetail::LiveTrace = detail {
                    if let Some(a) = &selected_agent {
                        div { class: "h-full min-h-0",
                            AgentTracePanel { agent_id: a.id.clone() }
                        }
                    }
                }
            }
        }
    }
}

/// 右侧现场面板：runtime / task / project / profile 四份快照的轮询与渲染。
///
/// ⚠️ 独立组件 + 独立订阅：轮询每 5s 写 signal 只重渲染本面板，
/// 不重渲染消息列表与输入框；这是「专注态消息区周期性一抖」的根治点。
#[component]
fn FocusScenePanel(
    scene: SceneSnapshots,
    focus_agent_id: Option<String>,
    selected_agent: Option<AgentListItem>,
    on_exit_focus: EventHandler<()>,
    left_detail: Signal<Option<FocusLeftDetail>>,
) -> Element {
    let SceneSnapshots {
        runtime: focus_runtime,
        task: focus_task,
        profile: focus_profile,
        project: focus_project,
    } = scene;
    let mut active_agent = use_signal(|| Option::<String>::None);
    use_effect(use_reactive!(|focus_agent_id| {
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
            let mut last_agent = Option::<String>::None;
            let mut last_project = Option::<String>::None;
            let mut task_tick: u32 = 0;
            let mut profile_tick: u32 = 0;
            loop {
                match active_agent.peek().clone() {
                    Some(aid) => {
                        if last_agent.as_deref() != Some(aid.as_str()) {
                            last_agent = Some(aid.clone());
                            task_tick = 0;
                            profile_tick = 0;
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
                                            focus_project.set(Some(p));
                                        }
                                    }
                                    None => {
                                        if focus_project.peek().is_some() {
                                            focus_project.set(None);
                                        }
                                    }
                                }
                            }

                            if profile_tick.is_multiple_of(6) {
                                let end_ms = crate::utils::now_ms();
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

    let runtime = focus_runtime.read().clone();
    let task = focus_task.read().clone();
    let project_detail = focus_project.read().clone();
    let profile = focus_profile.read();
    let runtime_state_label = match runtime.as_ref().map(|r| r.state.as_str()) {
        Some("busy") => "忙碌",
        Some("resting") => "休息",
        Some("idle") => "空闲",
        _ => "离线",
    };
    let focus_project_name = project_detail.as_ref().map(|p| p.name.clone());
    let project_loaded = project_detail.is_some();
    let task_loaded = task.is_some();
    let trace_available = selected_agent.is_some();
    let token_summary = profile
        .as_ref()
        .and_then(|p| p.model_call_stats.as_ref())
        .and_then(|m| m.token_summary.as_ref());
    let tokens_input = token_summary.map(|t| t.total_tokens_input);
    let tokens_output = token_summary.map(|t| t.total_tokens_output);
    let tokens_cached = token_summary.map(|t| t.total_cached_tokens).unwrap_or(0);
    let tokens_reasoning = token_summary.map(|t| t.total_reasoning_tokens).unwrap_or(0);
    let context_usage = profile.as_ref().and_then(|p| {
        p.context_length
            .map(|len| (len, p.context_length_threshold))
    });

    rsx! {
        div { class: "pointer-events-auto flex-1 min-w-[300px] min-h-0 h-full hud-glass rounded-xl border border-base-content/10 flex flex-col overflow-hidden",
            div { class: "p-3 border-b border-base-content/10 space-y-1.5",
                div { class: "flex items-center gap-2 min-w-0",
                    if let Some(a) = &selected_agent {
                        span { class: "min-w-0 truncate text-sm font-semibold", "{a.name}" }
                        span { class: "flex-shrink-0 {agent_runtime_badge(a.runtime_state)}",
                            {runtime
                                .as_ref()
                                .map(|_| runtime_state_label.to_string())
                                .unwrap_or_else(|| agent_runtime_label(a.runtime_state).to_string())}
                        }
                        for role in a.roles.iter().take(4) {
                            span { class: "flex-shrink-0 {tag_chip()}", "{role}" }
                        }
                    } else {
                        span { class: "text-sm font-semibold", "预览" }
                    }
                    button {
                        class: "btn hud-btn btn-ghost btn-xs flex-shrink-0 ml-auto",
                        title: "退出专注",
                        onclick: move |_| on_exit_focus.call(()),
                        "✕"
                    }
                }
                if let Some(desc) = selected_agent.as_ref().and_then(|a| a.description.as_deref()) {
                    p { class: "text-xs text-base-content/60 leading-relaxed line-clamp-2", "{desc}" }
                }
                div { class: "flex flex-wrap gap-1",
                    button {
                        class: "rounded border border-base-content/15 px-2 py-1 text-[10px] text-base-content/55 transition-colors hover:border-primary/40 hover:text-primary disabled:cursor-not-allowed disabled:opacity-35",
                        disabled: !task_loaded,
                        onclick: move |_| left_detail.set(Some(FocusLeftDetail::Task)),
                        "任务详情"
                    }
                    button {
                        class: "rounded border border-base-content/15 px-2 py-1 text-[10px] text-base-content/55 transition-colors hover:border-primary/40 hover:text-primary disabled:cursor-not-allowed disabled:opacity-35",
                        disabled: !project_loaded,
                        onclick: move |_| left_detail.set(Some(FocusLeftDetail::Project)),
                        "项目详情"
                    }
                    button {
                        class: "rounded border border-base-content/15 px-2 py-1 text-[10px] text-base-content/55 transition-colors hover:border-primary/40 hover:text-primary disabled:cursor-not-allowed disabled:opacity-35",
                        disabled: !trace_available,
                        onclick: move |_| left_detail.set(Some(FocusLeftDetail::LiveTrace)),
                        "完整轨迹"
                    }
                }
            }
            div { class: "flex-none max-h-[48%] min-h-0 overflow-y-auto p-3 space-y-3",
                div { class: "text-xs font-medium text-base-content/60", "任务上下文" }
                if let Some(t) = &task {
                    div { class: "hud-glass rounded-lg p-3 space-y-2",
                        div { class: "flex items-start gap-2",
                            span { class: "text-sm font-medium flex-1 min-w-0 break-words", "{t.title}" }
                            span { class: "flex-shrink-0 {task_status_badge(t.status)}",
                                "{task_status_text(t.status)}"
                            }
                        }
                        div { class: "flex items-center gap-2",
                            HudProgress {
                                value: t.progress,
                                tone: Some(progress_tone(t.progress).to_string()),
                                show_value: Some(true),
                            }
                        }
                    }
                } else if runtime.as_ref().is_none_or(|rt| rt.task_id.is_some()) {
                    div { class: "hud-glass rounded-lg p-3 text-sm text-base-content/50",
                        "任务详情加载中…"
                    }
                } else {
                    div { class: "hud-glass rounded-lg p-3",
                        div { class: "text-sm text-base-content/50", "当前没有关联任务" }
                    }
                }
            }
            div { class: "flex-1 min-h-0 overflow-y-auto p-3 space-y-3 border-t border-base-content/10",
                div { class: "flex items-start justify-between gap-2 min-w-0",
                    span { class: "flex-shrink-0 pt-0.5 text-xs font-medium text-base-content/60", "实时事件中心" }
                    div { class: "flex min-w-0 flex-wrap items-center justify-end gap-1 font-mono text-[10px] text-base-content/50",
                        if let (Some(input), Some(output)) = (tokens_input, tokens_output) {
                            span {
                                class: "rounded bg-base-200/60 px-1 py-0.5",
                                title: "近 60 分钟输入 Token",
                                "入 {format_compact_count(input)}"
                            }
                            span {
                                class: "rounded bg-base-200/60 px-1 py-0.5",
                                title: "近 60 分钟输出 Token",
                                "出 {format_compact_count(output)}"
                            }
                            if tokens_cached > 0 {
                                span {
                                    class: "rounded bg-base-200/60 px-1 py-0.5",
                                    title: "缓存命中 Token（输入的子集）",
                                    "缓 {format_compact_count(tokens_cached)}"
                                }
                            }
                            if tokens_reasoning > 0 {
                                span {
                                    class: "rounded bg-base-200/60 px-1 py-0.5",
                                    title: "思考 Token（输出的子集）",
                                    "思 {format_compact_count(tokens_reasoning)}"
                                }
                            }
                        }
                    }
                }
                div { class: "flex items-center gap-1.5 text-[10px] text-base-content/45 flex-wrap",
                    if let Some((len, threshold)) = context_usage {
                        div { class: "flex items-center gap-1",
                            if let Some(max) = threshold
                                && max > 0
                            {
                                RingProgress {
                                    value: len,
                                    max,
                                    title: Some(format!(
                                        "上下文 {} / 压缩阈值 {} tokens",
                                        format_compact_count(len),
                                        format_compact_count(max),
                                    )),
                                    size: Some(18),
                                }
                            }
                            span { "上下文 {format_compact_count(len)}" }
                            if let Some(max) = threshold
                                && max > 0
                            {
                                span { " / {format_compact_count(max)}" }
                            }
                        }
                    } else {
                        span { "上下文 —" }
                    }
                    if let Some(name) = focus_project_name.clone() {
                        span { class: "min-w-0 truncate", "· 项目 {name}" }
                    }
                    if let Some(think) = runtime.as_ref().and_then(|rt| rt.think_runtime.as_ref()) {
                        span {
                            "· {think_scene_label(&think.scene)} · 第 {think.round}/{think.max_rounds} 轮 · 工具 {think.tool_call_count} 次"
                        }
                    }
                }
                if let Some(agent_id) = selected_agent.as_ref().map(|a| a.id.clone()) {
                    AgentLiveCenter {
                        agent_id,
                        on_open_detail: Some(Callback::new(move |detail: LiveDetail| {
                            left_detail.set(Some(match detail {
                                LiveDetail::Text(t) => FocusLeftDetail::LiveText(t),
                                LiveDetail::Trace => FocusLeftDetail::LiveTrace,
                            }));
                        })),
                    }
                } else {
                    div { class: "hud-glass rounded-lg p-3 text-xs text-base-content/50",
                        "选择 Agent 后展示其实时事件流"
                    }
                }
            }
        }
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
    let focused = *chat_focused.read() || focus;
    let mut chat_focused = chat_focused;
    let mut auto_mention = auto_mention;
    let mut send_trigger = send_trigger;
    // 现场快照句柄只在此创建、不 read；真正的读写都在独立子组件里。
    let scene = SceneSnapshots {
        runtime: use_signal(|| Option::<RuntimeStatusResponse>::None),
        task: use_signal(|| Option::<GetTaskResponse>::None),
        profile: use_signal(|| Option::<GetAgentResponse>::None),
        project: use_signal(|| Option::<GetProjectResponse>::None),
    };

    // 交互态：话题根 / 左侧详情槽。两者只由用户点击驱动，
    // 不与现场轮询共用渲染作用域。
    let mut thread_root = use_signal(|| Option::<String>::None);
    let mut left_detail = use_signal(|| Option::<FocusLeftDetail>::None);
    use_effect(use_reactive!(|focus_agent_id| {
        let _ = focus_agent_id;
        thread_root.set(None);
        left_detail.set(None);
    }));

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
                    // 黄金比例：左 1.618 / 右 1（约 61.8% : 38.2%）
                    "pointer-events-auto relative flex-[1.618] min-w-0 h-full min-h-0 hud-glass rounded-xl border border-base-content/10 flex flex-col"
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
                // 消息区：主列表常驻；话题是基础视图层；详情是单层浮层。
                // 详情/话题都不卸载主列表，返回时滚动位置与长消息展开状态保留。
                div { class: if focus { "relative flex-1 min-h-0" } else { "relative" },
                    ChatMessageList {
                        focus,
                        focused,
                        chat_messages,
                        chat_is_typing,
                        show_receiver,
                        on_load_older,
                        thread_root,
                        left_detail,
                    }

                    FocusThreadOverlay { focus, chat_messages, thread_root }
                    if focus {
                        FocusDetailOverlay { scene, selected_agent: selected_agent.clone(), left_detail }
                    }
                }
                ChatInputArea {
                    input_text: chat_input,
                    pending_attachments: attachments.pending,
                    drag_hover: attachments.drag_hover,
                    uploading: attachments.uploading,
                    on_files: attachments.on_files,
                    on_send: Callback::new(move |_| send_trigger.set(true)),
                    messages: chat_messages,
                    mention,
                    tabs: mention_tab_list,
                    input_id: WORKSPACE_CHAT_INPUT_ID,
                    file_input_id: "workspace-chat-file-input",
                    compact: true,
                    agent_state: None,
                    reply_preview: None,
                    on_cancel_reply: None,
                    on_remove_mention: Some(EventHandler::new(move |_key: String| {
                        // 用户手动摘除后，不再视为「自动写入的内容」，
                        // 否则切任务时会被当成可覆盖的旧锚定
                        auto_mention.set(None);
                    })),
                    focus_selector: Some("#workspace-chat-dialog"),
                    on_focus: Some(EventHandler::new(move |_| chat_focused.set(true))),
                    on_blur: Some(EventHandler::new(move |_| chat_focused.set(false))),
                }
            }

            // ---- 聚焦模式右侧：独立订阅的现场面板（含实时事件中心） ----
            if focus {
                FocusScenePanel {
                    scene,
                    focus_agent_id: focus_agent_id.clone(),
                    selected_agent: selected_agent.clone(),
                    on_exit_focus,
                    left_detail,
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

    /// 三块独立订阅守卫：页面壳 `bottom_dock` 只创建现场句柄、不读现场信号；
    /// 消息链不再按现场「正在处理消息 id」定位。防止后续改动把轮询写回页面壳，
    /// 再次造成专注态消息区周期性重建、滚动位置归零。
    #[test]
    fn bottom_dock_does_not_read_scene_snapshots() {
        let src = include_str!("dock.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or("");
        let shell_start = src
            .find("pub fn bottom_dock(")
            .expect("bottom_dock must exist");
        let shell = &src[shell_start..];
        for field in ["runtime", "task", "profile", "project"] {
            let needle = format!("scene.{field}.read()");
            assert!(
                !shell.contains(&needle),
                "bottom_dock must not read scene.{field}; move it into FocusScenePanel/FocusDetailOverlay"
            );
        }
        assert!(
            !src.contains("current_message_id"),
            "message chain must not be anchored to live runtime's current message id"
        );
        // 输入区与对话页共用 components::chat::ChatInputArea，页面壳只传调度参数
        assert!(
            shell.contains("ChatInputArea {"),
            "bottom_dock must dispatch the shared ChatInputArea"
        );
        for inline in [
            "textarea {",
            "MentionPicker {",
            "clipboard_files_from_paste",
        ] {
            assert!(
                !shell.contains(inline),
                "bottom_dock must not inline input UI ({inline}); use the shared ChatInputArea"
            );
        }
    }

    /// 滚底守卫：跟随逻辑必须在 ChatMessageList 内部用 use_effect（DOM 提交后执行），
    /// 且必须保留 at_bottom 门槛。页面壳（mod.rs）不得再用固定 16ms 延时滚底——
    /// 抽成独立组件后这条路径拿不到提交完成的 DOM，专注态会停在顶部/中途。
    #[test]
    fn scroll_follow_lives_in_message_list_with_bottom_guard() {
        let src = include_str!("dock.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or("");
        let list_start = src
            .find("fn ChatMessageList(")
            .expect("ChatMessageList must exist");
        let list_end = src[list_start..]
            .find("/// 话题浮层")
            .expect("ChatMessageList end marker must exist")
            + list_start;
        let list = &src[list_start..list_end];
        assert!(
            list.contains("use_effect"),
            "ChatMessageList must own the scroll-follow effect"
        );
        assert!(
            list.contains("if !at_bottom()"),
            "scroll follow must respect at_bottom, else loading older history gets yanked down"
        );
        assert!(
            list.contains("scroll_to_latest()"),
            "scroll follow must actually scroll"
        );
        // 依赖末条 id 而非消息数：切会话满页同数量时也能滚底
        assert!(
            list.contains(".last().map(|m| m.message_id.clone())"),
            "scroll-follow effect must depend on tail message id"
        );
    }
}
