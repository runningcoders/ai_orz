//! 聊天信息侧栏（ChatSidePanel）
//!
//! 沟通页面右侧可收起的信息面板，按对话模式动态组装 Tab：
//! - 项目对话：总览 / 任务 / 产物 / Agent（项目内 Agent 列表）/ 工具 / 思考
//! - 默认对话：任务 / 产物 / Agent（当前任务参与的 Agent）/ 工具 / 思考
//!
//! 两种模式除「项目总览」外结构完全对齐：默认对话只是把数据源换成游离任务
//! （`project_id IS NULL`），Agent / 工具 / 思考统一圈定「当前任务出现过的 Agent」
//! （前台接待恒在集合首位）。
//!
//! 思考 Tab 展示 `pkg/think_trace` 落盘的运行时轨迹：项目模式按 project_id
//! 跨 Agent 聚合，默认对话按「当前任务出现过的 Agent」集合过滤（与工具调用 Tab 同源口径）。
//!
//! 面板纯只读：数据加载复用现有项目/任务/产物/Agent API，
//! 创建与编辑操作仍在跳转各自详情页完成。
//!
//! 产物是这条规则的例外——但只往「更少跳转」的方向：点击侧栏里的产物不再跳
//! 产物详情页，就地弹出 `ArtifactPreviewModal`（基础信息 + 预览 + 下载原件，纯只读）。
//! 详情页本身保留、深链仍可用，这条链路不提供任何编辑入口。
//!
//! ## 刷新只跟着「看得见的东西」走
//!
//! 侧栏收起时外层 div 只是 `hidden`，组件与各 Tab 仍挂载 —— 若不做门闩，
//! SSE 每来一条消息都会替一个看不见的面板重拉项目详情与任务列表。因此刷新分三层收紧：
//!
//! 1. 面板收起（`visible=false`）→ 冻结下发给各 Tab 的刷新计数，零请求；
//! 2. Tab 维度 → 只有当前 Tab 真正消费的那份数据才刷新（见 [`tab_data_needs`]），
//!    工具 / 思考 Tab 自取数，不再陪跑项目与任务请求；
//! 3. Agent Tab 的子视图 → 点进某个 Agent 详情后，列表视图不在屏上就不再批量重取。
//!
//! 切 Tab 会为它消费的数据即时补刷一次（等同局部手动刷新），所以裁剪不会让用户
//! 看到陈旧数据。
//!
//! 例外是**工具 / 思考**两个 Tab：它们的数据源（工具调用记录、`think_trace` JSONL）
//! 变化不产生会话消息、没有 SSE 可等，只能拉。故给它们一条独立的低频轮询通道
//! （`LIVE_REFRESH_SECS`，仅面板可见且正看这两个 Tab 时推进），近似实时地看到
//! 新增工具调用与思考轮次。

use std::collections::HashMap;
use std::time::Duration;

use dioxus::prelude::*;
use dioxus_router::Link;

use crate::api::hr::get_agent;
use crate::api::project::{get_project, get_task, list_project_tasks, query_tasks};
use crate::components::agent_summary::{agent_badge_row, agent_identity_row};
use crate::components::artifact_preview_modal::ArtifactPreviewModal;
use crate::components::avatar_bubble::AvatarTone;
use crate::components::chat::ProjectAgentsTab;
use crate::components::chat::ThinkTracesTab;
use crate::components::chat::ToolCallsTab;
use crate::components::chat::project_agents_tab::agent_ids_from_tasks;
use crate::components::graph::{EdgeCurve, Graph, GraphEdge, GraphNode, task_status_node_type};
use crate::components::hud::{HudPanel, HudProgress};
use crate::components::identity_chip::IdentityChip;
use crate::components::markdown::MarkdownRenderer;
use crate::components::modal::Modal;
use crate::components::state::Loading;
use crate::components::stats::AgentStatsPanelCompact;
use crate::store::toast::{ToastState, use_toast};
use crate::utils::dagre_layout::{DagreEdge, DagreNode, dagre_layout};
use crate::utils::message::request_scope;
use crate::utils::time::now_ms;
use crate::utils::{
    avatar_status_ring, format_file_size, format_timestamp_opt as format_timestamp, priority_badge,
    progress_tone, project_status_badge, project_status_text, tag_chip, task_status_badge,
    task_status_text,
};
use common::api::{
    ArtifactDetail, GetAgentRequest, GetAgentResponse, GetProjectRequest, GetProjectResponse,
    GetTaskRequest, GetTaskResponse, PaginationParams, TaskListItem, TaskQueryRequest,
};
use common::enums::{ArtifactSourceType, AssigneeType, TaskStatus};
use common::models::{AgentStats, ModelCallStats};

/// SSE 消息触发的防抖刷新等待时长（毫秒）
const REFRESH_DEBOUNCE_MS: u64 = 2000;

/// 工具 / 思考 Tab 的「接近实时」轮询周期（秒）
///
/// 这两个 Tab 的数据源（工具调用记录、`think_trace` JSONL）没有 SSE 推送，只能拉；
/// 但也不必跟 chat 主链路的 3s 诊断轮询同频：一道思考通常十几秒才落一轮，
/// 工具调用也只在真正调用时才有新行，取 6s（主链路两拍）已足够「接近实时」，
/// 请求量仍是一个 Tab 每 6s 一次分页查询。叠加各 Tab 自带的 2s 防抖后实际约 8s 一刷。
const LIVE_REFRESH_SECS: u64 = 6;

/// 运行统计的时间窗口（分钟）
///
/// 与工作台顶栏 `RUNTIME_METRICS_WINDOW_MINUTES` 同口径：侧栏这一栏是**运行时**读数，
/// 回答的是「此刻这个 Agent 在不在干活」，因此只看最近 60 分钟。
/// 粒度随之取分钟桶（`stats_interval = minutely`）：60 分钟窗口若用天桶只会得到 1 个点，
/// 用小时桶也只有 1~2 个点，都画不出曲线来。
const RUNTIME_WINDOW_MINUTES: i64 = 60;

/// 默认对话（游离）任务面板的容量上限
///
/// 拉取策略两段式：先拉未完成任务（进行中 + 待办），数量不足上限时再用
/// **最近完成**的任务按 updated_at 倒序补齐至此数。
/// 旧逻辑只展示未完成任务 ⇒ 任务一完成就从面板消失，用户来不及看；
/// 补齐段让「刚消失的那个任务」仍留在面板上（带已完成徽标）。
/// Archived 是用户主动归档的终态，刻意不进补齐池，不把刻意收起的再翻出来。
const FLOATING_TASK_PANEL_LIMIT: usize = 20;

/// 侧栏缩略图画布尺寸（桌面 w-96 侧栏内容区约 336px 宽，移动 w-80 约 272px，
/// 取 300 保证两端都不溢出）
const TASK_GRAPH_W: f64 = 300.0;
const TASK_GRAPH_H: f64 = 200.0;

/// 放大弹窗画布尺寸（max-w-5xl 弹窗内容区，减去弹窗内边距）
const TASK_GRAPH_ZOOM_W: f64 = 920.0;
const TASK_GRAPH_ZOOM_H: f64 = 620.0;

/// 产物预览卡片的打开目标。
///
/// 归属信息（项目名 / 任务标题）在打开时就地捕获：侧栏本就有这两份数据，
/// 弹窗不必再为一行文字多发一次请求。
#[derive(Debug, Clone, PartialEq)]
struct ArtifactPreviewTarget {
    artifact: ArtifactDetail,
    /// 所属项目名（空串 = 未知，弹窗不展示该行）
    project_name: String,
    /// 所属任务标题；`None` = 项目级产物
    task_title: Option<String>,
}

/// 产物预览的打开入口。
///
/// 把「项目名 + 目标信号」打包成一个参数：任务列表本身已有 6 个入参，
/// 再散着加两个会踩 clippy 的 `too_many_arguments`，而且这两项在语义上是一体的
/// ——「从这个项目的产物行打开预览」。
#[derive(Clone)]
struct ArtifactPreviewScope {
    /// 所属项目名（空串 = 未知，弹窗不展示该行）
    project_name: String,
    /// 当前打开的预览目标；`None` = 没有弹窗
    target: Signal<Option<ArtifactPreviewTarget>>,
}

/// 产物来源类型中文文案
fn artifact_source_type_text(source_type: ArtifactSourceType) -> &'static str {
    match source_type {
        ArtifactSourceType::Attachment => "附件",
        ArtifactSourceType::GeneratedContent => "生成内容",
        ArtifactSourceType::RemoteUrl => "远程链接",
    }
}

/// 侧栏 Tab 序列（索引 ↔ 名称的唯一事实源）
///
/// 渲染、刷新裁剪（[`tab_data_needs`]）、轮询判定（[`tab_needs_live_poll`]）都从
/// 这里取名字，不再各自硬编码 0..5 —— 增删或重排 Tab 时不会静默错位
/// （错位会让某个 Tab 白陪跑请求、或该轮询的没轮询到）。
fn tab_names(is_project_mode: bool) -> &'static [&'static str] {
    if is_project_mode {
        &["总览", "任务", "产物", "Agent", "工具", "思考"]
    } else {
        // 与项目模式同序，仅去掉「总览」（默认对话没有项目维度）
        &["任务", "产物", "Agent", "工具", "思考"]
    }
}

/// 取某个索引的 Tab 名，越界返回 `None`（调用方按「不消费 / 不轮询」处理）
fn tab_name(is_project_mode: bool, tab: usize) -> Option<&'static str> {
    tab_names(is_project_mode).get(tab).copied()
}

