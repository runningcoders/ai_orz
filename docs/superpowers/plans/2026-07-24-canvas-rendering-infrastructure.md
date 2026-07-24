# Canvas 渲染基础设施实施计划（阶段 1）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 建立 Dioxus ↔ Canvas 2D 桥接层，让 Canvas 成为可用的渲染目标，并通过 /workspace 工作台页面验证事件桥/渲染循环/性能，完全独立不影响现有页面。

**Architecture:** 在 Dioxus 0.7 + web-sys 基础上，封装 `<canvas>` 元素为可复用的 `CanvasScene` 组件。通过 `CanvasRenderer` trait 抽象渲染逻辑（clear/draw_node/draw_edge/hit_test），通过 `use_effect` 初始化 Canvas 2D Context，通过 `request_animation_frame` + dirty flag 实现按需重绘，通过鼠标事件桥接 Canvas 坐标到 Dioxus 状态更新。试点场景是 /workspace 工作台页面，渲染 Agent 状态节点 + 实时数据流连线，验证基础设施可行性。

**Tech Stack:** Dioxus 0.7.9, web-sys 0.3.103（扩展 Canvas features）, wasm-bindgen 0.2, js-sys 0.3

---

## 文件结构

| 文件 | 责任 | 操作 |
|------|------|------|
| `frontend/Cargo.toml` | 扩展 web-sys features | 修改 |
| `frontend/src/components/canvas_scene.rs` | CanvasScene 组件 + CanvasRenderer trait + 事件桥 + 渲染循环 | 新建 |
| `frontend/src/components/mod.rs` | 注册 canvas_scene 模块 | 修改 |
| `frontend/src/pages/workspace.rs` | /workspace 工作台页面（试点场景） | 新建 |
| `frontend/src/pages/mod.rs` | 注册 workspace 模块 + 新增路由 | 修改 |
| `frontend/src/layouts/navbar.rs` | 导航栏加入工作台入口 | 修改 |

---

### Task 1: 扩展 web-sys features

**Files:**
- Modify: `frontend/Cargo.toml:16`

- [ ] **Step 1: 扩展 web-sys features 列表**

在 `frontend/Cargo.toml` 第 16 行的 web-sys features 数组中，追加 Canvas 相关 features。将现有的 features 列表末尾的 `MediaQueryListEvent"]"` 改为：

```toml
web-sys = { version = "0.3.103", features = ["Window", "Performance", "Storage", "EventSource", "MessageEvent", "Event", "Request", "RequestInit", "RequestCredentials", "Response", "FormData", "HtmlInputElement", "FileList", "File", "Blob", "BlobPropertyBag", "HtmlElement", "Navigator", "Clipboard", "MediaQueryList", "MediaQueryListEvent", "HtmlCanvasElement", "CanvasRenderingContext2d", "Element", "DomRect", "CanvasGradient", "CanvasRenderingContext2dHelpers"] }
```

- [ ] **Step 2: 验证编译通过**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: 编译通过（features 解析成功）

- [ ] **Step 3: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/Cargo.toml
git commit -m "chore(frontend): 扩展 web-sys features 支持 Canvas 2D 渲染"
```

---

### Task 2: 创建 CanvasRenderer trait + 基础数据结构

**Files:**
- Create: `frontend/src/components/canvas_scene.rs`
- Modify: `frontend/src/components/mod.rs`

- [ ] **Step 1: 创建 canvas_scene.rs 文件，定义 CanvasRenderer trait 和基础数据结构**

在 `frontend/src/components/canvas_scene.rs` 中写入：

```rust
//! Canvas 场景渲染基础设施
//!
//! 提供 Dioxus ↔ Canvas 2D 桥接层：
//! - CanvasScene 组件封装 <canvas> 元素 + Context 初始化
//! - CanvasRenderer trait 抽象渲染逻辑（由业务场景实现）
//! - 事件桥：鼠标事件 → 坐标转换 → 命中检测 → Dioxus callback
//! - 渲染循环：request_animation_frame + dirty flag 按需重绘

use dioxus::prelude::*;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

/// Canvas 渲染节点（通用数据结构，业务场景填充字段）
#[derive(Debug, Clone, Default)]
pub struct CanvasNode {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub radius: f64,
    pub label: String,
    pub color: String,
}

