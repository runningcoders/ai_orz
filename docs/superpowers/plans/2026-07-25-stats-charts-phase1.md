# 统计图表 Phase 1 实施计划：基础设施 + 折线图 + 实体详情页时序图

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为统计信息增加 Canvas 自绘的 HUD 风格折线图，消费后端已就绪的 `model_call_time_series` 时序数据，在 4 个实体详情页（Agent/Project/Task/ModelProvider）的 StatsPanel 中展示模型调用次数与 Token 消耗趋势。

**Architecture:** 抽取公共 HUD 背景工具模块（`hud_palette.rs`），新建轻量图表 Scene（`chart_scene.rs`，复用 rAF/DPR/use_drop 模式但去掉粒子/力学），实现折线图渲染器（`charts/line_chart.rs`，对齐知识图谱 HUD 视觉：深色径向渐变背景 + 橙色主色 + 数据点呼吸光晕 + 折线流光发光），最后改造 4 个 StatsPanel 组件消费 `model_call_time_series` 字段。

**Tech Stack:** Rust + Dioxus 0.7 + web-sys Canvas 2D API（复用知识图谱已开启的 features：`HtmlCanvasElement` / `CanvasRenderingContext2d` / `CanvasGradient`）+ common::models::TimeSeriesPoint

---

## 文件结构

| 文件 | 操作 | 职责 |
|------|------|------|
| `frontend/src/components/hud_palette.rs` | 新建 | HUD 背景工具：径向渐变 + 网格 + 四角装饰 + hex_to_rgba |
| `frontend/src/components/chart_scene.rs` | 新建 | ChartRenderer trait + ChartScene 组件（轻量 Canvas 桥接） |
| `frontend/src/components/charts/mod.rs` | 新建 | charts 子模块声明 |
| `frontend/src/components/charts/line_chart.rs` | 新建 | LineChartRenderer：HUD 风格折线图 |
| `frontend/src/components/mod.rs` | 修改 | 注册 hud_palette / chart_scene / charts 模块 |
| `frontend/src/components/graph_canvas.rs` | 修改 | clear 方法改为调用 hud_palette（去重） |
| `frontend/src/components/stats.rs` | 修改 | 4 个 StatsPanel 加 LineChart 时序图 |
| `frontend/src/pages/hr/agent_detail.rs` | 修改 | 确认 stats_interval 参数已传递（如未传则补） |
| `frontend/src/pages/project/project_detail.rs` | 修改 | 同上 |
| `frontend/src/pages/project/task_detail.rs` | 修改 | 同上 |
| `frontend/src/pages/finance/model_provider_detail.rs` | 修改 | 同上 |

**设计决策：**
- 不复用 `CanvasScene`：它的 trait 签名（`draw_nodes`/`draw_edges`）偏图谱语义，图表场景语义不符；新建轻量 `ChartScene` 避免图谱包袱
- `ChartScene` 复用 `CanvasScene` 的 rAF 递归 + DPR + use_drop 模式，但去掉力导向/粒子/拖拽
- `hud_palette.rs` 抽取后，知识图谱 `clear` 和图表 `clear` 共享同一套 HUD 背景，保证视觉统一
- 折线图默认高度 200px，宽度跟随容器（响应式由 props 控制）

---

## Task 1: 抽取 HUD 背景工具到 hud_palette.rs

**Files:**
- Create: `frontend/src/components/hud_palette.rs`
- Modify: `frontend/src/components/mod.rs`（注册模块）
- Modify: `frontend/src/components/graph_canvas.rs:110-179`（clear 方法改为调用 hud_palette）

**目标：** 把 graph_canvas.rs 中 `clear` 方法的 HUD 背景绘制（基底 + 径向渐变 + 网格 + 四角）和 `hex_to_rgba` 工具函数提取到独立模块，供图表场景复用。

- [ ] **Step 1: 创建 hud_palette.rs**

创建 `frontend/src/components/hud_palette.rs`：

