# 统计图表 Phase 2 实施计划：环形图 + Project 任务状态分布

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 Project 详情页概览区域增加 HUD 风格环形图（DonutChart），可视化展示项目下任务的状态分布（已完成/进行中/待处理/已取消/已归档等），让用户一眼看清项目任务健康度。

**Architecture:** 新建 `DonutChart` 组件（`frontend/src/components/charts/donut_chart.rs`），消费 `Vec<DonutSlice>` 通用数据结构，复用 Phase 1 的 `hud_palette` 背景 + rAF 动画模式。组件采用 Canvas 内绘图 + Dioxus 外部图例的职责分离：Canvas 只画环形图，图例用 DaisyUI badge 渲染保证文字清晰。在 Project 详情页概览 Tab 的"项目概览"卡片中，把现有"任务统计"文字网格升级为"环形图 + 图例"组合展示。

**Tech Stack:** Rust + Dioxus 0.7 + web-sys Canvas 2D API（复用 Phase 1 已开启的 features）+ 现有 `task_total/task_completed/...` 本地计算结果

---

## 文件结构

| 文件 | 操作 | 职责 |
|------|------|------|
| `frontend/src/components/charts/donut_chart.rs` | 新建 | DonutChart 组件：HUD 风格环形图 + 图例 |
| `frontend/src/components/charts/mod.rs` | 修改 | 注册 donut_chart 模块 |
| `frontend/src/utils/status.rs` | 修改 | 新增 `task_status_color` 返回 HUD 风格颜色 |
| `frontend/src/pages/project/project_detail.rs` | 修改 | 概览 Tab 的"项目概览"区域集成 DonutChart |

**设计决策：**
- DonutChart 接受通用 `Vec<DonutSlice>`，不绑定任务状态语义，便于未来复用（如 Agent 类型分布、消息渠道分布等）
- 颜色由调用方提供（通过 `task_status_color` 辅助函数），组件不做业务色彩决策
- 图例用 Dioxus + DaisyUI 渲染，避免 Canvas 文字模糊问题
- 中心显示总数 + "任务总数"标签，提供一眼可见的总量信息
- 呼吸光晕动画对齐 LineChart 的 2.4s 周期，保持 HUD 视觉一致
- 空数据（total=0）显示"暂无任务"提示，不绘制环形

---

## Task 1: 实现 DonutChart 组件

**Files:**
- Create: `frontend/src/components/charts/donut_chart.rs`
- Modify: `frontend/src/components/charts/mod.rs`

**目标：** 实现 HUD 风格环形图组件，消费 `Vec<DonutSlice>` 数据，绘制多色扇区 + 中心总数 + 呼吸光晕。图例由组件内部用 Dioxus + DaisyUI 渲染。

- [ ] **Step 1: 创建 donut_chart.rs**

创建 `frontend/src/components/charts/donut_chart.rs`：

