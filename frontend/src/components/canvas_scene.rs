//! Canvas 场景渲染基础设施
//!
//! 提供 Dioxus ↔ Canvas 2D 桥接层：
//! - CanvasScene 组件封装 <canvas> 元素 + Context 初始化
//! - CanvasRenderer trait 抽象渲染逻辑（由业务场景实现）
//! - 事件桥：鼠标事件 → 坐标转换 → 命中检测 → Dioxus callback
//! - 渲染循环：request_animation_frame + 力导向布局 + 拖拽 + hover/选中动画

use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

use crate::components::button::{Button, ButtonVariant};
use crate::components::edge_style;
use crate::components::force_layout::{ForceLayout, ForceLayoutConfig, circle_initial_layout};
use crate::components::hud_palette;
use crate::components::node_card;
use crate::components::particles::{
    BackgroundParticles, BirthDeathParticles, DataFlowParticles, GlowParticles, ParticleSystem,
};

/// Canvas 渲染节点（通用数据结构，业务场景填充字段）
///
/// 节点形态由数据决定（见 [`CanvasNode::is_card`]）：
/// - 只填 `label` / `node_type` → **圆形**（工作台拓扑、Agent 关系图，视觉不变）
/// - 再填 `summary` / `description` / `tags` → **矩形信息卡**（知识图谱）
///
/// 这样三处图谱共用同一个默认渲染器，差异靠数据表达，而不是各写一套渲染器。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CanvasNode {
    pub id: String,
    pub x: f64,
    pub y: f64,
    /// 节点等效半径。圆形节点即圆半径；卡片节点应填**卡片外接圆半径**
    /// （`max(卡宽, 卡高) / 2`），力导向的碰撞避让依赖它，填小了卡片仍会互压。
    pub radius: f64,
    pub label: String,
    pub color: String,
    /// 节点类型标识（如 "project"/"agent"/"task"），用于点击回调判断
    pub node_type: Option<String>,
    /// 分层布局的层级（0=顶层，越大越靠下；None 表示不参与分层约束）
    pub layer: Option<i32>,
    /// 摘要：卡片正文优先取它（None / 空白时回退 [`Self::description`]）
    pub summary: Option<String>,
    /// 正文/描述：卡片上只放前两行，完整内容留给 hover 详情
    pub description: String,
    /// 分类标签（卡片底部胶囊 + hover 详情）
    pub tags: Vec<String>,
    /// 搜索命中等外部高亮（调用方按业务判定；渲染层只负责让它更亮）
    pub highlighted: bool,
}

impl CanvasNode {
    /// 是否渲染为矩形信息卡
    ///
    /// 判据是「有没有可展示的正文」——纯「名称 + 类型」的节点保持圆形：
    /// 卡片是渐进增强，数据不够就不硬撑一张空卡片。
    pub fn is_card(&self) -> bool {
        !self.description.trim().is_empty()
            || self
                .summary
                .as_deref()
                .is_some_and(|s| !s.trim().is_empty())
            || !self.tags.is_empty()
    }
}

/// Canvas 渲染连线
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CanvasEdge {
    pub from_id: String,
    pub to_id: String,
    /// 边的关系逻辑标签：表达连通性/关系类型，如 "ready" / "not_ready" / "disabled"。
    /// 渲染层据此着色与分组；为 None 时退化为默认灰线（对其它关系图零侵入）。
    pub tag: Option<String>,
    /// 边的关系描述：hover 时展示，例如未就绪原因与修复提示。
    pub description: Option<String>,
    /// 关系强度（0.0~1.0，越大越强）；`None` = 未标注
    ///
    /// 走 `edge_style::weight_style` 映射成**线宽 + 浓淡**（颜色已被关系类型/
    /// 状态语义占用，不再拿颜色表达强度）。未标注渲染基准粗细、不是最细 ——
    /// 只有知识图谱这类会声明强度的场景才填值，其余关系图留 `None` 即可。
    pub weight: Option<f32>,
}

/// 画布视口变换：世界坐标 ↔ 屏幕坐标
///
/// 屏幕坐标 = 世界坐标 × `scale` + `pan`。节点 / 边 / 粒子的坐标始终活在世界坐标里，
/// 缩放平移只改这一对参数 —— 力导向、拖拽落点、命中检测的语义全都不变，
/// 唯一的差别是「读鼠标坐标前先 `screen_to_world` 换算一次」。
///
/// 与 SVG 版 `graph.rs` 的 `view_transform`（`translate + scale`）语义一致，
/// 三处图谱因此共享同一套手感，而不是各有各的缩放。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub scale: f64,
    pub pan_x: f64,
    pub pan_y: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            scale: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
        }
    }
}

impl Viewport {
    /// 缩放下限：再小就看不清卡片正文了，此时「适应内容」比重排视图更有用
    pub const MIN_SCALE: f64 = 0.35;
    /// 缩放上限：再大只是在放大同一张卡片，不增加信息
    pub const MAX_SCALE: f64 = 2.5;

    pub fn world_to_screen(&self, x: f64, y: f64) -> (f64, f64) {
        (x * self.scale + self.pan_x, y * self.scale + self.pan_y)
    }

    pub fn screen_to_world(&self, x: f64, y: f64) -> (f64, f64) {
        ((x - self.pan_x) / self.scale, (y - self.pan_y) / self.scale)
    }

    /// 以屏幕坐标为锚点缩放：锚点下的世界坐标保持不动（滚轮缩放的正确手感）
    ///
    /// 只改 `scale` 而不动 `pan` 会让画面绕原点（左上角）缩放 —— 想看的节点会跑出视口。
    pub fn zoomed_at(&self, factor: f64, anchor_x: f64, anchor_y: f64) -> Self {
        let scale = (self.scale * factor).clamp(Self::MIN_SCALE, Self::MAX_SCALE);
        let (wx, wy) = self.screen_to_world(anchor_x, anchor_y);
        Self {
            scale,
            pan_x: anchor_x - wx * scale,
            pan_y: anchor_y - wy * scale,
        }
    }

    /// 按屏幕像素平移（pan 本身就在屏幕空间，拖拽增量直接相加，不必除以 scale）
    pub fn panned(&self, dx: f64, dy: f64) -> Self {
        Self {
            pan_x: self.pan_x + dx,
            pan_y: self.pan_y + dy,
            ..*self
        }
    }
}

/// 「适应内容」视口：把全部节点包进画布并居中
///
/// 节点被拖散或缩放过深之后，用户需要一个「一键回到能看全」的出口；
/// 单纯重置成 1:1 解决不了「节点被拖到画布外」的情况，所以两者都提供。
pub fn fit_viewport(nodes: &[CanvasNode], width: f64, height: f64) -> Viewport {
    if nodes.is_empty() || width <= 0.0 || height <= 0.0 {
        return Viewport::default();
    }
    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for n in nodes {
        // 包围盒按「看到的形状」算：卡片用实际卡宽高，圆用半径
        let (hw, hh) = if n.is_card() {
            let (w, h) = card_size(n);
            (w / 2.0, h / 2.0)
        } else {
            (n.radius, n.radius)
        };
        min_x = min_x.min(n.x - hw);
        max_x = max_x.max(n.x + hw);
        min_y = min_y.min(n.y - hh);
        max_y = max_y.max(n.y + hh);
    }
    if !min_x.is_finite() || !max_x.is_finite() || !min_y.is_finite() || !max_y.is_finite() {
        return Viewport::default();
    }
    /// 四周留白：卡片描边/选中扫描框会略微外扩，贴边会被裁掉
    const PAD: f64 = 40.0;
    let span_w = (max_x - min_x).max(1.0);
    let span_h = (max_y - min_y).max(1.0);
    let scale = ((width - PAD * 2.0) / span_w)
        .min((height - PAD * 2.0) / span_h)
        .clamp(Viewport::MIN_SCALE, Viewport::MAX_SCALE);
    let (cx, cy) = ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0);
    Viewport {
        scale,
        pan_x: width / 2.0 - cx * scale,
        pan_y: height / 2.0 - cy * scale,
    }
}

/// 节点类型 → 渲染色（按 kind 分类着色，未知 tag 经哈希落入调色板，保证稳定且区分）
pub fn node_color_for_kind(kind: &Option<String>) -> String {
    match kind.as_deref() {
        Some("agent") => "#fa520f".to_string(),
        Some("neural_tool") => "#6366f1".to_string(),
        Some("bound_tool") => "#10b981".to_string(),
        Some("pack_tool") => "#f59e0b".to_string(),
        Some("skill") | Some("neural_skill") => "#ec4899".to_string(),
        Some("project") => "#3b82f6".to_string(),
        Some("task") => "#8b5cf6".to_string(),
        Some(other) => tag_color(other),
        None => "#64748b".to_string(),
    }
}