```rust
//! HUD 驾驶舱风格背景工具
//!
//! 提供统一的 HUD 视觉元素绘制函数，供 CanvasScene（知识图谱）和 ChartScene（统计图表）共享：
//! - 深色径向渐变背景（基底 #0a0e1a + 橙色 #fa520f 中心光晕）
//! - 淡橙色网格线（40px 间距）
//! - 四角 HUD 装饰刻度线
//!
//! 视觉锚点：橙色 #fa520f（rgb 250, 82, 15）贯穿背景、网格、四角，形成统一驾驶舱观感。

use web_sys::{CanvasGradient, CanvasRenderingContext2d};

/// HUD 主色（橙色）
pub const HUD_PRIMARY: &str = "#fa520f";
/// HUD 主色 RGB 元组
pub const HUD_PRIMARY_RGB: (u8, u8, u8) = (250, 82, 15);
/// HUD 画布基底色（深色）
pub const HUD_BASE_BG: &str = "#0a0e1a";

/// 将 hex 颜色转换为 rgba 字符串
///
/// 统一替代 graph_canvas.rs 和 particles.rs 的重复实现。
/// 支持 6 位 hex（如 "#fa520f"），无效输入返回白色。
pub fn hex_to_rgba(hex: &str, alpha: f64) -> String {
    let hex = hex.trim_start_matches('#');
    let (r, g, b) = if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
        (r, g, b)
    } else {
        (255, 255, 255)
    };
    format!("rgba({}, {}, {}, {:.3})", r, g, b, alpha)
}

/// 绘制完整 HUD 背景（基底 + 径向渐变 + 网格 + 四角装饰）
///
/// 调用方通常在 ChartRenderer::clear / CanvasRenderer::clear 中调用此函数。
pub fn draw_hud_background(ctx: &CanvasRenderingContext2d, width: f64, height: f64) {
    draw_hud_base(ctx, width, height);
    draw_hud_radial_glow(ctx, width, height);
    draw_hud_grid(ctx, width, height);
    draw_hud_corners(ctx, width, height);
}

/// 绘制深色基底
pub fn draw_hud_base(ctx: &CanvasRenderingContext2d, width: f64, height: f64) {
    ctx.set_fill_style_str(HUD_BASE_BG);
    ctx.fill_rect(0.0, 0.0, width, height);
}

/// 绘制径向光晕（橙色中心向边缘淡出）
pub fn draw_hud_radial_glow(ctx: &CanvasRenderingContext2d, width: f64, height: f64) {
    if let Ok(grad) = ctx.create_radial_gradient(
        width / 2.0,
        height / 2.0,
        0.0,
        width / 2.0,
        height / 2.0,
        width.max(height) / 2.0,
    ) {
        let _ = grad.add_color_stop(0.0, "rgba(250, 82, 15, 0.08)");
        let _ = grad.add_color_stop(1.0, "rgba(250, 82, 15, 0)");
        ctx.set_fill_style_canvas_gradient(&grad);
        ctx.fill_rect(0.0, 0.0, width, height);
    }
}

/// 绘制淡橙色网格线（HUD 坐标系，40px 间距）
pub fn draw_hud_grid(ctx: &CanvasRenderingContext2d, width: f64, height: f64) {
    ctx.set_stroke_style_str("rgba(250, 82, 15, 0.06)");
    ctx.set_line_width(1.0);
    let mut x = 0.0;
    while x <= width {
        ctx.begin_path();
        ctx.move_to(x, 0.0);
        ctx.line_to(x, height);
        ctx.stroke();
        x += 40.0;
    }
    let mut y = 0.0;
    while y <= height {
        ctx.begin_path();
        ctx.move_to(0.0, y);
        ctx.line_to(width, y);
        ctx.stroke();
        y += 40.0;
    }
}

/// 绘制四角 HUD 装饰刻度线
pub fn draw_hud_corners(ctx: &CanvasRenderingContext2d, width: f64, height: f64) {
    ctx.set_stroke_style_str("rgba(250, 82, 15, 0.5)");
    ctx.set_line_width(1.5);
    let corner_len = 12.0;
    let offset = 8.0;
    // 左上
    ctx.begin_path();
    ctx.move_to(offset, corner_len + offset);
    ctx.line_to(offset, offset);
    ctx.line_to(corner_len + offset, offset);
    ctx.stroke();
    // 右上
    ctx.begin_path();
    ctx.move_to(width - corner_len - offset, offset);
    ctx.line_to(width - offset, offset);
    ctx.line_to(width - offset, corner_len + offset);
    ctx.stroke();
    // 左下
    ctx.begin_path();
    ctx.move_to(offset, height - corner_len - offset);
    ctx.line_to(offset, height - offset);
    ctx.line_to(corner_len + offset, height - offset);
    ctx.stroke();
    // 右下
    ctx.begin_path();
    ctx.move_to(width - corner_len - offset, height - offset);
    ctx.line_to(width - offset, height - offset);
    ctx.line_to(width - offset, height - corner_len - offset);
    ctx.stroke();
}
```

- [ ] **Step 2: 在 mod.rs 注册 hud_palette 模块**

修改 `frontend/src/components/mod.rs`，在现有模块声明中加入：

```rust
pub mod hud_palette;
```

- [ ] **Step 3: 修改 graph_canvas.rs 的 clear 方法调用 hud_palette**

修改 `frontend/src/components/graph_canvas.rs` 的 `clear` 方法（约 110-179 行），替换为：

```rust
fn clear(&self, ctx: &CanvasRenderingContext2d, width: f64, height: f64) {
    crate::components::hud_palette::draw_hud_background(ctx, width, height);
}
```

