//! 项目内 Agent 列表（ProjectAgentsTab，聊天侧栏项目模式 Agent Tab 专属）
//!
//! ## 背景
//! 项目会话右侧信息面板的 Agent Tab 此前复用单值 `AgentInfoTab`，
//! 永远只显示项目 owner（PMO）一个 Agent。本组件把该 Tab 改造为
//! 「项目内 Agent 列表」：谁在项目里干活（任务 assignee 推导）、
//! 项目内身份（负责人置顶打标）、点击进入单 Agent 详情子页。
//!
//! ## 数据口径（对齐 mention_picker::load_project_agents）
//! 项目没有成员表，「项目内 Agent」的唯一事实源 = 项目任务 assignee
//! （assignee_type==1 去重，用户 assignee 不展示）→ `query_agents(ids)`
//! 批量取详情。本组件不自己拉任务列表：复用面板已拉取的 `TaskListItem`
//! 推导 agent_ids，零额外项目请求；仅 Agent 详情走一次 `query_agents` 批量。
//!
//! ## 视图状态机（T2 设计说明 §2.1）
//! Loading（详情未就绪）→ Empty / Ready(list) ⇄ Detail(agent_id)。
//! list / detail 是同一 Tab 内的两个视图态（面板内切换，不走路由）；
//! Detail 原样复用 [`AgentInfoTab`]，返回列表时保留选中高亮。

use std::collections::HashSet;
use std::time::Duration;

use dioxus::prelude::*;
use wasm_bindgen::JsCast;

use crate::api::hr::query_agents;
use crate::components::chat::chat_side_panel::{AgentInfoTab, loading_placeholder};
use crate::utils::status::{agent_runtime_badge, agent_runtime_text, short_id, tag_chip};
use crate::utils::{avatar_initials, avatar_status_ring};
use common::api::{
    AgentListItem, AgentQueryRequest, GetAgentResponse, PaginationParams, TaskListItem,
};

/// Agent 详情批量查询上限（mention_picker CANDIDATE_LIMIT 同量级；项目任务有限不会触顶）
const AGENT_QUERY_LIMIT: usize = 50;

/// 防抖刷新等待时长（毫秒），与 ChatSidePanel 保持一致
const REFRESH_DEBOUNCE_MS: u64 = 2000;

/// 从任务列表推导项目内 Agent ID（assignee_type==1 去重，保持首次出现顺序）
fn agent_ids_from_tasks(tasks: &[TaskListItem]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut ids = Vec::new();
    for t in tasks {
        if t.assignee_type == 1 && seen.insert(t.assignee_id.clone()) {
            ids.push(t.assignee_id.clone());
        }
    }
    ids
}

/// 列表排序：负责人置顶第一，其余按名称字典序（T2 设计说明 §2.4）
fn sort_project_agents(items: &mut [AgentListItem], owner_id: Option<&str>) {
    items.sort_by(|a, b| {
        let a_owner = owner_id == Some(a.id.as_str());
        let b_owner = owner_id == Some(b.id.as_str());
        b_owner.cmp(&a_owner).then_with(|| a.name.cmp(&b.name))
    });
}

