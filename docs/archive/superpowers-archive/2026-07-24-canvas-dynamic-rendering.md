# Canvas 动态渲染增强实施计划（阶段 2）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 CanvasScene 从"静态单次渲染"升级为"持续渲染循环 + 力导向布局 + 拖拽 + hover/选中动画"，在 /workspace 上验证动态 Canvas 交互能力，为阶段 3（迁移知识图谱）奠定技术基础。

**Architecture:** 将力导向布局算法提取为独立纯函数模块 `force_layout.rs`（可单元测试）。CanvasScene 组件引入内部状态：`Signal<Vec<CanvasNode>>` 存储节点实时位置、`Signal<bool>` dirty flag、`Signal<Option<String>>` 拖拽/hover 状态。渲染循环通过 `request_animation_frame` 递归注册，每帧：若 dirty 或力导向未稳定 → 步进力学 → 更新位置 → 重绘；否则跳过（节能）。事件链：mousedown 命中检测开始拖拽，mousemove 更新拖拽位置或 hover 状态，mouseup 结束拖拽。

**Tech Stack:** Dioxus 0.7.9, web-sys 0.3.103（新增 Closure features）, wasm-bindgen 0.2

---

## 文件结构

| 文件 | 责任 | 操作 |
|------|------|------|
| `frontend/Cargo.toml` | 新增 wasm-bindgen features | 修改 |
| `frontend/src/components/force_layout.rs` | 力导向布局纯算法（斥力 + 吸引力 + 步进），可单元测试 | 新建 |
| `frontend/src/components/canvas_scene.rs` | CanvasScene 升级：渲染循环 + 拖拽 + hover + 选中 | 修改 |
| `frontend/src/components/mod.rs` | 注册 force_layout 模块 | 修改 |
| `frontend/src/pages/workspace.rs` | 增加更多示例节点验证力学效果 | 修改 |

---

### Task 1: 力导向布局算法（纯函数 + 单元测试）

**Files:**
- Create: `frontend/src/components/force_layout.rs`
- Modify: `frontend/src/components/mod.rs`

- [ ] **Step 1: 在 components/mod.rs 注册 force_layout 模块**

在 `frontend/src/components/mod.rs` 的 `pub mod canvas_scene;` 之后追加：

```rust
pub mod force_layout;
```

- [ ] **Step 2: 创建 force_layout.rs，定义数据结构与算法**

在 `frontend/src/components/force_layout.rs` 中写入：

