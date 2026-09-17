use dioxus::prelude::*;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct GraphNode {
    pub id: String,
    /// 展示名：**名称优先，回退正文首行**（由调用方挑好，渲染层不碰 ID）
    pub label: String,
    /// 正文/描述：卡片上只放前两行，完整内容留给 hover 详情
    pub description: String,
    pub node_type: String,
    pub x: f64,
    pub y: f64,
    /// 标签列表（卡片底部胶囊，最多 3 个 + 聚合计数）
    pub tags: Vec<String>,
    /// 摘要（优先于正文作为卡片正文行，None 时回退 description）
    pub summary: Option<String>,
}

impl Default for GraphNode {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            description: String::new(),
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
            Some(HoverCard {
                lines: node_hover_lines(node),
                accent: get_node_fill(&node.node_type).to_string(),
                anchor_x: gx * scale + pan_x,
                anchor_y: gy * scale + pan_y,
                // 矩形卡片：锚点偏移用半宽（圆形时代是固定 18）
                offset: NODE_BOX_W / 2.0 * scale + 12.0,
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

/// 节点组不透明度
///
/// ⚠️ 未高亮档位不能用圆形的 0.4：矩形卡片里还有正文小字，
/// 0.4 会让「展开出来的邻居节点」整片糊掉读不清；高亮的区分度交给发光 + 边框承担。
pub fn get_node_opacity(is_highlighted: bool, is_selected: bool) -> &'static str {
    if is_selected || is_highlighted {
        "1"
    } else {
        "0.78"
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

// ==================== 矩形节点卡片几何（canvas / SVG 双端 SSOT） ====================
//
// 知识节点此前是圆形：只能塞下 10 个字 + 一个 ID，信息量几乎为零。
// 改矩形卡片后画布上直接展示「名称 + 摘要/描述两行 + 标签」，ID 退到 hover 详情。
// ⚠️ 卡片尺寸 / 折行 / 截断全部收敛在此：canvas 与 SVG 两条渲染路径都读这里，
// 各画一套必然漂移（改了宽度忘了改折行宽度 = 文字溢出卡片）。

/// 卡片固定宽度（高度按内容浮动，见 [`node_box_height`]）
pub const NODE_BOX_W: f64 = 168.0;
/// 卡片圆角
pub const NODE_BOX_R: f64 = 6.0;
/// 卡片内边距
pub const NODE_BOX_PAD: f64 = 8.0;
/// 左侧类型色竖条宽度（取代圆形的整体填充，保留类型辨识度）
pub const NODE_ACCENT_W: f64 = 4.0;
/// 标题字号 / 行高
pub const NODE_TITLE_PX: f64 = 12.0;
pub const NODE_TITLE_H: f64 = 15.0;
/// 正文字号 / 行高 / 最大行数
pub const NODE_BODY_PX: f64 = 10.0;
pub const NODE_BODY_H: f64 = 13.0;
pub const NODE_BODY_MAX_LINES: usize = 2;
/// 标签胶囊高度
pub const NODE_TAG_H: f64 = 14.0;
/// 卡片内标签最多展示个数（超出聚合为 `+N`）
pub const NODE_TAG_MAX: usize = 3;
/// hover 详情卡正文折行宽度（与 canvas_scene 的 11px 卡片字号配套）
pub const HOVER_TEXT_W: f64 = 280.0;
pub const HOVER_FONT_PX: f64 = 11.0;

/// 卡片内容区可用宽度（扣掉竖条与左右内边距）
pub fn node_content_w() -> f64 {
    NODE_BOX_W - NODE_ACCENT_W - NODE_BOX_PAD * 2.0
}

/// 文本渲染宽度估算：CJK≈字号，ASCII≈0.55×字号
///
/// canvas 侧有 `measure_text_width` 可用真实度量，但 SVG 只能估算；
/// 两边共用这一套估算，卡片的折行结果才一致。
pub fn text_width(s: &str, font_px: f64) -> f64 {
    s.chars().fold(0.0, |acc, c| {
        acc + if c.is_ascii() {
            font_px * 0.55
        } else {
            font_px
        }
    })
}

/// 按像素宽度折行，最多 `max_lines` 行；超出时末行截断加省略号
pub fn wrap_text(s: &str, max_width: f64, font_px: f64, max_lines: usize) -> Vec<String> {
    // 正文常含换行/Markdown 换行，先摊平成单行再折，避免卡片里出现半截空行
    let flat: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() || max_lines == 0 {
        return Vec::new();
    }
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for ch in flat.chars() {
        let probe: String = cur.chars().chain(std::iter::once(ch)).collect();
        if !cur.is_empty() && text_width(&probe, font_px) > max_width {
            lines.push(std::mem::take(&mut cur));
        }
        cur.push(ch);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() {
            let mut t = last.clone();
            while !t.is_empty() && text_width(&format!("{t}…"), font_px) > max_width {
                t.pop();
            }
            t.push('…');
            *last = t;
        }
    }
    lines
}

/// 截断到 `max` 个字符（超出加省略号）
pub fn truncate_chars(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut t: String = s.chars().take(max - 1).collect();
    t.push('…');
    t
}

/// 卡片高度：标题行 + 正文两行（+ 标签行）
pub fn node_box_height(node: &GraphNode) -> f64 {
    let mut h = NODE_BOX_PAD * 2.0 + NODE_TITLE_H + NODE_BODY_H * NODE_BODY_MAX_LINES as f64;
    if !node.tags.is_empty() {
        h += NODE_TAG_H + 3.0;
    }
    h
}

/// 卡片标题（展示名单行，超出省略）
pub fn node_title(node: &GraphNode) -> String {
    wrap_text(&node.label, node_content_w(), NODE_TITLE_PX, 1)
        .pop()
        .unwrap_or_default()
}

/// 卡片正文行：摘要优先，回退正文描述
pub fn node_body_lines(node: &GraphNode) -> Vec<String> {
    let summary = node.summary.as_deref().map(str::trim).unwrap_or("");
    let text = if summary.is_empty() {
        node.description.trim()
    } else {
        summary
    };
    if text.is_empty() {
        return Vec::new();
    }
    wrap_text(text, node_content_w(), NODE_BODY_PX, NODE_BODY_MAX_LINES)
}

/// 卡片内标签胶囊（最多 `NODE_TAG_MAX` 个，超出聚合为 `+N`）
pub fn node_tag_chips(node: &GraphNode) -> Vec<(String, &'static str)> {
    if node.tags.is_empty() {
        return Vec::new();
    }
    let mut chips: Vec<(String, &'static str)> = node
        .tags
        .iter()
        .take(NODE_TAG_MAX)
        .map(|t| (t.clone(), tag_color(t)))
        .collect();
    let rest = node.tags.len().saturating_sub(NODE_TAG_MAX);
    if rest > 0 {
        chips.push((format!("+{rest}"), "#4b5563"));
    }
    chips
}

/// hover 详情行：名称 / 类型 / 标签 / 摘要 / 描述 / ID
///
/// ID 只在这里出现 —— 画布卡片上不再直出 ID（用户反馈「没有有用的信息」）。
pub fn node_hover_lines(node: &GraphNode) -> Vec<String> {
    let mut lines = vec![
        format!("名称: {}", node.label),
        format!("类型: {}", type_label(&node.node_type)),
    ];
    if !node.tags.is_empty() {
        lines.push(format!("标签: {}", node.tags.join("、")));
    }
    let summary = node.summary.as_deref().map(str::trim).unwrap_or("");
    if !summary.is_empty() {
        lines.extend(field_lines("摘要", summary));
    }
    let desc = node.description.trim();
    if !desc.is_empty() {
        lines.extend(field_lines("描述", desc));
    }
    lines.push(format!("ID: {}", node.id));
    lines
}

/// 带字段名前缀的折行（续行缩进对齐到字段名之后）
fn field_lines(label: &str, value: &str) -> Vec<String> {
    let indent = " ".repeat(label.chars().count() * 2 + 2);
    wrap_text(value, HOVER_TEXT_W, HOVER_FONT_PX, 2)
        .into_iter()
        .enumerate()
        .map(|(i, l)| {
            if i == 0 {
                format!("{label}: {l}")
            } else {
                format!("{indent}{l}")
            }
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
                    let (nx, ny) = node_positions.read().get(&node.id).copied().unwrap_or((node.x, node.y));
                    let opacity = get_node_opacity(is_highlighted, is_selected);
                    let glow = get_node_glow(is_highlighted, is_selected);

                    // === 矩形卡片几何（与 canvas 渲染器共用 SSOT）===
                    let box_h = node_box_height(&node);
                    let box_x = nx - NODE_BOX_W / 2.0;
                    let box_y = ny - box_h / 2.0;
                    // 卡片内文字起始 x（竖条 + 左内边距）
                    let text_x = box_x + NODE_ACCENT_W + NODE_BOX_PAD;
                    let title_text = node_title(&node);
                    let body_lines = node_body_lines(&node);
                    let title_baseline = box_y + NODE_BOX_PAD + 11.0;
                    let body_baseline0 = box_y + NODE_BOX_PAD + NODE_TITLE_H + NODE_BODY_H - 3.0;
                    // 标签胶囊：卡片底部一行（无标签时不渲染）
                    let tag_chips = node_tag_chips(&node);
                    let tag_row_y = box_y + NODE_BOX_PAD + NODE_TITLE_H + NODE_BODY_H * NODE_BODY_MAX_LINES as f64 + 3.0;
                    let tag_widths: Vec<(String, f64, &'static str)> = tag_chips
                        .iter()
                        .map(|(t, c)| (t.clone(), tag_label_width(t), *c))
                        .collect();
                    let mut tag_x = text_x;

                    // 事件闭包各自持有独立副本（move 捕获不能共享同一 String 字段）
                    let hover_enter = HoverTarget::Node(node.id.clone());
                    let hover_leave = HoverTarget::Node(node.id.clone());
                    let node_id_drag = node.id.clone();

                    rsx! {
                        g {
                            class: "kg-node-appear kg-node-group",
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

                            // 选中态：向外扩散的扫描框（矩形版扫描环）
                            if is_selected {
                                rect {
                                    class: "kg-box-scan",
                                    x: "{box_x - 5.0}",
                                    y: "{box_y - 5.0}",
                                    width: "{NODE_BOX_W + 10.0}",
                                    height: "{box_h + 10.0}",
                                    rx: "10",
                                    fill: "none",
                                    stroke: "#f97316",
                                    stroke_width: "1.5",
                                    stroke_dasharray: "6 4",
                                }
                            } else {
                                // 未选中态：类型色呼吸框（矩形版呼吸光晕）
                                rect {
                                    class: "kg-box-pulse",
                                    x: "{box_x - 3.0}",
                                    y: "{box_y - 3.0}",
                                    width: "{NODE_BOX_W + 6.0}",
                                    height: "{box_h + 6.0}",
                                    rx: "9",
                                    fill: "none",
                                    stroke: "{fill}",
                                    stroke_width: "2",
                                }
                            }

                            // 卡片主体
                            rect {
                                x: "{box_x}",
                                y: "{box_y}",
                                width: "{NODE_BOX_W}",
                                height: "{box_h}",
                                rx: "{NODE_BOX_R}",
                                fill: "rgba(17, 24, 39, 0.94)",
                                stroke: "{stroke}",
                                stroke_width: "{stroke_width}",
                            }

                            // 左侧类型色竖条（取代圆形整体填充，保留类型辨识度）
                            rect {
                                x: "{box_x}",
                                y: "{box_y}",
                                width: "{NODE_ACCENT_W}",
                                height: "{box_h}",
                                rx: "2",
                                fill: "{fill}",
                            }

                            // 名称
                            text {
                                x: "{text_x}",
                                y: "{title_baseline}",
                                font_size: "{NODE_TITLE_PX}",
                                fill: "#f9fafb",
                                font_weight: "600",
                                "{title_text}"
                            }

                            // 摘要 / 描述（最多两行）
                            for (i, line) in body_lines.iter().enumerate() {
                                text {
                                    x: "{text_x}",
                                    y: "{body_baseline0 + i as f64 * NODE_BODY_H}",
                                    font_size: "{NODE_BODY_PX}",
                                    fill: "#9ca3af",
                                    "{line}"
                                }
                            }

                            // 标签胶囊（卡片底部一行，最多 3 个 + 聚合计数）
                            for (tag_text, tw, color) in tag_widths.iter() {
                                {
                                    let tx = tag_x;
                                    tag_x += tw + 4.0;
                                    rsx! {
                                        g {
                                            rect {
                                                x: "{tx}",
                                                y: "{tag_row_y}",
                                                width: "{tw}",
                                                height: "{NODE_TAG_H - 2.0}",
                                                rx: "6",
                                                fill: "{color}",
                                            }
                                            text {
                                                x: "{tx + tw / 2.0}",
                                                y: "{tag_row_y + 9.0}",
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

        // 辐射半径需 ≥ 卡片宽度（168）+ 间隔：圆形时代 180 对矩形卡片太挤，会叠在一起
        let radius = 250.0;
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

    // 同上：矩形卡片按 240px 半径铺开，避免新节点压在中心卡片上
    let radius = 240.0;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn node(label: &str, summary: Option<&str>, desc: &str, tags: &[&str]) -> GraphNode {
        GraphNode {
            id: "01J8ZKQ7X4M2N5P6R8T0VWXYZ".to_string(),
            label: label.to_string(),
            description: desc.to_string(),
            node_type: "knowledge_node".to_string(),
            x: 100.0,
            y: 100.0,
            tags: tags.iter().map(|t| t.to_string()).collect(),
            summary: summary.map(|s| s.to_string()),
        }
    }

    #[test]
    fn text_width_counts_cjk_as_full_width() {
        // CJK 按字号满宽，ASCII 按 0.55 折半（与 SVG 估算口径一致）
        assert_eq!(text_width("知识", 10.0), 20.0);
        assert_eq!(text_width("abc", 10.0), 16.5);
        assert_eq!(text_width("", 10.0), 0.0);
    }

    #[test]
    fn wrap_text_respects_max_lines_and_ellipsis() {
        let long = "知识节点描述很长很长很长很长很长很长很长很长很长很长";
        let lines = wrap_text(long, 100.0, 10.0, 2);
        assert_eq!(lines.len(), 2, "超出 max_lines 必须被截断: {lines:?}");
        assert!(lines[1].ends_with('…'), "末行被截断时应带省略号: {lines:?}");
        // 每一行都不得超过可用宽度
        for l in &lines {
            assert!(text_width(l, 10.0) <= 100.0, "行超宽: {l:?}");
        }
    }

    #[test]
    fn wrap_text_flattens_newlines() {
        // 正文里的换行摊平成单行（单词间保留一个空格），否则卡片里会出现半截空行
        let lines = wrap_text("第一行\n第二行", 1000.0, 10.0, 2);
        assert_eq!(lines, vec!["第一行 第二行".to_string()]);
    }

    #[test]
    fn truncate_chars_keeps_boundary() {
        assert_eq!(truncate_chars("知识节点", 10), "知识节点");
        assert_eq!(truncate_chars("知识节点名称超长", 5), "知识节点…");
        assert_eq!(truncate_chars("知识节点", 0), "");
    }

    #[test]
    fn box_height_grows_with_tags() {
        let plain = node("名称", None, "", &[]);
        let tagged = node("名称", None, "", &["a"]);
        assert!(
            node_box_height(&tagged) > node_box_height(&plain),
            "有标签时卡片要更高"
        );
        // 无标签：上下内边距 + 标题行 + 正文两行
        assert_eq!(
            node_box_height(&plain),
            NODE_BOX_PAD * 2.0 + NODE_TITLE_H + NODE_BODY_H * 2.0
        );
    }

    #[test]
    fn body_lines_prefer_summary_then_description() {
        let with_summary = node("名称", Some("摘要内容"), "正文内容", &[]);
        assert_eq!(node_body_lines(&with_summary), vec!["摘要内容".to_string()]);
        let desc_only = node("名称", None, "正文内容", &[]);
        assert_eq!(node_body_lines(&desc_only), vec!["正文内容".to_string()]);
        let empty = node("名称", None, "", &[]);
        assert!(node_body_lines(&empty).is_empty());
    }

    #[test]
    fn tag_chips_aggregate_overflow() {
        let n = node("名称", None, "", &["一", "二", "三", "四", "五"]);
        let chips = node_tag_chips(&n);
        assert_eq!(chips.len(), NODE_TAG_MAX + 1);
        assert_eq!(chips.last().unwrap().0, "+2");
    }

    #[test]
    fn hover_lines_carry_id_and_body() {
        let n = node("项目上下文", Some("这是摘要"), "这是正文描述", &["标签A"]);
        let lines = node_hover_lines(&n);
        let joined = lines.join("\n");
        assert!(joined.contains("名称: 项目上下文"), "{joined}");
        assert!(joined.contains("类型: 知识节点"), "{joined}");
        assert!(joined.contains("标签: 标签A"), "{joined}");
        assert!(joined.contains("摘要: 这是摘要"), "{joined}");
        assert!(joined.contains("描述: 这是正文描述"), "{joined}");
        // ID 只出现在 hover 详情里（画布卡片不再直出）
        assert!(joined.contains("ID: 01J8ZKQ7X4M2N5P6R8T0VWXYZ"), "{joined}");
        assert_eq!(lines.last().unwrap().split(": ").next().unwrap(), "ID");
    }
}
