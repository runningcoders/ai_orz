//! 工作台页面的纯函数与小常量
//!
//! 只放无副作用、无 IO 的映射与判定：视图 ↔ URL 编码、对话上下文解析、消息归属、
//! 标签与 class 计算。页面壳与 HUD / Dock 共用，避免同一口径写两份。

use dioxus::prelude::*;

use crate::components::hud::StatReadout;
use crate::components::workspace_graph::WorkspaceView;
use crate::hooks::use_workspace_data::WorkspaceData;
use crate::utils::in_project_context;
use common::api::MessageListItem;

/// Project 状态标签
pub fn project_status_label(status: i32) -> &'static str {
    match status {
        1 => "进行中",
        2 => "已完成",
        3 => "已归档",
        _ => "未知",
    }
}

/// 顶栏聚合指标的时间窗口（分钟）
///
/// 请求聚合端点时随 query 传入；模型侧与工具侧共用同一窗口，保证两项读数口径一致；
/// 也与后端 `GetWorkspaceMetricsRequest.minutes` 的默认值一致。
pub const RUNTIME_METRICS_WINDOW_MINUTES: u32 = 60;

/// 顶栏运行时指标单元（紧凑读数）。
///
/// 薄封装 [`StatReadout`]，避免在 RSX 里重复多遍同样的参数样板。
/// 普通函数而非 `#[component]`：这里只在同一处渲染，无需独立属性类型。
pub fn runtime_metric(label: &str, value: String, accent: Option<&str>) -> Element {
    rsx! {
        StatReadout {
            label: label.to_string(),
            value,
            unit: None,
            icon: None,
            delta: None,
            accent: accent.map(str::to_string),
            compact: Some(true),
        }
    }
}

/// Agent 运行时状态标签
pub fn agent_runtime_label(runtime_state: i32) -> &'static str {
    match runtime_state {
        0 => "空闲",
        1 => "休息中",
        2 => "忙碌",
        _ => "未知",
    }
}

/// 运行中 Agent 过滤按钮的 active class
///
/// 当前过滤值与按钮值匹配时返回 "btn-active"，否则返回空字符串。
/// `None` 表示"全部"按钮。
pub fn filter_active_class(current: &Option<String>, target: Option<&str>) -> &'static str {
    let matches = match (current.as_deref(), target) {
        (Some(c), Some(t)) => c == t,
        (None, None) => true,
        _ => false,
    };
    if matches { "btn-active" } else { "" }
}

/// 根据视图计算对话上下文（project_id, task_id, to_agent_id）
///
/// `task_anchor` 是 TaskDetail 专用的「(task_id, 标题, project_id)」锚点，由图数据
/// 异步加载后回填 —— 因为任务到项目的归属关系只有 `query_tasks` 拿得到，
/// `WorkspaceData` 里没有。**必须用 task_id 校验**，否则切换任务时旧锚点会造成串台。
pub fn resolve_chat_context(
    view: &WorkspaceView,
    sidebar: &WorkspaceData,
    task_anchor: Option<&(String, String, Option<String>)>,
) -> (Option<String>, Option<String>, Option<String>) {
    match view {
        WorkspaceView::Global => (None, None, None),
        WorkspaceView::ProjectDetail(pid) => {
            let to_agent_id = sidebar
                .projects
                .iter()
                .find(|p| &p.id == pid)
                .and_then(|p| p.owner_agent_id.clone());
            (Some(pid.clone()), None, to_agent_id)
        }
        WorkspaceView::AgentDetail(aid) => (None, None, Some(aid.clone())),
        WorkspaceView::TaskDetail(tid) => {
            // 任务视图 = 项目会话锚定到该任务：project_id 与收件人（项目 owner / PMO）
            // 都与 ProjectDetail 对齐，任务只体现在 task_id 与默认 @ 上。
            // 锚点未就绪时返回 None —— 调用方据此跳过消息加载，等锚点回填后重跑，
            // 避免先拉一次全组织消息再被覆盖（会闪一下别的会话）。
            let pid = task_anchor
                .filter(|(anchor_tid, _, _)| anchor_tid == tid)
                .and_then(|(_, _, pid)| pid.clone());
            let to_agent_id = pid.as_deref().and_then(|pid| {
                sidebar
                    .projects
                    .iter()
                    .find(|p| p.id == pid)
                    .and_then(|p| p.owner_agent_id.clone())
            });
            (pid, Some(tid.clone()), to_agent_id)
        }
    }
}

