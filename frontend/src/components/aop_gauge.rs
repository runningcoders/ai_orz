//! AOP 消费者仪表盘组件（HUD 驾驶舱风格）
//!
//! 每个消费者一个圆形仪表盘，实时显示队列状态：
//! - 中心大数字：pending（待处理事件数）
//! - 副标题：消费者名称（顶部）
//! - 右上角徽章：in_progress（处理中数）
//! - 底部：order_keys 数量 + 最老事件年龄
//! - 颜色编码：
//!   - 绿色 (#10b981)：pending=0 && in_progress=0（idle 健康）
//!   - 黄色 (#f59e0b)：pending=0 && in_progress>0（处理中）
//!   - 橙色 (#fa520f)：0 < pending < 10（正常负载）
//!   - 红色 (#ef4444)：pending >= 10（堆积告警）
//! - 选中状态：加强发光边框
//! - 呼吸光晕（2.4s 周期，与 LineChart 一致）
//!
//! 点击 canvas 触发 on_click 回调（用于切换事件列表）

use dioxus::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

use crate::components::hud_palette;

/// AopGauge 组件 Props
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

/// AopGauge 组件
#[component]
pub fn AopGauge(props: AopGaugeProps) -> Element {
    let width = 200.0_f64;
    let height = 200.0_f64;

    let mut canvas_ref: Signal<Option<HtmlCanvasElement>> = use_signal(|| None);

    // 数据 cache：props 变化时更新
    let mut data_cache: Signal<AopGaugeProps> = use_signal(|| props.clone());
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

/// 绘制完整仪表盘
fn draw_gauge(
    ctx: &CanvasRenderingContext2d,
    width: f64,
    height: f64,
    data: &AopGaugeProps,
    now: f64,
) {
    // 1. HUD 背景
    hud_palette::draw_hud_background(ctx, width, height);

    let cx = width / 2.0;
    let cy = height / 2.0;
    let radius = width.min(height) / 2.0 - 18.0;

    let color = status_color(data.pending, data.in_progress);

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

    // 6. 中心数字（pending 大数字）
    ctx.set_fill_style_str(color);
    ctx.set_font("bold 36px sans-serif");
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    let center_text = if data.pending == 0 && data.in_progress == 0 {
        "OK".to_string()
    } else {
        data.pending.to_string()
    };
    let _ = ctx.fill_text(&center_text, cx, cy - 4.0);

    // 7. 中心副标签
    ctx.set_fill_style_str("rgba(255, 255, 255, 0.6)");
    ctx.set_font("10px sans-serif");
    let _ = ctx.fill_text("pending", cx, cy + 18.0);

    // 8. 顶部：消费者名称
    ctx.set_fill_style_str("rgba(250, 82, 15, 0.9)");
    ctx.set_font("bold 11px sans-serif");
    ctx.set_text_baseline("top");
    // 名称过长截断（按字符数粗略截断，避免越界 char 边界）
    let name = if data.consumer_name.chars().count() > 22 {
        let truncated: String = data.consumer_name.chars().take(21).collect();
        format!("{}...", truncated)
    } else {
        data.consumer_name.clone()
    };
    let _ = ctx.fill_text(&name, cx, 8.0);

    // 9. 右上角徽章：in_progress（处理中）
    if data.in_progress > 0 {
        ctx.set_fill_style_str("#f59e0b");
        ctx.set_font("bold 10px sans-serif");
        ctx.set_text_align("right");
        let badge_text = format!("⚙ {}", data.in_progress);
        let _ = ctx.fill_text(&badge_text, width - 6.0, 8.0);
        ctx.set_text_align("center"); // 重置
    }

    // 10. 底部：order_keys 数量 + 最老事件年龄
    ctx.set_fill_style_str("rgba(255, 255, 255, 0.5)");
    ctx.set_font("9px sans-serif");
    ctx.set_text_baseline("bottom");
    let mut bottom_text = format!("{} order_keys", data.order_keys_count);
    if let Some(age) = data.oldest_age_secs {
        bottom_text.push_str(&format!(" · {}s ago", age));
    }
    let _ = ctx.fill_text(&bottom_text, cx, height - 6.0);
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
