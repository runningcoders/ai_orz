use common::models::{AgentStats, ModelCallStats};
use dioxus::prelude::*;

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

#[component]
pub fn AgentStatsPanel(stats: Option<AgentStats>, model_call_stats: Option<ModelCallStats>) -> Element {
    rsx! {
        div { class: "card bg-base-100 shadow-md",
            div { class: "card-body",
                h2 { class: "card-title text-lg mb-2", "📊 Agent 统计" }
                div { class: "stats shadow overflow-visible",
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
            }
        }
    }
}

#[component]
pub fn ProjectStatsPanel(stats: Option<common::models::ProjectStats>, model_call_stats: Option<ModelCallStats>) -> Element {
    rsx! {
        div { class: "card bg-base-100 shadow-md",
            div { class: "card-body",
                h2 { class: "card-title text-lg mb-2", "📊 项目统计" }
                div { class: "stats shadow overflow-visible",
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
            }
        }
    }
}

#[component]
pub fn TaskStatsPanel(stats: Option<common::models::TaskStats>, model_call_stats: Option<ModelCallStats>) -> Element {
    rsx! {
        div { class: "card bg-base-100 shadow-md",
            div { class: "card-body",
                h2 { class: "card-title text-lg mb-2", "📊 任务统计" }
                div { class: "stats shadow overflow-visible",
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
            }
        }
    }
}

#[component]
pub fn ToolStatsPanel(stats: Option<common::models::ToolStats>) -> Element {
    rsx! {
        div { class: "card bg-base-100 shadow-md",
            div { class: "card-body",
                h2 { class: "card-title text-lg mb-2", "📊 工具统计" }
                div { class: "stats shadow overflow-visible",
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
    }
}

#[component]
pub fn ModelProviderStatsPanel(stats: Option<ModelCallStats>) -> Element {
    rsx! {
        div { class: "card bg-base-100 shadow-md",
            div { class: "card-body",
                h2 { class: "card-title text-lg mb-2", "📊 模型提供商统计" }
                div { class: "stats shadow overflow-visible",
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
            }
        }
    }
}
