//! HUD 驾驶舱风格布局原语
//!
//! 封装 `styles/input.css` 中已落地的 HUD class（`.hud-panel` / `.hud-eyebrow` /
//! `.hud-stat` / `.hud-signal` 等），作为全站统一的可复用"积木"。
//!
//! 设计约定：
//! - 全部走 `--color-*` 语义变量，双主题（orz-hud / orz-light）自适应。
//! - 不引入业务语义；状态徽章仍由 `utils::status` 单一事实源提供，此处不重复。
//! - 标题用 `font-display`（Chakra Petch）制造航天 / 仪表盘观感；数据用 `font-mono` + tabular-nums。

use dioxus::prelude::*;

/// 统一页头：eyebrow 小标签 + 大标题 + 右侧操作区。
/// 替代散落在各页的 `h1.text-2xl.font-bold`，给全站页头一致层级。
#[component]
pub fn PageHeader(eyebrow: Option<String>, title: String, actions: Option<Element>) -> Element {
    rsx! {
        div { class: "mb-6",
            if let Some(eb) = eyebrow {
                div { class: "hud-eyebrow mb-1", "{eb}" }
            }
            div { class: "flex items-end justify-between gap-4 flex-wrap",
                h1 { class: "font-display text-2xl font-bold tracking-tight", "{title}" }
                if let Some(a) = actions {
                    div { class: "flex items-center gap-2", {a} }
                }
            }
        }
    }
}

/// 切角 HUD 面板：1px 渐变发丝边 + 内网格 + 可选顶部信号流光。
/// 替代通用的 `card.bg-base-100.shadow-md`。
///
/// - `title`：面板标题（显示在 `.hud-panel-header` 行）。
/// - `eyebrow`：标题上方等宽大写小标签。
/// - `signal`：是否显示顶部流光条（仪表盘"在线"感）。
/// - `actions`：标题行右侧操作区（按钮等）。
/// - `class`：附加到根 `.hud-panel` 的额外类。
#[component]
pub fn HudPanel(
    title: Option<String>,
    eyebrow: Option<String>,
    signal: Option<bool>,
    actions: Option<Element>,
    extra_class: Option<String>,
    children: Element,
) -> Element {
    let root = format!("hud-panel {}", extra_class.unwrap_or_default());
    let has_header = title.is_some() || actions.is_some();
    rsx! {
        div { class: "{root}",
            if signal.unwrap_or(false) {
                div { class: "hud-signal" }
            }
            if has_header {
                div { class: "hud-panel-header px-5 pt-4 pb-3",
                    div { class: "min-w-0",
                        if let Some(eb) = eyebrow {
                            div { class: "hud-eyebrow mb-0.5", "{eb}" }
                        }
                        if let Some(t) = title {
                            div { class: "font-display text-lg font-semibold tracking-tight truncate", "{t}" }
                        }
                    }
                    if let Some(a) = actions {
                        div { class: "flex items-center gap-2 shrink-0", {a} }
                    }
                }
            }
            div { class: if has_header { "hud-panel-body px-5 pb-5" } else { "hud-panel-body p-5" },
                {children}
            }
        }
    }
}

/// 轻量分区：eyebrow + 标题，无面板边框。用于页内子分组。
/// `actions` 提供标题行右侧操作区（按钮等），与 HudPanel 对齐。
#[component]
pub fn HudSection(
    eyebrow: Option<String>,
    title: String,
    extra_class: Option<String>,
    actions: Option<Element>,
    children: Element,
) -> Element {
    let root = format!("mb-5 {}", extra_class.unwrap_or_default());
    rsx! {
        div { class: "{root}",
            div { class: "flex items-end justify-between gap-4 flex-wrap mb-3",
                div { class: "min-w-0",
                    if let Some(eb) = eyebrow {
                        div { class: "hud-eyebrow mb-1", "{eb}" }
                    }
                    h2 { class: "font-display text-base font-semibold tracking-tight", "{title}" }
                }
                if let Some(a) = actions {
                    div { class: "flex items-center gap-2 shrink-0", {a} }
                }
            }
            {children}
        }
    }
}

/// 大号数值读数：等宽 eyebrow 标签 + tabular-nums 数值 + 可选单位 / 图标 / 增量。
/// 替代 `stats.rs` 的 `StatsCard`（DaisyUI `stat`）。
#[component]
pub fn StatReadout(
    label: String,
    value: String,
    unit: Option<String>,
    icon: Option<String>,
    delta: Option<String>,
    accent: Option<String>,
) -> Element {
    let value_class = match accent.as_deref() {
        Some("primary") => "hud-stat text-primary",
        Some("accent") => "hud-stat text-accent",
        Some("success") => "hud-stat text-success",
        Some("info") => "hud-stat text-info",
        Some("warning") => "hud-stat text-warning",
        Some("error") => "hud-stat text-error",
        _ => "hud-stat",
    };
    rsx! {
        div { class: "min-w-0",
            div { class: "hud-eyebrow mb-1", "{label}" }
            div { class: "flex items-baseline gap-1.5",
                if let Some(ic) = icon {
                    span { class: "text-lg leading-none opacity-80", "{ic}" }
                }
                span { class: "{value_class}",
                    "{value}"
                    if let Some(u) = unit {
                        span { class: "u", "{u}" }
                    }
                }
            }
            if let Some(d) = delta {
                div { class: "text-xs mt-1 font-mono opacity-70", "{d}" }
            }
        }
    }
}

/// 数值读数网格容器（响应式列）。
#[component]
pub fn StatGrid(children: Element) -> Element {
    rsx! {
        div { class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 gap-4",
            {children}
        }
    }
}

/// HUD 风格提示条：替代散落的裸 `alert alert-info/warning/error`。
/// `tone`：info（默认）/ warning / error / success，决定边框、底色、文字色。
#[component]
pub fn HudCallout(tone: Option<String>, extra_class: Option<String>, children: Element) -> Element {
    let tone_class = match tone.as_deref() {
        Some("error") => "border-error/40 bg-error/10 text-error",
        Some("success") => "border-success/40 bg-success/10 text-success",
        Some("warning") => "border-warning/40 bg-warning/10 text-warning",
        _ => "border-info/40 bg-info/10 text-info",
    };
    let root = format!(
        "hud-callout rounded-md border px-4 py-3 text-sm {} {}",
        tone_class,
        extra_class.unwrap_or_default()
    );
    rsx! {
        div { class: "{root}", {children} }
    }
}