/// Canvas 渲染连线
#[derive(Debug, Clone, Default)]
pub struct CanvasEdge {
    pub from_id: String,
    pub to_id: String,
}

/// Canvas 渲染器 trait：业务场景实现此 trait 定义渲染逻辑
pub trait CanvasRenderer {
    /// 清空画布（通常由 CanvasScene 调用 ctx.clear_rect 后调用）
    fn clear(&self, ctx: &CanvasRenderingContext2d, width: f64, height: f64);

    /// 绘制所有节点
    fn draw_nodes(&self, ctx: &CanvasRenderingContext2d, nodes: &[CanvasNode]);

    /// 绘制所有连线
    fn draw_edges(&self, ctx: &CanvasRenderingContext2d, edges: &[CanvasEdge], nodes: &[CanvasNode]);

    /// 命中检测：给定画布坐标，返回命中的节点 ID（ None 表示空白处）
    fn hit_test(&self, nodes: &[CanvasNode], x: f64, y: f64) -> Option<String>;
}

/// 默认渲染器：基础圆形节点 + 直线连线
pub struct DefaultRenderer;

impl CanvasRenderer for DefaultRenderer {
    fn clear(&self, ctx: &CanvasRenderingContext2d, width: f64, height: f64) {
        ctx.clear_rect(0.0, 0.0, width, height);
    }

    fn draw_nodes(&self, ctx: &CanvasRenderingContext2d, nodes: &[CanvasNode]) {
        for node in nodes {
            // 节点圆形
            ctx.set_fill_style(&node.color.clone().into());
            ctx.begin_path();
            let _ = ctx.arc(node.x, node.y, node.radius, 0.0, std::f64::consts::TAU);
            ctx.fill();

            // 节点标签
            ctx.set_fill_style(&"white".into());
            ctx.set_font("10px sans-serif");
            ctx.set_text_align("center");
            ctx.set_text_baseline("middle");
            let label: String = node.label.chars().take(8).collect();
            let _ = ctx.fill_text(&label, node.x, node.y);
        }
    }

    fn draw_edges(&self, ctx: &CanvasRenderingContext2d, edges: &[CanvasEdge], nodes: &[CanvasNode]) {
        ctx.set_stroke_style(&"rgba(107, 114, 128, 0.4)".into());
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
        // 从后往前遍历（后绘制的在上层）
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
```

- [ ] **Step 2: 在 components/mod.rs 注册 canvas_scene 模块**

在 `frontend/src/components/mod.rs` 末尾追加：

```rust
pub mod canvas_scene;
```

- [ ] **Step 3: 验证编译通过**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: 编译通过

- [ ] **Step 4: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/components/canvas_scene.rs frontend/src/components/mod.rs
git commit -m "feat(canvas): 新增 CanvasRenderer trait 与基础数据结构"
```

---

### Task 3: 实现 CanvasScene 组件（canvas 元素 + context 初始化）

**Files:**
- Modify: `frontend/src/components/canvas_scene.rs`

- [ ] **Step 1: 在 canvas_scene.rs 末尾追加 CanvasScene 组件**

在 `frontend/src/components/canvas_scene.rs` 末尾追加：

```rust
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
///
/// 使用方式：
/// ```ignore
/// rsx! {
///     CanvasScene {
///         width: 800.0,
///         height: 600.0,
///         nodes: nodes(),
///         edges: edges(),
///         on_node_click: move |id: String| { /* 处理点击 */ }
///     }
/// }
/// ```
#[component]
pub fn CanvasScene(props: CanvasSceneProps) -> Element {
    let canvas_ref: Signal<Option<HtmlCanvasElement>> = use_signal(|| None);
    let renderer = DefaultRenderer;

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

        // 设置 Canvas 物理像素 = CSS 像素 * devicePixelRatio（高清屏适配）
        let dpr = web_sys::window()
            .and_then(|w| w.device_pixel_ratio())
            .unwrap_or(1.0);
        canvas.set_width((props.width * dpr) as u32);
        canvas.set_height((props.height * dpr) as u32);
        let _ = ctx.scale(dpr, dpr);

        // 初始渲染
        renderer.clear(&ctx, props.width, props.height);
        renderer.draw_edges(&ctx, &props.edges, &props.nodes);
        renderer.draw_nodes(&ctx, &props.nodes);
    });

