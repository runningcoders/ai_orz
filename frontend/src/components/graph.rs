use dioxus::prelude::*;
use std::collections::HashMap;
use std::f64::consts::PI;

#[derive(Debug, Clone, PartialEq)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub node_type: String,
    pub x: f64,
    pub y: f64,
    /// 标签列表（用于多色边框 + 上方标签展示）
    pub tags: Vec<String>,
    /// 摘要（显示在节点下方一行小字，None 时不显示）
    pub summary: Option<String>,
}

impl Default for GraphNode {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            node_type: String::new(),
            x: 0.0,
            y: 0.0,
            tags: Vec::new(),
            summary: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub label: String,
}

/// 图元素 hover 目标（节点 ID / 边端点对），驱动 hover 详情卡片
#[derive(Debug, Clone, PartialEq)]
enum HoverTarget {
    Node(String),
    Edge(String, String),
}

/// SVG hover 详情卡片渲染数据（锚点为屏幕坐标，卡片不随视图缩放）
struct HoverCard {
    lines: Vec<String>,
    accent: String,
    anchor_x: f64,
    anchor_y: f64,
    /// 卡片与锚点的最小间距（已含缩放）
    offset: f64,
}

/// SVG hover 卡片行文本宽度估算（font-size 11：中文≈11px，ASCII≈6.6px）
fn hover_line_width(s: &str) -> f64 {
    s.chars()
        .fold(0.0, |acc, c| acc + if c.is_ascii() { 6.6 } else { 11.0 })
}

/// SVG hover 卡片尺寸（宽, 高）——规格与 canvas 版 hover_card_size 对齐
fn hover_card_box(lines: &[String]) -> (f64, f64) {
    let w = lines
        .iter()
        .map(|l| hover_line_width(l))
        .fold(0.0f64, f64::max)
        + 16.0;
    (w, lines.len() as f64 * 16.0 + 16.0)
}

/// 构建 hover 详情卡片数据（节点卡与边卡统一结构；锚点为元素屏幕坐标）
fn build_hover_card(
    target: &HoverTarget,
    nodes: &[GraphNode],
    edges: &[GraphEdge],
    positions: &HashMap<String, (f64, f64)>,
    scale: f64,
    pan_x: f64,
    pan_y: f64,
) -> Option<HoverCard> {
    match target {
        HoverTarget::Node(id) => {
            let node = nodes.iter().find(|n| &n.id == id)?;
            let (gx, gy) = positions.get(id).copied().unwrap_or((node.x, node.y));
            let mut lines = vec![
                format!("名称: {}", node.label),
                format!("类型: {}", type_label(&node.node_type)),
            ];
            if let Some(summary) = &node.summary
                && !summary.is_empty()
            {
                lines.push(format!(
                    "摘要: {}",
                    summary.chars().take(30).collect::<String>()
                ));
            }
            if !node.tags.is_empty() {
                lines.push(format!("标签: {}", node.tags.join("、")));
            }
            Some(HoverCard {
                lines,
                accent: get_node_fill(&node.node_type).to_string(),
                anchor_x: gx * scale + pan_x,
                anchor_y: gy * scale + pan_y,
                offset: 18.0,
            })
        }
        HoverTarget::Edge(source, target_id) => {
            let edge = edges
                .iter()
                .find(|e| &e.source == source && &e.target == target_id)?;
            let from = nodes.iter().find(|n| &n.id == source)?;
            let to = nodes.iter().find(|n| &n.id == target_id)?;
            let (fx, fy) = positions.get(source).copied().unwrap_or((from.x, from.y));
            let (gx, gy) = positions.get(target_id).copied().unwrap_or((to.x, to.y));
            let rel: &str = if edge.label.is_empty() {
                "关联"
            } else {
                edge.label.as_str()
            };
            Some(HoverCard {
                lines: vec![
                    format!("关系: {rel}"),
                    format!("端点: {} → {}", from.label, to.label),
                ],
                accent: get_edge_color(rel).to_string(),
                anchor_x: (fx + gx) / 2.0 * scale + pan_x,
                anchor_y: (fy + gy) / 2.0 * scale + pan_y,
                offset: 14.0,
            })
        }
    }
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
pub fn get_node_fill(node_type: &str) -> &'static str {
    match node_type {
        "knowledge_node" => "#3b82f6",
        "short_term" => "#10b981",
        "trace" => "#f59e0b",
        "relation" => "#8b5cf6",
        _ => "#6b7280",
    }
}