```rust
//! 力导向布局算法（纯函数，可单元测试）
//!
//! 模型：
//! - 斥力：所有节点对互相排斥（库仑力，反比于距离平方）
//! - 吸引力：有连线的节点对互相吸引（胡克定律，正比于距离）
//! - 阻尼：每帧速度衰减，防止振荡
//! - 边界：节点不超出画布范围

use crate::components::canvas_scene::{CanvasEdge, CanvasNode};

/// 力导向布局参数
#[derive(Debug, Clone, Copy)]
pub struct ForceLayoutConfig {
    /// 斥力强度系数
    pub repulsion: f64,
    /// 连线吸引力强度系数（弹簧刚度）
    pub attraction: f64,
    /// 理想连线长度（弹簧自然长度）
    pub ideal_length: f64,
    /// 速度阻尼系数（每帧乘以该系数，<1.0 衰减）
    pub damping: f64,
    /// 单帧最大位移（防止爆炸性跳动）
    pub max_step: f64,
}

impl Default for ForceLayoutConfig {
    fn default() -> Self {
        Self {
            repulsion: 8000.0,
            attraction: 0.05,
            ideal_length: 120.0,
            damping: 0.85,
            max_step: 10.0,
        }
    }
}

/// 节点的速度状态（位置存在 CanvasNode.x/y，速度在此结构）
#[derive(Debug, Clone, Copy, Default)]
pub struct NodeVelocity {
    pub vx: f64,
    pub vy: f64,
}

/// 力导向布局模拟器
#[derive(Debug, Clone)]
pub struct ForceLayout {
    pub config: ForceLayoutConfig,
    pub velocities: Vec<NodeVelocity>,
}

impl ForceLayout {
    /// 创建新的力导向布局模拟器
    pub fn new(config: ForceLayoutConfig) -> Self {
        Self {
            config,
            velocities: Vec::new(),
        }
    }

    /// 同步速度向量数量与节点数量一致
    pub fn sync(&mut self, node_count: usize) {
        self.velocities.resize(node_count, NodeVelocity::default());
    }

    /// 执行一帧力学步进，更新节点位置，返回本帧总位移（用于稳定检测）
    ///
    /// 返回值：所有节点位移之和。当该值趋近于 0 时，布局已稳定。
    pub fn step(&mut self, nodes: &mut [CanvasNode], edges: &[CanvasEdge], width: f64, height: f64) -> f64 {
        self.sync(nodes.len());
        let n = nodes.len();
        if n == 0 {
            return 0.0;
        }

        let cfg = self.config;
        let mut forces: Vec<(f64, f64)> = vec![(0.0, 0.0); n];

        // 1. 斥力：所有节点对互相排斥
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = nodes[i].x - nodes[j].x;
                let dy = nodes[i].y - nodes[j].y;
                // 避免除零：距离极小时加微小偏移
                let dist_sq = dx * dx + dy * dy + 0.01;
                let dist = dist_sq.sqrt();
                let force = cfg.repulsion / dist_sq;
                let fx = force * dx / dist;
                let fy = force * dy / dist;
                forces[i].0 += fx;
                forces[i].1 += fy;
                forces[j].0 -= fx;
                forces[j].1 -= fy;
            }
        }

        // 2. 吸引力：有连线的节点对互相吸引（胡克定律）
        for edge in edges {
            let i = nodes.iter().position(|node| node.id == edge.from_id);
            let j = nodes.iter().position(|node| node.id == edge.to_id);
            if let (Some(i), Some(j)) = (i, j) {
                if i == j {
                    continue;
                }
                let dx = nodes[j].x - nodes[i].x;
                let dy = nodes[j].y - nodes[i].y;
                let dist = (dx * dx + dy * dy).sqrt().max(0.01);
                let displacement = dist - cfg.ideal_length;
                let force = cfg.attraction * displacement;
                let fx = force * dx / dist;
                let fy = force * dy / dist;
                forces[i].0 += fx;
                forces[i].1 += fy;
                forces[j].0 -= fx;
                forces[j].1 -= fy;
            }
        }

        // 3. 应用力到速度，再应用速度到位置（带阻尼和限幅）
        let mut total_displacement = 0.0;
        let margin = 30.0;
        for i in 0..n {
            self.velocities[i].vx = (self.velocities[i].vx + forces[i].0) * cfg.damping;
            self.velocities[i].vy = (self.velocities[i].vy + forces[i].1) * cfg.damping;

            // 限幅：单帧位移不超过 max_step
            let vx = self.velocities[i].vx.clamp(-cfg.max_step, cfg.max_step);
            let vy = self.velocities[i].vy.clamp(-cfg.max_step, cfg.max_step);
            self.velocities[i].vx = vx;
            self.velocities[i].vy = vy;

            nodes[i].x += vx;
            nodes[i].y += vy;

            // 边界约束：不超出画布
            nodes[i].x = nodes[i].x.clamp(margin, width - margin);
            nodes[i].y = nodes[i].y.clamp(margin, height - margin);

            total_displacement += vx.abs() + vy.abs();
        }

        total_displacement
    }

    /// 判断布局是否已稳定（总位移小于阈值）
    pub fn is_stable(&self, total_displacement: f64, threshold: f64) -> bool {
        total_displacement < threshold
    }
}

/// 给定节点数量，生成圆形初始布局（均匀分布在一个圆上）
pub fn circle_initial_layout(node_count: usize, center_x: f64, center_y: f64, radius: f64) -> Vec<(f64, f64)> {
    let mut positions = Vec::with_capacity(node_count);
    for i in 0..node_count {
        let angle = (i as f64 / node_count as f64) * std::f64::consts::TAU;
        positions.push((center_x + radius * angle.cos(), center_y + radius * angle.sin()));
    }
    positions
}
```

- [ ] **Step 3: 在 force_layout.rs 末尾追加单元测试**

