//! 记忆轨迹详情（Agent 详情页记忆段 / 对话侧弹窗共用）
//!
//! 数据源：`GET /api/v1/hr/agents/memory-traces/{trace_id}?agent_id=`
//!
//! 展示一次思考闭环的完整 input / output / metadata。长文本先给预览，超出部分
//! 通过原生 `<details>` 展开全文——与思考轮次时间线的折叠口径一致。
//!
//! ⚠️ 本组件按挂载时 props 拉取一次数据；调用方切换 `trace_id` 时必须加
//! `key: "{trace_id}"` 强制重挂载。

use dioxus::prelude::*;

use crate::api::hr::get_memory_trace;
use crate::components::state::Loading;
use crate::utils::format_timestamp_opt;
use common::api::MemoryTraceDetail as MemoryTraceDetailDto;

/// 预览长度（字符）：超出部分折叠
const PREVIEW_CHARS: usize = 400;

fn preview_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    format!("{}…", text.chars().take(max_chars).collect::<String>())
}

fn needs_expand(text: &str, max_chars: usize) -> bool {
    text.chars().count() > max_chars
}

/// 角色 → 中文文案
fn role_label(role: &str) -> String {
    match role {
        "system" => "系统".to_string(),
        "user" => "用户".to_string(),
        "assistant" => "助手".to_string(),
        "summary" => "归纳".to_string(),
        _ => role.to_string(),
    }
}

/// 角色 → DaisyUI badge 配色
fn role_class(role: &str) -> &'static str {
    match role {
        "system" => "badge badge-sm badge-neutral badge-soft",
        "user" => "badge badge-sm badge-info badge-soft",
        "assistant" => "badge badge-sm badge-success badge-soft",
        "summary" => "badge badge-sm badge-warning badge-soft",
        _ => "badge badge-sm badge-ghost",
    }
}

/// 记忆轨迹详情
#[component]
pub fn MemoryTraceDetail(agent_id: String, trace_id: String) -> Element {
    let mut detail = use_signal(|| None::<MemoryTraceDetailDto>);
    let mut error = use_signal(|| None::<String>);
    let mut loading = use_signal(|| true);

    // use_effect 会按值捕获 props，先留出副本供 rsx 渲染使用
    let effect_agent_id = agent_id.clone();
    let effect_trace_id = trace_id.clone();

    use_effect(move || {
        let agent_id = effect_agent_id.clone();
        let trace_id = effect_trace_id.clone();
        loading.set(true);
        error.set(None);
        spawn(async move {
            match get_memory_trace(&agent_id, &trace_id).await {
                Ok(dto) => detail.set(Some(dto)),
                Err(e) => error.set(Some(format!("加载记忆轨迹失败: {}", e))),
            }
            loading.set(false);
        });
    });

    rsx! {
        div { class: "rounded-lg border border-base-300 bg-base-100 p-2 space-y-2",
            div { class: "flex items-center gap-2 flex-wrap",
                span { class: "text-xs font-semibold", "记忆轨迹" }
                if let Some(d) = detail() {
                    span { class: role_class(&d.role), "{role_label(&d.role)}" }
                    span { class: "text-[11px] text-base-content/50", "{format_timestamp_opt(Some(d.created_at * 1000))}" }
                    if let Some(completed) = d.completed_at {
                        span {
                            class: "text-[11px] text-base-content/40",
                            "耗时 {(completed - d.created_at).max(0)}s"
                        }
                    }
                }
                span { class: "text-[10px] text-base-content/30 font-mono truncate flex-1 text-right",
                    title: "{trace_id}",
                    "{trace_id}"
                }
            }

            if loading() {
                div { class: "flex items-center justify-center py-6",
                    Loading { size: "sm" }
                    span { class: "ml-2 text-xs text-base-content/60", "加载中..." }
                }
            } else if let Some(err) = error() {
                div { class: "text-xs text-error", "{err}" }
            } else if let Some(d) = detail() {
                // metadata
                if !d.metadata.is_empty() {
                    div { class: "flex flex-wrap gap-1",
                        for (key, value) in d.metadata.iter() {
                            span {
                                key: "{key}",
                                class: "badge badge-xs badge-ghost font-mono",
                                title: "{value}",
                                "{key}: {value}"
                            }
                        }
                    }
                }
                div { class: "text-[11px] text-base-content/40 font-mono break-all",
                    "log: {d.log_id}"
                    if let Some(task_id) = d.task_id.clone() {
                        " · task: {task_id}"
                    }
                }

                // 输入
                div { class: "space-y-0.5",
                    div { class: "text-[11px] text-base-content/50", "思考输入（{d.input.chars().count()} 字符）" }
                    pre { class: "rounded border border-base-300/60 bg-base-200/40 p-1.5 whitespace-pre-wrap break-all text-xs text-base-content/70 max-h-56 overflow-auto",
                        "{preview_text(&d.input, PREVIEW_CHARS)}"
                    }
                    if needs_expand(&d.input, PREVIEW_CHARS) {
                        details {
                            summary { class: "cursor-pointer text-[11px] text-primary", "展开完整输入" }
                            pre { class: "mt-1 whitespace-pre-wrap break-all text-[11px] text-base-content/60", "{d.input}" }
                        }
                    }
                }

                // 输出
                div { class: "space-y-0.5",
                    div { class: "text-[11px] text-base-content/50", "思考输出" }
                    if let Some(output) = d.output.clone() {
                        pre { class: "rounded border border-base-300/60 bg-base-200/40 p-1.5 whitespace-pre-wrap break-all text-xs text-base-content/70 max-h-56 overflow-auto",
                            "{preview_text(&output, PREVIEW_CHARS)}"
                        }
                        if needs_expand(&output, PREVIEW_CHARS) {
                            details {
                                summary { class: "cursor-pointer text-[11px] text-primary", "展开完整输出" }
                                pre { class: "mt-1 whitespace-pre-wrap break-all text-[11px] text-base-content/60", "{output}" }
                            }
                        }
                    } else {
                        div { class: "text-xs text-base-content/40", "无输出（思考中断或尚未完成）" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_truncates_by_chars() {
        assert_eq!(preview_text("短文本", 10), "短文本");
        assert_eq!(preview_text("一二三四五", 3), "一二三…");
        assert!(!needs_expand("一二三", 3));
        assert!(needs_expand("一二三四", 3));
    }

    #[test]
    fn role_labels_cover_known_values() {
        assert_eq!(role_label("system"), "系统");
        assert_eq!(role_label("user"), "用户");
        assert_eq!(role_label("assistant"), "助手");
        assert_eq!(role_label("summary"), "归纳");
        assert_eq!(role_label("other"), "other");
    }
}