/// 项目内 Agent 列表 Tab
///
/// - `project_owner_id`：项目负责人 Agent ID（列表置顶 + 「负责人」徽标）
/// - `tasks`：面板已拉取的项目任务列表（assignee 推导的唯一事实源）
/// - `refresh_tick`：SSE + 手动刷新计数器，变化时防抖重取 Agent 详情
/// - `agent_info`：chat 主链路轮询共享的 Agent 详情，详情子页优先消费（id 匹配零请求）
/// - `agent_stats_tick`：Agent 统计刷新驱动（SSE + 手动 + 30s 周期叠加，透传 AgentInfoTab）
#[component]
pub fn ProjectAgentsTab(
    project_owner_id: Option<String>,
    tasks: Vec<TaskListItem>,
    refresh_tick: u64,
    agent_info: Signal<Option<GetAgentResponse>>,
    agent_stats_tick: u64,
) -> Element {
    let agent_ids = agent_ids_from_tasks(&tasks);
    let ids_key = agent_ids.join(",");

    let mut agents = use_signal(Vec::<AgentListItem>::new);
    let mut failed = use_signal(|| false);
    // 视图状态机：view_agent=Some(id) → Detail；None → list。
    // last_selected 与 view 分离：返回列表时 view 清空但选中高亮保留（T2 §2.6）。
    let mut view_agent = use_signal(|| None::<String>);
    let mut last_selected = use_signal(|| None::<String>);

    // 详情加载 + 刷新：集合变化 → 立即取；refresh_tick 变化 → 防抖重取。
    // 守卫结构与 ToolCallsTab 一致：仅值真正变化时写回，避免 effect 自触发循环。
    let ids_for_effect = ids_key.clone();
    let ids_for_refresh = agent_ids.clone();
    let mut prev_ids = use_signal(String::new);
    let mut prev_tick = use_signal(|| 0u64);

    let mut load_agents = move |ids: Vec<String>, debounce: bool| {
        if ids.is_empty() {
            agents.set(Vec::new());
            failed.set(false);
            return;
        }
        // 代际键：防抖窗口内集合再变时，旧请求醒来即丢弃（prev_ids 已被 effect 更新为新键）
        let key = ids.join(",");
        spawn(async move {
            if debounce {
                gloo_timers::future::sleep(Duration::from_millis(REFRESH_DEBOUNCE_MS)).await;
                // 代际校验：防抖窗口内集合又变了，本次过期丢弃
                if prev_ids() != key {
                    return;
                }
            }
            let req = AgentQueryRequest {
                ids: Some(ids),
                pagination: PaginationParams {
                    limit: Some(AGENT_QUERY_LIMIT),
                    offset: None,
                },
                ..Default::default()
            };
            match query_agents(&req).await {
                Ok(page) => {
                    agents.set(page.items);
                    failed.set(false);
                }
                Err(_) => failed.set(true),
            }
        });
    };

    use_effect(move || {
        let tick = refresh_tick;
        let ids_changed = prev_ids() != ids_for_effect;
        let tick_changed = prev_tick() != tick;
        if ids_changed {
            prev_ids.set(ids_for_effect.clone());
        }
        if tick_changed {
            prev_tick.set(tick);
        }
        if ids_changed {
            // 项目切换 / 任务集合变化：复位视图与选中高亮，避免残留上个项目的状态
            view_agent.set(None);
            last_selected.set(None);
            load_agents(ids_for_refresh.clone(), false);
        } else if tick_changed && !ids_for_refresh.is_empty() {
            load_agents(ids_for_refresh.clone(), true);
        }
    });

    // ---- 渲染分支（全部 hook 已在上方完成，早退安全）----

    // 加载中：首次未拿到详情（集合非空但一个详情都没有且未失败）
    if !agent_ids.is_empty() && agents.read().is_empty() && !failed() {
        return loading_placeholder();
    }
    if failed() {
        return rsx! {
            div { class: "text-center py-12 text-base-content/60 text-sm", "Agent 信息加载失败" }
        };
    }
    // 空态（T2 §2.5）：项目还没有任何 Agent assignee
    if agent_ids.is_empty() {
        return rsx! {
            div { class: "text-center py-12",
                div { class: "text-sm text-base-content/60", "项目暂无参与的 Agent" }
                div { class: "text-xs text-base-content/40 mt-1",
                    "给任务分配 Agent 后，会在这里显示"
                }
            }
        };
    }
    // Detail 视图：返回行 + AgentInfoTab 原样（T2 §2.6：详情滚动位置重置到面板顶部）
    if let Some(agent_id) = view_agent() {
        let detail_agent_id = agent_id.clone();
        return rsx! {
            div {
                class: "space-y-2",
                onmounted: move |evt: MountedEvent| {
                    if let Some(el) = evt.data().downcast::<web_sys::Element>() {
                        reset_panel_scroll_top(el);
                    }
                },
                button {
                    class: "btn hud-btn btn-ghost btn-xs",
                    onclick: move |_| view_agent.set(None),
                    "← 返回列表"
                }
                AgentInfoTab {
                    agent_id: detail_agent_id,
                    shared_info: agent_info,
                    refresh_tick: agent_stats_tick,
                }
            }
        };
    }

    // Ready(list)：负责人置顶 + 名称字典序；负责任务名列表由任务列表过滤（展开区数据源）
    let mut items: Vec<AgentListItem> = agents.read().clone();
    let owner_id = project_owner_id.clone();
    sort_project_agents(&mut items, owner_id.as_deref());

    rsx! {
        div { class: "space-y-0.5",
            for a in items.iter() {
                {
                    let a = a.clone();
                    let aid_open = a.id.clone();
                    let is_owner = owner_id.as_deref() == Some(a.id.as_str());
                    let is_selected = last_selected() == Some(a.id.clone());
                    let task_titles: Vec<String> = tasks
                        .iter()
                        .filter(|t| t.assignee_type == 1 && t.assignee_id == a.id)
                        .map(|t| t.title.clone())
                        .collect();
                    rsx! {
                        ProjectAgentRow {
                            key: "{aid_open}",
                            agent: a,
                            is_owner,
                            task_titles,
                            selected: is_selected,
                            on_open: move |_| {
                                view_agent.set(Some(aid_open.clone()));
                                last_selected.set(Some(aid_open.clone()));
                            },
                        }
                    }
                }
            }
        }
    }
}

