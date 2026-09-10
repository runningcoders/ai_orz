use common::models::{AgentStats, ModelCallStats};
use dioxus::prelude::*;

use crate::components::charts::donut_chart::{DonutChart, DonutSlice};
use crate::components::charts::line_chart::{LineChart, LineChartValueField};
use crate::components::hud::{HudPanel, StatGrid, StatReadout};
use crate::components::ring_progress::RingProgress;

/// 工具调用分布环形图调色板（循环使用，避免单一色调）
const TOOL_PALETTE: &[&str] = &[
    "#fa520f", // 橙
    "#10b981", // 绿
    "#3b82f6", // 蓝
    "#f59e0b", // 黄
    "#a855f7", // 紫
    "#ef4444", // 红
    "#06b6d4", // 青
    "#ec4899", // 粉
];

/// 渲染工具调用分布环形图（如果有数据）
fn render_tool_call_distribution(stats: &Option<AgentStats>) -> Element {
    if let Some(s) = stats
        && let Some(tool_calls) = &s.tool_call_summary
        && !tool_calls.by_tool.is_empty()
    {
        let slices: Vec<DonutSlice> = tool_calls
            .by_tool
            .iter()
            .enumerate()
            .map(|(i, c)| DonutSlice {
                label: if c.tool_name.is_empty() {
                    c.tool_id.clone()
                } else {
                    c.tool_name.clone()
                },
                value: c.count,
                color: TOOL_PALETTE[i % TOOL_PALETTE.len()].to_string(),
            })
            .collect();
        return rsx! {
            div { class: "mt-4",
                h3 { class: "text-sm font-semibold mb-2", "🛠️ 工具调用分布" }
                DonutChart {
                    data: slices,
                    width: Some(300.0),
                    height: Some(220.0),
                    center_label: Some("工具调用".to_string()),
                }
            }
        };
    }
    rsx! {}
}

fn format_token_count(count: u64) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}K", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}

fn format_qps(qps: f64) -> String {
    format!("{:.2}", qps)
}

/// 用户维度模型调用统计面板（无外壳版）
///
/// 供个人信息页等已有 HudPanel 外壳的页面内嵌使用：
/// 卡片（模型调用 / 输入 / 输出 Token）+ 三线趋势图（输入/输出 Token 共左轴 + 调用次数右轴）。
/// 数据口径为「打点命中」：仅统计模型调用事件 `user_id` 与当前用户一致的记录。
#[component]
pub fn UserStatsPanel(model_call_stats: Option<ModelCallStats>) -> Element {
    let has_data = model_call_stats
        .as_ref()
        .is_some_and(|m| m.call_summary.is_some() || m.token_summary.is_some());
    rsx! {
        div { class: "mt-6",
            h3 { class: "text-sm font-semibold mb-2", "📊 模型调用统计" }
            if has_data {
                div { class: "space-y-4",
                    StatGrid {
                        if let Some(mcs) = &model_call_stats {
                            if let Some(call) = &mcs.call_summary {
                                StatsCard { title: "模型调用".to_string(), icon: "🤖".to_string(), value: call.total_calls.to_string(), subtitle: None }
                            }
                            if let Some(token) = &mcs.token_summary {
                                StatsCard { title: "输入 Token".to_string(), icon: "📥".to_string(), value: format_token_count(token.total_tokens_input), subtitle: None }
                                StatsCard { title: "输出 Token".to_string(), icon: "📤".to_string(), value: format_token_count(token.total_tokens_output), subtitle: None }
                            }
                        }
                    }
                    {render_time_series_chart(&model_call_stats)}
                }
            } else {
                p { class: "text-sm text-base-content/50",
                    "暂无模型调用记录（仅统计由你触发的 Agent 调用）"
                }
            }
        }
    }
}

/// 渲染模型调用时序图（如果有数据），默认 600x200（详情页等宽容器）
fn render_time_series_chart(model_call_stats: &Option<ModelCallStats>) -> Element {
    render_time_series_chart_sized(model_call_stats, 600.0, 200.0)
}

