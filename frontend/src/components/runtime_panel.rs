//! Agent 运行时面板组件
//!
//! 接收 agent_id，自动轮询 runtime-status 接口，展示：
//! - 运行时状态（idle/busy/resting）+ 上下文（message/task/project）
//! - 思考运行时快照（轮次进度、token 统计、工具调用、trace_id）
//! - 取消思考按钮（Busy 时显示，ConfirmDialog 二次确认）
//!
//! 轮询策略：Busy 时 3 秒，Idle/Resting 时 30 秒降频。

use crate::api::hr::{cancel_thinking, get_runtime_status};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::hud::{HudPanel, HudProgress};
use crate::store::toast::use_toast;
use common::api::{CancelThinkingRequest, RuntimeStatusRequest, RuntimeStatusResponse};
use dioxus::prelude::*;

const POLL_INTERVAL_BUSY_MS: u32 = 3000;
const POLL_INTERVAL_IDLE_MS: u32 = 30000;

/// RuntimePanel Props
#[derive(Props, Clone, PartialEq)]
pub struct RuntimePanelProps {
    /// Agent ID
    pub agent_id: String,
}

/// Agent 运行时面板组件
#[component]
pub fn RuntimePanel(props: RuntimePanelProps) -> Element {
    let toast = use_toast();
    let mut status = use_signal(|| Option::<RuntimeStatusResponse>::None);
    let mut loading = use_signal(|| true);
    let mut show_cancel_confirm = use_signal(|| false);
    let mut cancelling = use_signal(|| false);
    let agent_id = props.agent_id.clone();

    // 轮询：use_future 自动管理生命周期，组件卸载自动取消
    use_future(move || {
        let aid = agent_id.clone();
        async move {
            loop {
                match get_runtime_status(RuntimeStatusRequest { id: aid.clone() }).await {
                    Ok(resp) => {
                        status.set(Some(resp));
                        loading.set(false);
                    }
                    Err(e) => {
                        toast.error(format!("加载运行时状态失败: {}", e));
                        loading.set(false);
                    }
                }
                // 根据状态选择轮询间隔：Busy 时高频，其他降频
                let is_busy = status()
                    .as_ref()
                    .map(|s| s.state == "busy")
                    .unwrap_or(false);
                let interval = if is_busy {
                    POLL_INTERVAL_BUSY_MS
                } else {
                    POLL_INTERVAL_IDLE_MS
                };
                gloo_timers::future::TimeoutFuture::new(interval).await;
            }
        }
    });

    let current = status();
    let agent_id_for_cancel = props.agent_id.clone();

    // 取消思考
    let on_cancel_confirm = move |_| {
        show_cancel_confirm.set(false);
        let aid = agent_id_for_cancel.clone();
        cancelling.set(true);
        spawn(async move {
            match cancel_thinking(CancelThinkingRequest { id: aid.clone() }).await {
                Ok(resp) => {
                    if resp.success {
                        toast.success(&resp.message);
                    } else {
                        toast.info(&resp.message);
                    }
                }
                Err(e) => {
                    toast.error(format!("取消失败: {}", e));
                }
            }
            cancelling.set(false);
        });
    };

    rsx! {
        div { class: "space-y-4",
            // 加载态
            if *loading.read() && current.is_none() {
                div { class: "text-center py-8 text-base-content/50",
                    "加载运行时状态..."
                }
            }

            // 运行时面板
            if let Some(s) = &current {
                // 状态行 + 取消按钮
                div { class: "flex items-center justify-between",
                    div { class: "flex items-center gap-2",
                        span { class: "text-lg font-semibold",
                            "🧠 Agent 运行时"
                        }
                        span { class: "badge {runtime_state_badge(&s.state)}",
                            "{runtime_state_label(&s.state)}"
                        }
                    }
                    if s.state == "busy" {
                        button {
                            class: "btn hud-btn btn-sm btn-warning",
                            disabled: *cancelling.read(),
                            onclick: move |_| show_cancel_confirm.set(true),
                            if *cancelling.read() {
                                "取消中..."
                            } else {
                                "⚡ 取消思考"
                            }
                        }
                    }
                }

                // 上下文信息
                div { class: "grid grid-cols-1 md:grid-cols-3 gap-2 text-sm",
                    ContextItem { label: "消息", value: s.current_message_id.clone() }
                    ContextItem { label: "任务", value: s.task_id.clone() }
                    ContextItem { label: "项目", value: s.project_id.clone() }
                }

                // 思考运行时快照（仅 Busy 时有值）
                if let Some(tr) = &s.think_runtime {
                    ThinkRuntimeCard { think: tr.clone() }
                } else if s.state == "busy" {
                    div { class: "text-sm text-base-content/50",
                        "思考运行时信息暂无"
                    }
                } else {
                    div { class: "text-sm text-base-content/50",
                        "Agent 当前空闲，无思考运行时"
                    }
                }
            }

            // 取消确认弹窗
            ConfirmDialog {
                show: *show_cancel_confirm.read(),
                title: "确认取消思考".to_string(),
                message: "取消后 Agent 将在当前轮次完成后退出思考，已消耗的 token 不会回退。确定取消？".to_string(),
                confirm_text: Some("确认取消".to_string()),
                confirm_class: Some("btn-warning".to_string()),
                on_confirm: on_cancel_confirm,
                on_cancel: move |_| show_cancel_confirm.set(false),
            }
        }
    }
}