/// 预设 tag 色板（鲜艳且可区分）
pub const TAG_COLORS: &[&str] = &[
    "#ef4444", "#f97316", "#f59e0b", "#eab308", "#84cc16", "#10b981", "#06b6d4", "#3b82f6",
    "#8b5cf6", "#ec4899",
];

/// 根据 tag 字符串 hash 稳定取色（同一 tag 始终同色）
pub fn tag_color(tag: &str) -> &'static str {
    let hash: u32 = tag
        .bytes()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
    TAG_COLORS[(hash as usize) % TAG_COLORS.len()]
}

/// 知识图谱节点类型中文名（hover 详情卡 / 详情面板共用映射）
pub fn type_label(t: &str) -> &'static str {
    match t {
        "knowledge_node" => "知识节点",
        "short_term" => "短期记忆",
        "trace" => "调用记录",
        "relation" => "关系",
        _ => "未知",
    }
}

/// 节点边框颜色（选中态）
pub fn get_node_stroke(is_selected: bool) -> &'static str {
    if is_selected { "#f97316" } else { "#ffffff" }
}

/// 节点边框宽度
pub fn get_node_stroke_width(is_selected: bool) -> &'static str {
    if is_selected { "3" } else { "2" }
}

pub fn get_node_opacity(is_highlighted: bool, is_selected: bool) -> &'static str {
    if is_selected || is_highlighted {
        "1"
    } else {
        "0.4"
    }
}

pub fn get_node_glow(is_highlighted: bool, is_selected: bool) -> String {
    if is_selected {
        "filter: drop-shadow(0 0 8px rgba(249, 115, 22, 0.6));".to_string()
    } else if is_highlighted {
        "filter: drop-shadow(0 0 6px rgba(59, 130, 246, 0.5));".to_string()
    } else {
        "".to_string()
    }
}

/// 节点基础半径（按类型）
pub fn base_node_radius(node_type: &str) -> f64 {
    match node_type {
        "knowledge_node" => 26.0,
        "short_term" => 22.0,
        "trace" => 18.0,
        "relation" => 14.0,
        _ => 18.0,
    }
}

/// 根据信息量（tags 数量、是否有简介、名称长度）动态计算节点半径
pub fn dynamic_node_radius(node: &GraphNode) -> f64 {
    let mut r = base_node_radius(&node.node_type);
    // 每个 tag +2（最多 +12）
    r += (node.tags.len() * 2).min(12) as f64;
    // 有简介 +3
    if node.summary.is_some() {
        r += 3.0;
    }
    // 名称较长（>8 字符）+2
    if node.label.chars().count() > 8 {
        r += 2.0;
    }
    r
}

/// 生成 tag 多色边框的 arc path 段
/// 返回每段 (path_d, color)；无 tags 时返回空
fn tag_border_arcs(cx: f64, cy: f64, r: f64, tags: &[String]) -> Vec<(String, &'static str)> {
    if tags.is_empty() {
        return Vec::new();
    }
    let n = tags.len() as f64;
    (0..tags.len())
        .map(|i| {
            let a1 = (i as f64 / n) * 2.0 * PI - PI / 2.0;
            let a2 = ((i + 1) as f64 / n) * 2.0 * PI - PI / 2.0;
            let x1 = cx + r * a1.cos();
            let y1 = cy + r * a1.sin();
            let x2 = cx + r * a2.cos();
            let y2 = cy + r * a2.sin();
            let large = if (a2 - a1) > PI { 1 } else { 0 };
            let d = format!("M {x1:.1} {y1:.1} A {r:.1} {r:.1} 0 {large} 1 {x2:.1} {y2:.1}");
            (d, tag_color(&tags[i]))
        })
        .collect()
}

/// 估算 tag 标签渲染宽度（font-size 9，中文≈9px，英文≈5px）
fn tag_label_width(tag: &str) -> f64 {
    tag.chars()
        .fold(0.0, |acc, c| acc + if c.is_ascii() { 5.0 } else { 9.0 })
        + 8.0 // padding
}

