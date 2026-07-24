//! 工作台页面（Canvas 渲染基础设施试点）
//!
//! 验证 CanvasScene 组件的：
//! - Canvas 2D Context 初始化
//! - 节点/连线渲染
//! - 鼠标事件桥接 + 命中检测
//! - 高清屏适配

use dioxus::prelude::*;

use crate::components::canvas_scene::{CanvasEdge, CanvasNode, CanvasScene};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;

/// 生成示例节点数据（模拟 Agent 状态面板）
fn sample_nodes() -> Vec<CanvasNode> {
    vec![
        CanvasNode {
            id: "agent-1".to_string(),
            x: 0.0,  // 初始位置 0,0 会触发圆形布局
            y: 0.0,
            radius: 30.0,
            label: "Agent 1".to_string(),
            color: "#3b82f6".to_string(),
            node_type: None,
        },
        CanvasNode {
            id: "agent-2".to_string(),
            x: 0.0,
            y: 0.0,
            radius: 25.0,
            label: "Agent 2".to_string(),
            color: "#10b981".to_string(),
            node_type: None,
        },
        CanvasNode {
            id: "agent-3".to_string(),
            x: 0.0,
            y: 0.0,
            radius: 28.0,
            label: "Agent 3".to_string(),
            color: "#8b5cf6".to_string(),
            node_type: None,
        },
        CanvasNode {
            id: "tool-1".to_string(),
            x: 0.0,
            y: 0.0,
            radius: 20.0,
            label: "Tool A".to_string(),
            color: "#f59e0b".to_string(),
            node_type: None,
        },
        CanvasNode {
            id: "tool-2".to_string(),
            x: 0.0,
            y: 0.0,
            radius: 18.0,
            label: "Tool B".to_string(),
            color: "#ef4444".to_string(),
            node_type: None,
        },
        CanvasNode {
            id: "tool-3".to_string(),
            x: 0.0,
            y: 0.0,
            radius: 16.0,
            label: "Tool C".to_string(),
            color: "#06b6d4".to_string(),
            node_type: None,
        },
    ]
}

/// 生成示例连线数据
fn sample_edges() -> Vec<CanvasEdge> {
    vec![
        CanvasEdge { from_id: "agent-1".to_string(), to_id: "tool-1".to_string() },
        CanvasEdge { from_id: "agent-1".to_string(), to_id: "tool-2".to_string() },
        CanvasEdge { from_id: "agent-2".to_string(), to_id: "tool-1".to_string() },
        CanvasEdge { from_id: "agent-2".to_string(), to_id: "tool-3".to_string() },
        CanvasEdge { from_id: "agent-3".to_string(), to_id: "tool-2".to_string() },
        CanvasEdge { from_id: "agent-3".to_string(), to_id: "tool-3".to_string() },
    ]
}

#[component]
pub fn Workspace() -> Element {
    let nodes = sample_nodes();
    let edges = sample_edges();
    let mut selected_id = use_signal(|| None::<String>);
    let toast = use_toast();
    let mut enable_data_flow = use_signal(|| true);
    let mut enable_glow = use_signal(|| true);
    let mut enable_background = use_signal(|| true);
    let mut enable_birth_death = use_signal(|| true);

    rsx! {
        AppLayout {
            div { class: "card bg-base-100 shadow-md",
                div { class: "card-body",
                    h2 { class: "card-title mb-2", "🚀 工作台（Canvas 试点）" }
                    p { class: "text-sm text-base-content/70 mb-4",
                        "验证 Canvas 渲染基础设施：节点渲染、连线绘制、点击事件桥接。"
                    }

                    // 粒子效果开关
                    div { class: "flex flex-wrap gap-2 mb-4",
                        label { class: "label cursor-pointer flex items-center gap-2",
                            input {
                                r#type: "checkbox",
                                class: "toggle toggle-sm toggle-primary",
                                checked: "{enable_data_flow}",
                                onchange: move |e| enable_data_flow.set(e.checked()),
                            }
                            span { class: "label-text text-sm", "数据流粒子" }
                        }
                        label { class: "label cursor-pointer flex items-center gap-2",
                            input {
                                r#type: "checkbox",
                                class: "toggle toggle-sm toggle-secondary",
                                checked: "{enable_glow}",
                                onchange: move |e| enable_glow.set(e.checked()),
                            }
                            span { class: "label-text text-sm", "辉光粒子" }
                        }
                        label { class: "label cursor-pointer flex items-center gap-2",
                            input {
                                r#type: "checkbox",
                                class: "toggle toggle-sm toggle-accent",
                                checked: "{enable_background}",
                                onchange: move |e| enable_background.set(e.checked()),
                            }
                            span { class: "label-text text-sm", "背景粒子" }
                        }
                        label { class: "label cursor-pointer flex items-center gap-2",
                            input {
                                r#type: "checkbox",
                                class: "toggle toggle-sm toggle-neutral",
                                checked: "{enable_birth_death}",
                                onchange: move |e| enable_birth_death.set(e.checked()),
                            }
                            span { class: "label-text text-sm", "诞生/消亡" }
                        }
                    }

                    // Canvas 场景
                    div { class: "flex justify-center",
                        CanvasScene {
                            width: 800.0,
                            height: 500.0,
                            nodes: nodes.clone(),
                            edges: edges.clone(),
                            enable_data_flow_particles: *enable_data_flow.read(),
                            enable_glow_particles: *enable_glow.read(),
                            enable_background_particles: *enable_background.read(),
                            enable_birth_death_particles: *enable_birth_death.read(),
                            on_node_click: move |id: String| {
                                selected_id.set(Some(id.clone()));
                                toast.info(&format!("点击节点: {id}"));
                            }
                        }
                    }

                    // 选中节点信息
                    if let Some(id) = &*selected_id.read() {
                        div { class: "alert alert-info mt-4",
                            span { "当前选中: {id}" }
                        }
                    } else {
                        div { class: "text-sm text-base-content/50 mt-4",
                            "点击 Canvas 中的节点查看交互效果"
                        }
                    }
                }
            }
        }
    }
}