    rsx! {
        canvas {
            width: "{props.width}",
            height: "{props.height}",
            style: "border: 1px solid #e5e7eb; border-radius: 8px; display: block; background: #fafafa;",
            onmounted: move |evt: MountedEvent| {
                let canvas = evt.element().to_owned().unchecked_into::<HtmlCanvasElement>();
                canvas_ref.set(Some(canvas));
            },
            onclick: move |e: MouseEvent| {
                let Some(canvas) = canvas_ref.read().clone() else {
                    return;
                };
                let Some(on_click) = &props.on_node_click else {
                    return;
                };
                // 坐标转换：屏幕坐标 → Canvas 坐标
                let rect = canvas.get_bounding_client_rect();
                let x = e.client_x() as f64 - rect.left();
                let y = e.client_y() as f64 - rect.top();
                // 命中检测
                if let Some(node_id) = renderer.hit_test(&props.nodes, x, y) {
                    on_click.call(node_id);
                }
            },
        }
    }
}
```

- [ ] **Step 2: 在文件顶部追加 use 语句**

在 `frontend/src/components/canvas_scene.rs` 的 `use dioxus::prelude::*;` 之后追加：

```rust
use wasm_bindgen::JsCast;
```

- [ ] **Step 3: 验证编译通过**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: 编译通过

- [ ] **Step 4: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/components/canvas_scene.rs
git commit -m "feat(canvas): 实现 CanvasScene 组件（canvas 元素 + context 初始化 + 事件桥）"
```

---

### Task 4: 创建工作台页面 /workspace

**Files:**
- Create: `frontend/src/pages/workspace.rs`
- Modify: `frontend/src/pages/mod.rs`

- [ ] **Step 1: 创建 workspace.rs 页面文件**

在 `frontend/src/pages/workspace.rs` 中写入：

```rust
//! 工作台页面（Canvas 渲染基础设施试点）
//!
//! 验证 CanvasScene 组件的：
//! - Canvas 2D Context 初始化
//! - 节点/连线渲染
//! - 鼠标事件桥接 + 命中检测
//! - 高清屏适配

use dioxus::prelude::*;

use crate::components::canvas_scene::{CanvasEdge, CanvasNode, CanvasScene};
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;

/// 生成示例节点数据（模拟 Agent 状态面板）
fn sample_nodes() -> Vec<CanvasNode> {
    vec![
        CanvasNode {
            id: "agent-1".to_string(),
            x: 200.0,
            y: 150.0,
            radius: 30.0,
            label: "Agent 1".to_string(),
            color: "#3b82f6".to_string(),
        },
        CanvasNode {
            id: "agent-2".to_string(),
            x: 500.0,
            y: 150.0,
            radius: 25.0,
            label: "Agent 2".to_string(),
            color: "#10b981".to_string(),
        },
        CanvasNode {
            id: "tool-1".to_string(),
            x: 350.0,
            y: 350.0,
            radius: 20.0,
            label: "Tool A".to_string(),
            color: "#f59e0b".to_string(),
        },
    ]
}

/// 生成示例连线数据
fn sample_edges() -> Vec<CanvasEdge> {
    vec![
        CanvasEdge { from_id: "agent-1".to_string(), to_id: "tool-1".to_string() },
        CanvasEdge { from_id: "agent-2".to_string(), to_id: "tool-1".to_string() },
    ]
}

#[component]
pub fn Workspace() -> Element {
    let nodes = sample_nodes();
    let edges = sample_edges();
    let mut selected_id = use_signal(|| None::<String>);
    let toast = use_toast();

    rsx! {
        AppLayout {
            div { class: "card bg-base-100 shadow-md",
                div { class: "card-body",
                    h2 { class: "card-title mb-2", "🚀 工作台（Canvas 试点）" }
                    p { class: "text-sm text-base-content/70 mb-4",
                        "验证 Canvas 渲染基础设施：节点渲染、连线绘制、点击事件桥接。"
                    }

                    // Canvas 场景
                    div { class: "flex justify-center",
                        CanvasScene {
                            width: 800.0,
                            height: 500.0,
                            nodes: nodes.clone(),
                            edges: edges.clone(),
                            on_node_click: move |id: String| {
                                selected_id.set(Some(id.clone()));
                                toast.info(&format!("点击节点: {id}"));
                            }
                        }
                    }

                    // 选中节点信息
                    if let Some(id) = &*selected_id.read() {
                        div { class: "alert alert-info mt-4",
                            span { "当前选中: {id}" }
                        }
                    } else {
                        div { class: "text-sm text-base-content/50 mt-4",
                            "点击 Canvas 中的节点查看交互效果"
                        }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 2: 在 pages/mod.rs 注册 workspace 模块**

在 `frontend/src/pages/mod.rs` 第 11 行 `pub mod user;` 之后追加：

```rust
pub mod workspace;
```

- [ ] **Step 3: 在 pages/mod.rs 导入 Workspace 组件**

在 `frontend/src/pages/mod.rs` 的 `use crate::pages::user::profile::UserProfile;` 之后追加：

```rust
use crate::pages::workspace::Workspace;
```

- [ ] **Step 4: 在 pages/mod.rs Route 枚举中新增 /workspace 路由**

在 `frontend/src/pages/mod.rs` 的 Route 枚举中，`#[route("/settings")]` 之前追加：

