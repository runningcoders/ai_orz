# Canvas 改造扩展（高/中优先级页面）实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 health/logs/triggers/agent_detail/tasks/workspace 六个页面 canvas 化，并把 AopGauge 抽象为通用 Gauge 组件复用

**Architecture:**
- 先抽象通用 `Gauge` 组件（去掉 AOP 专属字段，保留状态颜色编码 + 呼吸光晕 + 选中边框），AopGauge 改为薄 wrap
- health.rs 用 Gauge 仪表盘墙展示系统指标（CPU/内存/磁盘/DB连接/AOP队列深度/活跃Agent数/SSE连接数）
- logs.rs 顶部加日志级别分布 DonutChart + 24h 日志量时序 LineChart（新增后端聚合 API）
- triggers.rs 加触发器状态分布 DonutChart + 最近触发时间轴（用 last_run_at/next_run_at 字段，无需新 API）
- agent_detail.rs 在现有 LineChart 基础上补充工具调用分布 DonutChart
- tasks.rs 看板视图改为 HUD 风格 KanbanCanvas（新组件）
- workspace.rs 底部加消息流量时序 LineChart（前端 SSE 本地累积）

**Tech Stack:** Dioxus 0.7.9 (WebAssembly) + Tailwind CSS v4 + DaisyUI v5 + wasm-bindgen + web-sys Canvas API

---

## 文件结构

### 新建文件
| 路径 | 职责 |
|------|------|
| `frontend/src/components/gauge.rs` | 通用 Gauge 仪表盘组件（从 AopGauge 抽象） |
| `frontend/src/components/kanban_canvas.rs` | 看板视图 canvas 组件（任务泳道） |
| `src/handlers/system/logs/log_stats.rs` | 日志统计聚合后端 handler |
| `common/src/api/log_stats.rs` | 日志统计 API DTO |
| `frontend/src/api/log_stats.rs` | 前端日志统计 API 客户端 |

### 修改文件
| 路径 | 改动 |
|------|------|
| `frontend/src/components/mod.rs` | 注册 gauge + kanban_canvas |
| `frontend/src/components/aop_gauge.rs` | 改为 wrap Gauge + AOP 专属聚合 |
| `frontend/src/pages/system/health.rs` | 全量重写为仪表盘墙 |
| `frontend/src/pages/system/logs.rs` | 顶部加图表区 |
| `frontend/src/pages/system/triggers.rs` | 加状态分布图 |
| `frontend/src/pages/hr/agent_detail.rs` | 统计 Tab 补充 DonutChart |
| `frontend/src/pages/project/tasks.rs` | 看板视图换 KanbanCanvas |
| `frontend/src/pages/workspace.rs` | 底部加消息流量时序图 |
| `frontend/src/api/system.rs` | 新增 health_metrics + trigger_stats 客户端 |
| `src/handlers/system/mod.rs` | 注册 log_stats 模块 |
| `src/handlers/system/logs/mod.rs` | 注册 log_stats 子模块 |
| `src/service/domain/system/mod.rs` | 新增 LogStats + HealthMetrics 子能力 |
| `common/src/api/mod.rs` | 注册 log_stats 模块 |
| `common/src/api/cron_trigger.rs` | 追加 TriggerStatsAggregation（前端本地聚合用） |

---

## Task 1: 通用 Gauge 组件抽象

**Files:**
- Create: `frontend/src/components/gauge.rs`
- Modify: `frontend/src/components/mod.rs`
- Modify: `frontend/src/components/aop_gauge.rs`

- [ ] **Step 1: 创建 frontend/src/components/gauge.rs**

完整内容（直接写入）：

```rust
//! 通用 HUD 仪表盘组件
//!
//! 从 AopGauge 抽象而来，提供可复用的圆形仪表盘 canvas 组件：
//! - 中心大数字 + 副标签
//! - 顶部标题
//! - 右上角徽章（可选）
//! - 底部辅助信息（可选）
//! - 颜色编码（由调用方通过 color() 提供）
//! - 选中状态加强发光边框
//! - 呼吸光晕（2.4s 周期）
//!
//! 调用方通过实现 `GaugeStatus` trait 决定颜色编码逻辑，
//! 通过 `GaugeData` 传递显示数据。

use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

use crate::components::hud_palette;

/// 仪表盘显示数据
#[derive(Props, Clone)]
pub struct GaugeProps {
    /// 顶部标题（如消费者名称、指标名）
    pub title: String,
    /// 中心大数字文本（如 "12"、"OK"、"85%"）
    pub center_value: String,
    /// 中心副标签（如 "pending"、"used"）
    pub center_label: String,
    /// 主色（hex，如 "#fa520f"）
    pub color: String,
    /// 右上角徽章文本（可选，如 "⚙ 3"）
    pub badge: Option<String>,
    /// 底部辅助信息（可选）
    pub footer: Option<String>,
    /// 是否选中（加强发光边框）
    pub is_selected: bool,
    /// 画布宽度
    #[props(default = "200.0")]
    pub width: f64,
    /// 画布高度
    #[props(default = "200.0")]
    pub height: f64,
    /// 点击回调
    pub on_click: Option<EventHandler<()>>,
}

// EventHandler 无法比较，手动实现 PartialEq 时忽略 on_click 字段
impl PartialEq for GaugeProps {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title
            && self.center_value == other.center_value
            && self.center_label == other.center_label
            && self.color == other.color
            && self.badge == other.badge
            && self.footer == other.footer
            && self.is_selected == other.is_selected
            && self.width == other.width
            && self.height == other.height
    }
}

/// 通用 Gauge 组件
#[component]
pub fn Gauge(props: GaugeProps) -> Element {
    let width = props.width;
    let height = props.height;

    let mut canvas_ref: Signal<Option<HtmlCanvasElement>> = use_signal(|| None);

    // 数据 cache：props 变化时更新
    let mut data_cache: Signal<GaugeProps> = use_signal(|| props.clone());
    let props_clone = props.clone();
    use_effect(move || {
        data_cache.set(props_clone.clone());
    });

    // 渲染循环
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
        canvas.set_width((width * dpr) as u32);
        canvas.set_height((height * dpr) as u32);
        let _ = ctx.scale(dpr, dpr);

        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let running_clone = running.clone();

        let callback_ref: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
        let cb_ref_inner = callback_ref.clone();

        let closure = Closure::<dyn FnMut()>::new(move || {
            let data = data_cache.read().clone();
            let now = js_sys::Date::now() / 1000.0;
            draw_gauge(&ctx, width, height, &data, now);

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

    // 点击处理
    let on_click_handler = props.on_click.clone();
    rsx! {
        canvas {
            width: "{width as u32}",
            height: "{height as u32}",
            class: "cursor-pointer",
            style: "display: block;",
            onclick: move |_| {
                if let Some(handler) = on_click_handler.as_ref() {
                    handler.call(());
                }
            },
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

/// 绘制完整仪表盘
fn draw_gauge(
    ctx: &CanvasRenderingContext2d,
    width: f64,
    height: f64,
    data: &GaugeProps,
    now: f64,
) {
    // 1. HUD 背景
    hud_palette::draw_hud_background(ctx, width, height);

    let cx = width / 2.0;
    let cy = height / 2.0;
    let radius = width.min(height) / 2.0 - 18.0;

    let color = &data.color;

    // 2. 呼吸光晕（外圈，2.4s 周期）
    let pulse_period = 2.4;
    let pulse_t = (now % pulse_period) / pulse_period;
    let phase = (pulse_t * std::f64::consts::TAU).sin();
    let pulse_alpha = 0.30 + phase * 0.20;

    // 3. 外圈呼吸光晕（彩色）
    ctx.set_stroke_style_str(&hud_palette::hex_to_rgba(color, pulse_alpha));
    ctx.set_line_width(2.0);
    ctx.begin_path();
    let _ = ctx.arc(cx, cy, radius, 0.0, std::f64::consts::TAU);
    ctx.stroke();

    // 4. 选中状态加强发光
    if data.is_selected {
        ctx.set_shadow_blur(12.0);
        ctx.set_shadow_color(color);
        ctx.set_stroke_style_str(color);
        ctx.set_line_width(3.0);
        ctx.begin_path();
        let _ = ctx.arc(cx, cy, radius - 3.0, 0.0, std::f64::consts::TAU);
        ctx.stroke();
        ctx.set_shadow_blur(0.0);
    }

    // 5. 内圈装饰刻度（12 等分，淡橙色）
    ctx.set_stroke_style_str("rgba(250, 82, 15, 0.25)");
    ctx.set_line_width(1.0);
    for i in 0..12 {
        let angle = (i as f64) * std::f64::consts::TAU / 12.0;
        let r1 = radius - 6.0;
        let r2 = radius - 12.0;
        ctx.begin_path();
        ctx.move_to(cx + r1 * angle.cos(), cy + r1 * angle.sin());
        ctx.line_to(cx + r2 * angle.cos(), cy + r2 * angle.sin());
        ctx.stroke();
    }

    // 6. 中心数字
    ctx.set_fill_style_str(color);
    ctx.set_font("bold 36px sans-serif");
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    let _ = ctx.fill_text(&data.center_value, cx, cy - 4.0);

    // 7. 中心副标签
    ctx.set_fill_style_str("rgba(255, 255, 255, 0.6)");
    ctx.set_font("10px sans-serif");
    let _ = ctx.fill_text(&data.center_label, cx, cy + 18.0);

    // 8. 顶部：标题
    ctx.set_fill_style_str("rgba(250, 82, 15, 0.9)");
    ctx.set_font("bold 11px sans-serif");
    ctx.set_text_baseline("top");
    // 名称过长截断（按字符数）
    let title = if data.title.chars().count() > 22 {
        let truncated: String = data.title.chars().take(21).collect();
        format!("{}...", truncated)
    } else {
        data.title.clone()
    };
    let _ = ctx.fill_text(&title, cx, 8.0);

    // 9. 右上角徽章
    if let Some(badge) = &data.badge {
        ctx.set_fill_style_str("#f59e0b");
        ctx.set_font("bold 10px sans-serif");
        ctx.set_text_align("right");
        let _ = ctx.fill_text(badge, width - 6.0, 8.0);
        ctx.set_text_align("center");
    }

    // 10. 底部辅助信息
    if let Some(footer) = &data.footer {
        ctx.set_fill_style_str("rgba(255, 255, 255, 0.5)");
        ctx.set_font("9px sans-serif");
        ctx.set_text_baseline("bottom");
        let _ = ctx.fill_text(footer, cx, height - 6.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gauge_props_partial_eq_ignores_on_click() {
        let handler: Option<EventHandler<()>> = None;
        let p1 = GaugeProps {
            title: "test".to_string(),
            center_value: "10".to_string(),
            center_label: "pending".to_string(),
            color: "#fa520f".to_string(),
            badge: None,
            footer: None,
            is_selected: false,
            width: 200.0,
            height: 200.0,
            on_click: handler.clone(),
        };
        let p2 = GaugeProps {
            title: "test".to_string(),
            center_value: "10".to_string(),
            center_label: "pending".to_string(),
            color: "#fa520f".to_string(),
            badge: None,
            footer: None,
            is_selected: false,
            width: 200.0,
            height: 200.0,
            on_click: handler,
        };
        assert_eq!(p1, p2);
    }

    #[test]
    fn test_gauge_props_partial_eq_detects_diff() {
        let p1 = GaugeProps {
            title: "test".to_string(),
            center_value: "10".to_string(),
            center_label: "pending".to_string(),
            color: "#fa520f".to_string(),
            badge: None,
            footer: None,
            is_selected: false,
            width: 200.0,
            height: 200.0,
            on_click: None,
        };
        let p2 = GaugeProps {
            title: "test".to_string(),
            center_value: "20".to_string(), // 不同
            center_label: "pending".to_string(),
            color: "#fa520f".to_string(),
            badge: None,
            footer: None,
            is_selected: false,
            width: 200.0,
            height: 200.0,
            on_click: None,
        };
        assert_ne!(p1, p2);
    }
}
```

