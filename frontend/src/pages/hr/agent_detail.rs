use crate::api::finance::{list_model_providers, list_tool_tags, query_tools};
use crate::api::hr::*;
use crate::api::message::{load_older_messages, poll_new_messages, send_message_to_agent};
use crate::api::project::{query_projects, query_tasks};
use crate::components::SearchableSelect;
use crate::components::chat::{MessageBubble, TypingIndicator};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::hud::{HudCard, HudPanel};
use crate::components::markdown::MarkdownRenderer;
use crate::components::modal::Modal;
use crate::components::relation_graph::{RelationGraph, RelationNodeInfo};
use crate::components::state::{EmptyState, Loading};
use crate::components::stats::AgentStatsPanel;
use crate::components::workspace_graph::{WorkspaceGraph, WorkspaceView};
use crate::layouts::app_layout::AppLayout;
use crate::pages::hr::agent_memory_panel::AgentMemoryPanel;
use crate::pages::hr::knowledge_graph::KnowledgeGraph;
use crate::store::toast::use_toast;
use crate::utils::{
    build_optimistic_user_msg, format_time_hm as format_time, replace_tmp_with_real,
    status::{short_id, skill_author_type_badge, skill_author_type_text, tag_chip},
};
use common::api::{
    AgentListItem, AgentRuntimeConfigInfo, BindToolToAgentRequest, GetAgentRequest,
    InstallSkillPackRequest, InstallSkillToAgentRequest, InstallToolPackRequest,
    ListExpiredAgentSkillsRequest, ListMessagesRequest, ListModelProvidersResponseItem,
    MessageListItem, PaginationParams, ProjectListItem, ProjectQueryRequest, RestoreSkillRequest,
    RuntimeReady, SendMessageToAgentParams, SkillListItem, SkillQueryRequest, TaskListItem,
    TaskQueryRequest, ToolListItem, ToolQueryRequest, UnbindToolFromAgentRequest,
    UninstallSkillFromAgentRequest, UninstallSkillPackRequest, UninstallToolPackRequest,
    UpdateAgentRequest, UpdateAgentStatusRequest,
};
use common::enums::{AgentStatus, AssigneeType, SkillStatus};
use dioxus::prelude::*;
use dioxus_router::{Link, use_navigator};
use std::collections::HashSet;

/// 构造带统计参数的 GetAgentRequest（4 处 get_agent 调用复用，避免重复 stats 字段字面量）
/// 详情页请求的字段开关。
///
/// 详情页需要展示工具/技能全景（三分组），故显式打开 `with_tools` / `with_skills`；
/// 其余调用方（如聊天侧面板）使用 `..Default::default()`，不会装配全景数据。
fn build_agent_stats_request(id: String) -> GetAgentRequest {
    GetAgentRequest {
        id,
        with_stats: Some(true),
        with_model_call_stats: Some(true),
        stats_time_start: None,
        stats_time_end: None,
        stats_interval: Some("daily".to_string()),
        with_tools: Some(true),
        with_skills: Some(true),
    }
}

