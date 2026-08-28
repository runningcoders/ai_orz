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

use crate::components::force_layout::{ForceLayout, ForceLayoutConfig, circle_initial_layout};
use crate::components::particles::{
    BackgroundParticles, BirthDeathParticles, DataFlowParticles, GlowParticles, ParticleSystem,
};

/// Canvas 渲染节点（通用数据结构，业务场景填充字段）
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CanvasNode {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub radius: f64,
    pub label: String,
    pub color: String,
    /// 节点类型标识（如 "project"/"agent"/"task"），用于点击回调判断
    pub node_type: Option<String>,
    /// 分层布局的层级（0=顶层，越大越靠下；None 表示不参与分层约束）
    pub layer: Option<i32>,
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
    fn draw_edges(
        &self,
        ctx: &CanvasRenderingContext2d,
        edges: &[CanvasEdge],
        nodes: &[CanvasNode],
    );

    /// 命中检测：给定画布坐标，返回命中的节点 ID（None 表示空白处）
    fn hit_test(&self, nodes: &[CanvasNode], x: f64, y: f64) -> Option<String>;

    /// 带交互状态的节点渲染（hover/选中/拖拽），默认委托给 draw_nodes
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

    fn draw_edges(
        &self,
        ctx: &CanvasRenderingContext2d,
        edges: &[CanvasEdge],
        nodes: &[CanvasNode],
    ) {
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

            // 节点圆形
            ctx.set_fill_style_str(&node.color);
            ctx.begin_path();
            let _ = ctx.arc(node.x, node.y, draw_radius, 0.0, std::f64::consts::TAU);
            ctx.fill();

            // 选中边框
            if is_selected {
                ctx.set_stroke_style_str("#3b82f6");
                ctx.set_line_width(3.0);
                ctx.begin_path();
                let _ = ctx.arc(node.x, node.y, draw_radius, 0.0, std::f64::consts::TAU);
                ctx.stroke();
            }

            // 标签
            ctx.set_fill_style_str("white");
            ctx.set_font(if is_hovered {
                "11px sans-serif"
            } else {
                "10px sans-serif"
            });
            ctx.set_text_align("center");
            ctx.set_text_baseline("middle");
            let label: String = node.label.chars().take(8).collect();
            let _ = ctx.fill_text(&label, node.x, node.y);
        }
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
                    // 保留已有节点位置，更新外观字段
                    merged.push(CanvasNode {
                        id: existing.id.clone(),
                        x: existing.x,
                        y: existing.y,
                        radius: new_node.radius,
                        label: new_node.label.clone(),
                        color: new_node.color.clone(),
                        node_type: new_node.node_type.clone(),
                        layer: new_node.layer,
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

    // --- 渲染循环 effect：request_animation_frame 递归调用，每帧步进力学 + 重绘 ---
    let render_width = props.width;
    let render_height = props.height;
    let enable_force = props.enable_force_layout;
    let render_edges = props.edges.clone();
    let mut nodes_state_c = nodes_state;
    let mut force_layout_c = force_layout;
    let mut is_stable_c = is_stable;
    let dragging_id_c = dragging_id;
    let hovered_id_c = hovered_id;
    let selected_id_c = selected_id;
    let renderer_c = renderer;
    // RAF 渲染循环资源：保存 running flag + Closure 供顶层 use_drop 清理
    #[allow(clippy::type_complexity)]
    struct RafResource {
        running: std::sync::Arc<std::sync::atomic::AtomicBool>,
        callback_ref: Rc<RefCell<Option<Closure<dyn FnMut()>>>>,
    }
    let mut raf_resource = use_signal(|| Option::<RafResource>::None);

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
        // 克隆 edges 给内部 Closure（use_effect 是 FnMut 可多次调用，不能直接 move）
        let edges_inner = render_edges.clone();

        // running 标志：组件卸载时设为 false，停止递归 rAF
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let running_clone = running.clone();

        // Rc<RefCell<Option<Closure>>> 模式：Closure 自引用递归 rAF
        #[allow(clippy::type_complexity)]
        let callback_ref: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
        let cb_ref_inner = callback_ref.clone();

        let mut data_flow_c = data_flow;
        let mut glow_c = glow;
        let mut background_c = background;
        let mut birth_death_c = birth_death;
        let enable_data_flow = props.enable_data_flow_particles;
        let enable_glow = props.enable_glow_particles;
        let enable_bg = props.enable_background_particles;
        let enable_bd = props.enable_birth_death_particles;

        let closure = Closure::<dyn FnMut()>::new(move || {
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

            renderer_c.clear(&ctx, width, height);

            // 1. 背景粒子（最底层）
            if enable_bg {
                background_c.read().draw(&ctx);
            }

            // 2. 连线
            renderer_c.draw_edges(&ctx, &edges_inner, &nodes);

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

            // 6. 诞生/消亡粒子（最上层，醒目）
            if enable_bd {
                birth_death_c.read().draw(&ctx);
            }

            // 递归注册下一帧
            if running_clone.load(std::sync::atomic::Ordering::SeqCst)
                && let Some(cb) = cb_ref_inner.borrow().as_ref()
                && let Some(window) = web_sys::window()
            {
                let _ = window.request_animation_frame(cb.as_ref().unchecked_ref());
            }
        });

        // 初始注册第一帧
        if let Some(window) = web_sys::window() {
            let _ = window.request_animation_frame(closure.as_ref().unchecked_ref());
        }
        *callback_ref.borrow_mut() = Some(closure);

        raf_resource.set(Some(RafResource {
            running,
            callback_ref,
        }));
    });

    use_drop(move || {
        if let Some(res) = raf_resource.take() {
            res.running
                .store(false, std::sync::atomic::Ordering::SeqCst);
            *res.callback_ref.borrow_mut() = None;
        }
    });

    // 提取 onclick 所需字段
    let on_node_click = props.on_node_click;

    rsx! {
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
                let nodes = nodes_state.read().clone();
                if let Some(node_id) = renderer.hit_test(&nodes, x, y) {
                    dragging_id.set(Some(node_id.clone()));
                    is_stable.set(false);
                    if let Some(node) = nodes.iter().find(|n| n.id == node_id) {
                        drag_offset.set((x - node.x, y - node.y));
                        // 触发拖拽开始时的辉光
                        glow.write().trigger(node);
                    }
                }
            },
            onmousemove: move |e: MouseEvent| {
                let Some(canvas) = canvas_ref.read().clone() else { return; };
                let rect = canvas.get_bounding_client_rect();
                let coords = e.client_coordinates();
                let x = coords.x - rect.left();
                let y = coords.y - rect.top();

                let dragging = dragging_id.read().clone();
                if let Some(drag_id) = &dragging {
                    // 拖拽中：更新节点位置
                    let offset = *drag_offset.read();
                    let mut nodes = nodes_state.read().clone();
                    if let Some(node) = nodes.iter_mut().find(|n| &n.id == drag_id) {
                        node.x = x - offset.0;
                        node.y = y - offset.1;
                    }
                    nodes_state.set(nodes);
                    is_stable.set(false);
                } else {
                    // 非拖拽：更新 hover 状态
                    let nodes = nodes_state.read().clone();
                    let new_hovered = renderer.hit_test(&nodes, x, y);
                    let current_hovered = hovered_id.read().clone();
                    if new_hovered != current_hovered {
                        hovered_id.set(new_hovered);
                    }
                }
            },
            onmouseup: move |_| {
                dragging_id.set(None);
            },
            onmouseleave: move |_| {
                dragging_id.set(None);
                hovered_id.set(None);
            },
            onclick: move |e: MouseEvent| {
                let Some(canvas) = canvas_ref.read().clone() else { return; };
                let Some(on_click) = on_node_click.as_ref() else { return; };
                let rect = canvas.get_bounding_client_rect();
                let coords = e.client_coordinates();
                let x = coords.x - rect.left();
                let y = coords.y - rect.top();
                let nodes = nodes_state.read().clone();
                if let Some(node_id) = renderer.hit_test(&nodes, x, y) {
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
    }
}