/// 项目内 Agent 列表行（T2 设计说明 §2.2 + 2026-09-23/24 AMan 三轮反馈拍板布局）
///
/// 上半 = 40px 圆形头像（对齐详情页 agent_identity_row）+ 名称加粗横排
/// + 行尾标签区（运行时状态/负责人/角色全保留，≤3 枚 nowrap）；
/// 下半 = 左侧介绍两行截断（hover 原生 tooltip 看全文）+ 右侧任务数按钮恒在
/// （两块完整切分，不受左侧文本截断影响）；点按钮在行底展开/收起
/// 「1. 2. 3.」编号任务列表；Agent 行之间以加深细分隔线区分（input.css）。
/// hover / 选中 / focus 态由 `.project-agent-item` 及其修饰类提供（input.css）。
#[component]
fn ProjectAgentRow(
    agent: AgentListItem,
    is_owner: bool,
    task_titles: Vec<String>,
    selected: bool,
    on_open: Callback,
) -> Element {
    // 名称以详情为准，空名回退短 ID（可读性兜底，同 IdentityChip 口径）
    let name = if agent.name.trim().is_empty() {
        short_id(&agent.id)
    } else {
        agent.name.clone()
    };
    let ring = avatar_status_ring(agent.status);
    // 展开态：行内独立（项目切换行重建自动复位，无需父级清理）
    let mut expanded = use_signal(|| false);
    // 介绍：固定两行截断展示，hover 原生 tooltip 看全文；空介绍回退「暂无介绍」（零额外请求）
    let desc_text = agent
        .description
        .clone()
        .filter(|d| !d.trim().is_empty())
        .unwrap_or_else(|| "暂无介绍".to_string());
    let task_total = task_titles.len();
    // 徽标区配额 ≤3 枚 nowrap：负责人 + 运行时状态 + 角色（is_owner 1 枚角色 /
    // 非 owner 2 枚），超出截断
    let role_take = if is_owner { 1 } else { 2 };
    let roles: Vec<String> = agent.roles.iter().take(role_take).cloned().collect();
    // 运行时状态徽标（utils/status.rs 单一事实源）：空闲/休息中/忙碌，并入行尾 Tag 区
    let runtime_badge = agent_runtime_badge(agent.runtime_state);
    let runtime_text = agent_runtime_text(agent.runtime_state);
    rsx! {
        div {
            class: if selected { "project-agent-item is-selected" } else { "project-agent-item" },
            role: "button",
            tabindex: 0,
            onclick: move |_| on_open.call(()),
            onkeydown: move |evt: KeyboardEvent| match evt.key() {
                Key::Enter => on_open.call(()),
                Key::Character(c) if c == " " => on_open.call(()),
                _ => {}
            },
            // 首行：头像 + 名称横排，行尾徽标区铺右（负责人/状态/角色 ≤3 枚 nowrap）
            div { class: "project-agent-item-head",
                // 40px 头像：对齐详情页 agent_identity_row（w-10 h-10 rounded-full + font-bold）
                div { class: "w-10 h-10 rounded-full bg-secondary text-secondary-content flex items-center justify-center font-bold {ring}",
                    "{avatar_initials(&name)}"
                }
                div { class: "flex-1 min-w-0 text-sm font-semibold truncate", title: "{name}", "{name}" }
                div { class: "project-agent-item-badges",
                    if is_owner {
                        span { class: "badge hud-badge badge-primary badge-xs", "负责人" }
                    }
                    span { class: "{runtime_badge}", "{runtime_text}" }
                    for role in roles {
                        span { key: "{role}", class: "{tag_chip()}", "{role}" }
                    }
                }
            }
            // 下半部分（AMan 拍板 2026-09-24）：左侧 = 介绍两行截断（hover 看全文），
            // 右侧 = 任务数按钮恒在（两块完整切分，不受左侧文本截断影响）；
            // 按钮点击展开/收起行底任务列表，动作不得触发整行「打开详情」/行级键盘
            // 事件，故双重阻断冒泡；原生 button 自带键盘可达（Enter/Space）。
            div { class: "project-agent-item-body",
                div {
                    class: "project-agent-item-desc",
                    title: "{desc_text}",
                    "{desc_text}"
                }
                button {
                    class: "project-agent-item-tasks-btn",
                    title: if expanded() { "点击收起负责任务" } else { "点击展开负责任务" },
                    onclick: move |evt: MouseEvent| {
                        evt.stop_propagation();
                        expanded.set(!expanded());
                    },
                    onkeydown: move |evt: KeyboardEvent| evt.stop_propagation(),
                    "任务 {task_total}"
                }
            }
            // 展开区：任务数按钮展开/收起的「1. 2. 3.」编号任务列表
            if expanded() {
                div { class: "project-agent-item-detail",
                    div { class: "text-[11px] text-base-content/50", "负责任务（{task_total}）" }
                    if task_titles.is_empty() {
                        div { class: "text-xs text-base-content/40", "暂无任务" }
                    } else {
                        ol { class: "project-agent-item-tasks",
                            for t in task_titles.iter() {
                                li { key: "{t}", title: "{t}", "{t}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 把最近的滚动祖先容器滚回顶部（Detail 从列表深处切入，视觉应回到页首）。
///
/// 判定口径与 avatar_bubble::scroll_ancestor_rect 一致：内容高度 > 可视高度
/// 即视为滚动容器，不依赖 id / computed style（web-sys 未开 CssStyleDeclaration）。
fn reset_panel_scroll_top(from: &web_sys::Element) {
    let mut cur = from.parent_element();
    while let Some(el) = cur {
        if let Some(h) = el.dyn_ref::<web_sys::HtmlElement>()
            && h.scroll_height() > h.client_height() + 1
        {
            h.set_scroll_top(0);
            return;
        }
        cur = el.parent_element();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, assignee_type: i32, assignee_id: &str) -> TaskListItem {
        TaskListItem {
            id: id.to_string(),
            title: id.to_string(),
            description: None,
            status: 2,
            priority: 5,
            tags: Vec::new(),
            root_user_id: String::new(),
            assignee_type,
            assignee_id: assignee_id.to_string(),
            project_id: None,
            thinking_depth: 0,
            progress: 0,
            created_at: 0,
            updated_at: 0,
            dependencies: Vec::new(),
        }
    }

    fn agent(id: &str, name: &str) -> AgentListItem {
        AgentListItem {
            id: id.to_string(),
            name: name.to_string(),
            roles: Vec::new(),
            description: None,
            kind: "local".to_string(),
            model_provider_id: String::new(),
            status: 1,
            created_at: 0,
            runtime_state: 0,
        }
    }

    #[test]
    fn agent_ids_dedup_and_users_excluded() {
        let tasks = vec![
            task("t1", 1, "agt-a"),
            task("t2", 0, "usr-1"), // 用户 assignee 不展示
            task("t3", 1, "agt-a"), // 重复去重
            task("t4", 1, "agt-b"),
        ];
        assert_eq!(agent_ids_from_tasks(&tasks), vec!["agt-a", "agt-b"]);
    }

    #[test]
    fn agent_ids_empty_when_no_tasks() {
        assert!(agent_ids_from_tasks(&[]).is_empty());
    }

    #[test]
    fn sort_puts_owner_first_then_name_ascending() {
        // 名称用 ASCII 保证字典序期望值稳定（中文名按 Unicode 码点排序）
        let mut items = vec![agent("2", "Beta"), agent("1", "Alpha"), agent("3", "Gamma")];
        sort_project_agents(&mut items, Some("2"));
        assert_eq!(items[0].id, "2");
        assert_eq!(items[1].name, "Alpha");
        assert_eq!(items[2].name, "Gamma");
    }

    #[test]
    fn sort_without_owner_is_pure_name_order() {
        let mut items = vec![agent("2", "Beta"), agent("1", "Alpha")];
        sort_project_agents(&mut items, None);
        assert_eq!(items[0].name, "Alpha");
        assert_eq!(items[1].name, "Beta");
    }
}