/// 将任意 tag 字符串稳定地映射到调色板中的颜色（FNV-1a 哈希）
fn tag_color(tag: &str) -> String {
    const PALETTE: &[&str] = &[
        "#0ea5e9", "#14b8a6", "#f97316", "#eab308", "#84cc16", "#06b6d4", "#a855f7", "#ef4444",
        "#22c55e", "#3b82f6", "#ec4899", "#f43f5e", "#8b5cf6", "#10b981", "#f59e0b",
    ];
    let mut h: u64 = 1469598103934665603;
    for b in tag.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    PALETTE[(h as usize) % PALETTE.len()].to_string()
}

/// 精确测量文本在 canvas 中的渲染宽度（依赖 web-sys `TextMetrics` 特性）。
///
/// 使用 ctx 当前已设置的 font，调用前必须先 `set_font` 到目标字号/字体。
/// 若 `measure_text` 因极端异常失败，回退到「字符数 × 字号 × 0.6」估算，保证不崩。
pub fn measure_text_width(ctx: &CanvasRenderingContext2d, text: &str, font_px: f64) -> f64 {
    ctx.measure_text(text)
        .map(|m| m.width())
        .unwrap_or_else(|_| text.chars().count() as f64 * font_px * 0.6)
}

/// 点到线段距离（用于边 hover 命中检测）
fn point_to_segment_distance(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let dx = bx - ax;
    let dy = by - ay;
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return ((px - ax).powi(2) + (py - ay).powi(2)).sqrt();
    }
    let t = (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0);
    let cx = ax + t * dx;
    let cy = ay + t * dy;
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}

/// 找距离鼠标最近且在阈值内的边，返回 (from_id, to_id)
fn nearest_edge(
    edges: &[CanvasEdge],
    nodes: &[CanvasNode],
    x: f64,
    y: f64,
    threshold: f64,
) -> Option<(String, String)> {
    let mut best: Option<(f64, (String, String))> = None;
    for e in edges {
        let from = nodes.iter().find(|n| n.id == e.from_id);
        let to = nodes.iter().find(|n| n.id == e.to_id);
        if let (Some(f), Some(t)) = (from, to) {
            let d = point_to_segment_distance(x, y, f.x, f.y, t.x, t.y);
            if d <= threshold {
                let better = match best {
                    Some((bd, _)) => d < bd,
                    None => true,
                };
                if better {
                    best = Some((d, (e.from_id.clone(), e.to_id.clone())));
                }
            }
        }
    }
    best.map(|(_, pair)| pair)
}

/// hover 卡片统一规格：内边距 / 行高 / 字号（节点卡与边卡共用）
const HOVER_CARD_PADDING: f64 = 8.0;
const HOVER_CARD_LINE_H: f64 = 16.0;
const HOVER_CARD_FONT_PX: f64 = 11.0;

/// 测量统一 hover 卡片尺寸（宽, 高），供调用方做左右避让定位
pub fn hover_card_size(ctx: &CanvasRenderingContext2d, lines: &[String]) -> (f64, f64) {
    ctx.set_font(&format!("{HOVER_CARD_FONT_PX}px sans-serif"));
    // 精确测量文本宽度（web-sys TextMetrics 特性）
    let max_w = lines
        .iter()
        .map(|l| measure_text_width(ctx, l, HOVER_CARD_FONT_PX))
        .fold(0.0f64, f64::max);
    (
        max_w + HOVER_CARD_PADDING * 2.0,
        lines.len() as f64 * HOVER_CARD_LINE_H + HOVER_CARD_PADDING * 2.0,
    )
}

/// 统一 hover 详情卡片：深底 + 主色描边 + 行文本列表（节点卡与边卡共用绘制）
pub fn draw_hover_card(
    ctx: &CanvasRenderingContext2d,
    bx: f64,
    by: f64,
    lines: &[String],
    accent_color: &str,
) {
    let (box_w, box_h) = hover_card_size(ctx, lines);
    // 背景
    ctx.set_fill_style_str("rgba(17, 24, 39, 0.92)");
    ctx.fill_rect(bx, by, box_w, box_h);
    // 边框用主题色，强化归属
    ctx.set_stroke_style_str(accent_color);
    ctx.set_line_width(1.5);
    ctx.stroke_rect(bx, by, box_w, box_h);
    // 文本
    ctx.set_fill_style_str("#f9fafb");
    ctx.set_text_align("left");
    ctx.set_text_baseline("top");
    for (i, l) in lines.iter().enumerate() {
        let _ = ctx.fill_text(
            l,
            bx + HOVER_CARD_PADDING,
            by + HOVER_CARD_PADDING + i as f64 * HOVER_CARD_LINE_H,
        );
    }
}

/// 绘制 hover 卡并做画布内避让：优先放锚点右侧，右缘放不下翻左侧，纵向夹在画布内
///
/// `half_w` 是锚点物体的半宽（卡片半宽 / 圆半径）—— 卡片宽 168px，不避让时
/// 右半张图上的 hover 卡会直接跑出画布。
fn draw_hover_card_anchored(
    ctx: &CanvasRenderingContext2d,
    lines: &[String],
    accent: &str,
    anchor_x: f64,
    anchor_y: f64,
    half_w: f64,
) {
    let (box_w, box_h) = hover_card_size(ctx, lines);
    // 画布按 CSS 像素量（ctx 已 scale(dpr)，绘制坐标系与 client_* 同单位）；
    // 取不到 canvas 时按 0 处理，退化为「只往右放」，不影响绘制
    let (canvas_w, canvas_h) = ctx
        .canvas()
        .map(|c| (c.client_width() as f64, c.client_height() as f64))
        .unwrap_or((0.0, 0.0));

    let right_x = anchor_x + half_w + 10.0;
    let left_x = anchor_x - half_w - 10.0 - box_w;
    let bx = if right_x + box_w <= canvas_w || left_x < 0.0 {
        right_x
    } else {
        left_x
    };
    let by = (anchor_y - box_h / 2.0).clamp(0.0, (canvas_h - box_h).max(0.0));
    draw_hover_card(ctx, bx, by, lines, accent);
}

/// 绘制边 hover 提示框（关系标签 + 强度 + 描述）
fn draw_edge_tooltip(ctx: &CanvasRenderingContext2d, x: f64, y: f64, edge: &CanvasEdge) {
    let tag_label = match edge.tag.as_deref() {
        Some("ready") => "就绪",
        Some("not_ready") => "未就绪",
        Some(other) => other,
        None => return,
    };
    let mut lines = vec![format!("关系: {}", tag_label)];
    // 强度只在**标注过**时显示：未标注整行不渲染（写成「强度 0%」会被读成
    // 「明确很弱」）。Agent 关系图不给强度，也就不会平白多出一行噪音。
    if let Some(label) = edge_style::weight_label(edge.weight) {
        lines.push(label);
    }
    if let Some(desc) = &edge.description
        && !desc.is_empty()
    {
        lines.push(format!("说明: {}", desc));
    }
    // 边框按状态着色，强化语义
    let accent = if edge.tag.as_deref() == Some("not_ready") {
        "#f97316"
    } else {
        "#94a3b8"
    };
    draw_hover_card_anchored(ctx, &lines, accent, x, y, 0.0);
}

/// Canvas 渲染器 trait：业务场景实现此 trait 定义渲染逻辑
pub trait CanvasRenderer {
    /// 清空画布
    fn clear(&self, ctx: &CanvasRenderingContext2d, width: f64, height: f64);

    /// 绘制所有节点
    fn draw_nodes(&self, ctx: &CanvasRenderingContext2d, nodes: &[CanvasNode]);

    /// 绘制所有连线
    fn draw_edges(
        &self,
        ctx: &CanvasRenderingContext2d,
        edges: &[CanvasEdge],
        nodes: &[CanvasNode],
    );

    /// 带交互状态的连线渲染（hover 边提示），默认委托给 draw_edges
    fn draw_edges_with_state(
        &self,
        ctx: &CanvasRenderingContext2d,
        edges: &[CanvasEdge],
        nodes: &[CanvasNode],
        hovered_edge: &Option<(String, String)>,
    ) {
        let _ = hovered_edge;
        self.draw_edges(ctx, edges, nodes);
    }

    /// 命中检测：给定画布坐标，返回命中的节点 ID（None 表示空白处）
    fn hit_test(&self, nodes: &[CanvasNode], x: f64, y: f64) -> Option<String>;

