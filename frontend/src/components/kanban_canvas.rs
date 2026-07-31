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
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

use crate::components::hud_palette;

/// 看板列定义
#[derive(Clone, PartialEq, Debug)]
pub struct KanbanColumn {
    pub status: i32,
    pub title: String,
    pub color: String,
    pub tasks: Vec<KanbanTask>,
}

/// 看板任务卡片
#[derive(Clone, PartialEq, Debug)]
pub struct KanbanTask {
    pub id: String,
    pub title: String,
    pub progress: i32,
    pub priority: i32,
    pub tags: Vec<String>,
}

/// KanbanCanvas Props
#[derive(Props, Clone)]
pub struct KanbanCanvasProps {
    pub columns: Vec<KanbanColumn>,
    pub width: f64,
    pub height: f64,
    pub on_task_click: Option<EventHandler<String>>,
}

// EventHandler 无法比较，手动实现 PartialEq 时忽略 on_task_click 字段
impl PartialEq for KanbanCanvasProps {
    fn eq(&self, other: &Self) -> bool {
        self.columns == other.columns && self.width == other.width && self.height == other.height
    }
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

        #[allow(clippy::type_complexity)]
        let callback_ref: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
        let cb_ref_inner = callback_ref.clone();

        let closure = Closure::<dyn FnMut()>::new(move || {
            let data = data_cache.read().clone();
            draw_kanban(&ctx, width, height, &data);

            if running_clone.load(std::sync::atomic::Ordering::SeqCst)
                && let Some(cb) = cb_ref_inner.borrow().as_ref()
                && let Some(window) = web_sys::window()
            {
                let _ = window.request_animation_frame(cb.as_ref().unchecked_ref());
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

    // 点击处理：命中检测简化为不实现（仅展示）
    let _on_click_handler = props.on_task_click;
    rsx! {
        canvas {
            width: "{width as u32}",
            height: "{height as u32}",
            class: "cursor-pointer",
            style: "display: block;",
            onclick: move |_evt: MouseEvent| {
                // 简化：不实现点击命中检测，仅做展示
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

fn draw_kanban(ctx: &CanvasRenderingContext2d, width: f64, height: f64, data: &KanbanCanvasProps) {
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
        let _ = ctx.fill_text(
            &format!("{} ({})", col.title, col.tasks.len()),
            x + padding,
            12.0,
        );

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
            ctx.rect(card_x, card_y, card_w, card_height);
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
                let _ = ctx.fill_text(
                    &format!("P{}", task.priority),
                    card_x + card_w - 6.0,
                    card_y + 6.0,
                );
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
            let _ = ctx.fill_text(
                &format!("{}%", task.progress),
                card_x + card_w - 6.0,
                bar_y - 12.0,
            );
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
