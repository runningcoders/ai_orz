use dioxus::prelude::*;
use std::collections::HashMap;

use super::canvas_scene::arrow_tip_point;

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
    /// 方向性（方案 a′）：`"directed"` = 有向边，target 端绘制箭头；
    /// 其余（`"undirected"`）= 无向纯线。契约值由服务端解析带出，前端零词表映射纯透传。
    pub direction: String,
    /// 曲线形态（`None` = 直线，等价 `EdgeCurve::Straight`）—— 公共引擎能力：
    /// 任何 SVG 图都能按数据指定曲率，存量调用点补 `None` 即行为零变化。
    pub curve: Option<EdgeCurve>,
    /// 显式虚线样式（`None` = 按 `label` 的关系词表派生，存量图行为不变）
    pub dash: Option<String>,
    /// 显式线宽（`None` = 按 `weight` 派生；用于「按数据加粗」这类语义，
    /// 避免借用 weight 污染 hover 里的强度读数）
    pub width: Option<f64>,
}

/// 边曲线形态（公共引擎能力：直线 / 二次贝塞尔弧线 / 自环）
///
/// 为什么把曲率放在数据上而不是渲染时算：同一份图数据会被缩略图、放大弹窗、
/// 导出等多个画布尺寸复用，曲率若依赖画布尺寸就会各画一套；放到 `GraphEdge`
/// 上则一次算好、处处一致。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EdgeCurve {
    /// 直线（默认）
    Straight,
    /// 二次贝塞尔弧线：值为「曲线中点相对两端连线的法向偏移量（px，带符号）」。
    /// 符号决定绕行侧（`+` 走 left-hand normal 一侧），双向边用相反符号分居两侧。
    Arc(f64),
    /// 自环：从源节点右侧绕出的三次贝塞尔（半径由节点半宽决定，见 `edge_geometry`）
    Loop,
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
#[allow(clippy::too_many_arguments)]
fn build_hover_card(
    target: &HoverTarget,
    nodes: &[GraphNode],
    edges: &[GraphEdge],
    positions: &HashMap<String, (f64, f64)>,
    // 边的曲线中点（弧线/自环的 hover 锚点必须落在曲线上，不能取两端中点）
    edge_mids: &HashMap<(String, String), (f64, f64)>,
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
            // 弧线 / 自环的锚点取曲线中点；直线回退两端中点（与旧行为一致）
            let (ax, ay) = edge_mids
                .get(&(source.clone(), target_id.clone()))
                .copied()
                .unwrap_or(((fx + gx) / 2.0, (fy + gy) / 2.0));
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
                anchor_x: ax * scale + pan_x,
                anchor_y: ay * scale + pan_y,
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
    /// 全局点线视图模式：圆点 + 连接度映射半径（对齐 Canvas 全局模式；默认关，卡片态不受影响）
    #[props(default = false)]
    pub global_mode: bool,
    /// 节点连接度表（全局模式半径映射依据：id → degree，R4 in+out 仅活跃边）
    #[props(default)]
    pub node_degrees: Option<std::collections::HashMap<String, usize>>,
    /// 初始自适应缩放（默认关，存量调用点行为零变化）
    ///
    /// SVG 版自身没有自适应：节点坐标由布局按画布尺寸算，画布换尺寸/节点尺寸不定时
    /// 内容会跑出视野。打开后按节点包围盒算一次 fit 作初始视图，并作为「重置视图」目标态。
    #[props(default = false)]
    pub auto_fit: bool,
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
        // 思考路径图：节点是图引擎的执行单元（cyan-500，与记忆/任务色板可区分）
        "think_step" => "#06b6d4",
        _ => NEUTRAL_NODE_FILL,
    }
}

/// 点线图形态判定（SSOT）：全局点线视图与任务依赖图 mini 预览小框共用
/// 「圆点 + 连线」分支，同时抑制边标签 overlay——两处必须同开同关，
/// 否则会出现圆点节点配矩形边标签的混合形态。
pub fn dot_graph_mode(global_mode: bool, mini: bool) -> bool {
    global_mode || mini
}