在 `frontend/src/components/force_layout.rs` 末尾追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::canvas_scene::{CanvasEdge, CanvasNode};

    fn make_node(id: &str, x: f64, y: f64) -> CanvasNode {
        CanvasNode {
            id: id.to_string(),
            x,
            y,
            radius: 20.0,
            label: id.to_string(),
            color: "#3b82f6".to_string(),
        }
    }

    #[test]
    fn test_repulsion_pushes_nodes_apart() {
        // 两个节点初始重合，斥力应将它们推开
        let mut nodes = vec![make_node("a", 100.0, 100.0), make_node("a", 100.0, 100.0)];
        // 修正 id
        nodes[1].id = "b".to_string();
        let edges: Vec<CanvasEdge> = vec![];
        let mut layout = ForceLayout::new(ForceLayoutConfig::default());

        let before_dist = ((nodes[0].x - nodes[1].x).powi(2) + (nodes[0].y - nodes[1].y).powi(2)).sqrt();
        layout.step(&mut nodes, &edges, 800.0, 600.0);
        let after_dist = ((nodes[0].x - nodes[1].x).powi(2) + (nodes[0].y - nodes[1].y).powi(2)).sqrt();

        assert!(after_dist > before_dist, "斥力应将重合节点推开: before={}, after={}", before_dist, after_dist);
    }

    #[test]
    fn test_attraction_pulls_connected_nodes_closer() {
        // 两个节点距离远大于 ideal_length，吸引力应拉近它们
        let mut nodes = vec![make_node("a", 50.0, 300.0), make_node("b", 750.0, 300.0)];
        let edges = vec![CanvasEdge { from_id: "a".to_string(), to_id: "b".to_string() }];
        // 用强吸引力 + 弱斥力配置
        let config = ForceLayoutConfig {
            repulsion: 100.0,
            attraction: 0.2,
            ideal_length: 120.0,
            damping: 0.9,
            max_step: 50.0,
        };
        let mut layout = ForceLayout::new(config);

        let before_dist = ((nodes[0].x - nodes[1].x).powi(2) + (nodes[0].y - nodes[1].y).powi(2)).sqrt();
        // 步进多帧让吸引力生效
        for _ in 0..10 {
            layout.step(&mut nodes, &edges, 800.0, 600.0);
        }
        let after_dist = ((nodes[0].x - nodes[1].x).powi(2) + (nodes[0].y - nodes[1].y).powi(2)).sqrt();

        assert!(after_dist < before_dist, "吸引力应拉近连线节点: before={}, after={}", before_dist, after_dist);
    }

    #[test]
    fn test_boundary_constraint() {
        // 节点被推到画布外时应被拉回边界内
        let mut nodes = vec![make_node("a", 5.0, 5.0)]; // 接近左上角
        let edges: Vec<CanvasEdge> = vec![];
        let mut layout = ForceLayout::new(ForceLayoutConfig::default());

        layout.step(&mut nodes, &edges, 800.0, 600.0);

        let margin = 30.0;
        assert!(nodes[0].x >= margin, "节点 x 应在边界内: x={}", nodes[0].x);
        assert!(nodes[0].y >= margin, "节点 y 应在边界内: y={}", nodes[0].y);
    }

    #[test]
    fn test_stable_detection() {
        let layout = ForceLayout::new(ForceLayoutConfig::default());
        assert!(layout.is_stable(0.1, 1.0), "小位移应判为稳定");
        assert!(!layout.is_stable(100.0, 1.0), "大位移应判为不稳定");
    }

    #[test]
    fn test_circle_initial_layout() {
        let positions = circle_initial_layout(4, 400.0, 300.0, 100.0);
        assert_eq!(positions.len(), 4);
        // 第一个点应在 (center_x + radius, center_y) 附近（角度 0）
        assert!((positions[0].0 - 500.0).abs() < 0.01);
        assert!((positions[0].1 - 300.0).abs() < 0.01);
    }

    #[test]
    fn test_empty_nodes_step() {
        let mut layout = ForceLayout::new(ForceLayoutConfig::default());
        let mut nodes: Vec<CanvasNode> = vec![];
        let edges: Vec<CanvasEdge> = vec![];
        let displacement = layout.step(&mut nodes, &edges, 800.0, 600.0);
        assert_eq!(displacement, 0.0);
    }

    #[test]
    fn test_self_loop_edge_ignored() {
        // 自环边应被忽略（from_id == to_id）
        let mut nodes = vec![make_node("a", 100.0, 100.0)];
        let edges = vec![CanvasEdge { from_id: "a".to_string(), to_id: "a".to_string() }];
        let mut layout = ForceLayout::new(ForceLayoutConfig::default());
        let displacement = layout.step(&mut nodes, &edges, 800.0, 600.0);
        // 单节点无外力，位移应极小
        assert!(displacement < 1.0, "单节点自环不应产生位移: {}", displacement);
    }
}
```

- [ ] **Step 4: 运行单元测试验证通过**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo test --lib force_layout`
Expected: 7 个测试全部 PASS