```rust
//! HUD 风格环形图组件
//!
//! 消费 `Vec<DonutSlice>` 分类数据，绘制环形图展示各分类占比。
//! 视觉对齐知识图谱 HUD 驾驶舱风格：
//! - 深色径向渐变背景 + 网格 + 四角装饰（复用 hud_palette）
//! - 多色扇区 + shadow_blur 发光 + 扇区间隙
//! - 中心显示总数 + 标签
//! - 外圈呼吸光晕（2.4s 周期，对齐 LineChart）
//!
//! 图例由组件内部用 Dioxus + DaisyUI 渲染（避免 Canvas 文字模糊）。

use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

use crate::components::hud_palette;

/// 环形图单个扇区数据
#[derive(Debug, Clone, PartialEq)]
pub struct DonutSlice {
    /// 分类标签（如 "已完成"）
    pub label: String,
    /// 数值（如 5）
    pub value: u64,
    /// 颜色（hex 格式，如 "#10b981"）
    pub color: String,
}

/// DonutChart 组件 Props
#[derive(Props, Clone, PartialEq)]
pub struct DonutChartProps {
    /// 扇区数据
    pub data: Vec<DonutSlice>,
    /// 画布宽度（CSS 像素），默认 240
    pub width: Option<f64>,
    /// 画布高度（CSS 像素），默认 240
    pub height: Option<f64>,
    /// 中心总数标签（如 "任务总数"）
    pub center_label: Option<String>,
}

/// DonutChart 组件
#[component]
pub fn DonutChart(props: DonutChartProps) -> Element {
    let width = props.width.unwrap_or(240.0);
    let height = props.height.unwrap_or(240.0);
    let center_label = props.center_label.clone();

    let mut canvas_ref: Signal<Option<HtmlCanvasElement>> = use_signal(|| None);

    // 数据缓存：props.data 变化时更新
    let mut data_cache: Signal<Vec<DonutSlice>> = use_signal(|| props.data.clone());
    let props_data = props.data.clone();
    use_effect(move || {
        data_cache.set(props_data.clone());
    });

    // 渲染循环（复用 LineChart 的 rAF + use_drop 模式）
    let render_width = width;
    let render_height = height;
    let center_label_c = center_label.clone();
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
        let center_label_inner = center_label_c.clone();

        // running 标志：组件卸载时设为 false，停止递归 rAF
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let running_clone = running.clone();

        // Rc<RefCell<Option<Closure>>> 模式：Closure 自引用递归 rAF
        let callback_ref: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
        let cb_ref_inner = callback_ref.clone();

        let closure = Closure::<dyn FnMut()>::new(move || {
            let data = data_cache_c.read().clone();
            let now = js_sys::Date::now() / 1000.0;
            draw_chart(&ctx, width, height, &data, now, center_label_inner.as_deref());

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

    // 计算总数用于图例百分比
    let total: u64 = props.data.iter().map(|s| s.value).sum();
    let legend_data = props.data.clone();

    rsx! {
        div { class: "flex items-center gap-4 flex-wrap",
            canvas {
                width: "{width as u32}",
                height: "{height as u32}",
                onmounted: move |evt: MountedEvent| {
                    let data = evt.data();
                    if let Some(element) = data.downcast::<web_sys::Element>() {
                        let canvas = element.clone().unchecked_into::<HtmlCanvasElement>();
                        canvas_ref.set(Some(canvas));
                    }
                },
            }
            // 图例：Dioxus + DaisyUI 渲染（避免 Canvas 文字模糊）
            if !legend_data.is_empty() {
                div { class: "flex flex-col gap-2",
                    for slice in legend_data.iter() {
                        let percent = if total > 0 {
                            (slice.value as f64 / total as f64) * 100.0
                        } else {
                            0.0
                        };
                        div { class: "flex items-center gap-2",
                            span {
                                class: "inline-block w-3 h-3 rounded-full",
                                style: "background-color: {slice.color}; box-shadow: 0 0 6px {slice.color};"
                            }
                            span { class: "text-sm text-base-content/80",
                                "{slice.label}"
                            }
                            span { class: "text-sm font-mono text-base-content",
                                "{slice.value}"
                            }
                            span { class: "text-xs text-base-content/50",
                                "({:.1}%)", percent
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 绘制完整图表（背景 + 环形扇区 + 中心标签）
fn draw_chart(
    ctx: &CanvasRenderingContext2d,
    width: f64,
    height: f64,
    data: &[DonutSlice],
    now: f64,
    center_label: Option<&str>,
) {
    // 1. HUD 背景
    hud_palette::draw_hud_background(ctx, width, height);

    // 2. 计算环形几何参数
    let cx = width / 2.0;
    let cy = height / 2.0;
    let outer_r = width.min(height) / 2.0 - 20.0;
    let inner_r = outer_r * 0.62;

    // 3. 计算总数
    let total: u64 = data.iter().map(|s| s.value).sum();

    // 4. 空数据提示
    if total == 0 {
        ctx.set_fill_style_str("rgba(255, 255, 255, 0.4)");
        ctx.set_font("12px sans-serif");
        ctx.set_text_align("center");
        ctx.set_text_baseline("middle");
        let _ = ctx.fill_text("暂无数据", cx, cy);
        return;
    }

    // 5. 外圈呼吸光晕（2.4s 周期，对齐 LineChart）
    let pulse_period = 2.4;
    let pulse_t = (now % pulse_period) / pulse_period;
    let phase = (pulse_t * std::f64::consts::TAU).sin();
    let glow_alpha = 0.25 + phase * 0.15;
    ctx.set_stroke_style_str(&hud_palette::hex_to_rgba(hud_palette::HUD_PRIMARY, glow_alpha));
    ctx.set_line_width(2.0);
    ctx.begin_path();
    let _ = ctx.arc(cx, cy, outer_r + 6.0, 0.0, std::f64::consts::TAU);
    ctx.stroke();

    // 6. 绘制扇区
    let mut start_angle = -std::f64::consts::FRAC_PI_2; // 从顶部开始
    let gap_angle = 0.02; // 扇区间隙（约 1.15 度）

    for slice in data {
        if slice.value == 0 {
            continue;
        }
        let sweep = (slice.value as f64 / total as f64) * std::f64::consts::TAU;
        let end_angle = start_angle + sweep;

        // 扇区填充（带发光）
        ctx.set_shadow_blur(8.0);
        ctx.set_shadow_color(&slice.color);
        ctx.set_fill_style_str(&slice.color);
        ctx.begin_path();
        ctx.move_to(
            cx + inner_r * start_angle.cos(),
            cy + inner_r * start_angle.sin(),
        );
        let _ = ctx.arc(cx, cy, outer_r, start_angle, end_angle);
        let _ = ctx.arc(cx, cy, inner_r, end_angle, start_angle, true); // 逆时针
        ctx.close_path();
        ctx.fill();

        // 扇区间隙（用一个细的背景色扇区覆盖）
        if sweep > gap_angle * 2.0 {
            ctx.set_shadow_blur(0.0);
            ctx.set_fill_style_str(hud_palette::HUD_BASE_BG);
            ctx.begin_path();
            ctx.move_to(cx, cy);
            let _ = ctx.arc(
                cx,
                cy,
                outer_r + 1.0,
                end_angle - gap_angle / 2.0,
                end_angle + gap_angle / 2.0,
            );
            ctx.close_path();
            ctx.fill();
        }

        start_angle = end_angle;
    }
    ctx.set_shadow_blur(0.0);

    // 7. 中心总数 + 标签
    ctx.set_fill_style_str("white");
    ctx.set_font("bold 24px sans-serif");
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    let _ = ctx.fill_text(&total.to_string(), cx, cy - 8.0);

    if let Some(label) = center_label {
        ctx.set_fill_style_str("rgba(250, 82, 15, 0.8)");
        ctx.set_font("11px sans-serif");
        let _ = ctx.fill_text(label, cx, cy + 14.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_donut_slice_construction() {
        let slice = DonutSlice {
            label: "已完成".to_string(),
            value: 5,
            color: "#10b981".to_string(),
        };
        assert_eq!(slice.label, "已完成");
        assert_eq!(slice.value, 5);
        assert_eq!(slice.color, "#10b981");
    }

    #[test]
    fn test_empty_data_handled() {
        let data: Vec<DonutSlice> = vec![];
        let total: u64 = data.iter().map(|s| s.value).sum();
        assert_eq!(total, 0);
    }

    #[test]
    fn test_total_calculation() {
        let data = vec![
            DonutSlice { label: "A".into(), value: 3, color: "#fff".into() },
            DonutSlice { label: "B".into(), value: 5, color: "#fff".into() },
            DonutSlice { label: "C".into(), value: 2, color: "#fff".into() },
        ];
        let total: u64 = data.iter().map(|s| s.value).sum();
        assert_eq!(total, 10);
    }
}
```

