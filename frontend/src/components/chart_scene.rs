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
