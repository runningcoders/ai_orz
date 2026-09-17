//! 知识图谱 Canvas 渲染器（HUD 驾驶舱风格）
//!
//! 基于 CanvasScene 基础设施，实现自定义 CanvasRenderer：
//! - 深色径向渐变背景 + 淡橙色网格 + 四角 HUD 装饰
//! - 节点：**矩形信息卡**（左侧类型色竖条 + 名称 + 摘要/描述两行 + 标签胶囊），
//!   取代原先只能塞 10 个字的圆形；ID 不在卡片上出现，只进 hover 详情
//! - 边：实线边流光（lineDashOffset 动画）+ drop-shadow 发光
//! - 选中态：外扩扫描框 + 旋转虚线框；未选中态：呼吸框
//!
//! 与 SVG 版 Graph 组件功能对等，作为高级渲染模式。
//! SVG 版保留作为兜底方案（节点数少或 canvas 不可用时）。
//!
//! ⚠️ 卡片几何（宽高/折行/截断）全部取自 [`crate::components::graph`] 的 SSOT：
//! 本文件不得自算卡片尺寸，否则与 SVG 版漂移（改了一边另一边文字溢出/命中错位）。

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use dioxus::prelude::*;
use wasm_bindgen::JsValue;
use web_sys::CanvasRenderingContext2d;

use crate::components::canvas_scene::{
    CanvasEdge, CanvasNode, CanvasRenderer, CanvasScene, draw_hover_card, hover_card_size,
    measure_text_width,
};
use crate::components::graph::{
    GraphEdge, GraphNode, NODE_ACCENT_W, NODE_BODY_H, NODE_BODY_MAX_LINES, NODE_BODY_PX,
    NODE_BOX_PAD, NODE_BOX_R, NODE_BOX_W, NODE_TITLE_H, NODE_TITLE_PX, get_edge_color,
    get_edge_dash, get_node_fill, node_body_lines, node_box_height, node_hover_lines,
    node_tag_chips, node_title,
};

/// 辅助：将 f64 切片转为 JsValue 数组供 set_line_dash 使用
fn dash_array(values: &[f64]) -> JsValue {
    let arr = js_sys::Array::new();
    for v in values {
        arr.push(&JsValue::from_f64(*v));
    }
    JsValue::from(arr)
}

/// 辅助：设置虚线样式（set_line_dash 返回 Result，统一忽略）
fn set_dash(ctx: &CanvasRenderingContext2d, values: &[f64]) {
    let _ = ctx.set_line_dash(&dash_array(values));
}

/// 辅助：圆角矩形路径（卡片本体 / 外扩 HUD 框共用，避免各处手写 round_rect）
fn round_rect_path(ctx: &CanvasRenderingContext2d, x: f64, y: f64, w: f64, h: f64, r: f64) {
    ctx.begin_path();
    let _ = ctx.round_rect_with_f64(x, y, w, h, r);
}

/// 绘制统一 hover 卡片并做画布内避让：默认锚点右侧垂直居中，
/// 超出右缘放左侧，纵向夹在画布内（与 SVG 版行为对齐）
fn draw_hover_card_anchored(
    ctx: &CanvasRenderingContext2d,
    lines: &[String],
    accent: &str,
    anchor_x: f64,
    anchor_y: f64,
    offset: f64,
) {
    let (w, h) = hover_card_size(ctx, lines);
    // ctx 已按 DPR 缩放，逻辑坐标即 CSS px，可直接用 canvas 的 client 尺寸做边界
    let (canvas_w, canvas_h) = ctx
        .canvas()
        .map(|c| (c.client_width() as f64, c.client_height() as f64))
        .unwrap_or((f64::MAX, f64::MAX));
    let mut bx = anchor_x + offset;
    if bx + w > canvas_w - 8.0 {
        bx = anchor_x - offset - w;
    }
    let bx = bx.max(8.0);
    let by = (anchor_y - h / 2.0).clamp(8.0, (canvas_h - h - 8.0).max(8.0));
    draw_hover_card(ctx, bx, by, lines, accent);
}

/// HUD 风格知识图谱渲染器
///
/// 持有外部传入的高亮/选中状态和节点/边元数据，
/// 渲染时读取这些状态绘制 HUD 效果。
pub struct KnowledgeGraphRenderer {
    /// 高亮节点 ID 集合（搜索匹配结果）
    highlighted: RefCell<HashSet<String>>,
    /// 选中节点 ID（外部控制，优先于 CanvasScene 内部 selected）
    selected: RefCell<Option<String>>,
    /// 边 label 映射：(from_id, to_id) -> relation_type
    edge_labels: RefCell<HashMap<(String, String), String>>,
    /// 节点全量数据：id -> GraphNode（卡片文案 + 几何都从这里取，
    /// 不再复制一份 NodeMeta —— 复制结构必然与页面侧漂移）
    node_index: RefCell<HashMap<String, GraphNode>>,
    /// 边 hover 暂存：边卡需在节点绘制完成后置顶补绘（避免被节点遮挡）
    pending_edge_card: RefCell<Option<(String, String)>>,
}

