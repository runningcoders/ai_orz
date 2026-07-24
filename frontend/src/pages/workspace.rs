//! 工作台页面（驾驶舱）
//!
//! 三栏布局：左侧 Project 列表浮层 / 中间 Canvas 关系图 / 右侧 Agent 列表浮层
//! 顶部汇总状态条：项目数 / Agent 数 / 活跃任务 / 忙碌 Agent
//! 中间区域通过 WorkspaceView 状态机切换三种视图：
//! - Global：Project ↔ Agent 关联（默认）
//! - ProjectDetail：选中 Project 的 Task + Agent
//! - AgentDetail：选中 Agent 的 Task + Project

use dioxus::prelude::*;

use crate::components::workspace_graph::{WorkspaceGraph, WorkspaceView};
use crate::hooks::use_workspace_data::use_workspace_data;
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;

/// Project 状态标签
fn project_status_label(status: i32) -> &'static str {
    match status {
        1 => "活跃",
        2 => "待评审",
        3 => "进行中",
        4 => "已完成",
        5 => "已归档",
        _ => "未知",
    }
}

/// Agent 运行时状态标签
fn agent_runtime_label(runtime_state: i32) -> &'static str {
    match runtime_state {
        0 => "空闲",
        1 => "休息中",
        2 => "忙碌",
        _ => "未知",
    }
}

/// Agent 运行时状态颜色 class（用于 badge）
fn agent_runtime_badge_class(runtime_state: i32) -> &'static str {
    match runtime_state {
        0 => "badge badge-success",
        1 => "badge badge-warning",
        2 => "badge badge-error",
        _ => "badge badge-ghost",
    }
}