- [ ] **Step 5: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/components/force_layout.rs frontend/src/components/mod.rs
git commit -m "feat(canvas): 新增力导向布局算法（纯函数 + 7 个单元测试）"
```

---

### Task 2: 扩展 wasm-bindgen features 支持 Closure

**Files:**
- Modify: `frontend/Cargo.toml`

- [ ] **Step 1: 确认 wasm-bindgen features**

检查 `frontend/Cargo.toml` 中 wasm-bindgen 的配置。如果当前是：

```toml
wasm-bindgen = "0.2"
```

则改为：

```toml
wasm-bindgen = { version = "0.2", features = ["std"] }
```

如果已经有 features 配置，确保包含 `"std"`（用于 Closure）。实际上 wasm-bindgen 默认启用 std，通常无需修改。执行 `cargo check` 确认无问题即可跳过修改。

- [ ] **Step 2: 验证编译**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: 编译通过

- [ ] **Step 3: 无需 Commit（若 Cargo.toml 无改动）**

若 Cargo.toml 有改动则 commit，否则跳过。

---

### Task 3: CanvasScene 渲染循环 + 力导向集成

**Files:**
- Modify: `frontend/src/components/canvas_scene.rs`

这是阶段 2 的核心改造。将 CanvasScene 从静态渲染升级为持续渲染循环。

- [ ] **Step 1: 在 canvas_scene.rs 顶部追加 use 语句**

在 `frontend/src/components/canvas_scene.rs` 的 `use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};` 之后追加：

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use crate::components::force_layout::{ForceLayout, ForceLayoutConfig, circle_initial_layout};
```

注意：移除文件中已有的 `use wasm_bindgen::JsCast;`（避免重复导入）。

- [ ] **Step 2: 给 CanvasNode 添加 PartialEq 已有，无需改动。在 CanvasSceneProps 添加新字段**

将 `frontend/src/components/canvas_scene.rs` 中的 `CanvasSceneProps` 结构体改为：

```rust
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
}
```

并更新 `Default` 实现：

```rust
impl Default for CanvasSceneProps {
    fn default() -> Self {
        Self {
            width: 800.0,
            height: 600.0,
            nodes: Vec::new(),
            edges: Vec::new(),
            on_node_click: None,
            enable_force_layout: true,
        }
    }
}
```

- [ ] **Step 3: 重写 CanvasScene 组件，引入渲染循环**

将 `frontend/src/components/canvas_scene.rs` 中的 `CanvasScene` 组件函数（从 `#[component] pub fn CanvasScene` 开始到文件末尾）替换为：

```rust
/// CanvasScene 组件：封装 <canvas> 元素 + Context 初始化 + 渲染循环 + 事件桥
///
/// 特性：
/// - 持续渲染循环（request_animation_frame）
/// - 力导向布局自动分布节点
/// - 节点拖拽（mousedown → mousemove → mouseup）
/// - hover 高亮（mousemove 命中检测）
/// - 选中节点光晕
/// - dirty flag 节能：布局稳定时跳过重绘
#[component]
pub fn CanvasScene(props: CanvasSceneProps) -> Element {
    let mut canvas_ref: Signal<Option<HtmlCanvasElement>> = use_signal(|| None);
    let renderer = DefaultRenderer;

    // 内部状态：节点实时位置（从 props 初始化，被力导向和拖拽修改）
    let mut nodes_state: Signal<Vec<CanvasNode>> = use_signal(|| props.nodes.clone());
    let mut force_layout: Signal<ForceLayout> = use_signal(|| {
        ForceLayout::new(ForceLayoutConfig::default())
    });
    let mut is_stable: Signal<bool> = use_signal(|| false);

    // 拖拽状态
    let mut dragging_id: Signal<Option<String>> = use_signal(|| None);
    let mut drag_offset: Signal<(f64, f64)> = use_signal(|| (0.0, 0.0));

    // hover 状态
    let mut hovered_id: Signal<Option<String>> = use_signal(|| None);

    // 选中状态（从外部传入或点击设置）
    let mut selected_id: Signal<Option<String>> = use_signal(|| None);

    // props 变化时同步节点（保留已有位置，新增节点用圆形布局初始化）
    use_effect(use_effect);
    // 注意：上面的 use_effect 是占位，实际在下面 Step 实现

    rsx! {
        canvas {
            width: "{props.width}",
            height: "{props.height}",
            style: "border: 1px solid #e5e7eb; border-radius: 8px; display: block; background: #fafafa; cursor: grab;",
            onmounted: move |evt: MountedEvent| {
                let data = evt.data();
                if let Some(element) = data.downcast::<web_sys::Element>() {
                    let canvas = element.clone().unchecked_into::<HtmlCanvasElement>();
                    canvas_ref.set(Some(canvas));
                }
            },
        }
    }
}
```

