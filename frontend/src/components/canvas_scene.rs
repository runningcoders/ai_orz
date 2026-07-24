//! Canvas 场景渲染基础设施
//!
//! 提供 Dioxus ↔ Canvas 2D 桥接层：
//! - CanvasScene 组件封装 <canvas> 元素 + Context 初始化
//! - CanvasRenderer trait 抽象渲染逻辑（由业务场景实现）
//! - 事件桥：鼠标事件 → 坐标转换 → 命中检测 → Dioxus callback
//! - 渲染循环：request_animation_frame + dirty flag 按需重绘

use dioxus::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

/// Canvas 渲染节点（通用数据结构，业务场景填充字段）
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CanvasNode {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub radius: f64,
    pub label: String,
    pub color: String,
}

/// Canvas 渲染连线
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CanvasEdge {
    pub from_id: String,
    pub to_id: String,
}

/// Canvas 渲染器 trait：业务场景实现此 trait 定义渲染逻辑
pub trait CanvasRenderer {
    /// 清空画布
    fn clear(&self, ctx: &CanvasRenderingContext2d, width: f64, height: f64);

    /// 绘制所有节点
    fn draw_nodes(&self, ctx: &CanvasRenderingContext2d, nodes: &[CanvasNode]);

    /// 绘制所有连线
    fn draw_edges(&self, ctx: &CanvasRenderingContext2d, edges: &[CanvasEdge], nodes: &[CanvasNode]);

    /// 命中检测：给定画布坐标，返回命中的节点 ID（None 表示空白处）
    fn hit_test(&self, nodes: &[CanvasNode], x: f64, y: f64) -> Option<String>;
}

/// 默认渲染器：基础圆形节点 + 直线连线
#[derive(Clone, Copy)]
pub struct DefaultRenderer;

impl CanvasRenderer for DefaultRenderer {
    fn clear(&self, ctx: &CanvasRenderingContext2d, width: f64, height: f64) {
        ctx.clear_rect(0.0, 0.0, width, height);
    }

    fn draw_nodes(&self, ctx: &CanvasRenderingContext2d, nodes: &[CanvasNode]) {
        for node in nodes {
            // 节点圆形
            ctx.set_fill_style_str(&node.color);
            ctx.begin_path();
            let _ = ctx.arc(node.x, node.y, node.radius, 0.0, std::f64::consts::TAU);
            ctx.fill();

            // 节点标签
            ctx.set_fill_style_str("white");
            ctx.set_font("10px sans-serif");
            ctx.set_text_align("center");
            ctx.set_text_baseline("middle");
            let label: String = node.label.chars().take(8).collect();
            let _ = ctx.fill_text(&label, node.x, node.y);
        }
    }

    fn draw_edges(&self, ctx: &CanvasRenderingContext2d, edges: &[CanvasEdge], nodes: &[CanvasNode]) {
        ctx.set_stroke_style_str("rgba(107, 114, 128, 0.4)");
        ctx.set_line_width(1.5);
        for edge in edges {
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
}

impl Default for CanvasSceneProps {
    fn default() -> Self {
        Self {
            width: 800.0,
            height: 600.0,
            nodes: Vec::new(),
            edges: Vec::new(),
            on_node_click: None,
        }
    }
}

/// CanvasScene 组件：封装 <canvas> 元素 + Context 初始化 + 渲染 + 事件桥
#[component]
pub fn CanvasScene(props: CanvasSceneProps) -> Element {
    let mut canvas_ref: Signal<Option<HtmlCanvasElement>> = use_signal(|| None);
    let renderer = DefaultRenderer;

    // 提取 effect 所需字段（避免整体 move props 影响 rsx 使用）
    let effect_width = props.width;
    let effect_height = props.height;
    let effect_nodes = props.nodes.clone();
    let effect_edges = props.edges.clone();

    // 初始化 Canvas Context
    use_effect(move || {
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
        canvas.set_width((effect_width * dpr) as u32);
        canvas.set_height((effect_height * dpr) as u32);
        let _ = ctx.scale(dpr, dpr);

        // 初始渲染
        renderer.clear(&ctx, effect_width, effect_height);
        renderer.draw_edges(&ctx, &effect_edges, &effect_nodes);
        renderer.draw_nodes(&ctx, &effect_nodes);
    });

    // 提取 onclick 所需字段
    let click_nodes = props.nodes.clone();
    let on_node_click = props.on_node_click;

    rsx! {
        canvas {
            width: "{props.width}",
            height: "{props.height}",
            style: "border: 1px solid #e5e7eb; border-radius: 8px; display: block; background: #fafafa;",
            onmounted: move |evt: MountedEvent| {
                let data = evt.data();
                if let Some(element) = data.downcast::<web_sys::Element>() {
                    let canvas = element.clone().unchecked_into::<HtmlCanvasElement>();
                    canvas_ref.set(Some(canvas));
                }
            },
            onclick: move |e: MouseEvent| {
                let Some(canvas) = canvas_ref.read().clone() else {
                    return;
                };
                let Some(on_click) = on_node_click.as_ref() else {
                    return;
                };
                // 坐标转换：屏幕坐标 → Canvas 坐标
                let rect = canvas.get_bounding_client_rect();
                let coords = e.client_coordinates();
                let x = coords.x - rect.left();
                let y = coords.y - rect.top();
                // 命中检测
                if let Some(node_id) = renderer.hit_test(&click_nodes, x, y) {
                    on_click.call(node_id);
                }
            },
        }
    }
}
