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
    pub highlighted_node_ids: Option<Vec<String>>,
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

fn get_node_opacity(is_highlighted: bool, is_selected: bool) -> &'static str {
    if is_selected || is_highlighted { "1" } else { "0.4" }
}

fn get_node_glow(is_highlighted: bool, is_selected: bool) -> String {
    if is_selected {
        "filter: drop-shadow(0 0 8px rgba(249, 115, 22, 0.6));".to_string()
    } else if is_highlighted {
        "filter: drop-shadow(0 0 6px rgba(59, 130, 246, 0.5));".to_string()
    } else {
        "".to_string()
    }
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

/// 边颜色（根据关系类型）
fn get_edge_color(relation_type: &str) -> &'static str {
    match relation_type {
        "属于" => "#ef4444",
        "引用" => "#3b82f6",
        "包含" => "#10b981",
        "关联" => "#f59e0b",
        "派生" => "#8b5cf6",
        "依赖" => "#ec4899",
        _ => "#9ca3af",
    }
}

/// 边虚线样式（根据关系类型）
fn get_edge_dash(relation_type: &str) -> &'static str {
    match relation_type {
        "引用" | "依赖" => "5,5",
        _ => "none",
    }
}

fn calculate_edge_angle(sx: f64, sy: f64, tx: f64, ty: f64) -> f64 {
    let dx = tx - sx;
    let dy = ty - sy;
    dy.atan2(dx) * 180.0 / std::f64::consts::PI
}

fn get_label_transform(sx: f64, sy: f64, tx: f64, ty: f64) -> String {
    let mid_x = (sx + tx) / 2.0;
    let mid_y = (sy + ty) / 2.0;
    let angle = calculate_edge_angle(sx, sy, tx, ty);
    format!("translate({}, {}) rotate({})", mid_x, mid_y - 8.0, angle)
}