注意：上面的组件是骨架，事件和渲染循环在后续 Step 填充。

- [ ] **Step 4: 实现 props 同步 effect**

在 CanvasScene 组件函数体内（`use_effect` 占位处），替换为实际的 props 同步逻辑：

```rust
    // props 变化时同步节点：保留已有节点位置，新增节点用圆形布局初始化
    use_effect(move || {
        let props_nodes = props.nodes.clone();
        let current = nodes_state.read().clone();
        let mut merged: Vec<CanvasNode> = Vec::with_capacity(props_nodes.len());
        for new_node in &props_nodes {
            if let Some(existing) = current.iter().find(|n| n.id == new_node.id) {
                // 保留已有位置，更新其他属性（radius/label/color）
                merged.push(CanvasNode {
                    id: existing.id.clone(),
                    x: existing.x,
                    y: existing.y,
                    radius: new_node.radius,
                    label: new_node.label.clone(),
                    color: new_node.color.clone(),
                });
            } else {
                merged.push(new_node.clone());
            }
        }
        // 新增节点需要初始化位置（如果位置为 0,0，用圆形布局）
        let new_count = merged.iter().filter(|n| n.x == 0.0 && n.y == 0.0).count();
        if new_count > 0 {
            let positions = circle_initial_layout(
                merged.len(),
                props.width / 2.0,
                props.height / 2.0,
                (props.width.min(props.height) / 3.0).max(100.0),
            );
            for (i, node) in merged.iter_mut().enumerate() {
                if node.x == 0.0 && node.y == 0.0 {
                    node.x = positions[i].0;
                    node.y = positions[i].1;
                }
            }
        }
        // 如果节点数量变化，重置稳定状态
        if current.len() != merged.len() {
            is_stable.set(false);
            force_layout.write().sync(merged.len());
        }
        nodes_state.set(merged);
        is_stable.set(false); // props 变化时重新模拟
    });
```

- [ ] **Step 5: 实现渲染循环 effect**

在 CanvasScene 组件函数体内（props 同步 effect 之后），追加渲染循环 effect：

```rust
    // 渲染循环：request_animation_frame 持续运行
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

        // 高清屏适配
        let dpr = web_sys::window()
            .map(|w| w.device_pixel_ratio())
            .unwrap_or(1.0);
        canvas.set_width((props.width * dpr) as u32);
        canvas.set_height((props.height * dpr) as u32);
        let _ = ctx.scale(dpr, dpr);

        // 渲染循环
        let width = props.width;
        let height = props.height;
        let enable_force = props.enable_force_layout;
        let edges = props.edges.clone();

        // 使用 Arc<AtomicBool> 控制循环停止（组件卸载时设为 false）
        let running = std::sync::Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        let mut nodes_state_inner = nodes_state.clone();
        let mut force_layout_inner = force_layout.clone();
        let mut is_stable_inner = is_stable.clone();
        let dragging_id_inner = dragging_id.clone();
        let hovered_id_inner = hovered_id.clone();
        let selected_id_inner = selected_id.clone();

        let renderer = renderer.clone();

        let callback = Closure::<dyn FnMut()>::new(move || {
            // 力导向步进（仅在未稳定且启用时）
            if enable_force && !*is_stable_inner.read() {
                let mut nodes = nodes_state_inner.read().clone();
                let dragging = dragging_id_inner.read().clone();
                let mut layout = force_layout_inner.write();
                let displacement = layout.step(&mut nodes, &edges, width, height);

                // 拖拽中的节点保持鼠标位置（不被力学移动）
                if let Some(drag_id) = &dragging {
                    if let Some(node) = nodes.iter_mut().find(|n| &n.id == drag_id) {
                        // 力学会移动它，但我们会在 mousemove 中纠正位置
                        // 这里先让力学生效，mousemove 会覆盖位置
                    }
                }

                nodes_state_inner.set(nodes);

                // 稳定检测
                if layout.is_stable(displacement, 0.5) {
                    is_stable_inner.set(true);
                }
            }

            // 渲染
            let nodes = nodes_state_inner.read().clone();
            let hovered = hovered_id_inner.read().clone();
            let selected = selected_id_inner.read().clone();
            let dragging = dragging_id_inner.read().clone();

            renderer.clear(&ctx, width, height);
            renderer.draw_edges(&ctx, &edges, &nodes);
            renderer.draw_nodes_with_state(&ctx, &nodes, &hovered, &selected, &dragging);

            // 继续下一帧（如果仍在运行）
            if running_clone.load(Ordering::SeqCst) {
                let _ = web_sys::window()
                    .map(|w| w.request_animation_frame_with_callback(
                        &Closure::<dyn FnMut()>::new({
                            let running_clone2 = running_clone.clone();
                            move || {
                                // 递归调用由外部循环处理，这里不直接递归
                                // 实际的递归通过重新注册实现
                                let _ = running_clone2;
                            }
                        })
                        .as_ref()
                        .unchecked_ref()
                    ));
            }
        });

        // 启动渲染循环（这里需要正确实现递归 rAF，见 Step 6）
        let _ = callback;
    });
```