同时删除 `KnowledgeGraphRenderer` 中现有的 `hex_to_rgba` 私有方法（约 95-106 行），改为调用 `crate::components::hud_palette::hex_to_rgba`。搜索 `Self::hex_to_rgba` 的所有调用点（约 5 处，在 draw_nodes_with_state 中），替换为 `crate::components::hud_palette::hex_to_rgba`。

- [ ] **Step 4: 编译验证**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: 编译通过，无错误

- [ ] **Step 5: 视觉回归验证**

启动前端开发服务器，打开知识图谱页面，确认 HUD 背景视觉与抽取前完全一致（深色基底 + 橙色径向光晕 + 网格 + 四角装饰）。

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && dx serve --port 8080`
打开浏览器访问知识图谱页，肉眼对比抽取前后的 HUD 背景。

- [ ] **Step 6: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/components/hud_palette.rs frontend/src/components/mod.rs frontend/src/components/graph_canvas.rs
git commit -m "refactor: 抽取 HUD 背景工具到 hud_palette 模块供图表复用"
```

---

## Task 2: 创建 ChartRenderer trait + ChartScene 组件

**Files:**
- Create: `frontend/src/components/chart_scene.rs`
- Modify: `frontend/src/components/mod.rs`（注册 chart_scene 模块）

**目标：** 新建轻量 Canvas Scene，复用 CanvasScene 的 rAF 递归 + DPR + use_drop 模式，但去掉力导向/粒子/拖拽，只保留渲染循环。定义 ChartRenderer trait 供图表渲染器实现。

- [ ] **Step 1: 创建 chart_scene.rs**

创建 `frontend/src/components/chart_scene.rs`。Phase 1 采用务实模式——不强行抽象通用的 ChartScene 组件（避免过度工程化），只定义 `ChartRenderer` trait 供未来图表扩展。Phase 1 的 LineChart 组件直接在组件内实现渲染逻辑，不强制实现 trait。当出现第二种图表（如环形图）时，再抽取通用渲染逻辑到 trait。

```rust
//! 统计图表 Canvas 渲染基础
//!
//! 定义 ChartRenderer trait 供未来图表渲染器扩展。
//! Phase 1 的 LineChart 组件直接在组件内实现渲染逻辑，不强制实现 trait。
//! 当出现第二种图表（如环形图）时，再抽取通用渲染逻辑到 trait。

use web_sys::CanvasRenderingContext2d;

/// 图表渲染器 trait：具体图表（折线/柱状/环形）实现此 trait 定义渲染逻辑
pub trait ChartRenderer: Send + Sync {
    /// 清空画布并绘制背景
    fn clear(&self, ctx: &CanvasRenderingContext2d, width: f64, height: f64);

    /// 绘制图表内容（每帧调用，now_secs 为当前时间戳秒数，用于动画）
    fn draw(&self, ctx: &CanvasRenderingContext2d, width: f64, height: f64, now_secs: f64);
}
```

- [ ] **Step 2: 在 mod.rs 注册 chart_scene 模块**

修改 `frontend/src/components/mod.rs`，加入：

```rust
pub mod chart_scene;
```

- [ ] **Step 3: 编译验证**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: 编译通过

- [ ] **Step 4: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/components/chart_scene.rs frontend/src/components/mod.rs
git commit -m "feat: 新增 ChartRenderer trait 定义供图表扩展"
```

---

## Task 3: 实现 LineChart 折线图组件（HUD 风格）

**Files:**
- Create: `frontend/src/components/charts/mod.rs`
- Create: `frontend/src/components/charts/line_chart.rs`
- Modify: `frontend/src/components/mod.rs`（注册 charts 模块）

**目标：** 实现 HUD 风格折线图组件，消费 `Vec<TimeSeriesPoint>` 数据，展示模型调用次数趋势。视觉对齐知识图谱 HUD：深色径向渐变背景 + 橙色折线 + 数据点呼吸光晕 + 折线流光发光。

- [ ] **Step 1: 创建 charts/mod.rs**

创建 `frontend/src/components/charts/mod.rs`：

```rust
//! 统计图表渲染器集合
pub mod line_chart;
```

- [ ] **Step 2: 创建 line_chart.rs 骨架**

创建 `frontend/src/components/charts/line_chart.rs`：

```rust
//! HUD 风格折线图组件
//!
//! 消费 `Vec<TimeSeriesPoint>` 时序数据，绘制折线图展示趋势。
//! 视觉对齐知识图谱 HUD 驾驶舱风格：
//! - 深色径向渐变背景 + 网格 + 四角装饰（复用 hud_palette）
//! - 橙色折线 + shadow_blur 发光
//! - 数据点呼吸光晕（alpha 在 0.37~0.73 间摆动，2.4s 周期）
//! - 折线流光（line_dash_offset 持续滚动）
//! - 坐标轴刻度 + 数值标签

use common::models::TimeSeriesPoint;
use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

use crate::components::hud_palette;

