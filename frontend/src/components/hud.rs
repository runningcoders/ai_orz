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

/// 带色调变体的 HUD 卡片：用于「关联全景」等卡片网格场景（如 Agent 已装工具 / 技能）。
/// 与 `HudPanel` 同源（`.hud-panel` 渐变发丝边），通过 `tone` 切换渐变主色
/// （primary / accent / success / neutral），内部以 `card-body` 承载内容。
///
/// - `tone`：卡片色调，决定渐变主色；默认 primary。
/// - `extra_class`：附加到根 `.hud-panel` 的额外类。
#[component]
pub fn HudCard(
    tone: Option<&'static str>,
    extra_class: Option<String>,
    children: Element,
) -> Element {
    let tone_cls = tone
        .map(|t| format!("hud-tone-{}", t))
        .unwrap_or_else(|| "hud-tone-primary".to_string());
    let root = format!("hud-panel {} {}", tone_cls, extra_class.unwrap_or_default());
    rsx! {
        div { class: "{root}",
            div { class: "card-body p-4",
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

/// HUD 风格进度条：薄轨道 + 发光填充，替代全站三套旧实现
/// （DaisyUI `progress` / 自定义 `overview-progress` / 自定义 `progress-cell`）。
///
/// - `value`：进度 0-100（自动 clamp）。
/// - `tone`：warning / primary / accent / success / error / info，决定填充色与发光；默认 primary。
/// - `show_value`：是否在轨道下方显示 `NN%` 等宽读数；默认 false。
/// - `extra_class`：附加到根 `.hud-progress` 的额外类（如 `mt-1`）。
#[component]
pub fn HudProgress(
    value: i32,
    tone: Option<String>,
    show_value: Option<bool>,
    extra_class: Option<String>,
) -> Element {
    let v = value.clamp(0, 100);
    let tone_cls = tone.unwrap_or_else(|| "primary".to_string());
    let fill = format!("hud-progress-fill {}", tone_cls);
    let show = show_value.unwrap_or(false);
    let root = format!("hud-progress {}", extra_class.unwrap_or_default());
    rsx! {
        div { class: "{root}",
            div { class: "hud-progress-track",
                div { class: "{fill}", style: "width: {v}%;" }
            }
            if show {
                span { class: "hud-progress-text", "{v}%" }
            }
        }
    }
}

/// HUD 风格提示条：替代散落的裸 `alert alert-info/warning/error`。
/// `tone`：info（默认）/ warning / error / success，决定边框、底色、文字色。
#[component]
pub fn HudCallout(tone: Option<String>, extra_class: Option<String>, children: Element) -> Element {
    let tone_class = match tone.as_deref() {
        Some("error") => "text-error",
        Some("success") => "text-success",
        Some("warning") => "text-warning",
        _ => "text-info",
    };
    let root = format!(
        "hud-callout px-4 py-3 text-sm {} {}",
        tone_class,
        extra_class.unwrap_or_default()
    );
    rsx! {
        div { class: "{root}", {children} }
    }
}

/// HUD 分割线：两侧发丝线 + 可选等宽 eyebrow 文本。替代裸 `divider`。
#[component]
pub fn HudDivider(text: Option<String>, extra_class: Option<String>) -> Element {
    let root = format!("hud-divider {}", extra_class.unwrap_or_default());
    rsx! {
        div { class: "{root}",
            if let Some(t) = text {
                span { class: "hud-divider-text", "{t}" }
            }
        }
    }
}

/// HUD 表格容器：保留 DaisyUI `table` 结构，附加 `.hud-table` 皮肤（斑马纹 / 悬停发光 / 等宽表头）。
/// 旧代码可直接在现有 `table` 元素上追加 `hud-table` 类，无需改用本组件。
#[component]
pub fn HudTable(extra_class: Option<String>, children: Element) -> Element {
    let root = format!("hud-table {}", extra_class.unwrap_or_default());
    rsx! {
        div { class: "overflow-x-auto",
            table { class: "{root}", {children} }
        }
    }
}

/// HUD 标签页容器：flex 包裹 + gap-2，子按钮统一使用 `btn hud-btn btn-sm btn-primary`（选中）/ `btn hud-btn btn-sm btn-ghost`（未选）。
/// 与详情页（agent_detail / project_detail / task_detail 等）的 Tab 切换器保持一致。
#[component]
pub fn HudTabs(extra_class: Option<String>, children: Element) -> Element {
    let root = format!("flex flex-wrap gap-2 {}", extra_class.unwrap_or_default());
    rsx! {
        div { class: "{root}", {children} }
    }
}