#[component]
pub fn Graph(props: GraphProps) -> Element {
    let initial_nodes = props.nodes.clone();
    let mut node_positions = use_signal(|| {
        let mut pos = HashMap::new();
        for node in &initial_nodes {
            pos.insert(node.id.clone(), (node.x, node.y));
        }
        pos
    });

    let mut is_dragging = use_signal(|| false);
    let mut dragged_node_id = use_signal(|| None::<String>);
    let mut drag_start = use_signal(|| (0.0, 0.0));
    let mut drag_node_start = use_signal(|| (0.0, 0.0));

    let mut view_transform = use_signal(|| (0.0, 0.0, 1.0));
    let mut is_panning = use_signal(|| false);
    let mut pan_start = use_signal(|| (0.0, 0.0));

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

    let mut handle_node_drag_start = move |node_id: String| {
        is_dragging.set(true);
        dragged_node_id.set(Some(node_id.clone()));
        if let Some(&pos) = node_positions.read().get(&node_id) {
            drag_node_start.set(pos);
        }
    };

    let mut handle_mouse_move = move |e: MouseEvent| {
        if is_dragging() {
            let node_id = dragged_node_id.read().clone();
            if let Some(node_id) = node_id {
                let (start_x, start_y) = drag_start.read().clone();
                let (node_start_x, node_start_y) = drag_node_start.read().clone();
                let client_pos = e.client_coordinates();
                let dx = client_pos.x - start_x;
                let dy = client_pos.y - start_y;
                node_positions.write().insert(node_id, (node_start_x + dx, node_start_y + dy));
            }
        }
    };

    let mut handle_node_drag_start_with_event = move |e: MouseEvent, node_id: String| {
        is_dragging.set(true);
        dragged_node_id.set(Some(node_id.clone()));
        let client_pos = e.client_coordinates();
        drag_start.set((client_pos.x, client_pos.y));
        if let Some(&pos) = node_positions.read().get(&node_id) {
            drag_node_start.set(pos);
        }
    };

    let handle_mouse_up = move |_| {
        is_dragging.set(false);
        dragged_node_id.set(None);
        is_panning.set(false);
    };

    let handle_wheel = move |e: WheelEvent| {
        e.prevent_default();
        let (tx, ty, scale) = view_transform.read().clone();
        let delta_y = e.delta().strip_units().y;
        let delta = if delta_y > 0.0 { 0.9 } else { 1.1 };
        let new_scale = ((scale * delta) as f64).max(0.5).min(2.0);
        view_transform.set((tx, ty, new_scale));
    };

    let handle_context_menu = move |e: MouseEvent| {
        e.prevent_default();
    };

    let handle_pan_start = move |e: MouseEvent| {
        if e.held_buttons().len() == 1 {
            is_panning.set(true);
            pan_start.set((e.client_coordinates().x, e.client_coordinates().y));
        }
    };

    let mut handle_pan_move = move |e: MouseEvent| {
        if is_panning() {
            let (tx, ty, scale) = view_transform.read().clone();
            let (start_x, start_y) = pan_start.read().clone();
            let dx = (e.client_coordinates().x - start_x) / scale;
            let dy = (e.client_coordinates().y - start_y) / scale;
            view_transform.set((tx + dx, ty + dy, scale));
            pan_start.set((e.client_coordinates().x, e.client_coordinates().y));
        }
    };

    let (tx, ty, scale) = view_transform.read().clone();
    let render_nodes = props.nodes.clone();
    let render_edges = props.edges.clone();
    let highlighted_ids = props.highlighted_node_ids.clone();
    let on_click = props.on_node_click.clone();

    rsx! {
        svg {
            width: "{svg_width}",
            height: "{svg_height}",
            view_box: "0 0 {svg_width} {svg_height}",
            style: "border: 1px solid var(--border-color); border-radius: 8px; background: var(--bg-card);",
            onmousemove: move |e: MouseEvent| {
                let e2 = e.clone();
                handle_mouse_move(e);
                handle_pan_move(e2);
            },
            onmouseup: handle_mouse_up,
            onmouseleave: handle_mouse_up,
            onwheel: handle_wheel,
            oncontextmenu: handle_context_menu,
            onmousedown: handle_pan_start,

            g {
                transform: "translate({tx}, {ty}) scale({scale})",

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
            for (edge, (sx, sy), (tx, ty)) in valid_edges.into_iter() {
                {
                    let edge_color = get_edge_color(&edge.label);
                    let edge_dash = get_edge_dash(&edge.label);
                    let label_text = if !edge.label.is_empty() {
                        Some(edge.label.chars().take(10).collect::<String>())
                    } else {
                        None
                    };
                    rsx! {
                        line {
                            x1: "{sx}",
                            y1: "{sy}",
                            x2: "{tx}",
                            y2: "{ty}",
                            stroke: "{edge_color}",
                            stroke_width: "2",
                            stroke_dasharray: "{edge_dash}",
                            marker_end: "url(#arrowhead)",
                        }
                        if let Some(ref label) = label_text {
                            g {
                                transform: "{get_label_transform(sx, sy, tx, ty)}",
                                rect {
                                    x: "-{label.len() as f64 * 3.5}",
                                    y: "-7",
                                    width: "{label.len() as f64 * 7.0 + 4.0}",
                                    height: "14",
                                    rx: "2",
                                    fill: "rgba(255, 255, 255, 0.9)",
                                    stroke: "#e5e7eb",
                                    stroke_width: "1",
                                }
                                text {
                                    x: "0",
                                    y: "2",
                                    text_anchor: "middle",
                                    font_size: "10",
                                    fill: "#374151",
                                    font_weight: "500",
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            }

            // 节点
            for node in render_nodes.into_iter() {
                {
                    let is_selected = selected_id.as_deref() == Some(&node.id);
                    let is_highlighted = highlighted_ids.as_ref()
                        .map(|ids| ids.contains(&node.id))
                        .unwrap_or(false);
                    let node_id_for_click = node.id.clone();
                    let fill = get_node_fill(&node.node_type).to_string();
                    let stroke = get_node_stroke(is_selected).to_string();
                    let stroke_width = get_node_stroke_width(is_selected).to_string();
                    let radius = get_node_radius(&node.node_type);
                    let (nx, ny) = node_positions.read().get(&node.id).copied().unwrap_or((node.x, node.y));
                    let opacity = get_node_opacity(is_highlighted, is_selected);
                    let glow = get_node_glow(is_highlighted, is_selected);
                    rsx! {
                        g {
                            cursor: "move",
                            style: "{glow}",
                            opacity: "{opacity}",
                            onmousedown: move |e: MouseEvent| {
                                handle_node_drag_start_with_event(e, node.id.clone());
                                on_click.call(node_id_for_click.clone());
                            },
                            circle {
                                cx: "{nx}",
                                cy: "{ny}",
                                r: "{radius}",
                                fill: "{fill}",
                                stroke: "{stroke}",
                                stroke_width: "{stroke_width}",
                            }
                            text {
                                x: "{nx}",
                                y: "{ny}",
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
