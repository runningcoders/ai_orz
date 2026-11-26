//! 底部横幅（MMORPG 式：左下信息卡 / 中间对话框 / 右下预留，左右通透）
//!
//! 纯渲染函数：对话状态与信号由页面壳传入。
//! 紧凑态只保留消息区；专注态左侧是主信息区（主消息链 / 话题 / 详情），
//! 右侧是总览 + 可拉伸的实时事件中心。

use dioxus::html::HasFileData;
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
    AgentTracePanel, QuotedPreview, ReplyPreview, RichMessageItem, ThreadChain, TypingIndicator,
    message_preview_text, truncate_preview,
};
use crate::components::hud::HudProgress;
use crate::components::markdown::MarkdownRenderer;
use crate::components::mention_picker::{
    MentionCandidate, MentionPickedBar, MentionPicker, MentionState, mention_kinds_for,
    mention_tabs,
};
use crate::components::ring_progress::RingProgress;
use crate::components::state::Loading;
use crate::hooks::use_attachments::{AttachmentComposer, clipboard_files_from_paste};
use crate::pages::workspace::live_center::{AgentLiveCenter, LiveDetail, LiveDetailText};
use crate::store::directory::use_directory;
use crate::utils::mention::{read_caret, restore_caret};
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
enum FocusLeftDetail {
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
    let focus_project = use_signal(|| Option::<GetProjectResponse>::None);
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
    // 引用预览：工作台消息区没有 chat.rs 的富引用块，这里补一份轻量预览
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

    // 点击引用块 → 在专注面板打开整条话题；关闭后回到当前消息链
    let mut thread_root = use_signal(|| Option::<String>::None);
    // 专注态左侧主区：任务 / 项目 / 实时详情，非空时盖住主消息链
    let mut left_detail = use_signal(|| Option::<FocusLeftDetail>::None);
    use_effect(use_reactive!(|focus_agent_id| {
        let _ = focus_agent_id;
        thread_root.set(None);
        left_detail.set(None);
    }));
    let thread_now = thread_root();
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
    // 紧凑态与专注态都能打开话题：紧凑态用浮层，专注态用左侧主区
    let open_thread: Option<EventHandler<String>> = Some(Callback::new(move |root: String| {
        left_detail.set(None);
        thread_root.set(Some(root));
    }));
    // 话题窗口：固定保留根消息，再取当前消息附近最多 7 条，避免只看尾部而丢根
    let focus_chain: Vec<MessageListItem> = if thread_mode {
        let n = thread_items.len();
        if n <= 8 {
            thread_items.clone()
        } else {
            let cur_idx = focus_msg
                .as_ref()
                .and_then(|f| {
                    thread_items
                        .iter()
                        .position(|m| m.message_id == f.message_id)
                })
                .unwrap_or(n - 1);
            let start = cur_idx.saturating_sub(3).max(1);
            let end = (start + 7).min(n);
            let mut window = Vec::with_capacity(8);
            window.push(thread_items[0].clone());
            window.extend_from_slice(&thread_items[start..end]);
            window
        }
    } else {
        if let Some(focus) = &focus_msg {
            let mut ids = vec![focus.message_id.clone()];
            let mut cursor = focus.reply_to_id.clone();
            while ids.len() < 4 {
                let Some(pid) = cursor else { break };
                let Some(parent) = msg_by_id.get(pid.as_str()) else {
                    break;
                };
                ids.push(parent.message_id.clone());
                cursor = parent.reply_to_id.clone();
            }
            ids.reverse();
            let chain: Vec<MessageListItem> = ids
                .iter()
                .filter_map(|id| msg_by_id.get(id.as_str()).map(|m| (*m).clone()))
                .collect();
            if chain.len() >= 2 {
                chain
            } else if let Some(idx) = msgs.iter().position(|m| m.message_id == focus.message_id) {
                msgs[idx.saturating_sub(3)..=idx].to_vec()
            } else {
                chain
            }
        } else {
            msgs.iter().rev().take(3).rev().cloned().collect()
        }
    };
    let chain_items: Vec<(MessageListItem, bool, String)> = focus_chain
        .iter()
        .map(|m| {
            let is_current = focus_msg
                .as_ref()
                .is_some_and(|f| f.message_id == m.message_id);
            (m.clone(), is_current, message_preview_text(m))
        })
        .collect();
    // 专注态话题页用完整线程（不裁窗口）：盖住主消息区后有足够空间滚动
    let thread_chain_items: Vec<(MessageListItem, bool, String)> = thread_items
        .iter()
        .map(|m| {
            let is_current = focus_msg
                .as_ref()
                .is_some_and(|f| f.message_id == m.message_id);
            (m.clone(), is_current, message_preview_text(m))
        })
        .collect();
    let panel_title = if thread_mode {
        format!("话题 · {} 条", thread_items.len())
    } else {
        "消息链".to_string()
    };
    let runtime_state_label = match runtime.as_ref().map(|r| r.state.as_str()) {
        Some("busy") => "忙碌",
        Some("resting") => "休息",
        Some("idle") => "空闲",
        _ => "离线",
    };

