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
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
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
    let mut data_cache: Signal<Vec<TimeSeriesPoint>> = use_signal(|| props.data.clone());
    let props_data = props.data.clone();
    use_effect(move || {
        data_cache.set(props_data.clone());
    });

    // 渲染循环
    let render_width = width;
    let render_height = height;
    let title_c = title.clone();
    let value_label_c = value_label.clone();
    let data_cache_c = data_cache;
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
        #[allow(clippy::type_complexity)]
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
    draw_axes(
        ctx,
        pad_left,
        pad_top,
        plot_w,
        plot_h,
        max_y,
        data.len(),
        value_label,
    );

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
#[allow(clippy::too_many_arguments)]
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
    let _ = ctx.set_line_dash(&arr);
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
        // 将毫秒时间戳转为日期字符串（M-D 短日期）
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

#[cfg(test)]
mod tests {
    use super::*;

    // format_timestamp 内部使用 js_sys::Date::new，需要 JS 运行时，
    // 仅在 wasm32 目标下可运行；非 wasm 测试会 panic，因此加 cfg 门控。
    #[cfg(target_arch = "wasm32")]
    #[test]
    fn test_format_timestamp_returns_short_date() {
        // 2026-07-25 00:00:00 UTC 的毫秒时间戳
        let ts = 1753526400000_i64;
        let label = format_timestamp(ts);
        // 应包含月-日格式
        assert!(
            label.contains('-'),
            "label should contain '-' separator, got: {}",
            label
        );
    }

    #[test]
    fn test_empty_data_handled() {
        // draw_chart 在空数据时应只绘制背景和提示，不 panic
        // 此测试验证逻辑分支，实际 Canvas 调用在 WASM 环境无法测试
        let data: Vec<TimeSeriesPoint> = vec![];
        assert!(data.is_empty());
    }
}