- [ ] **Step 2: 在 charts/mod.rs 注册 donut_chart 模块**

修改 `frontend/src/components/charts/mod.rs`：

```rust
//! 统计图表渲染器集合
pub mod donut_chart;
pub mod line_chart;
```

- [ ] **Step 3: 编译验证**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: 编译通过，无错误

- [ ] **Step 4: 运行单元测试**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo test --bin frontend donut_chart`
Expected: 3 个测试全部通过（test_donut_slice_construction / test_empty_data_handled / test_total_calculation）

- [ ] **Step 5: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/components/charts/donut_chart.rs frontend/src/components/charts/mod.rs
git commit -m "feat: 新增 DonutChart HUD 风格环形图组件"
```

---

## Task 2: 新增 task_status_color 辅助函数

**Files:**
- Modify: `frontend/src/utils/status.rs`

**目标：** 在 `utils/status.rs` 中新增 `task_status_color` 函数，返回任务状态对应的 HUD 风格颜色（hex 字符串），供 DonutChart 调用方构造 `DonutSlice` 使用。颜色方案与现有 `task_status_badge` 的语义对齐（error/warning/info/primary/success/neutral），但改为适配深色背景的鲜艳 hex 值。

- [ ] **Step 1: 在 status.rs 末尾追加 task_status_color 函数**