    /// 带交互状态的节点渲染（hover/选中/拖拽），默认委托给 draw_nodes
    ///
    /// 只负责节点**本体**；hover 详情卡属于浮层，见 [`CanvasRenderer::draw_overlays`]。
    fn draw_nodes_with_state(
        &self,
        ctx: &CanvasRenderingContext2d,
        nodes: &[CanvasNode],
        hovered: &Option<String>,
        selected: &Option<String>,
        dragging: &Option<String>,
    ) {
        let _ = (hovered, selected, dragging);
        self.draw_nodes(ctx, nodes);
    }

    /// 绘制浮层（hover 详情卡等），调用方保证 ctx 处于**屏幕坐标系**
    ///
    /// 拆出来是为了让浮层与视口解耦：画布内容随缩放/平移变换，浮层不动
    /// （字号恒定、按屏幕边缘避让）。混在 `draw_nodes_with_state` 里画的话，
    /// 缩放后详情卡会跟着一起缩放并跑到画布外。
    fn draw_overlays(
        &self,
        ctx: &CanvasRenderingContext2d,
        nodes: &[CanvasNode],
        hovered: &Option<String>,
        dragging: &Option<String>,
        viewport: Viewport,
    ) {
        let _ = (ctx, nodes, hovered, dragging, viewport);
    }
}

// ==================== 节点形态：圆形 / 矩形信息卡 ====================
//
// DefaultRenderer 同时支持两种形态，由数据决定（见 CanvasNode::is_card）：
//   只填 label/node_type → 圆形（工作台拓扑、Agent 关系图，视觉不变）
//   再填 summary/description/tags → 矩形信息卡（知识图谱）
//
// 这里刻意**不做**「业务渲染器」：三处图谱的差异全是数据差异，多一套渲染器就多一份
// 漂移。历史教训——KnowledgeGraphRenderer 写了 400 行从未被执行，因为 CanvasScene
// 内部硬编码 DefaultRenderer，产物是「圆圈 + 圆下完整 ID」，用户看到的就是这个。

/// 圆角矩形路径（卡片本体 / 扫描框 / 标签胶囊共用，避免各处手写 round_rect）
pub fn round_rect_path(ctx: &CanvasRenderingContext2d, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    ctx.begin_path();
    ctx.move_to(x + r, y);
    ctx.line_to(x + w - r, y);
    ctx.quadratic_curve_to(x + w, y, x + w, y + r);
    ctx.line_to(x + w, y + h - r);
    ctx.quadratic_curve_to(x + w, y + h, x + w - r, y + h);
    ctx.line_to(x + r, y + h);
    ctx.quadratic_curve_to(x, y + h, x, y + h - r);
    ctx.line_to(x, y + r);
    ctx.quadratic_curve_to(x, y, x + r, y);
    ctx.close_path();
}

/// 卡片节点的实际宽高（内容驱动；与 SVG 版共用 node_card 的几何，不许各算一套）
pub fn card_size(node: &CanvasNode) -> (f64, f64) {
    let body = card_body_lines(node);
    (
        node_card::box_width(&node.label, body.len()),
        node_card::box_height(body.len(), !node.tags.is_empty()),
    )
}

/// 卡片正文行（摘要优先，回退描述）
fn card_body_lines(node: &CanvasNode) -> Vec<String> {
    node_card::body_lines(node.summary.as_deref(), &node.description)
}

/// 当前时间（秒，浮点）：HUD 呼吸/扫描动画的相位来源
fn now_secs() -> f64 {
    js_sys::Date::now() / 1000.0
}

/// 绘制矩形信息卡：左侧类型色竖条 + 名称 + 摘要/描述 ≤2 行 + 标签胶囊
///
/// 与 hover 详情卡同族（深底 + 类型色描边），使「卡片即节点、hover 出详情」
/// 成为一条连续的信息层级，而不是两种视觉语言。
///
/// `glow` 覆盖 hover / 拖拽 / 搜索命中三种「临时突出」——它们视觉诉求一致
/// （更亮 + 类型色外发光），分开传只会让参数表变长。
fn draw_card_node(
    ctx: &CanvasRenderingContext2d,
    node: &CanvasNode,
    is_selected: bool,
    glow: bool,
) {
    let now = now_secs();
    let (bw, bh) = card_size(node);
    let bx = node.x - bw / 2.0;
    let by = node.y - bh / 2.0;
    let accent = node.color.as_str();

    // 选中态：向外扩散的扫描框（矩形版雷达波纹）
    if is_selected {
        let t = (now % 1.8) / 1.8;
        let inflate = 4.0 + t * 14.0;
        ctx.set_stroke_style_str(&hud_palette::hex_to_rgba("#f97316", 0.9 * (1.0 - t)));
        ctx.set_line_width(2.5);
        round_rect_path(
            ctx,
            bx - inflate,
            by - inflate,
            bw + inflate * 2.0,
            bh + inflate * 2.0,
            node_card::NODE_BOX_R + 4.0,
        );
        ctx.stroke();
    } else {
        // 未选中态：类型色呼吸边框（透明度随时间起伏）
        let phase = ((now % 2.4) / 2.4 * std::f64::consts::TAU).sin();
        ctx.set_stroke_style_str(&hud_palette::hex_to_rgba(accent, 0.55 + phase * 0.18));
        ctx.set_line_width(2.0);
        round_rect_path(
            ctx,
            bx - 3.0,
            by - 3.0,
            bw + 6.0,
            bh + 6.0,
            node_card::NODE_BOX_R + 3.0,
        );
        ctx.stroke();
    }

    // 卡片主体
    if is_selected {
        ctx.set_shadow_blur(8.0);
        ctx.set_shadow_color("#f97316");
    } else if glow {
        // hover / 拖拽 / 搜索命中的外发光用类型色，与「选中」的橙色描边区分开
        ctx.set_shadow_blur(7.0);
        ctx.set_shadow_color(accent);
    } else {
        ctx.set_shadow_blur(0.0);
    }
    ctx.set_global_alpha(if is_selected || glow { 1.0 } else { 0.94 });
    ctx.set_fill_style_str("rgba(17, 24, 39, 0.94)");
    round_rect_path(ctx, bx, by, bw, bh, node_card::NODE_BOX_R);
    ctx.fill();

    // 绑成变量再传：`if/else` 分支里的临时 String 活不过该表达式，
    // 直接 `&hex_to_rgba(..)` 会被借用检查拦下（E0716）
    let border_color = hud_palette::hex_to_rgba(accent, 0.75);
    ctx.set_stroke_style_str(if is_selected {
        "#f97316"
    } else {
        &border_color
    });
    ctx.set_line_width(if is_selected { 3.0 } else { 1.5 });
    round_rect_path(ctx, bx, by, bw, bh, node_card::NODE_BOX_R);
    ctx.stroke();
    ctx.set_shadow_blur(0.0);
    ctx.set_global_alpha(1.0);

    // 左侧类型色竖条（取代圆形的整体填充，保留类型辨识度）
    ctx.set_fill_style_str(accent);
    round_rect_path(ctx, bx, by, node_card::NODE_ACCENT_W, bh, 2.0);
    ctx.fill();

    // 文案：名称 + 正文 + 标签胶囊
    let text_x = bx + node_card::NODE_ACCENT_W + node_card::NODE_BOX_PAD;
    ctx.set_text_align("left");
    ctx.set_text_baseline("top");
    ctx.set_font(&format!("600 {}px sans-serif", node_card::NODE_TITLE_PX));
    ctx.set_fill_style_str("#f9fafb");
    let _ = ctx.fill_text(
        &node_card::title(&node.label),
        text_x,
        by + node_card::NODE_BOX_PAD,
    );

    let body = card_body_lines(node);
    if !body.is_empty() {
        ctx.set_font(&format!("{}px sans-serif", node_card::NODE_BODY_PX));
        ctx.set_fill_style_str("#9ca3af");
        for (i, line) in body.iter().enumerate() {
            let _ = ctx.fill_text(
                line,
                text_x,
                by + node_card::NODE_BOX_PAD
                    + node_card::NODE_TITLE_H
                    + i as f64 * node_card::NODE_BODY_H,
            );
        }
    }

    let chips = node_card::tag_chips(&node.tags);
    if !chips.is_empty() {
        ctx.set_font("8px sans-serif");
        let tag_y = by
            + node_card::NODE_BOX_PAD
            + node_card::NODE_TITLE_H
            + node_card::NODE_BODY_MAX_LINES as f64 * node_card::NODE_BODY_H
            + 3.0;
        let mut tx = text_x;
        for (text, color) in &chips {
            let w = measure_text_width(ctx, text, 8.0) + 8.0;
            ctx.set_fill_style_str(color);
            round_rect_path(ctx, tx, tag_y, w, 12.0, 6.0);
            ctx.fill();
            ctx.set_text_align("center");
            ctx.set_fill_style_str("#ffffff");
            let _ = ctx.fill_text(text, tx + w / 2.0, tag_y + 2.5);
            ctx.set_text_align("left");
            tx += w + 4.0;
        }
    }
}