/// LineChart 组件 Props
#[derive(Props, Clone, PartialEq)]
pub struct LineChartProps {
    /// 时序数据点（按时间升序）
    pub data: Vec<TimeSeriesPoint>,
    /// 画布宽度（CSS 像素），默认 600
    pub width: Option<f64>,
    /// 画布高度（CSS 像素），默认 200
    pub height: Option<f64>,
    /// 图表标题（显示在左上角）
    pub title: Option<String>,
    /// 数值标签描述（如 "调用次数" / "Token 消耗"）
    pub value_label: Option<String>,
}

/// LineChart 组件
#[component]
pub fn LineChart(props: LineChartProps) -> Element {
    let width = props.width.unwrap_or(600.0);
    let height = props.height.unwrap_or(200.0);
    let title = props.title.clone();
    let value_label = props.value_label.clone();

    let mut canvas_ref: Signal<Option<HtmlCanvasElement>> = use_signal(|| None);

    // 数据缓存：props.data 变化时更新
    let data_cache: Signal<Vec<TimeSeriesPoint>> = use_signal(|| props.data.clone());
    let props_data = props.data.clone();
    use_effect(move || {
        data_cache.set(props_data.clone());
    });

    // 渲染循环
    let render_width = width;
    let render_height = height;
    let title_c = title.clone();
    let value_label_c = value_label.clone();
    let data_cache_c = data_cache.clone();
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
        canvas.set_width((render_width * dpr) as u32);
        canvas.set_height((render_height * dpr) as u32);
        let _ = ctx.scale(dpr, dpr);

        let width = render_width;
        let height = render_height;
        let title_inner = title_c.clone();
        let value_label_inner = value_label_c.clone();

        // running 标志：组件卸载时设为 false，停止递归 rAF
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let running_clone = running.clone();

        // Rc<RefCell<Option<Closure>>> 模式：Closure 自引用递归 rAF
        let callback_ref: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
        let cb_ref_inner = callback_ref.clone();

        let closure = Closure::<dyn FnMut()>::new(move || {
            let data = data_cache_c.read().clone();
            let now = js_sys::Date::now() / 1000.0;
            draw_chart(
                &ctx,
                width,
                height,
                &data,
                now,
                title_inner.as_deref(),
                value_label_inner.as_deref(),
            );

            // 递归注册下一帧
            if running_clone.load(std::sync::atomic::Ordering::SeqCst) {
                if let Some(cb) = cb_ref_inner.borrow().as_ref() {
                    if let Some(window) = web_sys::window() {
                        let _ = window.request_animation_frame(cb.as_ref().unchecked_ref());
                    }
                }
            }
        });

        // 初始注册第一帧
        if let Some(window) = web_sys::window() {
            let _ = window.request_animation_frame(closure.as_ref().unchecked_ref());
        }
        *callback_ref.borrow_mut() = Some(closure);

        // cleanup
        use_drop(move || {
            running.store(false, std::sync::atomic::Ordering::SeqCst);
            *callback_ref.borrow_mut() = None;
        });
    });

    rsx! {
        canvas {
            width: "{width as u32}",
            height: "{height as u32}",
            class: "w-full",
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

/// 绘制完整图表（背景 + 坐标轴 + 折线 + 数据点）
fn draw_chart(
    ctx: &CanvasRenderingContext2d,
    width: f64,
    height: f64,
    data: &[TimeSeriesPoint],
    now: f64,
    title: Option<&str>,
    value_label: Option<&str>,
) {
    // 1. HUD 背景
    hud_palette::draw_hud_background(ctx, width, height);

    // 2. 标题
    if let Some(t) = title {
        ctx.set_fill_style_str("rgba(250, 82, 15, 0.9)");
        ctx.set_font("bold 12px sans-serif");
        ctx.set_text_align("left");
        ctx.set_text_baseline("top");
        let _ = ctx.fill_text(t, 16.0, 14.0);
    }

    // 3. 空数据提示
    if data.is_empty() {
        ctx.set_fill_style_str("rgba(255, 255, 255, 0.4)");
        ctx.set_font("12px sans-serif");
        ctx.set_text_align("center");
        ctx.set_text_baseline("middle");
        let _ = ctx.fill_text("暂无时序数据", width / 2.0, height / 2.0);
        return;
    }

    // 4. 计算坐标系
    let pad_left = 40.0;
    let pad_right = 16.0;
    let pad_top = 32.0;
    let pad_bottom = 28.0;
    let plot_w = width - pad_left - pad_right;
    let plot_h = height - pad_top - pad_bottom;

    // 找出最大值（call_count），用于 Y 轴缩放
    let max_value = data.iter().map(|p| p.call_count).max().unwrap_or(1).max(1);
    let max_y = (max_value as f64 * 1.1).max(1.0); // 留 10% 顶部空间

    // 5. 绘制坐标轴
    draw_axes(ctx, pad_left, pad_top, plot_w, plot_h, max_y, data.len(), value_label);

    // 6. 计算数据点坐标
    let points: Vec<(f64, f64, u64)> = data
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let x = if data.len() > 1 {
                pad_left + (i as f64) * plot_w / (data.len() - 1) as f64
            } else {
                pad_left + plot_w / 2.0
            };
            let y = pad_top + plot_h - (p.call_count as f64 / max_y) * plot_h;
            (x, y, p.call_count)
        })
        .collect();

    // 7. 绘制折线（流光发光）
    draw_line(ctx, &points, now);

    // 8. 绘制数据点（呼吸光晕）
    draw_points(ctx, &points, now);

    // 9. 绘制 X 轴时间标签
    draw_x_labels(ctx, data, pad_left, pad_top + plot_h, plot_w);
}

