//! HUD 驾驶舱风格背景工具
//!
//! 提供统一的 HUD 视觉元素绘制函数，供 CanvasScene（知识图谱）和 ChartScene（统计图表）共享：
//! - 深色径向渐变背景（基底 #0a0e1a + 橙色 #fa520f 中心光晕）
//! - 淡橙色网格线（40px 间距）
//! - 四角 HUD 装饰刻度线
//!
//! 视觉锚点：橙色 #fa520f（rgb 250, 82, 15）贯穿背景、网格、四角，形成统一驾驶舱观感。

use web_sys::CanvasRenderingContext2d;

/// HUD 主色（橙色）
pub const HUD_PRIMARY: &str = "#fa520f";
/// HUD 主色 RGB 元组
#[allow(dead_code)]
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