/// 边颜色（按关系中文标签语义分组着色）
pub fn get_edge_color(relation_type: &str) -> &'static str {
    match relation_type {
        "属于" => "#ef4444",
        "包含" | "实例" | "分类" | "属性" | "取值" => "#10b981",
        "相关" | "相似" | "引用" => "#3b82f6",
        "依赖" | "被依赖" | "前置" | "后续" => "#8b5cf6",
        "导致" | "源于" | "相反" => "#ec4899",
        "关联" | "派生" => "#f59e0b",
        _ => "#9ca3af",
    }
}

/// 边虚线样式（依赖类关系用虚线弱化）
pub fn get_edge_dash(relation_type: &str) -> &'static str {
    match relation_type {
        "引用" | "依赖" | "被依赖" | "前置" | "后续" => "5,5",
        _ => "none",
    }
}

/// 边是否使用流光动画（实线边都加流光，虚线边保持静态）
fn edge_use_flow(relation_type: &str) -> bool {
    get_edge_dash(relation_type) == "none"
}

/// 计算两点间距离（用于 stroke-dasharray 估算）
fn edge_length(sx: f64, sy: f64, tx: f64, ty: f64) -> f64 {
    ((tx - sx).powi(2) + (ty - sy).powi(2)).sqrt()
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
    // 修复 H4：use_signal 只在首次初始化，后续 props.nodes 变化不会更新。
    // 修复 M_NEW：之前用 spawn 同步更新会有一帧延迟，首帧渲染时 node_positions 为旧状态。
    // 改为同步代码块直接更新，避免首帧渲染延迟
    let mut node_positions = use_signal(|| {
        let mut pos = HashMap::new();
        for node in &props.nodes {
            pos.insert(node.id.clone(), (node.x, node.y));
        }
        pos
    });

    {
        // 同步增量同步（无需 spawn）：移除已不存在的节点，新增节点用 props 中的 x/y
        let mut pos = node_positions.write();
        let new_ids: std::collections::HashSet<String> =
            props.nodes.iter().map(|n| n.id.clone()).collect();
        pos.retain(|id: &String, _| new_ids.contains(id));
        for node in &props.nodes {
            pos.entry(node.id.clone()).or_insert((node.x, node.y));
        }
    }

    let mut is_dragging = use_signal(|| false);
    let mut dragged_node_id = use_signal(|| None::<String>);
    let mut drag_start = use_signal(|| (0.0, 0.0));
    let mut drag_node_start = use_signal(|| (0.0, 0.0));
    // 修复 M8：drag_moved 标记拖拽是否实际移动超过阈值，用于区分点击与拖拽
    let mut drag_moved = use_signal(|| false);

    let mut view_transform = use_signal(|| (0.0, 0.0, 1.0));
    let mut is_panning = use_signal(|| false);
    let mut pan_start = use_signal(|| (0.0, 0.0));
    // hover 详情卡片目标（区别于点击选中；拖拽/平移中隐藏）
    let mut hovered = use_signal(|| None::<HoverTarget>);

    let svg_width = 800;
    let svg_height = 600;

    #[allow(clippy::type_complexity)]
    let valid_edges: Vec<(GraphEdge, (f64, f64), (f64, f64))> = props
        .edges
        .iter()
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

    let mut handle_mouse_move = move |e: MouseEvent| {
        if is_dragging() {
            let node_id = dragged_node_id.read().clone();
            if let Some(node_id) = node_id {
                let (start_x, start_y) = *drag_start.read();
                let (node_start_x, node_start_y) = *drag_node_start.read();
                let client_pos = e.client_coordinates();
                let dx = client_pos.x - start_x;
                let dy = client_pos.y - start_y;
                // 修复 M8：拖拽距离 > 3px 时标记为 moved，避免松手时误触发点击
                if dx.abs() > 3.0 || dy.abs() > 3.0 {
                    drag_moved.set(true);
                }
                node_positions
                    .write()
                    .insert(node_id, (node_start_x + dx, node_start_y + dy));
            }
        }
    };

    let mut handle_node_drag_start_with_event = move |e: MouseEvent, node_id: String| {
        is_dragging.set(true);
        drag_moved.set(false);
        dragged_node_id.set(Some(node_id.clone()));
        let client_pos = e.client_coordinates();
        drag_start.set((client_pos.x, client_pos.y));
        if let Some(&pos) = node_positions.read().get(&node_id) {
            drag_node_start.set(pos);
        }
    };

    let on_click = props.on_node_click;
    // 修复 M8：mouseup 时若 drag_moved=false 视为点击，调用 on_click
    let handle_mouse_up = move |_| {
        let was_dragging = is_dragging();
        let moved = drag_moved();
        let node_id = dragged_node_id.read().clone();
        is_dragging.set(false);
        dragged_node_id.set(None);
        drag_moved.set(false);
        is_panning.set(false);
        if was_dragging
            && !moved
            && let Some(node_id) = node_id
        {
            on_click.call(node_id);
        }
    };

    // mouseleave 时取消拖拽但不触发点击（用户离开 SVG 视为放弃操作）
    let handle_mouse_leave = move |_| {
        is_dragging.set(false);
        dragged_node_id.set(None);
        drag_moved.set(false);
        is_panning.set(false);
        hovered.set(None);
    };

    let handle_wheel = move |e: WheelEvent| {
        e.prevent_default();
        let (tx, ty, scale) = *view_transform.read();
        let delta_y = e.delta().strip_units().y;
        let delta: f64 = if delta_y > 0.0 { 0.9 } else { 1.1 };
        let new_scale = (scale * delta).clamp(0.5, 2.0);
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
            let (tx, ty, scale) = *view_transform.read();
            let (start_x, start_y) = *pan_start.read();
            let dx = (e.client_coordinates().x - start_x) / scale;
            let dy = (e.client_coordinates().y - start_y) / scale;
            view_transform.set((tx + dx, ty + dy, scale));
            let client_pos = e.client_coordinates();
            pan_start.set((client_pos.x, client_pos.y));
        }
    };

    let (tx, ty, scale) = *view_transform.read();
    let render_nodes = props.nodes.clone();
    let highlighted_ids = props.highlighted_node_ids.clone();
    // 修复 M8：on_click 已移到 handle_mouse_up，节点 mousedown 不再触发点击
    let reset_view = move |_| {
        view_transform.set((0.0, 0.0, 1.0));
    };

    // hover 详情卡片数据（拖拽/平移中隐藏；节点卡与边卡统一结构，锚点为屏幕坐标）
    let hover_card_data = if *is_dragging.read() || *is_panning.read() {
        None
    } else {
        hovered.read().as_ref().and_then(|target| {
            build_hover_card(
                target,
                &props.nodes,
                &props.edges,
                &node_positions.read(),
                scale,
                tx,
                ty,
            )
        })
    };

    rsx! {
        div { class: "relative",
        svg {
            width: "{svg_width}",
            height: "{svg_height}",
            view_box: "0 0 {svg_width} {svg_height}",
            class: "kg-bg rounded-lg",
            onmousemove: move |e: MouseEvent| {
                let e2 = e.clone();
                handle_mouse_move(e);
                handle_pan_move(e2);
            },
            onmouseup: handle_mouse_up,
            onmouseleave: handle_mouse_leave,
            onwheel: handle_wheel,
            oncontextmenu: handle_context_menu,
            onmousedown: handle_pan_start,

            // HUD 四角装饰
            path { class: "kg-corner", d: "M 8 20 L 8 8 L 20 8" }
            path { class: "kg-corner", d: "M {svg_width - 20} 8 L {svg_width - 8} 8 L {svg_width - 8} 20" }
            path { class: "kg-corner", d: "M 8 {svg_height - 20} L 8 {svg_height - 8} L 20 {svg_height - 8}" }
            path { class: "kg-corner", d: "M {svg_width - 20} {svg_height - 8} L {svg_width - 8} {svg_height - 8} L {svg_width - 8} {svg_height - 20}" }

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
                    let use_flow = edge_use_flow(&edge.label);
                    let len = edge_length(sx, sy, tx, ty);
                    let edge_class = if use_flow { "kg-edge-flow kg-edge-glow" } else { "kg-edge-glow" };
                    let edge_style = format!("--len: {len}px; color: {edge_color};");
                    let label_text = if !edge.label.is_empty() {
                        Some(edge.label.chars().take(10).collect::<String>())
                    } else {
                        None
                    };
                    // 事件闭包各自持有独立副本（move 捕获不能共享同一 String 字段）
                    let hover_enter = HoverTarget::Edge(edge.source.clone(), edge.target.clone());
                    let hover_leave = HoverTarget::Edge(edge.source.clone(), edge.target.clone());
                    rsx! {
                        line {
                            x1: "{sx}",
                            y1: "{sy}",
                            x2: "{tx}",
                            y2: "{ty}",
                            stroke: "{edge_color}",
                            stroke_width: "2",
                            stroke_dasharray: "{edge_dash}",
                            class: "{edge_class}",
                            style: "{edge_style}",
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
                                    stroke: "{edge_color}",
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

                        // 透明命中层：放宽边的 hover 命中区（视觉样式不变）
                        line {
                            x1: "{sx}",
                            y1: "{sy}",
                            x2: "{tx}",
                            y2: "{ty}",
                            stroke: "transparent",
                            stroke_width: "12",
                            style: "pointer-events: stroke; cursor: pointer;",
                            onmouseenter: move |_| {
                                hovered.set(Some(hover_enter.clone()));
                            },
                            onmouseleave: move |_| {
                                // 仅清除仍停留在当前元素上的 hover，避免误清新进入的其他元素
                                if hovered.read().as_ref() == Some(&hover_leave) {
                                    hovered.set(None);
                                }
                            },
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
                    let fill = get_node_fill(&node.node_type).to_string();
                    let stroke = get_node_stroke(is_selected).to_string();
                    let stroke_width = get_node_stroke_width(is_selected).to_string();
                    // 动态半径：信息越多节点越大
                    let radius = dynamic_node_radius(&node);
                    let (nx, ny) = node_positions.read().get(&node.id).copied().unwrap_or((node.x, node.y));
                    let opacity = get_node_opacity(is_highlighted, is_selected);
                    let glow = get_node_glow(is_highlighted, is_selected);

                    // 多色边框 arc 段（无 tags 时为空，使用 circle 自身 stroke）
                    let border_r = radius + 3.0;
                    let arcs = tag_border_arcs(nx, ny, border_r, &node.tags);

                    // HUD 外环半径（仅选中态显示旋转刻度环）
                    let ring_r = radius + 8.0;
                    // 扫描环初始/结束半径（仅选中态）
                    let scan_r0 = radius + 4.0;
                    let scan_r1 = radius + 22.0;
                    let scan_style = format!("--r0: {scan_r0}px; --r1: {scan_r1}px;");
                    // 外环四向刻度端点
                    let ring_top_y1 = ny - ring_r - 3.0;
                    let ring_top_y2 = ny - ring_r + 3.0;
                    let ring_right_x1 = nx + ring_r - 3.0;
                    let ring_right_x2 = nx + ring_r + 3.0;
                    let ring_bot_y1 = ny + ring_r - 3.0;
                    let ring_bot_y2 = ny + ring_r + 3.0;
                    let ring_left_x1 = nx - ring_r - 3.0;
                    let ring_left_x2 = nx - ring_r + 3.0;

                    // 节点上方 tags 标签：横向居中排列
                    let tag_label_y = ny - radius - 16.0;
                    let tag_widths: Vec<(String, f64, &'static str)> = node.tags.iter()
                        .map(|t| (t.clone(), tag_label_width(t), tag_color(t)))
                        .collect();
                    let total_tag_w: f64 = tag_widths.iter().map(|(_, w, _)| *w).sum::<f64>() + (tag_widths.len().saturating_sub(1) as f64) * 4.0;
                    let mut tag_x = nx - total_tag_w / 2.0;

                    // 节点下方简介（截断一行）
                    let summary_text = node.summary.as_ref().map(|s| {
                        s.chars().take(14).collect::<String>()
                    });
                    let label_text = node.label.chars().take(10).collect::<String>();

                    // HUD 节点组 class：出现动画 + hover 放大
                    let node_group_class = "kg-node-appear kg-node-group";

                    // 事件闭包各自持有独立副本（move 捕获不能共享同一 String 字段）
                    let hover_enter = HoverTarget::Node(node.id.clone());
                    let hover_leave = HoverTarget::Node(node.id.clone());
                    let node_id_drag = node.id.clone();

                    rsx! {
                        g {
                            class: "{node_group_class}",
                            cursor: "move",
                            style: "{glow}",
                            opacity: "{opacity}",
                            onmouseenter: move |_| {
                                hovered.set(Some(hover_enter.clone()));
                            },
                            onmouseleave: move |_| {
                                // 仅清除仍停留在当前元素上的 hover，避免误清新进入的其他元素
                                if hovered.read().as_ref() == Some(&hover_leave) {
                                    hovered.set(None);
                                }
                            },
                            onmousedown: move |e: MouseEvent| {
                                // 修复 HIGH #7：节点 mousedown 事件冒泡到 svg 的 handle_pan_start，
                                // 导致拖拽节点时 is_dragging 和 is_panning 同时为 true，
                                // 节点位移 = 节点移动 + 视图平移，所有节点拖拽都错乱。
                                // stop_propagation 阻止冒泡，确保拖拽节点时不平移画布。
                                e.stop_propagation();
                                handle_node_drag_start_with_event(e, node_id_drag.clone());
                            },

                            // 选中态：向外扩散的扫描环波纹（雷达扫描效果）
                            if is_selected {
                                circle {
                                    class: "kg-scan-ring",
                                    cx: "{nx}",
                                    cy: "{ny}",
                                    r: "{scan_r0}",
                                    fill: "none",
                                    stroke: "{fill}",
                                    stroke_width: "2",
                                    style: "{scan_style}",
                                }
                            }

                            // 选中态：HUD 外环刻度旋转（瞄准镜风格）
                            if is_selected {
                                g {
                                    class: "kg-ring-spin",
                                    style: "transform-origin: {nx}px {ny}px;",
                                    circle {
                                        cx: "{nx}",
                                        cy: "{ny}",
                                        r: "{ring_r}",
                                        fill: "none",
                                        stroke: "{fill}",
                                        stroke_width: "1",
                                        stroke_dasharray: "3 6",
                                        opacity: "0.6",
                                    }
                                    // 四个刻度小线段（上/右/下/左）
                                    line { x1: "{nx}", y1: "{ring_top_y1}", x2: "{nx}", y2: "{ring_top_y2}", stroke: "{fill}", stroke_width: "1.5" }
                                    line { x1: "{ring_right_x1}", y1: "{ny}", x2: "{ring_right_x2}", y2: "{ny}", stroke: "{fill}", stroke_width: "1.5" }
                                    line { x1: "{nx}", y1: "{ring_bot_y1}", x2: "{nx}", y2: "{ring_bot_y2}", stroke: "{fill}", stroke_width: "1.5" }
                                    line { x1: "{ring_left_x1}", y1: "{ny}", x2: "{ring_left_x2}", y2: "{ny}", stroke: "{fill}", stroke_width: "1.5" }
                                }
                            }

                            // 未选中态：微弱呼吸光晕（节点类型色光圈）
                            if !is_selected {
                                circle {
                                    cx: "{nx}",
                                    cy: "{ny}",
                                    r: "{radius + 2.0}",
                                    fill: "none",
                                    stroke: "{fill}",
                                    stroke_width: "2",
                                    style: "animation: kg-node-pulse 2.4s ease-in-out infinite; transform-origin: {nx}px {ny}px; transform-box: view-box;",
                                }
                            }

                            // 多色边框 arc 段
                            for (d, color) in arcs.iter() {
                                path {
                                    d: "{d}",
                                    fill: "none",
                                    stroke: "{color}",
                                    stroke_width: "3",
                                    stroke_linecap: "round",
                                }
                            }

                            // 节点主体
                            circle {
                                cx: "{nx}",
                                cy: "{ny}",
                                r: "{radius}",
                                fill: "{fill}",
                                stroke: "{stroke}",
                                stroke_width: "{stroke_width}",
                            }

                            // 节点名称（圆心）
                            text {
                                x: "{nx}",
                                y: "{ny}",
                                text_anchor: "middle",
                                dominant_baseline: "middle",
                                font_size: "10",
                                fill: "white",
                                font_weight: "500",
                                "{label_text}"
                            }

                            // 节点下方简介
                            if let Some(ref summary) = summary_text {
                                text {
                                    x: "{nx}",
                                    y: "{ny + radius + 11.0}",
                                    text_anchor: "middle",
                                    font_size: "8",
                                    fill: "#9ca3af",
                                    "{summary}"
                                }
                            }

                            // 节点上方 tags 标签（带颜色底色）
                            for (tag_text, tw, color) in tag_widths.iter() {
                                {
                                    let tx = tag_x;
                                    tag_x += tw + 4.0;
                                    rsx! {
                                        g {
                                            rect {
                                                x: "{tx}",
                                                y: "{tag_label_y}",
                                                width: "{tw}",
                                                height: "12",
                                                rx: "6",
                                                fill: "{color}",
                                            }
                                            text {
                                                x: "{tx + tw / 2.0}",
                                                y: "{tag_label_y + 9.0}",
                                                text_anchor: "middle",
                                                font_size: "8",
                                                fill: "white",
                                                font_weight: "500",
                                                "{tag_text}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            }

            // hover 详情卡片（屏幕坐标，不随视图缩放；pointer-events 穿透避免命中抖动）
            if let Some(card) = hover_card_data {
                {
                    let (card_w, card_h) = hover_card_box(&card.lines);
                    // 避让：默认锚点右侧垂直居中，超出右缘放左侧，纵向夹在画布内
                    let mut bx = card.anchor_x + card.offset;
                    if bx + card_w > svg_width as f64 - 8.0 {
                        bx = card.anchor_x - card.offset - card_w;
                    }
                    let bx = bx.max(8.0);
                    let by = (card.anchor_y - card_h / 2.0)
                        .max(8.0)
                        .min((svg_height as f64 - card_h - 8.0).max(8.0));
                    rsx! {
                        g {
                            style: "pointer-events: none;",
                            rect {
                                x: "{bx}",
                                y: "{by}",
                                width: "{card_w}",
                                height: "{card_h}",
                                rx: "6",
                                fill: "rgba(17, 24, 39, 0.92)",
                                stroke: "{card.accent}",
                                stroke_width: "1.5",
                            }
                            for (i, line_text) in card.lines.iter().enumerate() {
                                text {
                                    x: "{bx + 8.0}",
                                    y: "{by + 8.0 + i as f64 * 16.0 + 11.0}",
                                    font_size: "11",
                                    fill: "#f9fafb",
                                    "{line_text}"
                                }
                            }
                        }
                    }
                }
            }
        }
        // 修复 L16：添加重置视图按钮（右上角，重置缩放和平移到初始状态）
        button {
            class: "btn hud-btn btn-xs btn-ghost absolute top-2 right-2 bg-base-100/80 hover:bg-base-100 shadow-sm",
            r#type: "button",
            title: "重置视图",
            onclick: reset_view,
            "⟲ 重置"
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

    nodes
        .iter()
        .enumerate()
        .map(|(i, node)| {
            let angle = (i as f64 / n) * 2.0 * std::f64::consts::PI - std::f64::consts::FRAC_PI_2;
            GraphNode {
                x: center_x + radius * angle.cos(),
                y: center_y + radius * angle.sin(),
                ..node.clone()
            }
        })
        .collect()
}

/// 将新节点添加到已有布局中（围绕指定中心节点展开）
pub fn expand_layout(
    existing_nodes: &[GraphNode],
    new_nodes: &[GraphNode],
    center_id: &str,
) -> Vec<GraphNode> {
    // 找到中心节点的位置
    let center_pos = existing_nodes
        .iter()
        .find(|n| n.id == center_id)
        .map(|n| (n.x, n.y))
        .unwrap_or((400.0, 300.0));

    // 计算中心节点已有的关联节点数（用于角度偏移）
    let existing_around = existing_nodes.iter().filter(|n| n.id != center_id).count();

    let radius = 150.0;
    let n = new_nodes.len() as f64;
    let start_angle = (existing_around as f64) * 2.0 * std::f64::consts::PI
        / (existing_around + new_nodes.len()).max(1) as f64;

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
