//! 思考轮次时间线（ThinkTracesTab / Agent 轨迹面板共用）
//!
//! 逐轮展示：轮次编号 + 裁决徽章 + token/耗时 + 推理摘要 + 模型输出 + 工具调用。
//! 长文本先给预览，超出部分用原生 `<details>` 展开全文。

use dioxus::prelude::*;

use common::api::ThinkTraceRoundDto;

use crate::components::thinking_intensity_field::thinking_intensity_label;

/// 思考文本预览长度（字符）：超出部分折叠
const TEXT_PREVIEW_CHARS: usize = 160;

/// 推理摘要首条预览长度（字符）：多轮思考时避免卡片过长
const REASONING_PREVIEW_CHARS: usize = 120;

/// 按字符截断预览（避免 UTF-8 边界），超长追加省略号
fn preview_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    format!("{}…", text.chars().take(max_chars).collect::<String>())
}

/// 是否需要折叠（内容超出预览长度）
fn needs_expand(text: &str, max_chars: usize) -> bool {
    text.chars().count() > max_chars
}

/// 裁决结果 → 中文文案
fn decision_label(decision: &str) -> &'static str {
    match decision {
        "final" => "最终回复",
        "continue" => "继续",
        "reflect" => "自省",
        "exit" => "退出",
        _ => "其他",
    }
}

/// 裁决结果 → DaisyUI badge 配色
fn decision_class(decision: &str) -> &'static str {
    match decision {
        "final" => "badge badge-sm badge-success badge-soft",
        "reflect" => "badge badge-sm badge-warning badge-soft",
        "exit" => "badge badge-sm badge-error badge-soft",
        _ => "badge badge-sm badge-ghost",
    }
}

/// 工具执行状态 → 中文文案
fn tool_status_label(status: &str) -> &'static str {
    match status {
        "ok" => "成功",
        "error" => "失败",
        "blocked" => "已拦截",
        "not_found" => "未找到",
        "pending" => "待执行",
        _ => "其他",
    }
}

/// 工具执行状态 → DaisyUI badge 配色
fn tool_status_class(status: &str) -> &'static str {
    match status {
        "ok" => "badge badge-xs badge-success badge-soft",
        "error" | "not_found" => "badge badge-xs badge-error badge-soft",
        "blocked" => "badge badge-xs badge-warning badge-soft",
        _ => "badge badge-xs badge-ghost",
    }
}

