use dioxus::prelude::*;
use dioxus_router::use_navigator;
use wasm_bindgen::JsCast;

use crate::Route;
use crate::api::hr::{get_agent, get_reception_agent, list_agents};
use crate::api::message::{
    load_latest_messages, load_older_messages, poll_new_messages, send_message_to_agent,
};
use crate::api::project::{create_project, get_project, list_projects, query_projects};
use crate::components::artifact_preview_modal::{ArtifactPreviewModal, ArtifactPreviewTarget};
use crate::components::avatar_bubble::{
    AvatarBubble, AvatarTone, BubbleAlign, collapse_avatar_bubble,
};
use crate::components::chat::{
    CancelThinkingButton, ChatInputArea, ChatSidePanel, QuotedPreview, ReplyPreview,
    RichMessageItem, ThreadChain, message_preview_text, truncate_preview,
};
use crate::components::mention_picker::{MentionState, mention_kinds_for, mention_tabs};
use crate::components::modal::Modal;
use crate::components::state::Loading;
use crate::hooks::use_attachments::{compose_outbound_content, use_attachment_composer};
use crate::hooks::{EventSourceOptions, use_event_source};
use crate::layouts::navbar::Navbar;
use crate::store::auth::use_auth_state;
use crate::store::directory::use_directory;
use crate::store::toast::use_toast;
use crate::utils::local_store;
use crate::utils::time::format_date_group_label;
use crate::utils::{
    HISTORY_PAGE_SIZE, HISTORY_SCAN_MAX_PAGES, MESSAGE_POLL_LIMIT, MESSAGE_POLL_SECS, MSG_TEXT,
    append_new_messages, build_optimistic_user_msg, in_project_context, is_attachment_message,
    page_hidden, project_status_text as status_text, replace_tmp_with_real, request_scope,
};
use common::api::{
    AgentListItem, CreateProjectRequest, GetAgentRequest, GetAgentResponse, GetProjectRequest,
    GetReceptionAgentResponse, ListAgentsRequest, ListProjectsRequest, ListProjectsResponseItem,
    MessageListItem, ProjectQueryRequest, SendMessageToAgentParams,
};

/// 从消息正文的产物 chip 点击里取 artifact id。
///
/// chip 由 `utils::mention::render_resource_chip` 注入为带
/// `data-mention-kind="artifact" data-mention-id="..."` 的 `<span>`；Dioxus 不为
/// 注入 HTML 建 VNode，故在最外层消息列表容器做事件委托（与文档中心
/// `pages/system/docs.rs` 对 `a.doc-link-internal` 的处理同一范式）。
fn artifact_id_from_click(evt: &MouseEvent) -> Option<String> {
    let data = evt.data();
    let web_evt = data.downcast::<web_sys::MouseEvent>()?;
    let target = web_evt.target()?.dyn_into::<web_sys::Element>().ok()?;
    let Ok(Some(chip)) = target.closest("[data-mention-kind='artifact']") else {
        return None;
    };
    chip.get_attribute("data-mention-id")
}

/// 消息列表容器上的产物 chip 点击处理：命中则按 id 取详情并就地弹窗，
/// 未命中时直接返回（不拦截事件，消息项的展开/回复等交互照常工作）。
fn open_artifact_preview_from_click(
    evt: MouseEvent,
    mut preview: Signal<Option<ArtifactPreviewTarget>>,
    preview_gen: Signal<u64>,
    err_toast: crate::store::toast::ToastState,
) {
    let Some(id) = artifact_id_from_click(&evt) else {
        return;
    };
    evt.stop_propagation();
    // 捕获点击时代次：异步取详情期间若切了会话/话题（代次已递增），回包丢弃，
    // 避免把上一个上下文的产物预览弹到新会话里。
    let epoch = preview_gen();
    spawn(async move {
        match crate::api::project::get_artifact(&id).await {
            Ok(artifact) => {
                if preview_gen() != epoch {
                    return;
                }
                preview.set(Some(ArtifactPreviewTarget {
                    artifact,
                    // 正文引用只有 id：归属信息无从得知，弹窗不展示对应行
                    project_name: String::new(),
                    task_title: None,
                }))
            }
            Err(e) => err_toast.error(format!("打开产物预览失败: {e}")),
        }
    });
}

/// 问题一：左栏项目列表状态子过滤器标签组（单选互斥 + 显式「全部」，AMan 拍板①）
/// 标签→状态映射：未完成=InProgress(1)、已完成=Completed(2)、已归档=Archived(3)；
/// Deleted=0 由后端 list_projects 排除不可达，前端无需处理（方案 §4.2）。
const FILTER_TABS: [(&str, Option<i32>); 4] = [
    ("全部", None),
    ("未完成", Some(1)), // InProgress
    ("已完成", Some(2)), // Completed
    ("已归档", Some(3)), // Archived
];

/// 对话页「现场」快照句柄：页面壳只创建、不 `.read()`。
///
/// 与工作台 `dock::SceneSnapshots` 同一范式 —— 谁读谁重渲染，
/// 页面壳持有句柄不会因此订阅 3s 轮询。
#[derive(Clone, Copy, PartialEq)]
struct ChatSceneSignals {
    agent_state: Signal<i32>,
    agent_state_owner: Signal<Option<String>>,
    target_agent_id: Signal<Option<String>>,
    target_agent_info: Signal<Option<GetAgentResponse>>,
}

/// 置底 Agent 状态气泡：独占 3s 轮询的现场 signal。
///
/// ⚠️ 这些 signal 一旦在消息列表组件的渲染作用域里读取，轮询每 3s
/// 就会让整块消息区重渲染；独立组件把重渲染半径收敛到气泡本身。
#[component]
fn ChatAgentStatusBubble(
    scene: ChatSceneSignals,
    is_typing: Signal<bool>,
    sender_name: String,
    on_cancelled: EventHandler<()>,
) -> Element {
    let state = *scene.agent_state.read();
    let owner = scene.agent_state_owner.read().clone();
    let target = scene.target_agent_id.read().clone();
    let typing = *is_typing.read();
    let info = scene.target_agent_info.read().clone();
    let Some(status) = agent_status_line(
        typing,
        state,
        agent_state_owner_matches(owner.as_deref(), target.as_deref()),
    ) else {
        return VNode::empty();
    };
    rsx! {
        div { class: "chat chat-start",
            div { class: "chat-header pr-1 text-sm opacity-70", "{sender_name}" }
            div { class: "chat-image",
                AvatarBubble {
                    name: sender_name.clone(),
                    tone: AvatarTone::Agent,
                    agent_id: info.as_ref().map(|a| a.id.clone()),
                    status: info.as_ref().map(|a| a.status),
                    align: BubbleAlign::Start,
                }
            }
            div { class: "chat-bubble chat-bubble-neutral",
                div { class: "flex items-center gap-2",
                    if state == 1 {
                        span { "🌙" }
                    } else {
                        div { class: "typing-indicator flex gap-1",
                            div { class: "typing-dot" }
                            div { class: "typing-dot" }
                            div { class: "typing-dot" }
                        }
                    }
                    span { class: "text-sm", "{status}" }
                    // Busy(2) 才给就地出口；Resting(1) 取消思考无意义。
                    if state == 2 {
                        if let Some(aid) = target {
                            CancelThinkingButton { agent_id: aid, on_cancelled }
                        }
                    }
                }
            }
        }
    }
}

/// 单条消息：引用解析 / 状态环这类现场读取下沉到这里，
/// 3s 轮询只重渲染当前消息项，不波及滚动容器。
#[component]
fn ChatMessageRow(
    msg: MessageListItem,
    messages: Signal<Vec<MessageListItem>>,
    reply_target: Signal<Option<MessageListItem>>,
    scene: ChatSceneSignals,
    on_open_thread: EventHandler<String>,
) -> Element {
    let directory = use_directory();
    let dir = directory.read();
    let quoted = msg.reply_to_id.as_ref().and_then(|rid| {
        messages
            .read()
            .iter()
            .find(|m| &m.message_id == rid)
            .map(|m| QuotedPreview {
                sender: dir.sender_name(m),
                content: truncate_preview(&message_preview_text(m)),
                root_id: m.root_id.clone().or_else(|| Some(m.message_id.clone())),
            })
    });
    let reply_msg = msg.clone();
    let on_reply = Callback::new(move |_: ()| reply_target.set(Some(reply_msg.clone())));
    let agent_status = scene
        .target_agent_info
        .read()
        .as_ref()
        .filter(|a| a.id == msg.from_id)
        .map(|a| a.status);
    rsx! {
        RichMessageItem {
            key: "{msg.message_id}",
            msg: msg.clone(),
            quoted,
            on_reply: Some(on_reply),
            on_open_thread: Some(on_open_thread),
            agent_status,
        }
    }
}

