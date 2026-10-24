use dioxus::prelude::*;
use std::collections::HashMap;

use super::canvas_scene::{ARROW_TIP_GAP, arrow_tip_point};

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
#[derive(Debug, Clone, PartialEq)]
pub enum EdgeCurve {
    /// 直线（默认）
    Straight,
    /// 二次贝塞尔弧线：值为「曲线中点相对两端连线的法向偏移量（px，带符号）」。
    /// 符号决定绕行侧（`+` 走 left-hand normal 一侧），双向边用相反符号分居两侧。
    Arc(f64),
    /// 自环：从源节点右侧绕出的三次贝塞尔（半径由节点半宽决定，见 `edge_geometry`）
    Loop,
    /// 折线（dagre 边路由 waypoints）：折线经过的点（画布坐标，**不含起终点**，
    /// 方向 source→target）。用于分层图（任务依赖图 / 思考轨迹图）的 dummy node
    /// 边路由——取代旧弧线避障启发式，边不穿卡由路由层面保证。
    Polyline(Vec<(f64, f64)>),
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
/// 思考路径图「行为」节点类型 token 的前缀（完整形态 `think_step:{slot}`）。
///
/// 为什么要带槽位：一张思考轨迹图里每个节点都是**一个不同的行为**（think /
/// dispatch / evaluate …），但它们的节点类型原本都是同一个 `think_step`，于是
/// `get_node_fill` 只能给出同一个颜色 —— AMan 第十轮反馈③「这三个节点都用的是
/// 同一种颜色呀，不同的行为是不是应该用不同的颜色」。槽位由
/// `utils::think_path_graph::assign_color_slots` 分配（稳定哈希 + 组内贪心去重），
/// 本文件只负责「槽位 → 颜色」的映射。
pub const THINK_STEP_COLOR_PREFIX: &str = "think_step:";

/// 思考路径图行为调色板（8 色，彼此可分辨且与任务状态色板不冲突）。
///
/// 槽位 = 索引；同图内由 `assign_color_slots` 保证不撞色，同一行为在任何 trace 里
/// 落在同一个起点槽位（改色板顺序会换色，但不会造成同图撞色）。
pub const THINK_STEP_COLORS: &[&str] = &[
    "#06b6d4", "#8b5cf6", "#f59e0b", "#10b981", "#3b82f6", "#ef4444", "#ec4899", "#84cc16",
];

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
        // 思考路径图（带槽位）：`think_step:{slot}` → 按行为取色，
        // 让「think / dispatch / evaluate」在同一张图里一眼可分（AMan 第十轮反馈③）
        t if t.starts_with(THINK_STEP_COLOR_PREFIX) => {
            let slot = t[THINK_STEP_COLOR_PREFIX.len()..]
                .parse::<usize>()
                .unwrap_or(0);
            THINK_STEP_COLORS[slot % THINK_STEP_COLORS.len()]
        }
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

/// 点线预览（`mini` 缩略图）的圆点半径：按**画布尺寸与节点数**自适应。
///
/// # 为什么不能再是常量 7px（AMan 第八轮反馈②「预览图里的圆点太小」）
///
/// 圆点是按用户坐标绘制的，`auto_fit` 之后再乘一次 `scale`：300×200 缩略图里
/// 7px 半径 × 0.2~0.4 的 fit 缩放 = 视觉半径 1.4~2.8px，肉眼几乎看不见。
/// 而「原来那个大小」是卡片态被 fit 缩小后的结果（一张 104~168px 宽的卡缩到
/// 0.4 仍有 40px 视觉宽度），点线态按常量画自然对不上。
///
/// # 取值口径（第十轮修正：系数 0.085 → 0.12，把「原来那个大小」还回去）
///
/// ```text
/// base = clamp(min(width, height) × 0.12, 10, 28)
/// damp = (8 / max(n, 8)) ^ 0.35
/// r    = clamp(base × damp, 10, 28)
/// ```
///
/// - 300×200：3 / 4 / 8 节点**都是 24px**；12 节点 ≈ 20.9px；16 节点 ≈ 19.2px；
/// - 920×620：短边 620 → base 顶到 28px 上限（放大弹窗走卡片态，不消费本函数）。
///
/// # 为什么第九轮的 0.085 是错的（AMan 第十轮反馈①「跟上次改之前没有任何变化」）
///
/// 0.085 × 200 = **17px**，而更早那版按 `面积 / n` 均分在 3~4 节点时是 **22.9~24px**
/// —— 也就是说第九轮为了「两个缩略图点径统一」，把点从 24px 压到了 17px，
/// 统一是统一了，但把用户当时说「蛮合适」的尺寸改小了一整档，3 节点场景下
/// 观感几乎等于没动。此后基准取 0.12（= 24px @ 200 短边），统一（3/4/8 节点同值）
/// 又回到「原来那个大小」；节点数阻尼只在 8 个以上才收一点，避免 20+ 节点糊成一团。
/// **2026-10-24 再下调为 0.08（= 16px @ 200 短边，直径 32px）**：dagre 布局放开 scale
/// 铺满画布后，用户反馈点仍「非常大」，要求直径至少缩短 1/3（48px → 32px）。
///
/// 布局侧（`utils::dagre_layout` 点线态取 `dot_node_radius × 2` 作节点尺寸）
/// 用同一函数推节点占位与间距，保证「画多大点」与「留多大位」永远一致，不会点压点。
pub fn dot_node_radius(width: f64, height: f64, node_count: usize) -> f64 {
    let short = width.max(1.0).min(height.max(1.0));
    // 基准 0.08（= 16px @ 200 短边 → 直径 32px，比第十轮的 24px / 直径 48px **缩短
    // 1/3**）、上限 20。用户明确要求「直径至少缩短 1/3」：dagre 放开 scale 铺满画布
    // 后点距已经拉开，点再小也不影响辨识，反而让连线更清晰（仍随画布放大、
    // 节点数轻阻尼，3/4/8 节点仍同值）。
    let base = (short * 0.08).clamp(10.0, 20.0);
    let n = node_count.max(1) as f64;
    let damp = (8.0 / n.max(8.0)).powf(0.35);
    (base * damp).clamp(10.0, 20.0)
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

/// 边标签字号（衬底度量 / 渲染同源，改这里要同步 rsx 里的 font_size）
const EDGE_LABEL_PX: f64 = 10.0;

/// 边标签衬底宽度：文本度量 + 左右各 3px padding（`edge_label_overlays` 置顶层用）
///
/// ⚠️ 必须走 `node_card::text_width`（CJK≈字号、ASCII≈0.55×字号）而不是「字符数 ×
/// 固定系数」：旧实现 `7.0×n + 4` 只覆盖 ASCII，中文标签每字 ≈10px，衬底左侧会
/// 超出白底 3px——用户看到的就是「文字缺一角」；且旧写法右缘比左缘多 4px，衬底
/// 整体右偏。与卡片折行共用一套估算，SVG / Canvas 口径一致。
pub fn edge_label_box_width(label: &str) -> f64 {
    node_card::text_width(label, EDGE_LABEL_PX) + 6.0
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

/// 边标签朝向归一：**文字永远正立**（AMan 第六轮反馈③）
///
/// 边的 `mid_angle` 是中段切线角，方向随 `source → target` 走：`A→B` 朝右（≈0°），
/// `B→A` 朝左（≈180°）。直接拿它 `rotate()` 就会出现「同一条线的两个方向，一个正着写、
/// 一个倒着写」—— 一张图里两种朝向，读起来别扭且和布局是否合适无关（任何布局下
/// 反向边的切线角都在 180° 附近）。
///
/// 这里把角度折进 `(-90°, 90°]`：倒置的标签翻转 180° 后仍与线同向（沿线的姿态不变），
/// 但阅读方向统一为正立。这是图可视化领域的通行做法（标签恒水平可读），不依赖布局算法。
pub fn upright_angle(angle: f64) -> f64 {
    let mut a = angle % 360.0;
    if a > 180.0 {
        a -= 360.0;
    }
    if a <= -180.0 {
        a += 360.0;
    }
    if a > 90.0 {
        a - 180.0
    } else if a <= -90.0 {
        // 含 -90 边界：折进 (-90, 90] 才是「正立」的闭区间口径
        a + 180.0
    } else {
        a
    }
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

/// 折线中点与中点切线角（沿**弧长**参数化，不是按下标取中间点）
///
/// 折线各段长度不均（dagre 的 dummy node 通道可长可短），下标中点会落在视觉中点
/// 之外；标签锚点 / 旋转角必须取弧长一半处，与弧线的 `quadratic_at(0.5)` 口径一致。
fn polyline_mid(points: &[(f64, f64)]) -> ((f64, f64), f64) {
    if points.len() < 2 {
        return (points[0], 0.0);
    }
    let total = polyline_length(points);
    let mut acc = 0.0;
    for w in points.windows(2) {
        let seg = edge_length(w[0].0, w[0].1, w[1].0, w[1].1);
        if seg > 0.0 && acc + seg >= total / 2.0 {
            let t = (total / 2.0 - acc) / seg;
            let mid = (
                w[0].0 + (w[1].0 - w[0].0) * t,
                w[0].1 + (w[1].1 - w[0].1) * t,
            );
            return (mid, calculate_edge_angle(w[0].0, w[0].1, w[1].0, w[1].1));
        }
        acc += seg;
    }
    (
        points[points.len() / 2],
        calculate_edge_angle(points[0].0, points[0].1, points[1].0, points[1].1),
    )
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

/// 矩形卡片态箭头落点：沿「入射方向 × 轴对齐矩形边界」求交，再外推 `ARROW_TIP_GAP`
///
/// # 为什么不能沿用圆口径（本项目三修③的根因）
///
/// `arrow_tip_point` 按「离目标中心 `radius + GAP`」回退，这在圆形节点上正确。但卡片
/// 路径的节点是矩形（半宽 52~84、半高 25~45），而边几何传入的 `target_radius` 在卡片态
/// 恒为 `global_node_radius(0) = 7.0`（无度数信息），于是箭头终点只离矩形中心 9px，
/// **落在矩形内部**，被后绘制的卡片完全盖住 —— 视觉上就是「有连线没箭头」。
///
/// 求交口径：从矩形中心 `to` 沿入射反方向走，先撞哪条边由 `hw / |dx|` 与 `hh / |dy|`
/// 的较小者决定（水平入射撞左右缘、垂直入射撞上下缘、斜向撞最近的那条），再沿同一
/// 射线外推 `ARROW_TIP_GAP`，箭头尖端就停在卡片外缘之外。
pub fn rect_tip_point(from: (f64, f64), to: (f64, f64), half: (f64, f64)) -> (f64, f64) {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    let len = (dx * dx + dy * dy).sqrt();
    if len <= f64::EPSILON {
        // 退化（起终点重合）：无入射方向可言，直接给中心（不 panic、不除零）
        return to;
    }
    let hw = half.0.max(1.0);
    let hh = half.1.max(1.0);
    // t 的量纲是「向量倍数」：交点 = to - d * t
    let tx = if dx.abs() > f64::EPSILON {
        hw / dx.abs()
    } else {
        f64::MAX
    };
    let ty = if dy.abs() > f64::EPSILON {
        hh / dy.abs()
    } else {
        f64::MAX
    };
    let t = tx.min(ty) + ARROW_TIP_GAP / len;
    (to.0 - dx * t, to.1 - dy * t)
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
/// - `target_half`：目标节点矩形半宽/半高（卡片态箭头落点；`None` = 点线态走圆口径）
/// - `source_half`：源节点矩形半宽/半高（**起点收缩**；`None` = 从卡片中心起笔）
/// - `loop_radius`：自环绕行半径（调用方按节点半宽 + 8 传入）
///
/// # 起点为什么也必须收缩（AMan 第五轮反馈①「放大后线穿过节点卡片」）
///
/// 三修只把**终点**推出了卡外，`d` 仍从 `source`（= 源卡片**中心**）起笔 —— 整条线
/// 斜穿源卡片本体。缩略图缩小后看不出来，放大到 920×620 就一眼可见。这里把起点也按
/// `rect_tip_point` 沿出射方向推到源矩形外缘（同样外推 `ARROW_TIP_GAP`）；弧线的出射
/// 方向取「源中心 → 控制点」，与终点「控制点 → 终点」的切线口径对称。
pub fn edge_geometry(
    edge: &GraphEdge,
    source: (f64, f64),
    target: (f64, f64),
    target_radius: f64,
    target_half: Option<(f64, f64)>,
    source_half: Option<(f64, f64)>,
    loop_radius: f64,
) -> EdgeGeometry {
    let (sx, sy) = source;
    let (tx, ty) = target;
    let directed = edge.direction == "directed";
    // 箭头落点：卡片态走矩形边界求交（否则箭头藏在后绘卡片下面），
    // 点线态 / 无卡片信息时退回与 Canvas 同源的圆口径。
    let tip_of = |from: (f64, f64), to: (f64, f64)| match target_half {
        Some(half) => rect_tip_point(from, to, half),
        None => curve_arrow_tip_point(from, to, target_radius),
    };
    // 起点收缩：把「源卡片中心」沿出射方向推到源矩形外缘。`rect_tip_point` 的语义是
    // 「从 to 沿反方向回退」，故交换参数（from = 出射指向的点、to = 源中心）即得交点。
    let start_of = |from: (f64, f64), towards: (f64, f64)| match source_half {
        Some(half) => {
            let dx = towards.0 - from.0;
            let dy = towards.1 - from.1;
            // 出射方向不可靠（指向点落在卡片**内部**）时不收缩，避免起点被推到卡片对侧。
            // ⚠️ 判据必须是「是否落在矩形内」，不能拿矩形**对角线**比距离——对角线
            // （≈92px）远大于半宽/半高，卡片下方几十 px 的路点会被误判成「在内部」，
            // 起点就留在卡片中心（线从卡心起笔）。
            let inside = dx.abs() <= half.0 && dy.abs() <= half.1;
            if inside {
                from
            } else {
                rect_tip_point(towards, from, half)
            }
        }
        None => from,
    };
    // as_ref 而非 unwrap_or：EdgeCurve 含 Vec（Polyline），整体不再 Copy；
    // 渲染热路径避免每帧 clone 折线点
    match edge.curve.as_ref().unwrap_or(&EdgeCurve::Straight) {
        EdgeCurve::Straight => {
            let start = start_of(source, target);
            let tip = if directed {
                tip_of(source, target)
            } else {
                target
            };
            EdgeGeometry {
                d: format!("M {} {} L {} {}", start.0, start.1, tip.0, tip.1),
                d_full: format!("M {} {} L {tx} {ty}", start.0, start.1),
                tip,
                mid: ((start.0 + tx) / 2.0, (start.1 + ty) / 2.0),
                mid_angle: calculate_edge_angle(start.0, start.1, tx, ty),
                length: edge_length(start.0, start.1, tx, ty),
            }
        }
        EdgeCurve::Arc(bulge) => {
            let bulge = *bulge;
            let p1 = quadratic_control(sx, sy, tx, ty, bulge);
            // 起点沿「源中心 → 控制点」出射方向收缩（与终点「控制点 → 终点」切线口径对称）
            let start = start_of(source, p1);
            let tip = if directed { tip_of(p1, target) } else { target };
            let mid = quadratic_at(start, p1, target, 0.5);
            let prev = quadratic_at(start, p1, target, 0.45);
            let next = quadratic_at(start, p1, target, 0.55);
            let samples = [
                start,
                quadratic_at(start, p1, target, 0.25),
                mid,
                quadratic_at(start, p1, target, 0.75),
                target,
            ];
            EdgeGeometry {
                d: format!(
                    "M {} {} Q {} {} {} {}",
                    start.0, start.1, p1.0, p1.1, tip.0, tip.1
                ),
                d_full: format!("M {} {} Q {} {} {tx} {ty}", start.0, start.1, p1.0, p1.1),
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
        EdgeCurve::Polyline(pts) => {
            if pts.is_empty() {
                // 防御性兜底（dagre_layout 保证 pts.len() >= 2 才插入）：退化为直线
                let start = start_of(source, target);
                let tip = if directed {
                    tip_of(source, target)
                } else {
                    target
                };
                return EdgeGeometry {
                    d: format!("M {} {} L {} {}", start.0, start.1, tip.0, tip.1),
                    d_full: format!("M {} {} L {tx} {ty}", start.0, start.1),
                    tip,
                    mid: ((start.0 + tx) / 2.0, (start.1 + ty) / 2.0),
                    mid_angle: calculate_edge_angle(start.0, start.1, tx, ty),
                    length: edge_length(start.0, start.1, tx, ty),
                };
            }
            // dagre 边路由：起点 = 源中心沿「源中心 → 首个 waypoint」收缩到源卡片外缘；
            // 终点 = 「末个 waypoint → 目标中心」切线与目标边界的交点（箭头落在 target 端）。
            let first = pts[0];
            let last = pts[pts.len() - 1];
            let start = start_of(source, first);
            let tip = if directed {
                tip_of(last, target)
            } else {
                target
            };
            let mut d = format!("M {} {}", start.0, start.1);
            for (x, y) in pts.iter() {
                d.push_str(&format!(" L {x} {y}"));
            }
            // ⚠️ 必须补到箭头落点：dagre 的边界求交点（离节点中心 = 节点半尺寸）在
            // `dagre_layout` 里被整体缩放过（scale > 1 时离卡片更远），只画到
            // pts 末点会在卡片前**裂开一截**——用户截图「线没指到卡片上」的根因。
            d.push_str(&format!(" L {} {}", tip.0, tip.1));
            let mut d_full = d.clone();
            d_full.push_str(&format!(" L {tx} {ty}"));
            // 采样序列：起点 → 各 waypoint → 箭头落点 → 目标中心（与 d_full 同源，mid/length 一致）
            let samples: Vec<(f64, f64)> = std::iter::once(start)
                .chain(pts.iter().copied())
                .chain(std::iter::once(tip))
                .chain(std::iter::once(target))
                .collect();
            let (mid, mid_angle) = polyline_mid(&samples);
            EdgeGeometry {
                d,
                d_full,
                tip,
                mid,
                mid_angle,
                length: polyline_length(&samples),
            }
        }
    }
}

/// 给「同一对节点之间的多条边」分档弧线，其余边保持直线（存量视觉零变化）。
///
/// # 为什么需要它
///
/// 知识图谱等**非分层**图里，同一对节点之间可能存在多条边（不同关系类型各一条、
/// 或双向各一条）。直线绘制时它们在几何上完全重合，视觉上只剩一条线，用户
/// 无法看出「这里其实有两条关系」。
///
/// # 为什么按「参考方向」换算符号，而不是直接给相反符号
///
/// [`quadratic_control`] 的法线是相对**有向线段**的 left-hand normal：`B→A`
/// 的法线自带一个负号。因此给 `A→B` 与 `B→A` 分配相反符号会「负负得正」，
/// 两条曲线落到同一侧、几何依旧重合（思考路径图侧踩过的同一个坑）。
/// 这里统一在**参考方向**（字典序小 → 大）坐标系里分配偏移量，再按每条边自身
/// 方向换算回 `bulge`，保证任意方向组合下曲线都分居参考线两侧、互不重合。
///
/// # 刻意不做的事
///
/// - 单条边**不弯**：非分层图里弧线不承担避障（没有层级可绕），全弯只会更乱；
///   只有「重合是确定问题」的多重边才复用弧线能力。
/// - 不改 `weight` / `dash` / `width`：只动 `curve`，其余视觉仍由既有映射决定。
/// - 不改自环：`EdgeCurve::Loop` 由调用方决定，本函数不覆盖。
///
/// # 参数
///
/// - `base`：相邻两档之间的法向偏移间距（px）。`n` 条边占据
///   `[-(n-1)/2, +(n-1)/2] × base` 的位置，`n = 2` 时即 `±base/2`。
pub fn assign_parallel_edge_curves(edges: &mut [GraphEdge], base: f64) {
    if edges.is_empty() || base <= 0.0 {
        return;
    }
    // 无序对 → 组内边下标（保持首次出现序：同一份数据每次渲染结果一致，不会跳）
    let mut groups: HashMap<(String, String), Vec<usize>> = HashMap::new();
    let mut order: Vec<(String, String)> = Vec::new();
    for (i, e) in edges.iter().enumerate() {
        // 自环由 `EdgeCurve::Loop` 承载，不参与分档
        if e.source == e.target {
            continue;
        }
        let key = if e.source <= e.target {
            (e.source.clone(), e.target.clone())
        } else {
            (e.target.clone(), e.source.clone())
        };
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(i);
    }
    for key in order {
        let idxs = &groups[&key];
        let n = idxs.len();
        if n < 2 {
            continue;
        }
        for (rank, &idx) in idxs.iter().enumerate() {
            // 参考方向坐标系里的偏移：n 条边对称铺在参考线两侧（中间那条直）
            let offset = base * (rank as f64 - (n as f64 - 1.0) / 2.0);
            let same_dir = edges[idx].source <= edges[idx].target;
            let bulge = if same_dir { offset } else { -offset };
            let edge = &mut edges[idx];
            if bulge.abs() < f64::EPSILON {
                edge.curve = None;
            } else {
                edge.curve = Some(EdgeCurve::Arc(bulge));
            }
        }
    }
}

/// 初始自适应视图（`auto_fit` 打开时用）——**半尺寸由调用方给**。
///
/// SVG 版本身没有自适应：节点坐标由布局函数按画布尺寸算，画布一换尺寸内容就可能
/// 出界。这里按节点包围盒算一次 `(tx, ty, scale)`，使内容居中并完整可见。
/// 参照 Canvas 版 `fit_viewport` 的口径（含 padding、scale 上限 1.0 不放大）。
///
/// # 为什么半尺寸要作为参数（不要在这里自己按卡片算）
///
/// 同一份坐标在两种形态下占据的视觉面积差一个量级：卡片态是一张 104~168px 宽的
/// 矩形卡，点线态是一个十几像素的圆点。若点线态仍按卡片宽高算包围盒，300×200
/// 缩略图会被判成「内容严重超出」→ `scale` 压到 0.2~0.4 → 圆点视觉半径只剩
/// 2~3px，与「点线图看不清」是同一个根因（AMan 第八轮反馈②）。故包围盒必须与
/// 渲染形态同源。
pub fn compute_fit_view_with<F>(
    nodes: &[GraphNode],
    width: f64,
    height: f64,
    half_of: F,
) -> (f64, f64, f64)
where
    F: Fn(&GraphNode) -> (f64, f64),
{
    if nodes.is_empty() || width <= 0.0 || height <= 0.0 {
        return (0.0, 0.0, 1.0);
    }
    let pad = 18.0;
    let (mut min_x, mut min_y) = (f64::MAX, f64::MAX);
    let (mut max_x, mut max_y) = (f64::MIN, f64::MIN);
    for n in nodes {
        let (hw, hh) = half_of(n);
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
    // 小地图模式：卡片只保留名称（正文 / 标签省略），缩略图预览用
    let mini = props.mini.unwrap_or(false);
    // 全局点线模式（对齐 Canvas 全局模式）：圆点分支 + 跳过边标签 overlay
    let global_mode = props.global_mode;
    let global_degrees = props.node_degrees.clone().unwrap_or_default();
    // 点线形态判定（单点 SSOT：边标签抑制 / 节点渲染 / 半尺寸 / fit 包围盒必须同源）
    let dot_mode = dot_graph_mode(global_mode, mini);
    // 缩略图圆点半径：按画布尺寸与节点数自适应（AMan 第八轮反馈②「圆点太小」）。
    // 常量 7px 在 300×200 小画布上还要再乘一次 auto_fit 的 scale（0.2~0.4），
    // 视觉半径只剩 2~3px；布局侧用同一函数推间距，保证点不压点。
    let preview_dot_radius =
        dot_node_radius(svg_width as f64, svg_height as f64, props.nodes.len());
    // 点线态半径 SSOT：全局视图按连接度（度数本身有语义），缩略图按画布自适应
    let node_radius_of = |id: &str| -> f64 {
        if global_mode {
            global_node_radius(global_degrees.get(id).copied().unwrap_or(0))
        } else {
            preview_dot_radius
        }
    };
    // 节点半宽/半高（自环绕行半径、命中层、fit 包围盒共用；点线态按圆半径）
    let node_half_sizes: HashMap<String, (f64, f64)> = props
        .nodes
        .iter()
        .map(|n| {
            let size = if dot_mode {
                let r = node_radius_of(&n.id);
                (r, r)
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

    // 初始视图：auto_fit 打开时按节点包围盒 fit 一次（参照 Canvas 版 fit_viewport），
    // 否则维持 (0,0,1) —— 存量调用点（知识图谱 / 任务依赖图 / 全局点线）视觉零回归。
    // ⚠️ 包围盒取「当前形态的半尺寸」而不是卡片宽高：点线态若按 104~168px 的卡算，
    // 300×200 缩略图会被判成严重超出 → scale 压到 0.2~0.4，圆点再小一档。
    let initial_view = if props.auto_fit {
        compute_fit_view_with(&props.nodes, svg_width as f64, svg_height as f64, |n| {
            node_half_sizes
                .get(&n.id)
                .copied()
                .unwrap_or((node_box_width(n) / 2.0, node_box_height(n) / 2.0))
        })
    } else {
        (0.0, 0.0, 1.0)
    };
    let mut view_transform = use_signal(|| initial_view);
    let mut is_panning = use_signal(|| false);
    let mut pan_start = use_signal(|| (0.0, 0.0));
    // hover 详情卡片目标（区别于点击选中；拖拽/平移中隐藏）
    let mut hovered = use_signal(|| None::<HoverTarget>);

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

    // 自环绕行半径：节点半宽/半高取大者 + 8（弧线从卡片右缘绕出去，不压卡片本体）
    let loop_radius_of = |id: &str| -> f64 {
        node_half_sizes
            .get(id)
            .map(|(hw, hh)| hw.max(*hh) + 8.0)
            .unwrap_or(48.0)
    };

    // 箭头落点形态：点线态（全局视图 / 缩略图）用圆半径，卡片态用矩形半宽/半高。
    // 卡片态若仍按圆半径回退，箭头会停在矩形内部被后绘卡片盖住（三修③）。
    let tip_shape_of = |id: &str| -> (f64, Option<(f64, f64)>) {
        let radius = node_radius_of(id);
        if dot_mode {
            (radius, None)
        } else {
            (radius, node_half_sizes.get(id).copied())
        }
    };

    // 边标签置顶数据 + 边的曲线中点缓存（hover 锚点）
    // - 标签必须渲染在节点之后：SVG 无 z-index、后画覆盖先画，否则被卡片遮挡；
    // - 中点取自与渲染同一份几何（弧线上标签/hover 落在曲线中点，不是两端中点）；
    // - 全局点线态不绘边标签（对齐 Canvas 全局模式：点线承载类型着色）。
    let mut edge_mids: HashMap<(String, String), (f64, f64)> = HashMap::new();
    // (标签, transform, 边色, 衬底宽, 衬底 x)：衬底几何在这里算好——rsx 的 for 是
    // 宏语法节点，体内不能写 let，宽度计算必须留在 Rust 侧
    let edge_label_overlays: Vec<(String, String, &'static str, f64, f64)> = if dot_mode {
        for (edge, (sx, sy), (tx, ty)) in valid_edges.iter() {
            let (radius, half) = tip_shape_of(&edge.target);
            let geo = edge_geometry(
                edge,
                (*sx, *sy),
                (*tx, *ty),
                radius,
                half,
                tip_shape_of(&edge.source).1,
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
                let (radius, half) = tip_shape_of(&edge.target);
                let geo = edge_geometry(
                    edge,
                    (*sx, *sy),
                    (*tx, *ty),
                    radius,
                    half,
                    tip_shape_of(&edge.source).1,
                    loop_radius_of(&edge.source),
                );
                edge_mids
                    .entry((edge.source.clone(), edge.target.clone()))
                    .or_insert(geo.mid);
                let label: String = edge.label.chars().take(10).collect();
                (!label.is_empty()).then(|| {
                    // 衬底宽按文本度量算（中文每字 ≈10px，旧「字符数×7px」会让文字
                    // 左侧超出白底——「缺一角」观感）；x 取半宽负值保证左右对称
                    let box_w = edge_label_box_width(&label);
                    (
                        label,
                        format!(
                            "translate({}, {}) rotate({})",
                            geo.mid.0,
                            geo.mid.1,
                            upright_angle(geo.mid_angle)
                        ),
                        get_edge_color(&edge.label),
                        box_w,
                        -(box_w / 2.0),
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
                    let (node_radius, node_half) = tip_shape_of(&edge.target);
                    let geo = edge_geometry(
                        &edge,
                        (sx, sy),
                        (tx, ty),
                        node_radius,
                        node_half,
                        tip_shape_of(&edge.source).1,
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
                    if dot_mode {
                        // 点线态半径：全局视图按连接度（度数有语义），缩略图按画布自适应
                        // （固定 7px 在 300×200 上再被 auto_fit 缩一次，肉眼几乎看不见）
                        let radius = node_radius_of(&node.id);
                        // 缩略图不绘名：300×200 里节点名会互相压住，明细走 hover 卡；
                        // 全局视图按半径限绘（与 Canvas 全局模式同口径）
                        let label = if global_mode {
                            global_node_label(&node.label, radius)
                        } else {
                            String::new()
                        };
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
            // 衬底宽度走 text_width 度量（edge_label_box_width）：中文标签每字
            // ≈10px，旧「字符数×7px」估算下左侧超出白底，观感即「文字缺一角」
            for (label, transform, edge_color, label_box_w, label_box_x) in
                edge_label_overlays.iter()
            {
                g {
                    transform: "{transform}",
                    style: "pointer-events: none;",
                    rect {
                        x: "{label_box_x}",
                        y: "-7",
                        width: "{label_box_w}",
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
                        font_size: "{EDGE_LABEL_PX}",
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
    /// 测试用卡片口径 fit（生产侧只暴露 `compute_fit_view_with`，形态由调用方定）
    fn card_fit(nodes: &[GraphNode], width: f64, height: f64) -> (f64, f64, f64) {
        compute_fit_view_with(nodes, width, height, |n| {
            (
                super::node_box_width(n) / 2.0,
                super::node_box_height(n) / 2.0,
            )
        })
    }

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

    /// 边标签衬底必须完整包住文本且左右对称（中文标签「缺一角」事故的守卫）：
    /// 衬底半宽 = 字宽/2 + 3，最坏情况（全中文）下文字两侧仍各有 3px 白底。
    #[test]
    fn edge_label_box_covers_text_with_symmetric_padding() {
        for label in ["前置", "导致", "via前置", "a", "is_prerequisite_of"] {
            let text_w = node_card::text_width(label, EDGE_LABEL_PX);
            let box_w = edge_label_box_width(label);
            assert!(
                box_w >= text_w + 6.0 - 1e-9,
                "标签 `{label}` 衬底 {box_w} 应 ≥ 字宽 {text_w} + 6px padding"
            );
            // 对称性：x = -box_w/2，文字（text_anchor: middle）两侧余量相等
            let half = box_w / 2.0;
            assert!(
                (half - text_w / 2.0 - 3.0).abs() < 1e-9,
                "标签 `{label}` 左右 padding 应各 3px"
            );
        }
        // 全中文最坏场景：2 字 20px 字宽 → 衬底 26px（旧实现只有 18px，左侧缺 3px）
        assert!((edge_label_box_width("前置") - 26.0).abs() < 1e-9);
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
        let geo = edge_geometry(&e, (100.0, 100.0), (400.0, 100.0), 12.0, None, None, 48.0);
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
        let geo = edge_geometry(&e, (0.0, 0.0), (100.0, 0.0), 12.0, None, None, 48.0);
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
        let geo = edge_geometry(&e, (0.0, 0.0), (100.0, 0.0), 12.0, None, None, 48.0);
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
        let geo = edge_geometry(&e, (200.0, 200.0), (200.0, 200.0), 12.0, None, None, 48.0);
        assert!(geo.d.contains('C'), "自环必须走三次贝塞尔: {}", geo.d);
        assert!(geo.mid.0 > 200.0, "自环应绕到节点右侧: {}", geo.mid.0);
        assert!(geo.length > 40.0, "自环绕行长度不应退化: {}", geo.length);
    }

    #[test]
    fn upright_angle_keeps_labels_readable_in_both_directions() {
        // 正向/反向边的切线角互换 180°，标签旋转角必须落在 (-90°, 90°] 内
        for raw in [
            0.0, 45.0, 90.0, 135.0, 180.0, -135.0, -180.0, 179.9, -179.9, 270.0,
        ] {
            let a = upright_angle(raw);
            assert!(
                a > -90.0 && a <= 90.0,
                "标签角度必须落在 (-90, 90] 才正立: raw={raw} → {a}"
            );
        }
        // 同一条线的两个方向（0° / 180°）归一后是同一种朝向
        assert!((upright_angle(180.0) - 0.0).abs() < 1e-9);
        assert!((upright_angle(-180.0) - 0.0).abs() < 1e-9);
        // 斜线只翻不转：45° 保持 45°，135° 翻成 -45°，斜率不同侧但都正立
        assert!((upright_angle(45.0) - 45.0).abs() < 1e-9);
        assert!((upright_angle(135.0) + 45.0).abs() < 1e-9);
        assert!((upright_angle(-135.0) - 45.0).abs() < 1e-9);
    }

    #[test]
    fn mid_angle_stays_parallel_to_chord_for_arc_and_turns_for_loop() {
        // 二次贝塞尔的中点切线**恒平行于两端连线**（平行四边形性质）：
        // 弧顶标签仍沿「起点→终点」方向摆放，不会歪成别的角度。
        let e = edge_with(Some(EdgeCurve::Arc(40.0)), "undirected");
        let geo = edge_geometry(&e, (0.0, 0.0), (100.0, 100.0), 12.0, None, None, 48.0);
        assert!((geo.mid_angle - 45.0).abs() < 1e-6, "{}", geo.mid_angle);
        // 自环是三次贝塞尔：中点切线由控制点决定，在右侧绕行段近垂直
        let loop_e = edge_with(Some(EdgeCurve::Loop), "undirected");
        let loop_geo = edge_geometry(
            &loop_e,
            (200.0, 200.0),
            (200.0, 200.0),
            12.0,
            None,
            None,
            48.0,
        );
        assert!(
            loop_geo.mid_angle.abs() > 45.0,
            "自环中点切线应近垂直: {}",
            loop_geo.mid_angle
        );
    }

    // ==================== 起点收缩守卫（第五轮反馈①） ====================
    // 放大后「线穿过节点卡片」的根因是 `d` 从源卡片**中心**起笔：终点收了、起点没收。
    // 以下把「起点必须落在源矩形外缘」钉死。

    /// 从 `d` 里取第一个坐标（`M x y`）
    fn first_point(d: &str) -> (f64, f64) {
        let mut it = d.split_whitespace();
        assert_eq!(it.next(), Some("M"), "路径必须以 M 起笔: {d}");
        let x: f64 = it.next().unwrap().parse().unwrap();
        let y: f64 = it.next().unwrap().parse().unwrap();
        (x, y)
    }

    #[test]
    fn edge_geometry_clips_source_to_card_boundary() {
        let e = edge_with(None, "directed");
        let half = (60.0, 30.0);
        let geo = edge_geometry(
            &e,
            (100.0, 100.0),
            (400.0, 100.0),
            7.0,
            None,
            Some(half),
            48.0,
        );
        // 水平出射撞右缘：起点 = 中心 + 半宽 + GAP(2)
        assert!(
            geo.d.starts_with("M 162 100 L"),
            "起点必须收缩到源卡片右缘外: {}",
            geo.d
        );
        // 命中路径同口径（否则透明命中层仍从卡片中心画出去）
        assert!(
            geo.d_full.starts_with("M 162 100 L"),
            "命中路径也必须从起点起笔: {}",
            geo.d_full
        );
        // 点线态（source_half = None）保持原口径：从中心起笔
        let dot = edge_geometry(&e, (100.0, 100.0), (400.0, 100.0), 7.0, None, None, 48.0);
        assert!(dot.d.starts_with("M 100 100 L"), "{}", dot.d);
    }

    /// dagre 折线：路径必须一直画到箭头落点（目标卡片外缘）
    ///
    /// dagre 的边界求交点（离节点中心 = 节点半尺寸）在 `dagre_layout` 里被整体
    /// 缩放过（`scale > 1` 时离卡片更远），只画到折线末点会在卡片前**裂开一截**——
    /// 用户截图「线没指到卡片上」的根因。
    #[test]
    fn polyline_path_reaches_target_card_boundary() {
        let e = edge_with(
            Some(EdgeCurve::Polyline(vec![(100.0, 160.0), (100.0, 300.0)])),
            "directed",
        );
        let half = (84.0, 37.0);
        let geo = edge_geometry(
            &e,
            (100.0, 100.0),
            (100.0, 400.0),
            7.0,
            Some(half),
            Some(half),
            48.0,
        );
        // 路径末点 = 箭头落点
        assert!(
            geo.d.ends_with(&format!("L {} {}", geo.tip.0, geo.tip.1)),
            "折线必须画到箭头落点: {} / tip={:?}",
            geo.d,
            geo.tip
        );
        // 箭头落点 = 目标卡片上缘外 GAP
        assert!(
            (geo.tip.1 - (400.0 - 37.0 - 2.0)).abs() < 1e-6,
            "箭头落点应在目标卡片上缘外: {:?}",
            geo.tip
        );
        // 起点同样必须收进源卡片外缘：旧判据拿矩形**对角线**比距离，
        // 卡片下方几十 px 的路点会被误判成「方向不可靠」→ 起点留在卡心
        let s = first_point(&geo.d);
        assert!(
            (s.1 - 100.0).abs() >= 37.0,
            "折线起点必须落在源卡片外缘: {s:?}"
        );
    }

    #[test]
    fn arc_geometry_clips_source_along_start_tangent() {
        let e = edge_with(Some(EdgeCurve::Arc(30.0)), "directed");
        let half = (60.0, 30.0);
        let geo = edge_geometry(
            &e,
            (100.0, 100.0),
            (400.0, 100.0),
            7.0,
            None,
            Some(half),
            48.0,
        );
        // 弧线起点沿「源中心 → 控制点」出射，必须已离开卡片中心并落在源矩形之外
        let (x, y) = first_point(&geo.d);
        assert!(
            (x - 100.0).abs() > 1.0 || (y - 100.0).abs() > 1.0,
            "弧线起点不得仍在卡片中心: {x}, {y}"
        );
        assert!(
            (x - 100.0).abs() >= 60.0 - 0.5 || (y - 100.0).abs() >= 30.0 - 0.5,
            "弧线起点必须落在源矩形之外: {x}, {y}"
        );
        // 无起点信息时退回旧口径（起点 = 中心）
        let dot = edge_geometry(&e, (100.0, 100.0), (400.0, 100.0), 7.0, None, None, 48.0);
        assert_eq!(first_point(&dot.d), (100.0, 100.0));
    }

    // ==================== 卡片态箭头落点守卫 ====================
    // 三修③：卡片节点是矩形，箭头终点若按「离中心 radius + GAP」回退就会落进
    // 矩形内部、被后绘卡片盖住。以下三例把「箭头必须停在矩形外」钉死。

    #[test]
    fn card_target_keeps_arrow_tip_outside_rect_on_horizontal_hit() {
        let e = edge_with(None, "directed");
        let half = (84.0, 45.0);
        let geo = edge_geometry(
            &e,
            (100.0, 100.0),
            (400.0, 100.0),
            7.0,
            Some(half),
            None,
            48.0,
        );
        // 水平入射撞左缘：tip.x = 400 - 84 - GAP(2)
        assert!(
            (geo.tip.0 - (400.0 - 84.0 - 2.0)).abs() < 1e-9,
            "水平入射箭头应停在左缘外 GAP 处: {:?}",
            geo.tip
        );
        assert!(
            (geo.tip.1 - 100.0).abs() < 1e-9,
            "水平入射不应偏移 y: {:?}",
            geo.tip
        );
        // 旧圆口径只回退 7 + 2 = 9px，这里必须已明显越过左缘
        assert!(
            geo.tip.0 < 400.0 - 84.0,
            "箭头不得落进矩形内部: {:?}",
            geo.tip
        );
        // 命中路径仍到中心（hover 区更宽松，与视觉分离是有意的）
        assert!(geo.d_full.contains("L 400 100"), "{}", geo.d_full);
    }

    #[test]
    fn card_target_intersects_top_and_left_edges_by_direction() {
        let e = edge_with(None, "directed");
        let half = (84.0, 45.0);
        // 垂直入射：撞上缘（不是左右缘）
        let v = edge_geometry(
            &e,
            (200.0, 0.0),
            (200.0, 300.0),
            7.0,
            Some(half),
            None,
            48.0,
        );
        assert!(
            (v.tip.1 - (300.0 - 45.0 - 2.0)).abs() < 1e-9,
            "垂直入射箭头应停在上缘外 GAP 处: {:?}",
            v.tip
        );
        assert!((v.tip.0 - 200.0).abs() < 1e-9);
        // 斜向入射：撞最近的那条边，tip 必须落在矩形之外（至少一轴超出半宽/半高）
        let d = edge_geometry(
            &e,
            (0.0, 300.0),
            (400.0, 100.0),
            7.0,
            Some(half),
            None,
            48.0,
        );
        let dx = (d.tip.0 - 400.0).abs();
        let dy = (d.tip.1 - 100.0).abs();
        assert!(
            dx >= 84.0 || dy >= 45.0,
            "斜向入射箭头必须落在矩形边界之外: {:?}",
            d.tip
        );
    }

    #[test]
    fn arc_arrow_on_card_target_also_stays_outside_rect() {
        // 弧线走「控制点 → 终点」切线求交，矩形口径同样生效
        let e = edge_with(Some(EdgeCurve::Arc(30.0)), "directed");
        let geo = edge_geometry(
            &e,
            (0.0, 0.0),
            (100.0, 0.0),
            7.0,
            Some((40.0, 20.0)),
            None,
            48.0,
        );
        let dx = (geo.tip.0 - 100.0).abs();
        let dy = (geo.tip.1 - 0.0).abs();
        assert!(
            dx >= 40.0 || dy >= 20.0,
            "弧线箭头必须落在矩形边界之外: {:?}",
            geo.tip
        );
        assert!(
            (geo.tip.1 - 0.0).abs() > 0.5,
            "弧线箭头应带末端切线偏移: {:?}",
            geo.tip
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
        let (tx, ty, scale) = card_fit(&nodes, 300.0, 200.0);
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
        assert_eq!(card_fit(&[], 300.0, 200.0), (0.0, 0.0, 1.0));
        assert_eq!(card_fit(&[], 0.0, 0.0), (0.0, 0.0, 1.0));
    }

    /// 缩略图圆点半径必须跟画布尺寸与节点数走（AMan 第八轮反馈②「圆点太小」）
    /// 缩略图圆点半径必须跟画布尺寸走，节点数只做**轻微阻尼**
    /// （AMan 第八轮反馈②「圆点太小」+ 第九轮反馈①「两个缩略图点大小不一样」）
    #[test]
    fn dot_node_radius_scales_with_canvas_and_node_count() {
        // 同一节点数：画布越大点越大
        let small = dot_node_radius(300.0, 200.0, 4);
        let big = dot_node_radius(920.0, 620.0, 4);
        assert!(big > small, "大画布点半径应更大: {big} vs {small}");
        // 同一画布：节点越多点越小
        let many = dot_node_radius(300.0, 200.0, 12);
        assert!(small > many, "节点多时点半径应更小: {small} vs {many}");
        // 关键守卫：任务图（4 个任务）与思考图（8 个节点）在同一块缩略图上点径必须相同
        let tasks = dot_node_radius(300.0, 200.0, 4);
        let thinks = dot_node_radius(300.0, 200.0, 8);
        assert!(
            (tasks - thinks).abs() < 1e-9,
            "4 节点与 8 节点的点半径必须一致（同一套缩略图观感）: {tasks} vs {thinks}"
        );
        // 300×200 缩略图里不能退回常量 7px（那是「看不清」的根因）
        assert!(
            dot_node_radius(300.0, 200.0, 5) >= 15.0,
            "缩略图点半径过小: {}",
            dot_node_radius(300.0, 200.0, 5)
        );
        // 上下界：极小画布 / 极多节点不塌成 0，极大画布不失控
        // 3 节点（真实思考轨迹的常见规模）与 4 节点（任务图）也必须同值
        assert!(
            (dot_node_radius(300.0, 200.0, 3) - dot_node_radius(300.0, 200.0, 4)).abs() < 1e-9,
            "3 节点与 4 节点点径必须一致"
        );
        // 300×200 缩略图基准 16px（第十轮 24px → 2026-10-24 按「直径缩短 1/3」下调）
        assert!(
            (dot_node_radius(300.0, 200.0, 3) - 16.0).abs() < 1e-9,
            "缩略图点半径应为 16px，实际 {}",
            dot_node_radius(300.0, 200.0, 3)
        );
        assert!(dot_node_radius(20.0, 20.0, 500) >= 10.0);
        assert!(dot_node_radius(4000.0, 4000.0, 1) <= 28.0);
    }

    /// fit 包围盒必须取「当前形态的半尺寸」：点线态不能被卡片宽高拖小
    #[test]
    fn fit_view_in_dot_mode_uses_point_radius_not_card_box() {
        let nodes = vec![
            GraphNode {
                id: "a".to_string(),
                label: "A".to_string(),
                x: 40.0,
                y: 100.0,
                ..Default::default()
            },
            GraphNode {
                id: "b".to_string(),
                label: "B".to_string(),
                x: 260.0,
                y: 100.0,
                ..Default::default()
            },
        ];
        // 卡片口径：单卡宽 104~168 → 内容宽 ≈ 220 + 168 = 388，300 画布必须缩
        let (_, _, card_scale) = card_fit(&nodes, 300.0, 200.0);
        // 点线口径：半径 14 → 内容宽 ≈ 220 + 28 = 248，300 画布不该缩
        let (_, _, dot_scale) = compute_fit_view_with(&nodes, 300.0, 200.0, |_| (14.0, 14.0));
        assert!(
            dot_scale > card_scale,
            "点线态不应被卡片宽高拖小: {dot_scale} vs {card_scale}"
        );
        assert_eq!(dot_scale, 1.0, "点线态在 300×200 里应保持原尺寸");
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

    #[test]
    fn parallel_edges_arc_to_opposite_sides_of_reference_line() {
        // 同一对节点的双向两条边：直线绘制会几何完全重合，分档后必须分居两侧、等幅对称
        let mk = |s: &str, t: &str| GraphEdge {
            source: s.to_string(),
            target: t.to_string(),
            label: "相关".to_string(),
            weight: None,
            direction: "undirected".to_string(),
            curve: None,
            dash: None,
            width: None,
        };
        let mut edges = vec![mk("a", "b"), mk("b", "a")];
        assign_parallel_edge_curves(&mut edges, 30.0);
        // 两条都是重合嫌疑方，都必须带弧
        assert!(
            edges
                .iter()
                .all(|e| matches!(e.curve, Some(EdgeCurve::Arc(_)))),
            "{edges:?}"
        );
        let a = (100.0, 100.0);
        let b = (300.0, 100.0);
        let mut offsets = Vec::new();
        for e in &edges {
            let (from, to) = if e.source == "a" { (a, b) } else { (b, a) };
            let Some(EdgeCurve::Arc(bulge)) = e.curve else {
                unreachable!("上面已断言两条都是 Arc")
            };
            let ctrl = quadratic_control(from.0, from.1, to.0, to.1, bulge);
            // 二次贝塞尔 t=0.5 点：(P0 + 2P1 + P2) / 4
            let mid_y = (from.1 + 2.0 * ctrl.1 + to.1) / 4.0;
            offsets.push(mid_y - 100.0);
        }
        // 参考线 y = 100：两条弧的中点必须分居两侧且等幅
        assert!(offsets[0] * offsets[1] < 0.0, "{offsets:?}");
        assert!((offsets[0] + offsets[1]).abs() < 1e-6, "{offsets:?}");
        assert!(offsets[0].abs() > 1.0, "{offsets:?}");
    }

    #[test]
    fn lone_edge_keeps_straight_and_self_loop_untouched() {
        // 单条边保持直线（非分层图里弧线不承担避障，存量视觉零变化）；
        // 自环由 EdgeCurve::Loop 承载，分档不得改写它
        let mut edges = vec![
            GraphEdge {
                source: "a".to_string(),
                target: "b".to_string(),
                label: "相关".to_string(),
                weight: None,
                direction: "undirected".to_string(),
                curve: None,
                dash: None,
                width: None,
            },
            GraphEdge {
                source: "c".to_string(),
                target: "c".to_string(),
                label: "相关".to_string(),
                weight: None,
                direction: "undirected".to_string(),
                curve: Some(EdgeCurve::Loop),
                dash: None,
                width: None,
            },
        ];
        assign_parallel_edge_curves(&mut edges, 30.0);
        assert_eq!(edges[0].curve, None);
        assert_eq!(edges[1].curve, Some(EdgeCurve::Loop));
    }
}