/// 当前 Tab 真正消费的侧栏数据：`(需要项目详情, 需要任务列表)`
///
/// 侧栏按 Tab 裁剪刷新范围的依据 —— SSE 消息到达时，只有这两份数据的消费方
/// 值得为之付一次请求；其余 Tab 等用户切过去时再补（切 Tab 会即时刷一次）。
///
/// 按 Tab **名**判定而非索引，与 [`tab_names`] 同源：
/// - 总览只用 `GetProjectResponse`（目标、进度汇总、执行计划/结果）
/// - 任务只用任务列表（依赖图与列表都从 `TaskListItem` 构建）
/// - 产物两者都用：项目级产物来自详情，任务级产物要按任务标题分组
/// - 项目模式的 Agent 两者都用：owner 来自详情，成员集合由任务 assignee 推导
/// - 默认对话的 Agent 只用任务列表：置顶 Agent 来自 `target_agent_id`，
///   成员集合由游离任务 assignee 推导
/// - 工具 / 思考各自取数，不消费侧栏这两份数据
///
/// ⚠️ 同名 Tab 在两种模式下消费的数据并不相同，两个分支都必须带 `is_project_mode`：
/// - 产物：项目模式按任务标题分组，要两份；默认对话是硬编码空态「暂无产物」
///   （产物硬绑定真实项目，游离任务不可能有产物），一份都不该拉
/// - Agent：项目模式要项目详情 + 任务列表；默认对话不消费项目详情，只要任务列表
fn tab_data_needs(is_project_mode: bool, tab: usize) -> (bool, bool) {
    match tab_name(is_project_mode, tab) {
        Some("总览") => (true, false),
        Some("任务") => (false, true),
        Some("产物" | "Agent") if is_project_mode => (true, true),
        // 默认对话：成员集合由任务 assignee 推导，切过去时必须补齐任务列表
        Some("Agent") => (false, true),
        _ => (false, false),
    }
}

/// 该 Tab 是否该跟「接近实时」轮询（工具 / 思考）
///
/// 这两个 Tab 的内容变化不会推 SSE 消息（工具调用记录只在调用时写库，
/// `think_trace` 是逐轮落盘的 JSONL），只能靠拉取才看得到新增，
/// 因此给它们一条独立的周期通道；其余 Tab 的数据都有 SSE 消息或自身接口兜着。
///
/// 按 Tab 名判定：两种模式下工具 / 思考的索引并不相同（默认对话少了总览），
/// 用名字就不用记两套数字。
fn tab_needs_live_poll(is_project_mode: bool, tab: usize) -> bool {
    matches!(tab_name(is_project_mode, tab), Some("工具" | "思考"))
}

/// 将产物按归属拆分为两组：项目级（task_id=None）/ 任务级（task_id=Some）
fn split_artifacts(artifacts: &[ArtifactDetail]) -> (Vec<&ArtifactDetail>, Vec<&ArtifactDetail>) {
    let mut project_level = Vec::new();
    let mut task_level = Vec::new();
    for a in artifacts {
        if a.task_id.is_some() {
            task_level.push(a);
        } else {
            project_level.push(a);
        }
    }
    (project_level, task_level)
}

/// 任务级产物按 task_id 分组（保持首次出现顺序）
fn group_by_task<'a>(arts: &[&'a ArtifactDetail]) -> Vec<(String, Vec<&'a ArtifactDetail>)> {
    let mut groups: Vec<(String, Vec<&ArtifactDetail>)> = Vec::new();
    for a in arts {
        let Some(tid) = a.task_id.as_deref() else {
            continue;
        };
        if let Some((_, list)) = groups.iter_mut().find(|(t, _)| t == tid) {
            list.push(a);
        } else {
            groups.push((tid.to_string(), vec![a]));
        }
    }
    groups
}