/// 思考轮次时间线
///
/// 强度按轮记录（同一 run 内目前各轮相同，未来可逐轮动态调整）。
#[component]
pub fn ThinkRoundsTimeline(rounds: Vec<ThinkTraceRoundDto>) -> Element {
    if rounds.is_empty() {
        return rsx! {};
    }
    rsx! {
        div {
            label { class: "form-label", "轮次时间线（{rounds.len()} 轮）" }
            div { class: "space-y-2",
                for r in rounds.iter() {
                    {
                        let round_no = r.round;
                        let decision = r.decision.clone();
                        let assistant = r.assistant_text.clone();
                        let assistant_preview = assistant
                            .as_deref()
                            .map(|t| preview_text(t, TEXT_PREVIEW_CHARS));
                        let assistant_expand = assistant
                            .as_deref()
                            .is_some_and(|t| needs_expand(t, TEXT_PREVIEW_CHARS));
                        let reasoning = r.reasoning.clone();
                        let tools = r.tool_calls.clone();
                        let usage = format!(
                            "in {} / out {} · {}ms",
                            r.input_tokens,
                            r.output_tokens,
                            r.duration_ms,
                        );
                        let intensity_text = r
                            .thinking_intensity
                            .map(thinking_intensity_label)
                            .unwrap_or_else(|| "思考·未启用".to_string());
                        rsx! {
                            div {
                                key: "{round_no}",
                                class: "rounded-md border border-base-300 bg-base-200/40 p-2 space-y-1.5",
                                div { class: "flex items-center gap-2 flex-wrap",
                                    span { class: "text-xs font-semibold", "第 {round_no} 轮" }
                                    if let Some(decision) = decision.clone() {
                                        span { class: decision_class(&decision), "{decision_label(&decision)}" }
                                    }
                                    span {
                                        class: "badge badge-xs badge-ghost font-mono",
                                        title: "本轮实际下发的思考强度（0–100 分；未启用时请求体不下发）",
                                        "{intensity_text}"
                                    }
                                    span { class: "text-[11px] text-base-content/50 flex-1 text-right", "{usage}" }
                                }
                                // 推理摘要：先给预览，超长折叠
                                if !reasoning.is_empty() {
                                    div { class: "space-y-1",
                                        div { class: "text-[11px] text-base-content/50", "推理摘要" }
                                        for item in reasoning.iter() {
                                            {
                                                let preview = preview_text(item, REASONING_PREVIEW_CHARS);
                                                let expand = needs_expand(item, REASONING_PREVIEW_CHARS);
                                                rsx! {
                                                    div { class: "rounded border border-base-300/60 bg-base-100 p-1.5",
                                                        div { class: "text-xs text-base-content/70 whitespace-pre-wrap break-all", "{preview}" }
                                                        if expand {
                                                            details {
                                                                summary { class: "cursor-pointer text-[11px] text-primary mt-1", "展开全文" }
                                                                pre { class: "mt-1 whitespace-pre-wrap break-all text-[11px] text-base-content/70", "{item}" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                // 本轮模型可见文本（工具轮的思考说明）
                                if let Some(preview) = assistant_preview.clone() {
                                    div { class: "space-y-0.5",
                                        div { class: "text-[11px] text-base-content/50", "模型输出" }
                                        div { class: "text-xs text-base-content/70 whitespace-pre-wrap break-all", "{preview}" }
                                        if assistant_expand {
                                            if let Some(text) = assistant.clone() {
                                                details {
                                                    summary { class: "cursor-pointer text-[11px] text-primary", "展开全文" }
                                                    pre { class: "mt-1 whitespace-pre-wrap break-all text-[11px] text-base-content/70", "{text}" }
                                                }
                                            }
                                        }
                                    }
                                }
                                // 本轮工具调用：状态 + 入参 / 结果预览（call_id 可回查完整 trace）
                                for (tool_idx, tool) in tools.iter().enumerate() {
                                    {
                                        let name = tool.name.clone();
                                        let status = tool.status.clone();
                                        let arguments = tool.arguments.clone();
                                        let result = tool.result.clone();
                                        let result_preview = result
                                            .as_deref()
                                            .map(|r| preview_text(r, TEXT_PREVIEW_CHARS));
                                        let result_expand = result
                                            .as_deref()
                                            .is_some_and(|r| needs_expand(r, TEXT_PREVIEW_CHARS));
                                        let call_id = tool.call_id.clone();
                                        rsx! {
                                            div {
                                                key: "{name}-{tool_idx}",
                                                class: "rounded border border-base-300/70 bg-base-100 p-1.5 space-y-1",
                                                div { class: "flex items-center gap-2 flex-wrap",
                                                    span { class: "text-[11px] font-mono font-semibold", "{name}" }
                                                    span { class: tool_status_class(&status), "{tool_status_label(&status)}" }
                                                    if let Some(id) = call_id.clone() {
                                                        span {
                                                            class: "text-[10px] text-base-content/40 font-mono truncate flex-1 text-right",
                                                            title: "{id}",
                                                            "{preview_text(&id, 24)}"
                                                        }
                                                    }
                                                }
                                                details {
                                                    summary { class: "cursor-pointer text-[11px] text-base-content/50", "入参" }
                                                    pre { class: "mt-0.5 whitespace-pre-wrap break-all text-[11px] text-base-content/60", "{arguments}" }
                                                }
                                                if let Some(preview) = result_preview.clone() {
                                                    div { class: "space-y-0.5",
                                                        div { class: "text-[11px] text-base-content/50", "结果" }
                                                        div { class: "text-[11px] text-base-content/60 whitespace-pre-wrap break-all", "{preview}" }
                                                        if result_expand {
                                                            if let Some(full) = result.clone() {
                                                                details {
                                                                    summary { class: "cursor-pointer text-[11px] text-primary", "展开全文" }
                                                                    pre { class: "mt-1 whitespace-pre-wrap break-all text-[11px] text-base-content/60", "{full}" }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
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
    fn preview_text_truncates_by_chars() {
        assert_eq!(preview_text("短文本", 10), "短文本");
        assert_eq!(preview_text("一二三四五", 3), "一二三…");
        assert!(!needs_expand("一二三", 3));
        assert!(needs_expand("一二三四", 3));
    }

    #[test]
    fn decision_and_tool_status_labels_cover_known_values() {
        assert_eq!(decision_label("final"), "最终回复");
        assert_eq!(decision_label("continue"), "继续");
        assert_eq!(decision_label("reflect"), "自省");
        assert_eq!(decision_label("exit"), "退出");
        assert_eq!(decision_label("unknown"), "其他");

        assert_eq!(tool_status_label("ok"), "成功");
        assert_eq!(tool_status_label("error"), "失败");
        assert_eq!(tool_status_label("blocked"), "已拦截");
        assert_eq!(tool_status_label("not_found"), "未找到");
        assert_eq!(tool_status_label("pending"), "待执行");
    }
}