在 `frontend/src/utils/status.rs` 文件末尾追加：

```rust
/// 任务状态对应的 HUD 风格颜色（hex，适配深色背景）
///
/// 颜色语义与 `task_status_badge` 对齐，但用更鲜艳的 hex 值适配 HUD 深色背景：
/// - 0 已取消：红色 #ef4444
/// - 1 待审核：橙黄 #f59e0b
/// - 2 待处理：蓝色 #3b82f6
/// - 3 进行中：HUD 主色橙 #fa520f
/// - 4 已完成：绿色 #10b981
/// - 5 已归档：灰色 #6b7280
pub fn task_status_color(status: i32) -> &'static str {
    match status {
        0 => "#ef4444",
        1 => "#f59e0b",
        2 => "#3b82f6",
        3 => "#fa520f",
        4 => "#10b981",
        5 => "#6b7280",
        _ => "#6b7280",
    }
}
```

- [ ] **Step 2: 编译验证**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: 编译通过

- [ ] **Step 3: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/utils/status.rs
git commit -m "feat: 新增 task_status_color 返回 HUD 风格任务状态颜色"
```

---

## Task 3: 在 Project 详情页集成 DonutChart

**Files:**
- Modify: `frontend/src/pages/project/project_detail.rs:333-370`

**目标：** 在 Project 详情页概览 Tab 的"项目概览"卡片中，把现有"任务统计"文字网格升级为 DonutChart 环形图 + 图例组合展示。保留"整体进度"区域不变。

**背景：** 当前 `project_detail.rs:246-254` 已经从 `tasks_list` 本地计算了 `task_total/task_completed/task_in_progress/task_pending`。Phase 2 需要扩展为按 6 种状态全量统计（含已取消、待审核、已归档），并构造 `Vec<DonutSlice>` 传给 DonutChart。

- [ ] **Step 1: 修改 project_detail.rs 顶部导入**

在 `frontend/src/pages/project/project_detail.rs` 的 `use crate::components::stats::ProjectStatsPanel;` 下方新增导入：

```rust
use crate::components::charts::donut_chart::{DonutChart, DonutSlice};
use crate::utils::task_status_color;
```

- [ ] **Step 2: 扩展任务状态统计逻辑**

在 `frontend/src/pages/project/project_detail.rs` 中，找到现有的任务统计计算（约 246-254 行）：

```rust
    let overall_progress = if tasks_list.is_empty() {
        0
    } else {
        tasks_list.iter().map(|t| t.progress).sum::<i32>() / tasks_list.len() as i32
    };
    let task_total = tasks_list.len();
    let task_completed = tasks_list.iter().filter(|t| t.status == 4).count();
    let task_in_progress = tasks_list.iter().filter(|t| t.status == 3).count();
    let task_pending = tasks_list.iter().filter(|t| t.status != 3 && t.status != 4 && t.status != 0 && t.status != 5).count();
```

替换为：

```rust
    let overall_progress = if tasks_list.is_empty() {
        0
    } else {
        tasks_list.iter().map(|t| t.progress).sum::<i32>() / tasks_list.len() as i32
    };
    let task_total = tasks_list.len();

    // 按 6 种状态全量统计，构造 DonutChart 数据
    // 顺序：进行中(3) → 待处理(2) → 待审核(1) → 已完成(4) → 已归档(5) → 已取消(0)
    // 把"进行中"放最前让 HUD 主色橙最显眼，"已完成"绿色紧跟其后
    let task_status_counts: [(i32, &str); 6] = [
        (3, "进行中"),
        (2, "待处理"),
        (1, "待审核"),
        (4, "已完成"),
        (5, "已归档"),
        (0, "已取消"),
    ];
    let donut_slices: Vec<DonutSlice> = task_status_counts
        .iter()
        .map(|(status, label)| {
            let count = tasks_list.iter().filter(|t| t.status == *status).count() as u64;
            DonutSlice {
                label: label.to_string(),
                value: count,
                color: task_status_color(*status).to_string(),
            }
        })
        .filter(|s| s.value > 0) // 过滤掉 0 值状态，避免图例冗余
        .collect();