/// 聊天信息侧栏主组件
///
/// - `project_id`：选中项目 ID（None 表示默认对话模式）。
///   必须以 Signal 传入：use_effect 依赖其变化触发项目数据加载，
///   普通 prop 非响应式，切换项目后 effect 不会重跑（面板会永远转圈）
/// - `target_agent_id`：当前会话目标 Agent ID，由 chat 页统一解析（项目会话 = 项目
///   owner，默认对话 = 前台 Agent）。默认对话用它作为 Agent 列表的置顶项
///   （与 3s 轮询同源，避免「展示 A、刷新 B」错位）；项目模式由项目的 owner 置顶
/// - `agent_info`：主链路轮询共享的 Agent 详情，Agent Tab 优先消费（id 匹配时零请求）
/// - `refresh_tick`：SSE 消息计数器，变化时防抖 2s 后自动刷新项目数据。
///   同样必须以 Signal 传入才能驱动 use_effect 重跑
/// - `visible`：侧栏是否展开。面板收起时外层 div 只是 `hidden`、组件仍挂载，
///   本组件据此冻结下发给各 Tab 的刷新计数 —— 看不见的面板一次请求都不该发
/// - `stats_poll_tick`：统计周期刷新计数器（chat 页 3s 轮询每 30s 递增），
///   仅叠加进 Agent 统计 Tab 的刷新驱动，静默期统计不再停摆
/// - `on_close`：收起面板回调
#[component]
pub fn ChatSidePanel(
    project_id: Signal<Option<String>>,
    refresh_tick: Signal<u64>,
    visible: Signal<bool>,
    stats_poll_tick: u64,
    on_close: Callback,
    /// 主链路轮询共享的目标 Agent 详情（chat 页置底状态气泡与轮询同源），
    /// AgentInfoTab 优先消费，无值时保留自身懒加载兜底。
    agent_info: Signal<Option<GetAgentResponse>>,
    /// 当前会话目标 Agent ID：chat 页的单一解析来源（项目会话 = 项目 owner，
    /// 默认对话 = 前台 Agent）。
    ///
    /// 默认对话模式把它作为 Agent 列表的置顶项，不再自行从项目详情 / reception
    /// 里各取一份 ——「面板展示的 Agent」与「3s 轮询刷新的 Agent」必须是同一个，
    /// 否则会出现展示 A 的身份、刷新 B 的状态这种错位。
    target_agent_id: Signal<Option<String>>,
) -> Element {
    let toast = use_toast();
    let mut project = use_signal(|| None::<GetProjectResponse>);
    let mut tasks = use_signal(Vec::<TaskListItem>::new);
    // 在途请求数：项目详情与任务列表现在可被不同 Tab 独立刷新、也可能并发，
    // 用计数而不是 bool 承载顶部 spinner，避免先完成的一方提前把它关掉
    let mut pending = use_signal(|| 0u32);
    let mut active_tab = use_signal(|| 0usize);
    // 加载代际计数：按数据源分开。两份数据已能各自独立刷新，
    // 共用一代会让并发的两个请求互相把对方判为过期而丢弃
    let mut project_gen = use_signal(|| 0u64);
    let mut tasks_gen = use_signal(|| 0u64);
    let mut prev_project_id = use_signal(|| Option::<String>::None);
    let mut prev_tick = use_signal(|| 0u64);
    let mut prev_tab = use_signal(|| 0usize);
    // 首次可见运行开关：变化检测的初值比较在首帧恒为 false（None==None / 0==0），
    // 默认对话首帧会跳过加载 → 任务面板空「暂无任务」。首帧强制走一次加载后关闭。
    // 侧栏收起时首帧分支不执行（也不写回），故它实际是「首次可见」而非「首次渲染」。
    let mut first_run = use_signal(|| true);
    // 手动刷新计数：叠加到 refresh_tick 一并下发给工具调用 Tab
    let mut manual_tick = use_signal(|| 0u64);

    // 任务 Tab 展开状态与详情缓存（展开时懒加载，命中缓存不再请求）
    let mut expanded_task_id = use_signal(|| None::<String>);
    let mut task_cache = use_signal(HashMap::<String, GetTaskResponse>::new);
    let loading_task_id = use_signal(|| None::<String>);
    // 产物预览卡片：点侧栏产物就地弹窗（基础信息 + 预览 + 下载），不再跳详情页。
    // 状态放主组件 —— artifacts_tab / tasks_tab 都是普通函数，弹窗要挂在本组件的
    // rsx 里，与任务依赖图放大弹窗（graph_zoom_open）同一处置方式。
    let mut artifact_preview = use_signal(|| Option::<ArtifactPreviewTarget>::None);
    // 任务依赖图放大弹窗（缩略图右上按钮打开；状态必须放主组件 —— tasks_tab 是普通函数，
    // Modal 需要挂在本组件返回的 rsx 里）
    let mut graph_zoom_open = use_signal(|| false);

    // 加载项目详情（总览 / 产物 / Agent 三个 Tab 的消费源）
    //
    // debounce=true 时先等待防抖窗口（SSE 触发），期间被更新的代际直接丢弃。
    // 与任务列表拆开的理由见 [`tab_data_needs`]：任务 Tab 只要任务列表，
    // 没必要为它顺带拉一遍带 artifacts 的项目详情。
    let mut do_load_project = move |pid: String, debounce: bool| {
        let my_gen = project_gen() + 1;
        project_gen.set(my_gen);
        pending.set(pending() + 1);
        spawn(async move {
            if debounce {
                gloo_timers::future::sleep(Duration::from_millis(REFRESH_DEBOUNCE_MS)).await;
            }
            if project_gen() == my_gen {
                let req = GetProjectRequest {
                    id: pid,
                    with_progress_summary: Some(true),
                    with_artifacts: Some(true),
                    // 依赖图已改由前端从任务列表自绘（build_task_graph_data），
                    // 不再向后端要 Mermaid task_graph（项目详情页仍需要，别处勿效仿删除）
                    ..Default::default()
                };
                let res = get_project(req).await;
                if project_gen() == my_gen {
                    match res {
                        Ok(p) => project.set(Some(p)),
                        Err(e) => toast.error(format!("加载项目信息失败: {}", e)),
                    }
                }
            }
            pending.set(pending().saturating_sub(1));
        });
    };

    // 加载项目任务列表（任务 / 产物 / Agent 三个 Tab 的消费源）
    //
    // 产物按任务标题分组、Agent 成员集合由任务 assignee 推导，故这两个 Tab 也要它。
    let mut do_load_tasks = move |pid: String, debounce: bool| {
        let my_gen = tasks_gen() + 1;
        tasks_gen.set(my_gen);
        pending.set(pending() + 1);
        spawn(async move {
            if debounce {
                gloo_timers::future::sleep(Duration::from_millis(REFRESH_DEBOUNCE_MS)).await;
            }
            if tasks_gen() == my_gen {
                let res = list_project_tasks(&pid).await;
                if tasks_gen() == my_gen {
                    match res {
                        Ok(r) => tasks.set(r.tasks),
                        Err(e) => toast.error(format!("加载任务列表失败: {}", e)),
                    }
                }
            }
            pending.set(pending().saturating_sub(1));
        });
    };

    // 加载默认对话（虚拟项目）的游离任务：`project_id IS NULL` 的任务
    //
    // 请求侧带哨兵值（见 `utils::message::request_scope`）——后端把哨兵翻译成
    // `project_id IS NULL`，只返回未挂载任何项目的任务，与「默认对话 = 默认虚拟项目」
    // 的任务域口径一致。代际守卫与 `do_load` 同源，避免与项目加载的过期结果互相覆盖。
    //
    // 两段式拉取（2026-10-21 调整，取代 10-03「只展示未完成」的拍板）：
    // ① 未完成任务（进行中 + 待办）优先，可操作的排最前；
    // ② 不足 `FLOATING_TASK_PANEL_LIMIT` 条时，拉最近 Completed 倒序补齐——
    //    任务刚完成时 updated_at 最新，正好落在补齐段顶部，解决「还没等看就消失」。
    let mut do_load_floating = move |debounce: bool| {
        let my_gen = tasks_gen() + 1;
        tasks_gen.set(my_gen);
        pending.set(pending() + 1);
        spawn(async move {
            if debounce {
                gloo_timers::future::sleep(Duration::from_millis(REFRESH_DEBOUNCE_MS)).await;
            }
            // 第一段：未完成任务（进行中 + 待办），带 limit 上限
            let unfinished_req = TaskQueryRequest {
                project_id: request_scope(None),
                status_in: Some(vec![TaskStatus::InProgress, TaskStatus::Pending]),
                pagination: PaginationParams {
                    limit: Some(FLOATING_TASK_PANEL_LIMIT),
                    offset: None,
                },
                ..Default::default()
            };
            let unfinished = match query_tasks(&unfinished_req).await {
                Ok(page) => Some(page.items),
                Err(e) => {
                    if tasks_gen() == my_gen {
                        toast.error(format!("加载任务列表失败: {}", e));
                    }
                    None
                }
            };
            if let Some(mut merged) = unfinished
                && tasks_gen() == my_gen
            {
                // 第二段：数量不足上限才拉补齐段（零增量请求的开销只发生在真需要时）
                if merged.len() < FLOATING_TASK_PANEL_LIMIT {
                    let recent_req = TaskQueryRequest {
                        project_id: request_scope(None),
                        // 只补 Completed：Archived 是用户主动归档，不翻出来；
                        // Cancelled 由后端基线 `status != 0` 排除
                        status_in: Some(vec![TaskStatus::Completed]),
                        pagination: PaginationParams {
                            limit: Some(FLOATING_TASK_PANEL_LIMIT),
                            offset: None,
                        },
                        ..Default::default()
                    };
                    match query_tasks(&recent_req).await {
                        Ok(page) => {
                            merged = merge_unfinished_with_recent(
                                merged,
                                page.items,
                                FLOATING_TASK_PANEL_LIMIT,
                            );
                        }
                        // 补齐失败不丢主数据：未完成列表已可用，仅提示补齐段缺失
                        Err(e) => toast.error(format!("加载最近任务失败: {}", e)),
                    }
                }
                if tasks_gen() == my_gen {
                    tasks.set(merged);
                }
            }
            pending.set(pending().saturating_sub(1));
        });
    };

    // 手动刷新专用副本与模式判断（调用 Signal 读出当前值，渲染时同步订阅）
    let project_id_value = project_id();
    let is_project_mode = project_id_value.is_some();
    let pid_for_refresh = project_id_value.clone();
    let pid_for_tab = project_id_value.clone();

    // 分发给各 Tab 的刷新计数：侧栏收起时冻结。
    //
    // chat 页的 `refresh_tick` 按 SSE 消息递增，而侧栏收起时外层 div 只是 `hidden`、
    // 组件与各 Tab 仍挂着 —— 直接下发会让看不见的面板照样发请求。这里把它落成
    // 一个「可见时才前进」的计数：收起期间 tick 原地不动，重新展开时一次性补上，
    // 各 Tab 的 `refresh_tick` 变化依赖随之自然停摆。
    let mut tab_tick = use_signal(|| 0u64);
    use_effect(move || {
        if !visible() {
            return;
        }
        let now = refresh_tick() + manual_tick();
        // Signal::set 不做相等去重：只在真的变化时写回，否则 effect 自订阅成死循环（同 E2E-1）
        if tab_tick() != now {
            tab_tick.set(now);
        }
    });

    // 工具 / 思考 Tab 的接近实时轮询：只在「面板可见 且 正看这两个 Tab」时推进。
    //
    // 为什么不复用 SSE tick：这两个 Tab 的数据变化本就不产生会话消息（工具调用与思考
    // 轮次都是旁路写库/写文件），SSE tick 永远等不到它们；也不复用 chat 主链路的 3s
    // 循环 —— 那是给置底状态气泡用的，对这两份数据太浪费。故单起一条低频循环。
    // 每拍醒来重判可见性与 Tab：切走 / 收起立刻停摆，切回来重新计时，不会因切走前
    // 的那半拍马上触发一次。
    //
    // ⚠️ 这个 effect 只起**一个**循环：`use_effect` 的依赖在同步执行的闭包体里建立，
    //    `spawn` 内异步读取的 `visible()` / `project_id()` / `active_tab()` 不进依赖表，
    //    因此 `live_tick.set` 不会反过来重跑本 effect 再 spawn 一个循环（否则每 6s 翻倍）。
    //    与 chat 页 3s 主轮询同写法，改这里前先确认 Dioxus 仍保持该语义。
    let mut live_tick = use_signal(|| 0u64);
    use_effect(move || {
        spawn(async move {
            loop {
                gloo_timers::future::sleep(Duration::from_secs(LIVE_REFRESH_SECS)).await;
                if visible() && tab_needs_live_poll(project_id().is_some(), active_tab()) {
                    live_tick.set(live_tick() + 1);
                }
            }
        });
    });

    // 项目切换 → 立即全量加载并重置面板状态；
    // 切 Tab → 只补刷该 Tab 消费的那份数据；同一 Tab 内 tick 变化 → 防抖刷新它。
    //
    // 为什么要按 Tab 裁剪：SSE 每来一条消息都会推 tick，此前无论用户在看哪个 Tab
    // 都要重拉「带 artifacts 的项目详情 + 项目任务列表」；而工具 / 思考 Tab 自己取数、
    // 根本不消费这两份数据，任务 Tab 也只用任务列表。裁剪后不可见 Tab 零请求。
    use_effect(move || {
        // 面板收起：连 prev_* 一并不写回，让展开后按「首次可见 / 换过项目 / 换过 Tab」
        // 重新判定一次，该补的刷新在展开那一刻一次性完成 —— 收起的侧栏零请求零加载
        if !visible() {
            return;
        }
        let pid = project_id();
        let tick = tab_tick();
        let tab = active_tab();
        let is_project = pid.is_some();
        let project_changed = prev_project_id() != pid;
        let tab_changed = prev_tab() != tab;
        let tick_changed = prev_tick() != tick;
        let first_run_now = first_run();
        // 修复 E2E-1：仅在值真正变化时写回。Signal::set 不做相等去重，
        // 无条件写回本 effect 自己订阅的信号会触发 effect 重跑 → 无限循环卡死主线程
        if project_changed || first_run_now {
            prev_project_id.set(pid.clone());
        }
        if tab_changed {
            prev_tab.set(tab);
        }
        if tick_changed {
            prev_tick.set(tick);
        }
        if project_changed || first_run_now {
            first_run.set(false);
            active_tab.set(0);
            prev_tab.set(0);
            expanded_task_id.set(None);
            task_cache.set(HashMap::new());
            // 首帧 / 切项目：两份数据都拉，保证任何 Tab 切过去都有基线（不做按 Tab 裁剪）
            match pid {
                Some(id) => {
                    do_load_project(id.clone(), false);
                    do_load_tasks(id, false);
                }
                None => {
                    project.set(None);
                    // 默认对话 = 默认虚拟项目：任务域是游离任务，同步拉取
                    do_load_floating(false);
                }
            }
            return;
        }
        // 既没换 Tab 也没有新 tick：无需打扰后端。
        // 这一条同时挡住「首帧分支里 active_tab.set(0)」引发的自重跑（值变了但 tab_changed=false）
        if !tab_changed && !tick_changed {
            return;
        }
        let (need_project, need_tasks) = tab_data_needs(is_project, tab);
        if !need_project && !need_tasks {
            return;
        }
        // 切 Tab 是用户主动操作，等同局部手动刷新（立即拉，不防抖），避免切过去看到旧数据；
        // 同一 Tab 内由 SSE 触发的刷新仍走 2s 防抖合并
        let debounce = !tab_changed;
        match pid {
            Some(id) => {
                if need_project {
                    do_load_project(id.clone(), debounce);
                }
                if need_tasks {
                    do_load_tasks(id, debounce);
                }
            }
            None => {
                if need_tasks {
                    do_load_floating(debounce);
                }
            }
        }
    });

    // 手动刷新：项目模式重拉两份数据，默认对话重拉游离任务；两种模式均同步刷新工具调用 Tab
    let manual_refresh = move |_| {
        manual_tick.set(manual_tick() + 1);
        match pid_for_refresh.clone() {
            Some(id) => {
                do_load_project(id.clone(), false);
                do_load_tasks(id, false);
            }
            None => do_load_floating(false),
        }
    };

    // Tab 序列取自 [`tab_names`] 唯一事实源，渲染与刷新判定不会各自漂移
    let tab_labels: Vec<&'static str> = tab_names(is_project_mode).to_vec();

    // 工具调用 Tab 的刷新驱动：已按可见性冻结的 tick（SSE + 手动刷新）
    let tool_tab_tick = tab_tick();
    // 工具 / 思考 Tab 的实际刷新驱动：在可见 tick 之上叠加低频轮询节拍（仅这两个 Tab 在涨）
    let live_tab_tick = tool_tab_tick + live_tick();
    // Agent 统计 Tab 的刷新驱动：SSE tick + 手动刷新 + 30s 周期 tick（对齐后端统计落盘节奏）
    let agent_stats_tick = tool_tab_tick + stats_poll_tick;

    let project_data = project().clone();
    let tasks_list = tasks.read().clone();
    let tab = active_tab();
    // 默认对话的 Agent 作用域：前台接待（置顶）∪ 游离任务 assignee。
    // Agent 列表、工具调用、思考轨迹三处共用同一份集合 —— 否则会出现
    // 「列表里有这个 Agent，但轨迹里过滤不到」的口径漂移。
    let default_agent_ids = agent_ids_from_tasks(&tasks_list, target_agent_id().as_deref());
    let default_agent_scope = (!default_agent_ids.is_empty()).then_some(default_agent_ids);

    // 放大弹窗的图数据：与缩略图共用同一份构建函数，仅画布尺寸不同。
    // 弹窗关闭时不构建（空数据 + 不渲染），避免每次重渲染都白跑一遍布局。
    let graph_zoom_open_now = graph_zoom_open();
    let (zoom_nodes, zoom_edges) = if graph_zoom_open_now {
        build_task_graph_data(&tasks_list, TASK_GRAPH_ZOOM_W, TASK_GRAPH_ZOOM_H, false)
    } else {
        (Vec::new(), Vec::new())
    };

    // Tab 内容分发（模式切换时 active_tab 已由 effect 重置）
    let content: Element = if is_project_mode {
        match tab {
            0 => match &project_data {
                Some(p) => overview_tab(p),
                None => loading_placeholder(),
            },
            1 => tasks_tab(
                &tasks_list,
                expanded_task_id,
                task_cache,
                loading_task_id,
                graph_zoom_open,
                toast,
                ArtifactPreviewScope {
                    project_name: project_data
                        .as_ref()
                        .map(|p| p.name.clone())
                        .unwrap_or_default(),
                    target: artifact_preview,
                },
            ),
            2 => artifacts_tab(project_data.as_ref(), &tasks_list, artifact_preview),
            // 项目内 Agent 列表（T3 需求①）：assignee 推导（复用面板已拉的
            // tasks_list，零额外项目请求）→ query_agents 批量详情；负责人置顶打标，
            // 点击行进入单 Agent 详情子页（AgentInfoTab 原样复用）。
            // 项目详情未就绪时先走加载态，避免拿不到 owner_id 时闪现无负责人徽标的列表。
            3 => match project_data.as_ref() {
                Some(p) => rsx! {
                    ProjectAgentsTab {
                        pinned_agent_id: p.owner_agent_id.clone(),
                        tasks: tasks_list.clone(),
                        // Agent Tab 走 SSE/手动 tick 即可：它有会话消息可等，
                        // 不该吃工具 / 思考那条轮询通道（tab_needs_live_poll 已排除它，
                        // 但这里若写 live_tab_tick，将来 Tab 序列一变就会连带被轮询）
                        refresh_tick: tool_tab_tick,
                        agent_info,
                        agent_stats_tick,
                    }
                },
                None => loading_placeholder(),
            },
            4 => rsx! {
                ToolCallsTab {
                    project_id: pid_for_tab.clone(),
                    agent_ids: None,
                    refresh_tick: live_tab_tick,
                }
            },
            // 思考轨迹：项目模式下按 project_id 跨 Agent 聚合（一行一个 run，
            // 行头用 IdentityChip 标识发起 Agent），与工具调用 Tab 同源刷新
            5 => rsx! {
                ThinkTracesTab {
                    project_id: pid_for_tab.clone(),
                    agent_ids: None,
                    refresh_tick: live_tab_tick,
                }
            },
            _ => rsx! {},
        }
    } else {
        match tab {
            // 默认对话 = 默认虚拟项目：任务 Tab 展示游离任务，复用项目模式的任务列表组件
            0 => tasks_tab(
                &tasks_list,
                expanded_task_id,
                task_cache,
                loading_task_id,
                graph_zoom_open,
                toast,
                ArtifactPreviewScope {
                    // 游离任务没有所属项目名：弹窗里不展示该行
                    project_name: String::new(),
                    target: artifact_preview,
                },
            ),
            // 产物硬绑定真实项目，游离任务不可能有产物 → 先做空态占位
            1 => empty_hint("暂无产物"),
            // Agent Tab 与项目模式完全同构：前台接待置顶 + 当前游离任务的 assignee，
            // 点击行进入 Agent 详情子页（不再是单值「只显示前台接待」）
            2 => rsx! {
                ProjectAgentsTab {
                    pinned_agent_id: target_agent_id(),
                    tasks: tasks_list.clone(),
                    refresh_tick: tool_tab_tick,
                    agent_info,
                    agent_stats_tick,
                }
            },
            // 工具 / 思考：范围与 Agent 列表同源（当前任务出现过的 Agent 集合）
            3 => rsx! {
                ToolCallsTab {
                    project_id: None,
                    agent_ids: default_agent_scope.clone(),
                    refresh_tick: live_tab_tick,
                }
            },
            4 => rsx! {
                ThinkTracesTab {
                    project_id: None,
                    agent_ids: default_agent_scope.clone(),
                    refresh_tick: live_tab_tick,
                }
            },
            _ => rsx! {},
        }
    };

    rsx! {
        div { class: "p-3 border-b border-base-300 flex items-center gap-2",
            h3 { class: "font-semibold text-sm flex-1 truncate", "信息面板" }
            if pending() > 0 {
                Loading { size: "xs" }
            }
            button {
                class: "btn hud-btn btn-ghost btn-xs",
                title: "刷新",
                onclick: manual_refresh,
                "⟳"
            }
            button {
                class: "btn hud-btn btn-ghost btn-xs",
                title: "收起面板",
                onclick: move |_| on_close.call(()),
                "✕"
            }
        }
        div { class: "flex flex-wrap gap-2 m-2",
            for (i, label) in tab_labels.iter().enumerate() {
                button {
                    key: "{label}",
                    class: if tab == i { "btn hud-btn btn-xs btn-primary" } else { "btn hud-btn btn-xs btn-ghost" },
                    onclick: move |_| active_tab.set(i),
                    "{label}"
                }
            }
        }
        div { class: "flex-1 overflow-y-auto p-3", {content} }
        // 产物预览卡片：top layer 弹窗，关闭即卸载（show 语义由条件渲染承载）
        if let Some(target) = artifact_preview() {
            ArtifactPreviewModal {
                artifact: target.artifact,
                project_name: target.project_name,
                task_title: target.task_title,
                on_close: move |_| artifact_preview.set(None),
            }
        }
        // 任务依赖图放大弹窗：top layer 渲染不受侧栏 overflow / 堆叠上下文裁剪
        if graph_zoom_open_now {
            Modal {
                show: true,
                title: "任务依赖图".to_string(),
                on_close: move |_| graph_zoom_open.set(false),
                width_class: Some("max-w-5xl".to_string()),
                Graph {
                    nodes: zoom_nodes,
                    edges: zoom_edges,
                    svg_width: Some(TASK_GRAPH_ZOOM_W as u32),
                    svg_height: Some(TASK_GRAPH_ZOOM_H as u32),
                    // 力传导松弛是无边界迭代：坐标可能超出画布，靠初始 fit 收景
                    auto_fit: true,
                    on_node_click: move |_id: String| {},
                }
            }
        }
    }
}

