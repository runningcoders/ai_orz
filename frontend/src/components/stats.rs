use common::models::{AgentStats, ModelCallStats};
use dioxus::prelude::*;

use crate::components::charts::donut_chart::{DonutChart, DonutSlice};
use crate::components::charts::line_chart::LineChart;

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
    if let Some(s) = stats {
        if let Some(tool_calls) = &s.tool_call_summary {
            if !tool_calls.by_tool.is_empty() {
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
        }
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

/// 渲染模型调用时序图（如果有数据）
fn render_time_series_chart(model_call_stats: &Option<ModelCallStats>) -> Element {
    if let Some(mcs) = model_call_stats {
        if let Some(series) = &mcs.model_call_time_series {
            if !series.is_empty() {
                return rsx! {
                    div { class: "mt-4",
                        LineChart {
                            data: series.clone(),
                            width: 600.0,
                            height: 200.0,
                            title: Some("模型调用趋势".to_string()),
                            value_label: Some("调用次数".to_string()),
                        }
                    }
                };
            }
        }
    }
    rsx! {}
}

#[component]
pub fn StatsCard(title: String, icon: String, value: String, subtitle: Option<String>) -> Element {
    rsx! {
        div { class: "stat",
            div { class: "stat-figure text-xl", "{icon}" }
            div { class: "stat-title text-sm", "{title}" }
            div { class: "stat-value text-2xl text-primary", "{value}" }
            if let Some(sub) = subtitle {
                div { class: "stat-desc text-xs", "{sub}" }
            }
        }
    }
}

/// 通用统计面板外壳：统一的 card + title + stats 容器，内容由调用方通过 children 传入
#[component]
pub fn StatsPanel(title: String, children: Element) -> Element {
    rsx! {
        div { class: "card bg-base-100 shadow-md",
            div { class: "card-body",
                h2 { class: "card-title text-lg mb-2", "📊 {title}" }
                div { class: "stats shadow overflow-visible",
                    {children}
                }
            }
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