```

- [ ] **Step 3: 修改"项目概览"卡片，用 DonutChart 替换任务统计文字网格**

在 `frontend/src/pages/project/project_detail.rs` 中，找到"区域 2：项目概览统计"（约 333-370 行）：

```rust
                    // 区域 2：项目概览统计
                    div { class: "card bg-base-100 shadow-md",
                        div { class: "card-header",
                            h2 { class: "card-title", "项目概览" }
                        }
                        div { class: "overview-grid",
                            div { class: "overview-item",
                                div { class: "overview-label", "整体进度" }
                                div { class: "overview-progress",
                                    div { class: "overview-progress-bar",
                                        div { class: "{progress_bar_class(overall_progress)}", style: "width: {overall_progress}%;" }
                                    }
                                    span { class: "overview-progress-text", "{overall_progress}%" }
                                }
                            }
                            div { class: "overview-item",
                                div { class: "overview-label", "任务统计" }
                                div { class: "overview-stats",
                                    div { class: "overview-stat-item",
                                        span { class: "overview-stat-value", "{task_total}" }
                                        span { class: "overview-stat-label", "总数" }
                                    }
                                    div { class: "overview-stat-item",
                                        span { class: "overview-stat-value success", "{task_completed}" }
                                        span { class: "overview-stat-label", "完成" }
                                    }
                                    div { class: "overview-stat-item",
                                        span { class: "overview-stat-value primary", "{task_in_progress}" }
                                        span { class: "overview-stat-label", "进行中" }
                                    }
                                    div { class: "overview-stat-item",
                                        span { class: "overview-stat-value warning", "{task_pending}" }
                                        span { class: "overview-stat-label", "待处理" }
                                    }
                                }
                            }
                        }
                    }
```

替换为：

```rust
                    // 区域 2：项目概览统计
                    div { class: "card bg-base-100 shadow-md",
                        div { class: "card-header",
                            h2 { class: "card-title", "项目概览" }
                        }
                        div { class: "overview-grid",
                            div { class: "overview-item",
                                div { class: "overview-label", "整体进度" }
                                div { class: "overview-progress",
                                    div { class: "overview-progress-bar",
                                        div { class: "{progress_bar_class(overall_progress)}", style: "width: {overall_progress}%;" }
                                    }
                                    span { class: "overview-progress-text", "{overall_progress}%" }
                                }
                            }
                            div { class: "overview-item",
                                div { class: "overview-label", "任务状态分布" }
                                if donut_slices.is_empty() {
                                    div { class: "text-base-content/60 text-sm py-8 text-center",
                                        "暂无任务"
                                    }
                                } else {
                                    DonutChart {
                                        data: donut_slices.clone(),
                                        width: Some(240.0),
                                        height: Some(240.0),
                                        center_label: Some("任务总数".to_string()),
                                    }
                                }
                            }
                        }
                    }
```

- [ ] **Step 4: 编译验证**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check`
Expected: 编译通过，无错误，无 `task_completed/task_in_progress/task_pending` 未使用变量警告

- [ ] **Step 5: 启动开发服务器视觉验证**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && dx serve --port 8080`

打开浏览器访问任意有任务的项目详情页，切换到"概览"Tab，确认：
1. "任务状态分布"区域显示环形图 + 右侧图例
2. 环形图中心显示任务总数 + "任务总数"标签
3. 各扇区颜色与图例一致，扇区有发光效果
4. 外圈有呼吸光晕动画（2.4s 周期）
5. HUD 背景（深色径向渐变 + 网格 + 四角装饰）与折线图一致
6. 无任务的项目显示"暂无任务"文字提示

- [ ] **Step 6: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/pages/project/project_detail.rs
git commit -m "feat: Project 概览页集成 DonutChart 展示任务状态分布"
```

---

## Task 4: 运行全量测试 + 更新文档

**Files:**
- Modify: `AGENTS.md`

**目标：** 运行前后端全量测试确保无回归，更新 AGENTS.md 记录 Phase 2 里程碑。

- [ ] **Step 1: 运行前端全量测试**

