//! 实体关系图组件（通用）
//!
//! 复用 CanvasScene 组件，展示"中心实体 + 关联实体"的辐射关系图：
//! - 中心节点：一个主体实体（如 Agent / Tool / Skill）
//! - 周围节点：与中心实体有关联的实体（如绑定的 Tools / 安装的 Agents）
//! - 连线：中心 → 关联实体
//!
//! 通用设计：
//! - 调用方传入 center + related 列表即可，不绑定具体业务概念
//! - 颜色、空状态文案、节点类型标识均由调用方控制
//! - 点击回调通过 NodeClickEvent 返回 (kind, id, is_center)，由调用方决定处理逻辑
//!
//! 已接入场景：
//! - Agent 详情页"状态图" Tab：center=Agent, related=绑定的 Tools
//!
//! 预留场景（待后端支持反向关联数据后接入）：
//! - Tool 详情页：center=Tool, related=绑定此 Tool 的 Agents
//! - Skill 详情页：center=Skill, related=安装此 Skill 的 Agents

use dioxus::prelude::*;

use crate::components::canvas_scene::{CanvasEdge, CanvasNode, CanvasScene};

/// 关联实体节点信息（仅包含绘图所需的 ID 和名称）
#[derive(Debug, Clone, PartialEq)]
pub struct RelationNodeInfo {
    pub id: String,
    pub name: String,
    /// 可选节点类型标识（如 "agent"/"tool"/"skill"），由调用方约定，
    /// 回调触发时原样返回，组件不解释此字段
    pub kind: Option<String>,
}

impl RelationNodeInfo {
    /// 构造带类型标识的节点
    pub fn with_kind(id: String, name: String, kind: impl Into<String>) -> Self {
        Self { id, name, kind: Some(kind.into()) }
    }
}

/// 节点点击事件
///
/// 携带节点类型标识、ID 以及是否为中心节点，
/// 由调用方根据这些信息决定跳转或处理逻辑
#[derive(Debug, Clone, PartialEq)]
pub struct NodeClickEvent {
    /// 节点类型标识（原样回传调用方传入的 kind）
    pub kind: Option<String>,
    /// 节点 ID
    pub id: String,
    /// 是否为中心节点
    pub is_center: bool,
}

/// RelationGraph Props
#[derive(Props, Clone, PartialEq)]
pub struct RelationGraphProps {
    /// 中心实体 ID
    pub center_id: String,
    /// 中心实体名称（节点标签）
    pub center_name: String,
    /// 中心节点颜色（#rrggbb 格式）
    pub center_color: String,
    /// 中心节点类型标识（回调时原样返回）
    pub center_kind: Option<String>,
    /// 关联实体列表（周围节点）
    pub related: Vec<RelationNodeInfo>,
    /// 关联节点颜色（#rrggbb 格式）
    pub related_color: String,
    /// 关联实体名称（用于空状态文案，如"工具"/"技能"/"Agent"）
    pub related_label: String,
    /// 节点点击回调
    pub on_node_click: Option<EventHandler<NodeClickEvent>>,
}

#[component]
pub fn RelationGraph(props: RelationGraphProps) -> Element {
    let center_id = props.center_id.clone();
    let center_name = props.center_name.clone();
    let center_color = props.center_color.clone();
    let center_kind = props.center_kind.clone();
    let related = props.related.clone();
    let related_color = props.related_color.clone();
    let related_label = props.related_label.clone();
    let on_node_click = props.on_node_click;

    // 构建节点：中心 + 关联实体
    let mut nodes = vec![CanvasNode {
        id: center_id.clone(),
        x: 0.0,
        y: 0.0, // 触发圆形布局
        radius: 35.0,
        label: center_name,
        color: center_color,
        node_type: center_kind.clone(),
    }];
    for item in &related {
        nodes.push(CanvasNode {
            id: item.id.clone(),
            x: 0.0,
            y: 0.0,
            radius: 22.0,
            label: item.name.clone(),
            color: related_color.clone(),
            node_type: item.kind.clone(),
        });
    }

    // 连线：中心 → 每个关联实体
    let edges: Vec<CanvasEdge> = related
        .iter()
        .map(|item| CanvasEdge {
            from_id: center_id.clone(),
            to_id: item.id.clone(),
        })
        .collect();

    let count = related.len();

    // 构建节点 id → (kind, is_center) 查找表，供回调判断
    let mut click_map: std::collections::HashMap<String, (Option<String>, bool)> =
        std::collections::HashMap::with_capacity(related.len() + 1);
    click_map.insert(center_id.clone(), (center_kind.clone(), true));
    for item in &related {
        click_map.insert(item.id.clone(), (item.kind.clone(), false));
    }

    rsx! {
        div { class: "flex flex-col items-center",
            if count == 0 {
                div { class: "text-center py-12",
                    div { class: "text-5xl mb-4 opacity-30", "🎨" }
                    div { class: "text-base-content/70", "暂无关联{related_label}" }
                }
            } else {
                div { class: "text-sm text-base-content/70 mb-4",
                    "共 {count} 个关联{related_label}，拖拽节点可重新布局，点击节点查看详情"
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
                    on_node_click: on_node_click.map(|handler| {
                        EventHandler::new(move |id: String| {
                            if let Some((kind, is_center)) = click_map.get(&id) {
                                handler.call(NodeClickEvent {
                                    kind: kind.clone(),
                                    id: id.clone(),
                                    is_center: *is_center,
                                });
                            }
                        })
                    }),
                }
            }
        }
    }
}