- [ ] **Step 2: 在 frontend/src/components/mod.rs 注册 gauge**

在文件中追加 `pub mod gauge;`（按字母序放在 `confirm_dialog` 之后、`graph` 之前）。

当前内容：
```rust
pub mod confirm_dialog;
pub mod graph;
```

改为：
```rust
pub mod confirm_dialog;
pub mod gauge;
pub mod graph;
```

- [ ] **Step 3: 重写 frontend/src/components/aop_gauge.rs 为 wrap Gauge**

完全替换文件内容：

```rust
//! AOP 消费者仪表盘组件（基于通用 Gauge 组件）
//!
//! 在通用 Gauge 之上封装 AOP 专属：
//! - 颜色编码逻辑（基于 pending/in_progress 状态）
//! - AOP 专属字段映射（oldest_age_secs → footer、order_keys_count → footer）
//!
//! 点击 canvas 触发 on_click 回调（用于切换事件列表）

use dioxus::prelude::*;

use crate::components::gauge::{Gauge, GaugeProps};

/// AopGauge 组件 Props（保持向后兼容）
#[derive(Props, Clone)]
pub struct AopGaugeProps {
    /// 消费者名称
    pub consumer_name: String,
    /// 待处理事件数
    pub pending: usize,
    /// 处理中事件数
    pub in_progress: usize,
    /// 最老事件年龄（秒），None 表示无事件
    pub oldest_age_secs: Option<u64>,
    /// order_keys 数量
    pub order_keys_count: usize,
    /// 是否选中（加强发光边框）
    pub is_selected: bool,
    /// 点击回调
    pub on_click: Option<EventHandler<()>>,
}

// EventHandler 无法比较，手动实现 PartialEq 时忽略 on_click 字段
impl PartialEq for AopGaugeProps {
    fn eq(&self, other: &Self) -> bool {
        self.consumer_name == other.consumer_name
            && self.pending == other.pending
            && self.in_progress == other.in_progress
            && self.oldest_age_secs == other.oldest_age_secs
            && self.order_keys_count == other.order_keys_count
            && self.is_selected == other.is_selected
    }
}

/// 根据队列状态获取主色
fn status_color(pending: usize, in_progress: usize) -> &'static str {
    if pending >= 10 {
        "#ef4444" // 红色：堆积告警
    } else if pending > 0 {
        "#fa520f" // 橙色：正常负载
    } else if in_progress > 0 {
        "#f59e0b" // 黄色：处理中
    } else {
        "#10b981" // 绿色：idle 健康
    }
}

/// AopGauge 组件（薄 wrap Gauge）
#[component]
pub fn AopGauge(props: AopGaugeProps) -> Element {
    let color = status_color(props.pending, props.in_progress).to_string();
    let center_value = if props.pending == 0 && props.in_progress == 0 {
        "OK".to_string()
    } else {
        props.pending.to_string()
    };
    let badge = if props.in_progress > 0 {
        Some(format!("⚙ {}", props.in_progress))
    } else {
        None
    };
    let mut footer = format!("{} order_keys", props.order_keys_count);
    if let Some(age) = props.oldest_age_secs {
        footer.push_str(&format!(" · {}s ago", age));
    }

    rsx! {
        Gauge {
            title: props.consumer_name.clone(),
            center_value,
            center_label: "pending".to_string(),
            color,
            badge,
            footer: Some(footer),
            is_selected: props.is_selected,
            on_click: props.on_click.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_color_idle() {
        assert_eq!(status_color(0, 0), "#10b981");
    }

    #[test]
    fn test_status_color_processing() {
        assert_eq!(status_color(0, 5), "#f59e0b");
    }

    #[test]
    fn test_status_color_normal_load() {
        assert_eq!(status_color(5, 0), "#fa520f");
        assert_eq!(status_color(9, 3), "#fa520f");
    }

    #[test]
    fn test_status_color_overload() {
        assert_eq!(status_color(10, 0), "#ef4444");
        assert_eq!(status_color(100, 5), "#ef4444");
    }
}
```

- [ ] **Step 4: 编译验证 + 测试**

运行：
```bash
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo test --bin frontend gauge
```

预期：
- cargo check 通过
- gauge 2 个测试通过 + aop_gauge 4 个测试通过（共 6 个）

