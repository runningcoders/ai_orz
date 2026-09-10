//! HUD 风格环形进度组件（轻量 SVG 版）
//!
//! 与 `components/gauge.rs`（Canvas 大仪表盘，AOP 驾驶舱用）区分：这里是**纯 SVG
//! 小控件**，无 Canvas、无 RAF 动画，适合塞进侧栏 / 卡片等窄容器。
//!
//! 设计约定：
//! - 只吃**原始数值**，百分比在组件内换算——后端刻意不预先计算，这样阈值策略
//!   （如压缩阈值改为 max_context * 0.7）调整时前端无需跟着改。
//! - 视觉对齐 HUD：低对比轨道 + 语义色进度环 + 同色微光；按占用阶段自动变色
//!   （<60% 正常 / 60–85% 偏高 / ≥85% 临界）。
//! - 中心只显示百分比，**原始数值走 hover**（`title`），控件本体专注进度表达。
//!
//! ```rust
//! RingProgress {
//!     value: 12_800,
//!     max: 32_000,
//!     caption: Some("上下文".to_string()),
//!     title: Some("12,800 / 32,000 tokens".to_string()),
//!     size: None,
//! }
//! ```

use dioxus::prelude::*;

/// 阶段变色阈值（百分比）：低于 WARN 为正常色，低于 DANGER 为偏高色，否则临界色
const STAGE_WARN: f64 = 60.0;
const STAGE_DANGER: f64 = 85.0;

/// 环形几何（viewBox 单位）：半径 40 + 描边 9 → 周长 2πr
const RADIUS: f64 = 40.0;
const STROKE: f64 = 9.0;
const CIRCUMFERENCE: f64 = 2.0 * std::f64::consts::PI * RADIUS;

/// 环形进度组件 Props
#[derive(Props, Clone, PartialEq)]
pub struct RingProgressProps {
    /// 当前值（原始数，如 prompt token 数）
    pub value: u64,
    /// 上限 / 阈值（原始数，同口径）。**0 表示未配置**，此时不渲染
    pub max: u64,
    /// 环下方说明文字（可选，如 "上下文"）
    pub caption: Option<String>,
    /// 悬停提示：展示原始数值（由调用方格式化，组件不做业务假设）
    pub title: Option<String>,
    /// 直径（CSS 像素），默认 56
    pub size: Option<u32>,
}

/// 按占用阶段返回语义色（走 `--color-*` 变量，双主题自适应）
fn stage_color(pct: f64) -> &'static str {
    if pct >= STAGE_DANGER {
        "var(--color-error)"
    } else if pct >= STAGE_WARN {
        "var(--color-warning)"
    } else {
        "var(--color-success)"
    }
}

/// 环形进度组件
///
/// `max == 0`（阈值未配置）时返回空节点，由调用方决定退化展示方式。
#[component]
pub fn RingProgress(props: RingProgressProps) -> Element {
    let size = props.size.unwrap_or(56);
    // 百分比前端换算（后端只给原始值）；上限保护 100%，避免超阈值时环画过头
    let pct = if props.max == 0 {
        0.0
    } else {
        (props.value as f64 / props.max as f64 * 100.0).clamp(0.0, 100.0)
    };
    let color = stage_color(pct);
    let offset = CIRCUMFERENCE * (1.0 - pct / 100.0);
    let pct_text = format!("{}%", pct.round() as u32);

    rsx! {
        div {
            class: "flex flex-col items-center gap-1 select-none",
            title: "{props.title.clone().unwrap_or_default()}",
            svg {
                class: "block overflow-visible",
                width: "{size}",
                height: "{size}",
                view_box: "0 0 100 100",
                "aria-label": "{props.caption.clone().unwrap_or_else(|| \"进度\".to_string())} {pct_text}",
                // 轨道：base-content 12% 的低对比环
                circle {
                    cx: "50",
                    cy: "50",
                    r: "{RADIUS}",
                    fill: "none",
                    stroke_width: "{STROKE}",
                    style: "stroke: color-mix(in oklab, var(--color-base-content) 12%, transparent)",
                }
                // 进度环：语义色 + 同色微光，从 12 点起顺时针
                circle {
                    cx: "50",
                    cy: "50",
                    r: "{RADIUS}",
                    fill: "none",
                    stroke_width: "{STROKE}",
                    stroke_linecap: "round",
                    stroke_dasharray: "{CIRCUMFERENCE}",
                    stroke_dashoffset: "{offset}",
                    transform: "rotate(-90 50 50)",
                    style: "stroke: {color}; filter: drop-shadow(0 0 3px {color}); transition: stroke-dashoffset .4s ease, stroke .4s ease",
                }
                text {
                    x: "50",
                    y: "50",
                    text_anchor: "middle",
                    dominant_baseline: "central",
                    font_size: "25",
                    font_weight: "600",
                    style: "fill: var(--color-base-content)",
                    "{pct_text}"
                }
            }
            if let Some(caption) = props.caption.as_ref() {
                span { class: "text-[10px] leading-none text-base-content/55", "{caption}" }
            }
        }
    }
}