/// 渲染模型调用时序图（指定原生渲染尺寸；窄容器传小尺寸保证文字按原始比例清晰）
fn render_time_series_chart_sized(
    model_call_stats: &Option<ModelCallStats>,
    width: f64,
    height: f64,
) -> Element {
    if let Some(mcs) = model_call_stats
        && let Some(series) = &mcs.model_call_time_series
        && !series.is_empty()
    {
        return rsx! {
            div { class: "mt-4",
                LineChart {
                    data: series.clone(),
                    width,
                    height,
                    title: Some("模型调用趋势".to_string()),
                    value_label: Some("输入 Token".to_string()),
                    value_field: Some(LineChartValueField::TokensInput),
                    primary_second_field: Some(LineChartValueField::TokensOutput),
                    primary_second_label: Some("输出 Token".to_string()),
                    secondary_field: Some(LineChartValueField::CallCount),
                    secondary_label: Some("调用次数".to_string()),
                }
            }
        };
    }
    rsx! {}
}

#[component]
pub fn StatsCard(title: String, icon: String, value: String, subtitle: Option<String>) -> Element {
    rsx! {
        StatReadout {
            label: title,
            value: value,
            icon: if icon.is_empty() { None } else { Some(icon) },
            delta: subtitle,
        }
    }
}

/// 通用统计面板外壳：统一的 HUD 面板 + 数值读数网格，内容由调用方通过 children 传入
#[component]
pub fn StatsPanel(title: String, children: Element) -> Element {
    rsx! {
        HudPanel { eyebrow: "STATS".to_string(), title: title,
            StatGrid { {children} }
        }
    }
}

#[component]
pub fn AgentStatsPanel(
    stats: Option<AgentStats>,
    model_call_stats: Option<ModelCallStats>,
) -> Element {
    let chart_data = model_call_stats.clone();
    let tool_dist_data = stats.clone();
    rsx! {
        div { class: "space-y-4",
            StatsPanel { title: "Agent 统计".to_string(),
                if let Some(s) = stats {
                    if let Some(call) = s.call_summary {
                        StatsCard { title: "唤醒次数".to_string(), icon: "🔔".to_string(), value: call.total_calls.to_string(), subtitle: None }
                        if let Some(qps) = call.avg_qps {
                            StatsCard { title: "平均 QPS".to_string(), icon: "📈".to_string(), value: format_qps(qps), subtitle: None }
                        }
                        StatsCard { title: "瞬时 QPS".to_string(), icon: "⚡".to_string(), value: format_qps(call.instant_qps), subtitle: None }
                    }
                }
                if let Some(mcs) = model_call_stats {
                    if let Some(call) = mcs.call_summary {
                        StatsCard { title: "模型调用".to_string(), icon: "🤖".to_string(), value: call.total_calls.to_string(), subtitle: None }
                    }
                    if let Some(token) = mcs.token_summary {
                        StatsCard { title: "输入 Token".to_string(), icon: "📥".to_string(), value: format_token_count(token.total_tokens_input), subtitle: None }
                        StatsCard { title: "输出 Token".to_string(), icon: "📤".to_string(), value: format_token_count(token.total_tokens_output), subtitle: None }
                    }
                }
            }
            {render_time_series_chart(&chart_data)}
            {render_tool_call_distribution(&tool_dist_data)}
        }
    }
}

