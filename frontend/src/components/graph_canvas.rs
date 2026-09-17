//! 知识图谱 Canvas 适配层
//!
//! 职责只有一件事：把知识图谱的 `GraphNode` / `GraphEdge` 翻译成 `CanvasScene`
//! 的通用 `CanvasNode` / `CanvasEdge`。渲染、力导向、粒子、hover 详情、拖拽
//! 全部复用默认渲染器 —— 业务侧不再维护第二套渲染实现。
//!
//! ⚠️ **历史教训**：这里曾有 400+ 行 `KnowledgeGraphRenderer`（自定义 HUD 渲染器），
//! 但 `CanvasScene` 内部硬编码 `DefaultRenderer`，它**从未被执行**；实际产物是
//! 「圆圈 + 圆下完整 ID」—— 也就是「知识图谱只显示『知识节点』和一个 id」的直接来源。
//! 凡是「实现了但没接线」的代码等价于不存在，还会让人误判现状：注释写着
//! 「用自定义 HUD 效果」，跑出来的却是默认圆圈。

use std::collections::HashSet;

use dioxus::prelude::*;

use crate::components::canvas_scene::{CanvasEdge, CanvasNode, CanvasScene};
use crate::components::graph::{GraphEdge, GraphNode, get_node_fill};
use crate::components::node_card;

/// KnowledgeGraphCanvas Props
#[derive(Props, Clone, PartialEq)]
pub struct KnowledgeGraphCanvasProps {
    /// 节点列表（复用 SVG 版 GraphNode 结构）
    pub nodes: Vec<GraphNode>,
    /// 边列表（复用 SVG 版 GraphEdge 结构）
    pub edges: Vec<GraphEdge>,
    /// 选中节点 ID（受控：详情面板/列表切换选中时同步到画布）
    pub selected_node_id: Option<String>,
    /// 高亮节点 ID 列表（搜索匹配结果）
    pub highlighted_node_ids: Option<Vec<String>>,
    /// 节点点击回调
    pub on_node_click: EventHandler<String>,
    /// 是否自适应父容器尺寸（铺满包裹层，去掉固定 800×600 导致的 HiDPI 溢出）
    #[props(default = true)]
    pub auto_size: bool,
}

/// `GraphNode` → `CanvasNode`
///
/// 卡片尺寸在这里算一次，它同时是**节点等效半径**的来源：力导向的碰撞避让按
/// `radius` 算最小中心距，填成一个小圆半径，168px 宽的卡片照样互相压叠。
fn to_canvas_node(node: &GraphNode, highlighted: &HashSet<String>) -> CanvasNode {
    let body = node_card::body_lines(node.summary.as_deref(), &node.description);
    let (w, h) = (
        node_card::box_width(&node.label, body.len()),
        node_card::box_height(body.len(), !node.tags.is_empty()),
    );
    let color = get_node_fill(&node.node_type).to_string();

    // 先装配再定半径：形态判定只认 `CanvasNode::is_card` 一处，避免适配层
    // 自己再判一次（两处判定漂移 → 圆点却按卡片半径避让，图上全是空档）
    let mut canvas = CanvasNode {
        id: node.id.clone(),
        x: node.x,
        y: node.y,
        radius: 0.0,
        label: node.label.clone(),
        color,
        node_type: Some(node.node_type.clone()),
        layer: None,
        summary: node.summary.clone(),
        description: node.description.clone(),
        tags: node.tags.clone(),
        highlighted: highlighted.contains(&node.id),
    };
    // 有正文 → 卡片（等效半径 = 外接圆半径）；无正文的端点节点 → 小圆点，
    // 半径与 Agent 关系图同档，不硬撑一张空卡片
    canvas.radius = if canvas.is_card() {
        w.max(h) / 2.0
    } else {
        26.0
    };
    canvas
}

/// 知识图谱 Canvas 组件
///
/// 与 SVG 版 `Graph` 视觉对等（共用 `node_card` 的几何与文案），节点数多时走 Canvas。
#[component]
pub fn KnowledgeGraphCanvas(props: KnowledgeGraphCanvasProps) -> Element {
    let highlighted: HashSet<String> = props
        .highlighted_node_ids
        .clone()
        .unwrap_or_default()
        .into_iter()
        .collect();

    let canvas_nodes: Vec<CanvasNode> = props
        .nodes
        .iter()
        .map(|n| to_canvas_node(n, &highlighted))
        .collect();

    let canvas_edges: Vec<CanvasEdge> = props
        .edges
        .iter()
        .map(|e| CanvasEdge {
            from_id: e.source.clone(),
            to_id: e.target.clone(),
            // 关系类型必须进 tag：它既是边着色依据，也是 hover 详情卡的第一行。
            // 此前这里用 `..Default::default()` 把 tag/description 丢成 None，
            // 而 tooltip 在 tag 为 None 时直接 return —— 表现为「连线 hover 没反应」。
            tag: (!e.label.is_empty()).then(|| e.label.clone()),
            description: None,
        })
        .collect();

    let on_click = props.on_node_click;

    rsx! {
        // 包裹层提供确定高度（height:100% 需要父级有明确高度），canvas 内部自测量铺满
        div { class: "w-full h-[560px]",
            CanvasScene {
                width: 800.0,
                height: 600.0,
                transparent: props.auto_size,
                nodes: canvas_nodes,
                edges: canvas_edges,
                selected_node_id: props.selected_node_id,
                // 与 Agent 关系图对齐：力导向把重叠的卡片自然推开，粒子提供「活」的反馈。
                // 此前这四项全关（注释写「用自定义 HUD 效果替代」），而那个自定义渲染器
                // 从未执行 → 画布既没有动效也没有静态样式，就是「傻傻的」的来源。
                enable_force_layout: true,
                enable_data_flow_particles: true,
                enable_glow_particles: true,
                enable_background_particles: true,
                enable_birth_death_particles: true,
                on_node_click: Some(EventHandler::new(move |id: String| {
                    on_click.call(id);
                })),
            }
        }
    }
}
