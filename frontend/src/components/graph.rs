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
    /// 关系强度（0.0~1.0）；`None` = 未标注（渲染基准线宽）
    ///
    /// 与 Canvas 版共用 `edge_style` 的映射：两处各写一份系数，同一个强度
    /// 在两个视图里会粗细不同。
    pub weight: Option<f32>,
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
                // 矩形卡片：锚点偏移用半宽（宽度按内容收窄，不能再用常量）
                offset: node_box_width(node) / 2.0 * scale + 12.0,
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
                lines: {
                    let mut lines = vec![
                        format!("关系: {rel}"),
                        format!("端点: {} → {}", from.label, to.label),
                    ];
                    // 强度只在标注过时显示（未标注整行不渲染，避免被读成 0%）
                    if let Some(label) = edge_style::weight_label(edge.weight) {
                        lines.insert(1, label);
                    }
                    lines
                },
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
    /// SVG 画布宽度（默认 800）：缩略图 / 放大弹窗等不同容器按需传入
    #[props(default = None)]
    pub svg_width: Option<u32>,
    /// SVG 画布高度（默认 600）
    #[props(default = None)]
    pub svg_height: Option<u32>,
    /// 小地图模式：卡片只保留名称一行（正文 / 标签省略），缩略图预览用
    #[props(default = None)]
    pub mini: Option<bool>,
    on_node_click: EventHandler<String>,
}

/// 词表外 / 未知节点的填充色（中性灰）。
///
/// ⚠️ 这是**纯视觉降级**：类型原文照旧经 `node_card::type_label` 展示，只是没有语义色可给。
/// 新增 `MemoryType` 取值时必须同步补映射 —— 守卫测试
/// `memory_type_values_all_have_node_colors` 会在漏配时直接失败。
const NEUTRAL_NODE_FILL: &str = "#6b7280";

/// 任务状态（`TaskListItem.status`）→ 图节点类型 token
///
/// 任务依赖图复用本引擎的颜色 / hover 体系：node_type 用 `task_<状态>` 语义化编码，
/// 颜色走 `get_node_fill`、hover 类型行走 `type_label`（显示"任务 · 进行中"）。
/// token → 色 / 标签两处映射漏配由守卫测试
/// `task_status_node_types_all_have_semantic_colors` 兜底。
pub fn task_status_node_type(status: i32) -> &'static str {
    match status {
        // TaskStatus：Cancelled=0 / Pending=1 / InProgress=2 / Completed=3 / Archived=4
        0 => "task_cancelled",
        1 => "task_pending",
        2 => "task_in_progress",
        3 => "task_completed",
        4 => "task_archived",
        _ => "task",
    }
}

/// 节点填充颜色（key = `MemoryResult.memory_type` 的 snake_case 取值，或
/// `task_status_node_type` 的任务状态 token）
pub fn get_node_fill(node_type: &str) -> &'static str {
    match node_type {
        "knowledge_node" => "#3b82f6",
        "short_term" => "#10b981",
        "trace" => "#f59e0b",
        "relation" => "#8b5cf6",
        // 任务依赖图：色板与 utils::status 的 task_status_badge 同语义
        "task_cancelled" => "#ef4444",
        "task_pending" => "#3b82f6",
        "task_in_progress" => "#f59e0b",
        "task_completed" => "#10b981",
        // 归档用 slate-400 与兜底灰（#6b7280）区分，守卫测试才能兜住漏配
        "task_archived" => "#94a3b8",
        _ => NEUTRAL_NODE_FILL,
    }
}

// 卡片几何 / 文案 / 配色的实现在 `components::node_card`（Canvas 与 SVG 共用一份；
// 各画一套必然漂移：改了宽度忘改折行宽度 = 文字溢出卡片）。此处重导出保持调用点不变。
use crate::components::edge_style;
use crate::components::node_card;

pub use crate::components::node_card::type_label;

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