impl KnowledgeGraphRenderer {
    pub fn new() -> Self {
        Self {
            highlighted: RefCell::new(HashSet::new()),
            selected: RefCell::new(None),
            edge_labels: RefCell::new(HashMap::new()),
            node_index: RefCell::new(HashMap::new()),
            pending_edge_card: RefCell::new(None),
        }
    }

    /// 同步外部状态到渲染器
    pub fn sync_state(
        &self,
        highlighted: HashSet<String>,
        selected: Option<String>,
        edge_labels: HashMap<(String, String), String>,
        node_index: HashMap<String, GraphNode>,
    ) {
        *self.highlighted.borrow_mut() = highlighted;
        *self.selected.borrow_mut() = selected;
        *self.edge_labels.borrow_mut() = edge_labels;
        *self.node_index.borrow_mut() = node_index;
    }

    /// 卡片尺寸（含兜底：拿不到节点数据时退化为以 radius 为边长的方块）
    fn box_size(&self, node: &CanvasNode) -> (f64, f64) {
        match self.node_index.borrow().get(&node.id) {
            Some(gn) => (NODE_BOX_W, node_box_height(gn)),
            None => (node.radius * 2.0, node.radius * 2.0),
        }
    }

    /// 当前时间戳（秒），用于动画
    fn now_secs() -> f64 {
        js_sys::Date::now() / 1000.0
    }
}

impl Default for KnowledgeGraphRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl CanvasRenderer for KnowledgeGraphRenderer {
    fn clear(&self, ctx: &CanvasRenderingContext2d, width: f64, height: f64) {
        crate::components::hud_palette::draw_hud_background(ctx, width, height);
    }

    fn draw_nodes(&self, ctx: &CanvasRenderingContext2d, nodes: &[CanvasNode]) {
        // 委托给 draw_nodes_with_state，传入空状态
        self.draw_nodes_with_state(ctx, nodes, &None, &None, &None);
    }

    fn draw_edges(
        &self,
        ctx: &CanvasRenderingContext2d,
        edges: &[CanvasEdge],
        nodes: &[CanvasNode],
    ) {
        let now = Self::now_secs();
        let edge_labels = self.edge_labels.borrow();
        let selected = self.selected.borrow().clone();

        for edge in edges {
            let from = nodes.iter().find(|n| n.id == edge.from_id);
            let to = nodes.iter().find(|n| n.id == edge.to_id);
            if let (Some(from), Some(to)) = (from, to) {
                let key = (edge.from_id.clone(), edge.to_id.clone());
                let relation_type = edge_labels.get(&key).map(|s| s.as_str()).unwrap_or("");
                let color = get_edge_color(relation_type);
                let is_dashed = get_edge_dash(relation_type) != "none";

                // 选中节点的关联边流光加速
                let is_connected_to_selected =
                    selected.as_deref() == Some(&from.id) || selected.as_deref() == Some(&to.id);

                // 边发光
                ctx.set_shadow_blur(3.0);
                ctx.set_shadow_color(color);
                ctx.set_stroke_style_str(color);
                ctx.set_line_width(2.0);

                if is_dashed {
                    set_dash(ctx, &[5.0, 5.0]);
                    ctx.set_line_dash_offset(0.0);
                } else {
                    // 实线边流光：dashoffset 持续滚动
                    set_dash(ctx, &[4.0, 8.0]);
                    let speed = if is_connected_to_selected { 80.0 } else { 40.0 };
                    let offset = (now * speed) % 12.0;
                    ctx.set_line_dash_offset(-offset);
                }

                ctx.begin_path();
                ctx.move_to(from.x, from.y);
                ctx.line_to(to.x, to.y);
                ctx.stroke();

                // 边标签（关系类型）——与节点标签区分：小号浅字 + 关系色描边徽章，视觉降权
                if !relation_type.is_empty() {
                    let mid_x = (from.x + to.x) / 2.0;
                    let mid_y = (from.y + to.y) / 2.0 - 8.0;
                    let label: String = relation_type.chars().take(10).collect();
                    ctx.set_font("9px sans-serif");
                    let label_w = measure_text_width(ctx, &label, 9.0) + 4.0;

                    ctx.set_shadow_blur(0.0);
                    ctx.set_fill_style_str("rgba(17, 24, 39, 0.82)");
                    round_rect_path(ctx, mid_x - label_w / 2.0, mid_y - 6.0, label_w, 12.0, 2.0);
                    ctx.fill();
                    ctx.set_stroke_style_str(color);
                    ctx.set_line_width(1.0);
                    set_dash(ctx, &[]);
                    ctx.stroke();

                    ctx.set_fill_style_str("#d1d5db");
                    ctx.set_text_align("center");
                    ctx.set_text_baseline("middle");
                    let _ = ctx.fill_text(&label, mid_x, mid_y);
                }

                // 重置 shadow 避免影响后续绘制
                ctx.set_shadow_blur(0.0);
            }
        }
        set_dash(ctx, &[]);
    }