/// 绘制圆形节点（只有名称 + 类型的场景）
///
/// ⚠️ 不在圆下直出完整 ID：一屏 ulid 是「图谱看不懂」的主因之一，ID 只留给 hover 详情卡。
fn draw_circle_node(
    ctx: &CanvasRenderingContext2d,
    node: &CanvasNode,
    is_hovered: bool,
    is_selected: bool,
    is_dragging: bool,
) {
    // 选中光晕
    if is_selected {
        ctx.set_fill_style_str("rgba(59, 130, 246, 0.2)");
        ctx.begin_path();
        let _ = ctx.arc(
            node.x,
            node.y,
            node.radius + 8.0,
            0.0,
            std::f64::consts::TAU,
        );
        ctx.fill();
    }

    // 拖拽光晕
    if is_dragging {
        ctx.set_fill_style_str("rgba(245, 158, 11, 0.3)");
        ctx.begin_path();
        let _ = ctx.arc(
            node.x,
            node.y,
            node.radius + 12.0,
            0.0,
            std::f64::consts::TAU,
        );
        ctx.fill();
    }

    let draw_radius = if is_hovered {
        node.radius * 1.15
    } else {
        node.radius
    };

    ctx.set_fill_style_str(&node.color);
    ctx.begin_path();
    let _ = ctx.arc(node.x, node.y, draw_radius, 0.0, std::f64::consts::TAU);
    ctx.fill();

    if is_selected {
        ctx.set_stroke_style_str("#3b82f6");
        ctx.set_line_width(3.0);
        ctx.begin_path();
        let _ = ctx.arc(node.x, node.y, draw_radius, 0.0, std::f64::consts::TAU);
        ctx.stroke();
    }

    // 名称（圆内，按半径自适应字号，过长截断）
    ctx.set_fill_style_str("white");
    let font_size = (draw_radius * 0.42).max(9.0);
    ctx.set_font(&format!("{font_size:.0}px sans-serif"));
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    let max_chars = ((draw_radius * 1.5) as usize).clamp(3, 12);
    let name: String = node.label.chars().take(max_chars).collect();
    let _ = ctx.fill_text(&name, node.x, node.y);
}

/// 默认渲染器：圆形 / 矩形信息卡双形态节点 + 直线连线 + 边 hover 高亮
#[derive(Clone, Copy)]
pub struct DefaultRenderer;

impl CanvasRenderer for DefaultRenderer {
    fn clear(&self, ctx: &CanvasRenderingContext2d, width: f64, height: f64) {
        ctx.clear_rect(0.0, 0.0, width, height);
    }

    fn draw_nodes(&self, ctx: &CanvasRenderingContext2d, nodes: &[CanvasNode]) {
        self.draw_nodes_with_state(ctx, nodes, &None, &None, &None);
    }

    fn draw_edges(
        &self,
        ctx: &CanvasRenderingContext2d,
        edges: &[CanvasEdge],
        nodes: &[CanvasNode],
    ) {
        for edge in edges {
            // 颜色只表达**语义**，不表达强度：
            //   not_ready / ready → 语义色（关系图的「可用性」语义）
            //   其余 tag → 视为关系类型，按 tag 哈希稳定取色（知识图谱的 relation type）
            //   None → 中性灰（对不表达关系的连线零侵入）
            // 强度另走粗细 + 浓淡两个未被占用的通道（`edge_style::weight_style`）
            // 取 String 而不是 &str：`tag_color` 返回临时值，分支里 `.as_str()`
            // 会在 match 结束时就释放（E0716），借用活不过这一行
            let (hex, base_alpha) = match edge.tag.as_deref() {
                Some("not_ready") => ("#f97316".to_string(), 0.9),
                Some("ready") => ("#94a3b8".to_string(), 0.55),
                Some(tag) => (tag_color(tag), 0.5),
                None => ("#6b7280".to_string(), 0.4),
            };
            let (width, alpha_scale) = edge_style::weight_style(edge.weight);
            let color = hud_palette::hex_to_rgba(&hex, (base_alpha * alpha_scale).clamp(0.15, 1.0));
            ctx.set_stroke_style_str(&color);
            ctx.set_line_width(width);
            let from = nodes.iter().find(|n| n.id == edge.from_id);
            let to = nodes.iter().find(|n| n.id == edge.to_id);
            if let (Some(from), Some(to)) = (from, to) {
                ctx.begin_path();
                ctx.move_to(from.x, from.y);
                ctx.line_to(to.x, to.y);
                ctx.stroke();
            }
        }
    }

    fn draw_edges_with_state(
        &self,
        ctx: &CanvasRenderingContext2d,
        edges: &[CanvasEdge],
        nodes: &[CanvasNode],
        hovered_edge: &Option<(String, String)>,
    ) {
        self.draw_edges(ctx, edges, nodes);
        // 被 hover 的边重描一遍（橙色加粗）：详情卡负责「这条是什么关系」，
        // 高亮负责「指的是哪一条」——两者缺一都会让人对不上号
        if let Some((from_id, to_id)) = hovered_edge
            && let (Some(from), Some(to)) = (
                nodes.iter().find(|n| &n.id == from_id),
                nodes.iter().find(|n| &n.id == to_id),
            )
        {
            // 高亮线必须**盖得住**原线：线宽现在随强度变化（最粗 3.8），
            // 固定 3.0 会在粗线上露边，看起来像「高亮后反而细了」
            let (edge_width, _) = edges
                .iter()
                .find(|e| &e.from_id == from_id && &e.to_id == to_id)
                .map(|e| edge_style::weight_style(e.weight))
                .unwrap_or((edge_style::EDGE_BASE_WIDTH, 1.0));
            ctx.set_stroke_style_str("#f97316");
            ctx.set_line_width((edge_width + 1.5).max(3.0));
            ctx.begin_path();
            ctx.move_to(from.x, from.y);
            ctx.line_to(to.x, to.y);
            ctx.stroke();
        }
    }

    fn hit_test(&self, nodes: &[CanvasNode], x: f64, y: f64) -> Option<String> {
        for node in nodes.iter().rev() {
            // 命中形状必须与看到的形状一致：卡片按矩形、圆按半径
            let hit = if node.is_card() {
                let (w, h) = card_size(node);
                (x - node.x).abs() <= w / 2.0 && (y - node.y).abs() <= h / 2.0
            } else {
                let dx = x - node.x;
                let dy = y - node.y;
                dx * dx + dy * dy <= node.radius * node.radius
            };
            if hit {
                return Some(node.id.clone());
            }
        }
        None
    }

    fn draw_nodes_with_state(
        &self,
        ctx: &CanvasRenderingContext2d,
        nodes: &[CanvasNode],
        hovered: &Option<String>,
        selected: &Option<String>,
        dragging: &Option<String>,
    ) {
        for node in nodes {
            let is_hovered = hovered.as_deref() == Some(node.id.as_str());
            let is_selected = selected.as_deref() == Some(node.id.as_str());
            let is_dragging = dragging.as_deref() == Some(node.id.as_str());

            if node.is_card() {
                // hover / 拖拽 / 搜索命中同为「突出」——卡片是唯一的视觉载体，
                // 不像圆形那样还有半径放大可用
                draw_card_node(
                    ctx,
                    node,
                    is_selected,
                    is_hovered || is_dragging || node.highlighted,
                );
            } else {
                draw_circle_node(ctx, node, is_hovered, is_selected, is_dragging);
            }
        }
    }

    fn draw_overlays(
        &self,
        ctx: &CanvasRenderingContext2d,
        nodes: &[CanvasNode],
        hovered: &Option<String>,
        dragging: &Option<String>,
        viewport: Viewport,
    ) {
        // hover 详情卡（名称 / 类型 / 标签 / 摘要 / 描述 / ID）。
        // 拖拽中不弹：卡片跟随鼠标会挡住视线，也没人会边拖边读
        if let Some(hovered_id) = hovered
            && dragging.as_deref() != Some(hovered_id.as_str())
            && let Some(node) = nodes.iter().find(|n| &n.id == hovered_id)
        {
            draw_node_tooltip(ctx, node, viewport);
        }
    }
}

