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
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
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
    #[props(default = 200.0)]
    pub width: f64,
    /// 画布高度
    #[props(default = 200.0)]
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

    // 数据 cache：props 变化时更新（RAF 渲染闭包存活整个组件生命周期，只能经信号取当前值）。
    let mut data_cache: Signal<GaugeProps> = use_signal(|| props.clone());
    // ⚠️ 必须用 `use_reactive` 显式声明依赖：`use_effect` 只对**信号**依赖重跑，
    // 闭包捕获的 props 是普通值 —— 裸 `use_effect(move || data_cache.set(..))`
    // 只在挂载时执行一次，cache 停在首帧 props（仪表盘读数不随数据刷新）。
    use_effect(use_reactive(&props, move |p: GaugeProps| {
        data_cache.set(p);
    }));

    // 渲染循环资源：保存 RAF 回调 + running flag + pending 帧句柄，供顶层 use_drop 清理
    #[allow(clippy::type_complexity)]
    struct RafResource {
        running: std::sync::Arc<std::sync::atomic::AtomicBool>,
        callback_ref: Rc<RefCell<Option<Closure<dyn FnMut()>>>>,
        // 当前「已注册未触发」的 rAF 帧句柄（0 表示无）。卸载时必须先 cancel 再释放
        // Closure，否则浏览器下一帧会调用已 drop 的 Closure，抛出
        // 「closure invoked recursively or after being dropped」
        pending_frame: Rc<std::cell::Cell<i32>>,
    }
    let mut raf_resource = use_signal(|| Option::<RafResource>::None);

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

        #[allow(clippy::type_complexity)]
        let callback_ref: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
        let cb_ref_inner = callback_ref.clone();

        // 当前已注册未触发的 rAF 帧句柄：卸载时凭此 cancel，防止已 drop 的 Closure 仍被浏览器调用
        let pending_frame = Rc::new(std::cell::Cell::new(0i32));
        let pending_frame_inner = pending_frame.clone();

        let closure = Closure::<dyn FnMut()>::new(move || {
            // 组件卸载后仍可能被「已调度未取消」的帧触发一次：直接早退，不渲染不写信号
            if !running_clone.load(std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            let data = data_cache.read().clone();
            let now = js_sys::Date::now() / 1000.0;
            draw_gauge(&ctx, width, height, &data, now);

            // 递归注册下一帧（捕获句柄供卸载时 cancel）
            if running_clone.load(std::sync::atomic::Ordering::SeqCst)
                && let Some(cb) = cb_ref_inner.borrow().as_ref()
                && let Some(window) = web_sys::window()
                && let Ok(id) = window.request_animation_frame(cb.as_ref().unchecked_ref())
            {
                pending_frame_inner.set(id);
            }
        });

        // 初始注册第一帧（捕获句柄供卸载时 cancel）
        if let Some(window) = web_sys::window()
            && let Ok(id) = window.request_animation_frame(closure.as_ref().unchecked_ref())
        {
            pending_frame.set(id);
        }
        *callback_ref.borrow_mut() = Some(closure);

        raf_resource.set(Some(RafResource {
            running,
            callback_ref,
            pending_frame,
        }));
    });

    use_drop(move || {
        if let Some(res) = raf_resource.take() {
            res.running
                .store(false, std::sync::atomic::Ordering::SeqCst);
            // 关键：先取消「已注册未触发」的 rAF 帧，再释放 Closure。
            // 否则浏览器下一帧调用已 drop 的 Closure，抛出
            // 「closure invoked recursively or after being dropped」。
            if let Some(window) = web_sys::window() {
                let id = res.pending_frame.get();
                if id > 0 {
                    let _ = window.cancel_animation_frame(id);
                }
            }
            *res.callback_ref.borrow_mut() = None;
        }
    });

    // 点击处理
    let on_click_handler = props.on_click;
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
            on_click: handler,
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
        // 用 `assert!(==)` 而非 `assert_eq!`，避免 GaugeProps 需要 Debug
        // （EventHandler<()> 不实现 Debug）
        assert!(p1 == p2);
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
        assert!(p1 != p2);
    }
}