```rust
    // 工作台（Canvas 试点）
    #[route("/workspace")]
    Workspace {},

```

- [ ] **Step 5: 验证编译通过**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: 编译通过

- [ ] **Step 6: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/pages/workspace.rs frontend/src/pages/mod.rs
git commit -m "feat(workspace): 新增 /workspace 工作台页面（Canvas 试点）"
```

---

### Task 5: 导航栏加入工作台入口

**Files:**
- Modify: `frontend/src/layouts/navbar.rs`

- [ ] **Step 1: 在桌面导航加入工作台链接**

在 `frontend/src/layouts/navbar.rs` 中，找到 `Link { to: Route::MessageSearch {}, class: "btn btn-ghost btn-sm text-neutral-content", "🔍 消息搜索" }` 这一行，在其后追加：

```rust
                    Link { to: Route::Workspace {}, class: "btn btn-ghost btn-sm text-neutral-content", "🚀 工作台" }
```

- [ ] **Step 2: 在移动端抽屉菜单加入工作台链接**

在 `frontend/src/layouts/navbar.rs` 中，找到移动端抽屉的导航项分组（搜索 `对话` 或 `Route::MessageChat` 在移动端菜单中的位置），在"对话"导航项之后追加：

```rust
                            Link { to: Route::Workspace {}, class: "btn btn-ghost btn-sm text-base-content w-full text-left", onclick: move |_| drawer_open.set(false), "🚀 工作台" }
```

- [ ] **Step 3: 验证编译通过**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: 编译通过

- [ ] **Step 4: Commit**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/layouts/navbar.rs
git commit -m "feat(navbar): 导航栏新增工作台入口"
```

---

### Task 6: 完整验证 + Release 构建

**Files:**
- 无修改，仅验证

- [ ] **Step 1: 执行 release 构建验证**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo build --release`
Expected: 构建成功，无新增 error

- [ ] **Step 2: 检查无新增 warning**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo build --release 2>&1 | grep "^warning" | wc -l`
Expected: 不超过现有 warning 数量（当前基线 3 个）

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

完成所有 Task 后，手动验证以下功能（在浏览器中打开 /workspace）：

- [ ] Canvas 正确渲染 3 个示例节点（2 个 Agent + 1 个 Tool）
- [ ] 节点间连线正确绘制（Agent → Tool）
- [ ] 高清屏下节点边缘清晰（无模糊）
- [ ] 点击节点触发 toast 提示 + 底部显示选中 ID
- [ ] 点击空白处无反应
- [ ] 导航栏"🚀 工作台"入口可跳转
- [ ] 现有页面（对话/知识图谱/Agent 列表等）功能无回归

---

## 阶段 1 完成标志

1. CanvasScene 组件可复用（任何场景传入 nodes/edges/on_node_click 即可渲染）
2. CanvasRenderer trait 抽象清晰（业务场景可实现自定义渲染）
3. 事件桥正常工作（鼠标点击 → 坐标转换 → 命中检测 → 回调）
4. 高清屏适配（devicePixelRatio 处理）
5. /workspace 试点页面可交互
6. 现有功能零回归

## 阶段 2 预告（不在本计划范围）

- 渲染循环优化（request_animation_frame + dirty flag 按需重绘）
- 力导向布局
- 节点动画（hover/选中/淡入）
- pretext 集成（Canvas 文本分行）
- 虚拟化（仅渲染视口内节点）