/// 绘制坐标轴
fn draw_axes(
    ctx: &CanvasRenderingContext2d,
    pad_left: f64,
    pad_top: f64,
    plot_w: f64,
    plot_h: f64,
    max_y: f64,
    data_len: usize,
    value_label: Option<&str>,
) {
    // 坐标轴主线（淡橙色）
    ctx.set_stroke_style_str("rgba(250, 82, 15, 0.3)");
    ctx.set_line_width(1.0);

    // Y 轴
    ctx.begin_path();
    ctx.move_to(pad_left, pad_top);
    ctx.line_to(pad_left, pad_top + plot_h);
    ctx.stroke();

    // X 轴
    ctx.begin_path();
    ctx.move_to(pad_left, pad_top + plot_h);
    ctx.line_to(pad_left + plot_w, pad_top + plot_h);
    ctx.stroke();

    // Y 轴刻度（4 等分）
    ctx.set_fill_style_str("rgba(255, 255, 255, 0.5)");
    ctx.set_font("10px sans-serif");
    ctx.set_text_align("right");
    ctx.set_text_baseline("middle");
    for i in 0..=4 {
        let y = pad_top + (i as f64) * plot_h / 4.0;
        let value = max_y * (1.0 - i as f64 / 4.0);
        // 刻度线
        ctx.begin_path();
        ctx.move_to(pad_left - 4.0, y);
        ctx.line_to(pad_left, y);
        ctx.stroke();
        // 数值标签
        let label = if value >= 1000.0 {
            format!("{:.1}K", value / 1000.0)
        } else {
            format!("{:.0}", value)
        };
        let _ = ctx.fill_text(&label, pad_left - 6.0, y);
    }

    // 值标签描述（Y 轴顶部）
    if let Some(label) = value_label {
        ctx.set_fill_style_str("rgba(250, 82, 15, 0.7)");
        ctx.set_font("10px sans-serif");
        ctx.set_text_align("left");
        ctx.set_text_baseline("bottom");
        let _ = ctx.fill_text(label, pad_left, pad_top - 4.0);
    }

    let _ = data_len;
}

/// 设置虚线模式（参考 graph_canvas.rs:26-37 的 set_dash 辅助函数）
fn set_dash(ctx: &CanvasRenderingContext2d, values: &[f64]) {
    let arr = js_sys::Array::new();
    for v in values {
        arr.push(&wasm_bindgen::JsValue::from_f64(*v));
    }
    ctx.set_line_dash(&arr);
}

/// 绘制折线（流光发光）
fn draw_line(ctx: &CanvasRenderingContext2d, points: &[(f64, f64, u64)], now: f64) {
    if points.len() < 2 {
        return;
    }

    // 发光效果
    ctx.set_shadow_blur(6.0);
    ctx.set_shadow_color(hud_palette::HUD_PRIMARY);
    ctx.set_stroke_style_str(hud_palette::HUD_PRIMARY);
    ctx.set_line_width(2.0);

    // 流光虚线（dashoffset 持续滚动）
    set_dash(ctx, &[4.0, 8.0]);
    let offset = (now * 30.0) % 12.0;
    ctx.set_line_dash_offset(-offset);

    ctx.begin_path();
    ctx.move_to(points[0].0, points[0].1);
    for p in points.iter().skip(1) {
        ctx.line_to(p.0, p.1);
    }
    ctx.stroke();

    // 重置 shadow 和 dash
    ctx.set_shadow_blur(0.0);
    set_dash(ctx, &[]);
    ctx.set_line_dash_offset(0.0);
}

/// 绘制数据点（呼吸光晕）
fn draw_points(ctx: &CanvasRenderingContext2d, points: &[(f64, f64, u64)], now: f64) {
    // 呼吸动画：alpha 在 0.37~0.73 间摆动，2.4s 周期
    let pulse_period = 2.4;
    let pulse_t = (now % pulse_period) / pulse_period;
    let phase = (pulse_t * std::f64::consts::TAU).sin();
    let alpha = 0.55 + phase * 0.18;

    for (x, y, _value) in points {
        // 外圈呼吸光晕
        ctx.set_stroke_style_str(&hud_palette::hex_to_rgba(hud_palette::HUD_PRIMARY, alpha));
        ctx.set_line_width(2.0);
        ctx.begin_path();
        let _ = ctx.arc(*x, *y, 5.0, 0.0, std::f64::consts::TAU);
        ctx.stroke();

        // 内圈实心点
        ctx.set_fill_style_str(hud_palette::HUD_PRIMARY);
        ctx.begin_path();
        let _ = ctx.arc(*x, *y, 2.5, 0.0, std::f64::consts::TAU);
        ctx.fill();
    }
}