/// 技能卡片：神经 / 技能包 / 独立三分组共用。
/// 分组差异通过 `tone`（HUD 面板色调）与 `neural_badge`（是否带"神经"徽章）表达；
/// 卸载动作通过 `on_uninstall` 上抛（参数为 (skill_id, skill_name)），由调用方统一处理。
/// Agent 私有目录下单个技能卡片（活动技能 / 过期副本通用）。
/// - 状态徽章：根据 skill.status 自动区分 Expired（error/不可用）、Published（success/已发布）、Draft（info/草稿）。
/// - `on_uninstall`：针对活动技能的"卸载"按钮（Expired 技能在后端不绑定 agent 安装关系，走恢复而非卸载，按钮隐藏）。
/// - `on_restore`：可选；传入后**且** skill.status=Expired 时，会显示「恢复」按钮，点击调用恢复接口。
#[component]
fn SkillCard(
    skill: SkillListItem,
    tone: &'static str,
    neural_badge: bool,
    on_uninstall: EventHandler<(String, String)>,
    on_restore: Option<EventHandler<String>>,
) -> Element {
    let skill_id = skill.id.clone();
    let skill_name = skill.name.clone();
    let skill_desc = skill.description.clone();
    let tags = skill.tags.clone();
    let is_expired = matches!(skill.status, SkillStatus::Expired);
    let (status_label, status_badge_cls) = match skill.status {
        SkillStatus::Expired => ("已过期", "badge hud-badge badge-error"),
        SkillStatus::Published => ("已发布", "badge hud-badge badge-success"),
        SkillStatus::Draft => ("草稿", "badge hud-badge badge-info badge-outline"),
    };
    // 过期副本无绑定安装关系，不显示「卸载」；过期的卸载没有语义，也避免用户以为能卸载而 API 失败。
    let show_uninstall = !is_expired;
    let show_restore = is_expired && on_restore.is_some();
    // 过期副本强调边框：让用户一眼感知这与活动技能卡不是一组。
    let wrapper_cls = if is_expired {
        "rounded-lg ring-2 ring-error/30 p-1 bg-error/5"
    } else {
        ""
    };
    // 为两个 move onclick 闭包准备独立副本（避免 rsx 内 let 绑定的 "expected identifier" 错误）
    let skill_id_for_uninstall = skill_id.clone();
    let skill_name_for_uninstall = skill_name.clone();
    let skill_id_for_restore = skill_id.clone();

    let author_type = skill.author_type;
    let author_label = skill_author_type_text(author_type);
    let author_badge_cls = skill_author_type_badge(author_type);
    let author_short = short_id(&skill.author_id);

    rsx! {
        div { class: "{wrapper_cls}",
            HudCard { tone: Some(tone),
                div { class: "flex justify-between items-start",
                    span { class: "font-medium", "{skill_name}" }
                    div { class: "flex gap-1",
                        if neural_badge {
                            span { class: "badge orz-tag badge-xs", "神经" }
                        }
                        span { class: "{status_badge_cls}", "{status_label}" }
                    }
                }
                if !skill_desc.is_empty() {
                    p { class: "text-sm text-base-content/70 mt-2", "{skill_desc}" }
                }
                if !tags.is_empty() {
                    div { class: "flex flex-wrap gap-1 mt-2",
                        for tag in tags.iter() {
                            span { class: "{tag_chip()}", "{tag}" }
                        }
                    }
                }
                // === 作者行：对齐 HUD 基准样式（属性类走 orz-tag chip，ID 走等宽字体短展示）
                // 同一套 class / 文案 / 短 ID 规则，和 skills.rs 创建者列完全同源。
                div { class: "flex items-center gap-2 mt-2 pt-2 border-t border-base-200/60",
                    span { class: "{author_badge_cls}", "{author_label}" }
                    span { class: "font-mono text-xs text-base-content/60 select-all", "{author_short}" }
                }
                div { class: "card-actions justify-end mt-3",
                    if show_uninstall {
                        button {
                            class: "btn hud-btn btn-error btn-sm",
                            onclick: move |_| {
                                // FnMut 闭包体内 clone 出 owned 参数再抛事件；不能直接 move 捕获的外层 String（只能使用一次）。
                                let id = skill_id_for_uninstall.clone();
                                let nm = skill_name_for_uninstall.clone();
                                on_uninstall.call((id, nm));
                            },
                            "卸载"
                        }
                    }
                    if let Some(restore_handler) = on_restore {
                        if show_restore {
                            button {
                                class: "btn hud-btn btn-success btn-sm",
                                title: "把该过期副本恢复为 Draft，重新放入私有目录活动列表中",
                                onclick: move |_| {
                                    let rid = skill_id_for_restore.clone();
                                    restore_handler.call(rid);
                                },
                                "↩ 恢复"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 将工具就绪状态映射为关系图连线的（标签, 描述），用于按连通性着色与 hover 提示。
/// - 就绪：tag="ready"、无描述
/// - 未就绪：tag="not_ready"、描述含原因与修复提示
fn tool_edge_meta(rr: &RuntimeReady) -> (Option<String>, Option<String>) {
    match rr {
        RuntimeReady::NotReady { reason, hint } => (
            Some("not_ready".to_string()),
            Some(format!("未就绪（{}）：{}", reason, hint)),
        ),
        _ => (Some("ready".to_string()), None),
    }
}

/// 工具卡片：神经 / 直接绑定 / 工具包三分组共用。
/// `show_unbind` 控制是否渲染「解绑」按钮（仅直接绑定组）；
/// `badge` + `badge_class` 表达分组徽章（神经 / 已绑定 / 来自 xx）；
/// `runtime_ready` 的「未就绪」警示在组件内统一处理（advisory，不阻止使用）。
#[component]
fn ToolCard(
    tool: ToolListItem,
    tone: &'static str,
    badge: String,
    badge_class: &'static str,
    show_unbind: bool,
    on_unbind: EventHandler<(String, String)>,
) -> Element {
    let tool_id = tool.id.clone();
    let tool_name = tool.name.clone();
    let tool_desc = tool.description.clone().unwrap_or_default();
    let tags = tool.tags.clone();
    let not_ready_title = match &tool.runtime_ready {
        RuntimeReady::NotReady { reason, hint } => format!("未就绪（{}）：{}", reason, hint),
        _ => String::new(),
    };

    rsx! {
        HudCard { tone: Some(tone),
            div { class: "flex justify-between items-start",
                span { class: "font-medium", "{tool_name}" }
                div { class: "flex gap-1",
                    if !not_ready_title.is_empty() {
                        span {
                            class: "badge hud-badge badge-warning badge-outline",
                            title: "{not_ready_title}",
                            "未就绪"
                        }
                    }
                    span { class: "{badge_class}", "{badge}" }
                }
            }
            p { class: "text-sm text-base-content/70 mt-2", "{tool_desc}" }
            if !tags.is_empty() {
                div { class: "flex flex-wrap gap-1 mt-2",
                    for tag in tags.iter() {
                        span { class: "{tag_chip()}", "{tag}" }
                    }
                }
            }
            if show_unbind {
                div { class: "card-actions justify-end mt-3",
                    button {
                        class: "btn hud-btn btn-error btn-sm",
                        onclick: move |_| {
                            let id = tool_id.clone();
                            let name = tool_name.clone();
                            on_unbind.call((id, name));
                        },
                        "解绑"
                    }
                }
            }
        }
    }
}

/// 包卡片（工具包 / 技能包共用）：与具体工具 / 技能卡片（ToolCard / SkillCard）同构的 HUD 风格。
/// 整卡可点击 → 切换下方列表的包（tag）筛选条件（再次点击取消）；选中态以
/// `ring-2 ring-primary` + primary 色调高亮。「卸载」按钮触发二次确认弹窗
/// （stop_propagation 避免误触整卡筛选）。
#[component]
fn PackCard(
    pack_tag: String,
    subtitle: String,
    selected: bool,
    on_toggle: EventHandler<String>,
    on_uninstall: EventHandler<String>,
) -> Element {
    let toggle_tag = pack_tag.clone();
    let uninst_tag = pack_tag.clone();
    rsx! {
        div {
            class: if selected { "cursor-pointer rounded-lg ring-2 ring-primary" } else { "cursor-pointer" },
            onclick: move |_| on_toggle.call(toggle_tag.clone()),
            HudCard { tone: Some(if selected { "primary" } else { "neutral" }),
                div { class: "flex justify-between items-start",
                    span { class: "font-medium", "📦 {pack_tag}" }
                    span { class: "badge hud-badge badge-success", "已安装" }
                }
                if !subtitle.is_empty() {
                    p { class: "text-sm text-base-content/70 mt-2", "{subtitle}" }
                }
                div { class: "card-actions justify-end mt-3",
                    button {
                        class: "btn hud-btn btn-error btn-sm",
                        onclick: move |e: Event<MouseData>| {
                            e.stop_propagation();
                            on_uninstall.call(uninst_tag.clone());
                        },
                        "卸载"
                    }
                }
            }
        }
    }
}

// 生命周期状态（已入职/面试中…）属「状态」语义，走 hud-badge 彩色玻璃徽章；
// 角色 / 类型等属性标签才走 orz-tag（见 utils/status.rs 与 components）。
fn binding_status_badge_class() -> &'static str {
    "badge hud-badge badge-sm"
}

fn agent_status_label(status: i32) -> String {
    match status {
        0 => "已删除".to_string(),
        1 => "面试中".to_string(),
        2 => "待入职".to_string(),
        3 => "已入职".to_string(),
        4 => "已离职".to_string(),
        5 => "待离职".to_string(),
        _ => status.to_string(),
    }
}

fn kind_badge_class(kind: &str) -> &'static str {
    // Agent 来源类型（local/cli/remote）是「类别标签」，统一走中性 orz-tag chip
    match kind {
        "local" => "badge orz-tag badge-sm",
        "cli" => "badge orz-tag badge-sm",
        "remote" => "badge orz-tag badge-sm",
        _ => "badge orz-tag badge-sm",
    }
}

fn kind_label(kind: &str) -> String {
    match kind {
        "local" => "本地 Agent".to_string(),
        "cli" => "CLI Agent".to_string(),
        "remote" => "远程 Agent".to_string(),
        _ => kind.to_string(),
    }
}

const STATUS_OPTIONS: &[(i32, &str)] = &[
    (0, "已删除"),
    (1, "面试中"),
    (2, "待入职"),
    (3, "已入职"),
    (4, "已离职"),
    (5, "待离职"),
];

/// 消息流单页条数（双向查询各取 PAGE_SIZE，合并去重后取最新的 PAGE_SIZE 条）
const MSG_PAGE_SIZE: usize = 20;

/// 发送后等待 Agent 回复的轮询节奏：最多 20 次 × 3s ≈ 60s，与 is_typing 超时保护一致
const REPLY_POLL_MAX: usize = 20;
const REPLY_POLL_INTERVAL_MS: u32 = 3_000;

/// 拉取「与指定 Agent 相关」的消息（双向 OR 查询 + 合并去重）
///
/// 后端 `/api/v1/finance/messages` 的 `from_id` 与 `to_id` 是 **AND** 关系，
/// 而「与该 Agent 相关」语义是 `from_id == aid **OR** to_id == aid`，
/// 因此分两次查询（`from_id=aid` / `to_id=aid`）后按 `message_id` 去重、
/// 按 `created_at` 升序合并。这样翻页（before_timestamp）也是准确的，
/// 不像旧实现那样「拉 50 条再客户端过滤」（过滤后可能不足一页，且翻页会漏消息）。
///
/// `before` 为 `None` 时取最新一页；否则取早于该时间戳的一页。
async fn fetch_agent_messages(
    aid: &str,
    before: Option<i64>,
) -> Result<Vec<MessageListItem>, crate::api::ApiError> {
    let base = ListMessagesRequest {
        limit: Some(MSG_PAGE_SIZE),
        before_timestamp: before,
        ..Default::default()
    };

    let from_req = ListMessagesRequest {
        from_id: Some(aid.to_string()),
        ..base.clone()
    };
    let to_req = ListMessagesRequest {
        to_id: Some(aid.to_string()),
        ..base
    };

    // 两次查询顺序 await（同一后端，串行开销可接受，避免引入额外并发依赖）
    let sent = load_older_messages(from_req).await?.messages;
    let received = load_older_messages(to_req).await?.messages;

    let mut merged: Vec<MessageListItem> = Vec::with_capacity(sent.len() + received.len());
    let mut seen: HashSet<String> = HashSet::new();
    for m in sent.into_iter().chain(received) {
        if seen.insert(m.message_id.clone()) {
            merged.push(m);
        }
    }
    merged.sort_by_key(|m| m.created_at);
    // 双向各取 PAGE_SIZE，合并后可能到 2×PAGE_SIZE，只保留最新的 PAGE_SIZE 条
    if merged.len() > MSG_PAGE_SIZE {
        merged.drain(..merged.len() - MSG_PAGE_SIZE);
    }
    Ok(merged)
}

#[component]
pub fn HrAgentDetail(id: String) -> Element {
    // 方案 B：订阅路由并把 id 同步到响应式 rid，use_resource 绑定 rid，
    // 拉取仅在 id 变化时触发（同变体 /hr/agents/A → /hr/agents/B 也会重拉）
    let route = dioxus_router::use_route::<crate::pages::Route>();
    let mut rid = use_signal(String::new);
    if let crate::pages::Route::HrAgentDetail { id: route_id } = &route
        && *rid.peek() != *route_id
    {
        rid.set(route_id.clone());
    }
    let mut agent_res = use_resource(move || {
        let id = rid();
        async move { get_agent(build_agent_stats_request(id)).await }
    });
    let mut messages = use_signal(Vec::<MessageListItem>::new);
    // 消息流分页状态（后端分页拉取，不依赖 SSE）
    let mut msg_loading = use_signal(|| false);
    let mut msg_loading_more = use_signal(|| false);
    let mut msg_has_more = use_signal(|| true);
    let mut is_typing = use_signal(|| false);
    // 修复 H3：为聊天输入框分离独立 signal，避免状态污染
    let mut input_message = use_signal(String::new);
    let toast = use_toast();
    let mut tool_packs = use_signal(Vec::<String>::new);
    let mut tool_tags = use_signal(Vec::<String>::new);
    let mut skill_packs = use_signal(Vec::<String>::new);
    let mut skill_tags = use_signal(Vec::<String>::new);
    // 工具/技能列表筛选：包（tag）筛选；点击工具包/技能包 chip 即追加/移除 tag 条件
    let mut tool_filter_tags = use_signal(Vec::<String>::new);
    let mut skill_filter_tags = use_signal(Vec::<String>::new);
    // 统一搜索框选中已安装项时，在列表中高亮定位（set 对应 id；use_effect 滚动到该卡片）
    let mut highlight_tool_id = use_signal(|| None::<String>);
    let mut highlight_skill_id = use_signal(|| None::<String>);
    // 技能包卸载确认对话框：存当前待卸载的 tag
    let mut show_skill_pack_uninstall_dialog = use_signal(|| None::<String>);
    // 工具包卸载确认对话框：存当前待卸载的 tag
    let mut show_tool_pack_uninstall_dialog = use_signal(|| None::<String>);
    // 输入框选择后安装确认：存 (id, name)
    let mut show_bind_tool_dialog = use_signal(|| None::<(String, String)>);
    let mut show_install_skill_dialog = use_signal(|| None::<(String, String)>);
    // 卡片卸载/解绑确认：存 (id, name)
    let mut show_tool_unbind_dialog = use_signal(|| None::<(String, String)>);
    let mut show_skill_uninstall_dialog = use_signal(|| None::<(String, String)>);
    // 工具搜索动态结果与加载状态（SearchableSelect 动态搜索模式）
    let mut tool_search_results = use_signal(Vec::<ToolListItem>::new);
    let mut tool_search_loading = use_signal(|| false);
    // 单个技能安装：搜索结果、加载状态、已安装技能列表
    let mut skill_search_results = use_signal(Vec::<SkillListItem>::new);
    let mut skill_search_loading = use_signal(|| false);
    let mut show_edit_modal = use_signal(|| false);
    let mut edit_name = use_signal(String::new);
    let mut edit_roles = use_signal(Vec::<String>::new);
    let mut edit_roles_input = use_signal(String::new);
    let mut edit_description = use_signal(String::new);
    let mut edit_capabilities = use_signal(Vec::<String>::new);
    let mut edit_capabilities_input = use_signal(String::new);
    let mut edit_soul = use_signal(String::new);
    let mut edit_model_provider_id = use_signal(String::new);
    // 运行时配置编辑字段（number input 用 String 承载，提交时 parse）
    let mut edit_max_thinking_rounds = use_signal(String::new);
    let mut edit_intent_analyze_max_rounds = use_signal(String::new);
    let mut edit_summary_max_rounds = use_signal(String::new);
    let mut edit_think_timeout_secs = use_signal(String::new);
    let mut saving_meta = use_signal(|| false);
    let mut model_providers = use_signal(Vec::<ListModelProvidersResponseItem>::new);
    // Tab 切换信号：0=概览 1=工具与技能 2=工具关系 3=对话与记忆 4=关系图
    let mut active_tab = use_signal(|| 0usize);
    // 状态切换/入职操作的 Agent ID（必须在顶层声明，不能在渲染分支中调用 use_signal）
    // 克隆一份避免 move 走组件参数 id（后续多处仍使用 id.clone()）
    let agent_id_for_signal = id.clone();
    let agent_id_signal = use_signal(move || agent_id_for_signal);
    // 关系图所需数据：全局 projects + tasks + agents 列表
    let mut graph_projects = use_signal(Vec::<ProjectListItem>::new);
    let mut graph_tasks = use_signal(Vec::<TaskListItem>::new);
    let graph_agents = use_signal(Vec::<AgentListItem>::new);

    let skill_packs_list = skill_packs.read().clone();
    let tool_packs_list = tool_packs.read().clone();

    let rid_for_load = rid;
    let load_data = move || {
        let aid = rid_for_load();
        spawn(async move {
            match list_installed_tool_packs(&aid).await {
                Ok(resp) => tool_packs.set(resp.installed_tags),
                Err(e) => toast.error(format!("获取工具包失败: {}", e)),
            }
            match list_tool_tags().await {
                Ok(resp) => tool_tags.set(resp.tags),
                Err(e) => toast.error(format!("获取工具包标签失败: {}", e)),
            }
            match list_installed_skill_packs(&aid).await {
                Ok(resp) => skill_packs.set(resp.skill_packs),
                Err(e) => toast.error(format!("获取技能包失败: {}", e)),
            }
            match list_skill_tags().await {
                Ok(resp) => skill_tags.set(resp.tags),
                Err(e) => toast.error(format!("获取技能包标签失败: {}", e)),
            }
            // 消息流：后端分页拉取，只取与本 Agent 相关的信息流
            // （from_id=aid OR to_id=aid，见 fetch_agent_messages 的双向查询说明）
            msg_loading.set(true);
            match fetch_agent_messages(&aid, None).await {
                Ok(page) => {
                    msg_has_more.set(page.len() >= MSG_PAGE_SIZE);
                    messages.set(page);
                }
                Err(e) => toast.error(format!("加载消息失败: {}", e)),
            }
            msg_loading.set(false);
            match list_model_providers().await {
                Ok(resp) => model_providers.set(resp.providers),
                Err(e) => toast.error(format!("加载模型提供商列表失败: {}", e)),
            }
            // 按需加载关系图数据（避免全量加载）
            // 1. 按 agent_id 过滤 tasks（assignee_type=Agent）
            let req = TaskQueryRequest {
                assignee_id: Some(aid.clone()),
                assignee_type: Some(AssigneeType::Agent),
                pagination: PaginationParams::default(),
                ..Default::default()
            };
            match query_tasks(&req).await {
                Ok(page) => {
                    let tasks = page.items;
                    // 2. 从 tasks 中收集 unique project_ids，批量查询消除 N+1
                    let project_ids: Vec<String> = tasks
                        .iter()
                        .filter_map(|t| t.project_id.clone())
                        .collect::<HashSet<_>>()
                        .into_iter()
                        .collect();
                    graph_tasks.set(tasks);

                    if project_ids.is_empty() {
                        graph_projects.set(Vec::new());
                    } else {
                        let req = ProjectQueryRequest {
                            ids: Some(project_ids),
                            pagination: PaginationParams::default(),
                            ..Default::default()
                        };
                        match query_projects(&req).await {
                            Ok(page) => graph_projects.set(page.items),
                            Err(e) => toast.error(format!("批量获取项目失败: {}", e)),
                        }
                    }
                }
                Err(e) => toast.error(format!("获取任务列表失败: {}", e)),
            }
        });
    };

    use_effect(move || {
        let _ = rid();
        load_data();
    });

    // graph_agents 随 agent_res 完成而同步（load_data 并发执行，避免读取不到 agent）
    let agent_res_eff = agent_res;
    let mut graph_agents_eff = graph_agents;
    use_effect(move || {
        if let Some(Ok(a)) = agent_res_eff.read().as_ref() {
            graph_agents_eff.set(vec![AgentListItem::from(a)]);
        }
    });

    // === 消息流：后端分页拉取（不使用 SSE）===
    //
    // Agent 详情页的对话定位是「信息流回看」，不需要实时推送：
    // SSE 会常驻一条连接、且让消息列表与实时事件耦合（断连/重连都要处理）。
    // 改为：进入页面拉最新一页，点「加载更早」按 before_timestamp 向前翻页，
    // 发送消息后短时轮询拉取 Agent 回复（见 handle_send）。

    // 上拉加载更早消息。
    // 捕获 `rid`（Signal，Copy）而非 `id`（String）—— 让闭包保持 Copy，
    // 才能在「加载更早」按钮等场景复用。
    let load_more = move |_| {
        if msg_loading_more() || !msg_has_more() {
            return;
        }
        let aid = rid();
        msg_loading_more.set(true);
        spawn(async move {
            let before = messages.read().first().map(|m| m.created_at);
            match fetch_agent_messages(&aid, before).await {
                Ok(mut older) => {
                    if older.is_empty() {
                        msg_has_more.set(false);
                    } else {
                        msg_has_more.set(older.len() >= MSG_PAGE_SIZE);
                        older.extend(messages.read().iter().cloned());
                        messages.set(older);
                    }
                }
                Err(e) => toast.error(format!("加载更早消息失败: {}", e)),
            }
            msg_loading_more.set(false);
        });
    };

    // 拉取本 Agent 的新消息（after_timestamp 增量），追加到列表尾部。
    // 同 load_more：捕获 `rid` 保持闭包 Copy，供「刷新」按钮与发送后轮询共用。
    let poll_new = move || {
        let aid = rid();
        spawn(async move {
            let after = messages.read().iter().map(|m| m.created_at).max();
            let req = ListMessagesRequest {
                limit: Some(MSG_PAGE_SIZE),
                after_timestamp: after,
                ..Default::default()
            };
            let from_req = ListMessagesRequest {
                from_id: Some(aid.clone()),
                ..req.clone()
            };
            let to_req = ListMessagesRequest {
                to_id: Some(aid.clone()),
                ..req
            };
            let mut incoming = Vec::new();
            for r in [from_req, to_req] {
                if let Ok(resp) = poll_new_messages(r).await {
                    incoming.extend(resp.messages);
                }
            }
            if incoming.is_empty() {
                return;
            }
            incoming.sort_by_key(|m| m.created_at);
            let mut current = messages.write();
            let mut changed = false;
            for msg in incoming {
                // 移除同 content 的乐观消息（统一使用 replace_tmp_with_real）
                replace_tmp_with_real(&mut current, &msg);
                if current.iter().any(|m| m.message_id == msg.message_id) {
                    continue;
                }
                current.push(msg);
                changed = true;
            }
            // 收到 Agent 的回复即可停止「思考中」提示
            if changed && current.iter().any(|m| m.from_role == 1 && m.from_id == aid) {
                is_typing.set(false);
            }
        });
    };

    let id_for_send = id.clone();
    let handle_send = use_callback(move |_: ()| {
        let text = input_message().trim().to_string();
        if text.is_empty() {
            return;
        }
        let aid = id_for_send.clone();

        // 修复 M4（对齐 chat.rs）：保存输入快照，失败时恢复
        let text_snapshot = text.clone();
        input_message.set(String::new());
        is_typing.set(true);

        // 修复 M6（对齐 chat.rs）：is_typing 超时保护，防止 Agent 失败时永久卡死。
        // 用 `callback::Timeout::new(..).forget()` 而非 `spawn + TimeoutFuture`：
        // 组件卸载时 spawn 的 future 被 drop，TimeoutFuture 底层 Closure::once 在
        // 「已入队未触发」时会被浏览器调用已释放的闭包 → "closure invoked recursively
        // or after being dropped"。forget() 有意泄漏该一次性定时器，规避该竞态。
        gloo_timers::callback::Timeout::new(
            REPLY_POLL_MAX as u32 * REPLY_POLL_INTERVAL_MS,
            move || {
                is_typing.set(false);
            },
        )
        .forget();

        spawn(async move {
            let req = SendMessageToAgentParams {
                to_agent_id: Some(aid.clone()),
                content: text.clone(),
                project_id: None,
                task_id: None,
                reply_to_id: None,
                attachment_ids: None,
            };

            match send_message_to_agent(req).await {
                Ok(_) => {
                    // 构造乐观用户消息（统一使用 build_optimistic_user_msg）
                    let user_msg = build_optimistic_user_msg(text, None, None, Some(aid.clone()));
                    messages.write().push(user_msg);

                    // 无 SSE：发送后短时轮询拉取 Agent 回复（最多 ~60s，与超时保护同步）。
                    // 一旦 `poll_new` 收到该 Agent 的消息就会把 is_typing 置回 false。
                    for _ in 0..REPLY_POLL_MAX {
                        gloo_timers::future::TimeoutFuture::new(REPLY_POLL_INTERVAL_MS).await;
                        if !is_typing() {
                            break;
                        }
                        poll_new();
                    }
                }
                Err(e) => {
                    // 修复 M4：失败时恢复用户输入
                    input_message.set(text_snapshot);
                    toast.error(format!("发送消息失败: {}", e));
                    is_typing.set(false);
                }
            }
        });
    });

    rsx! {
        AppLayout {
            div { class: "mb-6 flex items-center justify-between",
                h1 { class: "text-2xl font-bold", "Agent 详情" }
                Link { class: "btn hud-btn btn-ghost", to: crate::pages::Route::HrAgents {},
                    "← 返回列表"
                }
            }
            {match agent_res.read().as_ref() {
                None => rsx! { Loading {} },
                Some(Ok(a)) => {
                    let a = a.clone();
            let capabilities = a.capabilities.clone().unwrap_or_default();
            let desc = a.description.as_deref().unwrap_or("");
            // ---- Agent 关联全景视图（后端返回扁平去重列表，前端自行按 tag 分包） ----
            let tool_list: Vec<ToolListItem> = a.tool_list.clone().unwrap_or_default();
            // ===== 过期技能虚拟 pack：📦 已过期技能 =====
            // - 虚拟 tag 常量（永远不会与真实 skill pack 撞名）；作为额外 chip 出现在技能包区。
            // - 点击选中后**首次**才懒加载 list_expired_agent_skills，结果写入 expired_skill_list。
            // - 再点取消（从 filter 移除）就不再显示该分组，下次再选则复用已缓存的 expired list（恢复操作已实时维护）。
            const EXPIRED_PACK_TAG: &str = "__expired_pack__";
            const EXPIRED_PACK_LABEL: &str = "📦 已过期技能";
            // 活动技能列表改成信号：恢复成功后需要把 skill 插入这里。
            let mut skill_list = use_signal(|| a.skill_list.clone().unwrap_or_default());
            let mut expired_skill_list = use_signal(Vec::<SkillListItem>::new);
            // 是否已懒加载过（选中虚拟 pack 时只在首次发请求）
            let mut expired_loaded = use_signal(|| false);

            let aid_for_load = agent_id_signal();
            let mut on_toggle_expired_pack = move || {
                let filter_has = skill_filter_tags.read().iter().any(|t| t == EXPIRED_PACK_TAG);
                if filter_has {
                    // 取消：从 filter 移除（保留 list 缓存，下次选中不重拉）
                    let mut v = skill_filter_tags.write();
                    v.retain(|t| t != EXPIRED_PACK_TAG);
                } else {
                    // 选中：先追加 filter tag，若未加载则触发异步 fetch
                    skill_filter_tags.write().push(EXPIRED_PACK_TAG.to_string());
                    if !expired_loaded() {
                        let agent_id = aid_for_load.clone();
                        spawn(async move {
                            match list_expired_agent_skills(ListExpiredAgentSkillsRequest {
                                agent_id: agent_id.clone(),
                            })
                            .await
                            {
                                Ok(r) => {
                                    expired_skill_list.set(r.skills);
                                    expired_loaded.set(true);
                                }
                                Err(e) => toast.error(format!("加载过期技能失败: {}", e)),
                            }
                        });
                    }
                }
            };
            // Restore: 调用接口，成功时把 skill 从 expired 移到 active（信号维护）。
            let toast_c = toast;
            let on_restore_skill = move |skill_id: String| {
                let skill_id_c = skill_id.clone();
                spawn(async move {
                    match restore_skill(RestoreSkillRequest { skill_id: skill_id_c.clone() }).await {
                        Ok(r) => {
                            // 1) 从 expired list 移除
                            expired_skill_list.write().retain(|s| s.id != r.id);
                            // 2) 转成 SkillListItem 追加 active
                            let item = SkillListItem {
                                id: r.id.clone(),
                                name: r.name.clone(),
                                description: r.description.clone(),
                                tags: r.tags.clone(),
                                category: r.category.clone(),
                                parent_skill_id: r.parent_skill_id.clone(),
                                author_id: r.author_id.clone(),
                                author_type: r.author_type,
                                status: r.status,
                                created_at: r.created_at,
                                updated_at: r.updated_at,
                            };
                            if !skill_list.read().iter().any(|s| s.id == r.id) {
                                skill_list.write().push(item);
                            }
                            toast_c.success(format!("已恢复：{}", r.name));
                        }
                        Err(e) => toast_c.error(format!("恢复失败：{}", e)),
                    }
                });
            };
            // 已安装工具包 tag 集合（含 neural，用于分组与关系图着色）
            let tool_packs_set: std::collections::HashSet<String> =
                tool_packs_list.iter().cloned().collect();
            // 技能分组 tag：已安装技能包 + 始终包含 neural
            let mut skill_group_tags: Vec<String> = skill_packs_list.clone();
            if !skill_group_tags.iter().any(|t| t == "neural") {
                skill_group_tags.insert(0, "neural".to_string());
            }

            // 三分组共用的卸载 / 解绑动作（统一 spawn + 刷新，卡片组件只上抛 (id, name) 事件）
            let on_uninstall_skill = move |(sid, sname): (String, String)| {
                show_skill_uninstall_dialog.set(Some((sid, sname)));
            };
            let on_unbind_tool = move |(tid, tname): (String, String)| {
                show_tool_unbind_dialog.set(Some((tid, tname)));
            };

            // 总数：扁平列表本身即去重后的全集
            let all_tool_count = tool_list.len();
            let all_skill_count = skill_list.read().len();
            // 已安装包 → 含项数映射（用于包卡片副标题）：按 tag 在扁平列表中命中计数。
            // 后端保证列表去重，故这里统计的也是去重后的项数。
            let tool_pack_counts: std::collections::HashMap<String, usize> = tool_packs_list
                .iter()
                .map(|tag| {
                    let c = tool_list
                        .iter()
                        .filter(|t| t.tags.iter().any(|x| x == tag))
                        .count();
                    (tag.clone(), c)
                })
                .collect();
            let skill_pack_counts: std::collections::HashMap<String, usize> = skill_group_tags
                .iter()
                .map(|tag| {
                    let c = skill_list
                        .read()
                        .iter()
                        .filter(|s| s.tags.iter().any(|x| x == tag))
                        .count();
                    (tag.clone(), c)
                })
                .collect();
            // Tab 按钮动态 class：避免在 rsx! 格式串中嵌套引号转义
            let tab0_class = if active_tab() == 0 { "btn hud-btn btn-sm btn-primary" } else { "btn hud-btn btn-sm btn-ghost" };
            let tab1_class = if active_tab() == 1 { "btn hud-btn btn-sm btn-primary" } else { "btn hud-btn btn-sm btn-ghost" };
            let tab2_class = if active_tab() == 2 { "btn hud-btn btn-sm btn-primary" } else { "btn hud-btn btn-sm btn-ghost" };
            let tab3_class = if active_tab() == 3 { "btn hud-btn btn-sm btn-primary" } else { "btn hud-btn btn-sm btn-ghost" };
            let tab4_class = if active_tab() == 4 { "btn hud-btn btn-sm btn-primary" } else { "btn hud-btn btn-sm btn-ghost" };
            let tab5_class = if active_tab() == 5 { "btn hud-btn btn-sm btn-primary" } else { "btn hud-btn btn-sm btn-ghost" };
            let tab6_class = if active_tab() == 6 { "btn hud-btn btn-sm btn-primary" } else { "btn hud-btn btn-sm btn-ghost" };

            // 工具/技能列表：按 tag 维度聚合 + 包（tag）筛选 + 名称搜索。
            // 预计算过滤结果（rsx 之前），把"包=筛选器"作用于下方聚合列表：
            let tool_filter = tool_filter_tags.read().clone();
            // 已安装工具 id 集合（扁平列表即全集）
            let installed_tool_ids: HashSet<String> =
                tool_list.iter().map(|t| t.id.clone()).collect();

            // 工具列表：按已安装包 tag 分组（一个工具命中多个包则分别出现在各包；扁平列表已去重）
            let mut tool_view: Vec<(String, &'static str, String, bool, Vec<ToolListItem>)> = Vec::new();
            for tag in tool_packs_list.iter() {
                let mut tools: Vec<ToolListItem> = tool_list
                    .iter()
                    .filter(|t| t.tags.iter().any(|x| x == tag))
                    .cloned()
                    .collect();
                if !tools.is_empty() {
                    tools.sort_by(|a, b| a.id.cmp(&b.id));
                    tool_view.push((
                        format!("📦 {}", tag),
                        "accent",
                        format!("来自 {}", tag),
                        false,
                        tools,
                    ));
                }
            }
            let bound_tools: Vec<ToolListItem> = tool_list
                .iter()
                .filter(|t| !t.tags.iter().any(|x| tool_packs_set.contains(x)))
                .cloned()
                .collect();
            if !bound_tools.is_empty() {
                tool_view.push((
                    "🔗 直接绑定".to_string(),
                    "success",
                    "已绑定".to_string(),
                    true,
                    bound_tools,
                ));
            }
            let tool_filtered: Vec<(String, &'static str, String, bool, Vec<ToolListItem>)> = tool_view
                .into_iter()
                .filter(|(label, _, _, _, _)| {
                    if tool_filter.is_empty() {
                        return true;
                    }
                    // 仅包分组（label 以 📦 开头）参与包筛选；直接绑定组在筛选时隐藏
                    if let Some(tag) = label.strip_prefix("📦 ") {
                        tool_filter.iter().any(|f| f == tag)
                    } else {
                        false
                    }
                })
                .filter(|(_, _, _, _, ts)| !ts.is_empty())
                .collect();

            let skill_filter = skill_filter_tags.read().clone();
            // 过期虚拟 tag 在 skill_view 分组/筛选中单独处理：从 filter 中拆分出来，
            // 不参与正常 📦 前缀 match，而是在下方 "已绑定技能" 面板末尾追加独立 Section。
            let expired_tag = EXPIRED_PACK_TAG.to_string();
            let filter_has_expired = skill_filter.iter().any(|t| t == &expired_tag);
            // 已安装技能 id 集合（扁平列表即全集）
            let installed_skill_ids: HashSet<String> =
                skill_list.read().iter().map(|s| s.id.clone()).collect();

            // 技能列表：按分组 tag（已安装技能包 + neural）分组
            let mut skill_view: Vec<(String, bool, &'static str, Vec<SkillListItem>)> = Vec::new();
            for tag in skill_group_tags.iter() {
                let mut skills: Vec<SkillListItem> = skill_list
                    .read()
                    .iter()
                    .filter(|s| s.tags.iter().any(|x| x == tag))
                    .cloned()
                    .collect();
                if !skills.is_empty() {
                    skills.sort_by(|a, b| a.id.cmp(&b.id));
                    let label = if tag == "neural" {
                        "🧠 神经技能".to_string()
                    } else {
                        format!("📦 {}", tag)
                    };
                    skill_view.push((label, tag == "neural", "accent", skills));
                }
            }
            let standalone_skills: Vec<SkillListItem> = skill_list
                .read()
                .iter()
                .filter(|s| !s.tags.iter().any(|x| skill_group_tags.iter().any(|g| g == x)))
                .cloned()
                .collect();
            if !standalone_skills.is_empty() {
                skill_view.push((
                    "🆓 独立技能".to_string(),
                    false,
                    "accent",
                    standalone_skills,
                ));
            }
            // Filter 时忽略虚拟过期 tag（它独立渲染）
            let skill_filter_no_expired: Vec<String> = skill_filter
                .iter()
                .filter(|t| t.as_str() != EXPIRED_PACK_TAG)
                .cloned()
                .collect();
            let skill_filtered: Vec<(String, bool, &'static str, Vec<SkillListItem>)> = skill_view
                .into_iter()
                .filter(|(label, _, _, _)| {
                    if skill_filter_no_expired.is_empty() {
                        return true;
                    }
                    if let Some(tag) = label.strip_prefix("📦 ") {
                        skill_filter_no_expired.iter().any(|f| f == tag)
                    } else {
                        false
                    }
                })
                .filter(|(_, _, _, ss)| !ss.is_empty())
                .collect();

            // 选中已安装工具/技能后，滚动定位到对应卡片（高亮 class 已在卡片外层 div 设置）
            use_effect(move || {
                if let Some(id) = highlight_tool_id.read().clone() {
                    let _ = web_sys::window()
                        .and_then(|w| w.document())
                        .and_then(|d| d.get_element_by_id(&format!("tool-card-{}", id)))
                        .inspect(|el| el.scroll_into_view());
                }
            });
            use_effect(move || {
                if let Some(id) = highlight_skill_id.read().clone() {
                    let _ = web_sys::window()
                        .and_then(|w| w.document())
                        .and_then(|d| d.get_element_by_id(&format!("skill-card-{}", id)))
                        .inspect(|el| el.scroll_into_view());
                }
            });

            rsx! {
                HudPanel {
                    signal: true,
                    eyebrow: Some("AGENT".to_string()),
                    title: Some(a.name.clone()),
                    actions: Some(rsx!{
                        button {
                            class: "btn hud-btn btn-ghost btn-sm",
                            onclick: move |_| {
                                let aid = agent_id_signal();
                                spawn(async move {
                                    match sync_agent_packs(&aid).await {
                                        Ok(resp) => {
                                            // 组装变更摘要（仅列出本次实际发生变更的包）
                                            let mut parts: Vec<String> = Vec::new();
                                            if !resp.installed_tool_tags.is_empty() {
                                                parts.push(format!("补装工具包 {}", resp.installed_tool_tags.join("、")));
                                            }
                                            if !resp.installed_skill_packs.is_empty() {
                                                parts.push(format!("补装技能包 {}", resp.installed_skill_packs.join("、")));
                                            }
                                            if !resp.refreshed_skill_packs.is_empty() {
                                                parts.push(format!("补全技能包 {}", resp.refreshed_skill_packs.join("、")));
                                            }
                                            if parts.is_empty() {
                                                toast.success("基础包与技能包均已是最新，无需变更");
                                            } else {
                                                toast.success(format!("同步完成: {}", parts.join("; ")));
                                            }
                                            // 刷新工具包 / 技能包列表与 Agent 全景
                                            match list_installed_tool_packs(&aid).await {
                                                Ok(r) => tool_packs.set(r.installed_tags),
                                                Err(e) => toast.error(format!("刷新工具包列表失败: {}", e)),
                                            }
                                            match list_installed_skill_packs(&aid).await {
                                                Ok(r) => skill_packs.set(r.skill_packs),
                                                Err(e) => toast.error(format!("刷新技能包列表失败: {}", e)),
                                            }
                                            match get_agent(build_agent_stats_request(aid.clone())).await {
                                                Ok(a) => agent_res.set(Some(Ok(a))),
                                                Err(e) => toast.error(format!("刷新 Agent 失败: {}", e)),
                                            }
                                        }
                                        Err(e) => toast.error(format!("同步包失败: {}", e)),
                                    }
                                });
                            },
                            "🔄 同步包"
                        }
                        button {
                            class: "btn hud-btn btn-ghost btn-sm",
                            onclick: move |_| {
                                if let Some(a) = agent_res.read().as_ref().and_then(|r| r.as_ref().ok()) {
                                    edit_name.set(a.name.clone());
                                    edit_roles.set(a.roles.clone());
                                    edit_roles_input.set(String::new());
                                    edit_description.set(a.description.clone().unwrap_or_default());
                                    edit_capabilities.set(a.capabilities.clone().unwrap_or_default());
                                    edit_capabilities_input.set(String::new());
                                    edit_soul.set(a.soul.clone().unwrap_or_default());
                                    edit_model_provider_id.set(a.model_provider_id.clone());
                                    // 加载运行时配置现有值（缺失时回退 0）
                                    let rc = a.runtime_config.as_ref();
                                    edit_max_thinking_rounds.set(
                                        rc.map(|r| r.max_thinking_rounds.to_string()).unwrap_or_else(|| "0".to_string()),
                                    );
                                    edit_intent_analyze_max_rounds.set(
                                        rc.map(|r| r.intent_analyze_max_rounds.to_string()).unwrap_or_else(|| "0".to_string()),
                                    );
                                    edit_summary_max_rounds.set(
                                        rc.map(|r| r.summary_max_rounds.to_string()).unwrap_or_else(|| "0".to_string()),
                                    );
                                    edit_think_timeout_secs.set(
                                        rc.map(|r| r.think_timeout_secs.to_string()).unwrap_or_else(|| "0".to_string()),
                                    );
                                    show_edit_modal.set(true);
                                }
                            },
                            "✏️ 编辑"
                        }
                    }),
                    div { class: "card-body",
                        // 详情介绍
                        if !desc.is_empty() {
                            div { class: "text-base-content/70 mb-3",
                                MarkdownRenderer { content: desc.to_string(), compact: true }
                            }
                        }

                        // Tab 导航（统一 HUD 切换器风格：hud-btn 切换，不再用 DaisyUI boxed tabs）
                        div { class: "flex flex-wrap gap-2 mb-6",
                            button {
                                class: "{tab0_class}",
                                onclick: move |_| active_tab.set(0),
                                "📋 概览"
                            }
                            button {
                                class: "{tab1_class}",
                                onclick: move |_| active_tab.set(1),
                                "🔧 工具"
                            }
                            button {
                                class: "{tab2_class}",
                                onclick: move |_| active_tab.set(2),
                                "🎯 技能"
                            }
                            button {
                                class: "{tab3_class}",
                                onclick: move |_| active_tab.set(3),
                                "💬 对话与记忆"
                            }
                            button {
                                class: "{tab4_class}",
                                onclick: move |_| active_tab.set(4),
                                "🕸️ 关系图"
                            }
                            button {
                                class: "{tab5_class}",
                                onclick: move |_| active_tab.set(5),
                                "🧠 知识图谱"
                            }
                            button {
                                class: "{tab6_class}",
                                onclick: move |_| active_tab.set(6),
                                "⚡ 运行时"
                            }
                        }

                        // Tab 内容
                        {match active_tab() {
                            0 => rsx! {
                                // === 概览：基本信息 + 核心能力 + 运行时配置 + 运行时参数 + 状态切换 ===
                                // （Agent 统计与趋势图已归入「⚡ 运行时」tab）
                                div { class: "mb-6",
                                    h3 { class: "text-lg font-semibold mb-3", "基本信息" }
                                    div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                                        div {
                                            span { class: "block text-sm text-base-content/70 mb-1", "ID" }
                                            span { class: "font-mono text-sm", "{a.id}" }
                                        }
                                        div {
                                            span { class: "block text-sm text-base-content/70 mb-1", "类型" }
                                            span { class: "{kind_badge_class(&a.kind)}",
                                                "{kind_label(&a.kind)}"
                                            }
                                        }
                                        div {
                                            span { class: "block text-sm text-base-content/70 mb-1", "状态" }
                                            span { class: "{binding_status_badge_class()}",
                                                "{agent_status_label(a.status)}"
                                            }
                                        }
                                        if a.kind == "local" {
                                            div {
                                                span { class: "block text-sm text-base-content/70 mb-1", "模型提供商" }
                                                span { class: "font-mono text-sm", "{a.model_provider_id}" }
                                            }
                                        }
                                        div {
                                            span { class: "block text-sm text-base-content/70 mb-1", "创建时间" }
                                            span { class: "text-sm", "{format_time(a.created_at)}" }
                                        }
                                    }
                                }

                                div { class: "mb-6",
                                    h3 { class: "text-lg font-semibold mb-3", "核心能力" }
                                    if !capabilities.is_empty() {
                                        div { class: "flex flex-wrap gap-2",
                                            for cap in capabilities.iter() {
                                                span { class: "badge orz-tag badge-sm", "{cap}" }
                                            }
                                        }
                                    } else {
                                        div { class: "text-sm text-base-content/70", "暂无核心能力" }
                                    }
                                }

                                // 灵魂设定（角色/性格 prompt，Markdown 渲染）
                                if let Some(soul) = a.soul.as_deref().filter(|s| !s.is_empty()) {
                                    div { class: "mb-6",
                                        h3 { class: "text-lg font-semibold mb-3", "灵魂设定" }
                                        MarkdownRenderer { content: soul.to_string() }
                                    }
                                }

                                if a.kind != "local" {
                                    if let Some(ext_cfg) = &a.external_config {
                                        div { class: "mb-6",
                                            h3 { class: "text-lg font-semibold mb-3", "运行时配置" }
                                            if let Some(cli_cfg) = &ext_cfg.cli {
                                                div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                                                    div {
                                                        span { class: "block text-sm text-base-content/70 mb-1", "启动命令" }
                                                        span { class: "font-mono text-sm", "{cli_cfg.command}" }
                                                    }
                                                    if !cli_cfg.args.is_empty() {
                                                        div { class: "md:col-span-2",
                                                            span { class: "block text-sm text-base-content/70 mb-1", "命令参数" }
                                                            span { class: "font-mono text-sm",
                                                                "{cli_cfg.args.join(\" \")}"
                                                            }
                                                        }
                                                    }
                                                    div { class: "md:col-span-2",
                                                        span { class: "block text-sm text-base-content/70 mb-1", "工作目录" }
                                                        span { class: "font-mono text-sm", "{cli_cfg.work_dir}" }
                                                    }
                                                    div {
                                                        span { class: "block text-sm text-base-content/70 mb-1", "超时时间" }
                                                        span { class: "text-sm", "{cli_cfg.timeout_secs} 秒" }
                                                    }
                                                    if let Some(template) = &cli_cfg.prompt_template {
                                                        div { class: "md:col-span-2",
                                                            span { class: "block text-sm text-base-content/70 mb-1", "Prompt 模板" }
                                                            div { class: "p-3 bg-base-200 rounded-lg font-mono text-sm", "{template}" }
                                                        }
                                                    }
                                                }
                                            }
                                            if let Some(remote_cfg) = &ext_cfg.remote {
                                                div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                                                    div { class: "md:col-span-2",
                                                        span { class: "block text-sm text-base-content/70 mb-1", "A2A Server" }
                                                        span { class: "font-mono text-sm", "{remote_cfg.endpoint}" }
                                                    }
                                                    div {
                                                        span { class: "block text-sm text-base-content/70 mb-1", "目标 Agent" }
                                                        span { class: "font-mono text-sm", "{remote_cfg.agent_name}" }
                                                    }
                                                    div {
                                                        span { class: "block text-sm text-base-content/70 mb-1", "超时时间" }
                                                        span { class: "text-sm", "{remote_cfg.timeout_secs} 秒" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // 运行时参数（思考轮次 / 超时，只读展示）
                                if let Some(rc) = &a.runtime_config {
                                    div { class: "mb-6",
                                        h3 { class: "text-lg font-semibold mb-3", "运行时参数" }
                                        div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4",
                                            div {
                                                span { class: "block text-sm text-base-content/70 mb-1", "最大思考轮次" }
                                                span { class: "text-sm",
                                                    if rc.max_thinking_rounds == 0 {
                                                        span { class: "text-base-content/60", "0（系统默认）" }
                                                    } else {
                                                        "{rc.max_thinking_rounds}"
                                                    }
                                                }
                                            }
                                            div {
                                                span { class: "block text-sm text-base-content/70 mb-1", "意图识别轮次" }
                                                span { class: "text-sm",
                                                    if rc.intent_analyze_max_rounds == 0 {
                                                        span { class: "text-base-content/60", "0（系统默认）" }
                                                    } else {
                                                        "{rc.intent_analyze_max_rounds}"
                                                    }
                                                }
                                            }
                                            div {
                                                span { class: "block text-sm text-base-content/70 mb-1", "总结轮次" }
                                                span { class: "text-sm",
                                                    if rc.summary_max_rounds == 0 {
                                                        span { class: "text-base-content/60", "0（系统默认）" }
                                                    } else {
                                                        "{rc.summary_max_rounds}"
                                                    }
                                                }
                                            }
                                            div {
                                                span { class: "block text-sm text-base-content/70 mb-1", "思考超时" }
                                                span { class: "text-sm",
                                                    if rc.think_timeout_secs == 0 {
                                                        span { class: "text-base-content/60", "0（不限制）" }
                                                    } else {
                                                        "{rc.think_timeout_secs} 秒"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                div { class: "mb-6",
                                    h3 { class: "text-lg font-semibold mb-3", "状态切换" }
                                    // 一键入职：仅对面试中/待入职的 Agent 显示，
                                    // 后端 transition_status 白名单为 Interviewing → PendingOnboard → Onboarded，
                                    // 因此需按当前状态走两步（面试中先转待入职，再转已入职）
                                    if a.status == AgentStatus::Interviewing as i32 || a.status == AgentStatus::PendingOnboard as i32 {
                                        div { class: "flex flex-wrap items-center gap-3 mb-3 p-3 rounded-lg bg-base-200",
                                            button {
                                                class: "btn hud-btn btn-success btn-sm",
                                                onclick: move |_| {
                                                    let aid = agent_id_signal();
                                                    spawn(async move {
                                                        if let Err(e) = update_agent_status(UpdateAgentStatusRequest { id: aid.clone(), status: AgentStatus::PendingOnboard }).await {
                                                            toast.error(format!("转入待入职失败: {}", e));
                                                            return;
                                                        }
                                                        match update_agent_status(UpdateAgentStatusRequest { id: aid.clone(), status: AgentStatus::Onboarded }).await {
                                                            Ok(_) => {
                                                                toast.success("Agent 已正式入职");
                                                                match get_agent(build_agent_stats_request(aid.clone())).await {
                                                                    Ok(a) => agent_res.set(Some(Ok(a))),
                                                                    Err(e) => toast.error(format!("刷新 Agent 失败: {}", e)),
                                                                }
                                                            }
                                                            Err(e) => toast.error(format!("入职失败: {}", e)),
                                                        }
                                                    });
                                                },
                                                "🚀 一键入职"
                                            }
                                            span { class: "text-sm text-base-content/60",
                                                "入职后 Agent 将正式对外提供服务，并自动安装项目管理工具包"
                                            }
                                        }
                                    }
                                    div { class: "flex flex-wrap gap-2",
                                        for (status, label) in STATUS_OPTIONS {
                                            {
                                                let is_current = a.status == *status;
                                                let btn_class = if is_current { "btn hud-btn btn-primary btn-sm" } else { "btn hud-btn btn-ghost btn-sm" };
                                                let target_status_val = *status;
                                                let aid = agent_id_signal();
                                                let label_str = label.to_string();
                                                let label_for_closure = label_str.clone();
                                                rsx! {
                                                    button {
                                                        class: "{btn_class}",
                                                        disabled: is_current,
                                                        onclick: move |_| {
                                                            let agent_id = aid.clone();
                                                            let label_clone = label_for_closure.clone();
                                                            spawn(async move {
                                                                let status_req = UpdateAgentStatusRequest {
                                                                    id: agent_id.clone(),
                                                                    status: AgentStatus::from_i32(target_status_val),
                                                                };
                                                                match update_agent_status(status_req).await {
                                                                    Ok(_) => {
                                                                        toast.success(format!("状态已更新为：{}", label_clone));
                                                                        match get_agent(build_agent_stats_request(agent_id.clone())).await {
                                                                            Ok(a) => agent_res.set(Some(Ok(a))),
                                                                            Err(e) => toast.error(format!("刷新 Agent 失败: {}", e)),
                                                                        }
                                                                    }
                                                                    Err(e) => toast.error(format!("状态更新失败: {}", e)),
                                                                }
                                                            });
                                                        },
                                                        if is_current { "{label_str}（当前）" } else { "{label_str}" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                            },
                            1 => rsx! {
                                // === 工具：工具关系总览（上部）+ 工具包 + 工具绑定 ===
                                // 工具关系图作为总览置于上部，直观展示 Agent 与全量可用工具（神经/绑定/工具包）的关系
                                {
                                // 关系图按工具「真实 tag」分类着色：遍历扁平去重列表，
                                // 主分类优先已安装包 tag，否则首个真实 tag，最后回退 bound_tool。
                                // 扁平列表本身已去重，seen 集合仅作兜底。
                                let mut all_tool_nodes: Vec<RelationNodeInfo> = Vec::new();
                                let mut seen_tool_ids: std::collections::HashSet<String> =
                                    std::collections::HashSet::new();

                                for t in tool_list.iter() {
                                    if seen_tool_ids.contains(&t.id) {
                                        continue;
                                    }
                                    seen_tool_ids.insert(t.id.clone());
                                    let primary = t
                                        .tags
                                        .iter()
                                        .find(|x| tool_packs_set.contains(*x))
                                        .cloned()
                                        .or_else(|| t.tags.first().cloned())
                                        .unwrap_or_else(|| "bound_tool".to_string());
                                    let (et, ed) = tool_edge_meta(&t.runtime_ready);
                                    all_tool_nodes.push(RelationNodeInfo {
                                        id: t.id.clone(),
                                        name: t.name.clone(),
                                        kind: Some(primary),
                                        edge_tag: et,
                                        edge_description: ed,
                                    });
                                }
                                let navigator = use_navigator();
                                rsx! {
                                    div { class: "mb-6",
                                        h3 { class: "text-lg font-semibold mb-3", "工具关系总览（按 tag 分类）" }
                                        RelationGraph {
                                            center_id: a.id.clone(),
                                            center_name: a.name.clone(),
                                            center_color: "#fa520f".to_string(),
                                            center_kind: Some("agent".to_string()),
                                            related: all_tool_nodes,
                                            related_color: "#f59e0b".to_string(),
                                            related_label: "工具".to_string(),
                                            color_by_kind: true,
                                            on_node_click: Some(EventHandler::new(move |evt: crate::components::relation_graph::NodeClickEvent| {
                                                if evt.is_center {
                                                    return;
                                                }
                                                // 关系图中关联节点均为工具，按 id 跳转工具详情
                                                navigator.push(format!("/finance/tools/{}", evt.id));
                                            })),
                                        }
                                    }
                                }
                            }
                                div { class: "mb-6",
                                    h3 { class: "text-lg font-semibold mb-3", "工具包" }
                                    div { class: "flex gap-2 items-center mb-4",
                                        div { class: "flex-1",
                                            SearchableSelect {
                                                placeholder: "搜索工具包 tag...".to_string(),
                                                selected: None,
                                                options: tool_tags.read().clone(),
                                                on_select: move |tag: String| {
                                                    let aid = agent_id_signal();
                                                    spawn(async move {
                                                        match install_tool_pack(InstallToolPackRequest {
                                                            agent_id: aid.clone(),
                                                            tag: tag.clone(),
                                                        }).await {
                                                            Ok(_) => {
                                                                toast.success(format!("工具包 [{}] 已安装", tag));
                                                                match list_installed_tool_packs(&aid).await {
                                                                    Ok(resp) => tool_packs.set(resp.installed_tags),
                                                                    Err(e) => toast.error(format!("刷新工具包列表失败: {}", e)),
                                                                }
                                                            }
                                                            Err(e) => toast.error(format!("安装工具包失败: {}", e)),
                                                        }
                                                    });
                                                },
                                                on_search: None,
                                            }
                                        }
                                    }
                                    // 包即筛选器：点击卡片切换下方列表的 tag 过滤（再次点击取消）；「卸载」按钮触发二次确认弹窗
                                    if tool_packs_list.is_empty() {
                                        p { class: "text-sm text-base-content/50", "暂无工具包，安装后会出现在这里并可点击筛选" }
                                    } else {
                                        div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                                            for tag in tool_packs_list.iter() {
                                                {
                                                    let tag_clone = tag.clone();
                                                    let tf = tool_filter_tags;
                                                    let dlg = show_tool_pack_uninstall_dialog;
                                                    let active = tool_filter_tags.read().contains(&tag_clone);
                                                    let count = tool_pack_counts.get(&tag_clone).copied().unwrap_or(0);
                                                    let subtitle = format!("包含 {} 个工具 · 点击筛选", count);
                                                    rsx! {
                                                        PackCard {
                                                            pack_tag: tag_clone.clone(),
                                                            subtitle: subtitle.clone(),
                                                            selected: active,
                                                            on_toggle: {
                                                                let mut tfc = tf;
                                                                move |t: String| {
                                                                    let mut v = tfc.write();
                                                                    if let Some(pos) = v.iter().position(|x| x == &t) {
                                                                        v.remove(pos);
                                                                    } else {
                                                                        v.push(t);
                                                                    }
                                                                }
                                                            },
                                                            on_uninstall: {
                                                                let mut dlg = dlg;
                                                                move |t: String| { dlg.set(Some(t)); }
                                                            },
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                div { class: "mb-6",
                                    h3 { class: "text-lg font-semibold mb-3", "工具绑定" }

                                    // 搜索框（动态搜索模式：on_search 回调调用 query_tools）
                                    div { class: "mb-4",
                                        SearchableSelect {
                                            placeholder: "搜索并绑定工具...".to_string(),
                                            selected: None,
                                            options: tool_search_results.read().iter().map(|t| {
                                                format!("{} ({})", t.name, t.id)
                                            }).collect(),
                                            on_select: {
                                                let installed = installed_tool_ids.clone();
                                                move |selection: String| {
                                                    // 从 "name (id)" 格式中提取 name 与 id
                                                    if let Some(id_start) = selection.rfind('(') {
                                                        let tool_id = selection[id_start+1..selection.len()-1].to_string();
                                                        let tool_name = selection[..id_start].trim().to_string();
                                                        if installed.contains(&tool_id) {
                                                            // 已安装：在已安装列表中高亮定位，不再弹安装确认
                                                            highlight_tool_id.set(Some(tool_id.clone()));
                                                            toast.success(format!("工具已安装：{}", tool_name));
                                                        } else {
                                                            show_bind_tool_dialog.set(Some((tool_id, tool_name)));
                                                        }
                                                    }
                                                }
                                            },
                                            on_search: Some(EventHandler::new(move |keyword: String| {
                                                spawn(async move {
                                                    if keyword.trim().is_empty() {
                                                        tool_search_results.set(Vec::new());
                                                        return;
                                                    }
                                                    tool_search_loading.set(true);
                                                    let req = ToolQueryRequest {
                                                        keyword: Some(keyword),
                                                        enabled_only: Some(true),
                                                        ..Default::default()
                                                    };
                                                    match query_tools(&req).await {
                                                        Ok(resp) => tool_search_results.set(resp.items),
                                                        Err(_) => tool_search_results.set(Vec::new()),
                                                    }
                                                    tool_search_loading.set(false);
                                                });
                                            })),
                                            loading: *tool_search_loading.read(),
                                        }
                                    }

                                    // ===== Agent 工具全景：按 tag 维度聚合 + 包筛选 =====
                                    // 包（工具包 tag）作为上方筛选器；下方聚合列表按包分组展示，
                                    // 可被包筛选 + 名称搜索过滤。卡片沿用现有 ToolCard 样式与各组色调/徽章。
                                    // 过滤条：已选包（tag）筛选条件（点击上方工具包 chip 即追加）；无筛选时整条隐藏
                                    if !tool_filter_tags.read().is_empty() {
                                        div { class: "flex flex-wrap items-center gap-2 mb-4",
                                            for ft in tool_filter_tags.read().iter() {
                                                span {
                                                    class: "badge orz-tag badge-primary gap-1",
                                                    "🔍 {ft}"
                                                    button {
                                                        class: "badge-remove",
                                                        onclick: {
                                                            let t = ft.clone();
                                                            let mut tf = tool_filter_tags;
                                                            move |_| { tf.write().retain(|x| x != &t); }
                                                        },
                                                        "×"
                                                    }
                                                }
                                            }
                                            button {
                                                class: "btn hud-btn btn-ghost btn-xs",
                                                onclick: move |_| tool_filter_tags.set(Vec::new()),
                                                "清除筛选"
                                            }
                                        }
                                    }
                                    if all_tool_count == 0 {
                                        div { class: "text-center py-12",
                                            div { class: "text-5xl mb-4 opacity-30", "🔧" }
                                            div { class: "text-base-content/70", "暂无可用工具" }
                                        }
                                    } else if tool_filtered.is_empty() {
                                        div { class: "text-center py-12",
                                            div { class: "text-5xl mb-4 opacity-30", "🔍" }
                                            div { class: "text-base-content/70", "没有匹配当前筛选条件的工具" }
                                        }
                                    } else {
                                        for (label, tone, badge, unbind, tools) in tool_filtered.iter() {
                                            div { class: "mb-4",
                                                div { class: "flex items-center gap-2 mb-2",
                                                    h4 { class: "font-semibold text-base", "{label}" }
                                                    span { class: "badge orz-tag badge-xs", "{tools.len()}" }
                                                }
                                                div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                                                    for tool in tools.iter() {
                                                        div {
                                                            id: "tool-card-{tool.id}",
                                                            class: if highlight_tool_id.read().as_deref() == Some(tool.id.as_str()) {
                                                                "rounded-lg ring-2 ring-primary"
                                                            } else {
                                                                ""
                                                            },
                                                            ToolCard {
                                                                key: "flt-{tool.id}",
                                                                tool: tool.clone(),
                                                                tone: *tone,
                                                                badge: badge.clone(),
                                                                badge_class: "badge orz-tag badge-xs",
                                                                show_unbind: *unbind,
                                                                on_unbind: on_unbind_tool,
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            },
                            2 => rsx! {
                                // === 技能：技能包 + 单个技能安装 ===
                                div { class: "mb-6",
                                    h3 { class: "text-lg font-semibold mb-3", "技能包" }
                                    div { class: "flex gap-2 items-center mb-4",
                                        div { class: "flex-1",
                                            SearchableSelect {
                                                placeholder: "搜索技能包 tag...".to_string(),
                                                selected: None,
                                                options: skill_tags.read().clone(),
                                                on_select: move |tag: String| {
                                                    let aid = agent_id_signal();
                                                    spawn(async move {
                                                        match install_skill_pack(InstallSkillPackRequest { agent_id: aid.clone(), tag: tag.clone() }).await {
                                                            Ok(_) => {
                                                                toast.success(format!("技能包 [{}] 已安装", tag));
                                                                match list_installed_skill_packs(&aid).await {
                                                                    Ok(resp) => skill_packs.set(resp.skill_packs),
                                                                    Err(e) => toast.error(format!("刷新技能包列表失败: {}", e)),
                                                                }
                                                            }
                                                            Err(e) => toast.error(format!("安装技能包失败: {}", e)),
                                                        }
                                                    });
                                                },
                                                on_search: None,
                                            }
                                        }
                                    }
                                    // 包即筛选器：点击卡片切换下方列表的 tag 过滤（再次点击取消）；「卸载」按钮触发二次确认弹窗。
                                    // 📦 已过期技能虚拟 pack 放在**真实 packs 之后（grid 末尾）**；
                                    // 具体过期技能卡片区域放**技能列表最前端**（用户点中后第一时间看到恢复按钮）。
                                    {
                                        let packs = skill_packs_list.clone();
                                        let tf = skill_filter_tags;
                                        let expired_active = tf.read().iter().any(|t| t == EXPIRED_PACK_TAG);
                                        let exp_count = expired_skill_list.read().len();
                                        let title = EXPIRED_PACK_LABEL.to_string();
                                        let subtitle_exp = if expired_loaded() {
                                            format!("过期副本 {} 个 · 点击切换显示", exp_count)
                                        } else {
                                            "点击加载并显示过期副本".to_string()
                                        };
                                        let style = if expired_active {
                                            "border-2 border-error bg-error/10 shadow-lg shadow-error/20"
                                        } else {
                                            "border-2 border-base-300 bg-base-200/40 hover:border-error/50 hover:bg-error/5"
                                        };
                                        rsx! {
                                            div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                                                // ====== 第 1..N 格：真实已安装 packs ======
                                                for tag in packs.iter() {
                                                    {
                                                        let tag_clone = tag.clone();
                                                        let mut tf2 = skill_filter_tags;
                                                        let active = skill_filter_tags.read().contains(&tag_clone);
                                                        let mut dlg = show_skill_pack_uninstall_dialog;
                                                        let count = skill_pack_counts.get(&tag_clone).copied().unwrap_or(0);
                                                        let subtitle_pack = format!("包含 {} 个技能 · 点击筛选", count);
                                                        rsx! {
                                                            PackCard {
                                                                pack_tag: tag_clone.clone(),
                                                                subtitle: subtitle_pack,
                                                                selected: active,
                                                                on_toggle: {
                                                                    move |t: String| {
                                                                        let mut v = tf2.write();
                                                                        if let Some(pos) = v.iter().position(|x| x == &t) {
                                                                            v.remove(pos);
                                                                        } else {
                                                                            v.push(t);
                                                                        }
                                                                    }
                                                                },
                                                                on_uninstall: {
                                                                    move |t: String| { dlg.set(Some(t)); }
                                                                },
                                                            }
                                                        }
                                                    }
                                                }
                                                // ====== 末尾一格：虚拟过期技能 pack ======
                                                div {
                                                    class: "card {style} cursor-pointer",
                                                    onclick: move |_| on_toggle_expired_pack(),
                                                    div {
                                                        class: "card-body py-3 px-4",
                                                        div {
                                                            class: "flex items-center justify-between w-full",
                                                            h3 {
                                                                class: "card-title text-sm font-semibold m-0 flex items-center gap-2",
                                                                span { class: "badge badge-error badge-sm", "Expired" }
                                                                "{title}"
                                                            }
                                                            div {
                                                                class: "text-xs text-base-content/60",
                                                                "{subtitle_exp}"
                                                            }
                                                        }
                                                        p {
                                                            class: "text-xs text-base-content/50 mt-1",
                                                            "升级时被替换下来的旧副本存放在这里，可选择性恢复为 Draft 重新使用。"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // === 单个技能安装 ===
                                div { class: "mb-6",
                                    h3 { class: "text-lg font-semibold mb-3", "单个技能安装" }

                                    // 搜索框（动态搜索模式：on_search 回调调用 query_skills）
                                    div { class: "mb-4",
                                        SearchableSelect {
                                            placeholder: "搜索并安装技能...".to_string(),
                                            selected: None,
                                            options: skill_search_results.read().iter().map(|s| {
                                                format!("{} ({})", s.name, s.id)
                                            }).collect(),
                                            on_select: {
                                                let installed = installed_skill_ids.clone();
                                                move |selection: String| {
                                                    // 从 "name (id)" 格式中提取 name 与 id
                                                    if let Some(id_start) = selection.rfind('(') {
                                                        let skill_id = selection[id_start+1..selection.len()-1].to_string();
                                                        let skill_name = selection[..id_start].trim().to_string();
                                                        if installed.contains(&skill_id) {
                                                            highlight_skill_id.set(Some(skill_id.clone()));
                                                            toast.success(format!("技能已安装：{}", skill_name));
                                                        } else {
                                                            show_install_skill_dialog.set(Some((skill_id, skill_name)));
                                                        }
                                                    }
                                                }
                                            },
                                            on_search: Some(EventHandler::new(move |keyword: String| {
                                                spawn(async move {
                                                    if keyword.trim().is_empty() {
                                                        skill_search_results.set(Vec::new());
                                                        return;
                                                    }
                                                    skill_search_loading.set(true);
                                                    let req = SkillQueryRequest {
                                                        keyword: Some(keyword),
                                                        ..Default::default()
                                                    };
                                                    match query_skills(&req).await {
                                                        Ok(resp) => skill_search_results.set(resp.items),
                                                        Err(_) => skill_search_results.set(Vec::new()),
                                                    }
                                                    skill_search_loading.set(false);
                                                });
                                            })),
                                            loading: *skill_search_loading.read(),
                                        }
                                    }

                                    // ===== Agent 已安装技能全景：按 tag 维度聚合 + 包筛选 =====
                                    // 包（技能包 tag）作为上方筛选器；下方聚合列表按包分组展示，
                                    // 可被包筛选 + 名称搜索过滤。卡片沿用现有 SkillCard 样式与 neural 徽章。
                                    // 过滤条：已选包（tag）筛选条件（点击上方技能包 chip 即追加）；无筛选时整条隐藏
                                    if !skill_filter_tags.read().is_empty() {
                                        div { class: "flex flex-wrap items-center gap-2 mb-4",
                                            for ft in skill_filter_tags.read().iter() {
                                                span {
                                                    class: "badge orz-tag badge-primary gap-1",
                                                    "🔍 {ft}"
                                                    button {
                                                        class: "badge-remove",
                                                        onclick: {
                                                            let t = ft.clone();
                                                            let mut tf = skill_filter_tags;
                                                            move |_| { tf.write().retain(|x| x != &t); }
                                                        },
                                                        "×"
                                                    }
                                                }
                                            }
                                            button {
                                                class: "btn hud-btn btn-ghost btn-xs",
                                                onclick: move |_| skill_filter_tags.set(Vec::new()),
                                                "清除筛选"
                                            }
                                        }
                                    }
                                    if all_skill_count == 0 {
                                        div { class: "text-center py-12",
                                            div { class: "text-5xl mb-4 opacity-30", "🧩" }
                                            div { class: "text-base-content/70", "暂无已安装技能" }
                                        }
                                    } else if skill_filtered.is_empty() {
                                        div { class: "text-center py-12",
                                            div { class: "text-5xl mb-4 opacity-30", "🔍" }
                                            div { class: "text-base-content/70", "没有匹配当前筛选条件的技能" }
                                        }
                                    } else {
                                        // --- 📦 已过期技能 · 独立分组（选中虚拟 pack 时渲染）---
                                        // 放在**技能列表最前端**：用户一点中虚拟 pack，立刻就能在最顶部看到
                                        // 过期副本卡片和恢复按钮，不用滚到底下找。
                                        {
                                            let expired = expired_skill_list.read().clone();
                                            let expired_empty_label = if expired_loaded() {
                                                "没有过期副本（或已有副本已被恢复）".to_string()
                                            } else {
                                                "正在从后端拉取过期副本……".to_string()
                                            };
                                            let empty_icon = if expired_loaded() { "✓" } else { "⏳" };
                                            let exp_count = expired.len();
                                            if filter_has_expired {
                                                rsx! {
                                                    div {
                                                        class: "mb-6 rounded-xl border-2 border-dashed border-error/40 p-4 bg-error/[0.04]",
                                                        div {
                                                            class: "flex items-center gap-2 mb-3",
                                                            span { class: "badge badge-error badge-sm", "Expired" }
                                                            h4 { class: "font-semibold text-base", "📦 已过期技能（点击虚拟 pack 加载）" }
                                                            span { class: "badge orz-tag badge-xs", "{exp_count}" }
                                                        }
                                                        if expired.is_empty() {
                                                            div { class: "text-center py-8",
                                                                div { class: "text-3xl mb-2 opacity-40", "{empty_icon}" }
                                                                div { class: "text-base-content/60 text-sm", "{expired_empty_label}" }
                                                            }
                                                        } else {
                                                            div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                                                                for skill in expired.iter() {
                                                                    div {
                                                                        id: "skill-card-expired-{skill.id}",
                                                                        SkillCard {
                                                                            key: "exp-{skill.id}",
                                                                            skill: skill.clone(),
                                                                            tone: "warning",
                                                                            neural_badge: false,
                                                                            on_uninstall: on_uninstall_skill,
                                                                            on_restore: Some(EventHandler::<String>::new(move |s: String| on_restore_skill(s))),
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            } else {
                                                rsx! { {} }
                                            }
                                        }
                                        // --- 正常活动技能分组 ---
                                        for (label, neural_badge, tone, skills) in skill_filtered.iter() {
                                            div { class: "mb-4",
                                                div { class: "flex items-center gap-2 mb-2",
                                                    h4 { class: "font-semibold text-base", "{label}" }
                                                    span { class: "badge orz-tag badge-xs", "{skills.len()}" }
                                                }
                                                div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                                                    for skill in skills.iter() {
                                                        div {
                                                            id: "skill-card-{skill.id}",
                                                            class: if highlight_skill_id.read().as_deref() == Some(skill.id.as_str()) {
                                                                "rounded-lg ring-2 ring-primary"
                                                            } else {
                                                                ""
                                                            },
                                                            SkillCard {
                                                                key: "flt-{skill.id}",
                                                                skill: skill.clone(),
                                                                tone: *tone,
                                                                neural_badge: *neural_badge,
                                                                on_uninstall: on_uninstall_skill,
                                                                on_restore: None,
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            },
                            3 => rsx! {
                                // === 对话与记忆（左右分栏）===
                                // 原先是纵向堆叠：消息流一长就把记忆面板挤到屏幕外，
                                // 两者还会互相抢占滚动。改为左右双栏、各自独立滚动，
                                // 互不干扰。窄屏（< lg）回退为上下堆叠。
                                div { class: "grid grid-cols-1 lg:grid-cols-[minmax(0,3fr)_minmax(0,2fr)] gap-4 items-start",

                                    // ---- 左栏：消息流 ----
                                    div { class: "flex flex-col min-h-0 lg:h-[600px]",
                                        div { class: "flex items-center justify-between mb-2",
                                            h3 { class: "text-lg font-semibold", "对话" }
                                            button {
                                                class: "btn hud-btn btn-ghost btn-xs",
                                                title: "拉取最新消息",
                                                onclick: move |_| poll_new(),
                                                "刷新"
                                            }
                                        }
                                        div { class: "agent-chat-messages flex-1 min-h-0 max-h-[65vh] lg:max-h-none",
                                            // 上拉翻页入口（置于顶部：列表按时间正序，早消息在上）
                                            if msg_has_more() {
                                                div { class: "flex justify-center pb-1",
                                                    button {
                                                        class: "btn hud-btn btn-ghost btn-xs",
                                                        disabled: msg_loading_more(),
                                                        onclick: load_more,
                                                        if msg_loading_more() { "加载中…" } else { "加载更早消息" }
                                                    }
                                                }
                                            } else if !messages().is_empty() {
                                                div { class: "text-center text-xs text-base-content/40 pb-1", "已到最早消息" }
                                            }

                                            if msg_loading() {
                                                Loading { size: "sm" }
                                            } else if messages().is_empty() && !is_typing() {
                                                div { class: "text-center py-12",
                                                    div { class: "text-5xl mb-4 opacity-30", "💬" }
                                                    div { class: "text-base-content/70", "暂无对话记录，发送消息开始对话" }
                                                }
                                            } else {
                                                for msg in messages().iter().cloned() {
                                                    MessageBubble { msg: msg.clone(), key: "{msg.message_id}" }
                                                }
                                                if is_typing() {
                                                    TypingIndicator {}
                                                }
                                            }
                                        }
                                        div { class: "flex gap-2 mt-3",
                                            input {
                                                class: "input input-bordered flex-1",
                                                r#type: "text",
                                                placeholder: "输入消息...",
                                                value: input_message,
                                                oninput: move |e| input_message.set(e.value().clone()),
                                                onkeydown: move |e| {
                                                    if e.key() == Key::Enter {
                                                        e.prevent_default();
                                                        handle_send(());
                                                    }
                                                },
                                            }
                                            button {
                                                class: "btn hud-btn btn-primary",
                                                onclick: move |_| handle_send(()),
                                                "发送"
                                            }
                                        }
                                    }

                                    // ---- 右栏：记忆 ----
                                    div { class: "flex flex-col min-h-0 lg:h-[600px]",
                                        h3 { class: "text-lg font-semibold mb-2", "记忆" }
                                        div { class: "flex-1 min-h-0 overflow-y-auto pr-1",
                                            AgentMemoryPanel { agent_id: Some(id.clone()) }
                                        }
                                    }
                                }
                            },
                            4 => rsx! {
                                HudPanel {
                                    title: "关系图".to_string(),
                                    eyebrow: "GRAPH".to_string(),
                                    div { class: "p-4",
                                        div { class: "w-full h-[520px]",
                                            WorkspaceGraph {
                                                view: WorkspaceView::AgentDetail(a.id.clone()),
                                                projects: graph_projects.read().clone(),
                                                agents: graph_agents.read().clone(),
                                                tasks: graph_tasks.read().clone(),
                                                width: 800.0,
                                                height: 500.0,
                                                auto_size: true,
                                            }
                                        }
                                    }
                                }
                            },
                            5 => rsx! {
                                KnowledgeGraph { agent_id: Some(id.clone()) }
                            },
                            6 => rsx! {
                                // === 运行时：统计概览（Agent 统计 + 模型调用趋势）→ 实时状态 + 思考快照 + 取消按钮 ===
                                // 统计数据属运行时数据，统一放在运行时 tab 上半部分
                                if a.stats.is_some() || a.model_call_stats.is_some() {
                                    div { class: "mb-6",
                                        AgentStatsPanel {
                                            stats: a.stats.clone(),
                                            model_call_stats: a.model_call_stats.clone(),
                                        }
                                    }
                                }
                                crate::components::runtime_panel::RuntimePanel {
                                    agent_id: id.clone(),
                                }
                            },
                            _ => rsx! {},
                        }}

                        Modal {
                            title: "编辑 Agent 基本信息".to_string(),
                            show: show_edit_modal(),
                            on_close: move |_| show_edit_modal.set(false),
                            footer: rsx! {
                                button { class: "btn hud-btn btn-ghost", onclick: move |_| show_edit_modal.set(false), "取消" }
                                button {
                                    class: "btn hud-btn btn-primary",
                                    disabled: saving_meta(),
                                    onclick: {
                                        let id_for_submit = id.clone();
                                        move |_| {
                                            let name = edit_name().trim().to_string();
                                            if name.is_empty() {
                                                toast.error("名称不能为空");
                                                return;
                                            }
                                            let roles: Vec<String> = edit_roles()
                                                .into_iter()
                                                .filter(|s| !s.trim().is_empty())
                                                .map(|s| s.trim().to_string())
                                                .collect();
                                            let capabilities: Vec<String> = edit_capabilities()
                                                .into_iter()
                                                .filter(|s| !s.trim().is_empty())
                                                .map(|s| s.trim().to_string())
                                                .collect();
                                            let soul = if edit_soul().trim().is_empty() { None } else { Some(edit_soul()) };
                                            let mp_id = if edit_model_provider_id().is_empty() { None } else { Some(edit_model_provider_id()) };
                                            // 运行时配置：解析输入框，空字符串/非法值视为 0
                                            let max_thinking_rounds = edit_max_thinking_rounds()
                                                .trim()
                                                .parse::<usize>()
                                                .unwrap_or(0);
                                            let intent_analyze_max_rounds = edit_intent_analyze_max_rounds()
                                                .trim()
                                                .parse::<usize>()
                                                .unwrap_or(0);
                                            let summary_max_rounds = edit_summary_max_rounds()
                                                .trim()
                                                .parse::<usize>()
                                                .unwrap_or(0);
                                            let think_timeout_secs = edit_think_timeout_secs()
                                                .trim()
                                                .parse::<u64>()
                                                .unwrap_or(0);
                                            let req = UpdateAgentRequest {
                                                id: id_for_submit.clone(),
                                                name: Some(name),
                                                roles: Some(roles),
                                                description: Some(edit_description()),
                                                capabilities: Some(capabilities),
                                                soul,
                                                model_provider_id: mp_id,
                                                runtime_config: Some(AgentRuntimeConfigInfo {
                                                    max_thinking_rounds,
                                                    intent_analyze_max_rounds,
                                                    summary_max_rounds,
                                                    think_timeout_secs,
                                                }),
                                            };
                                            saving_meta.set(true);
                                            let id_clone = id_for_submit.clone();
                                            spawn(async move {
                                                match update_agent(req).await {
                                                    Ok(_) => {
                                                        toast.success("Agent 信息已更新");
                                                        show_edit_modal.set(false);
                                                        match get_agent(build_agent_stats_request(id_clone.clone())).await {
                                                            Ok(a) => agent_res.set(Some(Ok(a))),
                                                            Err(e) => toast.error(format!("重新加载失败: {}", e)),
                                                        }
                                                    }
                                                    Err(e) => toast.error(format!("更新失败: {}", e)),
                                                }
                                                saving_meta.set(false);
                                            });
                                        }
                                    },
                                    if saving_meta() { "保存中..." } else { "保存" }
                                }
                            },
                            div { class: "space-y-4",
                                div { class: "form-control w-full",
                                    label { class: "label", span { class: "label-text font-medium", "名称 *" } }
                                    input { class: "input input-bordered w-full", value: "{edit_name}",
                                        oninput: move |e| edit_name.set(e.value()), placeholder: "Agent 名称" }
                                }
                                div { class: "form-control w-full",
                                    label { class: "label", span { class: "label-text font-medium", "描述" } }
                                    textarea { class: "textarea textarea-bordered w-full", value: "{edit_description}",
                                        oninput: move |e| edit_description.set(e.value()) }
                                }
                                div { class: "form-control w-full",
                                    label { class: "label",
                                        span { class: "label-text font-medium", "角色（多选）" }
                                        span { class: "label-text-alt", "用于路由匹配，如前台/Web接待/代码专家 等" }
                                    }
                                    div { class: "flex flex-wrap gap-2 mb-2",
                                        {
                                            const PRESET_ROLES: &[(&str, &str)] = &[
                                                ("reception", "Web前台接待"),
                                                ("feishu_reception", "飞书前台接待"),
                                                ("a2a_gateway", "A2A网关"),
                                                ("hr_specialist", "人事专员"),
                                                ("code_assistant", "代码助手"),
                                            ];
                                            PRESET_ROLES.iter().map(|(key, label)| {
                                                let key_clone = key.to_string();
                                                let selected = edit_roles().iter().any(|r| r == key);
                                                let cls = if selected {
                                                    "btn hud-btn btn-primary btn-sm"
                                                } else {
                                                    "btn hud-btn btn-outline btn-sm"
                                                };
                                                rsx! {
                                                    button { class: cls,
                                                        onclick: move |_| {
                                                            let mut v = edit_roles();
                                                            if let Some(pos) = v.iter().position(|x| x == key_clone.as_str()) {
                                                                v.remove(pos);
                                                            } else {
                                                                v.push(key_clone.clone());
                                                            }
                                                            edit_roles.set(v);
                                                        },
                                                        "{label}"
                                                    }
                                                }
                                            })
                                        }
                                    }
                                    div { class: "flex flex-wrap gap-2 items-center",
                                        if !edit_roles().is_empty() {
                                            for role in edit_roles() {
                                                span { class: "badge orz-tag badge-lg gap-1",
                                                    "{role}",
                                                    button { class: "btn hud-btn btn-ghost btn-xs",
                                                        onclick: move |_| {
                                                            let mut v = edit_roles();
                                                            if let Some(pos) = v.iter().position(|x| x == &role) {
                                                                v.remove(pos);
                                                            }
                                                            edit_roles.set(v);
                                                        },
                                                        "✕"
                                                    }
                                                }
                                            }
                                        }
                                        input { class: "input input-bordered input-sm flex-1 min-w-[180px]",
                                            value: "{edit_roles_input}",
                                            placeholder: "自定义角色，回车/逗号添加",
                                            oninput: move |e| {
                                                let val = e.value();
                                                if let Some(comma_pos) = val.find(',') {
                                                    let (head, rest) = val.split_at(comma_pos);
                                                    let v = head.trim().to_string();
                                                    if !v.is_empty() && !edit_roles().iter().any(|r| r == v.as_str()) {
                                                        let mut arr = edit_roles();
                                                        arr.push(v);
                                                        edit_roles.set(arr);
                                                    }
                                                    edit_roles_input.set(rest[1..].trim().to_string());
                                                } else {
                                                    edit_roles_input.set(val);
                                                }
                                            },
                                            onkeydown: move |e| {
                                                if e.key() == Key::Enter {
                                                    e.prevent_default();
                                                    let v = edit_roles_input().trim().to_string();
                                                    if !v.is_empty() && !edit_roles().iter().any(|r| r == v.as_str()) {
                                                        let mut arr = edit_roles();
                                                        arr.push(v);
                                                        edit_roles.set(arr);
                                                    }
                                                    edit_roles_input.set(String::new());
                                                }
                                            }
                                        }
                                    }
                                }
                                div { class: "form-control w-full",
                                    label { class: "label",
                                        span { class: "label-text font-medium", "能力关键词（多选，用于弱匹配）" }
                                        span { class: "label-text-alt", "如：chat、code_search、task、knowledge 等" }
                                    }
                                    div { class: "flex flex-wrap gap-2 items-center",
                                        if !edit_capabilities().is_empty() {
                                            for cap in edit_capabilities() {
                                                span { class: "badge orz-tag badge-lg gap-1",
                                                    "{cap}",
                                                    button { class: "btn hud-btn btn-ghost btn-xs",
                                                        onclick: move |_| {
                                                            let mut v = edit_capabilities();
                                                            if let Some(pos) = v.iter().position(|x| x == &cap) {
                                                                v.remove(pos);
                                                            }
                                                            edit_capabilities.set(v);
                                                        },
                                                        "✕"
                                                    }
                                                }
                                            }
                                        }
                                        input { class: "input input-bordered input-sm flex-1 min-w-[180px]",
                                            value: "{edit_capabilities_input}",
                                            placeholder: "自定义能力，回车/逗号添加",
                                            oninput: move |e| {
                                                let val = e.value();
                                                if let Some(comma_pos) = val.find(',') {
                                                    let (head, rest) = val.split_at(comma_pos);
                                                    let v = head.trim().to_string();
                                                    if !v.is_empty() && !edit_capabilities().iter().any(|r| r == v.as_str()) {
                                                        let mut arr = edit_capabilities();
                                                        arr.push(v);
                                                        edit_capabilities.set(arr);
                                                    }
                                                    edit_capabilities_input.set(rest[1..].trim().to_string());
                                                } else {
                                                    edit_capabilities_input.set(val);
                                                }
                                            },
                                            onkeydown: move |e| {
                                                if e.key() == Key::Enter {
                                                    e.prevent_default();
                                                    let v = edit_capabilities_input().trim().to_string();
                                                    if !v.is_empty() && !edit_capabilities().iter().any(|r| r == v.as_str()) {
                                                        let mut arr = edit_capabilities();
                                                        arr.push(v);
                                                        edit_capabilities.set(arr);
                                                    }
                                                    edit_capabilities_input.set(String::new());
                                                }
                                            }
                                        }
                                    }
                                }
                                div { class: "form-control w-full",
                                    label { class: "label", span { class: "label-text font-medium", "灵魂提示词 (Soul)" } }
                                    textarea { class: "textarea textarea-bordered w-full", value: "{edit_soul}",
                                        oninput: move |e| edit_soul.set(e.value()), rows: "4" }
                                }
                                div { class: "form-control w-full",
                                    label { class: "label", span { class: "label-text font-medium", "模型提供商" } }
                                    select {
                                        class: "select select-bordered w-full",
                                        value: "{edit_model_provider_id}",
                                        onchange: move |e| edit_model_provider_id.set(e.value()),
                                        option { value: "", "（不绑定）" }
                                        for p in model_providers.read().iter() {
                                            option { value: "{p.id}", "{p.name}" }
                                        }
                                    }
                                }
                                // 运行时配置分区
                                div { class: "pt-2 border-t border-base-300" }
                                div { class: "form-control w-full",
                                    label { class: "label",
                                        span { class: "label-text font-medium", "最大思考轮次" }
                                        span { class: "label-text-alt text-base-content/60", "0 = 系统默认" }
                                    }
                                    input { class: "input input-bordered w-full", r#type: "number",
                                        value: "{edit_max_thinking_rounds}",
                                        oninput: move |e| edit_max_thinking_rounds.set(e.value()),
                                        placeholder: "0 = 使用系统配置" }
                                }
                                div { class: "form-control w-full",
                                    label { class: "label",
                                        span { class: "label-text font-medium", "意图识别轮次" }
                                        span { class: "label-text-alt text-base-content/60", "0 = 系统默认" }
                                    }
                                    input { class: "input input-bordered w-full", r#type: "number",
                                        value: "{edit_intent_analyze_max_rounds}",
                                        oninput: move |e| edit_intent_analyze_max_rounds.set(e.value()),
                                        placeholder: "0 = 使用系统配置" }
                                }
                                div { class: "form-control w-full",
                                    label { class: "label",
                                        span { class: "label-text font-medium", "总结轮次" }
                                        span { class: "label-text-alt text-base-content/60", "0 = 系统默认" }
                                    }
                                    input { class: "input input-bordered w-full", r#type: "number",
                                        value: "{edit_summary_max_rounds}",
                                        oninput: move |e| edit_summary_max_rounds.set(e.value()),
                                        placeholder: "0 = 使用系统配置" }
                                }
                                div { class: "form-control w-full",
                                    label { class: "label",
                                        span { class: "label-text font-medium", "思考超时秒数" }
                                        span { class: "label-text-alt text-base-content/60", "0 = 不限制" }
                                    }
                                    input { class: "input input-bordered w-full", r#type: "number",
                                        value: "{edit_think_timeout_secs}",
                                        oninput: move |e| edit_think_timeout_secs.set(e.value()),
                                        placeholder: "0 = 不限制" }
                                }
                            }
                        }

                        // 技能包卸载确认对话框
                        if let Some(tag) = show_skill_pack_uninstall_dialog.read().as_ref() {
                            {
                            let tag_a = tag.clone();
                            let tag_b = tag.clone();
                            rsx! {
                            div {
                                class: "modal modal-open hud-modal",
                                onclick: move |_| show_skill_pack_uninstall_dialog.set(None),
                                div {
                                    class: "modal-box hud-modal-box",
                                    onclick: move |e| e.stop_propagation(),
                                    h3 { class: "font-bold text-lg mb-2", "卸载技能包" }
                                    p { class: "text-sm text-base-content/70 mb-4",
                                        "即将卸载技能包 [{tag_a}]，请选择卸载方式："
                                    }
                                    div { class: "flex flex-col gap-3",
                                        // 选项 A：仅移除关联
                                        button {
                                            class: "btn hud-btn btn-ghost justify-start text-left",
                                            onclick: move |_| {
                                                let aid = agent_id_signal();
                                                let t = tag_a.clone();
                                                show_skill_pack_uninstall_dialog.set(None);
                                                spawn(async move {
                                                    match uninstall_skill_pack(UninstallSkillPackRequest { agent_id: aid.clone(), tag: t.clone(), delete_copies: Some(false) }).await {
                                                        Ok(_) => {
                                                            toast.success(format!("技能包 [{}] 已卸载（保留副本）", t));
                                                            match list_installed_skill_packs(&aid).await {
                                                                Ok(resp) => {
                                                                    skill_packs.set(resp.skill_packs);
                                                                    skill_filter_tags.write().retain(|x| x != &t);
                                                                }
                                                                Err(e) => toast.error(format!("刷新失败: {}", e)),
                                                            }
                                                        }
                                                        Err(e) => toast.error(format!("卸载失败: {}", e)),
                                                    }
                                                });
                                            },
                                            div {
                                                p { class: "font-medium", "仅移除关联" }
                                                p { class: "text-xs text-base-content/50", "移除 tag 标记，保留 Agent 侧技能副本" }
                                            }
                                        }
                                        // 选项 B：同时删除副本
                                        button {
                                            class: "btn hud-btn btn-error btn-outline justify-start text-left",
                                            onclick: move |_| {
                                                let aid = agent_id_signal();
                                                let t = tag_b.clone();
                                                show_skill_pack_uninstall_dialog.set(None);
                                                spawn(async move {
                                                    match uninstall_skill_pack(UninstallSkillPackRequest { agent_id: aid.clone(), tag: t.clone(), delete_copies: Some(true) }).await {
                                                        Ok(_) => {
                                                            toast.success(format!("技能包 [{}] 已卸载（含副本删除）", t));
                                                            match list_installed_skill_packs(&aid).await {
                                                                Ok(resp) => {
                                                                    skill_packs.set(resp.skill_packs);
                                                                    skill_filter_tags.write().retain(|x| x != &t);
                                                                }
                                                                Err(e) => toast.error(format!("刷新失败: {}", e)),
                                                            }
                                                        }
                                                        Err(e) => toast.error(format!("卸载失败: {}", e)),
                                                    }
                                                });
                                            },
                                            div {
                                                p { class: "font-medium", "移除关联 + 删除副本" }
                                                p { class: "text-xs text-error/70",
                                                    "⚠ Agent 技能可能已经进化（修改过内容），删除后无法恢复"
                                                }
                                            }
                                        }
                                    }
                                    div { class: "modal-action",
                                        button {
                                            class: "btn hud-btn btn-ghost",
                                            onclick: move |_| show_skill_pack_uninstall_dialog.set(None),
                                            "取消"
                                        }
                                    }
                                }
                            }
                            }
                            }
                        }
                        // ===== 安装 / 卸载二次确认对话框（复用 ConfirmDialog）=====
                        ConfirmDialog {
                            show: show_tool_pack_uninstall_dialog.read().is_some(),
                            title: "卸载工具包".to_string(),
                            message: show_tool_pack_uninstall_dialog.read().as_ref().map(|x| format!("确认卸载工具包 [{}]？该包下所有工具将从 Agent 移除。", x)).unwrap_or_default(),
                            confirm_text: Some("卸载".to_string()),
                            confirm_class: Some("btn hud-btn btn-error".to_string()),
                            on_confirm: move |_| {
                                let t = show_tool_pack_uninstall_dialog();
                                show_tool_pack_uninstall_dialog.set(None);
                                if let Some(tag) = t {
                                    let aid = agent_id_signal();
                                    let mut ft = tool_filter_tags;
                                    spawn(async move {
                                        match uninstall_tool_pack(UninstallToolPackRequest { agent_id: aid.clone(), tag: tag.clone() }).await {
                                            Ok(_) => {
                                                toast.success(format!("工具包 [{}] 已卸载", tag));
                                                ft.write().retain(|x| x != &tag);
                                                match get_agent(build_agent_stats_request(aid.clone())).await {
                                                    Ok(a) => agent_res.set(Some(Ok(a))),
                                                    Err(e) => toast.error(format!("刷新失败: {}", e)),
                                                }
                                                match list_installed_tool_packs(&aid).await {
                                                    Ok(resp) => tool_packs.set(resp.installed_tags),
                                                    Err(e) => toast.error(format!("刷新工具包列表失败: {}", e)),
                                                }
                                            }
                                            Err(e) => toast.error(format!("卸载工具包失败: {}", e)),
                                        }
                                    });
                                }
                            },
                            on_cancel: move |_| show_tool_pack_uninstall_dialog.set(None),
                        }
                        ConfirmDialog {
                            show: show_bind_tool_dialog.read().is_some(),
                            title: "绑定工具".to_string(),
                            message: show_bind_tool_dialog.read().as_ref().map(|(_id, name)| format!("确认将工具 [{}] 绑定到该 Agent？", name)).unwrap_or_default(),
                            confirm_text: Some("绑定".to_string()),
                            confirm_class: Some("btn hud-btn btn-primary".to_string()),
                            on_confirm: move |_| {
                                let info = show_bind_tool_dialog();
                                show_bind_tool_dialog.set(None);
                                if let Some((tid, _tname)) = info {
                                    let aid = agent_id_signal();
                                    spawn(async move {
                                        match bind_tool_to_agent(BindToolToAgentRequest { agent_id: aid.clone(), tool_id: tid.clone() }).await {
                                            Ok(_) => {
                                                toast.success("工具已绑定");
                                                tool_search_results.set(Vec::new());
                                                match get_agent(build_agent_stats_request(aid.clone())).await {
                                                    Ok(a) => agent_res.set(Some(Ok(a))),
                                                    Err(e) => toast.error(format!("刷新 Agent 失败: {}", e)),
                                                }
                                            }
                                            Err(e) => toast.error(format!("绑定失败: {}", e)),
                                        }
                                    });
                                }
                            },
                            on_cancel: move |_| show_bind_tool_dialog.set(None),
                        }
                        ConfirmDialog {
                            show: show_install_skill_dialog.read().is_some(),
                            title: "安装技能".to_string(),
                            message: show_install_skill_dialog.read().as_ref().map(|(_id, name)| format!("确认将技能 [{}] 安装到该 Agent？", name)).unwrap_or_default(),
                            confirm_text: Some("安装".to_string()),
                            confirm_class: Some("btn hud-btn btn-primary".to_string()),
                            on_confirm: move |_| {
                                let info = show_install_skill_dialog();
                                show_install_skill_dialog.set(None);
                                if let Some((sid, _sname)) = info {
                                    let aid = agent_id_signal();
                                    spawn(async move {
                                        match install_skill_to_agent(InstallSkillToAgentRequest { agent_id: aid.clone(), skill_id: sid.clone() }).await {
                                            Ok(_) => {
                                                toast.success("技能已安装");
                                                skill_search_results.set(Vec::new());
                                                match get_agent(build_agent_stats_request(aid.clone())).await {
                                                    Ok(a) => agent_res.set(Some(Ok(a))),
                                                    Err(e) => toast.error(format!("刷新 Agent 失败: {}", e)),
                                                }
                                            }
                                            Err(e) => toast.error(format!("安装失败: {}", e)),
                                        }
                                    });
                                }
                            },
                            on_cancel: move |_| show_install_skill_dialog.set(None),
                        }
                        ConfirmDialog {
                            show: show_tool_unbind_dialog.read().is_some(),
                            title: "解绑工具".to_string(),
                            message: show_tool_unbind_dialog.read().as_ref().map(|(_id, name)| format!("确认解绑工具 [{}]？", name)).unwrap_or_default(),
                            confirm_text: Some("解绑".to_string()),
                            confirm_class: Some("btn hud-btn btn-error".to_string()),
                            on_confirm: move |_| {
                                let info = show_tool_unbind_dialog();
                                show_tool_unbind_dialog.set(None);
                                if let Some((tid, _tname)) = info {
                                    let aid = agent_id_signal();
                                    spawn(async move {
                                        match unbind_tool_from_agent(UnbindToolFromAgentRequest { agent_id: aid.clone(), tool_id: tid.clone() }).await {
                                            Ok(_) => {
                                                toast.success("工具已解绑");
                                                match get_agent(build_agent_stats_request(aid.clone())).await {
                                                    Ok(a) => agent_res.set(Some(Ok(a))),
                                                    Err(e) => toast.error(format!("刷新 Agent 失败: {}", e)),
                                                }
                                            }
                                            Err(e) => toast.error(format!("解绑失败: {}", e)),
                                        }
                                    });
                                }
                            },
                            on_cancel: move |_| show_tool_unbind_dialog.set(None),
                        }
                        ConfirmDialog {
                            show: show_skill_uninstall_dialog.read().is_some(),
                            title: "卸载技能".to_string(),
                            message: show_skill_uninstall_dialog.read().as_ref().map(|(_id, name)| format!("确认卸载技能 [{}]？", name)).unwrap_or_default(),
                            confirm_text: Some("卸载".to_string()),
                            confirm_class: Some("btn hud-btn btn-error".to_string()),
                            on_confirm: move |_| {
                                let info = show_skill_uninstall_dialog();
                                show_skill_uninstall_dialog.set(None);
                                if let Some((sid, _sname)) = info {
                                    let aid = agent_id_signal();
                                    spawn(async move {
                                        match uninstall_skill_from_agent(UninstallSkillFromAgentRequest { agent_id: aid.clone(), skill_id: sid.clone() }).await {
                                            Ok(_) => {
                                                toast.success("技能已卸载");
                                                match get_agent(build_agent_stats_request(aid.clone())).await {
                                                    Ok(a) => agent_res.set(Some(Ok(a))),
                                                    Err(e) => toast.error(format!("刷新失败: {}", e)),
                                                }
                                            }
                                            Err(e) => toast.error(format!("卸载失败: {}", e)),
                                        }
                                    });
                                }
                            },
                            on_cancel: move |_| show_skill_uninstall_dialog.set(None),
                        }
                    }
                }
            }
                }
            Some(Err(e)) => rsx! {
                EmptyState {
                    icon: "❓".to_string(),
                    message: format!("加载失败: {}", e),
                }
            },
            }}
        }
    }
}
