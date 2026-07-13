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
    pub on_node_click: Option<EventHandler<String>>,
}

fn get_node_color(node_type: &str) -> &str {
    match node_type {
        "knowledge_node" => "var(--color-primary)",
        "short_term" => "var(--color-accent)",
        "relation" => "var(--color-muted)",
        _ => "var(--color-neutral)",
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

    rsx! {
        svg {
            width: "{svg_width}",
            height: "{svg_height}",
            view_box: "0 0 {svg_width} {svg_height}",
            style: "border: 1px solid var(--border-color); border-radius: 8px; background: var(--bg-card);",

            for (edge, (sx, sy), (tx, ty)) in valid_edges {
                line {
                    x1: "{sx}",
                    y1: "{sy}",
                    x2: "{tx}",
                    y2: "{ty}",
                    stroke: "var(--color-muted)",
                    stroke_width: "2",
                }
                text {
                    x: "{(sx + tx) / 2.0}",
                    y: "{(sy + ty) / 2.0 - 5.0}",
                    text_anchor: "middle",
                    class: "text-xs fill-accent",
                    "{edge.label}"
                }
            }

            for node in &props.nodes {
                g {
                    cursor: "pointer",
                    circle {
                        cx: "{node.x}",
                        cy: "{node.y}",
                        r: "20",
                        fill: "{get_node_color(&node.node_type)}",
                        stroke: "white",
                        stroke_width: "2",
                    }
                    text {
                        x: "{node.x}",
                        y: "{node.y}",
                        text_anchor: "middle",
                        dominant_baseline: "middle",
                        class: "text-xs fill-white font-medium",
                        "{node.label.chars().take(8).collect::<String>()}"
                    }
                }
            }
        }
    }
}

pub fn calculate_layout(nodes: &[GraphNode]) -> Vec<GraphNode> {
    let radius = 200.0;
    let center_x = 400.0;
    let center_y = 300.0;
    let n = nodes.len() as f64;
    
    nodes.iter().enumerate().map(|(i, node)| {
        let angle = (i as f64 / n) * 2.0 * std::f64::consts::PI;
        GraphNode {
            x: center_x + radius * angle.cos(),
            y: center_y + radius * angle.sin(),
            ..node.clone()
        }
    }).collect()
}