    // 项目详情：内部轮询按 project_id 拉过详情后回填，组件不再依赖外部快照
    let project_detail = focus_project.read().clone();
    let project_loaded = project_detail.is_some();
    let focus_project_name = project_detail.as_ref().map(|p| p.name.clone());

    // 左侧详情页：标题与副标题在渲染前算好，正文按枚举分支铺开
    let left_detail_now = left_detail();
    let (left_detail_title, left_detail_subtitle) = match &left_detail_now {
        Some(FocusLeftDetail::Task) => (
            "任务详情".to_string(),
            task.as_ref().map(|t| t.title.clone()).unwrap_or_default(),
        ),
        Some(FocusLeftDetail::Project) => (
            "项目详情".to_string(),
            focus_project_name.clone().unwrap_or_default(),
        ),
        Some(FocusLeftDetail::LiveText(t)) => (t.title.clone(), t.subtitle.clone()),
        Some(FocusLeftDetail::LiveTrace) => (
            "完整轨迹".to_string(),
            selected_agent
                .as_ref()
                .map(|a| a.name.clone())
                .unwrap_or_default(),
        ),
        None => (String::new(), String::new()),
    };
    let task_loaded = task.is_some();
    let trace_available = selected_agent.is_some();

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
                                    // 与对话页共用同一套富消息渲染：这里只读，
                                    // 所以不传回复能力（on_reply=None 隐藏回复按钮）
                                    RichMessageItem {
                                        key: "{msg.message_id}",
                                        msg: msg.clone(),
                                        show_receiver,
                                        // 只有全屏专注的消息区够高、且无裁切祖先，才开资料卡浮层
                                        avatar_popover: focus,
                                        // 紧凑消息区更矮：长消息的折叠上限跟着可视高度走
                                        collapse_height: if focus { 300 } else if focused { 180 } else { 96 },
                                        quoted: reply_map.get(&msg.message_id).map(|r| QuotedPreview {
                                            sender: r.sender.clone(),
                                            content: r.text.clone(),
                                            root_id: r.root_id.clone(),
                                        }),
                                        on_open_thread: open_thread,
                                    }
                                }
                                if is_typing {
                                    TypingIndicator {}
                                }
                            }
                        }
                    }

                    // 话题基础视图层：详情可以盖在它上面，但不会销毁它
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
                                span { class: "ml-auto text-[10px] text-base-content/40",
                                    "与当前消息相关"
                                }
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

                    // 详情浮层：单槽，新的详情直接替换旧的；关闭后回到话题或主链
                    if focus && left_detail_now.is_some() {
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
                                    div { class: "text-sm font-medium truncate", "{left_detail_title}" }
                                    if !left_detail_subtitle.is_empty() {
                                        div { class: "text-[10px] text-base-content/40 truncate",
                                            "{left_detail_subtitle}"
                                        }
                                    }
                                }
                            }
                            div { class: "flex-1 min-h-0 overflow-y-auto p-4",
                                if let Some(detail) = &left_detail_now {
                                    if let FocusLeftDetail::Task = detail {
                                        if let Some(t) = &task {
                                            {task_detail_body(t, focus_project_name.as_deref())}
                                        } else {
                                            div { class: "text-sm text-base-content/45",
                                                "任务详情尚未加载完成，请稍后重试。"
                                            }
                                        }
                                    }
                                    if let FocusLeftDetail::Project = detail {
                                        if let Some(p) = &project_detail {
                                            {project_detail_body(p)}
                                        } else {
                                            div { class: "text-sm text-base-content/45",
                                                "项目详情尚未加载完成，请稍后重试。"
                                            }
                                        }
                                    }
                                    if let FocusLeftDetail::LiveText(t) = detail {
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
                }
                // 紧凑态的话题浮层：点击引用块后悬在对话框上方，看完可直接关闭
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
                            ThreadChain {
                                items: chain_items.clone(),
                                reply_map: reply_map.clone(),
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
                div { class: "pointer-events-auto flex-1 min-w-[300px] min-h-0 h-full hud-glass rounded-xl border border-base-content/10 flex flex-col overflow-hidden",
                    // 标题区：身份 + 状态 + 角色标签（第 1 行），介绍与快捷入口
                    // 作为第 2 / 3 行。用量与运行现场下移到「实时事件中心」标题行，
                    // 它们本质上都是事件的统计信息。
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
                                // 角色标签紧跟状态徽章，与标题拼成一行
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
                        // 第 2 行：角色介绍
                        if let Some(desc) = selected_agent.as_ref().and_then(|a| a.description.as_deref()) {
                            p { class: "text-xs text-base-content/60 leading-relaxed line-clamp-2", "{desc}" }
                        }
                        // 第 3 行：快捷入口（长内容统一铺到左侧主区）
                        div { class: "flex flex-wrap gap-1",
                            button {
                                class: "rounded border border-base-content/15 px-2 py-1 text-[10px] text-base-content/55 transition-colors hover:border-primary/40 hover:text-primary disabled:cursor-not-allowed disabled:opacity-35",
                                disabled: !task_loaded,
                                onclick: move |_| {
                                    left_detail.set(Some(FocusLeftDetail::Task));
                                },
                                "任务详情"
                            }
                            button {
                                class: "rounded border border-base-content/15 px-2 py-1 text-[10px] text-base-content/55 transition-colors hover:border-primary/40 hover:text-primary disabled:cursor-not-allowed disabled:opacity-35",
                                disabled: !project_loaded,
                                onclick: move |_| {
                                    left_detail.set(Some(FocusLeftDetail::Project));
                                },
                                "项目详情"
                            }
                            button {
                                class: "rounded border border-base-content/15 px-2 py-1 text-[10px] text-base-content/55 transition-colors hover:border-primary/40 hover:text-primary disabled:cursor-not-allowed disabled:opacity-35",
                                disabled: !trace_available,
                                onclick: move |_| {
                                    left_detail.set(Some(FocusLeftDetail::LiveTrace));
                                },
                                "完整轨迹"
                            }
                        }
                    }
                    // 固定总览：只保留任务上下文。角色 / 介绍 / 快捷入口已并入
                    // 标题区，用量与运行现场归入实时事件中心标题行。
                    // 内容超过面板 48% 时自身滚动，把更长的一段留给事件中心。
                    div { class: "flex-none max-h-[48%] min-h-0 overflow-y-auto p-3 space-y-3",
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
                                div { class: "flex items-center gap-2",
                                    HudProgress {
                                        value: t.progress,
                                        tone: Some(progress_tone(t.progress).to_string()),
                                        show_value: Some(true),
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

                    }
                    // 右侧可拉伸区：实时事件中心（工具调用 / 思考过程）。
                    // 与左侧同高对齐并独立滚动；详情统一铺到左侧主区。
                    div { class: "flex-1 min-h-0 overflow-y-auto p-3 space-y-3 border-t border-base-content/10",
                        // ---- 实时事件中心（大脑图 + 动作层 + 事件流） ----
                        // 用量与运行现场都是事件的统计信息，放在本节标题行。
                        div { class: "flex items-start justify-between gap-2 min-w-0",
                            span { class: "flex-shrink-0 pt-0.5 text-xs font-medium text-base-content/60", "实时事件中心" }
                            // Token：近 60 分钟窗口累计；输入 / 输出固定，
                            // 缓存命中与思考是各自子集，> 0 才入列。
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
                                on_open_detail: Some(Callback::new(
                                    move |detail: LiveDetail| {
                                        // 详情是覆盖层：保留当前话题，返回时回到原视图
                                        left_detail.set(Some(match detail {
                                            LiveDetail::Text(t) => FocusLeftDetail::LiveText(t),
                                            LiveDetail::Trace => FocusLeftDetail::LiveTrace,
                                        }));
                                    },
                                )),
                            }
                        } else {
                            div { class: "hud-glass rounded-lg p-3 text-xs text-base-content/50",
                                "选择 Agent 后展示其实时事件流"
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