/// 全局点线视图：连接度 → 节点半径（Canvas / SVG 双渲染器 SSOT）。
///
/// 底 7px、度数线性增长（每度 +1.1px，19 度封顶 ≈ 27.9px，28px 系防御性上限）。
/// 此前公式内联在 `graph_canvas.rs`，SVG 对齐时若各抄一份必然漂移，故上提至此。
pub fn global_node_radius(degree: usize) -> f64 {
    (7.0 + degree.min(19) as f64 * 1.1).min(28.0)
}

/// 全局点线视图：半径 → 名称限绘（Canvas / SVG 双渲染器 SSOT，对齐
/// `canvas_scene.rs` `draw_circle_node` 的免绘名守卫与截断宽度）。
///
/// 半径 < 14 时不绘名（画不下，返回空串由调用方跳过渲染）；
/// 否则按「半径 × 1.5」估可容宽度截断，clamp 在 3~12 字符。
pub fn global_node_label(label: &str, radius: f64) -> String {
    if radius < 14.0 {
        String::new()
    } else {
        label
            .chars()
            .take(((radius * 1.5) as usize).clamp(3, 12))
            .collect()
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

/// 计算两点间距离（直线段长度）
fn edge_length(sx: f64, sy: f64, tx: f64, ty: f64) -> f64 {
    ((tx - sx).powi(2) + (ty - sy).powi(2)).sqrt()
}

fn calculate_edge_angle(sx: f64, sy: f64, tx: f64, ty: f64) -> f64 {
    let dx = tx - sx;
    let dy = ty - sy;
    dy.atan2(dx) * 180.0 / std::f64::consts::PI
}

/// 二次贝塞尔控制点
///
/// 取两端中点为基准，沿连线法线偏移 `bulge * 2`：二次贝塞尔在 `t=0.5` 处的点
/// 是 `(P0 + 2·P1 + P2) / 4`，故控制点偏移量恰为中点偏移量的两倍——
/// 这样 `EdgeCurve::Arc(d)` 里的 `d` 就是**肉眼看到的弧高**，不是控制点距离。
pub fn quadratic_control(sx: f64, sy: f64, tx: f64, ty: f64, bulge: f64) -> (f64, f64) {
    let mx = (sx + tx) / 2.0;
    let my = (sy + ty) / 2.0;
    let dx = tx - sx;
    let dy = ty - sy;
    let len = (dx * dx + dy * dy).sqrt();
    if len <= f64::EPSILON {
        // 退化（两端重合）：无方向可言，控制点落中点即可（不 panic、不除零）
        return (mx, my);
    }
    // left-hand normal
    let nx = -dy / len;
    let ny = dx / len;
    (mx + nx * bulge * 2.0, my + ny * bulge * 2.0)
}

/// 二次贝塞尔取点
fn quadratic_at(p0: (f64, f64), p1: (f64, f64), p2: (f64, f64), t: f64) -> (f64, f64) {
    let mt = 1.0 - t;
    let a = mt * mt;
    let b = 2.0 * mt * t;
    let c = t * t;
    (
        a * p0.0 + b * p1.0 + c * p2.0,
        a * p0.1 + b * p1.1 + c * p2.1,
    )
}

/// 三次贝塞尔取点
fn cubic_at(p0: (f64, f64), p1: (f64, f64), p2: (f64, f64), p3: (f64, f64), t: f64) -> (f64, f64) {
    let mt = 1.0 - t;
    let a = mt * mt * mt;
    let b = 3.0 * mt * mt * t;
    let c = 3.0 * mt * t * t;
    let d = t * t * t;
    (
        a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
        a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
    )
}

/// 折线近似长度（5 点采样：二次贝塞尔 3 段折线近似误差 < 2%，足够流光动画用）
fn polyline_length(points: &[(f64, f64)]) -> f64 {
    points
        .windows(2)
        .map(|w| edge_length(w[0].0, w[0].1, w[1].0, w[1].1))
        .sum()
}

/// 曲线箭头终点：沿「控制点 → 终点」切线回退到目标半径外
///
/// ⚠️ 这是弧线改造最容易踩的坑：原来的 `arrow_tip_point` 只按「起点 → 终点」直线
/// 向量回退，弧线上箭头会斜着插进节点（视觉上箭头方向与曲线末端切线不一致）。
/// 曲线的末端切线方向是 `P2 - P1`（二次）或 `P3 - P2`（三次），所以回退基准
/// 必须换成「最后一个控制点 → 终点」。
pub fn curve_arrow_tip_point(
    control: (f64, f64),
    target: (f64, f64),
    target_radius: f64,
) -> (f64, f64) {
    arrow_tip_point(control, target, target_radius)
}

/// 一次算全的边几何（路径 / 箭头终点 / 中点 / 中点切线角 / 路径长度）
///
/// 渲染的四处（视觉 path、透明命中 path、箭头终点、标签位置与旋转）必须取自
/// **同一份几何**——分开各算一次，改曲率时必然漏改一处。
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeGeometry {
    /// 视觉路径（有向边终点已回退到目标半径外）
    pub d: String,
    /// 完整路径（到目标圆心；透明命中层用，命中区比视觉略长一点更宽松）
    pub d_full: String,
    /// 箭头终点（有向边返回回退后的点；无向边返回目标圆心）
    pub tip: (f64, f64),
    /// 曲线中点（hover 卡锚点 / 标签位置）
    pub mid: (f64, f64),
    /// 中点切线角度（度；标签沿曲线方向旋转）
    pub mid_angle: f64,
    /// 路径长度（px；供 `--len` 流光动画）
    pub length: f64,
}

/// 计算一条边的几何（直线 / 弧线 / 自环统一出口）
///
/// - `target_radius`：目标节点半径（箭头回退距离，口径同 Canvas 版 `arrow_tip_point`）
/// - `loop_radius`：自环绕行半径（调用方按节点半宽 + 8 传入）
pub fn edge_geometry(
    edge: &GraphEdge,
    source: (f64, f64),
    target: (f64, f64),
    target_radius: f64,
    loop_radius: f64,
) -> EdgeGeometry {
    let (sx, sy) = source;
    let (tx, ty) = target;
    let directed = edge.direction == "directed";
    match edge.curve.unwrap_or(EdgeCurve::Straight) {
        EdgeCurve::Straight => {
            let tip = if directed {
                arrow_tip_point(source, target, target_radius)
            } else {
                target
            };
            EdgeGeometry {
                d: format!("M {sx} {sy} L {} {}", tip.0, tip.1),
                d_full: format!("M {sx} {sy} L {tx} {ty}"),
                tip,
                mid: ((sx + tx) / 2.0, (sy + ty) / 2.0),
                mid_angle: calculate_edge_angle(sx, sy, tx, ty),
                length: edge_length(sx, sy, tx, ty),
            }
        }
        EdgeCurve::Arc(bulge) => {
            let p1 = quadratic_control(sx, sy, tx, ty, bulge);
            let tip = if directed {
                curve_arrow_tip_point(p1, target, target_radius)
            } else {
                target
            };
            let mid = quadratic_at(source, p1, target, 0.5);
            let prev = quadratic_at(source, p1, target, 0.45);
            let next = quadratic_at(source, p1, target, 0.55);
            let samples = [
                source,
                quadratic_at(source, p1, target, 0.25),
                mid,
                quadratic_at(source, p1, target, 0.75),
                target,
            ];
            EdgeGeometry {
                d: format!("M {sx} {sy} Q {} {} {} {}", p1.0, p1.1, tip.0, tip.1),
                d_full: format!("M {sx} {sy} Q {} {} {tx} {ty}", p1.0, p1.1),
                tip,
                mid,
                mid_angle: calculate_edge_angle(prev.0, prev.1, next.0, next.1),
                length: polyline_length(&samples),
            }
        }
        EdgeCurve::Loop => {
            // 自环：从卡片右缘绕出再绕回（起点/终点都在卡片右侧边缘，绕行半径 = 半宽 + 8）
            let half_w = (loop_radius - 8.0).max(12.0);
            let start = (sx + half_w, sy - 8.0);
            let end = (sx + half_w, sy + 8.0);
            let c1 = (sx + half_w + 34.0, sy - 22.0);
            let c2 = (sx + half_w + 34.0, sy + 22.0);
            // 自环回到自身卡片，回退量只留箭头间隙（半径传 0）
            let tip = if directed {
                curve_arrow_tip_point(c2, end, 0.0)
            } else {
                end
            };
            let mid = cubic_at(start, c1, c2, end, 0.5);
            let prev = cubic_at(start, c1, c2, end, 0.45);
            let next = cubic_at(start, c1, c2, end, 0.55);
            let samples = [
                start,
                cubic_at(start, c1, c2, end, 0.25),
                mid,
                cubic_at(start, c1, c2, end, 0.75),
                end,
            ];
            EdgeGeometry {
                d: format!(
                    "M {} {} C {} {}, {} {}, {} {}",
                    start.0, start.1, c1.0, c1.1, c2.0, c2.1, tip.0, tip.1
                ),
                d_full: format!(
                    "M {} {} C {} {}, {} {}, {} {}",
                    start.0, start.1, c1.0, c1.1, c2.0, c2.1, end.0, end.1
                ),
                tip,
                mid,
                mid_angle: calculate_edge_angle(prev.0, prev.1, next.0, next.1),
                length: polyline_length(&samples),
            }
        }
    }
}

/// 初始自适应视图（`auto_fit` 打开时用）
///
/// SVG 版本身没有自适应：节点坐标由布局函数按画布尺寸算，画布一换尺寸内容就可能
/// 出界。这里按节点卡片包围盒算一次 `(tx, ty, scale)`，使内容居中并完整可见。
/// 参照 Canvas 版 `fit_viewport` 的口径（含 padding、scale 上限 1.0 不放大）。
pub fn compute_fit_view(nodes: &[GraphNode], width: f64, height: f64) -> (f64, f64, f64) {
    if nodes.is_empty() || width <= 0.0 || height <= 0.0 {
        return (0.0, 0.0, 1.0);
    }
    let pad = 18.0;
    let (mut min_x, mut min_y) = (f64::MAX, f64::MAX);
    let (mut max_x, mut max_y) = (f64::MIN, f64::MIN);
    for n in nodes {
        let hw = node_box_width(n) / 2.0;
        let hh = node_box_height(n) / 2.0;
        min_x = min_x.min(n.x - hw);
        max_x = max_x.max(n.x + hw);
        min_y = min_y.min(n.y - hh);
        max_y = max_y.max(n.y + hh);
    }
    let content_w = (max_x - min_x).max(1.0);
    let content_h = (max_y - min_y).max(1.0);
    let scale = ((width - 2.0 * pad) / content_w)
        .min((height - 2.0 * pad) / content_h)
        .clamp(0.2, 1.0);
    let tx = (width - content_w * scale) / 2.0 - min_x * scale;
    let ty = (height - content_h * scale) / 2.0 - min_y * scale;
    (tx, ty, scale)
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

    let svg_width = props.svg_width.unwrap_or(800).max(160);
    let svg_height = props.svg_height.unwrap_or(600).max(120);
    // 初始视图：auto_fit 打开时按节点包围盒 fit 一次（参照 Canvas 版 fit_viewport），
    // 否则维持 (0,0,1) —— 存量调用点（知识图谱 / 任务依赖图 / 全局点线）视觉零回归
    let initial_view = if props.auto_fit {
        compute_fit_view(&props.nodes, svg_width as f64, svg_height as f64)
    } else {
        (0.0, 0.0, 1.0)
    };
    let mut view_transform = use_signal(|| initial_view);
    let mut is_panning = use_signal(|| false);
    let mut pan_start = use_signal(|| (0.0, 0.0));
    // hover 详情卡片目标（区别于点击选中；拖拽/平移中隐藏）
    let mut hovered = use_signal(|| None::<HoverTarget>);
    // 小地图模式：卡片只保留名称（正文 / 标签省略），缩略图预览用
    let mini = props.mini.unwrap_or(false);
    // 全局点线模式（对齐 Canvas 全局模式）：圆点分支 + 跳过边标签 overlay
    let global_mode = props.global_mode;
    let global_degrees = props.node_degrees.clone().unwrap_or_default();

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

    // 节点半宽/半高（自环绕行半径与命中层都用；点线态给统一小半径）
    let node_half_sizes: HashMap<String, (f64, f64)> = props
        .nodes
        .iter()
        .map(|n| {
            let size = if dot_graph_mode(global_mode, mini) {
                (10.0, 10.0)
            } else if mini {
                (
                    node_card::box_width(&n.label, 0) / 2.0,
                    node_card::box_height(0, false) / 2.0,
                )
            } else {
                (node_box_width(n) / 2.0, node_box_height(n) / 2.0)
            };
            (n.id.clone(), size)
        })
        .collect();

    // 自环绕行半径：节点半宽/半高取大者 + 8（弧线从卡片右缘绕出去，不压卡片本体）
    let loop_radius_of = |id: &str| -> f64 {
        node_half_sizes
            .get(id)
            .map(|(hw, hh)| hw.max(*hh) + 8.0)
            .unwrap_or(48.0)
    };

    // 边标签置顶数据 + 边的曲线中点缓存（hover 锚点）
    // - 标签必须渲染在节点之后：SVG 无 z-index、后画覆盖先画，否则被卡片遮挡；
    // - 中点取自与渲染同一份几何（弧线上标签/hover 落在曲线中点，不是两端中点）；
    // - 全局点线态不绘边标签（对齐 Canvas 全局模式：点线承载类型着色）。
    let mut edge_mids: HashMap<(String, String), (f64, f64)> = HashMap::new();
    let edge_label_overlays: Vec<(String, String, &'static str)> =
        if dot_graph_mode(global_mode, mini) {
            for (edge, (sx, sy), (tx, ty)) in valid_edges.iter() {
                let radius =
                    global_node_radius(global_degrees.get(&edge.target).copied().unwrap_or(0));
                let geo = edge_geometry(
                    edge,
                    (*sx, *sy),
                    (*tx, *ty),
                    radius,
                    loop_radius_of(&edge.source),
                );
                edge_mids
                    .entry((edge.source.clone(), edge.target.clone()))
                    .or_insert(geo.mid);
            }
            Vec::new()
        } else {
            valid_edges
                .iter()
                .filter_map(|(edge, (sx, sy), (tx, ty))| {
                    let radius =
                        global_node_radius(global_degrees.get(&edge.target).copied().unwrap_or(0));
                    let geo = edge_geometry(
                        edge,
                        (*sx, *sy),
                        (*tx, *ty),
                        radius,
                        loop_radius_of(&edge.source),
                    );
                    edge_mids
                        .entry((edge.source.clone(), edge.target.clone()))
                        .or_insert(geo.mid);
                    let label: String = edge.label.chars().take(10).collect();
                    (!label.is_empty()).then(|| {
                        (
                            label,
                            format!(
                                "translate({}, {}) rotate({})",
                                geo.mid.0, geo.mid.1, geo.mid_angle
                            ),
                            get_edge_color(&edge.label),
                        )
                    })
                })
                .collect()
        };

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
        // 上限 2.0 → 3.0：弧线/自环在放大后才看得清绕行细节；下限 0.2 兼容
        // auto_fit 产生的更小初始缩放（否则一滚轮就跳到 0.5，视图跳变）
        let new_scale = (scale * delta).clamp(0.2, 3.0);
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
    // 重置语义 = 恢复初始态（auto_fit 打开时回到 fit 视图，而不是硬编码 (0,0,1)）
    let reset_view = move |_| {
        view_transform.set(initial_view);
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
                &edge_mids,
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
                    // 虚线：边自带样式优先（思考路径图按 via 派生），否则按关系词表派生
                    let edge_dash = edge
                        .dash
                        .clone()
                        .unwrap_or_else(|| get_edge_dash(&edge.label).to_string());
                    let use_flow = edge_dash == "none";
                    // 线宽：边自带宽度优先（按数据加粗），否则按强度派生
                    let edge_width = edge
                        .width
                        .unwrap_or_else(|| edge_style::weight_style(edge.weight).0);
                    // 方向性边（方案 a′）：directed 挂箭头（target 端），其余纯线
                    let edge_marker = if edge.direction == "directed" {
                        "url(#arrowhead)"
                    } else {
                        "none"
                    };
                    // 统一几何：视觉路径 / 命中路径 / 箭头终点 / 路径长度同源
                    // （directed 边终点沿末端切线回退到 target 半径外，箭头不再被后绘节点遮挡）
                    let node_radius = global_node_radius(
                        global_degrees.get(&edge.target).copied().unwrap_or(0),
                    );
                    let geo = edge_geometry(
                        &edge,
                        (sx, sy),
                        (tx, ty),
                        node_radius,
                        loop_radius_of(&edge.source),
                    );
                    let len = geo.length;
                    let edge_class = if use_flow { "kg-edge-flow kg-edge-glow" } else { "kg-edge-glow" };
                    let edge_style = format!("--len: {len}px; color: {edge_color};");
                    let path_d = geo.d.clone();
                    let path_d_full = geo.d_full.clone();
                    // 事件闭包各自持有独立副本（move 捕获不能共享同一 String 字段）
                    let hover_enter = HoverTarget::Edge(edge.source.clone(), edge.target.clone());
                    let hover_leave = HoverTarget::Edge(edge.source.clone(), edge.target.clone());
                    rsx! {
                        path {
                            d: "{path_d}",
                            fill: "none",
                            stroke: "{edge_color}",
                            stroke_width: "{edge_width}",
                            stroke_dasharray: "{edge_dash}",
                            class: "{edge_class}",
                            style: "{edge_style}",
                            marker_end: "{edge_marker}",
                        }

                        // 透明命中层：放宽边的 hover 命中区（视觉样式不变）
                        // ⚠️ 必须同步换成 path：留在 line 上会让弧线边 hover 不到
                        path {
                            d: "{path_d_full}",
                            fill: "none",
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

                    // 任务依赖图 mini 预览小框：点线图分支——小圆点（状态语义色）+
                    // 连线呈现整体关系；不绘矩形卡 / 竖条 / 正文 / 标签，消除
                    // 300×200 小画布内矩形卡互相覆盖的问题。任务名等关键信息
                    // 全量承载于 hover 详情（node_hover_lines 通道）与点选详情；
                    // 全局点线分支与放大弹窗矩形卡路径零改动。
                    if dot_graph_mode(global_mode, mini) {
                        // mini 无 node_degrees 供给（deps 全在 global_mode 调用点）：
                        // degree=0 → 固定 7px 点半径，300×200 小画布下免绘名（半径 < 14）
                        let degree = global_degrees.get(&node.id).copied().unwrap_or(0);
                        let radius = global_node_radius(degree);
                        let label = global_node_label(&node.label, radius);
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
                                    // 与卡片分支一致：阻止冒泡，拖拽节点时不平移画布
                                    e.stop_propagation();
                                    handle_node_drag_start_with_event(e, node_id_drag.clone());
                                },

                                // 选中态：圆点外圈虚线扫描环（卡片扫描框的圆形对应）
                                if is_selected {
                                    circle {
                                        cx: "{nx}",
                                        cy: "{ny}",
                                        r: "{radius + 5.0}",
                                        fill: "none",
                                        stroke: "#f97316",
                                        stroke_width: "1.5",
                                        stroke_dasharray: "6 4",
                                    }
                                }

                                circle {
                                    cx: "{nx}",
                                    cy: "{ny}",
                                    r: "{radius}",
                                    fill: "{fill}",
                                    stroke: "{stroke}",
                                    stroke_width: "{stroke_width}",
                                }

                                // 名称限绘：半径足够才绘（global_node_label 空串则跳过）
                                if !label.is_empty() {
                                    text {
                                        x: "{nx}",
                                        y: "{ny + radius + 12.0}",
                                        text_anchor: "middle",
                                        font_size: "10",
                                        fill: "#f9fafb",
                                        font_weight: "500",
                                        "{label}"
                                    }
                                }
                            }
                        }
                    } else {

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

    #[test]
    fn global_node_radius_maps_degree_and_caps() {
        // 底值：0 度 → 7px
        assert_eq!(global_node_radius(0), 7.0);
        // 线性增长：19 度 → 7 + 19×1.1 = 27.9
        assert!((global_node_radius(19) - 27.9).abs() < 1e-9);
        // 封顶：内层 min(19) 先封，19 度以上恒 27.9（外层 28.0 系防御性上限）
        assert!((global_node_radius(28) - 27.9).abs() < 1e-9);
        assert!((global_node_radius(1000) - 27.9).abs() < 1e-9);
    }

    #[test]
    fn dot_graph_mode_covers_global_and_mini() {
        // 全局点线视图与任务依赖图 mini 预览小框共用点线形态：
        // 分支条件与边标签抑制必须同源（dot_graph_mode 单点判定）
        assert!(dot_graph_mode(true, false));
        assert!(dot_graph_mode(false, true));
        assert!(dot_graph_mode(true, true));
        // 卡片态（放大弹窗 / 局部关系图）不进点线分支
        assert!(!dot_graph_mode(false, false));
    }

    // ==================== 弧线引擎守卫 ====================
    // 目的：曲率/箭头切线/路径长度是同一份几何的三处消费点，任何一处算错
    // 都会表现为「箭头插进节点」「流光动画节奏不对」这类只在人眼可见的回归。

    fn edge_with(curve: Option<EdgeCurve>, direction: &str) -> GraphEdge {
        GraphEdge {
            source: "a".to_string(),
            target: "b".to_string(),
            label: "×2 · s1,2".to_string(),
            weight: None,
            direction: direction.to_string(),
            curve,
            dash: None,
            width: None,
        }
    }

    #[test]
    fn straight_geometry_keeps_legacy_tip_and_length() {
        let e = edge_with(None, "directed");
        let geo = edge_geometry(&e, (100.0, 100.0), (400.0, 100.0), 12.0, 48.0);
        // 直线：终点回退口径必须与旧实现逐位一致（arrow_tip_point）
        let legacy = arrow_tip_point((100.0, 100.0), (400.0, 100.0), 12.0);
        assert!((geo.tip.0 - legacy.0).abs() < 1e-9 && (geo.tip.1 - legacy.1).abs() < 1e-9);
        assert!(geo.d.starts_with("M 100 100 L"), "{}", geo.d);
        assert!((geo.length - 300.0).abs() < 1e-9, "直线长度应为弦长");
        assert!((geo.mid.0 - 250.0).abs() < 1e-9 && (geo.mid.1 - 100.0).abs() < 1e-9);
    }

    #[test]
    fn arc_geometry_bulges_by_declared_amount_at_mid() {
        let e = edge_with(Some(EdgeCurve::Arc(20.0)), "undirected");
        let geo = edge_geometry(&e, (0.0, 0.0), (100.0, 0.0), 12.0, 48.0);
        assert!(geo.d.contains('Q'), "弧线必须走二次贝塞尔: {}", geo.d);
        // Arc(d) 的语义 = 曲线中点偏移量，中点 = (P0+2P1+P2)/4 恰为 d
        assert!((geo.mid.0 - 50.0).abs() < 1e-9, "{}", geo.mid.0);
        assert!(
            (geo.mid.1 - 20.0).abs() < 1e-9,
            "中点弧高应等于声明值: {}",
            geo.mid.1
        );
        // 曲线必然比弦长（流光动画按路径长度算，用弦长会越走越快）
        assert!(
            geo.length > 100.0 + 1e-6,
            "弧线长度应大于弦长: {}",
            geo.length
        );
    }

    #[test]
    fn arc_arrow_tip_follows_end_tangent_not_chord() {
        let e = edge_with(Some(EdgeCurve::Arc(30.0)), "directed");
        let geo = edge_geometry(&e, (0.0, 0.0), (100.0, 0.0), 12.0, 48.0);
        let control = quadratic_control(0.0, 0.0, 100.0, 0.0, 30.0);
        // ① 反推回去：tip 必须落在「控制点 → 终点」射线上（弧线末端切线方向）
        let dx = 100.0 - control.0;
        let dy = 0.0 - control.1;
        let len = (dx * dx + dy * dy).sqrt();
        let expect = (
            100.0 - dx / len * (12.0 + 2.0),
            0.0 - dy / len * (12.0 + 2.0),
        );
        assert!(
            (geo.tip.0 - expect.0).abs() < 1e-9 && (geo.tip.1 - expect.1).abs() < 1e-9,
            "箭头终点必须沿末端切线回退: {:?} vs {:?}",
            geo.tip,
            expect
        );
        // ② 与「用直线向量回退」的旧口径必须有差别，否则说明切线逻辑没生效
        let legacy = arrow_tip_point((0.0, 0.0), (100.0, 0.0), 12.0);
        assert!(
            (geo.tip.1 - legacy.1).abs() > 1.0,
            "弧线箭头应明显偏离直线口径: {:?} vs {:?}",
            geo.tip,
            legacy
        );
    }

    #[test]
    fn loop_geometry_uses_cubic_and_bulges_right() {
        let e = edge_with(Some(EdgeCurve::Loop), "directed");
        let geo = edge_geometry(&e, (200.0, 200.0), (200.0, 200.0), 12.0, 48.0);
        assert!(geo.d.contains('C'), "自环必须走三次贝塞尔: {}", geo.d);
        assert!(geo.mid.0 > 200.0, "自环应绕到节点右侧: {}", geo.mid.0);
        assert!(geo.length > 40.0, "自环绕行长度不应退化: {}", geo.length);
    }

    #[test]
    fn mid_angle_stays_parallel_to_chord_for_arc_and_turns_for_loop() {
        // 二次贝塞尔的中点切线**恒平行于两端连线**（平行四边形性质）：
        // 弧顶标签仍沿「起点→终点」方向摆放，不会歪成别的角度。
        let e = edge_with(Some(EdgeCurve::Arc(40.0)), "undirected");
        let geo = edge_geometry(&e, (0.0, 0.0), (100.0, 100.0), 12.0, 48.0);
        assert!((geo.mid_angle - 45.0).abs() < 1e-6, "{}", geo.mid_angle);
        // 自环是三次贝塞尔：中点切线由控制点决定，在右侧绕行段近垂直
        let loop_e = edge_with(Some(EdgeCurve::Loop), "undirected");
        let loop_geo = edge_geometry(&loop_e, (200.0, 200.0), (200.0, 200.0), 12.0, 48.0);
        assert!(
            loop_geo.mid_angle.abs() > 45.0,
            "自环中点切线应近垂直: {}",
            loop_geo.mid_angle
        );
    }

    #[test]
    fn fit_view_keeps_all_nodes_inside_canvas() {
        let nodes = vec![
            GraphNode {
                id: "a".to_string(),
                label: "A".to_string(),
                x: 0.0,
                y: 0.0,
                ..Default::default()
            },
            GraphNode {
                id: "b".to_string(),
                label: "B".to_string(),
                x: 1000.0,
                y: 800.0,
                ..Default::default()
            },
        ];
        let (tx, ty, scale) = compute_fit_view(&nodes, 300.0, 200.0);
        assert!(scale > 0.0 && scale <= 1.0, "fit 只缩小不放大: {scale}");
        for n in &nodes {
            let hw = node_box_width(n) / 2.0;
            let hh = node_box_height(n) / 2.0;
            let left = (n.x - hw) * scale + tx;
            let right = (n.x + hw) * scale + tx;
            let top = (n.y - hh) * scale + ty;
            let bottom = (n.y + hh) * scale + ty;
            assert!(
                left >= -0.01 && right <= 300.01,
                "节点 {0} 横向出界: {left}~{right}",
                n.id
            );
            assert!(
                top >= -0.01 && bottom <= 200.01,
                "节点 {0} 纵向出界: {top}~{bottom}",
                n.id
            );
        }
    }

    #[test]
    fn fit_view_on_empty_or_degenerate_input_is_identity() {
        assert_eq!(compute_fit_view(&[], 300.0, 200.0), (0.0, 0.0, 1.0));
        assert_eq!(compute_fit_view(&[], 0.0, 0.0), (0.0, 0.0, 1.0));
    }

    #[test]
    fn global_node_label_truncates_by_radius() {
        // 半径不足 14：免绘名（空串）
        assert_eq!(global_node_label("任意名称", 13.9), "");
        // 半径恰 14：14×1.5=21 → clamp 后 12 字符上限
        let long = "一个相当长的知识节点名称超出了允许宽度上限";
        assert_eq!(global_node_label(long, 14.0).chars().count(), 12);
        assert_eq!(global_node_label(long, 28.0).chars().count(), 12);
        // 短名称完整保留
        assert_eq!(global_node_label("短名", 28.0), "短名");
    }
}