/// Agent 运行统计紧凑面板（无外壳版，聊天侧栏等窄容器用）
///
/// 与 `AgentStatsPanel` 同口径（唤醒/QPS + 模型调用 + 输入/输出 Token + 三线趋势），
/// 差异：
/// - 不带 HudPanel 外壳与工具分布环形图；
/// - 读数分**左右两列**：左列「运行」= 唤醒次数 / 瞬时 QPS / 上下文长度，
///   右列「模型调用」= 调用次数 / 输入 Token / 输出 Token。
///   不用 `StatGrid`（其 `lg:grid-cols-4` 在桌面视口下会把 384px 侧栏切成 4 列，
///   数值溢出），这里固定一次性 2 列，且**列内纵向堆叠**、不再嵌套 2×2 网格，
///   避免第三个读数落单留空位；数值统一用紧凑变体。
/// - 趋势图按 320px 原生渲染，避免 600px 图被 CSS 缩放后文字过小。
///
/// `context_length` 是**运行时内存指标**（最近一次 LLM 调用的 prompt token 数，不入库），
/// 用于直观观察上下文思考强度；为 None（从未思考过）时不展示该项。
///
/// 有 `context_length_threshold`（压缩阈值，与后端 `ContextOverflowPolicy` 同源）时
/// 渲染成环形进度（百分比 = 当前 / 阈值，中心只显示百分比，原始数值走 hover）；
/// 未配置阈值时退化为普通读数——此时没有分母，百分比无从谈起。
#[component]
pub fn AgentStatsPanelCompact(
    stats: Option<AgentStats>,
    model_call_stats: Option<ModelCallStats>,
    context_length: Option<u64>,
    context_length_threshold: Option<u64>,
) -> Element {
    let has_runtime = stats.as_ref().is_some_and(|s| s.call_summary.is_some());
    let has_model = model_call_stats
        .as_ref()
        .is_some_and(|m| m.call_summary.is_some() || m.token_summary.is_some());
    // 上下文长度无持久化统计也可能有值（刚思考过但统计批次未刷盘），单独计入有数据判定
    let has_data = has_runtime || has_model || context_length.is_some();
    let chart_data = model_call_stats.clone();
    let runtime_summary = stats.as_ref().and_then(|s| s.call_summary.as_ref());
    let model_summary = model_call_stats
        .as_ref()
        .and_then(|m| m.call_summary.as_ref());
    let token_summary = model_call_stats
        .as_ref()
        .and_then(|m| m.token_summary.as_ref());
    rsx! {
        div { class: "mt-4 pt-3 border-t border-base-300",
            h3 { class: "text-sm font-semibold mb-2", "📊 运行统计" }
            if has_data {
                div { class: "space-y-3",
                    div { class: "grid grid-cols-2 gap-x-3",
                        if has_runtime || context_length.is_some() {
                            div { class: "min-w-0 space-y-2",
                                label { class: "form-label", "运行" }
                                if let Some(call) = runtime_summary {
                                    CompactStat { label: "唤醒次数".to_string(), icon: "🔔".to_string(), value: call.total_calls.to_string() }
                                    CompactStat { label: "瞬时 QPS".to_string(), icon: "⚡".to_string(), value: format_qps(call.instant_qps) }
                                }
                                if let (Some(len), Some(threshold)) = (context_length, context_length_threshold)
                                {
                                    div { class: "w-fit pt-1",
                                        RingProgress {
                                            value: len,
                                            max: threshold,
                                            caption: Some("上下文".to_string()),
                                            title: Some(format!(
                                                "上下文 {} / 压缩阈值 {} tokens",
                                                format_token_count(len),
                                                format_token_count(threshold),
                                            )),
                                            size: None,
                                        }
                                    }
                                } else if let Some(len) = context_length {
                                    CompactStat { label: "上下文长度".to_string(), icon: "🧠".to_string(), value: format_token_count(len) }
                                }
                            }
                        }
                        if has_model {
                            div { class: "min-w-0 space-y-2",
                                label { class: "form-label", "模型调用" }
                                if let Some(call) = model_summary {
                                    CompactStat { label: "调用次数".to_string(), icon: "🤖".to_string(), value: call.total_calls.to_string() }
                                }
                                if let Some(token) = token_summary {
                                    CompactStat { label: "输入 Token".to_string(), icon: "📥".to_string(), value: format_token_count(token.total_tokens_input) }
                                    CompactStat { label: "输出 Token".to_string(), icon: "📤".to_string(), value: format_token_count(token.total_tokens_output) }
                                }
                            }
                        }
                    }
                    {render_time_series_chart_sized(&chart_data, 320.0, 180.0)}
                }
            } else {
                p { class: "text-xs text-base-content/50", "暂无运行统计数据" }
            }
        }
    }
}

/// 窄容器用紧凑读数：eyebrow 标签 + 1.25rem 等宽数值（复用 StatReadout 紧凑变体）
#[component]
fn CompactStat(label: String, icon: String, value: String) -> Element {
    rsx! {
        StatReadout {
            label,
            value,
            unit: None,
            icon: Some(icon),
            delta: None,
            accent: None,
            compact: Some(true),
        }
    }
}