pub(crate) fn loading_placeholder() -> Element {
    rsx! {
        div { class: "flex items-center justify-center py-12",
            Loading { size: "md" }
            span { class: "ml-2 text-sm text-base-content/60", "加载中..." }
        }
    }
}

fn empty_hint(msg: &str) -> Element {
    rsx! {
        div { class: "text-center py-12 text-base-content/60 text-sm", "{msg}" }
    }
}

/// Tab 总览：项目目标、进度汇总、执行计划/结果
fn overview_tab(p: &GetProjectResponse) -> Element {
    let desc = p.description.clone().filter(|s| !s.is_empty());
    let plan = p.execution_plan.clone().filter(|s| !s.is_empty());
    let result = p.execution_result.clone().filter(|s| !s.is_empty());
    rsx! {
        div { class: "space-y-4",
            // 基础信息：状态 / 优先级 / 标签 / 负责人
            div { class: "flex flex-wrap items-center gap-1",
                span { class: "{project_status_badge(p.status)}", "{project_status_text(p.status)}" }
                span { class: "{priority_badge(p.priority)}", "P{p.priority}" }
                for tag in p.tags.iter() {
                    span { key: "{tag}", class: "{tag_chip()}", "{tag}" }
                }
            }
            // 负责人字段只带 Agent ID，直接插 rsx 会把 ulid 打到界面上；
            // 走身份 chip 换成「头像 + 名字」，点头像展开与聊天页同源的 Agent 信息卡
            if let Some(owner) = p.owner_agent_id.as_deref() {
                div { class: "flex items-center gap-2 min-w-0",
                    span { class: "text-xs text-base-content/60", "负责人" }
                    IdentityChip { id: owner.to_string(), tone: AvatarTone::Agent }
                }
            }

            // 项目目标
            if let Some(d) = desc {
                div {
                    label { class: "form-label", "项目目标" }
                    MarkdownRenderer { content: d, compact: true }
                }
            }

            // 进度汇总
            if let Some(s) = &p.progress_summary {
                div {
                    label { class: "form-label", "整体进度" }
                    HudProgress { value: s.overall_percent as i32, tone: Some(progress_tone(s.overall_percent as i32).to_string()), show_value: Some(false) }
                    div { class: "text-xs text-base-content/60 mt-1",
                        "{s.overall_percent}% · 共 {s.total_tasks} 个任务（完成 {s.completed} / 进行中 {s.in_progress} / 待启动 {s.pending} / 已取消 {s.cancelled}）"
                    }
                }
            }

            // 执行计划 / 执行结果
            if let Some(plan) = plan {
                div {
                    label { class: "form-label", "执行计划" }
                    MarkdownRenderer { content: plan, compact: true }
                }
            }
            if let Some(result) = result {
                div {
                    label { class: "form-label", "执行结果" }
                    MarkdownRenderer { content: result, compact: true }
                }
            }
        }
    }
}

