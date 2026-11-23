//! 工作台 HUD 层：顶部状态栏与左右悬浮面板
//!
//! 纯渲染函数，不自持状态：数据与信号由页面壳传入，导航与清红点通过回调/信号完成。
//! 保持与拆分前逐字一致的 class 与交互，不在此层引入新的视觉。

use std::collections::HashSet;

use dioxus::prelude::*;

use common::api::{
    AgentListItem, ProjectListItem, SceneProjectionResponse, WorkspaceMetricsResponse,
};

use crate::components::workspace_graph::WorkspaceView;
use crate::utils::number::{format_compact_count, format_decimal};
use crate::utils::status::{agent_runtime_badge, project_status_badge, tag_chip};

use super::helpers::{
    agent_runtime_label, filter_active_class, project_status_label, runtime_metric,
};

/// 顶部状态栏（玻璃，悬浮顶部）
///
/// 全部数字出自单一聚合接口快照（30 秒轮询）；首帧快照到达前整体不渲染，
/// 避免 0 值闪现。统计事件是批次刷盘，最近 1~2 分钟可能尚未落库，读数偏低属预期。
pub fn top_bar(
    workspace_metrics: Signal<Option<WorkspaceMetricsResponse>>,
    scene: Signal<Option<SceneProjectionResponse>>,
    scene_stale: Signal<bool>,
) -> Element {
    let metrics = workspace_metrics.read();
    let Some(m) = metrics.as_ref() else {
        return rsx! {};
    };
    // 场景投影可能晚于聚合指标到达（5s 轮询且允许失败）；未到达时不渲染该格
    let scene_guard = scene.read();
    let attention_total = scene_guard.as_ref().map(|s| s.attention_total);
    let stale = *scene_stale.read();

    let project_count = m.project_count;
    let agent_count = m.agent_count;
    let active_project_count = m.active_project_count;
    let busy_agent_count = m.runtime.busy;

    // 运行中 Agent 实时状态计数（空闲展示已移除：顶栏只含本进程启动后
    // 唤醒过的 Agent，与列表页「查不到兜底 Idle」口径不一致，2026-10-02 AMan 拍板）
    let busy_n = m.runtime.busy;
    let rest_n = m.runtime.resting;

    // 运行时读数（近 N 分钟窗口）：模型侧 + 工具侧
    let window_minutes = m.window_minutes;
    let window_secs = window_minutes as f64 * 60.0;
    let model_throughput =
        (m.model.total_tokens_input + m.model.total_tokens_output) as f64 / window_secs;
    // 无调用时用「—」表示无数据，而不是误导性的 0ms
    let tool_duration_label = match m.tool.avg_duration_ms {
        Some(ms) => format!("{}ms", format_decimal(ms, 0)),
        None => "—".to_string(),
    };
    let queue_backlog = m.queue_backlog;
    let queue_backlog_label = format_compact_count(queue_backlog);

    rsx! {
        div { class: "absolute top-3 left-3 right-3 z-10 hud-glass rounded-xl px-4 py-2 flex items-center gap-4 flex-wrap",
            // 4 个概览指标
            div { class: "flex items-center gap-3",
                div { class: "text-center",
                    div { class: "text-xs text-base-content/60", "项目" }
                    div { class: "text-lg font-semibold text-primary", "{project_count}" }
                }
                div { class: "text-center",
                    div { class: "text-xs text-base-content/60", "Agent" }
                    div { class: "text-lg font-semibold text-info", "{agent_count}" }
                }
                div { class: "text-center",
                    div { class: "text-xs text-base-content/60", "运行中" }
                    div { class: "text-lg font-semibold text-secondary", "{active_project_count}" }
                }
                div { class: "text-center",
                    div { class: "text-xs text-base-content/60", "忙碌" }
                    div { class: "text-lg font-semibold text-error", "{busy_agent_count}" }
                }
                if let Some(total) = attention_total {
                    div { class: "text-center",
                        div { class: "text-xs text-base-content/60", "待处理" }
                        div {
                            class: if total > 0 { "text-lg font-semibold text-warning" } else { "text-lg font-semibold text-base-content/40" },
                            "{total}"
                        }
                    }
                }
            }
            div { class: "h-8 w-px bg-base-content/15" }
            // 运行中 Agent 实时状态
            div { class: "flex items-center gap-3 text-xs",
                span { class: "flex items-center gap-1",
                    span { class: "w-2 h-2 rounded-full bg-warning" } "思考 {busy_n}"
                }
                span { class: "flex items-center gap-1",
                    span { class: "w-2 h-2 rounded-full bg-error" } "休息 {rest_n}"
                }
                // 投影断流：显式提示数据停在上一拍，不假装还在更新
                if stale {
                    span { class: "flex items-center gap-1 text-error",
                        span { class: "w-2 h-2 rounded-full bg-error animate-pulse" } "数据陈旧"
                    }
                }
            }
            // 运行时读数（顶栏数字指标）
            //
            // 这里原来放的是 Token QPS 迷你折线图，但顶栏横条太窄，曲线看不出
            // 趋势，改成数字指标更直接。
            div { class: "ml-auto flex items-center gap-4 flex-wrap justify-end",
                span { class: "text-xs text-base-content/60 shrink-0", "近 {window_minutes} 分钟" }
                {runtime_metric("模型调用", format_compact_count(m.model.total_calls), Some("primary"))}
                {runtime_metric("输入 Token", format_compact_count(m.model.total_tokens_input), Some("info"))}
                {runtime_metric("输出 Token", format_compact_count(m.model.total_tokens_output), Some("accent"))}
                {runtime_metric("缓存 Token", format_compact_count(m.model.total_tokens_cached), Some("success"))}
                {runtime_metric("思考 Token", format_compact_count(m.model.total_tokens_reasoning), Some("warning"))}
                {runtime_metric("吞吐 Token/s", format_decimal(model_throughput, 1), None)}
                {runtime_metric("工具调用", format_compact_count(m.tool.total_calls), Some("info"))}
                {runtime_metric("工具失败", format_compact_count(m.tool.failed_calls), Some("error"))}
                {runtime_metric("工具均耗时", tool_duration_label, Some("warning"))}
                {runtime_metric("队列积压", queue_backlog_label, (queue_backlog > 0).then_some("warning"))}
            }
        }
    }
}