注意：上面的递归 rAF 实现有问题（Closure 生命周期）。在 Step 6 修正为正确的递归模式。

- [ ] **Step 6: 修正渲染循环为正确的递归 rAF 模式**

由于 Closure 的所有权和递归调用的复杂性，渲染循环需要用"自引用"模式。将 Step 5 的渲染循环 effect 中的 callback 部分替换为正确的递归实现：

将 Step 5 中 `// 渲染循环` 之后到 `let _ = callback;` 之前的代码替换为：

```rust
        // 渲染循环状态
        let running = std::sync::Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        // 用 Box 实现递归 rAF
        let callback_ref: std::rc::Rc<std::cell::RefCell<Option<Closure<dyn FnMut()>>>> =
            std::rc::Rc::new(std::rc::RefCell::new(None));

        let cb_ref_inner = callback_ref.clone();

        let mut nodes_state_c = nodes_state.clone();
        let mut force_layout_c = force_layout.clone();
        let mut is_stable_c = is_stable.clone();
        let dragging_id_c = dragging_id.clone();
        let hovered_id_c = hovered_id.clone();
        let selected_id_c = selected_id.clone();
        let edges_c = edges.clone();
        let renderer_c = renderer.clone();

        let closure = Closure::<dyn FnMut()>::new(move || {
            // 力导向步进
            if enable_force && !*is_stable_c.read() {
                let mut nodes = nodes_state_c.read().clone();
                let mut layout = force_layout_c.write();
                let displacement = layout.step(&mut nodes, &edges_c, width, height);
                nodes_state_c.set(nodes);
                if layout.is_stable(displacement, 0.5) {
                    is_stable_c.set(true);
                }
            }

            // 渲染
            let nodes = nodes_state_c.read().clone();
            let hovered = hovered_id_c.read().clone();
            let selected = selected_id_c.read().clone();
            let dragging = dragging_id_c.read().clone();

            renderer_c.clear(&ctx, width, height);
            renderer_c.draw_edges(&ctx, &edges_c, &nodes);
            renderer_c.draw_nodes_with_state(&ctx, &nodes, &hovered, &selected, &dragging);

            // 递归注册下一帧
            if running_clone.load(Ordering::SeqCst) {
                if let Some(cb) = cb_ref_inner.borrow().as_ref() {
                    let _ = web_sys::window()
                        .map(|w| w.request_animation_frame_with_callback(cb.as_ref().unchecked_ref()));
                }
            }
        });

        // 初始注册
        let _ = web_sys::window()
            .map(|w| w.request_animation_frame_with_callback(closure.as_ref().unchecked_ref()));

        *callback_ref.borrow_mut() = Some(closure);

        // 返回 cleanup：组件卸载时停止循环
        // 注意：use_effect 的返回值作为 cleanup
    });
```

- [ ] **Step 7: 给 CanvasRenderer trait 添加 draw_nodes_with_state 方法**

在 `frontend/src/components/canvas_scene.rs` 的 `CanvasRenderer` trait 定义中追加方法：

```rust
    /// 绘制节点（带交互状态：hover/selected/dragging）
    fn draw_nodes_with_state(
        &self,
        ctx: &CanvasRenderingContext2d,
        nodes: &[CanvasNode],
        hovered: &Option<String>,
        selected: &Option<String>,
        dragging: &Option<String>,
    ) {
        // 默认实现：委托给 draw_nodes（子类可覆盖）
        let _ = (hovered, selected, dragging);
        self.draw_nodes(ctx, nodes);
    }
```

- [ ] **Step 8: 为 DefaultRenderer 实现 draw_nodes_with_state**

在 `frontend/src/components/canvas_scene.rs` 的 `impl CanvasRenderer for DefaultRenderer` 块中追加：