/// 对话页消息列表：两个对话分支（项目 / 默认）共用的唯一实现。
///
/// ⚠️ 本组件只订阅消息、加载态与滚动态；3s Agent 轮询、SSE 计数、
/// 30s 统计节拍全部留在页面壳 / 各自子组件里，不进入本组件的渲染作用域。
/// 与工作台 `dock::ChatMessageList` 同一隔离范式。
#[component]
#[allow(clippy::too_many_arguments)]
fn ChatMessageList(
    messages: Signal<Vec<MessageListItem>>,
    loading_messages: Signal<bool>,
    is_typing: Signal<bool>,
    at_bottom: Signal<bool>,
    has_new_messages: Signal<bool>,
    reply_target: Signal<Option<MessageListItem>>,
    scene: ChatSceneSignals,
    on_load_older: EventHandler<()>,
    on_open_thread: EventHandler<String>,
    on_artifact_click: EventHandler<MouseEvent>,
    on_thinking_cancelled: EventHandler<()>,
    status_sender_name: String,
    empty_emoji: String,
    empty_title: String,
    empty_text: String,
) -> Element {
    // ⚠️ 滚动恢复只跟随「消息数 / typing / 用户回到顶部意图」。
    // 绝不把现场轮询状态放进依赖：Signal::set 不去重，
    // 每拍都会让这里无条件滚底，打断上翻阅读。
    use_effect(move || {
        let msg_count = messages().len();
        let typing = is_typing();
        if !at_bottom() {
            return;
        }
        let _ = (msg_count, typing);
        spawn(async move {
            if let Some(window) = web_sys::window()
                && let Some(doc) = window.document()
                && let Some(el) = doc.query_selector("#chat-scroll-container").ok().flatten()
                && let Ok(html_el) = el.dyn_into::<web_sys::HtmlElement>()
            {
                html_el.set_scroll_top(html_el.scroll_height());
            }
        });
    });

    let mut at_bottom = at_bottom;
    let mut has_new_messages = has_new_messages;
    let loading = *loading_messages.read();
    let msgs = messages.read();

    rsx! {
        div { class: "flex-1 overflow-y-auto p-4 bg-base-100", id: "chat-scroll-container",
            // scroll 不冒泡，监听必须挂在真正滚动的容器本体上
            onscroll: move |_e| {
                collapse_avatar_bubble();
                if let Some(html_el) = web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.query_selector("#chat-scroll-container").ok().flatten())
                    .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
                {
                    let scroll_top = html_el.scroll_top();
                    let max_scroll = html_el.scroll_height() - html_el.client_height();
                    let is_bottom = scroll_top as f64 >= (max_scroll - 50) as f64;
                    // 仅在状态翻转时写回：Signal::set 不做相等去重
                    if at_bottom() != is_bottom {
                        at_bottom.set(is_bottom);
                    }
                    if is_bottom && has_new_messages() {
                        has_new_messages.set(false);
                    }
                    if scroll_top == 0 {
                        on_load_older.call(());
                    }
                }
            },
            if loading && msgs.is_empty() {
                div { class: "flex items-center justify-center py-12",
                    Loading { size: "md" }
                    span { class: "ml-2 text-base-content/60", "加载消息中..." }
                }
            } else if msgs.is_empty() {
                div { class: "text-center py-12",
                    div { class: "text-5xl mb-3", "{empty_emoji}" }
                    if !empty_title.is_empty() {
                        div { class: "text-base-content/70 font-medium mb-2", "{empty_title}" }
                    }
                    div { class: "text-base-content/60", "{empty_text}" }
                }
            } else {
                div {
                    class: "flex flex-col gap-1 min-h-full",
                    // 正文产物 chip 是注入 HTML（无 VNode），在这里做事件委托
                    onclick: on_artifact_click,
                    for entry in group_messages_by_date(&msgs) {
                        match entry {
                            MessageListEntry::DateDivider(label) => rsx! {
                                // key 不含 messages().len()，切会话时保持稳定
                                div { class: "hud-divider my-2", key: "divider-{label}", "{label}" }
                            },
                            MessageListEntry::Message(msg) => rsx! {
                                ChatMessageRow {
                                    key: "{msg.message_id}",
                                    msg,
                                    messages,
                                    reply_target,
                                    scene,
                                    on_open_thread,
                                }
                            },
                        }
                    }
                    ChatAgentStatusBubble {
                        scene,
                        is_typing,
                        sender_name: status_sender_name.clone(),
                        on_cancelled: on_thinking_cancelled,
                    }
                    // 新消息提示：用户上翻阅读时悬浮底部，点击回最新消息
                    if has_new_messages() && !at_bottom() {
                        div { class: "sticky bottom-2 z-10 flex justify-center pointer-events-none",
                            button {
                                class: "btn hud-btn btn-sm btn-primary rounded-full shadow-lg pointer-events-auto",
                                onclick: move |_| {
                                    has_new_messages.set(false);
                                    at_bottom.set(true);
                                },
                                "↓ 有新消息"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn MessageChat(project: Option<String>) -> Element {
    // 修复 HIGH #3：之前 use_require_auth 提前 return 会跳过后续所有 use_signal，
    // Dioxus 的 hooks 槽位索引会与上一次 render 不一致，触发 panic。
    // 现在：先注册所有 hooks，再在渲染时根据 auth 状态条件返回。
    let mut projects = use_signal(Vec::<ListProjectsResponseItem>::new);
    let mut selected_project = use_signal(|| Option::<String>::None);
    let navigator = use_navigator();
    let mut messages = use_signal(Vec::<MessageListItem>::new);
    // 增量兜底轮询的游标（`created_at` 毫秒）。切换会话时置空，回落到列表最大值。
    let mut poll_cursor = use_signal(|| Option::<i64>::None);
    let mut is_typing = use_signal(|| false);
    // 当前会话目标 Agent 的运行时状态（0=Idle 1=Resting 2=Busy），轮询刷新。
    // 驱动置底状态气泡文案 + 非空闲时的发送门禁。
    let mut agent_state = use_signal(|| 0i32);
    // 思考状态跨会话污染修复（任务 01a11b5b，排查 01a11b41，方案 B）：agent_state/is_typing
    // 的归属快照 —— 记录当前状态值属于哪个会话目标 Agent。状态消费点必须先校验
    // agent_state_owner() == target_agent_id() 再消费，杜绝切换会话后旧 Agent 状态串台。
    let mut agent_state_owner = use_signal(|| Option::<String>::None);
    // 轮询得到的完整 Agent 详情（get_agent 响应），与右侧 ChatSidePanel 的
    // AgentInfoTab 共享同一份数据，避免重复请求、并让徽章实时刷新。
    let mut target_agent_info = use_signal(|| Option::<GetAgentResponse>::None);
    let mut input_text = use_signal(String::new);
    // 修复 L1：删除未使用的 error signal（之前从未 set，is_empty() 永远为 true）
    let mut loading_projects = use_signal(|| true);
    // 问题一：左栏状态子过滤器当前选中下标（AMan 拍板②：默认「未完成」，见 FILTER_TABS 下标 1）
    let status_filter = use_signal(|| 1usize);
    // 问题一（第三批·候选一）：过滤器点击改走后端 query_projects 拉最新数据。
    // filter_loading=局部加载态（仅标签组细条提示，不复用全局 loading_projects 避免整块闪烁）；
    // filter_request_id=自增竞态守卫（快速连点丢弃过期响应，与项目页 search_request_id 同款）。
    let mut filter_loading = use_signal(|| false);
    let mut filter_request_id = use_signal(|| 0u32);
    let mut has_more = use_signal(|| true);
    let mut loading_messages = use_signal(|| false);
    let mut sse_connected = use_signal(|| false);

    // 前台 Agent 信息（默认对话框顶部展示）
    let mut reception_agent = use_signal(|| Option::<GetReceptionAgentResponse>::None);

    // 新建项目弹窗状态
    let mut show_create_project = use_signal(|| false);
    // 正文产物 chip 点击后的就地预览目标（事件委托挂在消息列表容器上）
    let mut artifact_preview = use_signal(|| Option::<ArtifactPreviewTarget>::None);
    // 预览请求代次：点击后异步取详情期间若切会话/话题，回包据此丢弃，避免旧预览弹到新上下文
    let mut artifact_preview_gen = use_signal(|| 0u64);
    let mut new_project_name = use_signal(String::new);
    let mut new_project_desc = use_signal(String::new);
    let mut creating_project = use_signal(|| false);
    let mut project_owner_agent_id = use_signal(|| Option::<String>::None);
    let mut available_agents = use_signal(Vec::<AgentListItem>::new);

    // 修复 M3：追踪用户是否滚动到底部附近，用于决定新消息是否自动滚动
    let mut at_bottom = use_signal(|| true);
    // 用户上翻阅读期间有新消息到达的标记：仅展示悬浮提示不拽回底部，点击后回底
    let mut has_new_messages = use_signal(|| false);

    // 附件上传状态（与工作台 dock 共用同一套状态机）
    let toast = use_toast();
    let attachments = use_attachment_composer(toast);
    let mut pending_attachments = attachments.pending;
    let uploading = attachments.uploading;
    let drag_hover = attachments.drag_hover;
    // 全局名称目录：用于将消息 from_id 解析为展示名（用户 / Agent 名字）
    let directory = use_directory();

    // ===== 消息链（回复 / 话题）状态 =====
    // 正在回复的消息：驱动输入框引用条 + 发送时携带 reply_to_id
    let mut reply_target = use_signal(|| Option::<MessageListItem>::None);
    // 话题浏览页：Some(root_id) 时主内容区切换到整条消息链
    let mut thread_root = use_signal(|| Option::<String>::None);
    let mut thread_messages = use_signal(Vec::<MessageListItem>::new);
    let mut thread_loading = use_signal(|| false);

    // 话题浏览页数据加载：thread_root 变化时按 root_id 拉取整条消息链
    use_effect(move || {
        let Some(root) = thread_root() else {
            return;
        };
        thread_loading.set(true);
        spawn(async move {
            match load_latest_messages(common::api::ListMessagesRequest {
                root_id: Some(root.clone()),
                limit: Some(100),
                ..Default::default()
            })
            .await
            {
                Ok(resp) => thread_messages.set(resp.messages),
                Err(e) => toast.error(format!("加载话题消息失败: {}", e)),
            }
            thread_loading.set(false);
        });
    });

    // 移动端 sidebar 抽屉状态
    let mut sidebar_open = use_signal(|| false);
    let is_mobile = crate::hooks::use_breakpoint();

    // 信息侧栏状态（经通用组件层持久化，刷新页面后恢复）
    let mut panel_open = use_signal(|| {
        local_store::get_string(local_store::keys::CHAT_PANEL_OPEN)
            .ok()
            .flatten()
            .map(|v| v == "1")
            .unwrap_or(false)
    });
    // SSE 消息计数器：当前会话收到新消息时递增，驱动侧栏防抖刷新
    let mut refresh_tick = use_signal(|| 0u64);
    // 问题二：会话未读角标（仅 Text 消息计入；默认对话计入；持久化走组件层 ai_orz:unread_badges）
    let mut unread_badges = use_signal(local_store::load_unread_badges);
    // 统计周期刷新计数器：由下方 3s 轮询循环每 10 拍（30s，对齐后端统计落盘节奏）递增一次，
    // 单独成信号以便只命中 Agent 统计 Tab，不牵动项目总览/工具 Tab 的事件驱动语义。
    let mut stats_poll_tick = use_signal(|| 0u64);

    // 当前会话目标 Agent 的**唯一**解析入口：项目会话 = 项目 owner；默认对话 = 前台 Agent。
    //
    // ⚠️ 单一来源。此前 3s 轮询是从 `projects` 列表快照里找 owner、而右侧信息面板的
    // Agent 页从 `get_project` 响应里取 owner —— 两处口径不同：项目不在列表里（列表未
    // 刷新 / 分页未覆盖）时轮询目标会退化成 None，置底状态气泡归零、侧栏退回自身懒加载
    // 而不再实时刷新。现在统一以 `get_project` 为事实源，`projects` 列表只负责左侧导航。
    let mut target_agent_id = use_signal(|| Option::<String>::None);
    use_effect(move || {
        // 思考状态跨会话污染修复（任务 01a11b5b，排查 01a11b41，方案 A）：会话切换
        // （selected_project 变化）时立即重置思考状态为干净初始值，杜绝旧会话状态串台。
        // 本 effect 是 target 解析的唯一入口，覆盖项目→默认、默认→项目、项目→项目全部
        // 切换路径。残留竞态（轮询拍已带旧值断言）由方案 B 归属过滤根治。
        agent_state.set(0);
        is_typing.set(false);
        agent_state_owner.set(None);
        target_agent_info.set(None);
        let Some(pid) = selected_project() else {
            // 默认对话：前台 Agent（后端在无前台 Agent 时会回退 resolve_agent 兜底）
            target_agent_id.set(reception_agent().map(|a| a.agent_id));
            return;
        };
        spawn(async move {
            let owner = get_project(GetProjectRequest {
                id: pid,
                ..Default::default()
            })
            .await
            .ok()
            .and_then(|p| p.owner_agent_id);
            // 无需比较相等：本 effect 只读 selected_project / reception_agent，
            // 不订阅 target_agent_id，写回不会自触发重跑构成循环
            target_agent_id.set(owner);
        });
    });

    // 权限检查：先注册所有 hooks，再根据 auth 状态决定是否渲染
    // （Dioxus 要求 hooks 必须在每次 render 中按相同顺序注册，不能在条件分支中跳过）
    let is_authenticated = crate::hooks::use_require_auth();
    // 当前用户身份：判定「这条消息是不是跟我有关」（旁观消息视觉弱化）
    let auth = use_auth_state();

    let mut load_projects = move || {
        loading_projects.set(true);
        spawn(async move {
            match list_projects(ListProjectsRequest::default()).await {
                Ok(resp) => {
                    projects.set(resp.items);
                    // 不自动选择第一个项目，让用户从「默认对话」开始
                }
                Err(e) => {
                    toast.error(format!("加载项目列表失败: {}", e));
                }
            }
            loading_projects.set(false);
        });
    };

    // 问题一（第三批·候选一）：点击过滤器时按状态调 query_projects 拉取最新项目列表。
    // 竞态守卫：request_id 自增，响应回来时 id 不匹配（已有点击）则丢弃，旧不覆新；
    // 失败兜底：保留旧数据（projects 不动）+ toast 报错；全部=None 不过滤状态（语义与
    // 排查报告拍板④一致）；root_user_id 显式传当前用户 —— 后端现在会把它收敛到 ctx.uid()
    // （缺省即自己、传别人的值直接拒绝），显式传入只是与 list_projects 同口径的冗余声明。
    let reload_projects_filtered = move |status_in: Option<i32>| {
        let my_id = filter_request_id() + 1;
        filter_request_id.set(my_id);
        filter_loading.set(true);
        let my_user_id = auth.read().user_id.clone();
        spawn(async move {
            let result = query_projects(&ProjectQueryRequest {
                status_in: status_in.map(|s| vec![common::enums::ProjectStatus::from(s)]),
                root_user_id: Some(my_user_id),
                ..Default::default()
            })
            .await;
            // 竞态守卫：期间有新点击（id 已变）则丢弃本次过期响应
            if filter_request_id() != my_id {
                return;
            }
            match result {
                Ok(resp) => projects.set(resp.items),
                Err(e) => toast.error(format!("刷新项目列表失败: {}", e)),
            }
            filter_loading.set(false);
        });
    };

    // 加载前台 Agent 信息（用于默认对话框顶部展示）
    let load_reception_agent = move || {
        spawn(async move {
            match get_reception_agent().await {
                Ok(resp) => {
                    reception_agent.set(Some(resp));
                }
                Err(_) => {
                    // 无可用前台 Agent 时静默处理（默认对话框仍可用，发送时后端兜底）
                    reception_agent.set(None);
                }
            }
        });
    };

    let mut load_messages = move |project_id: Option<&str>| {
        // 转换为 owned：spawn 的 future 需要 'static 生命周期
        let project_id = project_id.map(|s| s.to_string());
        // ⚠️ 请求侧带哨兵值（见 `utils::message::request_scope`）：后端 `project_id=None`
        // 表示「不过滤」，会把项目消息一起拉回来，只能靠前端收敛 —— 那会让 `total` /
        // `has_more` 失真，一页被滤空还会让上拉卡在原地翻不到更早的历史。
        // 传哨兵后后端精确返回 `project_id IS NULL`，前端过滤降级为兜底。
        // 内部 `selected_project` 的 None 语义不变（`is_project_mode` 等依赖 is_some()）。
        let req_scope = request_scope(project_id.as_deref());
        // 切换会话重置滚动状态：进入对话视为停靠底部，加载完成后自动滚到最新消息
        at_bottom.set(true);
        has_new_messages.set(false);
        // 换会话 ⇒ 轮询游标作废（新旧会话的时间轴不可比）
        poll_cursor.set(None);
        loading_messages.set(true);
        spawn(async move {
            let mut visible = Vec::new();
            let mut cursor: Option<i64> = None;
            let mut full_page = false;
            for _ in 0..HISTORY_SCAN_MAX_PAGES {
                let req = common::api::ListMessagesRequest {
                    project_id: req_scope.clone(),
                    before_timestamp: cursor,
                    limit: Some(HISTORY_PAGE_SIZE),
                    ..Default::default()
                };
                let resp = match if cursor.is_some() {
                    load_older_messages(req).await
                } else {
                    load_latest_messages(req).await
                } {
                    Ok(resp) => resp,
                    Err(e) => {
                        toast.error(format!("加载消息失败: {}", e));
                        break;
                    }
                };
                // 满页 ⇒ 可能还有更早的；不满页 ⇒ 到头了
                full_page = resp.messages.len() >= HISTORY_PAGE_SIZE;
                let next_cursor = resp.messages.first().map(|m| m.created_at);
                visible = resp
                    .messages
                    .into_iter()
                    .filter(|m| in_project_context(m, project_id.as_deref()))
                    .collect();
                if !visible.is_empty() || !full_page {
                    break;
                }
                match next_cursor {
                    Some(ts) => cursor = Some(ts),
                    None => {
                        full_page = false;
                        break;
                    }
                }
            }
            messages.set(visible);
            // 问题二（AMan 拍板）：进入会话加载完成后清除该会话未读角标
            //（默认对话用哨兵键 default，项目会话用 project_id 原值）
            {
                let cur_key = project_id
                    .as_deref()
                    .map(str::to_string)
                    .unwrap_or_else(|| local_store::UNREAD_DEFAULT_KEY.to_string());
                let mut badges = unread_badges.write();
                badges.remove(&cur_key);
                let _ = local_store::set_json(local_store::keys::UNREAD_BADGES, &*badges);
            }
            has_more.set(full_page);
            loading_messages.set(false);
        });
    };

    let mut load_older = move || {
        if !has_more() || loading_messages() {
            return;
        }
        let project_id = selected_project();
        // 请求侧带哨兵值（同 `load_messages`）：默认对话要后端精确返回无 project 的消息
        let req_scope = request_scope(project_id.as_deref());
        let Some(first_ts) = messages.read().first().map(|m| m.created_at) else {
            return;
        };
        loading_messages.set(true);
        spawn(async move {
            const MAX_SCAN_PAGES: usize = HISTORY_SCAN_MAX_PAGES;
            let mut before = first_ts;
            for _ in 0..MAX_SCAN_PAGES {
                let resp = match load_older_messages(common::api::ListMessagesRequest {
                    project_id: req_scope.clone(),
                    before_timestamp: Some(before),
                    limit: Some(HISTORY_PAGE_SIZE),
                    ..Default::default()
                })
                .await
                {
                    Ok(resp) => resp,
                    Err(e) => {
                        toast.error(format!("加载更多消息失败: {}", e));
                        break;
                    }
                };

                let full_page = resp.messages.len() >= HISTORY_PAGE_SIZE;
                let next_before = resp.messages.first().map(|m| m.created_at);
                let older: Vec<_> = resp
                    .messages
                    .into_iter()
                    .filter(|m| in_project_context(m, project_id.as_deref()))
                    .collect();

                let has_visible = !older.is_empty();
                if has_visible {
                    let mut current = messages.write();
                    let mut merged = older;
                    merged.append(&mut current);
                    *current = merged;
                }

                if !full_page {
                    has_more.set(false);
                    break;
                }
                if has_visible {
                    break;
                }
                match next_before {
                    Some(ts) => before = ts,
                    None => {
                        has_more.set(false);
                        break;
                    }
                }
            }
            loading_messages.set(false);
        });
    };

    let mut handle_sse_message = move |data: String| {
        let msg: MessageListItem = match serde_json::from_str(&data) {
            Ok(m) => m,
            Err(_) => return,
        };
        let cur_project = selected_project();
        // 与历史加载共用同一个判定（`in_project_context`）：默认对话（None）
        // 只接收无 project 的消息，项目会话按 project_id 匹配。
        // ⚠️ 两处口径必须一致，否则「实时推来的看得到、一刷新就没了」。
        if in_project_context(&msg, cur_project.as_deref()) {
            let mut current = messages.write();
            // 移除同 content 的乐观消息（统一使用 replace_tmp_with_real）
            replace_tmp_with_real(&mut current, &msg);
            if current.iter().any(|m| m.message_id == msg.message_id) {
                return;
            }
            // 只有「发给我」的消息才算「回复到了」：受众推送也会送来项目内的
            // Agent 间交流 / 任务知会 / 工具调用（`to_role` 是 Agent/System），
            // 那些消息不该把「思考中」提前清掉。
            let for_me = msg.to_role == 0;
            current.push(msg);
            if for_me {
                is_typing.set(false);
            }
            // 用户上翻阅读时不拽回底部，仅标记新消息（由悬浮提示按钮承接）
            if !at_bottom() {
                has_new_messages.set(true);
            }
            // 通知信息侧栏防抖刷新（任务进度/执行计划/产物可能已变化）
            refresh_tick.set(refresh_tick() + 1);
        } else if msg.to_role == 0 && msg.message_type == MSG_TEXT {
            // 问题二（AMan 拍板）：非当前会话的 Text 消息计入未读角标；
            // 默认对话（project_id=None）用哨兵键 default，项目会话用 project_id 原值。
            //
            // ⚠️ 必须限定 `to_role == 0`（发给我的）：受众推送上线后，项目内的
            // Agent 间交流、任务知会、工具调用也会推到本端，它们是**旁听内容**，
            // 按条 `+= 1` 会让角标数字凭空暴涨。旁听消息在上面的 if 分支里
            // 正常上屏，只是不计数。
            let key = match msg.project_id.as_deref() {
                Some(pid) => pid.to_string(),
                None => local_store::UNREAD_DEFAULT_KEY.to_string(),
            };
            {
                let mut badges = unread_badges.write();
                *badges.entry(key).or_insert(0) += 1;
                let _ = local_store::set_json(local_store::keys::UNREAD_BADGES, &*badges);
            }
        }
    };

    use_effect(move || {
        load_projects();
        load_reception_agent();
    });

    // 滚动恢复已下沉到 ChatMessageList：依赖里不再有 agent_state，
    // 避免 3s 轮询每次 set 都强制滚底、打断上翻阅读。

    // URL 单一事实源：路由 query 解析出的 project prop 回流进 selected_project 镜像。
    // 写入侧（选中会话 / 默认对话 / 新建项目）只做 navigator.replace（会话切换属
    // 同页状态，replace 避免逐级刷历史栈），signal 统一由本守卫回流；值相等时 set
    // 幂等，收敛无环。注册在消息加载 effect 之前，保证同一轮先同步 prop 再触发加载。
    // use_reactive 把非响应式 prop 装进内部 signal：use_effect 只追踪闭包内的响应式
    // 读取，裸捕获 prop 的 effect 在 prop 变化时不会重跑——对话页内切换项目会失灵。
    use_effect(use_reactive!(|project| {
        selected_project.set(project);
    }));

    // 修复 M1+M2：统一由 use_effect 根据 selected_project 变化加载消息，
    // 默认对话 (None) 也加载历史。handle_project_click 不再直接调用 load_messages 避免重复请求。
    use_effect(move || {
        let proj = selected_project();
        match &proj {
            Some(proj_id) => load_messages(Some(proj_id.as_str())),
            None => load_messages(None),
        }
    });

    // SSE 生命周期统一由 hook 管理：创建失败 / 连接打开 / 连接出错
    let on_message = EventHandler::new(move |data: String| handle_sse_message(data));
    let on_open = EventHandler::new(move |_| sse_connected.set(true));
    let on_error = EventHandler::new(move |_| sse_connected.set(false));
    let on_create_error = EventHandler::new(move |_| {
        toast.error("SSE 连接初始化失败，实时消息将无法接收");
    });
    use_event_source(
        "/api/v1/finance/messages/sse".to_string(),
        on_message,
        EventSourceOptions {
            on_open: Some(on_open),
            on_error: Some(on_error),
            on_create_error: Some(on_create_error),
        },
    );

    // SSE 兜底轮询：可见时每 `MESSAGE_POLL_SECS` 拉一次当前会话的增量消息
    //
    // SSE 是尽力而为的实时通道：断线（休眠 / 换网）与内存 broadcast 缓冲（100）溢出
    // 都会静默丢事件且不补发，而历史加载是查库的 —— 一旦漏就是「刷新才看到」。
    // 另一个真实场景：同一账号在别的标签页 / 手机上发的消息，本页没有乐观气泡，
    // 只有库里那条，SSE 又只推给收件人。
    //
    // ⚠️ 只起一个循环：闭包体不读 signal，异步块里读的不进 `use_effect` 依赖表
    //    （同下方 3s 状态轮询的写法），否则每拍会再 spawn 一个循环。
    use_effect(move || {
        spawn(async move {
            loop {
                gloo_timers::future::sleep(std::time::Duration::from_secs(MESSAGE_POLL_SECS)).await;
                if page_hidden() {
                    continue;
                }
                let cur_project = selected_project();
                // 请求侧带哨兵值（默认对话 → `__default__`），与首屏加载同口径
                let req_scope = request_scope(cur_project.as_deref());
                // 游标只认**服务端**时间戳（上一拍响应里的最新一条）。本地列表不能当
                // 基准：乐观消息的 `created_at` 是本机时钟，浏览器一旦快于服务端，游标
                // 就被推到「未来」，这段窗口内的消息再也拉不到（游标只前进，`.max()`
                // 还会把虚高值钉住）。没有游标时传 `None` ⇒ 取「最新一页」，用响应里
                // 服务端的时间戳校准，重复的按 `message_id` 去掉。
                let cursor = poll_cursor();
                let resp = match poll_new_messages(common::api::ListMessagesRequest {
                    project_id: req_scope,
                    // 开区间 + 回退 1ms：同一毫秒内的多条不会被游标跨过（重复按 id 去重）
                    after_timestamp: cursor.map(|c| c.saturating_sub(1)),
                    limit: Some(MESSAGE_POLL_LIMIT),
                    ..Default::default()
                })
                .await
                {
                    Ok(resp) => resp,
                    // 失败静默：SSE 仍在跑，下一拍重试
                    Err(_) => continue,
                };
                // 竞态守卫：请求期间切了会话 ⇒ 丢弃本次结果
                if selected_project() != cur_project {
                    continue;
                }
                // 游标只前进不回退：被前端滤掉的原始消息也要算进去，
                // 否则它们会一直占住窗口、永远推不到更新的一条
                if let Some(newest) = resp.messages.last().map(|m| m.created_at) {
                    poll_cursor.set(Some(cursor.map_or(newest, |c| newest.max(c))));
                }
                let fresh = resp
                    .messages
                    .into_iter()
                    .filter(|m| in_project_context(m, cur_project.as_deref()))
                    .collect::<Vec<_>>();
                if fresh.is_empty() {
                    continue;
                }
                // 与 SSE 分支同口径：只有「发给我」的新消息才清「思考中」，
                // 旁观消息（Agent 间交流 / 工具调用）不算回复到达
                let for_me = fresh.iter().any(|m| m.to_role == 0);
                let mut current = messages.write();
                if append_new_messages(&mut current, fresh) > 0 {
                    if for_me {
                        is_typing.set(false);
                    }
                    if !at_bottom() {
                        has_new_messages.set(true);
                    }
                    refresh_tick.set(refresh_tick() + 1);
                }
            }
        });
    });

    // 轮询当前会话目标 Agent 的运行时状态（3s 周期），驱动置底状态气泡实时刷新。
    // 目标直接读 target_agent_id（项目对话 = 项目 owner，默认对话 = 前台 Agent），
    // 与右侧信息面板 Agent 页同源；无法定位时归零。
    // spawn 的 future 绑定组件 scope，页面卸载时自动取消。
    use_effect(move || {
        spawn(async move {
            // 统计周期刷新：每 10 拍（30s）递增一次 stats_poll_tick，
            // Agent 统计 Tab 借既有 tick→防抖→load_stats 管道获得周期刷新，无需第二个循环
            let mut poll_count = 0u64;
            loop {
                // 目标 Agent 统一由 target_agent_id 解析（项目 owner / 前台 Agent），
                // 与右侧信息面板 Agent 页共用同一来源，两处不再各自解析
                let target = target_agent_id();
                match target {
                    Some(id) => {
                        if let Ok(resp) = get_agent(GetAgentRequest {
                            id: id.clone(),
                            ..Default::default()
                        })
                        .await
                        {
                            // 思考状态跨会话污染修复（任务 01a11b5b，方案 B）：状态与
                            // 归属成对写入，消费点据此过滤跨会话旧状态。
                            // ⚠️ Signal::set 不做相等去重：只在真变化时写，
                            // 避免 3s 轮询把整页（含消息列表 props）一起唤醒。
                            if *agent_state.peek() != resp.runtime_state {
                                agent_state.set(resp.runtime_state);
                            }
                            if agent_state_owner.peek().as_deref() != Some(id.as_str()) {
                                agent_state_owner.set(Some(id));
                            }
                            target_agent_info.set(Some(resp));
                        }
                    }
                    None => {
                        if *agent_state.peek() != 0 {
                            agent_state.set(0);
                        }
                        target_agent_info.set(None);
                    }
                }
                poll_count += 1;
                // 侧栏收起时不推进统计节拍：面板隐藏时没有任何组件消费这个 tick，
                // 推进只会白白触发一次侧栏重渲染（侧栏组件在 hidden 容器里仍挂载）
                if panel_open() && poll_count.is_multiple_of(10) {
                    stats_poll_tick.set(stats_poll_tick() + 1);
                }
                gloo_timers::future::sleep(std::time::Duration::from_secs(3)).await;
            }
        });
    });

    // ===== @ 提及 =====
    // 可 @ 的类型由 mention_kinds_for 统一界定（项目会话 Agent+任务+项目 / 默认对话
    // Agent+游离任务+项目——默认对话即默认虚拟项目，其任务域为未挂载项目的游离任务）。
    // 注意：@ 只是上下文补充，不改变消息路由——回应的仍是前台 Agent / 项目 owner。
    // 候选范围随 selected_project 变化、接待 Agent 异步到达后要补一次置顶打标，
    // 故都传 signal 而非快照值。
    let mention_tab_list = mention_tabs(&mention_kinds_for(selected_project().as_deref()));
    let mention = MentionState::new(selected_project, reception_agent);

    let handle_send = use_callback(move |_: ()| {
        let text = input_text().trim().to_string();
        let attachments = pending_attachments();
        if text.is_empty() && attachments.is_empty() {
            return;
        }
        // 忙碌不再拦截发送：消息照常入队，等当前唤醒结束后按队列顺序处理。
        // 忙碌提示保留在置底状态气泡（「正在思考中…」）与输入框 placeholder，
        // 只提示、不阻断——用户连发多条正是「边想边补充」的常见形态。
        // 回复目标：本次发送携带 reply_to_id（引用条已清空则不带）
        let reply_to_id = reply_target().map(|m| m.message_id);
        let project_id = selected_project();

        // to_agent_id 路由优先级：
        // 1. Project 对话框：从 project.owner_agent_id 取（若为 None 则不传，后端走 resolve_agent 兜底）
        // 2. 默认对话框：不传 to_agent_id，后端走 resolve_agent(ctx) 兜底
        let to_agent_id = if let Some(ref pid) = project_id {
            projects
                .read()
                .iter()
                .find(|p| &p.id == pid)
                .and_then(|p| p.owner_agent_id.clone())
        } else {
            // 默认对话框：使用已加载的前台 Agent（若有）
            reception_agent().map(|a| a.agent_id)
        };

        // 附件引用在请求侧就写进正文，与微信 / 飞书入站「先解析转换再入库」同口径；
        // `attachment_ids` 仍随请求传给后端做归属校验与兼容兜底。
        let composed_content = compose_outbound_content(&text, &attachments);
        let attachment_ids: Vec<String> = attachments.iter().map(|a| a.id.clone()).collect();
        let attachment_ids_opt = if attachment_ids.is_empty() {
            None
        } else {
            Some(attachment_ids)
        };

        // 修复 M4：保存输入快照，失败时恢复（之前 spawn 前就清空，网络失败导致输入丢失）
        let text_snapshot = text.clone();
        let attachments_snapshot = attachments.clone();
        // 乐观消息的回复指向快照（reply_to_id 本体已 move 进请求）
        let reply_to_id_snapshot = reply_to_id.clone();
        input_text.set(String::new());
        pending_attachments.set(Vec::new());
        reply_target.set(None);
        // 提及已随正文一起发出，「已提及」记录只服务于本次输入
        mention.reset_picked();
        is_typing.set(true);

        // 修复 M6：is_typing 超时保护，防止 Agent 失败/SSE 断开时永久卡死
        spawn(async move {
            gloo_timers::future::TimeoutFuture::new(60_000).await;
            is_typing.set(false);
        });

        spawn(async move {
            // 乐观消息要带上收件人 —— 否则气泡头部拼不出「→ @谁」，
            // 自己的消息会比历史消息少一截接收方信息（to_agent_id 下面会被 move 进请求）
            let to_agent_snapshot = to_agent_id.clone();
            let req = SendMessageToAgentParams {
                to_agent_id,
                content: composed_content.clone(),
                project_id: project_id.clone(),
                task_id: None,
                reply_to_id,
                attachment_ids: attachment_ids_opt,
                notify_only: None,
            };

            match send_message_to_agent(req).await {
                Ok(resp) => {
                    // 乐观气泡直接用请求的同一份正文（含附件引用），否则发送后只看到
                    // 一段文字、刷新后才冒出附件。message_type 仍硬编码 MSG_TEXT
                    // （见 utils::message 的结构守卫），渲染层 `MSG_TEXT | MSG_MIXED`
                    // 同路，content 带引用即可正常出图。
                    // 修复 L18：tmp_msg_id 用 now_ms+random 避免同毫秒发送两条消息 ID 碰撞
                    let mut user_msg = build_optimistic_user_msg(
                        composed_content,
                        project_id,
                        None,
                        to_agent_snapshot,
                    );
                    // 用户消息（to_role=Agent）只触发 Agent 唤醒，后端**不回推 SSE**，
                    // 乐观消息永远不会被真实消息替换 → 本地恒持有 tmp_ ID。
                    // 而 Agent 回复的 reply_to_id 指向本条消息的**真实 ID**，
                    // 前端引用块按 ID 在本地列表里查目标，查不到就整块不渲染
                    // （表现为「引用块要刷新后才出现」）。这里直接用响应里的
                    // 真实 message_id 覆盖，保证引用目标可解析。
                    user_msg.message_id = resp.message_id;
                    user_msg.reply_to_id = reply_to_id_snapshot;
                    // 用户主动发送 → 无条件回到底部（自己的动作，期待看到刚发的消息）
                    at_bottom.set(true);
                    let mut current = messages.write();
                    current.push(user_msg);
                }
                Err(e) => {
                    // 修复 M4：失败时恢复用户输入（恢复的是纯文字 + 待发附件，
                    // 请求里拼好的协议引用不能回灌进输入框）
                    input_text.set(text_snapshot);
                    pending_attachments.set(attachments_snapshot);
                    toast.error(format!("发送消息失败: {}", e));
                    is_typing.set(false);
                }
            }
        });
    });

    // 所有 hooks 已注册完毕，现在可以根据 auth 状态决定是否渲染
    // 未登录时显示加载指示器（use_require_auth 内部会触发重定向）
    if !is_authenticated {
        return rsx! {
            div { class: "min-h-screen flex items-center justify-center",
                Loading { size: "lg" }
            }
        };
    }

    let mut handle_project_click = move |project_id: String| {
        // URL 即状态：会话选择写入路由（replace 保持历史栈单条），signal 由守卫回流
        navigator.replace(Route::MessageChat {
            project: Some(project_id.clone()),
        });
        // 修复 M1：不再直接调用 load_messages，由 use_effect 响应 selected_project 变化触发
        // 切换会话时同步清理回复 / 话题状态 / 产物预览弹窗
        reply_target.set(None);
        thread_root.set(None);
        artifact_preview.set(None);
        artifact_preview_gen += 1;
        if is_mobile() {
            sidebar_open.set(false);
        }
    };

    // 切换信息侧栏并持久化展开状态
    let toggle_panel = move |_| {
        let next = !panel_open();
        panel_open.set(next);
        // 经组件层持久化（ai_orz:chat_panel_open，编码统一沿用 "1"/"0" 语义）
        let flag = if next { "1" } else { "0" };
        let _ = local_store::set_json(local_store::keys::CHAT_PANEL_OPEN, &flag.to_string());
    };

    // 点击「默认对话」条目：清空选中项目
    let handle_default_chat_click = move |_: ()| {
        navigator.replace(Route::MessageChat { project: None });
        // 切换会话时同步清理回复 / 话题状态 / 产物预览弹窗
        reply_target.set(None);
        thread_root.set(None);
        artifact_preview.set(None);
        artifact_preview_gen += 1;
        if is_mobile() {
            sidebar_open.set(false);
        }
    };

    // 提交新建项目：优先使用用户选择的 owner agent，未选择则继承前台 Agent
    let handle_create_project_submit = move |_| {
        let name = new_project_name().trim().to_string();
        if name.is_empty() {
            toast.error("项目名称不能为空");
            return;
        }
        let desc = new_project_desc().trim().to_string();
        // 优先使用用户手动选择的 agent，未选择则用前台 agent，都没有则 None
        let owner_agent_id =
            project_owner_agent_id().or_else(|| reception_agent().map(|a| a.agent_id));

        creating_project.set(true);
        spawn(async move {
            let req = CreateProjectRequest {
                name: name.clone(),
                description: if desc.is_empty() { None } else { Some(desc) },
                priority: None,
                tags: None,
                owner_agent_id,
            };
            match create_project(req).await {
                Ok(resp) => {
                    let new_project = ListProjectsResponseItem {
                        id: resp.id.clone(),
                        name: resp.name.clone(),
                        description: resp.description,
                        status: resp.status,
                        priority: resp.priority,
                        tags: resp.tags,
                        root_user_id: resp.root_user_id,
                        owner_agent_id: resp.owner_agent_id,
                        created_at: resp.created_at,
                        updated_at: resp.updated_at,
                    };
                    // URL 即状态：新建会话直接进路由（replace 保持历史栈单条）
                    navigator.replace(Route::MessageChat {
                        project: Some(new_project.id.clone()),
                    });
                    projects.write().push(new_project.clone());
                    messages.set(Vec::new());
                    has_more.set(true);
                    show_create_project.set(false);
                    new_project_name.set(String::new());
                    new_project_desc.set(String::new());
                    project_owner_agent_id.set(None);
                    toast.info(format!("项目「{}」已创建", name));
                }
                Err(e) => {
                    toast.error(format!("创建项目失败: {}", e));
                }
            }
            creating_project.set(false);
        });
    };

    // 打开新建项目弹窗时加载可用 Agent 列表
    let open_create_project = move |_: ()| {
        // 先把默认 owner 设为当前前台 agent
        project_owner_agent_id.set(reception_agent().map(|a| a.agent_id));
        spawn(async move {
            match list_agents(ListAgentsRequest::default()).await {
                Ok(resp) => available_agents.set(resp.items),
                Err(e) => toast.error(format!("加载 Agent 列表失败: {}", e)),
            }
        });
        show_create_project.set(true);
    };

    let current_project = projects
        .read()
        .iter()
        .find(|p| selected_project().as_deref() == Some(&p.id))
        .cloned();

    // 输入框回复条预览：正在回复谁 + 内容摘要（读 reply_target 使组件订阅其变化）
    let reply_preview = reply_target().map(|rt| {
        let content: String = rt.content.chars().take(60).collect();
        (directory().sender_name(&rt), content)
    });
    // 取消回复（引用条 × 按钮）
    // EventHandler 是 Copy：两个会话分支共用同一份，避免闭包被 move 消耗
    let cancel_reply = EventHandler::new(move |_: MouseEvent| reply_target.set(None));
    // 会话栏调度：侧栏组件只读自己的展示数据，业务逻辑仍留在页面壳
    let mut handle_default_chat_click = handle_default_chat_click;
    let mut open_create_project = open_create_project;
    let on_default_chat = EventHandler::new(move |_: ()| handle_default_chat_click(()));
    let on_open_create = EventHandler::new(move |_: ()| open_create_project(()));
    let on_project_click = EventHandler::new(move |id: String| handle_project_click(id));
    let on_filter = EventHandler::new(move |status: Option<i32>| {
        let mut reload = reload_projects_filtered;
        reload(status)
    });

    // 停止思考成功后的乐观回写：runtime_state 是 3s 周期轮询的，不立刻归零的话
    // 按钮要等最多一拍才消失，用户会以为没点中而连点。顺带清掉「等待回复」标记 ——
    // Agent 已被叫停，这一轮不会再有回复送达。
    // EventHandler 是 Copy，下面两个对话分支（项目 / 默认）可各自传一份。
    let on_thinking_cancelled = EventHandler::new(move |_: ()| {
        agent_state.set(0);
        is_typing.set(false);
    });

    // 现场句柄：页面壳只创建、不 `.read()`。3s 轮询写入只重渲染
    // ChatAgentStatusBubble / ChatMessageRow，不会重建消息列表 DOM。
    let scene = ChatSceneSignals {
        agent_state,
        agent_state_owner,
        target_agent_id,
        target_agent_info,
    };

    // 产物 chip 点击（事件委托）：toast 不实现 PartialEq，不能当 prop 下传
    let on_artifact_click = {
        let preview = artifact_preview;
        let preview_gen = artifact_preview_gen;
        let err_toast = toast;
        EventHandler::new(move |evt: MouseEvent| {
            open_artifact_preview_from_click(evt, preview, preview_gen, err_toast)
        })
    };

    // 打开话题：先清旧链，避免首帧闪出上一个话题
    let on_open_thread = {
        let mut artifact_preview = artifact_preview;
        let mut artifact_preview_gen = artifact_preview_gen;
        let mut thread_messages = thread_messages;
        let mut thread_loading = thread_loading;
        let mut thread_root = thread_root;
        EventHandler::new(move |root_id: String| {
            artifact_preview.set(None);
            artifact_preview_gen += 1;
            thread_messages.set(Vec::new());
            thread_loading.set(true);
            thread_root.set(Some(root_id));
        })
    };

    // 话题视图：服务端拉回的整条链 → 共享 ThreadChain 需要的形态。
    // 用 memo 缓存，避免聊天页因 typing / SSE 重渲染时反复克隆整条话题链。
    let thread_view = use_memo(move || {
        let root_id = thread_root().as_deref().map(str::to_string)?;
        let snapshot: Vec<MessageListItem> = thread_messages().to_vec();
        let dir = directory();
        let items: Vec<(MessageListItem, bool, String)> = snapshot
            .iter()
            .map(|m| {
                let content =
                    if is_attachment_message(m.message_type) && m.content.trim().is_empty() {
                        m.file_meta
                            .as_ref()
                            .map(|f| format!("📎 {}", f.name))
                            .unwrap_or_else(|| "[附件消息]".to_string())
                    } else {
                        m.content.clone()
                    };
                (
                    m.clone(),
                    root_id.as_str() == m.message_id.as_str(),
                    content,
                )
            })
            .collect();
        let reply_map: std::collections::HashMap<String, ReplyPreview> = snapshot
            .iter()
            .filter_map(|m| {
                let to = m.reply_to_id.as_ref()?;
                let target = snapshot.iter().find(|x| &x.message_id == to)?;
                Some((
                    m.message_id.clone(),
                    ReplyPreview {
                        message_id: target.message_id.clone(),
                        root_id: target.root_id.clone(),
                        sender: dir.sender_name(target),
                        text: target.content.chars().take(80).collect(),
                    },
                ))
            })
            .collect();
        Some((items, reply_map))
    });

    let chat_content = if let Some(project) = current_project {
        let project_name = project.name.clone();
        // 状态气泡展示名：项目 owner agent 的目录名，解析不到兜底「Agent」
        let status_sender_name = project
            .owner_agent_id
            .as_deref()
            .and_then(|id| directory().agents.get(id).cloned())
            .unwrap_or_else(|| "Agent".to_string());
        rsx! {
            div { class: "p-3 border-b border-base-300 flex items-center justify-between bg-base-100 gap-2",
                if is_mobile() {
                    button {
                        class: "btn hud-btn btn-ghost btn-sm",
                        onclick: move |_| sidebar_open.set(true),
                        "←"
                    }
                }
                h2 { class: "font-semibold text-lg truncate", "{project_name}" }
                div { class: "flex items-center gap-2 ml-auto",
                    if sse_connected() {
                        span { class: "text-success text-sm", "● 实时" }
                    } else {
                        span { class: "text-base-content/50 text-sm", "○ 连接中..." }
                    }
                    button {
                        class: "btn hud-btn btn-ghost btn-sm",
                        title: "信息面板",
                        onclick: toggle_panel,
                        "ⓘ"
                    }
                }
            }

            ChatMessageList {
                messages,
                loading_messages,
                is_typing,
                at_bottom,
                has_new_messages,
                reply_target,
                scene,
                on_load_older: EventHandler::new(move |_: ()| load_older()),
                on_open_thread,
                on_artifact_click,
                on_thinking_cancelled,
                status_sender_name: status_sender_name.clone(),
                empty_emoji: "💬".to_string(),
                empty_title: String::new(),
                empty_text: "暂无消息，开始对话吧".to_string(),
            }

            ChatInputArea {
                input_text,
                pending_attachments,
                drag_hover,
                uploading,
                on_files: attachments.on_files,
                on_send: handle_send,
                messages,
                mention,
                tabs: mention_tab_list.clone(),
                input_id: "chat-input-textarea",
                file_input_id: "chat-file-input",
                compact: false,
                agent_state: Some(agent_state),
                reply_preview: reply_preview.clone(),
                on_cancel_reply: Some(cancel_reply),
                on_remove_mention: None,
                focus_selector: None,
                on_focus: None,
                on_blur: None,
            }
        }
    } else {
        // 默认对话框（无 project_id，后端走 resolve_agent 兜底路由前台 Agent）
        let has_reception = reception_agent().is_some();
        let reception_name = reception_agent()
            .map(|a| a.agent_name)
            .unwrap_or_else(|| "未设置".to_string());
        // 状态气泡展示名：前台 Agent 名
        let status_sender_name = reception_agent()
            .map(|a| a.agent_name)
            .unwrap_or_else(|| "Agent".to_string());
        rsx! {
            div { class: "p-3 border-b border-base-300 flex items-center justify-between bg-base-100 gap-2",
                if is_mobile() {
                    button {
                        class: "btn hud-btn btn-ghost btn-sm",
                        onclick: move |_| sidebar_open.set(true),
                        "←"
                    }
                }
                h2 { class: "font-semibold text-lg", "默认对话" }
                if has_reception {
                    span { class: "text-sm text-base-content/60 truncate", "当前前台：{reception_name}" }
                } else {
                    span { class: "text-sm text-warning truncate", "⚠ 暂无前台 Agent，请创建并入职 Agent" }
                }
                div { class: "flex items-center gap-2 ml-auto",
                    if sse_connected() {
                        span { class: "text-success text-sm", "● 实时" }
                    } else {
                        span { class: "text-base-content/50 text-sm", "○ 连接中..." }
                    }
                    button {
                        class: "btn hud-btn btn-ghost btn-sm",
                        title: "信息面板",
                        onclick: toggle_panel,
                        "ⓘ"
                    }
                }
            }

            ChatMessageList {
                messages,
                loading_messages,
                is_typing,
                at_bottom,
                has_new_messages,
                reply_target,
                scene,
                on_load_older: EventHandler::new(move |_: ()| load_older()),
                on_open_thread,
                on_artifact_click,
                on_thinking_cancelled,
                status_sender_name,
                empty_emoji: (if has_reception { "💬" } else { "🤖" }).to_string(),
                empty_title: if has_reception {
                    String::new()
                } else {
                    "暂无可用的前台 Agent".to_string()
                },
                empty_text: if has_reception {
                    "与前台 Agent 直接沟通，复杂需求可新建项目组织".to_string()
                } else {
                    "请前往 Agent 管理页面创建 Agent 并设置为 Onboarded 状态".to_string()
                },
            }

            ChatInputArea {
                input_text,
                pending_attachments,
                drag_hover,
                uploading,
                on_files: attachments.on_files,
                on_send: handle_send,
                messages,
                mention,
                tabs: mention_tab_list.clone(),
                input_id: "chat-input-textarea",
                file_input_id: "chat-file-input",
                compact: false,
                agent_state: Some(agent_state),
                reply_preview: reply_preview.clone(),
                on_cancel_reply: Some(cancel_reply),
                on_remove_mention: None,
                focus_selector: None,
                on_focus: None,
                on_blur: None,
            }
        }
    };

    rsx! {
        // 统一导航栏布局：Navbar(4rem) + 剩余高度的聊天主体（flex-1 min-h-0 避免内容撑出视口）
        div { class: "h-screen flex flex-col bg-base-100",
            Navbar {}
            div { class: "flex-1 min-h-0 flex flex-col lg:flex-row relative",
            ChatProjectSidebar {
                is_mobile: is_mobile(),
                sidebar_open,
                projects,
                selected_project,
                status_filter,
                loading_projects,
                filter_loading,
                unread_badges,
                on_open_create,
                on_default_chat,
                on_project_click,
                on_filter,
            }

            div { class: "relative flex-1 flex flex-col min-w-0",
                {chat_content}

                // 话题浏览页：覆盖整个主内容列（聊天头 / 消息列表 / 输入区），
                // 消息列表保持挂载，返回时滚动位置与展开状态都不丢。
                if thread_root().is_some() {
                    div {
                        class: "absolute inset-0 z-20 flex flex-col bg-base-100",
                        div { class: "flex items-center gap-3 border-b border-base-300 px-4 py-3",
                            button {
                                class: "btn hud-btn btn-ghost btn-sm gap-1",
                                onclick: move |_| {
                                    artifact_preview.set(None);
                                    artifact_preview_gen += 1;
                                    thread_root.set(None);
                                },
                                "← 返回"
                            }
                            div { class: "min-w-0",
                                div { class: "text-sm font-semibold truncate", "话题讨论区" }
                                div { class: "text-[10px] text-base-content/40 truncate",
                                    "与当前消息相关"
                                }
                            }
                            span { class: "ml-auto flex-shrink-0 text-[10px] text-base-content/40",
                                "{thread_view().as_ref().map_or(0, |(items, _)| items.len())} 条"
                            }
                        }
                        div {
                            class: "flex-1 min-h-0 overflow-y-auto p-4",
                            // 话题视图与主消息流是兄弟节点，产物 chip 的点击委托单独挂一次
                            onclick: move |evt| open_artifact_preview_from_click(evt, artifact_preview, artifact_preview_gen, toast),
                            if thread_loading() {
                                div { class: "flex items-center justify-center py-8",
                                    Loading { size: "sm" }
                                }
                            } else if thread_view()
                                .as_ref()
                                .is_none_or(|(items, _)| items.is_empty())
                            {
                                div { class: "text-center text-sm text-base-content/50 py-8",
                                    "暂无消息"
                                }
                            } else if let Some((items, reply_map)) = thread_view().as_ref() {
                                ThreadChain {
                                    items: items.clone(),
                                    reply_map: reply_map.clone(),
                                }
                            }
                        }
                    }
                }
            }

            // 信息侧栏遮罩（移动端展开时）
            if is_mobile() && panel_open() {
                div {
                    class: "fixed inset-0 bg-black/50 z-40",
                    onclick: move |_| panel_open.set(false),
                }
            }
            // 信息侧栏：桌面静态列 / 移动端右侧抽屉（top-16 避开 Navbar）
            div {
                class: if is_mobile() {
                    if panel_open() {
                        "fixed top-16 bottom-0 right-0 z-50 w-80 bg-base-200 flex flex-col border-l border-base-300"
                    } else {
                        "hidden"
                    }
                } else if panel_open() {
                    "w-96 shrink-0 bg-base-200 flex flex-col border-l border-base-300"
                } else {
                    "hidden"
                },
                ChatSidePanel {
                    project_id: selected_project,
                    refresh_tick: refresh_tick,
                    // 面板收起时组件仍挂载（外层只是 hidden），必须显式告知可见性，
                    // 否则看不见的侧栏照样跟着 SSE 刷新发请求
                    visible: panel_open,
                    stats_poll_tick: stats_poll_tick(),
                    on_close: move |_| panel_open.set(false),
                    agent_info: target_agent_info,
                    target_agent_id: target_agent_id,
                }
            }

            // 正文产物引用点击：复用侧栏同一张预览弹窗。
            // 挂在 Router 内（ArtifactPreviewModal 用了 use_navigator），顶层条件渲染即可。
            if let Some(target) = artifact_preview() {
                ArtifactPreviewModal {
                    artifact: target.artifact,
                    project_name: target.project_name,
                    task_title: target.task_title,
                    on_close: move |_| artifact_preview.set(None),
                }
            }

            // 新建项目弹窗
            Modal {
                show: show_create_project(),
                title: "新建项目".to_string(),
                on_close: move |_| show_create_project.set(false),
                footer: rsx! {
                    button {
                        class: "btn hud-btn btn-ghost",
                        onclick: move |_| show_create_project.set(false),
                        "取消"
                    }
                    button {
                        class: "btn hud-btn btn-primary",
                        disabled: creating_project() || new_project_name().trim().is_empty(),
                        onclick: handle_create_project_submit,
                        if creating_project() {
                            span { class: "loading loading-spinner loading-sm mr-1" }
                            "创建中..."
                        } else { "创建" }
                    }
                },
                div { class: "space-y-4",
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text", "项目名称" }
                        }
                        input {
                            r#type: "text",
                            class: "input input-bordered w-full",
                            placeholder: "输入项目名称",
                            value: "{new_project_name}",
                            oninput: move |e| new_project_name.set(e.value()),
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text", "项目描述（可选）" }
                        }
                        textarea {
                            class: "textarea textarea-bordered w-full",
                            placeholder: "输入项目描述",
                            value: "{new_project_desc}",
                            oninput: move |e| new_project_desc.set(e.value()),
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text", "项目负责人 Agent" }
                        }
                        select {
                            class: "select select-bordered w-full",
                            value: "{project_owner_agent_id().unwrap_or_default()}",
                            onchange: move |e| {
                                let v = e.value();
                                project_owner_agent_id.set(if v.is_empty() { None } else { Some(v) });
                            },
                            if available_agents().is_empty() {
                                option { value: "", "暂无可用 Agent" }
                            } else {
                                option { value: "", "默认（继承前台 Agent）" }
                                for agent in available_agents().iter() {
                                    option {
                                        key: "{agent.id}",
                                        value: "{agent.id}",
                                        selected: project_owner_agent_id() == Some(agent.id.clone()),
                                        "{agent.name}"
                                    }
                                }
                            }
                        }
                        div { class: "label",
                            span { class: "label-text-alt", "不选择时默认使用当前前台 Agent" }
                        }
                    }
                }
            }
            }
        }
    }
}

/// 左侧会话栏：项目列表 / 状态过滤 / 未读角标。
///
/// ⚠️ 列表数据、过滤态、角标只读在本组件内订阅，页面壳只负责调度回调。
/// 与消息区、现场面板同款隔离：谁读谁重渲染。
#[component]
fn ChatProjectSidebar(
    is_mobile: bool,
    sidebar_open: Signal<bool>,
    projects: Signal<Vec<ListProjectsResponseItem>>,
    selected_project: Signal<Option<String>>,
    status_filter: Signal<usize>,
    loading_projects: Signal<bool>,
    filter_loading: Signal<bool>,
    unread_badges: Signal<local_store::UnreadBadges>,
    on_open_create: EventHandler<()>,
    on_default_chat: EventHandler<()>,
    on_project_click: EventHandler<String>,
    on_filter: EventHandler<Option<i32>>,
) -> Element {
    let mut sidebar_open = sidebar_open;
    let mut status_filter = status_filter;
    let visible = *sidebar_open.read();
    let active_filter = *status_filter.read();
    // 默认「未完成」过滤下项目可能为空：内存筛选兜底保持选中项可见，
    // 选中项目被隐藏时不闪变（与后端过滤拉取并存）。
    let project_items: Vec<ListProjectsResponseItem> = projects
        .read()
        .iter()
        .filter(|p| FILTER_TABS[active_filter].1.is_none_or(|s| p.status == s))
        .cloned()
        .collect();
    let selected = selected_project.read();
    let loading = *loading_projects.read();
    let filtering = *filter_loading.read();
    let badges = unread_badges.read();

    rsx! {
        // 遮罩仅依赖 sidebar_open：默认对话（selected=None）下不再恒显示
        if is_mobile && visible {
            div {
                class: "fixed inset-0 bg-black/50 z-40 lg:hidden",
                onclick: move |_| sidebar_open.set(false),
            }
        }
        div {
            class: if is_mobile {
                if visible {
                    // top-16：避开 Navbar（4rem），抽屉不盖导航栏
                    "fixed top-16 bottom-0 left-0 z-50 w-72 bg-base-200 flex flex-col border-r border-base-300 transition-transform"
                } else {
                    "fixed top-16 bottom-0 left-0 z-50 w-72 bg-base-200 flex flex-col border-r border-base-300 transition-transform -translate-x-full"
                }
            } else {
                "w-full lg:w-72 bg-base-200 flex flex-col border-r border-base-300"
            },
            div { class: "p-3 border-b border-base-300 flex items-center justify-between",
                h2 { class: "font-semibold", "项目列表" }
                button {
                    class: "btn hud-btn btn-primary btn-sm",
                    r#type: "button",
                    onclick: move |_| on_open_create.call(()),
                    "+ 新建项目"
                }
            }
            // 状态子过滤器：单选互斥 + 显式「全部」，flex-wrap 自适应抽屉宽度
            div { class: "flex flex-wrap gap-1 px-3 py-2 border-b border-base-300",
                for (idx, (label, _)) in FILTER_TABS.iter().enumerate() {
                    {
                        let chip_class = if active_filter == idx {
                            "btn hud-btn btn-xs bg-primary/10 border border-primary text-primary"
                        } else {
                            "btn hud-btn btn-xs bg-base-100 border border-base-300 text-base-content/70"
                        };
                        rsx! {
                            button {
                                key: "{idx}",
                                class: "{chip_class}",
                                r#type: "button",
                                onclick: move |_| {
                                    status_filter.set(idx);
                                    on_filter.call(FILTER_TABS[idx].1);
                                },
                                "{label}"
                            }
                        }
                    }
                }
            }
            // 过滤器拉取中的局部加载提示（细条，不复用全局 loading）
            if filtering {
                div { class: "flex items-center gap-2 px-3 py-1 text-xs text-base-content/60 border-b border-base-300",
                    span { class: "loading loading-dots loading-xs" }
                    "刷新中…"
                }
            }
            div { class: "flex-1 overflow-y-auto",
                {
                    let is_active = selected.is_none();
                    let item_class = if is_active {
                        "px-3 py-2 cursor-pointer bg-primary/10 border-l-4 border-primary transition-colors"
                    } else {
                        "px-3 py-2 cursor-pointer hover:bg-base-300 transition-colors"
                    };
                    let count = badges
                        .get(local_store::UNREAD_DEFAULT_KEY)
                        .copied()
                        .unwrap_or(0);
                    rsx! {
                        div {
                            class: "{item_class}",
                            onclick: move |_| on_default_chat.call(()),
                            div { class: "flex items-center justify-between gap-2",
                                div { class: "font-medium", "💬 默认对话" }
                                { unread_badge(count) }
                            }
                            div { class: "text-xs text-base-content/60", "与前台 Agent 直接沟通" }
                        }
                    }
                }
                if !loading && project_items.is_empty() {
                    div { class: "text-center text-sm text-base-content/50 py-8",
                        "该状态下暂无项目"
                    }
                } else if loading {
                    div { class: "flex items-center justify-center py-8",
                        Loading { size: "sm" }
                        span { class: "ml-2 text-sm text-base-content/60", "加载中..." }
                    }
                } else {
                    for project in project_items.iter() {
                        {
                            let id = project.id.clone();
                            let name = project.name.clone();
                            let status = project.status;
                            let is_active = selected.as_deref() == Some(id.as_str());
                            let item_class = if is_active {
                                "px-3 py-2 cursor-pointer bg-primary/10 border-l-4 border-primary transition-colors"
                            } else {
                                "px-3 py-2 cursor-pointer hover:bg-base-300 transition-colors"
                            };
                            let count = badges.get(&id).copied().unwrap_or(0);
                            rsx! {
                                div {
                                    class: "{item_class}",
                                    onclick: move |_| on_project_click.call(id.clone()),
                                    div { class: "flex items-center justify-between gap-2",
                                        div { class: "font-medium", "{name}" }
                                        { unread_badge(count) }
                                    }
                                    div { class: "text-xs text-base-content/60", "{status_text(status)}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 置底状态气泡文案：与 Agent 运行时状态匹配；None 表示不显示气泡。
///
/// 优先级：Busy（思考中）> Resting（休息中）> 刚发送等待回复 > 不显示。
/// 气泡恒渲染在消息列表末尾，新消息经 SSE 到达后自然堆叠在其上方。
///
/// 思考状态跨会话污染修复（任务 01a11b5b，排查 01a11b41，方案 B）：
/// `owner_matches` = 状态归属 Agent 与当前会话目标 Agent 一致。切换会话后
/// agent_state/is_typing 仍是旧会话的残留值（3s 轮询兜底前一拍），归属不匹配
/// 即返回 None —— 根治置底气泡误挂旧 Agent 状态（含「正在等待回复…」变体）。
fn agent_status_line(
    awaiting_reply: bool,
    state: i32,
    owner_matches: bool,
) -> Option<&'static str> {
    if !owner_matches {
        return None;
    }
    match state {
        2 => Some("正在思考中…"),
        1 => Some("正在休息，恢复精力…"),
        _ if awaiting_reply => Some("正在等待回复…"),
        _ => None,
    }
}

/// 思考状态归属校验（任务 01a11b5b，方案 B）：状态快照属于哪个 Agent、
/// 是否正挂在当前会话目标上。两侧同为 None（状态从未经轮询写入，如刚重置）
/// 视为一致，避免切换瞬间误伤新会话自身的状态。
fn agent_state_owner_matches(state_owner: Option<&str>, current_target: Option<&str>) -> bool {
    match (state_owner, current_target) {
        (Some(o), Some(t)) => o == t,
        _ => true,
    }
}

/// 消息项：可能是普通消息，也可能是日期分隔条
#[allow(clippy::large_enum_variant)]
enum MessageListEntry {
    /// 日期分隔条
    DateDivider(String),
    /// 普通消息
    Message(MessageListItem),
}

/// 将消息按日期分组，生成带日期分隔条的列表
fn group_messages_by_date(messages: &[MessageListItem]) -> Vec<MessageListEntry> {
    let mut entries: Vec<MessageListEntry> = Vec::new();
    let mut current_date = String::new();
    for msg in messages {
        let label = format_date_group_label(msg.created_at);
        if label != current_date {
            entries.push(MessageListEntry::DateDivider(label.clone()));
            current_date = label;
        }
        entries.push(MessageListEntry::Message(msg.clone()));
    }
    entries
}

/// 未读角标（问题二·UI 形态优化）：可伸缩胶囊形红底 + 数字展示。
///
/// 只改「长什么样」，不改「怎么计数」：count > 0 才渲染的显示行为原样保留；
/// 进会话清零 / 默认对话哨兵键 / 仅 Text 消息计入等计数逻辑由调用方持有，本函数零触碰。
/// 胶囊宽度随数字位数自适应伸缩（min-w 保底、px 内边距随内容撑开），
/// 尺寸与配色对齐 `hud-badge badge-sm`（11px 字号 / 20px 高）与 `badge-error` 语义。
fn unread_badge(count: u32) -> Element {
    if count == 0 {
        return rsx! { {} };
    }
    rsx! {
        span {
            class: "inline-flex items-center justify-center h-5 min-w-[1.25rem] px-1.5 rounded-full bg-error text-error-content text-[11px] font-semibold leading-none",
            "{count}",
        }
    }
}

#[cfg(test)]
mod attachment_request_tests {
    use super::compose_outbound_content;
    use crate::hooks::use_attachments::PendingAttachment;

    fn one_image() -> Vec<PendingAttachment> {
        vec![PendingAttachment {
            id: "att_1".to_string(),
            name: "设计稿.png".to_string(),
        }]
    }

    #[test]
    fn no_attachments_keeps_text_verbatim() {
        assert_eq!(compose_outbound_content("你好", &[]), "你好");
        // 用户手打的这三个字不是渠道占位，不能被清掉
        assert_eq!(compose_outbound_content("[图片]", &[]), "[图片]");
    }

    #[test]
    fn outbound_content_embeds_protocol_refs() {
        assert_eq!(
            compose_outbound_content("看这张", &one_image()),
            "看这张\n\n[设计稿.png](attachment:att_1)"
        );
        // 纯附件（无文字）也必须留下引用而不是空正文
        assert_eq!(
            compose_outbound_content("", &one_image()),
            "[设计稿.png](attachment:att_1)"
        );
    }

    /// 请求正文与唤醒侧解析必须闭环：拼出的引用能被 extract_refs 收回。
    #[test]
    fn outbound_refs_are_extractable() {
        let content = compose_outbound_content("看这张", &one_image());
        let refs = common::markdown_protocol::extract_refs(&content);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].0.scheme, "attachment");
        assert_eq!(refs[0].0.id, "att_1");
    }

    /// 结构守卫：请求体与乐观气泡共用同一份拼装结果，且没有第二处再拼一遍。
    #[test]
    fn request_and_optimistic_bubble_share_one_content() {
        let src = include_str!("chat.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default()
            .to_string();
        assert_eq!(
            src.matches("compose_outbound_content(&text, &attachments)")
                .count(),
            1
        );
        assert_eq!(src.matches("content: composed_content.clone(),").count(), 1);
        assert!(!src.contains("optimistic_content"));
    }
}

#[cfg(test)]
mod chat_state_fix_tests {
    /// 思考状态跨会话污染修复（任务 01a11b5b，排查 01a11b41）源码结构守卫：
    /// 方案 A 切换重置 + 方案 B 归属过滤的关键锚点一旦被回退即测试失败。
    /// include_str! 取本文件源码，按 #[cfg(test)] 切出代码区，防测试文本自匹配
    /// （批3/批5 实证教训）。
    fn code_region() -> String {
        include_str!("chat.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default()
            .to_string()
    }

    #[test]
    fn chat_state_fix_switch_reset_anchors_present() {
        let src = code_region();
        // 方案 A：target 解析 effect 入口切换即重置（四信号一次清干净）
        assert_eq!(
            src.matches("agent_state.set(0);\n        is_typing.set(false);\n        agent_state_owner.set(None);\n        target_agent_info.set(None);")
                .count(),
            1,
            "方案 A：切换即重置四信号块应恰出现 1 次"
        );
        // 方案 B：归属快照 signal 与轮询成对写入
        assert_eq!(
            src.matches("let mut agent_state_owner = use_signal(|| Option::<String>::None);")
                .count(),
            1,
            "方案 B：agent_state_owner signal 应恰出现 1 次"
        );
        assert_eq!(
            src.matches("agent_state_owner.set(Some(id));").count(),
            1,
            "方案 B：轮询状态与归属应成对写入"
        );
        // 方案 B：get_agent 请求带 id.clone()（归属 id 与响应分别持有）
        assert_eq!(src.matches("id: id.clone(),").count(), 1);
    }

    #[test]
    fn chat_state_fix_owner_filter_anchors_present() {
        let src = code_region();
        // agent_status_line 签名带 owner_matches 参数（带防回退 doc 注释）
        assert_eq!(
            src.matches("owner_matches: bool,").count(),
            1,
            "方案 B：agent_status_line 应带归属参数"
        );
        assert_eq!(
            src.matches("if !owner_matches {\n        return None;")
                .count(),
            1
        );
        // 归属谓词函数恰 1 处定义
        assert_eq!(
            src.matches("fn agent_state_owner_matches(state_owner: Option<&str>, current_target: Option<&str>) -> bool {")
                .count(),
            1
        );
        // 归属过滤收敛到独立状态气泡组件（原双分支内联已合并为单点）
        assert_eq!(
            src.matches("agent_state_owner_matches(owner.as_deref(), target.as_deref())")
                .count(),
            1,
            "方案 B：归属过滤应在 ChatAgentStatusBubble 单点接入"
        );
        assert_eq!(
            src.matches("agent_status_line(is_typing(), agent_state())")
                .count(),
            0,
            "旧三参调用不应残留"
        );
        // 忙碌只提示、不阻断：旧兜底门禁与「处理中」禁用态都应清零
        assert_eq!(
            src.matches("if agent_state() != 0\n            && agent_state_owner_matches(")
                .count(),
            0,
            "发送兜底门禁应已移除（忙碌只提示不阻断）"
        );
        assert_eq!(
            src.matches("对方正在处理消息，请稍候再发").count(),
            0,
            "旧的忙碌拦截提示不应残留"
        );
        assert_eq!(
            src.matches("if agent_state() == 2 && cancel_agent_id.is_some() {")
                .count(),
            0,
            "输入区不应再出现停止思考键位（停止只保留在状态气泡内）"
        );
        assert_eq!(
            src.matches("CancelThinkingStyle::Block").count(),
            0,
            "输入区 Block 停止按钮样式应已删除"
        );
        assert_eq!(
            src.matches("(agent_state() != 0 && cancel_agent_id.is_some())")
                .count(),
            0,
            "发送键不应再因忙碌禁用"
        );
    }

    #[test]
    fn chat_message_list_does_not_read_scene_signals() {
        let src = code_region();
        let start = src
            .find("fn ChatMessageList(")
            .expect("ChatMessageList must exist");
        let end = start
            + src[start..]
                .find("\n}\n")
                .expect("ChatMessageList function end must exist");
        let body = &src[start..end];
        // ⚠️ 与工作台 dock 的同类守卫对齐：现场 signal 只能由
        // ChatAgentStatusBubble / ChatMessageRow 读，消息列表容器不订阅。
        for field in [
            "agent_state",
            "agent_state_owner",
            "target_agent_id",
            "target_agent_info",
            "stats_poll_tick",
            "refresh_tick",
        ] {
            assert!(
                !body.contains(field),
                "ChatMessageList must not read {field}; keep it in the scene sub-components"
            );
        }
    }

    #[test]
    fn chat_shell_only_dispatches_panels() {
        let src = code_region();
        let start = src
            .find("pub fn MessageChat(")
            .expect("MessageChat must exist");
        let end = start
            + src[start..]
                .find("\n}\n")
                .expect("MessageChat function end must exist");
        let shell = &src[start..end];
        // 页面壳只调度组件，不内联重渲染面积大的面板
        for panel in [
            "ChatMessageList {",
            "ChatInputArea {",
            "ChatProjectSidebar {",
        ] {
            assert!(shell.contains(panel), "MessageChat must dispatch {panel}");
        }
        for inline in [
            "chat_input_area(",
            "sidebar_visible_on_mobile",
            "for project in project_items.iter()",
            "id: \"chat-scroll-container\"",
            // 输入区必须整体走共享组件：以下任一出现在页面壳里，
            // 就意味着三处聊天入口又开始各自维护输入交互
            "onpaste: move |e: dioxus::events::ClipboardEvent|",
            "show_slash_menu",
            "clipboard_files_from_paste",
        ] {
            assert!(
                !shell.contains(inline),
                "MessageChat must not inline {inline}; move it back into its panel component"
            );
        }
    }
}
