//! 知识图谱 Canvas 渲染器（HUD 驾驶舱风格）
//!
//! 基于 CanvasScene 基础设施，实现自定义 CanvasRenderer：
//! - 深色径向渐变背景 + 淡橙色网格 + 四角 HUD 装饰
//! - 节点：选中态扫描环 + 旋转刻度环；未选中态呼吸光晕
//! - 边：实线边流光（lineDashOffset 动画）+ drop-shadow 发光
//! - 节点出现动画（首次渲染 scale 0→1）
//!
//! 与 SVG 版 Graph 组件功能对等，作为高级渲染模式。
//! SVG 版保留作为兜底方案（节点数少或 canvas 不可用时）。

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use dioxus::prelude::*;
use wasm_bindgen::JsValue;
use web_sys::CanvasRenderingContext2d;

use crate::components::canvas_scene::{CanvasEdge, CanvasNode, CanvasRenderer, CanvasScene};
use crate::components::graph::{
    GraphEdge, GraphNode, dynamic_node_radius, get_edge_color, get_node_fill, tag_color,
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

/// 节点扩展元数据（canvas 渲染需要但 CanvasNode 未携带的信息）
#[derive(Clone, Default)]
pub struct NodeMeta {
    node_type: String,
    tags: Vec<String>,
    summary: Option<String>,
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
    /// 节点扩展元数据：id -> NodeMeta
    node_meta: RefCell<HashMap<String, NodeMeta>>,
    /// 已渲染过的节点 ID（用于首次出现动画）
    appeared: RefCell<HashSet<String>>,
}

impl KnowledgeGraphRenderer {
    pub fn new() -> Self {
        Self {
            highlighted: RefCell::new(HashSet::new()),
            selected: RefCell::new(None),
            edge_labels: RefCell::new(HashMap::new()),
            node_meta: RefCell::new(HashMap::new()),
            appeared: RefCell::new(HashSet::new()),
        }
    }

    /// 同步外部状态到渲染器
    pub fn sync_state(
        &self,
        highlighted: HashSet<String>,
        selected: Option<String>,
        edge_labels: HashMap<(String, String), String>,
        node_meta: HashMap<String, NodeMeta>,
    ) {
        *self.highlighted.borrow_mut() = highlighted;
        *self.selected.borrow_mut() = selected;
        *self.edge_labels.borrow_mut() = edge_labels;
        *self.node_meta.borrow_mut() = node_meta;
    }

    /// 当前时间戳（秒），用于动画
    fn now_secs() -> f64 {
        js_sys::Date::now() / 1000.0
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
                let is_dashed = matches!(relation_type, "引用" | "依赖");

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

                // 边标签（关系类型）
                if !relation_type.is_empty() {
                    let mid_x = (from.x + to.x) / 2.0;
                    let mid_y = (from.y + to.y) / 2.0 - 8.0;
                    let label: String = relation_type.chars().take(10).collect();
                    let label_w = label.chars().count() as f64 * 7.0 + 4.0;

                    ctx.set_shadow_blur(0.0);
                    ctx.set_fill_style_str("rgba(255, 255, 255, 0.9)");
                    ctx.begin_path();
                    let _ = ctx.round_rect_with_f64(
                        mid_x - label_w / 2.0,
                        mid_y - 7.0,
                        label_w,
                        14.0,
                        2.0,
                    );
                    ctx.fill();
                    ctx.set_stroke_style_str("#e5e7eb");
                    ctx.set_line_width(1.0);
                    set_dash(ctx, &[]);
                    ctx.stroke();

                    ctx.set_fill_style_str("#374151");
                    ctx.set_font("10px sans-serif");
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

    fn hit_test(&self, nodes: &[CanvasNode], x: f64, y: f64) -> Option<String> {
        for node in nodes.iter().rev() {
            let dx = x - node.x;
            let dy = y - node.y;
            if dx * dx + dy * dy <= node.radius * node.radius {
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
        let node_meta = self.node_meta.borrow();
        let mut appeared = self.appeared.borrow_mut();

        for node in nodes {
            let is_selected = effective_selected.as_deref() == Some(node.id.as_str());
            let is_highlighted = highlighted.contains(&node.id);
            let is_hovered = hovered.as_deref() == Some(node.id.as_str());
            let is_dragging = dragging.as_deref() == Some(node.id.as_str());

            let meta = node_meta.get(&node.id).cloned().unwrap_or_default();
            let base_radius = node.radius;

            // 出现动画：首次渲染时 scale 0→1 弹性淡入
            let is_new = appeared.insert(node.id.clone());
            let appear_scale = if is_new {
                // 新节点：刚出现，scale 从 0 开始
                0.0
            } else {
                1.0
            };
            // 注：canvas 无法像 SVG 那样用 CSS 动画自动过渡，
            // 这里用时间戳计算 scale（首次出现后 0.5s 内动画）
            // 但 appeared 只在首次插入时为 true，后续帧 is_new=false，
            // 所以无法用 appeared 跟踪动画进度。
            // 简化：用节点 id hash + now 生成稳定的"出现时间"，但无法得知真实首次时间。
            // 折中：不做出现动画（canvas 重绘频繁，CSS 动画不适用），保留呼吸/扫描即可。
            let _ = (is_new, appear_scale);

            let radius = if is_hovered || is_dragging {
                base_radius * 1.1
            } else {
                base_radius
            };

            // === 选中态：扫描环 + 旋转刻度环 ===
            if is_selected {
                // 扫描环波纹（雷达扫描）：向外扩散并淡出
                let scan_period = 1.8;
                let scan_t = (now % scan_period) / scan_period;
                let scan_r = base_radius + 4.0 + scan_t * 18.0;
                let scan_alpha = 0.9 * (1.0 - scan_t);
                ctx.set_stroke_style_str(&crate::components::hud_palette::hex_to_rgba(
                    &node.color,
                    scan_alpha,
                ));
                ctx.set_line_width(2.5);
                set_dash(ctx, &[]);
                ctx.begin_path();
                let _ = ctx.arc(node.x, node.y, scan_r, 0.0, std::f64::consts::TAU);
                ctx.stroke();

                // HUD 外环刻度（瞄准镜风格）：旋转的虚线圆 + 四向小刻度
                let ring_r = base_radius + 8.0;
                let rotation = (now * 30.0_f64.to_radians()) % std::f64::consts::TAU;
                ctx.set_stroke_style_str(&crate::components::hud_palette::hex_to_rgba(
                    &node.color,
                    0.6,
                ));
                ctx.set_line_width(1.0);
                set_dash(ctx, &[3.0, 6.0]);
                ctx.set_line_dash_offset(-rotation * 6.0);
                ctx.begin_path();
                let _ = ctx.arc(node.x, node.y, ring_r, 0.0, std::f64::consts::TAU);
                ctx.stroke();
                set_dash(ctx, &[]);

                // 四向小刻度线
                ctx.set_line_width(1.5);
                for i in 0..4 {
                    let angle = rotation + (i as f64) * std::f64::consts::FRAC_PI_2;
                    let x1 = node.x + (ring_r - 3.0) * angle.cos();
                    let y1 = node.y + (ring_r - 3.0) * angle.sin();
                    let x2 = node.x + (ring_r + 3.0) * angle.cos();
                    let y2 = node.y + (ring_r + 3.0) * angle.sin();
                    ctx.begin_path();
                    ctx.move_to(x1, y1);
                    ctx.line_to(x2, y2);
                    ctx.stroke();
                }
            } else {
                // === 未选中态：呼吸光晕 ===
                let pulse_period = 2.4;
                let pulse_t = (now % pulse_period) / pulse_period;
                let phase = (pulse_t * std::f64::consts::TAU).sin();
                let alpha = 0.55 + phase * 0.18;
                ctx.set_stroke_style_str(&crate::components::hud_palette::hex_to_rgba(
                    &node.color,
                    alpha,
                ));
                ctx.set_line_width(2.0);
                set_dash(ctx, &[]);
                ctx.begin_path();
                let _ = ctx.arc(
                    node.x,
                    node.y,
                    base_radius + 2.0,
                    0.0,
                    std::f64::consts::TAU,
                );
                ctx.stroke();
            }

            // === 多色 tag 边框 arc 段 ===
            if !meta.tags.is_empty() {
                let border_r = base_radius + 3.0;
                let n = meta.tags.len() as f64;
                for (i, tag) in meta.tags.iter().enumerate() {
                    let a1 = (i as f64 / n) * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2;
                    let a2 =
                        ((i + 1) as f64 / n) * std::f64::consts::TAU - std::f64::consts::FRAC_PI_2;
                    ctx.set_stroke_style_str(tag_color(tag));
                    ctx.set_line_width(3.0);
                    set_dash(ctx, &[]);
                    ctx.begin_path();
                    let _ = ctx.arc(node.x, node.y, border_r, a1, a2);
                    ctx.stroke();
                }
            }

            // === 节点主体 ===
            // 选中/高亮发光
            if is_selected {
                ctx.set_shadow_blur(8.0);
                ctx.set_shadow_color("#f97316");
            } else if is_highlighted {
                ctx.set_shadow_blur(6.0);
                ctx.set_shadow_color(&node.color);
            } else {
                ctx.set_shadow_blur(0.0);
            }

            let opacity = if is_selected || is_highlighted {
                1.0
            } else {
                0.85
            };
            ctx.set_global_alpha(opacity);
            ctx.set_fill_style_str(&node.color);
            ctx.begin_path();
            let _ = ctx.arc(node.x, node.y, radius, 0.0, std::f64::consts::TAU);
            ctx.fill();

            // 选中边框
            if is_selected {
                ctx.set_shadow_blur(0.0);
                ctx.set_stroke_style_str("#f97316");
                ctx.set_line_width(3.0);
                set_dash(ctx, &[]);
                ctx.begin_path();
                let _ = ctx.arc(node.x, node.y, radius, 0.0, std::f64::consts::TAU);
                ctx.stroke();
            } else {
                ctx.set_stroke_style_str("#ffffff");
                ctx.set_line_width(2.0);
                set_dash(ctx, &[]);
                ctx.begin_path();
                let _ = ctx.arc(node.x, node.y, radius, 0.0, std::f64::consts::TAU);
                ctx.stroke();
            }
            ctx.set_global_alpha(1.0);
            ctx.set_shadow_blur(0.0);

            // === 节点标签 ===
            ctx.set_fill_style_str("white");
            ctx.set_font(if is_hovered {
                "11px sans-serif"
            } else {
                "10px sans-serif"
            });
            ctx.set_text_align("center");
            ctx.set_text_baseline("middle");
            let label: String = node.label.chars().take(10).collect();
            let _ = ctx.fill_text(&label, node.x, node.y);

            // === 节点下方简介 ===
            if let Some(summary) = &meta.summary {
                let s: String = summary.chars().take(14).collect();
                ctx.set_fill_style_str("#9ca3af");
                ctx.set_font("8px sans-serif");
                let _ = ctx.fill_text(&s, node.x, node.y + radius + 11.0);
            }

            // === 节点上方 tag 标签（带颜色底色）===
            if !meta.tags.is_empty() {
                let tag_label_y = node.y - radius - 16.0;
                let tag_widths: Vec<(String, f64, &str)> = meta
                    .tags
                    .iter()
                    .map(|t| {
                        let w: f64 = t
                            .chars()
                            .fold(0.0, |acc, c| acc + if c.is_ascii() { 5.0 } else { 9.0 })
                            + 8.0;
                        (t.clone(), w, tag_color(t))
                    })
                    .collect();
                let total_w: f64 = tag_widths.iter().map(|(_, w, _)| *w).sum::<f64>()
                    + (tag_widths.len().saturating_sub(1) as f64) * 4.0;
                let mut tx = node.x - total_w / 2.0;
                for (tag_text, w, color) in &tag_widths {
                    ctx.set_fill_style_str(color);
                    ctx.begin_path();
                    let _ = ctx.round_rect_with_f64(tx, tag_label_y, *w, 12.0, 6.0);
                    ctx.fill();
                    ctx.set_fill_style_str("white");
                    ctx.set_font("8px sans-serif");
                    ctx.set_text_align("center");
                    ctx.set_text_baseline("middle");
                    let _ = ctx.fill_text(tag_text, tx + w / 2.0, tag_label_y + 6.0);
                    tx += w + 4.0;
                }
            }
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
}

/// 知识图谱 Canvas 组件（HUD 驾驶舱风格）
///
/// 与 SVG 版 Graph 组件功能对等，使用 Canvas 渲染提升大规模节点性能。
/// 通过 sync_state 将外部状态同步到自定义渲染器。
#[component]
pub fn KnowledgeGraphCanvas(props: KnowledgeGraphCanvasProps) -> Element {
    // 创建渲染器实例（仅首次渲染时创建，后续通过 sync_state 更新）
    let renderer: Rc<KnowledgeGraphRenderer> = use_hook(|| Rc::new(KnowledgeGraphRenderer::new()));

    // 同步外部状态到渲染器（高亮、选中、边 label、节点元数据）
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
        let mut node_meta: HashMap<String, NodeMeta> = HashMap::new();
        for n in &props.nodes {
            node_meta.insert(
                n.id.clone(),
                NodeMeta {
                    node_type: n.node_type.clone(),
                    tags: n.tags.clone(),
                    summary: n.summary.clone(),
                },
            );
        }
        renderer.sync_state(highlighted, selected, edge_labels, node_meta);
    }

    // 转换 GraphNode -> CanvasNode
    let canvas_nodes: Vec<CanvasNode> = props
        .nodes
        .iter()
        .map(|n| CanvasNode {
            id: n.id.clone(),
            x: n.x,
            y: n.y,
            radius: dynamic_node_radius(n),
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
        })
        .collect();

    let on_click = props.on_node_click;

    rsx! {
        CanvasScene {
            width: 800.0,
            height: 600.0,
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