/// 左侧项目面板（玻璃，悬浮左，可折叠）
pub fn project_panel(
    projects: Vec<ProjectListItem>,
    // 传入「有效的对话视图」而非路由视图：席位就地聚焦时也能高亮对应上下文
    current: WorkspaceView,
    collapsed: Signal<bool>,
    project_status_filter: Signal<Option<i32>>,
    project_unread: Signal<HashSet<String>>,
    on_navigate: EventHandler<WorkspaceView>,
) -> Element {
    let collapsed_val = *collapsed.read();
    let mut collapsed_sig = collapsed;
    let mut project_status_filter = project_status_filter;
    let mut project_unread = project_unread;
    let ps = *project_status_filter.read();
    let ps_key = ps.map(|v| v.to_string());
    // 空态按「过滤后」判定：全部被筛掉时也要给出提示，而不是留一片空白
    let visible_count = projects
        .iter()
        .filter(|p| ps.is_none_or(|s| p.status == s))
        .count();
    let empty_hint = if projects.is_empty() {
        "暂无项目"
    } else {
        "该状态下暂无项目"
    };

    rsx! {
        div {
            class: if collapsed_val {
                "absolute left-3 top-24 bottom-40 z-10 w-12 hud-glass rounded-xl flex flex-col items-center py-2"
            } else {
                "absolute left-3 top-24 bottom-40 z-10 w-64 hud-glass rounded-xl flex flex-col overflow-hidden"
            },
            div { class: "hud-panel-header p-3 border-b border-base-content/10",
                if !collapsed_val {
                    h3 { class: "text-sm font-semibold", "项目列表" }
                }
                div { class: "flex items-center gap-1",
                    button {
                        class: "hud-collapse-btn text-sm",
                        onclick: move |_| collapsed_sig.set(!collapsed_val),
                        if collapsed_val { "▶" } else { "◀" }
                    }
                    if !collapsed_val {
                        button {
                            class: "btn hud-btn btn-ghost btn-xs",
                            onclick: move |_| on_navigate.call(WorkspaceView::Global),
                            "全局"
                        }
                    }
                }
            }
            if !collapsed_val {
                // 状态过滤（与右侧 Agent 面板同款）
                div { class: "flex gap-1 px-3 py-2 border-b border-base-content/10",
                    button {
                        class: "btn hud-btn btn-xs join-item {filter_active_class(&ps_key, None)}",
                        onclick: move |_| project_status_filter.set(None),
                        "全部"
                    }
                    button {
                        class: "btn hud-btn btn-xs join-item {filter_active_class(&ps_key, Some(\"1\"))}",
                        onclick: move |_| project_status_filter.set(Some(1)),
                        "进行中"
                    }
                    button {
                        class: "btn hud-btn btn-xs join-item {filter_active_class(&ps_key, Some(\"2\"))}",
                        onclick: move |_| project_status_filter.set(Some(2)),
                        "已完成"
                    }
                }
                div { class: "flex-1 overflow-y-auto divide-y divide-base-200",
                    for p in projects.iter().filter(|p| ps.is_none_or(|s| p.status == s)) {
                        {
                            let pid = p.id.clone();
                            let is_selected = matches!(current, WorkspaceView::ProjectDetail(ref id) if id == &pid);
                            let has_unread = project_unread.read().contains(&pid);
                            let item_class = if is_selected {
                                "relative overflow-hidden w-full text-left p-3 hover:bg-base-200 transition-colors bg-base-200"
                            } else {
                                "relative overflow-hidden w-full text-left p-3 hover:bg-base-200 transition-colors"
                            };
                            rsx! {
                                button {
                                    class: "{item_class}",
                                    onclick: move |_| {
                                        on_navigate.call(WorkspaceView::ProjectDetail(pid.clone()));
                                        project_unread.write().remove(&pid);
                                    },
                                    if has_unread {
                                        span { class: "hud-streak" }
                                    }
                                    div { class: "flex justify-between items-start",
                                        div { class: "flex items-center gap-1 min-w-0",
                                            span { class: "text-sm font-medium truncate", "{p.name}" }
                                        }
                                        span { class: "{project_status_badge(p.status)} ml-2 flex-shrink-0",
                                            "{project_status_label(p.status)}"
                                        }
                                    }
                                    if !p.tags.is_empty() {
                                        div { class: "flex flex-wrap gap-1 mt-1",
                                            for tag in p.tags.iter().take(2) {
                                                span { class: "{tag_chip()}", "{tag}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if visible_count == 0 {
                        div { class: "p-4 text-center text-sm text-base-content/50",
                            "{empty_hint}"
                        }
                    }
                }
            }
        }
    }
}

/// 右侧 Agent 面板（玻璃，悬浮右，可折叠 + 状态过滤）
pub fn agent_panel(
    agents: Vec<AgentListItem>,
    // 传入「有效的对话视图」而非路由视图：席位就地聚焦时也能高亮对应 Agent
    current: WorkspaceView,
    collapsed: Signal<bool>,
    runtime_filter: Signal<Option<String>>,
    agent_unread: Signal<HashSet<String>>,
    on_navigate: EventHandler<WorkspaceView>,
) -> Element {
    let collapsed_val = *collapsed.read();
    let mut collapsed_sig = collapsed;
    let mut runtime_filter = runtime_filter;
    let mut agent_unread = agent_unread;
    let ra_filter = runtime_filter.read().clone();
    // 同项目面板：空态按过滤后判定
    let visible_count = agents
        .iter()
        .filter(|a| match ra_filter.as_deref() {
            None => true,
            Some("busy") => a.runtime_state == 2,
            Some("resting") => a.runtime_state == 1,
            _ => true,
        })
        .count();
    let empty_hint = if agents.is_empty() {
        "暂无 Agent"
    } else {
        "该状态下暂无 Agent"
    };

    rsx! {
        div {
            class: if collapsed_val {
                "absolute right-3 top-24 bottom-40 z-10 w-12 hud-glass rounded-xl flex flex-col items-center py-2"
            } else {
                "absolute right-3 top-24 bottom-40 z-10 w-64 hud-glass rounded-xl flex flex-col overflow-hidden"
            },
            div { class: "hud-panel-header p-3 border-b border-base-content/10",
                if !collapsed_val {
                    h3 { class: "text-sm font-semibold", "Agent 列表" }
                }
                div { class: "flex items-center gap-1",
                    button {
                        class: "hud-collapse-btn text-sm",
                        onclick: move |_| collapsed_sig.set(!collapsed_val),
                        if collapsed_val { "◀" } else { "▶" }
                    }
                    if !collapsed_val {
                        button {
                            class: "btn hud-btn btn-ghost btn-xs",
                            onclick: move |_| on_navigate.call(WorkspaceView::Global),
                            "全局"
                        }
                    }
                }
            }
            if !collapsed_val {
                // 状态过滤（原独立"运行中 Agent"卡片的过滤迁入）
                div { class: "flex gap-1 px-3 py-2 border-b border-base-content/10",
                    button {
                        class: "btn hud-btn btn-xs join-item {filter_active_class(&ra_filter, None)}",
                        onclick: move |_| runtime_filter.set(None),
                        "全部"
                    }
                    button {
                        class: "btn hud-btn btn-xs join-item {filter_active_class(&ra_filter, Some(\"busy\"))}",
                        onclick: move |_| runtime_filter.set(Some("busy".to_string())),
                        "思考"
                    }
                    button {
                        class: "btn hud-btn btn-xs join-item {filter_active_class(&ra_filter, Some(\"resting\"))}",
                        onclick: move |_| runtime_filter.set(Some("resting".to_string())),
                        "休息"
                    }
                }
                div { class: "flex-1 overflow-y-auto divide-y divide-base-200",
                    for a in agents.iter().filter(|a| match ra_filter.as_deref() {
                        None => true,
                        Some("busy") => a.runtime_state == 2,
                        Some("resting") => a.runtime_state == 1,
                        _ => true,
                    }) {
                        {
                            let aid = a.id.clone();
                            let is_selected = matches!(current, WorkspaceView::AgentDetail(ref id) if id == &aid);
                            let has_unread = agent_unread.read().contains(&aid);
                            let item_class = if is_selected {
                                "relative overflow-hidden w-full text-left p-3 hover:bg-base-200 transition-colors bg-base-200"
                            } else {
                                "relative overflow-hidden w-full text-left p-3 hover:bg-base-200 transition-colors"
                            };
                            rsx! {
                                button {
                                    class: "{item_class}",
                                    onclick: move |_| {
                                        on_navigate.call(WorkspaceView::AgentDetail(aid.clone()));
                                        agent_unread.write().remove(&aid);
                                    },
                                    if has_unread {
                                        span { class: "hud-streak" }
                                    }
                                    div { class: "flex justify-between items-start",
                                        div { class: "flex items-center gap-1 min-w-0",
                                            span { class: "text-sm font-medium truncate", "{a.name}" }
                                        }
                                        span { class: "ml-2 flex-shrink-0 {agent_runtime_badge(a.runtime_state)}",
                                            "{agent_runtime_label(a.runtime_state)}"
                                        }
                                    }
                                    div { class: "text-xs text-base-content/60 mt-1",
                                        "{a.kind}"
                                    }
                                }
                            }
                        }
                    }
                    if visible_count == 0 {
                        div { class: "p-4 text-center text-sm text-base-content/50",
                            "{empty_hint}"
                        }
                    }
                }
            }
        }
    }
}