// 卡片几何常量与折行函数由上面的 `node_card` 提供，此处重导出保持调用点不变。
// （宽度不再有常量版本：走 `node_box_width`，纯名称卡片会被收窄。）
pub use crate::components::node_card::{
    HOVER_FONT_PX, HOVER_TEXT_W, NODE_ACCENT_W, NODE_BODY_H, NODE_BODY_PX, NODE_BOX_PAD,
    NODE_BOX_R, NODE_TAG_GAP, NODE_TAG_H, NODE_TITLE_H, NODE_TITLE_PX, wrap_text,
};

// 以下皆是 `GraphNode` → `node_card` 的薄包装：几何与文案的实现在
// `components::node_card`（Canvas 与 SVG 共用一份），这里只做「按 GraphNode
// 取字段」的适配。两处各写一套必然漂移——卡片高度对不上，画出来一张高一张矮。

/// 卡片高度：按实际正文行数（知识节点 = 高卡片，纯名称节点 = 矮卡片）
pub fn node_box_height(node: &GraphNode) -> f64 {
    node_card::box_height(node_body_lines(node).len(), !node.tags.is_empty())
}

/// 卡片宽度：按标题与正文的实际内容收窄（纯名称节点不占满 168px）
pub fn node_box_width(node: &GraphNode) -> f64 {
    node_card::box_width(&node.label, node_body_lines(node).len())
}

/// 卡片标题（展示名单行，超出省略）
pub fn node_title(node: &GraphNode) -> String {
    node_card::title(&node.label)
}

/// 卡片正文行：摘要优先，回退正文描述
pub fn node_body_lines(node: &GraphNode) -> Vec<String> {
    node_card::body_lines(node.summary.as_deref(), &node.description)
}