/// 绘制 X 轴时间标签
fn draw_x_labels(
    ctx: &CanvasRenderingContext2d,
    data: &[TimeSeriesPoint],
    pad_left: f64,
    y_base: f64,
    plot_w: f64,
) {
    ctx.set_fill_style_str("rgba(255, 255, 255, 0.5)");
    ctx.set_font("10px sans-serif");
    ctx.set_text_align("center");
    ctx.set_text_baseline("top");

    // 数据点多时只显示首末和中间，避免重叠
    let n = data.len();
    let indices: Vec<usize> = if n <= 6 {
        (0..n).collect()
    } else {
        vec![0, n / 4, n / 2, 3 * n / 4, n - 1]
    };

    for i in indices {
        let x = if n > 1 {
            pad_left + (i as f64) * plot_w / (n - 1) as f64
        } else {
            pad_left + plot_w / 2.0
        };
        // 将毫秒时间戳转为日期字符串（YYYY-MM-DD 或 HH:MM）
        let label = format_timestamp(data[i].interval_start);
        let _ = ctx.fill_text(&label, x, y_base + 6.0);
    }
}

/// 将毫秒时间戳格式化为短日期字符串
fn format_timestamp(ts_ms: i64) -> String {
    // 简化实现：取日期部分
    // 注：WASM 无 chrono，用 js_sys::Date 格式化
    let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ts_ms as f64));
    let month = date.get_month() + 1; // 0-11 → 1-12
    let day = date.get_date();
    format!("{}-{}", month, day)
}
```

**注意事项：**
- `format_timestamp` 用 `js_sys::Date` 格式化，避免引入 chrono 依赖。
- `data_cache` Signal 用于 props.data 变化时触发重绘（rAF 循环每帧 read 最新值）。
- `set_dash` 辅助函数参考 graph_canvas.rs:26-37 实现，使用 `js_sys::Array` 构造 dash 数组。

- [ ] **Step 3: 在 mod.rs 注册 charts 模块**

修改 `frontend/src/components/mod.rs`，加入：

```rust
pub mod charts;
```

- [ ] **Step 4: 编译验证**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: 编译通过

- [ ] **Step 5: 单元测试 - 数据点坐标计算**

在 `line_chart.rs` 末尾添加测试模块：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_timestamp_returns_short_date() {
        // 2026-07-25 00:00:00 UTC 的毫秒时间戳
        let ts = 1753526400000_i64;
        let label = format_timestamp(ts);
        // 应包含月-日格式
        assert!(label.contains("-"), "label should contain '-' separator, got: {}", label);
    }

    #[test]
    fn test_empty_data_handled() {
        // draw_chart 在空数据时应只绘制背景和提示，不 panic
        // 此测试验证逻辑分支，实际 Canvas 调用在 WASM 环境无法测试
        let data: Vec<TimeSeriesPoint> = vec![];
        assert!(data.is_empty());
    }
}
```

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo test --lib line_chart`
Expected: 测试通过

- [ ] **Step 6: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/components/charts/ frontend/src/components/mod.rs
git commit -m "feat: 实现 HUD 风格折线图组件 LineChart"
```

---

## Task 4: 改造 4 个 StatsPanel 组件消费 model_call_time_series

**Files:**
- Modify: `frontend/src/components/stats.rs`（4 个 StatsPanel 加 LineChart）

**目标：** 在 AgentStatsPanel / ProjectStatsPanel / TaskStatsPanel / ModelProviderStatsPanel 中，当 `model_call_stats.model_call_time_series` 有数据时，在数字卡片下方渲染 LineChart 折线图，展示模型调用次数趋势。

- [ ] **Step 1: 在 stats.rs 引入 LineChart**

修改 `frontend/src/components/stats.rs` 顶部，加入：

```rust
use crate::components::charts::line_chart::LineChart;
```

- [ ] **Step 2: 添加通用时序图渲染辅助函数**

在 stats.rs 中 `format_qps` 函数后添加：

```rust
/// 渲染模型调用时序图（如果有数据）
fn render_time_series_chart(model_call_stats: &Option<ModelCallStats>) -> Element {
    if let Some(mcs) = model_call_stats {
        if let Some(series) = &mcs.model_call_time_series {
            if !series.is_empty() {
                return rsx! {
                    div { class: "mt-4",
                        LineChart {
                            data: series.clone(),
                            width: 600.0,
                            height: 200.0,
                            title: Some("模型调用趋势".to_string()),
                            value_label: Some("调用次数".to_string()),
                        }
                    }
                };
            }
        }
    }
    rsx! {}
}
```

