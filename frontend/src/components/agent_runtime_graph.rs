//! Agent 运行时关系图组件
//!
//! 复用 CanvasScene 组件，展示 Agent 与其绑定 Tools 的关系图：
//! - 中心节点：Agent 自身
//! - 周围节点：绑定的工具
//! - 连线：Agent → Tool 的绑定关系
//!
//! 注意：由于 `GetAgentResponse` 和 `ToolListItem`（定义在 common crate 中）未实现 `PartialEq`，
//! 而 Dioxus 的 `#[derive(Props)]` 要求字段类型实现 `PartialEq`，
//! 因此本组件仅接收所需的简单数据（ID + 名称），由调用方从 Agent/Tool 详情中提取。

use dioxus::prelude::*;

use crate::components::canvas_scene::{CanvasEdge, CanvasNode, CanvasScene};

/// 工具节点信息（仅包含绘图所需的 ID 和名称）
#[derive(Debug, Clone, PartialEq)]
pub struct ToolNodeInfo {
    pub id: String,
    pub name: String,
}

/// AgentRuntimeGraph Props
#[derive(Props, Clone, PartialEq)]
pub struct AgentRuntimeGraphProps {
    /// Agent ID（中心节点）
    pub agent_id: String,
    /// Agent 名称（中心节点标签）
    pub agent_name: String,
    /// 已绑定的工具列表（周围节点）
    pub bound_tools: Vec<ToolNodeInfo>,
}

#[component]
pub fn AgentRuntimeGraph(props: AgentRuntimeGraphProps) -> Element {
    let agent_id = props.agent_id.clone();
    let agent_name = props.agent_name.clone();
    let bound_tools = props.bound_tools.clone();

    // 构建节点：Agent 中心 + 绑定的 Tools
    let mut nodes = vec![CanvasNode {
        id: agent_id.clone(),
        x: 0.0,
        y: 0.0, // 触发圆形布局
        radius: 35.0,
        label: agent_name,
        color: "#fa520f".to_string(), // 品牌色
    }];
    for tool in &bound_tools {
        nodes.push(CanvasNode {
            id: tool.id.clone(),
            x: 0.0,
            y: 0.0,
            radius: 22.0,
            label: tool.name.clone(),
            color: "#f59e0b".to_string(),
        });
    }

    // 连线：Agent → 每个绑定的 Tool
    let edges: Vec<CanvasEdge> = bound_tools
        .iter()
        .map(|t| CanvasEdge {
            from_id: agent_id.clone(),
            to_id: t.id.clone(),
        })
        .collect();

    let tool_count = bound_tools.len();

    rsx! {
        div { class: "flex flex-col items-center",
            if tool_count == 0 {
                div { class: "text-center py-12",
                    div { class: "text-5xl mb-4 opacity-30", "🎨" }
                    div { class: "text-base-content/70", "该 Agent 暂未绑定工具" }
                }
            } else {
                div { class: "text-sm text-base-content/70 mb-4",
                    "共 {tool_count} 个绑定工具，拖拽节点可重新布局，点击节点查看详情"
                }
                CanvasScene {
                    width: 800.0,
                    height: 500.0,
                    nodes: nodes,
                    edges: edges,
                    enable_data_flow_particles: true,
                    enable_glow_particles: true,
                    enable_background_particles: true,
                    enable_birth_death_particles: true,
                    on_node_click: None,
                }
            }
        }
    }
}