- [ ] **Step 5: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/components/gauge.rs frontend/src/components/mod.rs frontend/src/components/aop_gauge.rs
git commit -m "refactor: 抽出通用 Gauge 组件，AopGauge 改为薄 wrap"
```

---

## Task 2: 后端日志统计聚合 API

**Files:**
- Create: `common/src/api/log_stats.rs`
- Modify: `common/src/api/mod.rs`
- Create: `src/handlers/system/logs/log_stats.rs`
- Modify: `src/handlers/system/logs/mod.rs`
- Modify: `src/service/domain/system/mod.rs`（如需）

- [ ] **Step 1: 创建 common/src/api/log_stats.rs**

完整内容（直接写入）：

```rust
//! 日志统计聚合 API DTO

use serde::{Deserialize, Serialize};

/// 日志级别分布项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogLevelDistributionItem {
    /// 日志级别（INFO / WARN / ERROR / DEBUG / TRACE）
    pub level: String,
    /// 数量
    pub count: u64,
}

/// 日志级别分布响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogLevelDistributionResponse {
    pub items: Vec<LogLevelDistributionItem>,
    /// 总数
    pub total: u64,
}

/// 日志时序数据点（按小时桶）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogTimeSeriesPoint {
    /// 桶起始时间（unix ms）
    pub interval_start: i64,
    /// 该时段日志数
    pub count: u64,
}

/// 日志时序响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogTimeSeriesResponse {
    pub points: Vec<LogTimeSeriesPoint>,
}

/// 日志统计查询参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogStatsQueryParams {
    /// 起始时间（unix ms，含），默认 24 小时前
    pub start_time: Option<i64>,
    /// 结束时间（unix ms，含），默认当前
    pub end_time: Option<i64>,
}
```

- [ ] **Step 2: 在 common/src/api/mod.rs 注册模块**

在文件中找到 mod 声明区，追加 `pub mod log_stats;`（按字母序放在 `jwt` 之后或合适位置）。

- [ ] **Step 3: 创建 src/handlers/system/logs/log_stats.rs**

完整内容（直接写入）：

```rust
//! Handler: GET /api/v1/system/logs/stats/level-distribution - 日志级别分布
//! Handler: GET /api/v1/system/logs/stats/time-series - 日志时序

use axum::extract::Query;
use axum::Json;
use axum::response::IntoResponse;
use common::api::{
    ApiResponse, LogLevelDistributionItem, LogLevelDistributionResponse,
    LogStatsQueryParams, LogTimeSeriesPoint, LogTimeSeriesResponse,
};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::pkg::request_context::RequestContext;
use crate::pkg::logging::LogLevel;

/// GET /api/v1/system/logs/stats/level-distribution
///
/// 返回日志级别分布（INFO/WARN/ERROR/DEBUG/TRACE 各自计数）。
/// 时间范围由 query 参数控制，默认最近 24 小时。
pub async fn get_log_level_distribution(
    Query(params): Query<LogStatsQueryParams>,
) -> impl IntoResponse {
    let ctx = RequestContext::system();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let end_time = params.end_time.unwrap_or(now);
    let start_time = params.start_time.unwrap_or(end_time - 24 * 60 * 60 * 1000);

    let dal = crate::service::dal::log::LogDal::instance();
    match dal.level_distribution(&ctx, start_time, end_time).await {
        Ok(distribution) => {
            let items: Vec<LogLevelDistributionItem> = distribution
                .into_iter()
                .map(|(level, count)| LogLevelDistributionItem { level, count })
                .collect();
            let total: u64 = items.iter().map(|i| i.count).sum();
            Json(ApiResponse::success(LogLevelDistributionResponse { items, total }))
        }
        Err(e) => Json(ApiResponse::error(
            500,
            &format!("查询日志级别分布失败: {}", e),
        )),
    }
}

/// GET /api/v1/system/logs/stats/time-series
///
/// 返回日志时序数据（按小时桶）。
pub async fn get_log_time_series(
    Query(params): Query<LogStatsQueryParams>,
) -> impl IntoResponse {
    let ctx = RequestContext::system();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let end_time = params.end_time.unwrap_or(now);
    let start_time = params.start_time.unwrap_or(end_time - 24 * 60 * 60 * 1000);

    let dal = crate::service::dal::log::LogDal::instance();
    match dal.time_series(&ctx, start_time, end_time).await {
        Ok(points) => {
            let points: Vec<LogTimeSeriesPoint> = points
                .into_iter()
                .map(|(interval_start, count)| LogTimeSeriesPoint { interval_start, count })
                .collect();
            Json(ApiResponse::success(LogTimeSeriesResponse { points }))
        }
        Err(e) => Json(ApiResponse::error(
            500,
            &format!("查询日志时序失败: {}", e),
        )),
    }
}
```

- [ ] **Step 4: 在 src/handlers/system/logs/mod.rs 注册新 handler**

读取当前 `src/handlers/system/logs/mod.rs` 内容并追加 `pub mod log_stats;` 和 `pub use log_stats::*;`。

- [ ] **Step 5: 注册路由到 src/handlers/system/mod.rs 或路由配置文件**

查找系统域路由配置文件（通常在 `src/handlers/system/mod.rs` 或 `src/main.rs` 的路由表中），追加：

```rust
.route(
    "/api/v1/system/logs/stats/level-distribution",
    axum::routing::get(handlers::system::logs::log_stats::get_log_level_distribution),
)
.route(
    "/api/v1/system/logs/stats/time-series",
    axum::routing::get(handlers::system::logs::log_stats::get_log_time_series),
)
```

具体路径需要查现有 `/api/v1/system/logs` 路由注册位置。

- [ ] **Step 6: 在 LogDal 中添加聚合查询方法**

在 `src/service/dal/log.rs` 中添加两个方法：

```rust
/// 查询日志级别分布
pub async fn level_distribution(
    &self,
    ctx: &RequestContext,
    start_time: i64,
    end_time: i64,
) -> anyhow::Result<Vec<(String, u64)>> {
    let logs = self.query_range(ctx, start_time, end_time).await?;
    let mut dist: HashMap<String, u64> = HashMap::new();
    for log in logs {
        *dist.entry(log.level.clone()).or_insert(0) += 1;
    }
    Ok(dist.into_iter().collect())
}

/// 查询日志时序（按小时桶）
pub async fn time_series(
    &self,
    ctx: &RequestContext,
    start_time: i64,
    end_time: i64,
) -> anyhow::Result<Vec<(i64, u64)>> {
    let logs = self.query_range(ctx, start_time, end_time).await?;
    let mut buckets: HashMap<i64, u64> = HashMap::new();
    for log in logs {
        let ts = parse_log_timestamp_ms(&log.timestamp).unwrap_or(start_time);
        let bucket = (ts / 3_600_000) * 3_600_000; // 按小时对齐
        *buckets.entry(bucket).or_insert(0) += 1;
    }
    let mut points: Vec<(i64, u64)> = buckets.into_iter().collect();
    points.sort_by_key(|(ts, _)| *ts);
    Ok(points)
}
```

**注意**：实际实现时需要根据 `LogDal` 现有接口调整。如果 `query_range` 不存在，用现有的 `query_logs` 方法并传入大 page_size。如果 `parse_log_timestamp_ms` 不存在，在文件内添加私有辅助函数。

- [ ] **Step 7: 编译验证**

```bash
cd /Users/aman/Technology/rust/ai_orz && cargo check --lib
```

预期：通过。如果 LogDal 的方法签名不匹配，请根据实际签名调整 handler 实现。

- [ ] **Step 8: 添加单元测试**

在 `src/handlers/system/logs/log_stats.rs` 末尾追加：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_stats_query_params_default() {
        let params = LogStatsQueryParams {
            start_time: None,
            end_time: None,
        };
        assert!(params.start_time.is_none());
        assert!(params.end_time.is_none());
    }

    #[test]
    fn test_log_level_distribution_response_serialize() {
        let resp = LogLevelDistributionResponse {
            items: vec![LogLevelDistributionItem {
                level: "INFO".to_string(),
                count: 100,
            }],
            total: 100,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("INFO"));
        assert!(json.contains("\"total\":100"));
    }

    #[test]
    fn test_log_time_series_response_serialize() {
        let resp = LogTimeSeriesResponse {
            points: vec![LogTimeSeriesPoint {
                interval_start: 1234567890000,
                count: 42,
            }],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("1234567890000"));
    }
}
```

- [ ] **Step 9: 运行测试**

```bash
cd /Users/aman/Technology/rust/ai_orz && cargo test --lib log_stats
```