    /// 带交互状态的连线渲染：绘制边后暂存 hover 目标，
    /// 边卡延迟到节点绘制完成后置顶补绘（避免被节点遮挡）
    fn draw_edges_with_state(
        &self,
        ctx: &CanvasRenderingContext2d,
        edges: &[CanvasEdge],
        nodes: &[CanvasNode],
        hovered_edge: &Option<(String, String)>,
    ) {
        self.draw_edges(ctx, edges, nodes);
        *self.pending_edge_card.borrow_mut() = hovered_edge.clone();
    }

    /// 矩形卡片命中检测：圆形时代的「半径圆内」对矩形卡片会漏掉四角、
    /// 又会在卡片外的空白误命中，必须与绘制几何同源
    fn hit_test(&self, nodes: &[CanvasNode], x: f64, y: f64) -> Option<String> {
        for node in nodes.iter().rev() {
            let (w, h) = self.box_size(node);
            if (x - node.x).abs() <= w / 2.0 && (y - node.y).abs() <= h / 2.0 {
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
        let now = Self::now_secs();
        let highlighted = self.highlighted.borrow();
        let external_selected = self.selected.borrow().clone();
        // 外部 selected 优先，否则用 CanvasScene 内部 selected
        let effective_selected = external_selected.or(selected.clone());
        let node_index = self.node_index.borrow();

        for node in nodes {
            let is_selected = effective_selected.as_deref() == Some(node.id.as_str());
            let is_highlighted = highlighted.contains(&node.id);
            let is_hovered = hovered.as_deref() == Some(node.id.as_str());
            let is_dragging = dragging.as_deref() == Some(node.id.as_str());

            let (bw, bh) = self.box_size(node);
            let bx = node.x - bw / 2.0;
            let by = node.y - bh / 2.0;
            let accent = node.color.as_str();

            // === 选中态：外扩扫描框 + 旋转虚线框（矩形版 HUD 环）===
            if is_selected {
                let scan_period = 1.8;
                let scan_t = (now % scan_period) / scan_period;
                let inflate = 4.0 + scan_t * 14.0;
                let scan_alpha = 0.9 * (1.0 - scan_t);
                ctx.set_stroke_style_str(&crate::components::hud_palette::hex_to_rgba(
                    "#f97316", scan_alpha,
                ));
                ctx.set_line_width(2.5);
                set_dash(ctx, &[]);
                round_rect_path(
                    ctx,
                    bx - inflate,
                    by - inflate,
                    bw + inflate * 2.0,
                    bh + inflate * 2.0,
                    NODE_BOX_R + 4.0,
                );
                ctx.stroke();

                // 瞄准镜风格虚线框：dashoffset 随时间滚动
                let rotation = now * 30.0;
                ctx.set_stroke_style_str(&crate::components::hud_palette::hex_to_rgba(accent, 0.6));
                ctx.set_line_width(1.0);
                set_dash(ctx, &[6.0, 4.0]);
                ctx.set_line_dash_offset(-rotation);
                round_rect_path(
                    ctx,
                    bx - 6.0,
                    by - 6.0,
                    bw + 12.0,
                    bh + 12.0,
                    NODE_BOX_R + 5.0,
                );
                ctx.stroke();
                set_dash(ctx, &[]);
            } else {
                // === 未选中态：呼吸框 ===
                let pulse_period = 2.4;
                let pulse_t = (now % pulse_period) / pulse_period;
                let phase = (pulse_t * std::f64::consts::TAU).sin();
                let alpha = 0.55 + phase * 0.18;
                ctx.set_stroke_style_str(&crate::components::hud_palette::hex_to_rgba(
                    accent, alpha,
                ));
                ctx.set_line_width(2.0);
                set_dash(ctx, &[]);
                round_rect_path(
                    ctx,
                    bx - 3.0,
                    by - 3.0,
                    bw + 6.0,
                    bh + 6.0,
                    NODE_BOX_R + 3.0,
                );
                ctx.stroke();
            }

            // === 卡片主体 ===
            if is_selected {
                ctx.set_shadow_blur(8.0);
                ctx.set_shadow_color("#f97316");
            } else if is_highlighted || is_hovered || is_dragging {
                ctx.set_shadow_blur(6.0);
                ctx.set_shadow_color(accent);
            } else {
                ctx.set_shadow_blur(0.0);
            }

            let opacity = if is_selected || is_highlighted || is_hovered || is_dragging {
                1.0
            } else {
                0.92
            };
            ctx.set_global_alpha(opacity);
            ctx.set_fill_style_str("rgba(17, 24, 39, 0.94)");
            round_rect_path(ctx, bx, by, bw, bh, NODE_BOX_R);
            ctx.fill();

            let border_color = crate::components::hud_palette::hex_to_rgba(accent, 0.75);
            ctx.set_stroke_style_str(if is_selected {
                "#f97316"
            } else {
                &border_color
            });
            ctx.set_line_width(if is_selected { 3.0 } else { 1.5 });
            set_dash(ctx, &[]);
            round_rect_path(ctx, bx, by, bw, bh, NODE_BOX_R);
            ctx.stroke();
            ctx.set_global_alpha(1.0);
            ctx.set_shadow_blur(0.0);

            // 左侧类型色竖条（取代圆形的整体填充）
            ctx.set_fill_style_str(accent);
            round_rect_path(ctx, bx, by, NODE_ACCENT_W, bh, 2.0);
            ctx.fill();

            // === 卡片文案：名称 + 摘要/描述 + 标签 ===
            let text_x = bx + NODE_ACCENT_W + NODE_BOX_PAD;
            ctx.set_text_align("left");
            ctx.set_text_baseline("top");

            let gn = node_index.get(&node.id);
            let title = match gn {
                Some(gn) => node_title(gn),
                None => crate::components::graph::truncate_chars(&node.label, 12),
            };
            ctx.set_font(&format!("600 {NODE_TITLE_PX}px sans-serif"));
            ctx.set_fill_style_str("#f9fafb");
            let _ = ctx.fill_text(&title, text_x, by + NODE_BOX_PAD);

            if let Some(gn) = gn {
                ctx.set_font(&format!("{NODE_BODY_PX}px sans-serif"));
                ctx.set_fill_style_str("#9ca3af");
                for (i, line) in node_body_lines(gn).iter().enumerate() {
                    let _ = ctx.fill_text(
                        line,
                        text_x,
                        by + NODE_BOX_PAD + NODE_TITLE_H + i as f64 * NODE_BODY_H,
                    );
                }

                // 标签胶囊（卡片底部一行，最多 3 个 + 聚合计数）
                let chips = node_tag_chips(gn);
                if !chips.is_empty() {
                    ctx.set_font("8px sans-serif");
                    let tag_y = by
                        + NODE_BOX_PAD
                        + NODE_TITLE_H
                        + NODE_BODY_MAX_LINES as f64 * NODE_BODY_H
                        + 3.0;
                    let mut tx = text_x;
                    for (text, color) in &chips {
                        let w = measure_text_width(ctx, text, 8.0) + 8.0;
                        ctx.set_fill_style_str(color);
                        round_rect_path(ctx, tx, tag_y, w, 12.0, 6.0);
                        ctx.fill();
                        ctx.set_fill_style_str("#ffffff");
                        ctx.set_text_align("center");
                        let _ = ctx.fill_text(text, tx + w / 2.0, tag_y + 2.5);
                        ctx.set_text_align("left");
                        tx += w + 4.0;
                    }
                }
            }
        }

        // === hover 详情卡片（节点卡与边卡统一结构，绘制在全部节点之上）===
        // 节点卡：hover 命中且非拖拽中时展示（名称/类型/标签/摘要/描述/ID）
        if let Some(hid) = hovered.as_ref()
            && dragging.as_ref() != Some(hid)
            && let Some(node) = nodes.iter().find(|n| &n.id == hid)
        {
            let lines = match node_index.get(hid) {
                Some(gn) => node_hover_lines(gn),
                None => vec![format!("ID: {hid}")],
            };
            let (bw, _) = self.box_size(node);
            draw_hover_card_anchored(ctx, &lines, &node.color, node.x, node.y, bw / 2.0 + 12.0);
        }

        // 边卡补绘：取 draw_edges_with_state 暂存的 hover 目标（此刻置顶于所有节点）
        let pending = self.pending_edge_card.borrow().clone();
        if let Some((sf, st)) = pending {
            let relation = {
                let edge_labels = self.edge_labels.borrow();
                edge_labels
                    .get(&(sf.clone(), st.clone()))
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .unwrap_or_else(|| "关联".to_string())
            };
            let from = nodes.iter().find(|n| n.id == sf);
            let to = nodes.iter().find(|n| n.id == st);
            if let (Some(from), Some(to)) = (from, to) {
                let lines = vec![
                    format!("关系: {relation}"),
                    format!("端点: {} → {}", from.label, to.label),
                ];
                let mx = (from.x + to.x) / 2.0;
                let my = (from.y + to.y) / 2.0;
                draw_hover_card_anchored(ctx, &lines, get_edge_color(&relation), mx, my, 14.0);
            }
            *self.pending_edge_card.borrow_mut() = None;
        }
    }
}

/// KnowledgeGraphCanvas Props
#[derive(Props, Clone, PartialEq)]
pub struct KnowledgeGraphCanvasProps {
    /// 节点列表（复用 SVG 版 GraphNode 结构）
    pub nodes: Vec<GraphNode>,
    /// 边列表（复用 SVG 版 GraphEdge 结构）
    pub edges: Vec<GraphEdge>,
    /// 选中节点 ID
    pub selected_node_id: Option<String>,
    /// 高亮节点 ID 列表（搜索匹配结果）
    pub highlighted_node_ids: Option<Vec<String>>,
    /// 节点点击回调
    pub on_node_click: EventHandler<String>,
    /// 是否自适应父容器尺寸（铺满包裹层，去掉固定 800×600 导致的 HiDPI 溢出）
    #[props(default = true)]
    pub auto_size: bool,
}

/// 知识图谱 Canvas 组件（HUD 驾驶舱风格）
///
/// 与 SVG 版 Graph 组件功能对等，使用 Canvas 渲染提升大规模节点性能。
/// 通过 sync_state 将外部状态同步到自定义渲染器。
#[component]
pub fn KnowledgeGraphCanvas(props: KnowledgeGraphCanvasProps) -> Element {
    // 创建渲染器实例（仅首次渲染时创建，后续通过 sync_state 更新）
    let renderer: Rc<KnowledgeGraphRenderer> = use_hook(|| Rc::new(KnowledgeGraphRenderer::new()));

    // 同步外部状态到渲染器（高亮、选中、边 label、节点全量数据）
    {
        let highlighted: HashSet<String> = props
            .highlighted_node_ids
            .clone()
            .unwrap_or_default()
            .into_iter()
            .collect();
        let selected = props.selected_node_id.clone();
        let mut edge_labels: HashMap<(String, String), String> = HashMap::new();
        for e in &props.edges {
            edge_labels.insert((e.source.clone(), e.target.clone()), e.label.clone());
        }
        let mut index: HashMap<String, GraphNode> = HashMap::new();
        for n in &props.nodes {
            index.insert(n.id.clone(), n.clone());
        }
        renderer.sync_state(highlighted, selected, edge_labels, index);
    };

    // 转换 GraphNode -> CanvasNode（矩形卡片用 node_index 取几何，radius 仅作兜底）
    let canvas_nodes: Vec<CanvasNode> = props
        .nodes
        .iter()
        .map(|n| CanvasNode {
            id: n.id.clone(),
            x: n.x,
            y: n.y,
            radius: node_box_height(n) / 2.0,
            label: n.label.clone(),
            color: get_node_fill(&n.node_type).to_string(),
            node_type: Some(n.node_type.clone()),
            layer: None,
        })
        .collect();
    // 转换 GraphEdge -> CanvasEdge
    let canvas_edges: Vec<CanvasEdge> = props
        .edges
        .iter()
        .map(|e| CanvasEdge {
            from_id: e.source.clone(),
            to_id: e.target.clone(),
            ..Default::default()
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
                // 关闭力导向布局：保留外部 calculate_layout/expand_layout 的辐射布局
                enable_force_layout: false,
                // 关闭 CanvasScene 自带粒子（知识图谱用自定义 HUD 效果，避免视觉过载）
                enable_data_flow_particles: false,
                enable_glow_particles: false,
                enable_background_particles: false,
                enable_birth_death_particles: false,
                on_node_click: Some(EventHandler::new(move |id: String| {
                    on_click.call(id);
                })),
            }
        }
    }
}