```rust
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

            // 选中光晕（外圈半透明圆）
            if is_selected {
                ctx.set_fill_style_str("rgba(59, 130, 246, 0.2)");
                ctx.begin_path();
                let _ = ctx.arc(node.x, node.y, node.radius + 8.0, 0.0, std::f64::consts::TAU);
                ctx.fill();
            }

            // 拖拽时光晕（更强调）
            if is_dragging {
                ctx.set_fill_style_str("rgba(245, 158, 11, 0.3)");
                ctx.begin_path();
                let _ = ctx.arc(node.x, node.y, node.radius + 12.0, 0.0, std::f64::consts::TAU);
                ctx.fill();
            }

            // hover 放大效果
            let draw_radius = if is_hovered { node.radius * 1.15 } else { node.radius };

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

            // 节点标签
            ctx.set_fill_style_str("white");
            ctx.set_font(if is_hovered { "11px sans-serif" } else { "10px sans-serif" });
            ctx.set_text_align("center");
            ctx.set_text_baseline("middle");
            let label: String = node.label.chars().take(8).collect();
            let _ = ctx.fill_text(&label, node.x, node.y);
        }
    }
```

- [ ] **Step 9: 实现鼠标事件（拖拽 + hover + 点击选中）**

在 `frontend/src/components/canvas_scene.rs` 的 `rsx!` 块的 `canvas` 元素中，添加事件处理器。将整个 `rsx!` 块替换为：

```rust
    rsx! {
        canvas {
            width: "{props.width}",
            height: "{props.height}",
            style: "border: 1px solid #e5e7eb; border-radius: 8px; display: block; background: #fafafa; cursor: grab;",
            onmounted: move |evt: MountedEvent| {
                let data = evt.data();
                if let Some(element) = data.downcast::<web_sys::Element>() {
                    let canvas = element.clone().unchecked_into::<HtmlCanvasElement>();
                    canvas_ref.set(Some(canvas));
                }
            },
            onmousedown: move |e: MouseEvent| {
                let Some(canvas) = canvas_ref.read().clone() else {
                    return;
                };
                let rect = canvas.get_bounding_client_rect();
                let coords = e.client_coordinates();
                let x = coords.x - rect.left();
                let y = coords.y - rect.top();
                let nodes = nodes_state.read().clone();
                if let Some(node_id) = renderer.hit_test(&nodes, x, y) {
                    // 开始拖拽
                    dragging_id.set(Some(node_id.clone()));
                    is_stable.set(false); // 拖拽时重新模拟
                    // 记录偏移（鼠标位置 - 节点位置）
                    if let Some(node) = nodes.iter().find(|n| n.id == node_id) {
                        drag_offset.set((x - node.x, y - node.y));
                    }
                }
            },
            onmousemove: move |e: MouseEvent| {
                let Some(canvas) = canvas_ref.read().clone() else {
                    return;
                };
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
                    // 非拖拽：hover 命中检测
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
                let Some(canvas) = canvas_ref.read().clone() else {
                    return;
                };
                let Some(on_click) = props.on_node_click.as_ref() else {
                    return;
                };
                let rect = canvas.get_bounding_client_rect();
                let coords = e.client_coordinates();
                let x = coords.x - rect.left();
                let y = coords.y - rect.top();
                let nodes = nodes_state.read().clone();
                if let Some(node_id) = renderer.hit_test(&nodes, x, y) {
                    selected_id.set(Some(node_id.clone()));
                    on_click.call(node_id);
                }
            },
        }
    }
```

- [ ] **Step 10: 验证编译**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: 编译通过

如果遇到错误，重点检查：
- Closure 的所有权和生命周期（Rc<RefCell<Option<Closure>>> 模式）
- Signal 的 clone 和 write 操作
- `request_animation_frame_with_callback` 的参数类型（需要 `&js_sys::Function`）
- `Closure::as_ref().unchecked_ref()` 的转换

修复编译错误直到通过。

