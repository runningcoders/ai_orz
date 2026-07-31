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
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
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
        let center_label_inner = center_label_c.clone();

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
                center_label_inner.as_deref(),
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

    // 计算总数用于图例百分比
    let total: u64 = props.data.iter().map(|s| s.value).sum();
    // 预处理图例数据：把百分比预格式化为字符串（rsx 宏不支持 {:.1} 格式化语法）
    let legend_data: Vec<(DonutSlice, String)> = props
        .data
        .iter()
        .map(|slice| {
            let percent = if total > 0 {
                (slice.value as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            (slice.clone(), format!("({:.1}%)", percent))
        })
        .collect();

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
                    for (slice, percent_str) in legend_data.iter() {
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
                                "{percent_str}"
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
    ctx.set_stroke_style_str(&hud_palette::hex_to_rgba(
        hud_palette::HUD_PRIMARY,
        glow_alpha,
    ));
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
        let _ = ctx.arc_with_anticlockwise(cx, cy, inner_r, end_angle, start_angle, true); // 逆时针
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
        let data = [
            DonutSlice {
                label: "A".into(),
                value: 3,
                color: "#fff".into(),
            },
            DonutSlice {
                label: "B".into(),
                value: 5,
                color: "#fff".into(),
            },
            DonutSlice {
                label: "C".into(),
                value: 2,
                color: "#fff".into(),
            },
        ];
        let total: u64 = data.iter().map(|s| s.value).sum();
        assert_eq!(total, 10);
    }
}
