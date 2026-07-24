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
            x: 200.0,
            y: 150.0,
            radius: 30.0,
            label: "Agent 1".to_string(),
            color: "#3b82f6".to_string(),
        },
        CanvasNode {
            id: "agent-2".to_string(),
            x: 500.0,
            y: 150.0,
            radius: 25.0,
            label: "Agent 2".to_string(),
            color: "#10b981".to_string(),
        },
        CanvasNode {
            id: "tool-1".to_string(),
            x: 350.0,
            y: 350.0,
            radius: 20.0,
            label: "Tool A".to_string(),
            color: "#f59e0b".to_string(),
        },
    ]
}

/// 生成示例连线数据
fn sample_edges() -> Vec<CanvasEdge> {
    vec![
        CanvasEdge { from_id: "agent-1".to_string(), to_id: "tool-1".to_string() },
        CanvasEdge { from_id: "agent-2".to_string(), to_id: "tool-1".to_string() },
    ]
}

#[component]
pub fn Workspace() -> Element {
    let nodes = sample_nodes();
    let edges = sample_edges();
    let mut selected_id = use_signal(|| None::<String>);
    let toast = use_toast();

    rsx! {
        AppLayout {
            div { class: "card bg-base-100 shadow-md",
                div { class: "card-body",
                    h2 { class: "card-title mb-2", "🚀 工作台（Canvas 试点）" }
                    p { class: "text-sm text-base-content/70 mb-4",
                        "验证 Canvas 渲染基础设施：节点渲染、连线绘制、点击事件桥接。"
                    }

                    // Canvas 场景
                    div { class: "flex justify-center",
                        CanvasScene {
                            width: 800.0,
                            height: 500.0,
                            nodes: nodes.clone(),
                            edges: edges.clone(),
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