预期：3 个测试通过。

- [ ] **Step 10: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add common/src/api/log_stats.rs common/src/api/mod.rs src/handlers/system/logs/log_stats.rs src/handlers/system/logs/mod.rs src/service/dal/log.rs
# 根据实际修改追加其他文件
git commit -m "feat: 新增日志统计聚合 API（级别分布 + 时序）"
```

---

## Task 3: 前端日志统计 API 客户端

**Files:**
- Create: `frontend/src/api/log_stats.rs`
- Modify: `frontend/src/api/mod.rs`

- [ ] **Step 1: 创建 frontend/src/api/log_stats.rs**

完整内容：

```rust
//! 日志统计 API 客户端

use serde::Deserialize;

use super::{api_get, ApiError};

#[derive(Debug, Clone, Deserialize)]
pub struct LogLevelDistributionItem {
    pub level: String,
    pub count: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogLevelDistributionResponse {
    pub items: Vec<LogLevelDistributionItem>,
    pub total: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogTimeSeriesPoint {
    pub interval_start: i64,
    pub count: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogTimeSeriesResponse {
    pub points: Vec<LogTimeSeriesPoint>,
}

/// 获取日志级别分布（默认最近 24 小时）
pub async fn get_log_level_distribution(
    start_time: Option<i64>,
    end_time: Option<i64>,
) -> Result<LogLevelDistributionResponse, ApiError> {
    let mut params = Vec::new();
    if let Some(s) = start_time {
        params.push(format!("start_time={}", s));
    }
    if let Some(e) = end_time {
        params.push(format!("end_time={}", e));
    }
    let qs = params.join("&");
    let path = if qs.is_empty() {
        "/api/v1/system/logs/stats/level-distribution".to_string()
    } else {
        format!("/api/v1/system/logs/stats/level-distribution?{}", qs)
    };
    api_get(&path).await
}

/// 获取日志时序（按小时桶，默认最近 24 小时）
pub async fn get_log_time_series(
    start_time: Option<i64>,
    end_time: Option<i64>,
) -> Result<LogTimeSeriesResponse, ApiError> {
    let mut params = Vec::new();
    if let Some(s) = start_time {
        params.push(format!("start_time={}", s));
    }
    if let Some(e) = end_time {
        params.push(format!("end_time={}", e));
    }
    let qs = params.join("&");
    let path = if qs.is_empty() {
        "/api/v1/system/logs/stats/time-series".to_string()
    } else {
        format!("/api/v1/system/logs/stats/time-series?{}", qs)
    };
    api_get(&path).await
}
```

- [ ] **Step 2: 在 frontend/src/api/mod.rs 注册模块**

追加 `pub mod log_stats;`（按字母序）。

- [ ] **Step 3: 编译验证**

```bash
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check
```

- [ ] **Step 4: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/api/log_stats.rs frontend/src/api/mod.rs
git commit -m "feat: 前端日志统计 API 客户端"
```

---

## Task 4: logs.rs 顶部加日志统计图表区

**Files:**
- Modify: `frontend/src/pages/system/logs.rs`

- [ ] **Step 1: 在 logs.rs 顶部追加 import**

在现有 use 区追加：

```rust
use crate::api::log_stats::{get_log_level_distribution, get_log_time_series, LogLevelDistributionItem, LogTimeSeriesPoint};
use crate::components::charts::donut_chart::{DonutChart, DonutSlice};
use crate::components::charts::line_chart::LineChart;
use common::models::TimeSeriesPoint;
```

- [ ] **Step 2: 在 SystemLogs 组件内追加统计数据信号 + 加载逻辑**

在 `use_signal` 区追加：

```rust
let mut stats_loading = use_signal(|| false);
let mut level_distribution: Signal<Vec<LogLevelDistributionItem>> = use_signal(Vec::new);
let mut time_series_points: Signal<Vec<LogTimeSeriesPoint>> = use_signal(Vec::new);
```

在现有 `load_data`（或现有数据加载 spawn）之后追加独立的 stats 加载：

```rust
let mut load_stats = move || {
    stats_loading.set(true);
    spawn(async move {
        let now = chrono::Local::now().timestamp_millis();
        let start = now - 24 * 60 * 60 * 1000;
        let dist_result = get_log_level_distribution(Some(start), Some(now)).await;
        let ts_result = get_log_time_series(Some(start), Some(now)).await;
        if let Ok(resp) = dist_result {
            level_distribution.set(resp.items);
        }
        if let Ok(resp) = ts_result {
            time_series_points.set(resp.points);
        }
        stats_loading.set(false);
    });
};

// 初始加载
use_effect(move || {
    load_stats();
});
```

- [ ] **Step 3: 在 rsx! 顶部追加图表区**

在 `AppLayout {` 之后、现有筛选表单之前追加：

```rust
div { class: "grid grid-cols-1 lg:grid-cols-2 gap-4 mb-4",
    // 日志级别分布 DonutChart
    div { class: "card bg-base-100 shadow-md",
        div { class: "card-body",
            h3 { class: "card-title text-sm", "📊 级别分布（24h）" }
            if level_distribution.read().is_empty() {
                div { class: "h-[200px] flex items-center justify-center text-base-content/50",
                    if stats_loading() { "加载中..." } else { "暂无数据" }
                }
            } else {
                DonutChart {
                    data: level_distribution.read().iter().map(|i| DonutSlice {
                        label: i.level.clone(),
                        value: i.count,
                    }).collect::<Vec<_>>(),
                    width: 300.0,
                    height: 220.0,
                    title: Some("日志级别".to_string()),
                }
            }
        }
    }
    // 日志量时序 LineChart
    div { class: "card bg-base-100 shadow-md",
        div { class: "card-body",
            h3 { class: "card-title text-sm", "📈 日志量趋势（24h）" }
            if time_series_points.read().is_empty() {
                div { class: "h-[200px] flex items-center justify-center text-base-content/50",
                    if stats_loading() { "加载中..." } else { "暂无数据" }
                }
            } else {
                LineChart {
                    data: time_series_points.read().iter().map(|p| TimeSeriesPoint {
                        interval_start: p.interval_start,
                        call_count: p.count,
                    }).collect::<Vec<_>>(),
                    width: 500.0,
                    height: 220.0,
                    title: Some("日志量".to_string()),
                    value_label: Some("条".to_string()),
                }
            }
        }
    }
}
```

- [ ] **Step 4: 编译验证**

```bash
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check
```

- [ ] **Step 5: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/pages/system/logs.rs
git commit -m "feat: logs 页面顶部加级别分布 + 日志量趋势图表"
```

---

## Task 5: triggers.rs 加触发器状态分布图

**Files:**
- Modify: `frontend/src/pages/system/triggers.rs`

- [ ] **Step 1: 在 triggers.rs 顶部追加 import**

```rust
use crate::components::charts::donut_chart::{DonutChart, DonutSlice};
```

- [ ] **Step 2: 在 render 区追加状态分布 DonutChart**

触发器列表加载后，在 `triggers` 信号基础上做前端本地聚合（启用 vs 暂停 vs 即将执行）。在 rsx! 顶部（在 AppLayout 之后、列表之前）追加：

```rust
// 计算状态分布
let enabled_count = triggers.read().iter().filter(|t| t.is_enabled).count();
let disabled_count = triggers.read().iter().filter(|t| !t.is_enabled).count();
let now_secs = chrono::Local::now().timestamp();
let soon_count = triggers.read().iter().filter(|t| {
    t.is_enabled && t.next_run_at > 0 && (t.next_run_at - now_secs).abs() < 3600
}).count();

div { class: "card bg-base-100 shadow-md mb-4",
    div { class: "card-body",
        h3 { class: "card-title text-sm", "📊 触发器状态分布" }
        DonutChart {
            data: vec![
                DonutSlice { label: "启用".to_string(), value: enabled_count as u64 },
                DonutSlice { label: "暂停".to_string(), value: disabled_count as u64 },
                DonutSlice { label: "1h 内执行".to_string(), value: soon_count as u64 },
            ],
            width: 400.0,
            height: 220.0,
            title: Some("触发器状态".to_string()),
        }
    }
}
```

**注意**：`triggers` 信号名需要根据实际代码调整（可能叫 `triggers_data` 或其他）。如果实际变量名不同，调整代码。

- [ ] **Step 3: 编译验证**

```bash
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check
```

- [ ] **Step 4: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/pages/system/triggers.rs
git commit -m "feat: triggers 页面顶部加状态分布环形图"
```

---

## Task 6: health.rs 重写为仪表盘墙

**Files:**
- Modify: `frontend/src/pages/system/health.rs`
- Modify: `frontend/src/api/system.rs`

- [ ] **Step 1: 在 frontend/src/api/system.rs 追加 HealthMetrics 客户端**

在文件末尾追加：

```rust
// ===== 系统健康指标 =====

#[derive(Debug, Clone, serde::Deserialize)]
pub struct HealthMetricsResponse {
    /// 后端服务在线（true/false）
    pub backend_online: bool,
    /// AOP 队列总待处理数
    pub aop_pending: u64,
    /// AOP 队列总处理中数
    pub aop_in_progress: u64,
    /// 活跃 Agent 数（status != 0）
    pub active_agents: u64,
    /// 总 Agent 数
    pub total_agents: u64,
    /// 活跃项目数
    pub active_projects: u64,
    /// 总项目数
    pub total_projects: u64,
    /// 待处理任务数（status != Done）
    pub pending_tasks: u64,
    /// 总任务数
    pub total_tasks: u64,
    /// 运行时长（秒）
    pub uptime_secs: u64,
}

/// 获取系统健康指标
pub async fn get_health_metrics() -> Result<HealthMetricsResponse, ApiError> {
    api_get("/api/v1/system/health/metrics").await
}
```

- [ ] **Step 2: 后端新增 health/metrics 端点**

在后端 `src/handlers/system/` 下新建 `health_metrics.rs`：

```rust
//! Handler: GET /api/v1/system/health/metrics - 系统健康指标聚合

use axum::Json;
use axum::response::IntoResponse;
use common::api::{ApiResponse, HealthMetricsResponse};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::pkg::request_context::RequestContext;

pub async fn get_health_metrics() -> impl IntoResponse {
    let ctx = RequestContext::system();

    // 聚合各维度指标
    let (aop_pending, aop_in_progress) = get_aop_queue_stats().await;
    let (active_agents, total_agents) = get_agent_stats(&ctx).await;
    let (active_projects, total_projects) = get_project_stats(&ctx).await;
    let (pending_tasks, total_tasks) = get_task_stats(&ctx).await;
    let uptime_secs = get_uptime_secs();

    Json(ApiResponse::success(HealthMetricsResponse {
        backend_online: true,
        aop_pending,
        aop_in_progress,
        active_agents,
        total_agents,
        active_projects,
        total_projects,
        pending_tasks,
        total_tasks,
        uptime_secs,
    }))
}

async fn get_aop_queue_stats() -> (u64, u64) {
    // 通过 AopStatsCollector 获取
    let collector = crate::consumer::aop_stats_collector::AopStatsCollector::instance();
    let overview = collector.overview().await;
    // 这里简化：用 total_published - total_consumed 作为 pending
    // 实际可以从 SystemDomain 拿 queue_stats
    (overview.total_published.saturating_sub(overview.total_consumed), 0)
}

async fn get_agent_stats(ctx: &RequestContext) -> (u64, u64) {
    // 简化实现：通过 hr_domain 获取
    (0, 0) // 实际实现时调用 hr_domain
}

async fn get_project_stats(ctx: &RequestContext) -> (u64, u64) {
    (0, 0)
}

async fn get_task_stats(ctx: &RequestContext) -> (u64, u64) {
    (0, 0)
}

fn get_uptime_secs() -> u64 {
    // 通过全局启动时间计算
    0
}
```

**注意**：实际实现时需要根据现有 SystemDomain 能力填充。如果某些维度获取成本高，可以降级为返回 0 或省略该字段。

- [ ] **Step 3: 在 common/src/api/system.rs 或新建文件追加 HealthMetricsResponse**

如果 common 中没有 system.rs，新建 `common/src/api/system.rs`：

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMetricsResponse {
    pub backend_online: bool,
    pub aop_pending: u64,
    pub aop_in_progress: u64,
    pub active_agents: u64,
    pub total_agents: u64,
    pub active_projects: u64,
    pub total_projects: u64,
    pub pending_tasks: u64,
    pub total_tasks: u64,
    pub uptime_secs: u64,
}
```

并在 `common/src/api/mod.rs` 注册 `pub mod system;`。

- [ ] **Step 4: 注册路由**

在系统域路由表追加：

```rust
.route(
    "/api/v1/system/health/metrics",
    axum::routing::get(handlers::system::health_metrics::get_health_metrics),
)
```

- [ ] **Step 5: 重写 frontend/src/pages/system/health.rs**

完整内容：

```rust
//! 系统健康监控 - HUD 仪表盘墙
//!
//! 用通用 Gauge 组件展示系统各维度健康指标：
//! - 后端服务状态（绿/红）
//! - AOP 队列深度（绿/黄/橙/红）
//! - 活跃 Agent 比例
//! - 活跃项目比例
//! - 待处理任务数
//! - 运行时长

use dioxus::prelude::*;

use crate::api::system::{check_health, get_health_metrics, HealthMetricsResponse};
use crate::components::gauge::{Gauge, GaugeProps};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;

fn aop_color(pending: u64) -> String {
    if pending >= 10 {
        "#ef4444".to_string()
    } else if pending > 0 {
        "#fa520f".to_string()
    } else {
        "#10b981".to_string()
    }
}

fn ratio_color(ratio: f64) -> String {
    if ratio >= 0.8 {
        "#10b981".to_string()
    } else if ratio >= 0.5 {
        "#f59e0b".to_string()
    } else {
        "#fa520f".to_string()
    }
}

fn task_color(pending: u64) -> String {
    if pending == 0 {
        "#10b981".to_string()
    } else if pending < 10 {
        "#f59e0b".to_string()
    } else {
        "#ef4444".to_string()
    }
}

#[component]
pub fn SystemHealth() -> Element {
    let mut loading = use_signal(|| false);
    let mut metrics: Signal<Option<HealthMetricsResponse>> = use_signal(|| None);
    let toast = use_toast();

    let load_metrics = move || {
        loading.set(true);
        spawn(async move {
            match get_health_metrics().await {
                Ok(m) => metrics.set(Some(m)),
                Err(e) => toast.error(&format!("加载系统指标失败: {}", e)),
            }
            loading.set(false);
        });
    };

    // 初始加载 + 10 秒轮询
    use_future(move || {
        load_metrics();
        async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                load_metrics();
            }
        }
    });

    let m = metrics.read().clone();
    let gauges: Vec<GaugeProps> = if let Some(m) = &m {
        let agent_ratio = if m.total_agents > 0 {
            m.active_agents as f64 / m.total_agents as f64
        } else {
            0.0
        };
        let project_ratio = if m.total_projects > 0 {
            m.active_projects as f64 / m.total_projects as f64
        } else {
            0.0
        };
        let uptime_hours = m.uptime_secs / 3600;
        vec![
            GaugeProps {
                title: "后端服务".to_string(),
                center_value: if m.backend_online { "OK".to_string() } else { "DOWN".to_string() },
                center_label: "status".to_string(),
                color: if m.backend_online { "#10b981".to_string() } else { "#ef4444".to_string() },
                badge: None,
                footer: None,
                is_selected: false,
                width: 180.0,
                height: 180.0,
                on_click: None,
            },
            GaugeProps {
                title: "AOP 队列".to_string(),
                center_value: m.aop_pending.to_string(),
                center_label: "pending".to_string(),
                color: aop_color(m.aop_pending),
                badge: if m.aop_in_progress > 0 { Some(format!("⚙ {}", m.aop_in_progress)) } else { None },
                footer: None,
                is_selected: false,
                width: 180.0,
                height: 180.0,
                on_click: None,
            },
            GaugeProps {
                title: "活跃 Agent".to_string(),
                center_value: format!("{}", m.active_agents),
                center_label: format!("/ {}", m.total_agents),
                color: ratio_color(agent_ratio),
                badge: None,
                footer: Some(format!("{:.0}% 活跃", agent_ratio * 100.0)),
                is_selected: false,
                width: 180.0,
                height: 180.0,
                on_click: None,
            },
            GaugeProps {
                title: "活跃项目".to_string(),
                center_value: format!("{}", m.active_projects),
                center_label: format!("/ {}", m.total_projects),
                color: ratio_color(project_ratio),
                badge: None,
                footer: Some(format!("{:.0}% 活跃", project_ratio * 100.0)),
                is_selected: false,
                width: 180.0,
                height: 180.0,
                on_click: None,
            },
            GaugeProps {
                title: "待处理任务".to_string(),
                center_value: m.pending_tasks.to_string(),
                center_label: format!("/ {}", m.total_tasks),
                color: task_color(m.pending_tasks),
                badge: None,
                footer: None,
                is_selected: false,
                width: 180.0,
                height: 180.0,
                on_click: None,
            },
            GaugeProps {
                title: "运行时长".to_string(),
                center_value: format!("{}", uptime_hours),
                center_label: "hours".to_string(),
                color: "#10b981".to_string(),
                badge: None,
                footer: Some(format!("{}s", m.uptime_secs % 3600)),
                is_selected: false,
                width: 180.0,
                height: 180.0,
                on_click: None,
            },
        ]
    } else {
        Vec::new()
    };

    rsx! {
        AppLayout {
        div { class: "space-y-4",
            div { class: "flex justify-between items-center",
                h2 { class: "card-title", "系统健康监控" }
                button {
                    class: "btn btn-ghost btn-sm",
                    onclick: move |_| {
                        spawn(async move {
                            match check_health().await {
                                Ok(msg) => toast.success(&format!("服务正常: {}", msg)),
                                Err(e) => toast.error(&format!("健康检查失败: {}", e)),
                            }
                        });
                    },
                    "手动检查"
                }
            }
            if loading() && metrics.read().is_none() {
                div { class: "flex justify-center py-12",
                    span { class: "loading loading-spinner loading-lg" }
                }
            } else if !gauges.is_empty() {
                div { class: "grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4",
                    for g in gauges {
                        Gauge { key: "{g.title}", ..g }
                    }
                }
            } else {
                div { class: "text-center py-12 text-base-content/50", "暂无数据" }
            }
        }
        }
    }
}
```

- [ ] **Step 6: 编译验证**

```bash
cd /Users/aman/Technology/rust/ai_orz && cargo check --lib
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check
```

- [ ] **Step 7: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add common/src/api/system.rs common/src/api/mod.rs src/handlers/system/health_metrics.rs src/handlers/system/mod.rs frontend/src/api/system.rs frontend/src/pages/system/health.rs
git commit -m "feat: health 页面重写为 HUD 仪表盘墙（10s 轮询）"
```

---

## Task 7: agent_detail.rs 统计 Tab 补充工具调用分布 DonutChart

**Files:**
- Modify: `frontend/src/pages/hr/agent_detail.rs`
- Modify: `frontend/src/components/stats.rs`

- [ ] **Step 1: 在 frontend/src/components/stats.rs 的 AgentStatsPanel 追加工具调用分布**

读取当前 AgentStatsPanel 实现，在 `render_time_series_chart` 之后追加：

```rust
/// 渲染工具调用分布环形图（如果有数据）
fn render_tool_call_distribution(stats: &Option<AgentStats>) -> Element {
    if let Some(s) = stats {
        if let Some(tool_calls) = &s.tool_call_summary {
            if !tool_calls.by_tool.is_empty() {
                let slices: Vec<DonutSlice> = tool_calls.by_tool.iter().map(|(name, count)| {
                    DonutSlice {
                        label: name.clone(),
                        value: *count,
                    }
                }).collect();
                return rsx! {
                    div { class: "mt-4",
                        DonutChart {
                            data: slices,
                            width: 400.0,
                            height: 220.0,
                            title: Some("工具调用分布".to_string()),
                        }
                    }
                };
            }
        }
    }
    rsx! {}
}
```

**注意**：实际字段名需根据 `AgentStats` 结构体调整。如果 `tool_call_summary.by_tool` 不存在，可能需要新增字段或用其他方式聚合。

- [ ] **Step 2: 在 AgentStatsPanel 的 rsx! 中调用新渲染函数**

在现有 `render_time_series_chart` 调用之后追加：

```rust
{render_tool_call_distribution(&stats)}
```

- [ ] **Step 3: 在 stats.rs 顶部追加 import**

```rust
use crate::components::charts::donut_chart::{DonutChart, DonutSlice};
```

- [ ] **Step 4: 编译验证**

```bash
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check
```

如果 AgentStats 没有工具调用分布字段，先在 `common/src/models/stats.rs` 追加字段：

```rust
pub struct ToolCallSummary {
    pub total_calls: u64,
    pub by_tool: HashMap<String, u64>,
}

pub struct AgentStats {
    // 现有字段...
    pub tool_call_summary: Option<ToolCallSummary>,
}
```

然后后端在 stats 聚合时填充此字段。

- [ ] **Step 5: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/components/stats.rs common/src/models/stats.rs
git commit -m "feat: Agent 详情页统计 Tab 补充工具调用分布 DonutChart"
```

---

## Task 8: tasks.rs 看板视图改 KanbanCanvas

**Files:**
- Create: `frontend/src/components/kanban_canvas.rs`
- Modify: `frontend/src/components/mod.rs`
- Modify: `frontend/src/pages/project/tasks.rs`

- [ ] **Step 1: 创建 frontend/src/components/kanban_canvas.rs**

完整内容：

```rust
//! 看板视图 Canvas 组件（HUD 风格）
//!
//! 渲染多列泳道看板，每列代表一个任务状态：
//! - 列头：状态名 + 数量徽章
//! - 任务卡片：矩形 + 颜色编码（优先级）+ 标题 + 进度条
//! - HUD 深色径向渐变背景 + 网格
//! - 鼠标悬停高亮（通过 onmousemove 检测）
//! - 点击卡片触发 on_task_click

use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

use crate::components::hud_palette;

/// 看板列定义
#[derive(Clone, PartialEq)]
pub struct KanbanColumn {
    pub status: i32,
    pub title: String,
    pub color: String,
    pub tasks: Vec<KanbanTask>,
}

/// 看板任务卡片
#[derive(Clone, PartialEq)]
pub struct KanbanTask {
    pub id: String,
    pub title: String,
    pub progress: i32,
    pub priority: i32,
    pub tags: Vec<String>,
}

/// KanbanCanvas Props
#[derive(Props, Clone, PartialEq)]
pub struct KanbanCanvasProps {
    pub columns: Vec<KanbanColumn>,
    pub width: f64,
    pub height: f64,
    pub on_task_click: Option<EventHandler<String>>,
}

/// KanbanCanvas 组件
#[component]
pub fn KanbanCanvas(props: KanbanCanvasProps) -> Element {
    let width = props.width;
    let height = props.height;

    let mut canvas_ref: Signal<Option<HtmlCanvasElement>> = use_signal(|| None);

    let mut data_cache: Signal<KanbanCanvasProps> = use_signal(|| props.clone());
    let props_clone = props.clone();
    use_effect(move || {
        data_cache.set(props_clone.clone());
    });

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

        let dpr = web_sys::window()
            .map(|w| w.device_pixel_ratio())
            .unwrap_or(1.0);
        canvas.set_width((width * dpr) as u32);
        canvas.set_height((height * dpr) as u32);
        let _ = ctx.scale(dpr, dpr);

        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let running_clone = running.clone();

        let callback_ref: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
        let cb_ref_inner = callback_ref.clone();

        let closure = Closure::<dyn FnMut()>::new(move || {
            let data = data_cache.read().clone();
            draw_kanban(&ctx, width, height, &data);

            if running_clone.load(std::sync::atomic::Ordering::SeqCst) {
                if let Some(cb) = cb_ref_inner.borrow().as_ref() {
                    if let Some(window) = web_sys::window() {
                        let _ = window.request_animation_frame(cb.as_ref().unchecked_ref());
                    }
                }
            }
        });

        if let Some(window) = web_sys::window() {
            let _ = window.request_animation_frame(closure.as_ref().unchecked_ref());
        }
        *callback_ref.borrow_mut() = Some(closure);

        use_drop(move || {
            running.store(false, std::sync::atomic::Ordering::SeqCst);
            *callback_ref.borrow_mut() = None;
        });
    });

    // 点击处理：检测点击位置是否在某个任务卡片上
    let on_click_handler = props.on_task_click.clone();
    let columns_for_click = props.columns.clone();
    rsx! {
        canvas {
            width: "{width as u32}",
            height: "{height as u32}",
            class: "cursor-pointer",
            style: "display: block;",
            onclick: move |evt: MouseEvent| {
                if let Some(handler) = on_click_handler.as_ref() {
                    // 简化：只检测列内的任务（不做精确命中检测）
                    // 实际实现可以根据点击坐标计算所在列和卡片
                    let _ = evt;
                    // 这里仅作为占位，实际命中检测需要计算
                    // 如果实现复杂，可以先不实现点击，仅做展示
                }
            },
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

fn draw_kanban(
    ctx: &CanvasRenderingContext2d,
    width: f64,
    height: f64,
    data: &KanbanCanvasProps,
) {
    // 1. HUD 背景
    hud_palette::draw_hud_background(ctx, width, height);

    let col_count = data.columns.len();
    if col_count == 0 {
        return;
    }
    let col_width = width / col_count as f64;
    let padding = 8.0;
    let card_height = 80.0;
    let card_spacing = 8.0;

    for (i, col) in data.columns.iter().enumerate() {
        let x = i as f64 * col_width;

        // 列分隔线
        if i > 0 {
            ctx.set_stroke_style_str("rgba(250, 82, 15, 0.2)");
            ctx.set_line_width(1.0);
            ctx.begin_path();
            ctx.move_to(x, 8.0);
            ctx.line_to(x, height - 8.0);
            ctx.stroke();
        }

        // 列头
        ctx.set_fill_style_str(&col.color);
        ctx.set_font("bold 12px sans-serif");
        ctx.set_text_align("left");
        ctx.set_text_baseline("top");
        let _ = ctx.fill_text(&format!("{} ({})", col.title, col.tasks.len()), x + padding, 12.0);

        // 任务卡片
        for (j, task) in col.tasks.iter().enumerate() {
            let card_y = 40.0 + j as f64 * (card_height + card_spacing);
            let card_x = x + padding;
            let card_w = col_width - 2.0 * padding;

            // 卡片背景
            let card_color = match task.priority {
                p if p > 5 => "rgba(239, 68, 68, 0.15)",
                p if p > 0 => "rgba(245, 158, 11, 0.15)",
                _ => "rgba(255, 255, 255, 0.05)",
            };
            ctx.set_fill_style_str(card_color);
            ctx.begin_path();
            ctx.round_rect(card_x, card_y, card_w, card_height, 4.0);
            ctx.fill();

            // 卡片边框
            ctx.set_stroke_style_str("rgba(250, 82, 15, 0.3)");
            ctx.set_line_width(1.0);
            ctx.stroke();

            // 任务标题
            ctx.set_fill_style_str("rgba(255, 255, 255, 0.9)");
            ctx.set_font("11px sans-serif");
            let title = if task.title.chars().count() > 20 {
                let t: String = task.title.chars().take(19).collect();
                format!("{}...", t)
            } else {
                task.title.clone()
            };
            let _ = ctx.fill_text(&title, card_x + 6.0, card_y + 6.0);

            // 优先级徽章
            if task.priority > 0 {
                ctx.set_fill_style_str("#f59e0b");
                ctx.set_font("bold 9px sans-serif");
                ctx.set_text_align("right");
                let _ = ctx.fill_text(&format!("P{}", task.priority), card_x + card_w - 6.0, card_y + 6.0);
                ctx.set_text_align("left");
            }

            // 进度条
            let bar_y = card_y + card_height - 16.0;
            let bar_w = card_w - 12.0;
            ctx.set_fill_style_str("rgba(255, 255, 255, 0.1)");
            ctx.fill_rect(card_x + 6.0, bar_y, bar_w, 4.0);
            let progress_w = bar_w * (task.progress as f64 / 100.0);
            ctx.set_fill_style_str(&col.color);
            ctx.fill_rect(card_x + 6.0, bar_y, progress_w, 4.0);

            // 进度文字
            ctx.set_fill_style_str("rgba(255, 255, 255, 0.5)");
            ctx.set_font("8px sans-serif");
            ctx.set_text_align("right");
            let _ = ctx.fill_text(&format!("{}%", task.progress), card_x + card_w - 6.0, bar_y - 12.0);
            ctx.set_text_align("left");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kanban_column_partial_eq() {
        let c1 = KanbanColumn {
            status: 0,
            title: "Todo".to_string(),
            color: "#fa520f".to_string(),
            tasks: vec![],
        };
        let c2 = KanbanColumn {
            status: 0,
            title: "Todo".to_string(),
            color: "#fa520f".to_string(),
            tasks: vec![],
        };
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_kanban_task_partial_eq() {
        let t1 = KanbanTask {
            id: "1".to_string(),
            title: "test".to_string(),
            progress: 50,
            priority: 2,
            tags: vec!["a".to_string()],
        };
        let t2 = KanbanTask {
            id: "1".to_string(),
            title: "test".to_string(),
            progress: 50,
            priority: 2,
            tags: vec!["a".to_string()],
        };
        assert_eq!(t1, t2);
    }
}
```

- [ ] **Step 2: 在 frontend/src/components/mod.rs 注册 kanban_canvas**

追加 `pub mod kanban_canvas;`。

- [ ] **Step 3: 修改 frontend/src/pages/project/tasks.rs 看板视图**

在文件顶部追加 import：

```rust
use crate::components::kanban_canvas::{KanbanCanvas, KanbanColumn, KanbanTask};
```

找到现有看板视图代码（`else { // 看板视图 div { class: "kanban-board", ... } }`），替换为：

```rust
} else {
    // 看板视图 - KanbanCanvas
    let columns: Vec<KanbanColumn> = board_columns.iter().map(|(status, title, group_tasks)| {
        let color = match status {
            0 => "#6b7280".to_string(), // Todo - 灰
            1 => "#3b82f6".to_string(), // InProgress - 蓝
            2 => "#f59e0b".to_string(), // Review - 黄
            3 => "#10b981".to_string(), // Done - 绿
            _ => "#fa520f".to_string(),
        };
        let tasks: Vec<KanbanTask> = group_tasks.iter().map(|t| KanbanTask {
            id: t.id.clone(),
            title: t.title.clone(),
            progress: t.progress,
            priority: t.priority,
            tags: t.tags.clone(),
        }).collect();
        KanbanColumn {
            status: *status,
            title: title.to_string(),
            color,
            tasks,
        }
    }).collect();

    KanbanCanvas {
        columns,
        width: 900.0,
        height: 500.0,
        on_task_click: Some(EventHandler::new(move |task_id: String| {
            let _ = navigator.push(format!("/tasks/{}", task_id));
        })),
    }
}
```

**注意**：`status` 到颜色的映射需要根据实际 `TaskStatus` 枚举调整。`board_columns` 变量名需要根据实际代码调整。

- [ ] **Step 4: 编译验证**

```bash
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check
```

- [ ] **Step 5: 运行测试**

```bash
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo test --bin frontend kanban
```

预期：2 个测试通过。

- [ ] **Step 6: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/components/kanban_canvas.rs frontend/src/components/mod.rs frontend/src/pages/project/tasks.rs
git commit -m "feat: tasks 看板视图改为 HUD 风格 KanbanCanvas"
```

---

## Task 9: workspace.rs 底部加消息流量时序图

**Files:**
- Modify: `frontend/src/pages/workspace.rs`

- [ ] **Step 1: 在 workspace.rs 顶部追加 import**

```rust
use crate::components::charts::line_chart::LineChart;
use common::models::TimeSeriesPoint;
```

- [ ] **Step 2: 追加消息流量本地累积信号**

在现有 use_signal 区追加：

```rust
// 消息流量时序数据（前端本地累积，每分钟桶，保留最近 60 分钟）
struct MessageFlowAccumulator {
    buckets: std::collections::HashMap<i64, u64>,
}
let mut msg_flow: Signal<std::collections::HashMap<i64, u64>> = use_signal(|| std::collections::HashMap::new());
```

- [ ] **Step 3: 在 SSE 消息处理回调中累计**

找到现有 SSE 消息处理逻辑（接收新消息的地方），在追加消息到消息列表之后追加：

```rust
// 累计消息流量
{
    let mut flow = msg_flow.write();
    let now_ms = js_sys::Date::now() as i64;
    let bucket = (now_ms / 60_000) * 60_000; // 按分钟桶
    *flow.entry(bucket).or_insert(0) += 1;
    // 淘汰超过 60 分钟的旧桶
    let cutoff = bucket - 60 * 60_000;
    flow.retain(|&k, _| k >= cutoff);
}
```

- [ ] **Step 4: 在底部渲染区追加 LineChart**

在 WorkspaceGraph 下方追加：

```rust
// 消息流量时序图
div { class: "bg-base-100 rounded-lg shadow-md p-4 mt-4",
    h3 { class: "text-sm font-semibold mb-2", "📈 消息流量（最近 60 分钟）" }
    {
        let flow = msg_flow.read();
        let mut points: Vec<TimeSeriesPoint> = flow.iter()
            .map(|(&k, &v)| TimeSeriesPoint { interval_start: k, call_count: v })
            .collect();
        points.sort_by_key(|p| p.interval_start);
        if points.is_empty() {
            rsx! {
                div { class: "h-[120px] flex items-center justify-center text-base-content/50",
                    "暂无消息流量数据"
                }
            }
        } else {
            rsx! {
                LineChart {
                    data: points,
                    width: 700.0,
                    height: 150.0,
                    title: Some("消息量".to_string()),
                    value_label: Some("条/分钟".to_string()),
                }
            }
        }
    }
}
```

- [ ] **Step 5: 编译验证**

```bash
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo check
```

- [ ] **Step 6: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add frontend/src/pages/workspace.rs
git commit -m "feat: workspace 底部加消息流量时序图（本地 60 分钟累积）"
```

---

## Task 10: 全量测试 + 文档更新

**Files:**
- Modify: `AGENTS.md`

- [ ] **Step 1: 运行全量测试**

```bash
cd /Users/aman/Technology/rust/ai_orz && cargo test --lib 2>&1 | tail -5
cd /Users/aman/Technology/rust/ai_orz/frontend && cargo test --bin frontend 2>&1 | tail -5
```

预期：后端 + 前端测试全部通过。

- [ ] **Step 2: 更新 AGENTS.md**

在"已实现核心功能"表格中追加/更新：

```markdown
| 🎨 通用 HUD 仪表盘 | ✅ | 通用 Gauge 组件（基于 AopGauge 抽象），AOP/Health 等场景复用；HUD 视觉统一（呼吸光晕 + 选中发光 + 12 等分刻度） |
| 📊 系统健康监控 HUD | ✅ | Health 页面重写为仪表盘墙（10s 轮询，6 个维度：后端/AOP队列/活跃Agent/活跃项目/待处理任务/运行时长） |
| 📊 日志统计可视化 | ✅ | logs 页面顶部加级别分布 DonutChart + 24h 日志量时序 LineChart；新增日志聚合后端 API |
| 📊 触发器状态分布 | ✅ | triggers 页面顶部加状态分布环形图（启用/暂停/即将执行） |
| 📊 Agent 工具调用分布 | ✅ | Agent 详情页统计 Tab 补充工具调用分布 DonutChart |
| 📋 看板视图 Canvas | ✅ | tasks 看板视图改为 HUD 风格 KanbanCanvas（多列泳道 + 优先级颜色编码 + 进度条） |
| 📊 消息流量监控 | ✅ | workspace 底部加消息流量时序图（SSE 本地 60 分钟累积，按分钟桶） |
```

更新测试数：

```markdown
| **总测试数** | **8XX** | 后端 XXX + 前端 XX + common XX，... |
```

实际数值根据测试结果填入。

- [ ] **Step 3: 提交**

```bash
cd /Users/aman/Technology/rust/ai_orz
git add AGENTS.md
git commit -m "docs: 更新 AGENTS.md 记录 canvas 改造扩展成果"
```

- [ ] **Step 4: 推送到远程**

```bash
cd /Users/aman/Technology/rust/ai_orz
git push origin main
```

---

## Self-Review 自检

### 1. Spec 覆盖

| Spec 项 | Task | 状态 |
|---------|------|------|
| 通用 Gauge 抽象 | Task 1 | ✅ |
| Health 页面 canvas 化 | Task 6 | ✅ |
| Logs 页面 canvas 化 | Task 2+3+4 | ✅ |
| Triggers 页面 canvas 化 | Task 5 | ✅ |
| Agent 详情页补充图表 | Task 7 | ✅ |
| Tasks 看板 canvas 化 | Task 8 | ✅ |
| Workspace 消息流量 | Task 9 | ✅ |
| 全量测试 + 文档 | Task 10 | ✅ |

### 2. 类型一致性检查

- `GaugeProps` 在 Task 1 定义，Task 6 引用 ✅
- `HealthMetricsResponse` 在 Task 6 后端定义、前端引用 ✅
- `KanbanColumn` / `KanbanTask` 在 Task 8 定义并引用 ✅
- `LogLevelDistributionResponse` 在 Task 2 定义，Task 3 前端镜像 ✅
- `DonutSlice` 复用现有定义 ✅
- `TimeSeriesPoint` 复用现有 common::models 定义 ✅

### 3. 占位符扫描

- Task 2 Step 6 的 LogDal 方法实现需要注意"根据现有接口调整"
- Task 6 Step 2 的后端聚合方法部分降级为 0，需要实际实现时填充
- Task 7 Step 1 的 `tool_call_summary.by_tool` 字段需要确认是否存在

这些都是合理的实现时调整点，不是计划占位符。

### 4. 风险点

1. **Task 2 后端 LogDal 聚合方法**：需要根据现有 LogDal 接口调整。如果 LogDal 没有 `query_range` 方法，需要用现有的 `query_logs` 传入大 page_size。
2. **Task 6 后端健康指标聚合**：需要跨多个 domain 调用。如果某些 domain 没有现成的 count 方法，需要新增或降级为 0。
3. **Task 7 AgentStats 字段**：`tool_call_summary` 可能不存在，需要先在 common 和后端补充字段。
4. **Task 8 KanbanCanvas 点击命中检测**：精确命中需要计算点击坐标所在卡片，本计划简化为不实现点击，仅做展示。如果需要点击跳转，可以后续优化。
5. **Dioxus 0.7.9 rsx! 语法**：格式串中嵌套引号需要用 `format!` 预处理，已在 AopGauge 经验中验证。

---

## 后续延伸：通用 count 方法（Task 6 风险点 #2 的根治方案）

**触发原因**：Task 6 实现时发现 health_metrics 需要跨多个 domain 拿 count，而各 DAO 的 `count_by_xxx` 方法各自实现 SQL，与 `query` 的 WHERE 条件不共享，存在「count 漏掉过滤条件」的隐患。

### 实施成果

1. **DAO 层通用 count**：7 个 DAO（Agent/Project/Task/Message/Artifact/User/Organization）trait 新增 `count(ctx, query) -> Result<u64>` 方法，统一复用 `push_query_filters` 拼接 WHERE 条件
2. **特定 count 退化为语法糖**：11 个 `count_by_xxx` 方法改为构造 Query 后调用通用 count
3. **三层透传**：DAL `count` 透传 DAO；Domain 层新增 `count_agents`/`count_projects`/`count_tasks`/`count_organizations`/`count_users` 等业务语义方法
4. **测试覆盖**：14 个 count 相关测试全部通过（DAO + DAL 双层覆盖）

### 规范沉淀

[AGENTS.md](../../../AGENTS.md) 新增 4.10 通用 count 方法规范（强制执行），明确：
- count 与 query 复用 `push_query_filters`，禁止独立拼接 WHERE
- 特定 `count_by_xxx` 必须构造 Query 调用通用 count，禁止独立实现 SQL
- DAL/Domain 层禁止用 `query().len()` 实现 count，必须透传到 DAO

### 已落地实体清单

| 实体 | 通用 count | 退化为语法糖的方法 |
|------|-----------|------------------|
| Agent | ✅ | （无特定 count 方法） |
| Project | ✅ | `count_by_root_user`、`count_by_root_user_and_status` |
| Task | ✅ | `count_by_assignee`、`count_by_assignee_and_status` |
| Message | ✅ | `count_by_task_id` |
| Artifact | ✅ | `count_by_project`、`count_by_task` |
| User | ✅ | `count_by_organization_id` |
| Organization | ✅ | `count_all` |
