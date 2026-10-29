//! 思考轮次时间线（ThinkTracesTab / Agent 轨迹面板共用）
//!
//! 逐轮展示：左侧轮次 + 裁决（竖排撑满两行）、中间 token 明细 2×2、
//! 右侧思考强度与耗时上下两行，以及推理摘要、模型输出、工具调用。
//!
//! 长文本先给预览，超出部分用原生 `<details>` 展开全文。

use dioxus::prelude::*;

use common::api::ThinkTraceRoundDto;
use common::enums::ThinkingIntensity;

use crate::components::thinking_intensity_field::thinking_intensity_label;
use crate::utils::number::{format_compact_count, format_decimal};

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

/// 思考强度 → DaisyUI badge 配色（绿 → 黄 → 红，与分数从低到高同向）
fn intensity_class(intensity: Option<ThinkingIntensity>) -> &'static str {
    match intensity {
        None => "badge badge-xs badge-ghost",
        Some(i) if i.score() <= 33 => "badge badge-xs badge-success badge-soft",
        Some(i) if i.score() <= 66 => "badge badge-xs badge-warning badge-soft",
        Some(_) => "badge badge-xs badge-error badge-soft",
    }
}

/// 耗时 → 合适单位（<1s 用 ms，<1min 用 s，<1h 用 m/s，更长用 h/m）
fn format_duration(ms: u64) -> String {
    if ms < 1_000 {
        return format!("{ms}ms");
    }
    let total_seconds = ms / 1_000;
    if total_seconds < 60 {
        return format!("{}s", format_decimal(ms as f64 / 1_000.0, 1));
    }
    let total_minutes = total_seconds / 60;
    if total_minutes < 60 {
        let seconds = total_seconds % 60;
        return if seconds == 0 {
            format!("{total_minutes}m")
        } else {
            format!("{total_minutes}m {seconds}s")
        };
    }
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    if minutes == 0 {
        format!("{hours}h")
    } else {
        format!("{hours}h {minutes}m")
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
                        let input_text = format_compact_count(r.input_tokens);
                        let output_text = format_compact_count(r.output_tokens);
                        let cached_text = format_compact_count(r.cached_tokens);
                        let reasoning_text = format_compact_count(r.reasoning_tokens);
                        let duration_text = format_duration(r.duration_ms);
                        let intensity_text = r
                            .thinking_intensity
                            .map(thinking_intensity_label)
                            .unwrap_or_else(|| "思考·未启用".to_string());
                        rsx! {
                            div {
                                key: "{round_no}",
                                class: "rounded-md border border-base-300 bg-base-200/40 p-2 space-y-1.5",
                                // 顶行：左侧轮次/裁决 → 中间 token 2×2 → 右侧强度/耗时上下两行
                                div { class: "flex flex-wrap items-start gap-2",
                                    // 轮次 + 裁决竖排，撑满两行高度（与右侧 token 网格等高）
                                    div { class: "flex flex-col items-start justify-center gap-1 shrink-0 self-stretch",
                                        span { class: "text-sm font-semibold whitespace-nowrap leading-none", "第 {round_no} 轮" }
                                        if let Some(decision) = decision.clone() {
                                            span { class: decision_class(&decision), "{decision_label(&decision)}" }
                                        }
                                    }
                                    // token 明细 2×2（缓存/思考未上报时为 0），靠右贴近强度/耗时
                                    div { class: "grid grid-cols-2 gap-1 ml-auto",
                                        span {
                                            class: "badge badge-xs badge-info badge-soft font-mono justify-start min-w-[60px]",
                                            title: "本轮输入（prompt）token",
                                            "入 {input_text}"
                                        }
                                        span {
                                            class: "badge badge-xs badge-accent badge-soft font-mono justify-start min-w-[60px]",
                                            title: "本轮输出（completion）token；含思考 token",
                                            "出 {output_text}"
                                        }
                                        span {
                                            class: "badge badge-xs badge-success badge-soft font-mono justify-start min-w-[60px]",
                                            title: "提示缓存命中的输入 token；provider 未上报时为 0，不代表未命中",
                                            "缓存 {cached_text}"
                                        }
                                        span {
                                            class: "badge badge-xs badge-warning badge-soft font-mono justify-start min-w-[60px]",
                                            title: "思考/推理消耗的 token，已包含在输出 token 内",
                                            "思考 {reasoning_text}"
                                        }
                                    }
                                    // 右端两行：上强度标签、下耗时
                                    div { class: "flex flex-col items-end gap-1 shrink-0",
                                        span {
                                            class: intensity_class(r.thinking_intensity),
                                            title: "本轮实际下发的思考强度（0–100 分；未启用时请求体不下发）",
                                            "{intensity_text}"
                                        }
                                        span {
                                            class: "text-[11px] text-base-content/50 font-mono",
                                            title: "本轮模型调用耗时",
                                            "{duration_text}"
                                        }
                                    }
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
    fn duration_scales_to_readable_units() {
        assert_eq!(format_duration(420), "420ms");
        assert_eq!(format_duration(1_000), "1s");
        assert_eq!(format_duration(1_500), "1.5s");
        assert_eq!(format_duration(61_000), "1m 1s");
        assert_eq!(format_duration(600_000), "10m");
        assert_eq!(format_duration(3_600_000), "1h");
        assert_eq!(format_duration(3_720_000), "1h 2m");
    }

    #[test]
    fn intensity_class_covers_tiers() {
        assert!(intensity_class(None).contains("badge-ghost"));
        assert!(intensity_class(Some(ThinkingIntensity::new(10))).contains("badge-success"));
        assert!(intensity_class(Some(ThinkingIntensity::new(50))).contains("badge-warning"));
        assert!(intensity_class(Some(ThinkingIntensity::new(90))).contains("badge-error"));
    }

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