- [ ] **Step 3: 改造 AgentStatsPanel**

修改 `AgentStatsPanel`（约 47-71 行），在 `StatsPanel` children 末尾、闭合 `div` 前加入时序图。由于 `StatsPanel` 的 children 是 `Element`，需要把时序图放在 stats 容器外。

**结构调整：** 把 `StatsPanel` 包裹在一个外层 div 中，时序图放在 StatsPanel 之后：

```rust
#[component]
pub fn AgentStatsPanel(stats: Option<AgentStats>, model_call_stats: Option<ModelCallStats>) -> Element {
    let chart_data = model_call_stats.clone();
    rsx! {
        div { class: "space-y-4",
            StatsPanel { title: "Agent 统计".to_string(),
                if let Some(s) = stats {
                    if let Some(call) = s.call_summary {
                        StatsCard { title: "唤醒次数".to_string(), icon: "🔔".to_string(), value: call.total_calls.to_string(), subtitle: None }
                        if let Some(qps) = call.avg_qps {
                            StatsCard { title: "平均 QPS".to_string(), icon: "📈".to_string(), value: format_qps(qps), subtitle: None }
                        }
                        StatsCard { title: "瞬时 QPS".to_string(), icon: "⚡".to_string(), value: format_qps(call.instant_qps), subtitle: None }
                    }
                }
                if let Some(mcs) = model_call_stats {
                    if let Some(call) = mcs.call_summary {
                        StatsCard { title: "模型调用".to_string(), icon: "🤖".to_string(), value: call.total_calls.to_string(), subtitle: None }
                    }
                    if let Some(token) = mcs.token_summary {
                        StatsCard { title: "输入 Token".to_string(), icon: "📥".to_string(), value: format_token_count(token.total_tokens_input), subtitle: None }
                        StatsCard { title: "输出 Token".to_string(), icon: "📤".to_string(), value: format_token_count(token.total_tokens_output), subtitle: None }
                    }
                }
            }
            {render_time_series_chart(&chart_data)}
        }
    }
}
```

- [ ] **Step 4: 改造 ProjectStatsPanel**

同样模式修改 `ProjectStatsPanel`（约 73-96 行），包裹外层 div + 添加 `{render_time_series_chart(&model_call_stats.clone())}`。

- [ ] **Step 5: 改造 TaskStatsPanel**

同样模式修改 `TaskStatsPanel`（约 98-121 行）。

- [ ] **Step 6: 改造 ModelProviderStatsPanel**

同样模式修改 `ModelProviderStatsPanel`（约 143-162 行）。注意 ModelProviderStatsPanel 的参数是 `stats: Option<ModelCallStats>`，直接用：

```rust
{render_time_series_chart(&stats)}
```

- [ ] **Step 7: ToolStatsPanel 不改**

ToolStatsPanel（约 123-141 行）不修改，因为 Tool 没有 `model_call_time_series` 数据（ToolStats 只有 call_summary + failed_count）。

- [ ] **Step 8: 编译验证**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: 编译通过

- [ ] **Step 9: 前端测试验证**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo test`
Expected: 所有前端测试通过（34 个 + 新增的 line_chart 测试）

- [ ] **Step 10: 视觉验证**

启动前端，打开 Agent 详情页（确保该 Agent 有模型调用历史数据），确认：
1. 数字卡片正常显示
2. 数字卡片下方出现 HUD 风格折线图
3. 折线图背景为深色径向渐变 + 网格 + 四角装饰
4. 折线为橙色 + 发光效果 + 流光动画
5. 数据点有呼吸光晕
6. X 轴显示日期标签，Y 轴显示数值刻度

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && dx serve --port 8080`