/// 绘制 hover 详情卡：名称 / 类型 / 标签 / 摘要 / 描述 / ID
///
/// **ID 只在这里出现** —— 画布上的卡片不再直出 ID。
///
/// ⚠️ 调用方必须已把 ctx 恢复到**屏幕坐标系**：浮层不参与缩放（放大十倍也不会把
/// 详情卡放大十倍），同时「贴到画布边缘就翻到另一侧」的避让按屏幕边界计算才准。
fn draw_node_tooltip(ctx: &CanvasRenderingContext2d, node: &CanvasNode, viewport: Viewport) {
    let lines = node_card::hover_lines(
        &node.id,
        &node.label,
        node.node_type.as_deref().unwrap_or(""),
        &node.tags,
        node.summary.as_deref(),
        &node.description,
    );
    let (sx, sy) = viewport.world_to_screen(node.x, node.y);
    // 锚点避让节点本体的实际外接宽度（卡片半宽 / 圆半径），随缩放同步变化
    let half = if node.is_card() {
        card_size(node).0 / 2.0
    } else {
        node.radius
    } * viewport.scale;
    draw_hover_card_anchored(ctx, &lines, &node.color, sx, sy, half);
}

/// CanvasScene 组件 Props
#[derive(Props, Clone, PartialEq)]
pub struct CanvasSceneProps {
    /// Canvas 宽度（CSS 像素）
    pub width: f64,
    /// Canvas 高度（CSS 像素）
    pub height: f64,
    /// 节点列表
    pub nodes: Vec<CanvasNode>,
    /// 连线列表
    pub edges: Vec<CanvasEdge>,
    /// 点击节点回调
    pub on_node_click: Option<EventHandler<String>>,
    /// 是否启用力导向布局（默认 true）
    #[props(default = true)]
    pub enable_force_layout: bool,
    /// 是否启用数据流粒子（连线能量流动）
    #[props(default = true)]
    pub enable_data_flow_particles: bool,
    /// 是否启用节点辉光粒子（hover/选中扩散）
    #[props(default = true)]
    pub enable_glow_particles: bool,
    /// 是否启用背景粒子（环境氛围）
    #[props(default = true)]
    pub enable_background_particles: bool,
    /// 是否启用节点诞生/消亡粒子
    #[props(default = true)]
    pub enable_birth_death_particles: bool,
    /// 是否透明背景（HUD 全屏背景模式：去掉边框/圆角/白底，铺满父容器）
    #[props(default = false)]
    pub transparent: bool,
    /// 受控选中节点 ID（None = 不受控，选中态由画布内部点击维护）
    ///
    /// 页面的选中还可能来自详情面板 / 列表，需要外部反向驱动画布 ——
    /// 否则「点了列表，画布上的卡片却不亮」。
    #[props(default)]
    pub selected_node_id: Option<String>,
}

impl Default for CanvasSceneProps {
    fn default() -> Self {
        Self {
            width: 800.0,
            height: 600.0,
            nodes: Vec::new(),
            edges: Vec::new(),
            on_node_click: None,
            enable_force_layout: true,
            enable_data_flow_particles: true,
            enable_glow_particles: true,
            enable_background_particles: true,
            enable_birth_death_particles: true,
            transparent: false,
            selected_node_id: None,
        }
    }
}