/// 任务列表 → 自有图引擎数据（缩略图与放大弹窗共用，含布局坐标）
///
/// 依赖方向：A.dependencies 含 B ⇒ B 是 A 的前置 ⇒ 画 B → A（前置在上、依赖在下）。
///
/// # 布局管线（dagre 分层布局，2026-10-24 替换旧四步自研管线）
///
/// 旧管线（Kahn 分层 → spread → 力导向松弛 → 弧线避障）有三个结构性问题：
/// 同层均分不知卡片宽度（宽卡同层必互压）、弧线避障是 O(边×节点) 局部启发式
/// （密集边仍穿卡）、成环数据要靠破环补丁。dagre（Sugiyama 框架工业实现）
/// 一次做完四件事，详见 `utils::dagre_layout`：
///
/// 1. 节点按**实际视觉尺寸**喂 dagre（卡片态逐节点 `node_box_width/height`，
///    点线态统一点径）——同层不重叠由坐标层面保证；
/// 2. 边产出 waypoints 折线（`EdgeCurve::Polyline`）——边不穿卡由路由层面保证；
/// 3. 自环不进 dagre（dagre 不布局 `source==target`），走 `EdgeCurve::Loop`；
/// 4. 输出包围盒等比缩放 + 居中映射进画布——任何规模完整落进画布，
///    「飞出画布 / 贴边」从根上消失。
///
/// # 参数
///
/// - `mini`：`true` = 缩略图点线态，`false` = 放大弹窗卡片态。两者节点视觉尺寸差
///   一个量级，间距与节点尺寸分别按态传入，否则缩略图会被按卡片宽拉散 /
///   卡片态互相压住。
fn build_task_graph_data(
    tasks: &[TaskListItem],
    width: f64,
    height: f64,
    mini: bool,
) -> (Vec<GraphNode>, Vec<GraphEdge>) {
    let ids: std::collections::HashSet<&str> = tasks.iter().map(|t| t.id.as_str()).collect();
    // 负责人展示名：Agent 分配对象（assignee_type=1）显示 ID 前 8 位（完整 ID 过长，
    // hover 详情另有 ID 行），其余 / 未分配显示占位文案（任务列表页同款口径）
    let assignee_name = |t: &TaskListItem| -> String {
        if t.assignee_id.is_empty() {
            "未分配".to_string()
        } else if AssigneeType::from_i32(t.assignee_type) == AssigneeType::Agent {
            format!("Agent·{}", t.assignee_id.get(..8).unwrap_or(&t.assignee_id))
        } else {
            t.assignee_id.clone()
        }
    };
    // 1. 节点（坐标先占位画布中心，实际位置由 dagre 布局写回）
    let mut nodes: Vec<GraphNode> = tasks
        .iter()
        .map(|t| GraphNode {
            id: t.id.clone(),
            label: t.title.clone(),
            description: t.description.clone().unwrap_or_default(),
            // 状态 → 语义化 token：着色与 hover 标签都按状态区分（见 graph.rs 配色映射）
            node_type: task_status_node_type(t.status).to_string(),
            x: width / 2.0,
            y: height / 2.0,
            tags: t.tags.clone(),
            // hover 关键信息一行概览：状态 · 进度 · 负责人
            //（node_hover_lines 通道自动携带，点线图下名称也由 hover 承载）
            summary: Some(format!(
                "{} · 进度 {}% · 负责人 {}",
                task_status_text(t.status),
                t.progress,
                assignee_name(t)
            )),
        })
        .collect();
    // 2. 边（前置 → 任务；样式字段不变，曲率由 dagre waypoints 写回）
    let mut edges: Vec<GraphEdge> = tasks
        .iter()
        .flat_map(|t| {
            t.dependencies
                .iter()
                .filter(|d| ids.contains(d.as_str()))
                .map(move |d| GraphEdge {
                    source: d.clone(),
                    target: t.id.clone(),
                    label: "前置".to_string(),
                    weight: None,
                    // 任务依赖图语义有向（前置 → 任务）：保持既有恒挂箭头行为零回退
                    direction: "directed".to_string(),
                    curve: None,
                    dash: None,
                    width: None,
                })
        })
        .collect();
    // 3. dagre 布局：节点尺寸按态取（卡片态逐节点算，点线态统一点径）
    let dot =
        (crate::components::graph::dot_node_radius(width, height, nodes.len()) * 2.0).max(18.0);
    let dagre_nodes: Vec<DagreNode> = nodes
        .iter()
        .map(|n| {
            let (w, h) = if mini {
                (dot, dot)
            } else {
                (
                    crate::components::graph::node_box_width(n),
                    crate::components::graph::node_box_height(n),
                )
            };
            DagreNode {
                id: n.id.clone(),
                width: w,
                height: h,
            }
        })
        .collect();
    let dagre_edges: Vec<DagreEdge> = edges
        .iter()
        .map(|e| DagreEdge {
            source: e.source.clone(),
            target: e.target.clone(),
        })
        .collect();
    let layout = dagre_layout(&dagre_nodes, &dagre_edges, width, height, mini);
    // 4. 写回节点坐标
    for n in &mut nodes {
        if let Some((x, y)) = layout.positions.get(&n.id) {
            n.x = *x;
            n.y = *y;
        }
    }
    // 5. 写回边曲率：自环 → Loop（dagre 不布局）；其余 → waypoints 折线；
    //    无路由（理论不可达）保持直线兜底
    for e in &mut edges {
        let key = (e.source.clone(), e.target.clone());
        if layout.self_loops.contains(&key) {
            e.curve = Some(EdgeCurve::Loop);
        } else if let Some(pts) = layout.routes.get(&key) {
            e.curve = Some(EdgeCurve::Polyline(pts.clone()));
        }
    }
    (nodes, edges)
}

/// 游离任务面板合并：未完成任务在前，不足 `limit` 条时用「最近任务」补齐
///
/// - 未完成段保持后端返回序（priority DESC, created_at DESC），不重排——可操作的最相关；
/// - 补齐段按 `updated_at` 倒序（同值按 id 倒序，与后端 tie-break 同风格）：
///   任务刚完成时 updated_at 最新，正好落在补齐段顶部；
/// - 按 id 去重（补齐查询可能带回已在未完成段里的任务），并硬截断到 `limit`。
///
/// 后端 `query` 端点只支持 priority/created_at 排序，没有 updated_at 选项，
/// 故补齐段在前端重排；窗口上限由 limit 保证，重排不会漏掉窗口外更「近」的任务。
fn merge_unfinished_with_recent(
    unfinished: Vec<TaskListItem>,
    recent: Vec<TaskListItem>,
    limit: usize,
) -> Vec<TaskListItem> {
    let mut merged = unfinished;
    merged.truncate(limit);
    if merged.len() >= limit {
        return merged;
    }
    let mut fillers: Vec<TaskListItem> = recent
        .into_iter()
        .filter(|t| !merged.iter().any(|u| u.id == t.id))
        .collect();
    fillers.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| b.id.cmp(&a.id))
    });
    merged.extend(fillers.into_iter().take(limit - merged.len()));
    merged
}

