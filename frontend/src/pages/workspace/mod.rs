//! 工作台页面（驾驶舱）
//!
//! 三栏布局：左侧 Project 列表浮层 / 中间固定席位场 / 右侧 Agent 列表浮层
//! 顶部汇总状态条：项目数 / Agent 数 / 运行中项目 / 忙碌 Agent
//!
//! 场景层只渲染一种视图：**固定席位场**（`scene::SceneField`）。席位由常量表决定，
//! 实体状态变化只改变「谁在席上、长什么样」，不移动座位；点击席位进入该 Agent 的
//! 对话上下文（AgentDetail），点空白 / 空席回到全景（Global）。
//! 对话上下文由 `WorkspaceView` 决定（Global / ProjectDetail / AgentDetail /
//! TaskDetail），视图切换只改对话与标题，不再为关系图预取任务数据。
//!
//! 数据加载策略：
//! - 侧边栏：全量加载 projects + agents（轻量）
//! - 场景层：只吃 5 秒轮询的场景投影（Agent 语义状态）
//! - TaskDetail：只拉一次任务归属关系，用于解析项目会话与默认 @ 任务
//!
//! 底部游戏式横幅（MMORPG 风格，左右通透）：
//! - 左下角：当前对话上下文 / 选中 Agent 信息卡（名称 + 运行状态 + 角色标签 + 简介）
//! - 中间：对话框（消息区 + 输入行 + @ 提及浮层）；聚焦与否仅消息区展开高度不同，布局两态一致
//! - 视图联动（三种对话场景 + 一种锚定）：
//!   - Global = 默认对话（后端兜底到前台接待）
//!   - ProjectDetail = 项目会话：消息按 project_id 过滤，发给项目 owner（PMO）
//!   - AgentDetail = Agent 私聊：无项目域，发给该 Agent
//!   - TaskDetail = **项目会话锚定到某任务**：对话上下文与 ProjectDetail 完全一致
//!     （project_id + PMO 收件人，消息连续不割裂），额外做两件事 ——
//!     ① 发送时带 task_id：`messages` 表有该列，PMO 的 prompt 会收到 `【关联任务】`
//!     ② 进入视图时默认 @ 该任务（内容层提示，可一键摘除）
//!
//!   语义是「跟 PMO 说，由 PMO 转达给任务执行 Agent」，不对任务 Agent 做微操。
//! - @ 提及范围跟着对话上下文走（见 `MentionState::new(chat_project_id, reception_agent)`）：
//!   项目会话（ProjectDetail / TaskDetail）= 项目协作 Agent（打头，组织其余打
//!   「未在项目内」缀后）+ 项目内任务 + 项目（当前项目置顶打标）；全局 / Agent
//!   私聊没有项目域 = 组织全量 Agent（接待 Agent 置顶打「当前接待」）+ 全部项目，
//!   任务引导到项目下讨论。
//!
//! 结构拆分（P0）：
//! - `helpers.rs` 纯函数与常量（视图编码 / 对话上下文 / 消息归属）
//! - `hud.rs` 顶部状态栏与左右面板
//! - `dock.rs` 底部横幅
//! - `source.rs` 场景投影数据入口（`SceneSource`）
//!
//! 本文件只保留页面壳：状态、效应（图数据 / 会话 / SSE / 轮询 / 发送）与渲染编排。

use dioxus::prelude::*;
use dioxus_router::use_navigator;

use crate::Route;
use common::api::{
    GetReceptionAgentResponse, GetWorkspaceMetricsRequest, WorkspaceMetricsResponse,
};

use crate::api::hr::get_reception_agent;
use crate::api::message::{load_latest_messages, load_older_messages, send_message_to_agent};
use crate::api::project::{list_project_tasks, query_tasks};
use crate::api::system::get_workspace_metrics;
use crate::components::mention_picker::{MentionCandidate, MentionState};
use crate::components::workspace_graph::WorkspaceView;
use crate::hooks::use_attachments::{compose_outbound_content, use_attachment_composer};
use crate::hooks::use_workspace_data::use_workspace_data;
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use crate::utils::mention::MentionKind;
use crate::utils::{
    HISTORY_PAGE_SIZE, HISTORY_SCAN_MAX_PAGES, build_optimistic_user_msg, request_scope,
};
use common::api::{
    AgentListItem, MessageListItem, PaginationParams, SendMessageToAgentParams, TaskQueryRequest,
};
use common::api::{PresenceDims, SceneAttention, SceneEntity, SceneEntityKind};
use wasm_bindgen::{JsCast, closure::Closure};

mod dock;
mod feed;
mod helpers;
mod hud;
mod live_center;
mod presentation;
mod scene;
mod source;

use helpers::{
    RUNTIME_METRICS_WINDOW_MINUTES, context_agent_filter, filter_in_context, resolve_chat_context,
    view_from_query, view_to_query,
};
use scene::SceneField;
use scene::seats::SEAT_COUNT;
use source::{PollingSource, SCENE_POLL_MS, SceneSource};

/// 用侧栏 Agent 档案补一个场景实体（不在投影里时使用）
fn synthetic_agent(agent: &AgentListItem) -> SceneEntity {
    SceneEntity {
        id: agent.id.clone(),
        kind: SceneEntityKind::Agent,
        display_name: agent.name.clone(),
        dims: PresenceDims {
            category: presentation::category_for_runtime(agent.runtime_state),
            ..Default::default()
        },
        labels: Vec::new(),
        attention: SceneAttention::None,
        last_activity_at: 0,
    }
}

