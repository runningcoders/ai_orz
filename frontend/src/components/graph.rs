use dioxus::prelude::*;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub node_type: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub label: String,
}

#[derive(Props, Clone, PartialEq)]
pub struct GraphProps {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub selected_node_id: Option<String>,
    on_node_click: EventHandler<String>,
}

/// 节点填充颜色
fn get_node_fill(node_type: &str) -> &'static str {
    match node_type {
        "knowledge_node" => "#3b82f6",
        "short_term" => "#10b981",
        "trace" => "#f59e0b",
        "relation" => "#8b5cf6",
        _ => "#6b7280",
    }
}

/// 节点边框颜色（选中态）
fn get_node_stroke(is_selected: bool) -> &'static str {
    if is_selected { "#f97316" } else { "#ffffff" }
}

/// 节点边框宽度
fn get_node_stroke_width(is_selected: bool) -> &'static str {
    if is_selected { "3" } else { "2" }
}

/// 节点半径
fn get_node_radius(node_type: &str) -> f64 {
    match node_type {
        "knowledge_node" => 24.0,
        "short_term" => 20.0,
        "trace" => 16.0,
        "relation" => 14.0,
        _ => 18.0,
    }
}

#[component]
pub fn Graph(props: GraphProps) -> Element {
    let node_positions = use_signal(|| {
        let mut pos = HashMap::new();
        for node in &props.nodes {
            pos.insert(node.id.clone(), (node.x, node.y));
        }
        pos
    });

    let svg_width = 800;
    let svg_height = 600;

    let valid_edges: Vec<(GraphEdge, (f64, f64), (f64, f64))> = props.edges.iter()
        .filter_map(|e| {
            if let (Some(&source_pos), Some(&target_pos)) = (
                node_positions.read().get(&e.source),
                node_positions.read().get(&e.target),
            ) {
                Some((e.clone(), source_pos, target_pos))
            } else {
                None
            }
        })
        .collect();

    let selected_id = props.selected_node_id.clone();

    rsx! {
        svg {
            width: "{svg_width}",
            height: "{svg_height}",
            view_box: "0 0 {svg_width} {svg_height}",
            style: "border: 1px solid var(--border-color); border-radius: 8px; background: var(--bg-card);",

            // 箭头标记定义
            defs {
                marker {
                    id: "arrowhead",
                    marker_width: "10",
                    marker_height: "7",
                    ref_x: "10",
                    ref_y: "3.5",
                    orient: "auto",
                    polygon {
                        points: "0 0, 10 3.5, 0 7",
                        fill: "#9ca3af",
                    }
                }
            }

            // 边
            for (edge, (sx, sy), (tx, ty)) in &valid_edges {
                line {
                    x1: "{sx}",
                    y1: "{sy}",
                    x2: "{tx}",
                    y2: "{ty}",
                    stroke: "#9ca3af",
                    stroke_width: "1.5",
                    marker_end: "url(#arrowhead)",
                }
                if !edge.label.is_empty() {
                    text {
                        x: "{(sx + tx) / 2.0}",
                        y: "{(sy + ty) / 2.0 - 8.0}",
                        text_anchor: "middle",
                        font_size: "11",
                        fill: "#6b7280",
                        "{edge.label.chars().take(12).collect::<String>()}"
                    }
                }
            }

            // 节点
            for node in &props.nodes {
                {
                    let is_selected = selected_id.as_deref() == Some(&node.id);
                    let node_id_for_click = node.id.clone();
                    let fill = get_node_fill(&node.node_type).to_string();
                    let stroke = get_node_stroke(is_selected).to_string();
                    let stroke_width = get_node_stroke_width(is_selected).to_string();
                    let radius = get_node_radius(&node.node_type);
                    rsx! {
                        g {
                            cursor: "pointer",
                            onclick: move |_| {
                                props.on_node_click.call(node_id_for_click.clone());
                            },
                            circle {
                                cx: "{node.x}",
                                cy: "{node.y}",
                                r: "{radius}",
                                fill: "{fill}",
                                stroke: "{stroke}",
                                stroke_width: "{stroke_width}",
                            }
                            text {
                                x: "{node.x}",
                                y: "{node.y}",
                                text_anchor: "middle",
                                dominant_baseline: "middle",
                                font_size: "10",
                                fill: "white",
                                font_weight: "500",
                                "{node.label.chars().take(6).collect::<String>()}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 以中心节点为原点，关联节点围绕分布的布局算法
pub fn calculate_layout(nodes: &[GraphNode], center_id: Option<&str>) -> Vec<GraphNode> {
    if nodes.is_empty() {
        return Vec::new();
    }

    let center_x = 400.0;
    let center_y = 300.0;

    // 如果有中心节点，将其放在正中
    if let Some(cid) = center_id {
        let mut result = Vec::new();
        let mut others = Vec::new();

        for node in nodes {
            if node.id == cid {
                result.push(GraphNode {
                    x: center_x,
                    y: center_y,
                    ..node.clone()
                });
            } else {
                others.push(node.clone());
            }
        }

        let radius = 180.0;
        let n = others.len() as f64;
        for (i, node) in others.into_iter().enumerate() {
            let angle = (i as f64 / n) * 2.0 * std::f64::consts::PI - std::f64::consts::FRAC_PI_2;
            result.push(GraphNode {
                x: center_x + radius * angle.cos(),
                y: center_y + radius * angle.sin(),
                ..node
            });
        }

        return result;
    }

    // 无中心节点：圆形布局
    let radius = 200.0;
    let n = nodes.len() as f64;

    nodes.iter().enumerate().map(|(i, node)| {
        let angle = (i as f64 / n) * 2.0 * std::f64::consts::PI - std::f64::consts::FRAC_PI_2;
        GraphNode {
            x: center_x + radius * angle.cos(),
            y: center_y + radius * angle.sin(),
            ..node.clone()
        }
    }).collect()
}

/// 将新节点添加到已有布局中（围绕指定中心节点展开）
pub fn expand_layout(
    existing_nodes: &[GraphNode],
    new_nodes: &[GraphNode],
    center_id: &str,
) -> Vec<GraphNode> {
    // 找到中心节点的位置
    let center_pos = existing_nodes.iter()
        .find(|n| n.id == center_id)
        .map(|n| (n.x, n.y))
        .unwrap_or((400.0, 300.0));

    // 计算中心节点已有的关联节点数（用于角度偏移）
    let existing_around = existing_nodes.iter()
        .filter(|n| n.id != center_id)
        .count();

    let radius = 150.0;
    let n = new_nodes.len() as f64;
    let start_angle = (existing_around as f64) * 2.0 * std::f64::consts::PI / (existing_around + new_nodes.len()).max(1) as f64;

    let mut result = existing_nodes.to_vec();
    for (i, node) in new_nodes.iter().enumerate() {
        let angle = start_angle + (i as f64 / n) * 2.0 * std::f64::consts::PI;
        result.push(GraphNode {
            x: center_pos.0 + radius * angle.cos(),
            y: center_pos.1 + radius * angle.sin(),
            ..node.clone()
        });
    }

    result
}