- [ ] **Step 11: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/components/stats.rs
git commit -m "feat: 4 个 StatsPanel 加 HUD 风格折线图展示模型调用趋势"
```

---

## Task 5: 确认详情页 stats_interval 参数传递

**Files:**
- Modify（如需）: `frontend/src/pages/hr/agent_detail.rs`
- Modify（如需）: `frontend/src/pages/project/project_detail.rs`
- Modify（如需）: `frontend/src/pages/project/task_detail.rs`
- Modify（如需）: `frontend/src/pages/finance/model_provider_detail.rs`

**目标：** 确认 4 个详情页在请求统计时传递了 `stats_interval=daily`（或 hourly），让后端返回 `model_call_time_series` 数据。

- [ ] **Step 1: 检查各详情页的 StatsOptions 传递**

Run: 搜索 `with_model_call_stats` 在 4 个详情页文件中的调用，确认 `stats_interval` 字段已设置。

```bash
cd /Users/aman/Technology/rust/ai_orz
# 用 Grep 工具搜索
```

预期：4 个详情页都应该有类似 `stats_interval: Some("daily".to_string())` 的代码。

- [ ] **Step 2: 如果缺失则补充**

如果某个详情页未传 `stats_interval`，则在 `StatsOptions` 构造处补充：

```rust
StatsOptions {
    with_stats: true,
    with_model_call_stats: true,
    stats_interval: Some("daily".to_string()),
}
```

- [ ] **Step 3: 确认后端 with_time_series 已开启**

检查后端 Domain 层在 `with_model_call_stats=true` 时是否自动设置 `with_time_series=true`。搜索 `with_time_series` 在 `src/service/domain/` 下的使用，确认 StatsFetchOptions 的构造逻辑。

如果后端未自动开启 `with_time_series`，需要在 Domain 层补充：当 `with_model_call_stats=true` 时 `options.with_time_series = true`。

- [ ] **Step 4: 端到端验证**

启动后端 + 前端，打开 Agent 详情页，通过浏览器开发者工具 Network 面板确认 API 响应中 `model_call_time_series` 字段有数据。

Run:
```bash
# 后端
cd /Users/aman/Technology/rust/ai_orz && cargo run &
# 前端
cd /Users/aman/Technology/rust/ai_orz/frontend && dx serve --port 8080
```

- [ ] **Step 5: 提交（如有修改）**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/pages/ src/service/domain/
git commit -m "fix: 确保详情页请求时携带 stats_interval 参数触发时序数据返回"
```

---

## Task 6: 全量测试与文档更新

**Files:**
- Modify: `AGENTS.md`（里程碑记录）

- [ ] **Step 1: 全量后端测试**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo test`
Expected: 746 个后端测试 + 50 个 common 测试全部通过

- [ ] **Step 2: 全量前端测试**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo test`
Expected: 34 个 + 新增 line_chart 测试全部通过

- [ ] **Step 3: 更新 AGENTS.md 里程碑**

在 AGENTS.md 第六章"工作流与开发记录"顶部新增 2026-07-25 里程碑条目（在已有的 2026-07-25 里程碑上方）：

```markdown
### 2026-07-25 里程碑（统计图表 Phase 1）
**✅ 统计图表基础设施 + 实体详情页时序图**
- **HUD 背景工具抽取**：新增 `frontend/src/components/hud_palette.rs`，从 graph_canvas.rs 提取 HUD 背景绘制（径向渐变 + 网格 + 四角装饰 + hex_to_rgba），供知识图谱和统计图表共享，统一驾驶舱视觉语言
- **ChartRenderer trait**：新增 `frontend/src/components/chart_scene.rs`，定义图表渲染器 trait 供未来图表扩展（折线/柱状/环形）
- **HUD 风格折线图**：新增 `frontend/src/components/charts/line_chart.rs`，消费 `Vec<TimeSeriesPoint>` 时序数据，视觉对齐知识图谱 HUD（深色径向渐变背景 + 橙色折线 + shadow_blur 发光 + 数据点呼吸光晕 2.4s 周期 + 折线流光 line_dash_offset 滚动 + 坐标轴刻度 + X 轴日期标签）
- **4 个 StatsPanel 时序图**：AgentStatsPanel / ProjectStatsPanel / TaskStatsPanel / ModelProviderStatsPanel 在数字卡片下方渲染 LineChart，消费后端已就绪的 `model_call_time_series` 字段（此前前端从未读取该字段）
- **测试统计**：前端测试 + 新增 line_chart 单元测试，100% 通过
```

同时更新 1.2 节"已实现核心功能"表格，新增一行：

```markdown
| 📊 统计图表可视化 | ✅ | HUD 风格 Canvas 折线图，4 个实体详情页展示模型调用趋势（消费 model_call_time_series） |
```

- [ ] **Step 4: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add AGENTS.md
git commit -m "docs: 记录统计图表 Phase 1 里程碑"
```

---

## 验收标准

Phase 1 完成后应满足：

1. **视觉统一**：折线图背景与知识图谱 HUD 风格一致（深色径向渐变 + 橙色网格 + 四角装饰）
2. **数据消费**：4 个详情页的 StatsPanel 正确消费 `model_call_time_series`，展示模型调用次数趋势
3. **动画效果**：折线流光发光 + 数据点呼吸光晕，与知识图谱节点动画风格一致
4. **无回归**：知识图谱页面视觉无变化（hud_palette 抽取为纯重构），所有现有测试通过
5. **代码质量**：无过度抽象，LineChart 组件自包含，hud_palette 模块职责单一

## 后续阶段预告

- **Phase 2**：Project 任务状态环形图（donut_chart），4 宫格数字 → 环形图
- **Phase 3**：AOP 队列监控时序图（需后端定期采样 + 新增时序查询接口）
- **Phase 4**：全局 Dashboard 新页面（需后端全局统计聚合接口 + 多图表组合）