Run: `cd /Users/aman/Technology/rust/ai_orz/frontend && cargo test --bin frontend`
Expected: 所有测试通过（含新增的 3 个 donut_chart 测试）

- [ ] **Step 2: 运行后端全量测试**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo test --lib`
Expected: 746 个测试全部通过（Phase 2 不涉及后端改动，应无回归）

- [ ] **Step 3: 运行 common 全量测试**

Run: `cd /Users/aman/Technology/rust/ai_orz && cargo test -p common`
Expected: 50 个测试全部通过

- [ ] **Step 4: 更新 AGENTS.md 里程碑记录**

在 `AGENTS.md` 的"## 六、工作流与开发记录"章节，找到最新的 `### 2026-07-25 里程碑` 标题，在其下方（在已有 Phase 1 里程碑之后）新增 Phase 2 里程碑条目。

注意：`### 2026-07-25 里程碑` 标题下当前已有"知识图谱 Canvas HUD 驾驶舱风格 + 聊天共享组件抽取 + utils 模块化"和"统计图表 Phase 1"等内容。在 Phase 1 相关内容之后追加 Phase 2 子标题。

在 Phase 1 描述块结束后追加：

```markdown
**✅ 统计图表 Phase 2：Project 任务状态分布环形图（donut_chart）**
- **DonutChart 组件**：新增 `frontend/src/components/charts/donut_chart.rs`，消费通用 `Vec<DonutSlice>` 数据结构，绘制 HUD 风格环形图（深色径向渐变背景 + 多色扇区 shadow_blur 发光 + 扇区间隙 + 外圈呼吸光晕 2.4s 周期 + 中心总数标签）
- **图例职责分离**：Canvas 只画环形图，图例由 Dioxus + DaisyUI 渲染（彩色圆点 + 标签 + 数值 + 百分比），避免 Canvas 文字模糊
- **task_status_color 辅助函数**：`utils/status.rs` 新增 `task_status_color(status: i32) -> &'static str`，返回 6 种状态对应的 HUD 风格鲜艳颜色（红 #ef4444 / 橙黄 #f59e0b / 蓝 #3b82f6 / HUD 主色橙 #fa520f / 绿 #10b981 / 灰 #6b7280）
- **Project 详情页集成**：概览 Tab 的"项目概览"卡片中，把原"任务统计"文字网格升级为 DonutChart + 图例组合展示；按 6 种状态全量统计（进行中→待处理→待审核→已完成→已归档→已取消），过滤 0 值状态避免图例冗余；无任务时显示"暂无任务"提示
- **测试统计**：前端 38 测试（新增 3 个 donut_chart 测试）+ 后端 746 测试 + common 50 测试 100% 通过，总计 834 测试
```

- [ ] **Step 5: 更新 AGENTS.md 测试统计**

在 `AGENTS.md` 的"### 1.3 整体完成度与测试统计"表格中，更新测试统计数值：

找到表格行：
```
| **总测试数** | **831** | 后端 746 + 前端 35 + common 50，DAO + DAL + Domain + Handler + Pkg 完整覆盖 |
```

替换为：
```
| **总测试数** | **834** | 后端 746 + 前端 38 + common 50，DAO + DAL + Domain + Handler + Pkg 完整覆盖 |
```

- [ ] **Step 6: 提交文档更新**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add AGENTS.md
git commit -m "docs: 记录统计图表 Phase 2 里程碑（DonutChart 环形图）"
```

---

## Self-Review 检查

**1. Spec 覆盖：**
- ✅ HUD 风格环形图组件 → Task 1
- ✅ 任务状态颜色辅助函数 → Task 2
- ✅ Project 详情页集成 → Task 3
- ✅ 测试 + 文档 → Task 4

**2. Placeholder 扫描：** 无 TODO/TBD/占位符，所有步骤含完整代码

**3. Type 一致性：**
- `DonutSlice { label: String, value: u64, color: String }` 在 Task 1 定义，Task 3 构造时字段名一致 ✅
- `DonutChartProps { data, width, height, center_label }` 在 Task 1 定义，Task 3 调用时 prop 名一致 ✅
- `task_status_color(status: i32) -> &'static str` 在 Task 2 定义，Task 3 调用签名一致 ✅
- `donut_slices` 变量名在 Task 3 Step 2 定义，Step 3 引用一致 ✅