/// 思考运行时快照卡片
#[component]
fn ThinkRuntimeCard(think: common::api::ThinkRuntimeInfo) -> Element {
    let pct = if think.max_rounds > 0 {
        (think.round as f64 / think.max_rounds as f64 * 100.0) as usize
    } else {
        0
    };
    let scene_label = match think.scene.as_str() {
        "awaken" => "唤醒",
        "settle" => "沉淀",
        "summary" => "总结",
        "intent-analyze" => "意图分析",
        _ => &think.scene,
    };
    let status_badge = match think.status.as_str() {
        "thinking" => "badge hud-badge badge-warning",
        "cancelled" => "badge hud-badge badge-error",
        "finished" => "badge hud-badge badge-success",
        _ => "badge hud-badge badge-ghost",
    };

    rsx! {
        HudPanel { signal: None,
            div { class: "space-y-3",
                // 场景 + 状态
                div { class: "flex items-center justify-between",
                    span { class: "text-sm font-medium",
                        "场景：{scene_label}"
                    }
                    span { class: "badge badge-sm {status_badge}",
                        "{think.status}"
                    }
                }

                // 轮次进度条
                div { class: "space-y-1",
                    div { class: "flex justify-between text-xs",
                        span { "轮次 {think.round} / {think.max_rounds}" }
                        span { "{pct}%" }
                    }
                    HudProgress { value: pct as i32, tone: Some("warning".to_string()), show_value: Some(false) }
                }

                // 指标网格
                div { class: "grid grid-cols-2 md:grid-cols-4 gap-2 text-xs",
                    MetricItem { label: "输入 Token", value: format_token(think.tokens_input) }
                    MetricItem { label: "输出 Token", value: format_token(think.tokens_output) }
                    MetricItem { label: "总 Token", value: format_token(think.total_tokens) }
                    MetricItem { label: "工具调用", value: think.tool_call_count.to_string() }
                }

                // trace_id（日志检索用）
                div { class: "text-xs text-base-content/50 font-mono truncate",
                    "trace: {think.trace_id}"
                }
            }
        }
    }
}

/// 上下文信息项
#[component]
fn ContextItem(label: &'static str, value: Option<String>) -> Element {
    rsx! {
        div { class: "bg-base-200 rounded px-2 py-1",
            span { class: "text-xs text-base-content/50 block", "{label}" }
            span { class: "text-sm font-mono truncate block",
                {value.unwrap_or_else(|| "—".to_string())}
            }
        }
    }
}

/// 指标项
#[component]
fn MetricItem(label: &'static str, value: String) -> Element {
    rsx! {
        div { class: "bg-base-200 rounded px-2 py-1 text-center",
            div { class: "text-base-content/50", "{label}" }
            div { class: "font-mono font-semibold", "{value}" }
        }
    }
}

/// 运行时状态标签
fn runtime_state_label(state: &str) -> &'static str {
    match state {
        "idle" => "空闲",
        "busy" => "思考中",
        "resting" => "休息中",
        _ => "未知",
    }
}

/// 运行时状态 badge class
fn runtime_state_badge(state: &str) -> &'static str {
    match state {
        "idle" => "badge hud-badge badge-success",
        "busy" => "badge hud-badge badge-error",
        "resting" => "badge hud-badge badge-warning",
        _ => "badge hud-badge badge-ghost",
    }
}

/// 格式化 token 数值（千分位）
fn format_token(n: u64) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}