- [ ] **Step 11: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/components/canvas_scene.rs
git commit -m "feat(canvas): CanvasScene 升级为持续渲染循环 + 力导向 + 拖拽 + hover/选中"
```

---

### Task 4: workspace 页面增强验证

**Files:**
- Modify: `frontend/src/pages/workspace.rs`

- [ ] **Step 1: 增加更多示例节点验证力学效果**

将 `frontend/src/pages/workspace.rs` 中的 `sample_nodes()` 函数替换为：

```rust
/// 生成示例节点数据（模拟 Agent 状态面板）
fn sample_nodes() -> Vec<CanvasNode> {
    vec![
        CanvasNode {
            id: "agent-1".to_string(),
            x: 0.0,  // 初始位置 0,0 会触发圆形布局
            y: 0.0,
            radius: 30.0,
            label: "Agent 1".to_string(),
            color: "#3b82f6".to_string(),
        },
        CanvasNode {
            id: "agent-2".to_string(),
            x: 0.0,
            y: 0.0,
            radius: 25.0,
            label: "Agent 2".to_string(),
            color: "#10b981".to_string(),
        },
        CanvasNode {
            id: "agent-3".to_string(),
            x: 0.0,
            y: 0.0,
            radius: 28.0,
            label: "Agent 3".to_string(),
            color: "#8b5cf6".to_string(),
        },
        CanvasNode {
            id: "tool-1".to_string(),
            x: 0.0,
            y: 0.0,
            radius: 20.0,
            label: "Tool A".to_string(),
            color: "#f59e0b".to_string(),
        },
        CanvasNode {
            id: "tool-2".to_string(),
            x: 0.0,
            y: 0.0,
            radius: 18.0,
            label: "Tool B".to_string(),
            color: "#ef4444".to_string(),
        },
        CanvasNode {
            id: "tool-3".to_string(),
            x: 0.0,
            y: 0.0,
            radius: 16.0,
            label: "Tool C".to_string(),
            color: "#06b6d4".to_string(),
        },
    ]
}

/// 生成示例连线数据
fn sample_edges() -> Vec<CanvasEdge> {
    vec![
        CanvasEdge { from_id: "agent-1".to_string(), to_id: "tool-1".to_string() },
        CanvasEdge { from_id: "agent-1".to_string(), to_id: "tool-2".to_string() },
        CanvasEdge { from_id: "agent-2".to_string(), to_id: "tool-1".to_string() },
        CanvasEdge { from_id: "agent-2".to_string(), to_id: "tool-3".to_string() },
        CanvasEdge { from_id: "agent-3".to_string(), to_id: "tool-2".to_string() },
        CanvasEdge { from_id: "agent-3".to_string(), to_id: "tool-3".to_string() },
    ]
}
```

- [ ] **Step 2: 验证编译**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: 编译通过

- [ ] **Step 3: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/pages/workspace.rs
git commit -m "feat(workspace): 增加 6 节点 6 连线验证力学布局效果"
```

---

### Task 5: 完整验证 + Release 构建

**Files:**
- 无修改，仅验证

- [ ] **Step 1: 运行单元测试**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo test --lib`
Expected: 力导向布局的 7 个测试全部 PASS，无其他测试回归

- [ ] **Step 2: Release 构建**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo build --release`
Expected: 构建成功，warning 数量不超过基线（3 个）

- [ ] **Step 3: 后端测试回归验证**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo test --workspace 2>&1 | grep "test result"`
Expected: 746 passed（无回归）

- [ ] **Step 4: 推送到远程**

```bash
cd /Users/aman/Technology/rust/ai_orz
git push origin main
```

---

## 验证清单

完成所有 Task 后，手动验证以下功能（在浏览器中打开 /workspace，需登录）：

- [ ] 页面加载后节点自动分布（力导向布局生效，节点从圆形展开）
- [ ] 布局逐渐稳定（节点停止移动）
- [ ] 鼠标悬停节点时放大 + 字体变大
- [ ] 按住节点拖拽可移动位置，其他节点随之调整
- [ ] 拖拽时节点有橙色光晕
- [ ] 释放拖拽后布局重新稳定
- [ ] 点击节点触发 toast + 蓝色光晕 + 选中边框
- [ ] 高清屏下节点边缘清晰
- [ ] 现有页面功能无回归

## 阶段 2 完成标志

1. CanvasScene 支持续渲染循环（request_animation_frame）
2. 力导向布局自动分布节点（7 个单元测试覆盖核心算法）
3. 节点可拖拽，拖拽时力学重新模拟
4. hover 节点放大，选中节点光晕 + 边框
5. dirty flag 节能：布局稳定时跳过重绘
6. /workspace 验证 6 节点 6 连线的力学分布
7. 现有功能零回归

## 阶段 3 预告（不在本计划范围）

- WebGL 渲染（wgpu）
- 粒子系统（数据流方向动画）
- 物理引擎（rapier2d）
- pretext 集成（Canvas 文本分行）
- 虚拟化（仅渲染视口内节点，支持 1000+ 节点）
- 知识图谱从 SVG 迁移到 Canvas