/// CanvasScene 组件：封装 <canvas> 元素 + Context 初始化 + 持续渲染循环 + 力导向 + 拖拽 + hover/选中
#[component]
pub fn CanvasScene(props: CanvasSceneProps) -> Element {
    let mut canvas_ref: Signal<Option<HtmlCanvasElement>> = use_signal(|| None);
    let renderer = DefaultRenderer;

    // 内部状态：节点实时位置、力导向模拟器、稳定标志、拖拽/hover/选中
    let mut nodes_state: Signal<Vec<CanvasNode>> = use_signal(|| props.nodes.clone());
    let force_layout: Signal<ForceLayout> =
        use_signal(|| ForceLayout::new(ForceLayoutConfig::default()));
    let mut is_stable: Signal<bool> = use_signal(|| false);
    let mut dragging_id: Signal<Option<String>> = use_signal(|| None);
    let mut drag_offset: Signal<(f64, f64)> = use_signal(|| (0.0, 0.0));
    let mut hovered_id: Signal<Option<String>> = use_signal(|| None);
    let mut selected_id: Signal<Option<String>> = use_signal(|| None);
    // 边 hover：记录命中边的 (from_id, to_id)，用于绘制关系标签/描述提示
    let mut hovered_edge: Signal<Option<(String, String)>> = use_signal(|| None);

    // 视口：滚轮缩放 + 空白处拖拽平移。渲染循环每帧读取，因此改它无需触发组件重渲染。
    let mut viewport: Signal<Viewport> = use_signal(Viewport::default);
    let mut is_panning: Signal<bool> = use_signal(|| false);
    // 上一次鼠标屏幕坐标（平移是增量式的，逐次累加）
    let mut pan_anchor: Signal<(f64, f64)> = use_signal(|| (0.0, 0.0));
    // 本次手势是否真的拖动过：超过阈值才算平移，否则 mouseup 后的 click 仍按「点节点」处理
    let mut pan_moved: Signal<bool> = use_signal(|| false);

    // 粒子系统状态（glow 需 mut 因事件闭包中 .write()，其他仅 clone 使用）
    let data_flow: Signal<DataFlowParticles> = use_signal(DataFlowParticles::new);
    let mut glow: Signal<GlowParticles> = use_signal(GlowParticles::new);
    let background: Signal<BackgroundParticles> =
        use_signal(|| BackgroundParticles::new(props.width, props.height, 40));
    let birth_death: Signal<BirthDeathParticles> = use_signal(BirthDeathParticles::new);

    // --- props 同步 effect：仅在 props 变化时同步（保留已有节点位置，新增节点圆形布局）---
    // 修复死循环：原 use_effect 订阅 nodes_state 并在 effect 内 set nodes_state，
    // 被 RAF 每帧触发后形成无限循环冻结主线程。改用 use_reactive 限定依赖为 props
    // （PartialEq 变化才执行），并用 peek() 读取 nodes_state（不订阅自身）。
    let mut nodes_state_sync = nodes_state;
    let mut force_layout_sync = force_layout;
    let mut is_stable_sync = is_stable;
    let mut birth_death_sync = birth_death;
    use_effect(use_reactive(
        (&props.nodes, &props.width, &props.height),
        move |(props_nodes, sync_width, sync_height)| {
            let current = nodes_state_sync.peek().clone();
            let current_ids: std::collections::HashSet<&str> =
                current.iter().map(|n| n.id.as_str()).collect();
            let new_node_ids: Vec<String> = props_nodes
                .iter()
                .filter(|n| !current_ids.contains(n.id.as_str()))
                .map(|n| n.id.clone())
                .collect();
            let mut merged: Vec<CanvasNode> = Vec::with_capacity(props_nodes.len());
            for new_node in &props_nodes {
                if let Some(existing) = current.iter().find(|n| n.id == new_node.id) {
                    // 保留已有节点位置，其余外观字段整体以新 props 为准。
                    // ⚠️ 用 `..new_node.clone()` 而不是逐个列字段：这份列表历史上漏过一次
                    // （新增卡片字段后 props 一更新就丢，表现为「首帧是卡片、重渲染后塌成圆点」）。
                    merged.push(CanvasNode {
                        x: existing.x,
                        y: existing.y,
                        ..new_node.clone()
                    });
                } else {
                    merged.push(new_node.clone());
                }
            }
            // 新增节点位置为 0,0 时用圆形布局初始化
            let has_uninit = merged.iter().any(|n| n.x == 0.0 && n.y == 0.0);
            if has_uninit {
                let positions = circle_initial_layout(
                    merged.len(),
                    sync_width / 2.0,
                    sync_height / 2.0,
                    (sync_width.min(sync_height) / 3.0).max(100.0),
                );
                for (i, node) in merged.iter_mut().enumerate() {
                    if node.x == 0.0 && node.y == 0.0 {
                        node.x = positions[i].0;
                        node.y = positions[i].1;
                    }
                }
            }
            if current.len() != merged.len() {
                is_stable_sync.set(false);
                force_layout_sync.write().sync(merged.len());
            }
            // 触发新增节点的诞生效果
            if !new_node_ids.is_empty() {
                let mut bd = birth_death_sync.write();
                for id in &new_node_ids {
                    if let Some(node) = merged.iter().find(|n| &n.id == id) {
                        bd.trigger_birth(node);
                    }
                }
            }
            nodes_state_sync.set(merged);
            is_stable_sync.set(false);
        },
    ));

    // --- props 同步 effect：边列表同步进 signal ---
    // ⚠️ 不能让渲染循环直接捕获 props.edges 的快照：RAF 循环是在 effect 里注册的
    // 闭包，捕获的是「effect 当次运行时」的 edges。展开节点后新增的边若走快照，
    // 画布上就会出现「新节点没有连线」——节点走 signal 每帧刷新、边却停在挂载时。
    // 边与节点同构：都同步进 signal，渲染循环每帧读最新值。
    let edges_state: Signal<Vec<CanvasEdge>> = use_signal(|| props.edges.clone());
    let mut edges_state_sync = edges_state;
    use_effect(use_reactive(&props.edges, move |edges| {
        edges_state_sync.set(edges);
    }));

    // --- props 同步 effect：受控选中 ---
    // 只在外部值**真的变化**时写入（use_reactive 语义），所以页面不传时
    // 画布内部的点击选中态不会被 None 冲掉。
    let mut selected_id_sync = selected_id;
    use_effect(use_reactive(&props.selected_node_id, move |sel| {
        selected_id_sync.set(sel);
    }));

    // 渲染循环 effect：request_animation_frame 递归调用，每帧步进力学 + 重绘
    let render_width = props.width;
    let render_height = props.height;
    let enable_force = props.enable_force_layout;
    // 供鼠标事件做边命中检测（与渲染同源，每帧从 signal 读取）
    let edges_static = edges_state;
    let mut nodes_state_c = nodes_state;
    let edges_state_c = edges_state;
    let mut force_layout_c = force_layout;
    let mut is_stable_c = is_stable;
    let dragging_id_c = dragging_id;
    let hovered_id_c = hovered_id;
    let selected_id_c = selected_id;
    let hovered_edge_c = hovered_edge;
    let renderer_c = renderer;
    let viewport_c = viewport;
    // RAF 渲染循环资源：保存 running flag + Closure + pending 帧句柄供顶层 use_drop 清理
    #[allow(clippy::type_complexity)]
    struct RafResource {
        running: std::sync::Arc<std::sync::atomic::AtomicBool>,
        callback_ref: Rc<RefCell<Option<Closure<dyn FnMut()>>>>,
        // 当前「已注册未触发」的 rAF 帧句柄（0 表示无）。卸载时必须先 cancel 再释放
        // Closure，否则浏览器下一帧会调用已 drop 的 Closure，抛出
        // 「closure invoked recursively or after being dropped」
        pending_frame: Rc<std::cell::Cell<i32>>,
    }
    let mut raf_resource = use_signal(|| Option::<RafResource>::None);

    use_effect(move || {
        // 防御：effect 重跑时先停掉旧渲染循环（取消 pending 帧 + 释放旧 Closure），避免双循环
        if let Some(old) = raf_resource.take() {
            old.running
                .store(false, std::sync::atomic::Ordering::SeqCst);
            if let Some(window) = web_sys::window() {
                let old_id = old.pending_frame.get();
                if old_id > 0 {
                    let _ = window.cancel_animation_frame(old_id);
                }
            }
            *old.callback_ref.borrow_mut() = None;
        }
        let Some(canvas) = canvas_ref.read().clone() else {
            return;
        };
        let ctx = canvas
            .get_context("2d")
            .ok()
            .flatten()
            .and_then(|c| c.dyn_into::<CanvasRenderingContext2d>().ok());
        let Some(ctx) = ctx else {
            return;
        };

        // 高清屏适配：物理像素 = CSS 像素 * devicePixelRatio
        let dpr = web_sys::window()
            .map(|w| w.device_pixel_ratio())
            .unwrap_or(1.0);
        // 实测 canvas 真实显示尺寸（CSS px）。固定属性尺寸在 HiDPI 下会按
        // width*dpr 显示导致溢出容器/被裁切，且 buffer 与显示尺寸不匹配会模糊。
        // 改为按实际布局尺寸设置缓冲：既铺满容器又保持清晰。
        let rect = canvas.get_bounding_client_rect();
        let css_w = if rect.width() > 0.0 {
            rect.width()
        } else {
            render_width
        };
        let css_h = if rect.height() > 0.0 {
            rect.height()
        } else {
            render_height
        };
        canvas.set_width((css_w * dpr) as u32);
        canvas.set_height((css_h * dpr) as u32);
        let _ = ctx.scale(dpr, dpr);

        let width = css_w;
        let height = css_h;
        // 边每帧从 signal 取（见上方 edges_state 注释：捕获快照会导致新增边不渲染）

        // running 标志：组件卸载时设为 false，停止递归 rAF
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let running_clone = running.clone();

        // Rc<RefCell<Option<Closure>>> 模式：Closure 自引用递归 rAF
        #[allow(clippy::type_complexity)]
        let callback_ref: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
        let cb_ref_inner = callback_ref.clone();

        // 当前已注册未触发的 rAF 帧句柄：卸载时凭此 cancel，防止已 drop 的 Closure 仍被浏览器调用
        let pending_frame = Rc::new(std::cell::Cell::new(0i32));
        let pending_frame_inner = pending_frame.clone();

        let mut data_flow_c = data_flow;
        let mut glow_c = glow;
        let mut background_c = background;
        let mut birth_death_c = birth_death;
        let enable_data_flow = props.enable_data_flow_particles;
        let enable_glow = props.enable_glow_particles;
        let enable_bg = props.enable_background_particles;
        let enable_bd = props.enable_birth_death_particles;

        let closure = Closure::<dyn FnMut()>::new(move || {
            // 组件卸载后仍可能被「已调度未取消」的帧触发一次：直接早退，不渲染不写信号
            if !running_clone.load(std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            // 每帧取最新边列表（新增/删除的边当帧即生效）
            let edges_inner = edges_state_c.read().clone();
            // 力导向步进
            if enable_force && !*is_stable_c.read() {
                let mut nodes = nodes_state_c.read().clone();
                let mut layout = force_layout_c.write();
                let displacement = layout.step(&mut nodes, &edges_inner, width, height);
                nodes_state_c.set(nodes);
                if layout.is_stable(displacement, 0.5) {
                    is_stable_c.set(true);
                }
            }

            // 粒子更新（dt 约为 1/60 秒）
            let dt = 1.0 / 60.0;
            if enable_bg {
                background_c.write().update(dt);
            }
            if enable_data_flow {
                let nodes = nodes_state_c.read().clone();
                data_flow_c.write().spawn(&edges_inner, &nodes, dt);
                data_flow_c.write().update(dt);
            }
            if enable_glow {
                glow_c.write().update(dt);
            }
            if enable_bd {
                birth_death_c.write().update(dt);
            }

            // 渲染
            let nodes = nodes_state_c.read().clone();
            let hovered = hovered_id_c.read().clone();
            let selected = selected_id_c.read().clone();
            let dragging = dragging_id_c.read().clone();
            let vp = *viewport_c.read();

            renderer_c.clear(&ctx, width, height);

            // 1. 背景粒子（最底层）：留在屏幕空间，缩放时氛围密度不跟着变
            if enable_bg {
                background_c.read().draw(&ctx);
            }

            // 以下内容全部活在世界坐标里：整体套一次视口变换即可。
            // 力导向、拖拽落点、命中检测的坐标语义都不变，各自无需感知视口。
            ctx.save();
            let _ = ctx.translate(vp.pan_x, vp.pan_y);
            let _ = ctx.scale(vp.scale, vp.scale);

            // 2. 连线（携带边 hover 状态，供自定义渲染器绘制边详情卡片）
            renderer_c.draw_edges_with_state(
                &ctx,
                &edges_inner,
                &nodes,
                &hovered_edge_c.read().clone(),
            );

            // 3. 数据流粒子（在连线上方，节点下方）
            if enable_data_flow {
                data_flow_c.read().draw(&ctx);
            }

            // 4. 节点辉光粒子（节点周围扩散）
            if enable_glow {
                glow_c.read().draw(&ctx);
            }

            // 5. 节点
            renderer_c.draw_nodes_with_state(&ctx, &nodes, &hovered, &selected, &dragging);

            // 6. 诞生/消亡粒子（在节点层之上，比节点醒目）
            if enable_bd {
                birth_death_c.read().draw(&ctx);
            }

            ctx.restore();

            // 7. 浮层（屏幕空间，不参与缩放/平移）：hover 详情卡与边提示。
            //    放在变换之外，放大十倍也不会让详情卡变成十倍大的遮挡物，
            //    且「贴边翻到另一侧」的避让按真正的屏幕边界计算。
            renderer_c.draw_overlays(&ctx, &nodes, &hovered, &dragging, vp);

            // 7.5 边 hover 提示（展示关系标签/描述），锚点换算到屏幕坐标
            if let Some((ef, et)) = hovered_edge_c.read().clone()
                && let Some(edge) = edges_inner
                    .iter()
                    .find(|e| e.from_id == ef && e.to_id == et)
                && let (Some(f), Some(t)) = (
                    nodes.iter().find(|n| n.id == ef),
                    nodes.iter().find(|n| n.id == et),
                )
            {
                let (fx, fy) = vp.world_to_screen(f.x, f.y);
                let (tx, ty) = vp.world_to_screen(t.x, t.y);
                draw_edge_tooltip(&ctx, (fx + tx) / 2.0, (fy + ty) / 2.0, edge);
            }

            // 递归注册下一帧
            if running_clone.load(std::sync::atomic::Ordering::SeqCst)
                && let Some(cb) = cb_ref_inner.borrow().as_ref()
                && let Some(window) = web_sys::window()
                && let Ok(id) = window.request_animation_frame(cb.as_ref().unchecked_ref())
            {
                pending_frame_inner.set(id);
            }
        });

        // 初始注册第一帧
        if let Some(window) = web_sys::window()
            && let Ok(id) = window.request_animation_frame(closure.as_ref().unchecked_ref())
        {
            pending_frame.set(id);
        }
        *callback_ref.borrow_mut() = Some(closure);

        raf_resource.set(Some(RafResource {
            running,
            callback_ref,
            pending_frame,
        }));
    });

    use_drop(move || {
        if let Some(res) = raf_resource.take() {
            res.running
                .store(false, std::sync::atomic::Ordering::SeqCst);
            // 关键：先取消「已注册未触发」的 rAF 帧，再释放 Closure。
            // 否则浏览器下一帧调用已 drop 的 Closure，抛出
            // 「closure invoked recursively or after being dropped」。
            if let Some(window) = web_sys::window() {
                let id = res.pending_frame.get();
                if id > 0 {
                    let _ = window.cancel_animation_frame(id);
                }
            }
            *res.callback_ref.borrow_mut() = None;
        }
    });

    // 提取 onclick 所需字段
    let on_node_click = props.on_node_click;

    rsx! {
        // 包裹层只做两件事：给 canvas 一个定位上下文，承载视口控制浮层。
        // canvas 自身的 100% 尺寸语义不变，宿主容器的高度仍然决定画布大小。
        div { class: "relative w-full h-full",
        canvas {
            width: "{props.width}",
            height: "{props.height}",
            style: if props.transparent {
                "width: 100%; height: 100%; display: block; background: transparent; cursor: grab;"
            } else {
                // 关键修复：非透明（卡片）模式也必须 width/height:100%，否则 canvas 按
                // 属性尺寸（width*dpr）显示，在 HiDPI 下溢出容器并被裁切/撑破布局。
                "width: 100%; height: 100%; display: block; border: 1px solid #e5e7eb; border-radius: 8px; background: #fafafa; cursor: grab;"
            },
            onwheel: move |e: WheelEvent| {
                // 阻止默认滚动：否则滚轮既缩放画布、又滚动整页
                e.prevent_default();
                let Some(canvas) = canvas_ref.read().clone() else { return; };
                let rect = canvas.get_bounding_client_rect();
                let coords = e.client_coordinates();
                let x = coords.x - rect.left();
                let y = coords.y - rect.top();
                // 以光标为锚点缩放：想看哪里就放大哪里（锚点下的内容不会跑掉）
                let factor = if e.delta().strip_units().y > 0.0 {
                    0.9
                } else {
                    1.1
                };
                let next = viewport.read().zoomed_at(factor, x, y);
                viewport.set(next);
            },
            onmounted: move |evt: MountedEvent| {
                let data = evt.data();
                if let Some(element) = data.downcast::<web_sys::Element>() {
                    let canvas = element.clone().unchecked_into::<HtmlCanvasElement>();
                    canvas_ref.set(Some(canvas));
                }
            },
            onmousedown: move |e: MouseEvent| {
                let Some(canvas) = canvas_ref.read().clone() else { return; };
                let rect = canvas.get_bounding_client_rect();
                let coords = e.client_coordinates();
                let x = coords.x - rect.left();
                let y = coords.y - rect.top();
                hovered_edge.set(None);
                let nodes = nodes_state.read().clone();
                // 命中和拖拽偏移都在世界坐标里算：屏幕坐标先反变换一次
                let (wx, wy) = viewport.read().screen_to_world(x, y);
                if let Some(node_id) = renderer.hit_test(&nodes, wx, wy) {
                    dragging_id.set(Some(node_id.clone()));
                    is_stable.set(false);
                    if let Some(node) = nodes.iter().find(|n| n.id == node_id) {
                        drag_offset.set((wx - node.x, wy - node.y));
                        // 触发拖拽开始时的辉光
                        glow.write().trigger(node);
                    }
                } else {
                    // 空白处按下 = 开始平移视图（与 SVG 版一致：拖背景即移动画布）
                    is_panning.set(true);
                    pan_moved.set(false);
                    pan_anchor.set((x, y));
                }
            },
            onmousemove: move |e: MouseEvent| {
                let Some(canvas) = canvas_ref.read().clone() else { return; };
                let rect = canvas.get_bounding_client_rect();
                let coords = e.client_coordinates();
                let x = coords.x - rect.left();
                let y = coords.y - rect.top();

                // 平移中：屏幕增量直接累加进 pan（pan 本身就在屏幕空间，无需除以 scale）
                if is_panning() {
                    let (ax, ay) = *pan_anchor.read();
                    let (dx, dy) = (x - ax, y - ay);
                    // >0.5px 才算拖动：否则一次「点空白」也会记成平移，把随后的点击吞掉
                    if dx.abs() > 0.5 || dy.abs() > 0.5 {
                        pan_moved.set(true);
                        let next = viewport.read().panned(dx, dy);
                        viewport.set(next);
                    }
                    pan_anchor.set((x, y));
                    return;
                }

                let vp = *viewport.read();
                let (wx, wy) = vp.screen_to_world(x, y);
                let dragging = dragging_id.read().clone();
                if let Some(drag_id) = &dragging {
                    // 拖拽中：更新节点位置
                    let offset = *drag_offset.read();
                    let mut nodes = nodes_state.read().clone();
                    if let Some(node) = nodes.iter_mut().find(|n| &n.id == drag_id) {
                        node.x = wx - offset.0;
                        node.y = wy - offset.1;
                    }
                    nodes_state.set(nodes);
                    is_stable.set(false);
                } else {
                    // 非拖拽：更新 hover 状态
                    let nodes = nodes_state.read().clone();
                    let new_hovered = renderer.hit_test(&nodes, wx, wy);
                    let current_hovered = hovered_id.read().clone();
                    if new_hovered != current_hovered {
                        hovered_id.set(new_hovered.clone());
                    }
                    // 未命中节点时做边命中检测（展示边的关系标签/描述）
                    // 阈值是**屏幕** 10px：细线（1.5~2px）用 6px 几乎点不中，用户会以为
                    // 「连线不能 hover」；世界坐标下按 scale 换算，缩小后照样点得中。
                    if new_hovered.is_none() {
                        let he = nearest_edge(
                            &edges_static.read().clone(),
                            &nodes,
                            wx,
                            wy,
                            10.0 / vp.scale,
                        );
                        if hovered_edge.read().clone() != he {
                            hovered_edge.set(he);
                        }
                    } else if hovered_edge.read().is_some() {
                        hovered_edge.set(None);
                    }
                }
            },
            onmouseup: move |_| {
                dragging_id.set(None);
                is_panning.set(false);
            },
            onmouseleave: move |_| {
                dragging_id.set(None);
                hovered_id.set(None);
                hovered_edge.set(None);
                // 指针移出画布即结束平移，否则回到画布时会「幽灵跟随」鼠标
                is_panning.set(false);
            },
            onclick: move |e: MouseEvent| {
                // 刚刚拖拽平移过：这次 click 只是拖动的尾巴，不作为「点节点」处理
                if pan_moved() {
                    pan_moved.set(false);
                    return;
                }
                let Some(canvas) = canvas_ref.read().clone() else { return; };
                let Some(on_click) = on_node_click.as_ref() else { return; };
                let rect = canvas.get_bounding_client_rect();
                let coords = e.client_coordinates();
                let x = coords.x - rect.left();
                let y = coords.y - rect.top();
                let nodes = nodes_state.read().clone();
                let (wx, wy) = viewport.read().screen_to_world(x, y);
                if let Some(node_id) = renderer.hit_test(&nodes, wx, wy) {
                    selected_id.set(Some(node_id.clone()));
                    // 触发选中时的辉光
                    if let Some(node) = nodes.iter().find(|n| n.id == node_id) {
                        glow.write().reset_trigger();
                        glow.write().trigger(node);
                    }
                    on_click.call(node_id);
                }
            },
        }
        // 视口控制浮层：固定在画布右下角，不随画布变换
        div { class: "absolute bottom-2 right-2 z-10 flex items-center gap-1",
            ZoomBadge { viewport }
            Button {
                variant: ButtonVariant::Ghost,
                small: true,
                title: "缩小",
                onclick: move |_| {
                    let (cx, cy) = canvas_display_size(&canvas_ref);
                    let next = viewport.read().zoomed_at(0.85, cx / 2.0, cy / 2.0);
                    viewport.set(next);
                },
                "−"
            }
            Button {
                variant: ButtonVariant::Ghost,
                small: true,
                title: "放大",
                onclick: move |_| {
                    let (cx, cy) = canvas_display_size(&canvas_ref);
                    let next = viewport.read().zoomed_at(1.18, cx / 2.0, cy / 2.0);
                    viewport.set(next);
                },
                "+"
            }
            Button {
                variant: ButtonVariant::Ghost,
                small: true,
                title: "适应内容",
                onclick: move |_| {
                    let (w, h) = canvas_display_size(&canvas_ref);
                    let nodes = nodes_state.read().clone();
                    viewport.set(fit_viewport(&nodes, w, h));
                },
                "适应"
            }
            Button {
                variant: ButtonVariant::Ghost,
                small: true,
                title: "重置视图",
                onclick: move |_| viewport.set(Viewport::default()),
                "重置"
            }
        }
        }
    }
}

/// 画布当前显示尺寸（CSS 像素）：视口按钮的缩放锚点与「适应内容」的范围都以它为准
///
/// 取不到（未挂载 / 尺寸为 0）时回退到默认尺寸，保证按钮始终有确定行为。
fn canvas_display_size(canvas_ref: &Signal<Option<HtmlCanvasElement>>) -> (f64, f64) {
    canvas_ref
        .read()
        .as_ref()
        .map(|c| {
            let r = c.get_bounding_client_rect();
            (r.width(), r.height())
        })
        .filter(|(w, h)| *w > 0.0 && *h > 0.0)
        .unwrap_or((800.0, 600.0))
}

/// 缩放百分比徽标
///
/// 独立成组件只为隔离重渲染：读数随视口变化，若写在 CanvasScene 的 rsx 里，
/// 每次滚轮缩放都会连带重渲染整个画布组件（canvas 属性重设 + 事件闭包重建）。
#[component]
fn ZoomBadge(viewport: Signal<Viewport>) -> Element {
    let percent = (viewport.read().scale * 100.0).round() as i32;
    rsx! {
        span { class: "text-xs text-base-content/60 tabular-nums select-none", "{percent}%" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(label: &str) -> CanvasNode {
        CanvasNode {
            label: label.to_string(),
            ..Default::default()
        }
    }

    /// 形态判定的唯一入口：有正文才是卡片，纯「名称 + 类型」保持圆形。
    ///
    /// 三处图谱靠它共用一个渲染器（工作台 / Agent 关系图 = 圆形，知识图谱 = 卡片）。
    /// 判定一旦放宽，工作台和关系图会凭空多出一堆空卡片。
    #[test]
    fn is_card_requires_body_content() {
        let mut plain = node("工具");
        plain.node_type = Some("bound_tool".to_string());
        assert!(!plain.is_card(), "纯名称节点不应渲染成卡片");

        let mut with_desc = node("工具");
        with_desc.description = "一段说明".to_string();
        assert!(with_desc.is_card(), "有描述应是卡片");

        let mut with_summary = node("工具");
        with_summary.summary = Some("摘要".to_string());
        assert!(with_summary.is_card(), "有摘要应是卡片");

        let mut with_tags = node("工具");
        with_tags.tags = vec!["memory".to_string()];
        assert!(with_tags.is_card(), "有标签应是卡片");
    }

    /// 全空白等同「没有正文」：否则 `description = "   "` 会撑出一张空卡片
    #[test]
    fn is_card_ignores_blank_body() {
        let mut blank = node("工具");
        blank.description = "   \n  ".to_string();
        blank.summary = Some("  ".to_string());
        assert!(!blank.is_card());
    }

    /// 卡片尺寸按内容收窄，且宽高为正（力导向的碰撞半径依赖它）
    #[test]
    fn card_size_is_content_driven() {
        let mut full = node("知识节点名称");
        full.description = "一段足够长的描述文本，用来撑满两行正文区".to_string();
        let (w, h) = card_size(&full);
        assert!(w > 0.0 && h > 0.0);
        assert!(w <= crate::components::node_card::NODE_BOX_W + 1e-6);

        let plain = node("短名");
        let (w_plain, h_plain) = card_size(&plain);
        assert!(w_plain < w, "纯名称卡片应比带正文的窄");
        assert!(h_plain < h, "纯名称卡片应比带正文的矮");
    }

    // ---------- 视口 ----------

    /// 以光标为锚点缩放：锚点下的世界坐标必须原地不动
    ///
    /// 这是「滚轮缩放好用」的全部内容。只改 scale 不动 pan 时，画面绕左上角缩放，
    /// 想看的节点会被推走 —— 而这个错误在肉眼上看只是「有点怪」，很难归因。
    #[test]
    fn zoom_keeps_anchor_world_point() {
        let vp = Viewport {
            scale: 1.0,
            pan_x: 30.0,
            pan_y: -12.0,
        };
        let (ax, ay) = (410.0, 260.0);
        let before = vp.screen_to_world(ax, ay);
        let zoomed = vp.zoomed_at(1.1, ax, ay);
        let after = zoomed.screen_to_world(ax, ay);
        assert!((before.0 - after.0).abs() < 1e-9, "锚点世界 x 漂移");
        assert!((before.1 - after.1).abs() < 1e-9, "锚点世界 y 漂移");
        assert!(zoomed.scale > vp.scale, "放大应让 scale 变大");
    }

    /// 缩放夹在上下限内：连续滚轮不会把画布缩成一点或放到无限大
    #[test]
    fn zoom_is_clamped() {
        let mut bigger = Viewport::default();
        for _ in 0..50 {
            bigger = bigger.zoomed_at(1.1, 100.0, 100.0);
        }
        assert!((bigger.scale - Viewport::MAX_SCALE).abs() < 1e-9);

        let mut smaller = Viewport::default();
        for _ in 0..50 {
            smaller = smaller.zoomed_at(0.9, 100.0, 100.0);
        }
        assert!((smaller.scale - Viewport::MIN_SCALE).abs() < 1e-9);
    }

    /// 世界 ↔ 屏幕往返一致：命中检测靠它，错一点就会「点不中卡片」
    #[test]
    fn viewport_roundtrip() {
        let vp = Viewport {
            scale: 1.7,
            pan_x: -240.0,
            pan_y: 88.0,
        };
        let (sx, sy) = vp.world_to_screen(321.0, -45.0);
        let (wx, wy) = vp.screen_to_world(sx, sy);
        assert!((wx - 321.0).abs() < 1e-9, "x 往返不一致");
        assert!((wy + 45.0).abs() < 1e-9, "y 往返不一致");
    }

    /// 平移只动 pan，不动 scale（拖背景不该意外改变缩放）
    #[test]
    fn pan_keeps_scale() {
        let vp = Viewport {
            scale: 1.3,
            pan_x: 10.0,
            pan_y: 20.0,
        };
        let moved = vp.panned(-15.0, 7.0);
        assert!((moved.scale - 1.3).abs() < 1e-9, "平移不应改变 scale");
        assert!((moved.pan_x + 5.0).abs() < 1e-9, "pan_x 应累加位移");
        assert!((moved.pan_y - 27.0).abs() < 1e-9, "pan_y 应累加位移");
    }

    /// 适应内容：所有节点落进画布，且内容中心对齐画布中心
    #[test]
    fn fit_viewport_contains_all_nodes() {
        let mut a = node("A");
        a.x = 1000.0;
        a.y = 800.0;
        a.radius = 30.0;
        let mut b = node("B");
        b.x = 1400.0;
        b.y = 1200.0;
        b.radius = 30.0;
        let nodes = vec![a, b];

        let (w, h) = (800.0, 600.0);
        let vp = fit_viewport(&nodes, w, h);
        for n in &nodes {
            let (sx, sy) = vp.world_to_screen(n.x, n.y);
            assert!(sx >= 0.0 && sx <= w, "节点跑出画布宽度：{sx}");
            assert!(sy >= 0.0 && sy <= h, "节点跑出画布高度：{sy}");
        }
        // 内容包围盒中心 (1200, 1000) 应落在画布正中
        let (cx, cy) = vp.world_to_screen(1200.0, 1000.0);
        assert!((cx - w / 2.0).abs() < 1e-6, "内容未水平居中");
        assert!((cy - h / 2.0).abs() < 1e-6, "内容未垂直居中");
    }

    /// 空节点集 / 非法尺寸退化为默认视口，不 panic
    #[test]
    fn fit_viewport_handles_empty() {
        assert_eq!(fit_viewport(&[], 800.0, 600.0), Viewport::default());
        let single = vec![node("A")];
        assert_eq!(fit_viewport(&single, 0.0, 0.0), Viewport::default());
    }
}