/// 底部对话框输入框 DOM id：@ 提及需要读写真实光标位置（边界判定 + 插入后复位）
pub const WORKSPACE_CHAT_INPUT_ID: &str = "workspace-chat-input";

/// 底部对话框消息区 DOM id：上拉加载更早历史需要读 `scrollTop`
pub const WORKSPACE_CHAT_SCROLL_ID: &str = "workspace-chat-scroll";

/// 单条消息是否属于当前对话上下文
///
/// 历史首屏 / 上拉翻页 / SSE 实时推送三条链路**必须同一口径**，否则会出现
/// 「实时推来的看得见、一刷新就没了」。收口成一个函数就是为了杜绝这种漂移。
///
/// - `agent_id = Some`（Agent 私聊）：唯一「按人收窄」的视图 —— 它没有项目域，
///   只能靠 to/from 圈定
/// - 其余按 `project_id` 判定：项目会话（ProjectDetail / TaskDetail）展示项目内
///   **全部**往来（含 Agent 之间的横向交流，用户是旁听者 —— ⚠️ 不可按项目 owner
///   收窄，那正是「刷新即丢失」的成因）；Global（`None`）收敛为「只显示无 project
///   的默认对话」
pub fn message_in_context(
    msg: &MessageListItem,
    project_id: Option<&str>,
    agent_id: Option<&str>,
) -> bool {
    if let Some(aid) = agent_id {
        return msg.to_id == aid || msg.from_id == aid;
    }
    in_project_context(msg, project_id)
}

/// 批量过滤，复用 [`message_in_context`] 的判定
pub fn filter_in_context(
    msgs: Vec<MessageListItem>,
    project_id: Option<&str>,
    agent_id: Option<&str>,
) -> Vec<MessageListItem> {
    msgs.into_iter()
        .filter(|m| message_in_context(m, project_id, agent_id))
        .collect()
}

/// 仅在 Agent 私聊视图下才把收件人当作过滤条件
///
/// ⚠️ 项目会话的 `to_agent_id` 也是 `Some`（项目 owner / PMO），直接拿去当过滤器
/// 会把 Agent 之间的横向交流全滤掉 —— 这里显式只在 AgentDetail 生效。
pub fn context_agent_filter(view: &WorkspaceView, to_agent_id: Option<String>) -> Option<String> {
    match view {
        WorkspaceView::AgentDetail(_) => to_agent_id,
        _ => None,
    }
}

/// WorkspaceView → 路由 query 编码（URL 即状态的写入侧）
///
/// Global 缺省不产生 query（`/workspace` 比 `/workspace?view=global` 干净）；
/// 其余变体用 `类型:id` 前缀编码。id 为 UUID（字母数字连字符），`:` 在 query
/// 中是合法字符且 dioxus 路由解析不做 percent-decode（identity round-trip），
/// 无需转义。
pub fn view_to_query(view: &WorkspaceView) -> Option<String> {
    match view {
        WorkspaceView::Global => None,
        WorkspaceView::ProjectDetail(id) => Some(format!("project:{id}")),
        WorkspaceView::AgentDetail(id) => Some(format!("agent:{id}")),
        WorkspaceView::TaskDetail(id) => Some(format!("task:{id}")),
    }
}

/// 路由 query → WorkspaceView 解码（URL 即状态的读取侧）
///
/// 未知编码（旧链接 / 手改 URL）回落全局视图，避免页面卡死在空状态。
pub fn view_from_query(query: &Option<String>) -> WorkspaceView {
    match query.as_deref() {
        None => WorkspaceView::Global,
        Some(raw) => {
            if let Some(id) = raw.strip_prefix("project:") {
                WorkspaceView::ProjectDetail(id.to_string())
            } else if let Some(id) = raw.strip_prefix("agent:") {
                WorkspaceView::AgentDetail(id.to_string())
            } else if let Some(id) = raw.strip_prefix("task:") {
                WorkspaceView::TaskDetail(id.to_string())
            } else {
                WorkspaceView::Global
            }
        }
    }
}