#[component]
pub fn ProjectStatsPanel(
    stats: Option<common::models::ProjectStats>,
    model_call_stats: Option<ModelCallStats>,
) -> Element {
    let chart_data = model_call_stats.clone();
    rsx! {
        div { class: "space-y-4",
            StatsPanel { title: "项目统计".to_string(),
                if let Some(s) = stats {
                    if let Some(call) = s.call_summary {
                        StatsCard { title: "事件次数".to_string(), icon: "📝".to_string(), value: call.total_calls.to_string(), subtitle: None }
                        if let Some(qps) = call.avg_qps {
                            StatsCard { title: "平均 QPS".to_string(), icon: "📈".to_string(), value: format_qps(qps), subtitle: None }
                        }
                    }
                }
                if let Some(mcs) = model_call_stats {
                    if let Some(call) = mcs.call_summary {
                        StatsCard { title: "模型调用".to_string(), icon: "🤖".to_string(), value: call.total_calls.to_string(), subtitle: None }
                    }
                    if let Some(token) = mcs.token_summary {
                        StatsCard { title: "输入 Token".to_string(), icon: "📥".to_string(), value: format_token_count(token.total_tokens_input), subtitle: None }
                        StatsCard { title: "输出 Token".to_string(), icon: "📤".to_string(), value: format_token_count(token.total_tokens_output), subtitle: None }
                    }
                }
            }
            {render_time_series_chart(&chart_data)}
        }
    }
}

#[component]
pub fn TaskStatsPanel(
    stats: Option<common::models::TaskStats>,
    model_call_stats: Option<ModelCallStats>,
) -> Element {
    let chart_data = model_call_stats.clone();
    rsx! {
        div { class: "space-y-4",
            StatsPanel { title: "任务统计".to_string(),
                if let Some(s) = stats {
                    if let Some(call) = s.call_summary {
                        StatsCard { title: "事件次数".to_string(), icon: "📝".to_string(), value: call.total_calls.to_string(), subtitle: None }
                        if let Some(qps) = call.avg_qps {
                            StatsCard { title: "平均 QPS".to_string(), icon: "📈".to_string(), value: format_qps(qps), subtitle: None }
                        }
                    }
                }
                if let Some(mcs) = model_call_stats {
                    if let Some(call) = mcs.call_summary {
                        StatsCard { title: "模型调用".to_string(), icon: "🤖".to_string(), value: call.total_calls.to_string(), subtitle: None }
                    }
                    if let Some(token) = mcs.token_summary {
                        StatsCard { title: "输入 Token".to_string(), icon: "📥".to_string(), value: format_token_count(token.total_tokens_input), subtitle: None }
                        StatsCard { title: "输出 Token".to_string(), icon: "📤".to_string(), value: format_token_count(token.total_tokens_output), subtitle: None }
                    }
                }
            }
            {render_time_series_chart(&chart_data)}
        }
    }
}

#[component]
pub fn ToolStatsPanel(stats: Option<common::models::ToolStats>) -> Element {
    rsx! {
        StatsPanel { title: "工具统计".to_string(),
            if let Some(s) = stats {
                if let Some(call) = s.call_summary {
                    StatsCard { title: "调用次数".to_string(), icon: "🛠️".to_string(), value: call.total_calls.to_string(), subtitle: None }
                    if let Some(qps) = call.avg_qps {
                        StatsCard { title: "平均 QPS".to_string(), icon: "📈".to_string(), value: format_qps(qps), subtitle: None }
                    }
                    StatsCard { title: "瞬时 QPS".to_string(), icon: "⚡".to_string(), value: format_qps(call.instant_qps), subtitle: None }
                }
                if let Some(failed) = s.failed_count {
                    StatsCard { title: "失败次数".to_string(), icon: "❌".to_string(), value: failed.to_string(), subtitle: None }
                }
            }
        }
    }
}

#[component]
pub fn ModelProviderStatsPanel(stats: Option<ModelCallStats>) -> Element {
    let chart_data = stats.clone();
    rsx! {
        div { class: "space-y-4",
            StatsPanel { title: "模型提供商统计".to_string(),
                if let Some(s) = stats {
                    if let Some(call) = s.call_summary {
                        StatsCard { title: "模型调用".to_string(), icon: "🤖".to_string(), value: call.total_calls.to_string(), subtitle: None }
                        if let Some(qps) = call.avg_qps {
                            StatsCard { title: "平均 QPS".to_string(), icon: "📈".to_string(), value: format_qps(qps), subtitle: None }
                        }
                        StatsCard { title: "瞬时 QPS".to_string(), icon: "⚡".to_string(), value: format_qps(call.instant_qps), subtitle: None }
                    }
                    if let Some(token) = s.token_summary {
                        StatsCard { title: "输入 Token".to_string(), icon: "📥".to_string(), value: format_token_count(token.total_tokens_input), subtitle: None }
                        StatsCard { title: "输出 Token".to_string(), icon: "📤".to_string(), value: format_token_count(token.total_tokens_output), subtitle: None }
                    }
                }
            }
            {render_time_series_chart(&chart_data)}
        }
    }
}