#[component]
pub fn Workspace(view: Option<String>) -> Element {
    let (sidebar_signal, _) = use_workspace_data();
    let navigator = use_navigator();
    let mut current_view = use_signal(|| WorkspaceView::Global);

    // URL 单一事实源：路由 query 解析出的 view prop 回流进 current_view 镜像。
    // 写入侧（图节点点击 / 侧栏切换）只做 navigator.push（返回可逐级回退），
    // signal 统一由本守卫回流；值相等时 set 幂等，收敛无环。
    // use_reactive 把非响应式 prop 装进内部 signal：use_effect 只追踪闭包内的响应式
    // 读取，裸捕获 prop 的 effect 在 prop 变化时不会重跑——页内切换视图会失灵。
    use_effect(use_reactive!(|view| {
        let url_view = view_from_query(&view);
        if current_view() != url_view {
            current_view.set(url_view);
        }
    }));
    let toast = use_toast();
    // 附件上传：与对话页共用同一套状态机（回形针 / 粘贴 / 拖放）
    let attachments = use_attachment_composer(toast);

    // 场景聚焦：点击席位只切换「景深层」的对话对象，不改路由。
    // `effective_view` 是页面内所有对话相关推导的唯一入口：
    // 有聚焦席位 = 该 Agent 私聊；否则跟随路由视图。
    let focus_seat = use_signal(|| Option::<String>::None);
    let mut focus_seat_sig = focus_seat;
    // 最近一次非 Agent 的视图：退出 Agent 专注时回到这里（项目上下文不丢）
    let last_scene_view = use_signal(|| WorkspaceView::Global);
    let mut last_scene_sig = last_scene_view;
    use_effect(move || {
        let v = current_view.read().clone();
        if !matches!(v, WorkspaceView::AgentDetail(_)) {
            last_scene_sig.set(v);
        }
    });
    // 路由变化（侧栏导航 / 浏览器前进后退）优先于席位聚焦：焦点随 URL 一起复位，
    // 避免「地址已是项目会话，画面还停在某个 Agent 上」。
    use_effect(use_reactive!(|view| {
        let _ = view;
        focus_seat_sig.set(None);
    }));
    let effective_view = use_memo(move || match focus_seat() {
        Some(id) => WorkspaceView::AgentDetail(id),
        None => current_view(),
    });

    // 对话框状态
    let chat_messages = use_signal(Vec::<MessageListItem>::new);
    let chat_input = use_signal(String::new);
    let chat_focused = use_signal(|| false);
    let chat_is_typing = use_signal(|| false);
    let chat_project_id = use_signal(|| Option::<String>::None);
    let chat_task_id = use_signal(|| Option::<String>::None);
    let chat_to_agent_id = use_signal(|| Option::<String>::None);
    // 历史分页：是否还有更早的消息 / 是否正在上拉加载（滚到消息区顶部按
    // `before_timestamp` 续拉，因此「首屏 50 条」只是页大小，不是历史深度上限）
    let mut chat_has_more = use_signal(|| true);
    let chat_loading_older = use_signal(|| false);
    // 增量兜底轮询的游标（`created_at` 毫秒）。与列表里的最大时间戳分开记：
    // Agent 私聊的可见性收敛在前端（后端 to/from 是「或」），原始页里可能有大量
    // 不属于本视图的消息 —— 只按「列表最大值」推进会在这些消息上卡住不再前进。
    // 切换视图时置空，回落到列表最大值重新起算。
    let mut poll_cursor = use_signal(|| Option::<i64>::None);

    // 任务视图锚点：(task_id, 任务标题, project_id)，由 TaskDetail 的图数据加载回填。
    // 用途：① 把任务视图解析成「项目会话 + 该任务」（`resolve_chat_context`）
    //       ② 进入任务视图时默认 @ 该任务（见下方「任务视图锚定」effect）
    let mut task_anchor = use_signal(|| Option::<(String, String, Option<String>)>::None);
    // 上一次由锚定 effect 自动写入输入框的**完整文本**，用于判断「内容是否仍是我写的」
    // —— 只有输入框为空、或内容与它完全一致时才允许覆盖，绝不碰用户正在输入的内容。
    let auto_mention = use_signal(|| Option::<String>::None);

    // 接待 Agent：默认对话（无项目域）@ 的「当前接待」置顶打标要用，挂载时拉一次。
    // 失败静默处理（@ 里只是少一个置顶项，发送时后端仍有兜底）。
    let mut reception_agent = use_signal(|| Option::<GetReceptionAgentResponse>::None);
    use_effect(move || {
        spawn(async move {
            if let Ok(resp) = get_reception_agent().await {
                reception_agent.set(Some(resp));
            }
        });
    });

    // @ 提及：与对话页（pages/message/chat.rs）共用同一套状态机（触发判定 / 候选加载 /
    // 键盘导航 / 已提及胶囊）。候选范围直接跟随对话上下文 chat_project_id：
    // - ProjectDetail / TaskDetail = 项目会话口径：项目协作 Agent + 项目内任务 + 项目
    //   （当前项目置顶打标，组织其余 Agent 打「未在项目内」缀后）
    // - Global / AgentDetail = 无项目域：组织全量 Agent（接待 Agent 置顶）+ 项目
    // 也就是说「@ 能选什么」不需要单独一套规则，视图切到哪儿就跟着哪儿。
    let mention = MentionState::new(chat_project_id, reception_agent);

    // 侧边栏红点提示：收到新消息但不在当前视图时，对应 project/agent 亮红点
    let project_unread = use_signal(std::collections::HashSet::<String>::new);
    let agent_unread = use_signal(std::collections::HashSet::<String>::new);

    // 顶栏聚合指标（单一端点，30 秒轮询；首帧到达前为 None，顶栏不渲染避免 0 闪现）
    //
    // 概览计数 + 运行态三色 + 模型/工具窗口读数 + 队列积压，全部出自同一份后端快照。
    let workspace_metrics = use_signal(|| Option::<WorkspaceMetricsResponse>::None);
    // 场景投影（5 秒轮询）：语义层给的「谁需要我」，驱动全景席位场。
    let scene_projection = use_signal(|| Option::<common::api::SceneProjectionResponse>::None);
    // 投影断流标记：任一轮询失败即置位，顶栏显式提示「数据陈旧」
    let scene_stale = use_signal(|| false);
    let runtime_filter = use_signal(|| None::<String>);
    // 左侧项目过滤：全部 / 进行中 / 已完成（与右侧 Agent 状态过滤同款）
    // 默认停在「进行中」：工作台关注的是正在推进的事，归档/已完成默认不占视野
    let project_status_filter = use_signal(|| Some(1));
    // 项目上下文成员：项目 id → (负责人 + 任务执行 Agent)。任务列表异步拉取一次。
    let project_agent_ids =
        use_signal(|| Option::<(String, std::collections::HashSet<String>)>::None);

    // HUD 悬浮面板折叠状态
    let project_panel_collapsed = use_signal(|| false);
    let agent_panel_collapsed = use_signal(|| false);

    let sidebar = sidebar_signal.read().clone();

    // 顶栏聚合指标轮询：30 秒间隔拉单一聚合端点，全部数字出自同一份快照。
    //
    // 注意：统计事件是批次刷盘，最近 1~2 分钟可能尚未落库，读数偏低属预期。
    use_future(move || {
        let mut workspace_metrics = workspace_metrics;
        async move {
            loop {
                let minutes = Some(RUNTIME_METRICS_WINDOW_MINUTES);
                if let Ok(resp) =
                    get_workspace_metrics(GetWorkspaceMetricsRequest { minutes }).await
                {
                    workspace_metrics.set(Some(resp));
                }
                gloo_timers::future::TimeoutFuture::new(30_000).await;
            }
        }
    });

    // 场景投影轮询：5 秒一次；页面不可见时跳过请求。
    // 表现层从投影快照直接推导过渡（颜色 / 高亮），不需要事件流。
    use_future(move || {
        let mut scene_projection = scene_projection;
        let mut scene_stale = scene_stale;
        async move {
            let mut source = PollingSource::new();
            loop {
                if !crate::utils::page_hidden() {
                    match source.poll().await {
                        Ok(projection) => {
                            scene_projection.set(Some(projection));
                            scene_stale.set(false);
                        }
                        Err(_) => scene_stale.set(true),
                    }
                }
                gloo_timers::future::TimeoutFuture::new(SCENE_POLL_MS).await;
            }
        }
    });

    // 任务视图锚点：席位场不消费图数据，只在 TaskDetail 拉一次任务归属关系。
    // 用途：① 把任务视图解析成「项目会话 + 该任务」（`resolve_chat_context`）
    //       ② 进入任务视图时默认 @ 该任务（见下方「任务视图锚定」effect）
    // 其余视图无需额外请求，对话上下文直接由侧边栏数据解析。
    use_effect(move || {
        let view = current_view.read().clone();
        let live_view = current_view;
        let toast = toast;

        spawn(async move {
            let WorkspaceView::TaskDetail(tid) = view else {
                return;
            };
            let req = TaskQueryRequest {
                ids: Some(vec![tid.clone()]),
                pagination: PaginationParams::default(),
                ..Default::default()
            };
            match query_tasks(&req).await {
                Ok(page) => {
                    // 视图已切走：丢弃过期结果，避免旧任务锚点污染新会话
                    if live_view.read().clone() != WorkspaceView::TaskDetail(tid) {
                        return;
                    }
                    if let Some(task) = page.items.into_iter().next() {
                        task_anchor.set(Some((
                            task.id.clone(),
                            task.title.clone(),
                            task.project_id.clone(),
                        )));
                    }
                }
                Err(e) => toast.error(format!("获取 Task 失败: {}", e)),
            }
        });
    });

    // 视图变化时重新加载对话消息 + 更新对话上下文
    {
        let mut chat_messages = chat_messages;
        let mut chat_project_id = chat_project_id;
        let mut chat_task_id = chat_task_id;
        let mut chat_to_agent_id = chat_to_agent_id;
        // 侧栏数据每 30s 被轮询整包重写（use_workspace_data），本 effect 为解析
        // 项目 owner 订阅了它 ⇒ 每次轮询都会重跑。但对话上下文通常没变：若照旧
        // `chat_messages.set(visible)`，会丢掉用户上拉加载的历史，专注态还会跟着
        // `scroll_to_latest()`，表现为周期性「一抖、只剩最新几条」。
        // 只有上下文真正变化（切会话 / owner 变更 / 任务锚点回填）才重载。
        let last_chat_ctx =
            use_signal(|| Option::<(Option<String>, Option<String>, Option<String>)>::None);
        let mut last_chat_ctx = last_chat_ctx;

        use_effect(move || {
            let view = effective_view.read().clone();
            let sidebar_data = sidebar_signal.read().clone();
            // 任务锚点（TaskDetail 专用）：随图数据异步回填，回填后本 effect 自动重跑
            let anchor = task_anchor.read().clone();

            // 计算新的对话上下文
            let (pid, tid, aid) = if let Some(data) = &sidebar_data {
                resolve_chat_context(&view, data, anchor.as_ref())
            } else {
                (None, None, None)
            };
            let ctx = (pid.clone(), tid.clone(), aid.clone());
            if last_chat_ctx.peek().as_ref() == Some(&ctx) {
                return;
            }
            last_chat_ctx.set(Some(ctx));

            // 任务视图的 project_id 依赖异步锚点：未就绪时先不拉消息，
            // 否则会先闪一次「全组织最近 20 条」再被项目会话覆盖。
            if matches!(view, WorkspaceView::TaskDetail(_)) && pid.is_none() {
                chat_project_id.set(None);
                chat_task_id.set(tid.clone());
                chat_to_agent_id.set(None);
                chat_messages.set(Vec::new());
                chat_has_more.set(false);
                poll_cursor.set(None);
                return;
            }

            chat_project_id.set(pid.clone());
            chat_task_id.set(tid.clone());
            chat_to_agent_id.set(aid.clone());
            // 换会话 ⇒ 游标重来（新旧项目的消息时间轴不可比）
            poll_cursor.set(None);

            // 加载首屏历史消息（页大小 HISTORY_PAGE_SIZE，更早的靠上拉续拉）
            let pid_clone = pid.clone();
            // Agent 私聊是唯一「按人收窄」的视图（见 `context_agent_filter`）
            let agent_only = context_agent_filter(&view, aid.clone());
            // 请求侧 project_id（见 `utils::message::request_scope`）：
            // - Global（pid = None，默认对话）→ 哨兵值，后端精确返回 `project_id IS NULL`
            // - Agent 私聊 → 必须保持 None（= 不过滤，另按 to/from 收窄），否则会丢掉
            //   带 project_id 的私聊消息
            // - 项目 / 任务会话 → 项目 id，原样透传
            let req_pid = if matches!(view, WorkspaceView::AgentDetail(_)) {
                None
            } else {
                request_scope(pid.as_deref())
            };
            spawn(async move {
                // 首屏也可能「整页都不可见」：Agent 私聊的可见性收敛在前端
                // （后端 to/from 是「或」关系），一页原始消息有可能被滤到一条不剩，
                // 那时页面既空白、又没有可滚动区域（滚动事件永远不会来）。
                // 因而沿用与上拉一致的策略：整页被滤空就继续往前翻，最多 N 页。
                let mut visible = Vec::new();
                let mut cursor: Option<i64> = None;
                let mut full_page = false;
                for _ in 0..HISTORY_SCAN_MAX_PAGES {
                    let req = common::api::ListMessagesRequest {
                        project_id: req_pid.clone(),
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
                        Err(_) => break,
                    };
                    // 满页 ⇒ 可能还有更早的（来源是 DESC，已由后端反转为 ASC）
                    full_page = resp.messages.len() >= HISTORY_PAGE_SIZE;
                    let next_cursor = resp.messages.first().map(|m| m.created_at);
                    visible = filter_in_context(
                        resp.messages,
                        pid_clone.as_deref(),
                        agent_only.as_deref(),
                    );
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
                chat_messages.set(visible);
                chat_has_more.set(full_page);
                // 展开态或 Agent 专注下切换会话：等一帧让列表重排后回到最新
                if *chat_focused.peek()
                    || matches!(&*effective_view.peek(), WorkspaceView::AgentDetail(_))
                {
                    gloo_timers::future::TimeoutFuture::new(16).await;
                    dock::scroll_to_latest();
                }
            });
        });
    }

    // 上拉加载更早的历史消息（滚到消息区顶部触发）
    //
    // `before_timestamp` 是**开区间**（后端取 `created_at < before`），所以用当前
    // 首条消息的时间戳做游标不会重复拉回同一条。
    // ⚠️ Agent 私聊拿不到精确的服务端过滤（它的 to/from 是「或」关系，后端按
    // project 过滤帮不上忙），一页原始消息可能被过滤到一条不剩。这时如果直接停手，
    // 用户既看不到新内容、也不会再有滚动事件 —— 所以整页被滤空时继续往前翻，
    // 最多 MAX_SCAN 页，避免长卡顿。
    // （Global 自「默认对话哨兵值」改造后已由后端精确返回，这段兜底对它不再触发。）
    let load_older = move || {
        if !chat_has_more() || chat_loading_older() {
            return;
        }
        let pid = chat_project_id();
        let view = effective_view.read().clone();
        let agent_only = context_agent_filter(&view, chat_to_agent_id());
        // 请求侧 project_id：与首屏同口径（Global 用哨兵、Agent 私聊保持 None）
        let req_pid = if matches!(view, WorkspaceView::AgentDetail(_)) {
            None
        } else {
            request_scope(pid.as_deref())
        };
        let Some(mut before) = chat_messages.read().first().map(|m| m.created_at) else {
            return;
        };
        const MAX_SCAN_PAGES: usize = HISTORY_SCAN_MAX_PAGES;

        let mut chat_messages = chat_messages;
        let mut chat_has_more = chat_has_more;
        let mut chat_loading_older = chat_loading_older;
        chat_loading_older.set(true);
        spawn(async move {
            for _ in 0..MAX_SCAN_PAGES {
                let resp = match load_older_messages(common::api::ListMessagesRequest {
                    project_id: req_pid.clone(),
                    before_timestamp: Some(before),
                    limit: Some(HISTORY_PAGE_SIZE),
                    ..Default::default()
                })
                .await
                {
                    Ok(resp) => resp,
                    Err(e) => {
                        toast.error(format!("加载更早消息失败: {e}"));
                        break;
                    }
                };

                let full_page = resp.messages.len() >= HISTORY_PAGE_SIZE;
                // 本页最早的时间戳（ASC 排序）→ 下一页的游标；先取再消费 messages
                let next_before = resp.messages.first().map(|m| m.created_at);
                let older = filter_in_context(resp.messages, pid.as_deref(), agent_only.as_deref());

                let has_visible = !older.is_empty();
                if has_visible {
                    let mut current = chat_messages.write();
                    let mut merged = older;
                    merged.append(&mut current);
                    *current = merged;
                }

                // 不满页 ⇒ 到头了；否则用本页游标继续（整页被滤空时也要继续）
                if !full_page {
                    chat_has_more.set(false);
                    break;
                }
                if has_visible {
                    break;
                }
                match next_before {
                    Some(ts) => before = ts,
                    None => {
                        chat_has_more.set(false);
                        break;
                    }
                }
            }
            chat_loading_older.set(false);
        });
    };

    // 任务视图锚定：进入某任务视图时默认在输入框 @ 该任务（内容层提示，可一键摘除）。
    // 结构化通道由 `task_id` 承担（发送时随消息落库，PMO 的 prompt 带【关联任务】），
    // 这里补的是「人也能一眼看到当前在说哪个任务」。
    //
    // 只在「输入框为空」或「内容与上一次自动写入的完全一致」时才写入 —— 绝不打断用户输入。
    // 读输入用 peek（不订阅），否则每次打字都会重跑本 effect。
    {
        let mut chat_input = chat_input;
        let mut auto_mention = auto_mention;
        use_effect(move || {
            let Some((tid, title, _)) = task_anchor.read().clone() else {
                return;
            };
            // 仅当对话上下文确实停在该任务上才锚定（席位聚焦 / 图数据回填都可能滞后）
            let is_current =
                matches!(&*effective_view.read(), WorkspaceView::TaskDetail(v) if v == &tid);
            if !is_current {
                return;
            }

            let current = chat_input.peek().clone();
            let trimmed = current.trim();
            let is_mine = auto_mention.peek().as_deref().map(str::trim) == Some(trimmed);
            if !trimmed.is_empty() && !is_mine {
                return;
            }

            let item = MentionCandidate {
                kind: MentionKind::Task,
                id: tid.clone(),
                org: None,
                name: title.clone(),
                subtitle: String::new(),
                flag: None,
            };
            // 换任务时先摘掉上一轮自动写入的提及，避免残留或重复
            mention.reset_picked();
            let text = format!("{} ", mention.preset(item));
            auto_mention.set(Some(text.clone()));
            chat_input.set(text);
        });
    }

    // 实时消息：SSE + 可见时兜底轮询（过滤口径与首屏 / 上拉共用，详见 `feed.rs`）
    feed::use_message_feed(feed::MessageFeedSignals {
        messages: chat_messages,
        project_id: chat_project_id,
        task_id: chat_task_id,
        to_agent_id: chat_to_agent_id,
        poll_cursor,
        view: effective_view,
        project_unread,
        agent_unread,
    });

    // 发送消息的信号触发器
    let send_trigger = use_signal(|| false);

    // 发送消息
    {
        let mut chat_input = chat_input;
        let mut chat_messages = chat_messages;
        let mut chat_is_typing = chat_is_typing;
        let mut send_trigger = send_trigger;
        let mut auto_mention = auto_mention;
        let mut attachments = attachments;

        use_effect(move || {
            if !*send_trigger.read() {
                return;
            }
            send_trigger.set(false);

            let text = chat_input.read().trim().to_string();
            let pending = attachments.pending.peek().clone();
            if text.is_empty() && pending.is_empty() {
                return;
            }

            let pid = chat_project_id.read().clone();
            let tid = chat_task_id.read().clone();
            let aid = chat_to_agent_id.read().clone();
            let text_snapshot = text.clone();
            let pending_snapshot = pending.clone();
            // 附件引用在请求侧写进正文，attachment_ids 同时带给后端做归属校验
            let composed = compose_outbound_content(&text, &pending);
            let attachment_ids: Vec<String> = pending.iter().map(|a| a.id.clone()).collect();
            let attachment_ids_opt = (!attachment_ids.is_empty()).then_some(attachment_ids);

            chat_input.set(String::new());
            attachments.pending.set(Vec::new());
            // 提及已随正文一起发出，「已提及」记录只服务于本次输入；
            // 锚定记录一并清空，否则下次会自动复用上一次的判定基准
            mention.reset_picked();
            auto_mention.set(None);
            chat_is_typing.set(true);

            {
                let mut chat_is_typing = chat_is_typing;
                // 一次性延时复位：用 callback::Timeout + forget() 泄露安全，
                // 避免 spawn + TimeoutFuture 在组件卸载时 drop 触发 "closure invoked recursively or after being dropped"。
                gloo_timers::callback::Timeout::new(60_000, move || {
                    chat_is_typing.set(false);
                })
                .forget();
            }

            spawn(async move {
                let req = SendMessageToAgentParams {
                    to_agent_id: aid.clone(),
                    content: composed.clone(),
                    project_id: pid.clone(),
                    task_id: tid.clone(),
                    reply_to_id: None,
                    attachment_ids: attachment_ids_opt,
                    notify_only: None,
                };

                match send_message_to_agent(req).await {
                    Ok(resp) => {
                        // 用响应里的真实 message_id 覆盖 tmp_ ID（与 chat / agent_detail
                        // 同口径）：否则 Agent 回复的 reply_to_id 在本地查不到目标，
                        // 引用块不渲染；SSE 去重也只能退化成按 content 比对。
                        let mut user_msg = build_optimistic_user_msg(composed, pid, tid, aid);
                        user_msg.message_id = resp.message_id;
                        chat_messages.write().push(user_msg);
                        // 自己发的消息：展开态或专注态无条件回到底部
                        if *chat_focused.peek()
                            || matches!(&*effective_view.peek(), WorkspaceView::AgentDetail(_))
                        {
                            gloo_timers::future::TimeoutFuture::new(16).await;
                            dock::scroll_to_latest();
                        }
                    }
                    Err(e) => {
                        chat_input.set(text_snapshot);
                        attachments.pending.set(pending_snapshot);
                        toast.error(format!("发送消息失败: {}", e));
                        chat_is_typing.set(false);
                    }
                }
            });
        });
    }

    // 项目上下文：工位场只保留该项目成员（负责人 + 任务执行 Agent）
    {
        let mut project_agent_ids = project_agent_ids;
        use_effect(move || {
            let view = effective_view.read().clone();
            let sidebar = sidebar_signal.read().clone();
            let WorkspaceView::ProjectDetail(pid) = view else {
                // Agent 专注是项目的子场景：保留成员集合，退出时不闪「只剩负责人」
                // （回到项目后本 effect 仍会重跑并刷新，这里只负责过渡平滑）
                if !matches!(view, WorkspaceView::AgentDetail(_)) {
                    project_agent_ids.set(None);
                }
                return;
            };
            let owner = sidebar
                .as_ref()
                .and_then(|d| d.projects.iter().find(|p| p.id == pid))
                .and_then(|p| p.owner_agent_id.clone());
            spawn(async move {
                let mut ids = std::collections::HashSet::new();
                if let Some(owner) = owner {
                    ids.insert(owner);
                }
                if let Ok(resp) = list_project_tasks(&pid).await {
                    for task in resp.tasks {
                        // AssigneeType::Agent == 1
                        if task.assignee_type == 1 && !task.assignee_id.is_empty() {
                            ids.insert(task.assignee_id);
                        }
                    }
                }
                // 切换守卫：项目已换则不写回过期结果
                if matches!(&*effective_view.peek(), WorkspaceView::ProjectDetail(current) if current == &pid)
                {
                    project_agent_ids.set(Some((pid, ids)));
                }
            });
        });
    }

    // 展开对话框时定位到最新消息：收起态高度不足以滚动（scrollTop 恒为 0），
    // 展开后若不主动滚动，会停在当前加载的最早一条上。
    {
        let mut was_focused = use_signal(|| false);
        use_effect(move || {
            let focused = *chat_focused.read();
            // Agent 专注是全屏聊天：同样需要「打开即最新」
            let agent_view = matches!(&*effective_view.read(), WorkspaceView::AgentDetail(_));
            let active = focused || agent_view;
            let expanded = active && !*was_focused.peek();
            was_focused.set(active);
            if expanded {
                spawn(async move {
                    // 等一帧：展开后的高度与新消息要先生效
                    gloo_timers::future::TimeoutFuture::new(16).await;
                    dock::scroll_to_latest();
                });
            }
        });
    }

    // 底部横幅的上下文：视图、标题、选中 Agent、是否展示收件人
    let dock_view = effective_view.read().clone();
    // Agent 专注：整块铺满，隐藏 HUD 与侧栏，只留聊天 + 预览
    let agent_focus = matches!(&dock_view, WorkspaceView::AgentDetail(_));
    // 专注浮层的轮询目标：浮层自己维护运行时循环，这里只告诉它「盯谁」。
    // 不用 selected_agent（依赖侧栏档案是否加载到），避免深链时轮询被拖住。
    let focus_agent_id = match &dock_view {
        WorkspaceView::AgentDetail(aid) => Some(aid.clone()),
        _ => None,
    };
    // Agent 之间也会互相说话，气泡头部拼出接收方（复用正文 @ 提及的 chip
    // 写法），配合旁观消息降透明度，一眼看清谁在跟谁聊。
    // 专注态与群聊视图都需要；只有默认 1:1 对话收件人显然，不重复展示。
    let show_receiver = matches!(
        dock_view,
        WorkspaceView::ProjectDetail(_)
            | WorkspaceView::TaskDetail(_)
            | WorkspaceView::AgentDetail(_)
    );
    let chat_title: String = match &dock_view {
        WorkspaceView::Global => "默认对话".to_string(),
        WorkspaceView::ProjectDetail(pid) => {
            let name = sidebar
                .as_ref()
                .and_then(|d| d.projects.iter().find(|p| &p.id == pid))
                .map(|p| p.name.as_str())
                .unwrap_or("项目对话");
            format!("{} · 项目对话", name)
        }
        WorkspaceView::AgentDetail(aid) => {
            let name = sidebar
                .as_ref()
                .and_then(|d| d.agents.iter().find(|a| &a.id == aid))
                .map(|a| a.name.as_str())
                .unwrap_or("Agent");
            format!("{} · Agent 对话", name)
        }
        WorkspaceView::TaskDetail(tid) => {
            // 有锚点用真实标题（更可读），否则退回 ID 前 8 位
            match task_anchor().filter(|(anchor_tid, _, _)| anchor_tid == tid) {
                Some((_, title, _)) => format!("{} · 项目对话", title),
                None => format!("任务 {} · 对话", &tid[..tid.len().min(8)]),
            }
        }
    };
    // 左下角信息卡：AgentDetail 视图展示选中 Agent；其余视图展示对话上下文
    let selected_agent = match &dock_view {
        WorkspaceView::AgentDetail(aid) => sidebar
            .as_ref()
            .and_then(|d| d.agents.iter().find(|a| &a.id == aid))
            .cloned(),
        _ => None,
    };
    let on_load_older = Callback::new(move |_: ()| load_older());
    let on_navigate = Callback::new(move |v: WorkspaceView| {
        // 显式导航优先：清掉席位聚焦，让对话跟随路由
        focus_seat_sig.set(None);
        navigator.push(Route::Workspace {
            view: view_to_query(&v),
        });
    });
    // 退出 Agent 专注：席位聚焦先清焦点；路由进入的回最近一次场景视图
    let on_exit_focus = Callback::new(move |_: ()| {
        if focus_seat.peek().is_some() {
            focus_seat_sig.set(None);
            return;
        }
        let target = last_scene_view.peek().clone();
        navigator.push(Route::Workspace {
            view: view_to_query(&target),
        });
    });
    // 席位场点击 → 就地聚焦该 Agent（对话在底部横幅完成，不跳路由）；
    // 点空白 / 空席 → 退出聚焦回到全景
    let mut agent_unread = agent_unread;
    let on_scene_select = Callback::new(move |id: Option<String>| match id {
        Some(id) => {
            // 打开对话即视为已读：红点与席位聚焦保持一致
            agent_unread.write().remove(&id);
            focus_seat_sig.set(Some(id));
        }
        None => {
            focus_seat_sig.set(None);
            // 路由停在 AgentDetail（深链 / 侧栏进入）时，点空白回到进入前的场景，
            // 与 Esc / ✕ 的退出目标保持一致（而不是无条件回全局）
            if matches!(&*current_view.peek(), WorkspaceView::AgentDetail(_)) {
                let target = last_scene_view.peek().clone();
                navigator.push(Route::Workspace {
                    view: view_to_query(&target),
                });
            }
        }
    });
    // Esc 退出聚焦：先清席位聚焦；路由停在 AgentDetail 时回到全景。
    // 监听挂在 document 上（页面级快捷键），卸载时移除，避免误触发已销毁的作用域。
    {
        #[allow(clippy::type_complexity)]
        let mut esc_listener = use_signal(|| {
            Option::<(
                web_sys::Document,
                Closure<dyn FnMut(web_sys::KeyboardEvent)>,
            )>::None
        });
        let mut focus_seat = focus_seat;
        use_effect(move || {
            let Some(window) = web_sys::window() else {
                return;
            };
            let Some(document) = window.document() else {
                return;
            };
            let cb = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
                move |evt: web_sys::KeyboardEvent| {
                    if evt.key() != "Escape" {
                        return;
                    }
                    if focus_seat.peek().is_some() {
                        focus_seat.set(None);
                    } else if matches!(&*current_view.peek(), WorkspaceView::AgentDetail(_)) {
                        let target = last_scene_view.peek().clone();
                        navigator.push(Route::Workspace {
                            view: view_to_query(&target),
                        });
                    }
                },
            );
            let _ =
                document.add_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref());
            esc_listener.set(Some((document, cb)));
        });
        use_drop(move || {
            if let Some((document, cb)) = esc_listener.take() {
                let _ = document
                    .remove_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref());
            }
        });
    }
    // 席位场：所有视图共用同一张「现场」，聚焦时高亮并推进镜头
    let scene_selected = match &dock_view {
        WorkspaceView::AgentDetail(aid) => Some(aid.clone()),
        _ => None,
    };
    let scene_entities = {
        let mut entities = scene_projection
            .read()
            .as_ref()
            .map(|p| p.entities.clone())
            .unwrap_or_default();

        // 项目上下文：工位场只留该项目成员（负责人 + 任务执行 Agent）
        if let WorkspaceView::ProjectDetail(pid) = &dock_view {
            let owner = sidebar
                .as_ref()
                .and_then(|d| d.projects.iter().find(|p| &p.id == pid))
                .and_then(|p| p.owner_agent_id.clone());
            let mut ids: std::collections::HashSet<String> = match project_agent_ids.read().clone()
            {
                Some((cached_pid, ids)) if &cached_pid == pid => ids,
                _ => std::collections::HashSet::new(),
            };
            if let Some(owner) = &owner {
                ids.insert(owner.clone());
            }
            entities.retain(|e| ids.contains(&e.id));

            // 不在全局投影里的成员（多数 Idle Agent 不在前 8）就地补位
            let mut wanted: Vec<String> = owner.iter().cloned().collect();
            for id in &ids {
                if !wanted.contains(id) {
                    wanted.push(id.clone());
                }
            }
            for id in wanted {
                if entities.iter().any(|e| e.id == id) {
                    continue;
                }
                if let Some(agent) = sidebar
                    .as_ref()
                    .and_then(|d| d.agents.iter().find(|a| a.id == id))
                {
                    entities.push(synthetic_agent(agent));
                }
            }
            if let Some(owner) = &owner {
                entities.sort_by_key(|e| if &e.id == owner { 0 } else { 1 });
            }
            entities.truncate(SEAT_COUNT);
        }

        // 投影按关注度截断到 8 席，选中的 Agent 可能不在其中。
        // 补一位「被选中的人」并挤掉排名最后的一位，保证点谁谁在场上。
        if let Some(aid) = &scene_selected
            && !entities.iter().any(|e| &e.id == aid)
            && let Some(agent) = sidebar
                .as_ref()
                .and_then(|d| d.agents.iter().find(|a| &a.id == aid))
        {
            if entities.len() >= SEAT_COUNT {
                entities.pop();
            }
            entities.push(synthetic_agent(agent));
        }
        entities
    };

    rsx! {
        AppLayout {
            // 修复：原写法 `relative h-full w-full overflow-hidden` 在 `AppLayout` 的
            // `min-h-screen flex flex-col` + main `flex-1` 父容器下，`h-full` 因为父
            // 高度 `min-height:100vh` + `height:auto` 对子级百分比解析为不定（definite
            // size）而坍塌为 0，导致该 div 内部所有 `absolute inset-0` 子元素（关系图
            // 全屏背景 / HUD 面板 / 加载遮罩）被 `overflow-hidden` 裁掉，页面整片空白。
            // 改用 `absolute inset-0`：锚定到 `position:relative` 的 main，绕过百分比
            // 高度解析、必然填满 `100vh - navbar` 区域（main 由 flex-1 保证该高度）。
            // 场景层同样用 `absolute inset-0` 铺满，不依赖 `h-full`。
            div { class: "absolute inset-0 overflow-hidden",
                // === 场景层：固定席位场（所有视图共用，不随数据重排移动座位） ===
                div { class: "absolute inset-0 z-0",
                    SceneField {
                        entities: scene_entities.clone(),
                        selected: scene_selected,
                        on_select: on_scene_select,
                    }
                }

                // Agent 专注时隐藏 HUD 与侧栏，屏蔽杂音
                if !agent_focus {
                    // === 顶部状态栏（玻璃，悬浮顶部） ===
                    {hud::top_bar(workspace_metrics, scene_projection, scene_stale)}

                    // === 左侧项目面板（玻璃，悬浮左，可折叠） ===
                    if let Some(d) = sidebar.as_ref() {
                        {hud::project_panel(
                            d.projects.clone(),
                            dock_view.clone(),
                            project_panel_collapsed,
                            project_status_filter,
                            project_unread,
                            on_navigate,
                        )}
                    }

                    // === 右侧 Agent 面板（玻璃，悬浮右，可折叠 + 状态过滤） ===
                    if let Some(d) = sidebar.as_ref() {
                        {hud::agent_panel(
                            d.agents.clone(),
                            dock_view.clone(),
                            agent_panel_collapsed,
                            runtime_filter,
                            agent_unread,
                            on_navigate,
                        )}
                    }
                }

                // === 底部横幅（MMORPG 式：左下 Agent 卡 / 中间对话框 / 右下预留，左右通透） ===
                {dock::bottom_dock(
                    chat_title,
                    selected_agent,
                    show_receiver,
                    agent_focus,
                    on_exit_focus,
                    chat_messages,
                    chat_focused,
                    chat_is_typing,
                    chat_input,
                    chat_project_id,
                    attachments,
                    mention,
                    auto_mention,
                    send_trigger,
                    on_load_older,
                    focus_agent_id,
                )}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::helpers::{view_from_query, view_to_query};
    use crate::components::workspace_graph::WorkspaceView;

    #[test]
    fn view_query_round_trip_preserves_all_variants() {
        let views = vec![
            WorkspaceView::Global,
            WorkspaceView::ProjectDetail("prj-01".to_string()),
            WorkspaceView::AgentDetail("agt-01".to_string()),
            WorkspaceView::TaskDetail("tsk-01".to_string()),
        ];
        for view in views {
            let encoded = view_to_query(&view);
            assert_eq!(view_from_query(&encoded), view);
        }
    }

    #[test]
    fn global_view_encodes_to_none() {
        // Global 缺省不产生 query，URL 保持 `/workspace` 干净
        assert_eq!(view_to_query(&WorkspaceView::Global), None);
    }

    #[test]
    fn unknown_query_falls_back_to_global() {
        // 旧链接 / 手改 URL：未知编码回落全局视图，不卡死空状态
        assert_eq!(
            view_from_query(&Some("bogus:xyz".to_string())),
            WorkspaceView::Global
        );
        assert_eq!(
            view_from_query(&Some("project".to_string())),
            WorkspaceView::Global
        );
        assert_eq!(view_from_query(&None), WorkspaceView::Global);
    }
}