/// Tab 任务：任务依赖图（自有图引擎，hover 看节点详情 / 右上按钮放大）+ 任务列表，
/// 点击单任务展开详情（懒加载 + 缓存）
fn tasks_tab(
    tasks: &[TaskListItem],
    mut expanded_task_id: Signal<Option<String>>,
    mut task_cache: Signal<HashMap<String, GetTaskResponse>>,
    mut loading_task_id: Signal<Option<String>>,
    mut graph_zoom_open: Signal<bool>,
    toast: ToastState,
    preview_scope: ArtifactPreviewScope,
) -> Element {
    if tasks.is_empty() {
        return empty_hint("暂无任务");
    }
    // 缩略图与放大弹窗共用同一套图数据：仅画布尺寸不同
    let (graph_nodes, raw_edges) = build_task_graph_data(tasks, TASK_GRAPH_W, TASK_GRAPH_H, true);
    rsx! {
        // 任务依赖图：位于任务列表上方
        HudPanel {
            title: "任务依赖图".to_string(),
            eyebrow: "DEPENDENCIES".to_string(),
            actions: Some(rsx! {
                button {
                    class: "btn hud-btn btn-ghost btn-xs",
                    title: "放大查看",
                    onclick: move |_| graph_zoom_open.set(true),
                    "🔍"
                }
            }),
            Graph {
                nodes: graph_nodes,
                edges: raw_edges,
                svg_width: Some(TASK_GRAPH_W as u32),
                svg_height: Some(TASK_GRAPH_H as u32),
                // 小地图模式：卡片只保留名称 + 连线，正文 / 标签省略，作缩略图预览
                mini: Some(true),
                // 力传导松弛是无边界迭代：与放大弹窗同口径，靠初始 fit 收景
                auto_fit: true,
                // 点节点 = 展开任务列表中对应项的详情
                on_node_click: move |id: String| {
                    expanded_task_id.set(Some(id));
                },
            }
        }
        div { class: "space-y-2",
            for t in tasks.iter() {
                {
                    let tid = t.id.clone();
                    let title = t.title.clone();
                    let status = t.status;
                    let progress = t.progress;
                    let assignee = t.assignee_id.clone();
                    // 分配对象类型（0=用户 1=Agent）→ 头像基调 / 名称目录 / 卡片形态
                    let assignee_tone = AvatarTone::from(AssigneeType::from_i32(t.assignee_type));
                    let is_expanded = expanded_task_id() == Some(tid.clone());
                    let is_loading = loading_task_id() == Some(tid.clone());
                    rsx! {
                        div {
                            key: "{tid}",
                            class: "rounded-lg border border-base-300 bg-base-100",
                            // 列表行：点击切换展开
                            div {
                                class: "p-2 cursor-pointer hover:bg-base-200 rounded-t-lg",
                                onclick: move |_| {
                                    if expanded_task_id() == Some(tid.clone()) {
                                        expanded_task_id.set(None);
                                        return;
                                    }
                                    expanded_task_id.set(Some(tid.clone()));
                                    // 首次展开懒加载详情（缓存命中则跳过）
                                    if !task_cache.read().contains_key(&tid)
                                        && loading_task_id() != Some(tid.clone())
                                    {
                                        loading_task_id.set(Some(tid.clone()));
                                        let tid2 = tid.clone();
                                        spawn(async move {
                                            let req = GetTaskRequest {
                                                id: tid2.clone(),
                                                with_artifacts: Some(true),
                                                ..Default::default()
                                            };
                                            match get_task(req).await {
                                                Ok(t) => {
                                                    task_cache.write().insert(tid2, t);
                                                }
                                                Err(e) => {
                                                    toast.error(format!("加载任务详情失败: {}", e));
                                                }
                                            }
                                            loading_task_id.set(None);
                                        });
                                    }
                                },
                                div { class: "flex items-center gap-2",
                                    span { class: "{task_status_badge(status)}", "{task_status_text(status)}" }
                                    span { class: "font-medium text-sm flex-1 truncate", "{title}" }
                                    span { class: "text-xs text-base-content/60", "{progress}%" }
                                    if is_expanded { "▲" } else { "▼" }
                                }
                                HudProgress { value: progress, tone: Some(progress_tone(progress).to_string()), show_value: Some(false), extra_class: Some("mt-1".to_string()) }
                                div { class: "flex items-center gap-2 min-w-0 mt-1",
                                    span { class: "text-xs text-base-content/60", "负责人" }
                                    IdentityChip { id: assignee, tone: assignee_tone }
                                }
                            }
                            // 展开详情
                            if is_expanded {
                                div { class: "p-2 border-t border-base-300",
                                    if is_loading {
                                        div { class: "flex items-center justify-center py-4",
                                            Loading { size: "sm" }
                                        }
                                    } else if let Some(detail) = task_cache.read().get(&tid) {
                                        {
                                            let detail = detail.clone();
                                            let tid_link = tid.clone();
                                            rsx! {
                                                {task_expanded_content(detail, tid_link, preview_scope.clone())}
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
    }
}

/// 任务展开内容：描述、时间、执行计划/结果、产物列表
fn task_expanded_content(
    t: GetTaskResponse,
    tid: String,
    preview_scope: ArtifactPreviewScope,
) -> Element {
    let task_title = t.title.clone();
    let desc = t.description.clone().filter(|s| !s.is_empty());
    let plan = t.execution_plan.clone().filter(|s| !s.is_empty());
    let result = t.execution_result.clone().filter(|s| !s.is_empty());
    let artifacts = t.artifacts.clone().unwrap_or_default();
    rsx! {
        div { class: "space-y-3 text-sm",
            div { class: "text-xs text-base-content/60",
                "开始：{format_timestamp(t.start_at)} · 截止：{format_timestamp(t.due_at)}"
            }
            if let Some(d) = desc {
                div {
                    label { class: "form-label", "描述" }
                    MarkdownRenderer { content: d, compact: true }
                }
            }
            if let Some(plan) = plan {
                div {
                    label { class: "form-label", "执行计划" }
                    MarkdownRenderer { content: plan, compact: true }
                }
            }
            if let Some(result) = result {
                div {
                    label { class: "form-label", "执行结果" }
                    MarkdownRenderer { content: result, compact: true }
                }
            }
            if !artifacts.is_empty() {
                div {
                    label { class: "form-label", "产物（{artifacts.len()}）" }
                    div { class: "flex flex-wrap gap-1",
                        for a in artifacts.iter() {
                            {
                                let a = a.clone();
                                let aid = a.id.clone();
                                let aname = a.name.clone();
                                let scope = preview_scope.clone();
                                let tt = task_title.clone();
                                rsx! {
                                    div {
                                        key: "{aid}",
                                        class: "{tag_chip()} cursor-pointer",
                                        title: "预览产物",
                                        onclick: move |_| {
                                            let mut target = scope.target;
                                            target.set(Some(ArtifactPreviewTarget {
                                                artifact: a.clone(),
                                                project_name: scope.project_name.clone(),
                                                task_title: Some(tt.clone()),
                                            }))
                                        },
                                        "{aname}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Link {
                class: "btn hud-btn btn-ghost btn-xs",
                to: crate::pages::Route::TaskDetail { id: tid },
                "在详情页打开 →"
            }
        }
    }
}

/// Tab 产物：项目级产物 + 任务级产物（按任务分组）
fn artifacts_tab(
    project: Option<&GetProjectResponse>,
    tasks: &[TaskListItem],
    mut artifact_preview: Signal<Option<ArtifactPreviewTarget>>,
) -> Element {
    let Some(p) = project else {
        return loading_placeholder();
    };
    let project_name = p.name.clone();
    let artifacts = p.artifacts.clone().unwrap_or_default();
    if artifacts.is_empty() {
        return empty_hint("暂无产物");
    }
    let (project_level, task_level) = split_artifacts(&artifacts);
    let groups = group_by_task(&task_level);
    rsx! {
        div { class: "space-y-4",
            // 项目级产物
            div {
                label { class: "form-label", "项目级产物（{project_level.len()}）" }
                if project_level.is_empty() {
                    div { class: "text-xs text-base-content/60", "暂无" }
                } else {
                    div { class: "space-y-1",
                        for a in project_level.iter() {
                            {
                                let a = (*a).clone();
                                let pn = project_name.clone();
                                rsx! {
                                    ArtifactRow {
                                        artifact: a,
                                        on_open: move |art: ArtifactDetail| artifact_preview.set(Some(ArtifactPreviewTarget {
                                            artifact: art,
                                            project_name: pn.clone(),
                                            task_title: None,
                                        })),
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // 任务级产物（按任务分组）
            div {
                label { class: "form-label", "任务级产物（{task_level.len()}）" }
                if groups.is_empty() {
                    div { class: "text-xs text-base-content/60", "暂无" }
                } else {
                    for (tid, arts) in groups.iter() {
                        {
                            let task_title = tasks
                                .iter()
                                .find(|t| &t.id == tid)
                                .map(|t| t.title.clone())
                                .unwrap_or_else(|| {
                                    let truncated: String = tid.chars().take(8).collect();
                                    format!("任务 {}…", truncated)
                                });
                            let arts = arts.iter().map(|a| (*a).clone()).collect::<Vec<_>>();
                            let tid_key = tid.clone();
                            rsx! {
                                div { key: "{tid_key}", class: "mb-2",
                                    div { class: "text-xs font-medium text-base-content/70 mb-1", "📌 {task_title}" }
                                    div { class: "space-y-1",
                                        for a in arts.iter() {
                                            {
                                                let a = a.clone();
                                                let pn = project_name.clone();
                                                let tt = task_title.clone();
                                                rsx! {
                                                    ArtifactRow {
                                                        artifact: a,
                                                        on_open: move |art: ArtifactDetail| artifact_preview.set(Some(ArtifactPreviewTarget {
                                                            artifact: art,
                                                            project_name: pn.clone(),
                                                            task_title: Some(tt.clone()),
                                                        })),
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
            }
        }
    }
}

/// 单行产物卡片（只读，点击就地打开预览卡片弹窗，不再跳转产物详情页）
#[component]
fn ArtifactRow(artifact: ArtifactDetail, on_open: EventHandler<ArtifactDetail>) -> Element {
    let payload = artifact.clone();
    let name = artifact.name.clone();
    let source_text = artifact_source_type_text(artifact.source_type);
    let size = format_file_size(artifact.file_size);
    let created = format_timestamp(Some(artifact.created_at));
    rsx! {
        div {
            class: "flex items-center gap-2 p-2 rounded-lg border border-base-300 bg-base-100 hover:bg-base-200 cursor-pointer",
            title: "预览产物",
            onclick: move |_| on_open.call(payload.clone()),
            div { class: "flex-1 min-w-0",
                div { class: "text-sm font-medium truncate", "{name}" }
                div { class: "text-xs text-base-content/60", "{size} · {created}" }
            }
            span { class: "badge orz-tag badge-sm", "{source_text}" }
        }
    }
}

/// Tab Agent（两种模式共用）：展示 Agent 详情。
///
/// 数据源优先级：chat 主链路轮询共享的 `shared_info`（id 匹配才消费，
/// 随轮询实时刷新 runtime 徽章）> 组件自身懒加载兜底（面板独立使用 /
/// 共享数据未就绪时）。
///
/// 运行统计（唤醒/Token/趋势）独立拉取：共享轮询请求不带 stats 参数
/// （主链路高频轮询零额外开销），统计仅在 Tab 挂载时按需加载，
/// 并随 `refresh_tick`（SSE/手动刷新）防抖刷新，机制与 ToolCallsTab 一致。
#[component]
pub(crate) fn AgentInfoTab(
    agent_id: String,
    shared_info: Signal<Option<GetAgentResponse>>,
    refresh_tick: u64,
) -> Element {
    let mut agent = use_signal(|| None::<GetAgentResponse>);
    let mut failed = use_signal(|| false);
    // 共享数据已就绪且 id 匹配：跳过自身请求
    //
    // ⚠️ `agent_id` 是普通 prop，`use_effect` 不订阅它 —— 必须用 `use_reactive`
    // 显式声明，否则从项目内 Agent 列表切到另一个 Agent 详情时会继续展示上一个的数据
    use_effect(use_reactive(&agent_id, move |aid: String| {
        // peek 而非读取：chat 主链路每 3s 重写 shared_info，若在这里订阅，看非 owner
        // Agent 时会被它的高频更新带着每 3s 重发一次自身请求
        let shared = shared_info.peek();
        if shared.as_ref().is_some_and(|a| a.id == aid) {
            return;
        }
        spawn(async move {
            let req = GetAgentRequest {
                id: aid,
                ..Default::default()
            };
            match get_agent(req).await {
                Ok(a) => agent.set(Some(a)),
                Err(_) => failed.set(true),
            }
        });
    }));

    // ---- 运行统计：独立拉取（带 stats fetch-options），防抖刷新与代际丢弃 ----
    // 四元组同源于一次响应：避免切换 Agent 时统计数据与上下文长度错代混用
    let mut stats_pair = use_signal(|| {
        None::<(
            Option<AgentStats>,
            Option<ModelCallStats>,
            Option<u64>,
            Option<u64>,
        )>
    });
    let mut stats_loaded = use_signal(|| false);
    let mut stats_gen = use_signal(|| 0u64);
    let mut prev_stats_tick = use_signal(|| 0u64);

    // id 由调用方传入而不是闭包捕获：项目内 Agent 列表里切到另一个 Agent 时，
    // 组件不重建，捕获值会一直是挂载时那个 id，统计就会拉到别人身上
    let mut load_stats = move |id: String, debounce: bool| {
        let my_gen = stats_gen() + 1;
        stats_gen.set(my_gen);
        spawn(async move {
            if debounce {
                gloo_timers::future::sleep(Duration::from_millis(REFRESH_DEBOUNCE_MS)).await;
                if stats_gen() != my_gen {
                    return;
                }
            }
            // 运行统计窗口 = 最近 60 分钟（与工作台顶栏同口径），粒度取分钟桶。
            // ⚠️ `stats_interval` 是**白名单字符串**：后端只认 `minutely` / `hourly` / `daily`，
            // 其余取值会被静默忽略并退回 daily（`StatsFetchOptions::interval` 的兜底），
            // 于是又画出一排日期标签 —— 改这个值时要同步核对 `handlers/hr/agent/get_agent.rs`。
            let end_ms = now_ms();
            let req = GetAgentRequest {
                id,
                with_stats: Some(true),
                with_model_call_stats: Some(true),
                stats_time_start: Some(end_ms - RUNTIME_WINDOW_MINUTES * 60_000),
                stats_time_end: Some(end_ms),
                stats_interval: Some("minutely".to_string()),
                ..Default::default()
            };
            let pair = match get_agent(req).await {
                Ok(a) => {
                    // 顺带用这次响应刷新详情本身：非 owner 的 Agent（项目内其它成员）吃不到
                    // chat 主链路的 3s 共享轮询，状态徽章只有靠这一次才能动，
                    // 否则会一直停在「点开那一刻」的运行时状态
                    agent.set(Some(a.clone()));
                    Some((
                        a.stats.clone(),
                        a.model_call_stats.clone(),
                        a.context_length,
                        a.context_length_threshold,
                    ))
                }
                Err(_) => None,
            };
            if stats_gen() != my_gen {
                return;
            }
            if let Some(p) = pair {
                stats_pair.set(Some(p));
            }
            stats_loaded.set(true);
        });
    };

    // 挂载/Agent 切换 → 立即加载；refresh_tick 变化 → 防抖刷新。
    // 守卫结构与 ToolCallsTab 一致：仅在值真正变化时写回 + 调 load，
    // 避免 load 内写 gen 信号触发 effect 重跑 → 无条件 load 的请求循环（同 E2E-1）。
    //
    // ⚠️ 两个值 prop 都要 `use_reactive` 声明：`use_effect` 不订阅普通 prop，
    // 否则 30s 统计节拍、SSE 与手动刷新都推不动这一栏
    let mut prev_stats_agent = use_signal(String::new);
    use_effect(use_reactive(
        (&agent_id, &refresh_tick),
        move |(aid, tick)| {
            let agent_changed = prev_stats_agent() != aid;
            let tick_changed = prev_stats_tick() != tick;
            if agent_changed {
                prev_stats_agent.set(aid.clone());
            }
            if tick_changed {
                prev_stats_tick.set(tick);
            }
            if agent_changed {
                // 切换 Agent 时丢弃旧统计，避免新请求返回前闪现上一个 Agent 的数据
                stats_pair.set(None);
                stats_loaded.set(false);
                load_stats(aid, false);
            } else if tick_changed {
                load_stats(aid, true);
            }
        },
    ));

    if failed() {
        return empty_hint("Agent 信息加载失败");
    }
    // 共享数据优先（id 匹配），否则用自身懒加载结果
    let shared = shared_info().as_ref().filter(|a| a.id == agent_id).cloned();
    let Some(a) = shared.or_else(|| agent().clone()) else {
        return loading_placeholder();
    };
    let desc = a.description.clone().filter(|s| !s.is_empty());
    let capabilities = a.capabilities.clone().unwrap_or_default();
    let aid = a.id.clone();
    let (agent_stats, model_call_stats, context_length, context_length_threshold) =
        stats_pair().unwrap_or((None, None, None, None));
    rsx! {
        div { class: "space-y-4",
            // 身份行 / 徽章行与头像信息气泡（AvatarBubble）共用同一实现，
            // 避免两处展示逐字复制后静默漂移（见 components/agent_summary.rs）
            { agent_identity_row(&a, avatar_status_ring(a.status)) }
            { agent_badge_row(&a) }
            // 运行统计：首次加载完成后渲染（无数据时面板内给出提示）
            if stats_loaded() {
                AgentStatsPanelCompact {
                    stats: agent_stats,
                    model_call_stats,
                    context_length,
                    context_length_threshold,
                    // 阈值缺失时面板需要给出「去配置」入口，指向本条消息所属 Agent 的供应商
                    model_provider_id: a.model_provider_id.clone(),
                    // 读数口径与上方请求窗口同源，避免被读成历史累计
                    window_label: Some(format!("最近 {RUNTIME_WINDOW_MINUTES} 分钟")),
                }
            }
            if let Some(d) = desc {
                div {
                    label { class: "form-label", "简介" }
                    MarkdownRenderer { content: d, compact: true }
                }
            }
            if !capabilities.is_empty() {
                div {
                    label { class: "form-label", "能力" }
                    div { class: "flex flex-wrap gap-1",
                        for c in capabilities.iter() {
                            span { key: "{c}", class: "{tag_chip()}", "{c}" }
                        }
                    }
                }
            }
            Link {
                class: "btn hud-btn btn-ghost btn-xs",
                to: crate::pages::Route::HrAgentDetail { id: aid },
                "在详情页打开 →"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::enums::FileType;

    #[test]
    fn tab_needs_live_poll_only_tools_and_thinking() {
        // 项目模式：总览/任务/产物/Agent 走 SSE 或自身接口；工具(4)/思考(5) 才需要低频轮询
        assert!(!tab_needs_live_poll(true, 0));
        assert!(!tab_needs_live_poll(true, 1));
        assert!(!tab_needs_live_poll(true, 2));
        assert!(!tab_needs_live_poll(true, 3));
        assert!(tab_needs_live_poll(true, 4));
        assert!(tab_needs_live_poll(true, 5));
        // 默认对话与项目模式同序（少总览）：任务(0)/产物(1)/Agent(2)/工具(3)/思考(4)
        assert!(!tab_needs_live_poll(false, 0));
        assert!(!tab_needs_live_poll(false, 1));
        assert!(!tab_needs_live_poll(false, 2));
        assert!(tab_needs_live_poll(false, 3));
        assert!(tab_needs_live_poll(false, 4));
        // 越界索引不得被当成工具/思考（否则会凭空多一条轮询通道）
        assert!(!tab_needs_live_poll(true, 6));
        assert!(!tab_needs_live_poll(false, 5));
    }

    #[test]
    fn tab_data_needs_only_marks_consumers() {
        // 项目模式：总览只要项目详情；任务只要任务列表；
        // 产物 / Agent 两者都要；工具 / 思考都不需要（各自取数）
        assert_eq!(tab_data_needs(true, 0), (true, false));
        assert_eq!(tab_data_needs(true, 1), (false, true));
        assert_eq!(tab_data_needs(true, 2), (true, true));
        assert_eq!(tab_data_needs(true, 3), (true, true));
        assert_eq!(tab_data_needs(true, 4), (false, false));
        assert_eq!(tab_data_needs(true, 5), (false, false));
        // 默认对话：任务 / Agent 都消费游离任务列表（Agent 的成员集合由 assignee 推导），
        // 产物是空态占位，工具 / 思考各自取数
        assert_eq!(tab_data_needs(false, 0), (false, true));
        assert_eq!(tab_data_needs(false, 1), (false, false));
        assert_eq!(tab_data_needs(false, 2), (false, true));
        assert_eq!(tab_data_needs(false, 3), (false, false));
        assert_eq!(tab_data_needs(false, 4), (false, false));
    }

    #[test]
    fn tab_judgements_follow_tab_names_not_indexes() {
        // 判定函数按 Tab **名**取值，与渲染用的 tab_names 同源。
        // 这里遍历真实 Tab 序列逐个核对，且**不认识的名字直接 panic** ——
        // 增删或重命名 Tab 时，判定表必须同步更新，不能悄悄落进默认分支
        // （默认是不消费 / 不轮询，看似安全，实则新 Tab 会一直拿不到刷新）。
        for is_project_mode in [true, false] {
            for (index, name) in tab_names(is_project_mode).iter().enumerate() {
                let needs = tab_data_needs(is_project_mode, index);
                match *name {
                    "总览" => assert_eq!(needs, (true, false), "{name}: 只用项目详情"),
                    "任务" => assert_eq!(needs, (false, true), "{name}: 只用任务列表"),
                    // 产物只在项目模式消费这两份，默认对话是硬空态；
                    // Agent 两种模式都消费任务列表（默认对话只需任务列表推导成员集合）
                    "产物" => {
                        assert_eq!(
                            needs,
                            (is_project_mode, is_project_mode),
                            "{name}: 仅项目模式消费"
                        )
                    }
                    "Agent" => {
                        assert_eq!(
                            needs,
                            if is_project_mode {
                                (true, true)
                            } else {
                                (false, true)
                            },
                            "{name}: 成员集合由任务 assignee 推导"
                        )
                    }
                    "工具" | "思考" => {
                        assert_eq!(needs, (false, false), "{name}: 自取数，不陪跑")
                    }
                    other => panic!("新 Tab 名 {other} 未在 tab_data_needs 中显式判定"),
                }

                let live = tab_needs_live_poll(is_project_mode, index);
                match *name {
                    "工具" | "思考" => assert!(live, "{name}: 无 SSE 可等，必须轮询"),
                    "总览" | "任务" | "产物" | "Agent" => {
                        assert!(!live, "{name}: 有 SSE 或自身接口，不该轮询")
                    }
                    other => panic!("新 Tab 名 {other} 未在 tab_needs_live_poll 中显式判定"),
                }
            }
        }
    }

    fn test_artifact(id: &str, task_id: Option<&str>) -> ArtifactDetail {
        ArtifactDetail {
            id: id.to_string(),
            project_id: "p1".to_string(),
            task_id: task_id.map(|s| s.to_string()),
            name: id.to_string(),
            description: String::new(),
            file_type: FileType::Document,
            source_type: ArtifactSourceType::GeneratedContent,
            file_path: String::new(),
            mime_type: String::new(),
            file_size: 0,
            tags: Vec::new(),
            status: 1,
            created_by: String::new(),
            modified_by: String::new(),
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn split_artifacts_empty() {
        let (project_level, task_level) = split_artifacts(&[]);
        assert!(project_level.is_empty());
        assert!(task_level.is_empty());
    }

    #[test]
    fn split_artifacts_mixed() {
        let arts = vec![
            test_artifact("a1", None),
            test_artifact("a2", Some("t1")),
            test_artifact("a3", None),
            test_artifact("a4", Some("t2")),
        ];
        let (project_level, task_level) = split_artifacts(&arts);
        assert_eq!(project_level.len(), 2);
        assert_eq!(task_level.len(), 2);
        assert!(project_level.iter().all(|a| a.task_id.is_none()));
        assert!(task_level.iter().all(|a| a.task_id.is_some()));
    }

    fn test_task_item(
        id: &str,
        title: &str,
        status: i32,
        progress: i32,
        assignee: &str,
    ) -> TaskListItem {
        TaskListItem {
            id: id.to_string(),
            title: title.to_string(),
            description: None,
            status,
            priority: 1,
            tags: Vec::new(),
            root_user_id: String::new(),
            assignee_type: 1,
            assignee_id: assignee.to_string(),
            project_id: None,
            thinking_depth: 0,
            progress,
            created_at: 0,
            updated_at: 0,
            dependencies: Vec::new(),
        }
    }

    #[test]
    fn graph_node_summary_carries_status_progress_assignee() {
        // 点线图（mini）下节点不绘名称卡，任务名/状态/进度/负责人全量由 hover 承载：
        // summary 一行概览必须含三要素（node_hover_lines 通道自动携带）
        let tasks = vec![test_task_item("t1", "任务甲", 2, 40, "01J8ZKQ7X4M2")];
        let (nodes, _edges) = build_task_graph_data(&tasks, TASK_GRAPH_W, TASK_GRAPH_H, true);
        assert_eq!(nodes.len(), 1);
        let summary = nodes[0].summary.as_deref().unwrap_or_default();
        assert!(summary.contains("进行中"), "{summary}");
        assert!(summary.contains("进度 40%"), "{summary}");
        assert!(summary.contains("负责人 Agent·01J8ZKQ7"), "{summary}");
    }

    #[test]
    fn graph_node_summary_handles_unassigned_and_user_assignee() {
        // 未分配 → 占位文案；用户分配对象（assignee_type=0）→ ID 原样展示
        let mut t = test_task_item("t1", "任务甲", 0, 0, "");
        let (nodes, _) =
            build_task_graph_data(std::slice::from_ref(&t), TASK_GRAPH_W, TASK_GRAPH_H, true);
        assert!(
            nodes[0]
                .summary
                .as_deref()
                .unwrap_or_default()
                .contains("负责人 未分配")
        );
        t.assignee_id = "user123".to_string();
        t.assignee_type = 0;
        let (nodes, _) =
            build_task_graph_data(std::slice::from_ref(&t), TASK_GRAPH_W, TASK_GRAPH_H, true);
        assert!(
            nodes[0]
                .summary
                .as_deref()
                .unwrap_or_default()
                .contains("负责人 user123")
        );
    }

    #[test]
    fn group_by_task_keeps_first_seen_order() {
        let arts = [
            test_artifact("a1", Some("t2")),
            test_artifact("a2", Some("t1")),
            test_artifact("a3", Some("t2")),
        ];
        let groups = group_by_task(&arts.iter().collect::<Vec<_>>());
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].0, "t2");
        assert_eq!(groups[0].1.len(), 2);
        assert_eq!(groups[1].0, "t1");
        assert_eq!(groups[1].1.len(), 1);
    }

    #[test]
    fn floating_panel_merge_fills_recent_by_updated_at_desc() {
        // 未完成段原序在前；补齐段去重后按 updated_at 倒序（刚完成的排最前）
        let unfinished = vec![
            test_task_item("u1", "未完成甲", 2, 40, "a"),
            test_task_item("u2", "未完成乙", 1, 0, "a"),
        ];
        let mut recent = vec![
            test_task_item("u1", "未完成甲", 2, 40, "a"), // 与未完成段重复 → 去重
            test_task_item("c1", "已完成旧", 3, 100, "a"),
            test_task_item("c2", "刚完成", 3, 100, "a"),
            test_task_item("c3", "完成中", 3, 100, "a"),
        ];
        recent[1].updated_at = 100;
        recent[2].updated_at = 300;
        recent[3].updated_at = 200;
        let merged = merge_unfinished_with_recent(unfinished, recent, 5);
        let ids: Vec<&str> = merged.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["u1", "u2", "c2", "c3", "c1"]);
    }

    #[test]
    fn floating_panel_merge_caps_at_limit() {
        let unfinished = vec![test_task_item("u1", "未完成", 2, 40, "a")];
        let mut recent = vec![
            test_task_item("c1", "完成近", 3, 100, "a"),
            test_task_item("c2", "完成中", 3, 100, "a"),
            test_task_item("c3", "完成远", 3, 100, "a"),
        ];
        recent[0].updated_at = 300;
        recent[1].updated_at = 200;
        recent[2].updated_at = 100;
        let merged = merge_unfinished_with_recent(unfinished, recent, 2);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].id, "u1");
        assert_eq!(merged[1].id, "c1");
    }

    #[test]
    fn floating_panel_merge_skips_fill_when_unfinished_full() {
        // 未完成段已满 limit：不发起补齐（请求侧同样短路，这里锁合并函数行为）
        let unfinished: Vec<TaskListItem> = (0..3)
            .map(|i| test_task_item(&format!("u{i}"), "未完成", 2, 40, "a"))
            .collect();
        let recent = vec![test_task_item("c1", "刚完成", 3, 100, "a")];
        let merged = merge_unfinished_with_recent(unfinished, recent, 3);
        assert_eq!(merged.len(), 3);
        assert!(merged.iter().all(|t| t.id.starts_with('u')));
    }

    #[test]
    fn floating_panel_merge_truncates_overlong_unfinished() {
        // 防御性截断：未完成段超出 limit（请求侧已带 limit，双保险）
        let unfinished: Vec<TaskListItem> = (0..5)
            .map(|i| test_task_item(&format!("u{i}"), "未完成", 2, 40, "a"))
            .collect();
        let merged = merge_unfinished_with_recent(unfinished, Vec::new(), 3);
        assert_eq!(merged.len(), 3);
    }

    #[test]
    fn floating_panel_merge_handles_empty_recent() {
        let unfinished = vec![test_task_item("u1", "未完成", 2, 40, "a")];
        let merged = merge_unfinished_with_recent(unfinished, Vec::new(), 20);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].id, "u1");
    }
}