#[component]
pub fn Workspace() -> Element {
    let (data_signal, mut refresh) = use_workspace_data();
    let mut current_view = use_signal(|| WorkspaceView::Global);
    let toast = use_toast();

    let data = data_signal.read().clone();

    rsx! {
        AppLayout {
            div { class: "flex flex-col h-full gap-4",
                // === 顶部汇总状态条 ===
                {data.as_ref().map(|d| {
                    let project_count = d.projects.len();
                    let agent_count = d.agents.len();
                    let active_task_count = d.tasks.iter().filter(|t| t.status == 1).count();
                    let busy_agent_count = d.agents.iter().filter(|a| a.runtime_state == 2).count();

                    rsx! {
                        div { class: "grid grid-cols-2 md:grid-cols-4 gap-3",
                            div { class: "stat bg-base-100 rounded-lg shadow-sm",
                                div { class: "stat-title", "项目" }
                                div { class: "stat-value text-primary", "{project_count}" }
                            }
                            div { class: "stat bg-base-100 rounded-lg shadow-sm",
                                div { class: "stat-title", "Agent" }
                                div { class: "stat-value text-info", "{agent_count}" }
                            }
                            div { class: "stat bg-base-100 rounded-lg shadow-sm",
                                div { class: "stat-title", "活跃任务" }
                                div { class: "stat-value text-secondary", "{active_task_count}" }
                            }
                            div { class: "stat bg-base-100 rounded-lg shadow-sm",
                                div { class: "stat-title", "忙碌 Agent" }
                                div { class: "stat-value text-error", "{busy_agent_count}" }
                            }
                        }
                    }
                })}

                // === 三栏布局：左 Project / 中 Canvas / 右 Agent ===
                div { class: "flex gap-4 flex-1 min-h-0",
                    // 左侧 Project 列表浮层
                    {data.as_ref().map(|d| {
                        rsx! {
                            div { class: "w-64 flex-shrink-0 bg-base-100 rounded-lg shadow-md overflow-y-auto",
                                div { class: "p-3 sticky top-0 bg-base-100 border-b border-base-200 z-10",
                                    div { class: "flex justify-between items-center",
                                        h3 { class: "text-sm font-semibold", "项目列表" }
                                        button {
                                            class: "btn btn-ghost btn-xs",
                                            onclick: move |_| { current_view.set(WorkspaceView::Global); },
                                            "全局"
                                        }
                                    }
                                }
                                div { class: "divide-y divide-base-200",
                                    for p in d.projects.iter() {
                                        {
                                            let pid = p.id.clone();
                                            let is_selected = matches!(*current_view.read(), WorkspaceView::ProjectDetail(ref id) if id == &pid);
                                            let item_class = if is_selected {
                                                "w-full text-left p-3 hover:bg-base-200 transition-colors bg-base-200"
                                            } else {
                                                "w-full text-left p-3 hover:bg-base-200 transition-colors"
                                            };
                                            rsx! {
                                                button {
                                                    class: "{item_class}",
                                                    onclick: move |_| {
                                                        current_view.set(WorkspaceView::ProjectDetail(pid.clone()));
                                                    },
                                                    div { class: "flex justify-between items-start",
                                                        span { class: "text-sm font-medium truncate", "{p.name}" }
                                                        span { class: "badge badge-xs badge-ghost ml-2",
                                                            "{project_status_label(p.status)}"
                                                        }
                                                    }
                                                    if !p.tags.is_empty() {
                                                        div { class: "flex flex-wrap gap-1 mt-1",
                                                            for tag in p.tags.iter().take(2) {
                                                                span { class: "badge badge-xs badge-ghost", "{tag}" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if d.projects.is_empty() {
                                        div { class: "p-4 text-center text-sm text-base-content/50",
                                            "暂无项目"
                                        }
                                    }
                                }
                            }
                        }
                    })}

                    // 中间 Canvas 关系图
                    div { class: "flex-1 bg-base-100 rounded-lg shadow-md p-4 min-w-0",
                        {data.as_ref().map(|d| {
                            rsx! {
                                WorkspaceGraph {
                                    view: current_view.read().clone(),
                                    projects: d.projects.clone(),
                                    agents: d.agents.clone(),
                                    tasks: d.tasks.clone(),
                                    width: 700.0,
                                    height: 500.0,
                                }
                            }
                        })}
                        {data.is_none().then(|| rsx! {
                            div { class: "flex items-center justify-center h-full",
                                span { class: "loading loading-spinner loading-lg" }
                            }
                        })}
                    }

                    // 右侧 Agent 列表浮层
                    {data.as_ref().map(|d| {
                        rsx! {
                            div { class: "w-64 flex-shrink-0 bg-base-100 rounded-lg shadow-md overflow-y-auto",
                                div { class: "p-3 sticky top-0 bg-base-100 border-b border-base-200 z-10",
                                    div { class: "flex justify-between items-center",
                                        h3 { class: "text-sm font-semibold", "Agent 列表" }
                                        button {
                                            class: "btn btn-ghost btn-xs",
                                            onclick: move |_| { current_view.set(WorkspaceView::Global); },
                                            "全局"
                                        }
                                    }
                                }
                                div { class: "divide-y divide-base-200",
                                    for a in d.agents.iter() {
                                        {
                                            let aid = a.id.clone();
                                            let is_selected = matches!(*current_view.read(), WorkspaceView::AgentDetail(ref id) if id == &aid);
                                            let item_class = if is_selected {
                                                "w-full text-left p-3 hover:bg-base-200 transition-colors bg-base-200"
                                            } else {
                                                "w-full text-left p-3 hover:bg-base-200 transition-colors"
                                            };
                                            rsx! {
                                                button {
                                                    class: "{item_class}",
                                                    onclick: move |_| {
                                                        current_view.set(WorkspaceView::AgentDetail(aid.clone()));
                                                    },
                                                    div { class: "flex justify-between items-start",
                                                        span { class: "text-sm font-medium truncate", "{a.name}" }
                                                        span { class: "badge badge-xs ml-2 {agent_runtime_badge_class(a.runtime_state)}",
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
                                    if d.agents.is_empty() {
                                        div { class: "p-4 text-center text-sm text-base-content/50",
                                            "暂无 Agent"
                                        }
                                    }
                                }
                            }
                        }
                    })}
                }

                // === 底部图例 + 刷新按钮 ===
                div { class: "flex justify-between items-center text-xs text-base-content/70",
                    div { class: "flex gap-4",
                        span { class: "flex items-center gap-1",
                            span { class: "w-3 h-3 rounded-full bg-success" }
                            "空闲/活跃"
                        }
                        span { class: "flex items-center gap-1",
                            span { class: "w-3 h-3 rounded-full bg-warning" }
                            "休息/待评审"
                        }
                        span { class: "flex items-center gap-1",
                            span { class: "w-3 h-3 rounded-full bg-error" }
                            "忙碌"
                        }
                        span { class: "flex items-center gap-1",
                            span { class: "w-3 h-3 rounded-full bg-info" }
                            "进行中"
                        }
                    }
                    button {
                        class: "btn btn-ghost btn-xs",
                        onclick: move |_| { refresh(); toast.info("已刷新数据"); },
                        "🔄 刷新"
                    }
                }
            }
        }
    }
}