/// 卡片内标签胶囊（限个数也限宽度：行宽 = 卡宽 − 竖条 − 左右内边距）
pub fn node_tag_chips(node: &GraphNode) -> Vec<(String, &'static str)> {
    let row_width = node_box_width(node) - NODE_ACCENT_W - NODE_BOX_PAD * 2.0;
    node_card::tag_chips(&node.tags, row_width)
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

/// 词表外关系的边颜色（中性灰）。
///
/// ⚠️ 关系**原文照旧展示**（标签来自 `KnowledgeRelationType::zh_label_from_display`），
/// 这里只是没有语义色可给 —— Agent 自由标注（如「实现」）走这条兜底，**不丢信息**。
/// 新增规范关系词时必须同步补映射 —— 守卫测试
/// `vocabulary_relation_labels_all_have_semantic_colors` 会在漏配时直接失败。
const NEUTRAL_EDGE_COLOR: &str = "#9ca3af";

/// 边颜色（按关系中文标签语义分组着色）
///
/// ⚠️ key 是**展示标签**（`zh_label_from_display` 的输出，如"导致"），不是落库原文
/// （`causes`）—— 改词表映射时必须回来核对这张表，否则语义色会静默全部落到兜底灰。
pub fn get_edge_color(relation_type: &str) -> &'static str {
    match relation_type {
        "属于" => "#ef4444",
        "包含" | "实例" | "分类" | "属性" | "取值" => "#10b981",
        "相关" | "相似" | "引用" => "#3b82f6",
        "依赖" | "被依赖" | "前置" | "后续" => "#8b5cf6",
        "导致" | "源于" | "相反" => "#ec4899",
        "关联" | "派生" => "#f59e0b",
        _ => NEUTRAL_EDGE_COLOR,
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
    // 标签背景自带白底，直接压在边中点即可（此前的 -8 偏移会让标签漂离中点）
    format!("translate({}, {}) rotate({})", mid_x, mid_y, angle)
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

    let svg_width = props.svg_width.unwrap_or(800).max(160);
    let svg_height = props.svg_height.unwrap_or(600).max(120);
    // 小地图模式：卡片只保留名称（正文 / 标签省略），缩略图预览用
    let mini = props.mini.unwrap_or(false);

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

    // 边标签置顶数据：SVG 后画覆盖先画，标签组必须渲染在节点之后才不被卡片
    // 遮挡；这里提前算好文本 / 变换 / 颜色，渲染段只做纯输出
    let edge_label_overlays: Vec<(String, String, &'static str)> = valid_edges
        .iter()
        .filter_map(|(edge, (sx, sy), (tx, ty))| {
            let label: String = edge.label.chars().take(10).collect();
            (!label.is_empty()).then(|| {
                (
                    label,
                    get_label_transform(*sx, *sy, *tx, *ty),
                    get_edge_color(&edge.label),
                )
            })
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
        // 缩放只认「Ctrl/⌘ + 滚轮」（与 Canvas 渲染器行为一致）：
        // 裸滚轮极易误触缩放，直接交还给页面滚动，这里不做任何处理
        let m = e.modifiers();
        if !m.ctrl() && !m.meta() {
            return;
        }
        // Ctrl/⌘+滚轮同时是浏览器整页缩放快捷键，必须拦截默认行为
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
                    // 线宽表达强度（颜色已被关系类型占用）；未标注走基准粗细
                    let edge_width = edge_style::weight_style(edge.weight).0;
                    let len = edge_length(sx, sy, tx, ty);
                    let edge_class = if use_flow { "kg-edge-flow kg-edge-glow" } else { "kg-edge-glow" };
                    let edge_style = format!("--len: {len}px; color: {edge_color};");
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
                            stroke_width: "{edge_width}",
                            stroke_dasharray: "{edge_dash}",
                            class: "{edge_class}",
                            style: "{edge_style}",
                            marker_end: "url(#arrowhead)",
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
                    // mini 小地图：几何按纯名称卡塌缩（一行标题），正文 / 标签全部省略；
                    // 几何与内容必须同步切换——box_height 内部独立重算正文行数，
                    // 只清空内容会让卡片仍按正文预留高度
                    let (box_w, box_h) = if mini {
                        (
                            node_card::box_width(&node.label, 0),
                            node_card::box_height(0, false),
                        )
                    } else {
                        (node_box_width(&node), node_box_height(&node))
                    };
                    let box_x = nx - box_w / 2.0;
                    let box_y = ny - box_h / 2.0;
                    // 卡片内文字起始 x（竖条 + 左内边距）
                    let text_x = box_x + NODE_ACCENT_W + NODE_BOX_PAD;
                    let title_text = node_title(&node);
                    let body_lines = if mini {
                        Vec::new()
                    } else {
                        node_body_lines(&node)
                    };
                    let title_baseline = box_y + NODE_BOX_PAD + 11.0;
                    let body_baseline0 = box_y + NODE_BOX_PAD + NODE_TITLE_H + NODE_BODY_H - 3.0;
                    // 标签胶囊：卡片底部一行（无标签时不渲染；mini 模式省略）
                    let tag_chips = if mini {
                        Vec::new()
                    } else {
                        node_tag_chips(&node)
                    };
                    // 标签行 y 按实际正文行数排（此前固定按两行预留，
                    // 无正文但有标签的矮卡片会把标签画到卡外）
                    let tag_row_y = box_y + NODE_BOX_PAD + NODE_TITLE_H + NODE_BODY_H * body_lines.len() as f64 + 3.0;
                    let tag_widths: Vec<(String, f64, &'static str)> = tag_chips
                        .iter()
                        .map(|(t, c)| (t.clone(), node_card::tag_chip_width(t), *c))
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
                                    width: "{box_w + 10.0}",
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
                                    width: "{box_w + 6.0}",
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
                                width: "{box_w}",
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
                                    tag_x += tw + NODE_TAG_GAP;
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

            // 边标签置顶层：SVG 无 z-index、后画覆盖先画，节点卡片不再遮挡关系名；
            // 宽度按字符数（此前用字节数，中文标签底框会偏宽）
            for (label, transform, edge_color) in edge_label_overlays.iter() {
                g {
                    transform: "{transform}",
                    style: "pointer-events: none;",
                    rect {
                        x: "-{label.chars().count() as f64 * 3.5}",
                        y: "-7",
                        width: "{label.chars().count() as f64 * 7.0 + 4.0}",
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
    use crate::components::node_card::{NODE_TAG_MAX, text_width};

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
    fn box_height_grows_with_tags() {
        let plain = node("名称", None, "", &[]);
        let tagged = node("名称", None, "", &["a"]);
        assert!(
            node_box_height(&tagged) > node_box_height(&plain),
            "有标签时卡片要更高"
        );
        // 无标签、无正文 → 只有标题行（高度按实际内容行数算，不再固定撑两行）
        assert_eq!(node_box_height(&plain), NODE_BOX_PAD * 2.0 + NODE_TITLE_H);
        assert_eq!(
            node_box_height(&tagged) - node_box_height(&plain),
            NODE_TAG_H + 3.0,
            "标签行只应多出标签高度 + 间距"
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

    // ==================== 类型/关系配色守卫 ====================
    // 这三条测试的目的：**加词忘改颜色表时直接失败**。
    // 否则新增的规范词/记忆类型会静默落到中性灰 —— 不报错、不 panic，只是界面悄悄失色。

    /// 关系词表里每个规范词都必须有确定的语义色（`custom` 本身是兜底项，跳过）。
    #[test]
    fn vocabulary_relation_labels_all_have_semantic_colors() {
        for key in common::enums::KnowledgeRelationType::VOCABULARY {
            if key == "custom" {
                continue;
            }
            let label = common::enums::KnowledgeRelationType::zh_label_from_display(key);
            assert_ne!(label, key, "规范词 {key} 应当映射成中文标签");
            assert_ne!(
                get_edge_color(label),
                NEUTRAL_EDGE_COLOR,
                "规范词 {key}（标签 {label}）在 get_edge_color 里缺少语义色"
            );
        }
    }

    /// 词表外的关系原文 → 中性色（Agent 自由标注必须有确定的视觉归宿，不 panic）。
    #[test]
    fn unknown_relation_label_falls_back_to_neutral_edge_color() {
        for raw in ["实现", "is_prerequisite_of", "父节点", ""] {
            let label = common::enums::KnowledgeRelationType::zh_label_from_display(raw);
            assert_eq!(
                get_edge_color(label),
                NEUTRAL_EDGE_COLOR,
                "`{raw}` 应走中性色兜底"
            );
        }
        // 关系类型缺失时的兜底标签「关联」有确定的语义色，不落中性 —— 否则存量空类型边会全灰
        assert_ne!(get_edge_color("关联"), NEUTRAL_EDGE_COLOR);
    }

    /// `MemoryType` 的每个具体取值都必须有节点填充色（`all` 是查询侧伪类型，不参与着色）。
    #[test]
    fn memory_type_values_all_have_node_colors() {
        for token in common::enums::MemoryType::ACCEPTED_VALUES.split(',') {
            let token = token.trim();
            if token == "all" {
                continue;
            }
            assert_ne!(
                get_node_fill(token),
                NEUTRAL_NODE_FILL,
                "MemoryType `{token}` 在 get_node_fill 里缺少配色"
            );
        }
    }

    /// 任务状态 0..=4 每个取值都必须命中专属 token 且有语义色
    /// （新增 `TaskStatus` 枚举项时若漏改 `task_status_node_type` / `get_node_fill` 直接失败）。
    #[test]
    fn task_status_node_types_all_have_semantic_colors() {
        for status in 0..=4i32 {
            let token = task_status_node_type(status);
            assert_ne!(token, "task", "任务状态 {status} 不应落兜底 token");
            assert_ne!(
                get_node_fill(token),
                NEUTRAL_NODE_FILL,
                "任务状态 {status}（token {token}）在 get_node_fill 里缺少语义色"
            );
            // hover 卡片标签不能直出英文 token
            assert_ne!(
                node_card::type_label(token),
                token,
                "任务状态 {status}（token {token}）在 type_label 里缺少中文标签"
            );
        }
        // 超范围状态必须落 "task" 兜底（不 panic）
        assert_eq!(task_status_node_type(99), "task");
    }
}
