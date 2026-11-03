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

/// 多轮聚合摘要（列表折叠态用）
///
/// 折叠条目想回答「跑了几轮 / 花了多少 token / 多久」，答案都在已落盘的
/// `rounds` 里，前端叠加即可 —— 不为它新增后端字段，也不为它再要一次请求。
///
/// ⚠️ 刻意**不含**「第 N/M 轮」的分母：运行时实际会跑几轮不可知（可能提前
/// final / exit，也可能因意图修订折返重跑），拿配置预算当分母等于把「预算」
/// 说成「实际」。所以只给已记录轮数，配合「进行中」徽章表达还在增长。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RoundStats {
    /// 已记录的轮数
    pub recorded: usize,
    /// 累计口径下最后一个完成的轮次编号（跨压缩累计；0 = 尚无任何一轮）
    pub last_round: usize,
    /// 累计输入 token
    pub input_tokens: u64,
    /// 累计输出 token（含思考 token）
    pub output_tokens: u64,
    /// 累计提示缓存命中 token
    pub cached_tokens: u64,
    /// 累计思考/推理 token（已包含在输出内）
    pub reasoning_tokens: u64,
    /// 累计模型调用耗时
    pub duration_ms: u64,
    /// 强度：取最后一轮实际下发的（各轮一致时即全程强度）
    pub intensity: Option<ThinkingIntensity>,
    /// 是否出现过逐轮不同的强度（true 时 `intensity` 只代表最新一轮）
    pub intensity_varies: bool,
}

/// 把多轮明细叠加成一条摘要
pub fn summarize_rounds(rounds: &[ThinkTraceRoundDto]) -> RoundStats {
    let mut stats = RoundStats {
        recorded: rounds.len(),
        ..Default::default()
    };
    let mut first: Option<ThinkingIntensity> = None;
    for r in rounds {
        stats.input_tokens = stats.input_tokens.saturating_add(r.input_tokens);
        stats.output_tokens = stats.output_tokens.saturating_add(r.output_tokens);
        stats.cached_tokens = stats.cached_tokens.saturating_add(r.cached_tokens);
        stats.reasoning_tokens = stats.reasoning_tokens.saturating_add(r.reasoning_tokens);
        stats.duration_ms = stats.duration_ms.saturating_add(r.duration_ms);
        // 行序即轮次序，但取 max 更稳：合并片段时不依赖顺序假设
        stats.last_round = stats.last_round.max(r.round);
        if let Some(i) = r.thinking_intensity {
            match first {
                None => first = Some(i),
                Some(f) if f.score() != i.score() => stats.intensity_varies = true,
                _ => {}
            }
            stats.intensity = Some(i);
        }
    }
    stats
}

/// 轮次进度文案：`已 3 轮` / `3 轮`
///
/// `finished=false`（进行中）时数字还会继续涨，故加「已」字；轮次编号跨压缩累计，
/// 与本片段轮数不一致时两个都给，否则会让人以为丢了轮次。
pub fn round_progress_label(rounds: &[ThinkTraceRoundDto], finished: bool) -> String {
    let stats = summarize_rounds(rounds);
    if stats.recorded == 0 {
        return if finished {
            "0 轮".to_string()
        } else {
            "尚未开始".to_string()
        };
    }
    let prefix = if finished { "" } else { "已 " };
    if stats.last_round == stats.recorded {
        format!("{prefix}{} 轮", stats.recorded)
    } else {
        format!(
            "{prefix}{} 轮（累计第 {} 轮）",
            stats.recorded, stats.last_round
        )
    }
}

/// token 明细 2×2 badge 网格（逐轮时间线 + 折叠态概览共用）
///
/// 顺序按「左列入/出单字、右列缓存/思考两字」排布；时间线传**单轮**值、
/// 折叠概览传**累计**值，配色与文案保持同一套，避免两种视觉读法。
#[component]
fn TokenBadgeGrid(
    input_tokens: u64,
    cached_tokens: u64,
    output_tokens: u64,
    reasoning_tokens: u64,
) -> Element {
    let input_text = format_compact_count(input_tokens);
    let cached_text = format_compact_count(cached_tokens);
    let output_text = format_compact_count(output_tokens);
    let reasoning_text = format_compact_count(reasoning_tokens);
    rsx! {
        div { class: "grid grid-cols-2 gap-1 ml-auto shrink-0",
            span {
                class: "badge badge-xs badge-info badge-soft font-mono justify-start min-w-[60px]",
                title: "输入（prompt）token",
                "入 {input_text}"
            }
            span {
                class: "badge badge-xs badge-success badge-soft font-mono justify-start min-w-[60px]",
                title: "提示缓存命中的输入 token；provider 未上报时为 0，不代表未命中",
                "缓存 {cached_text}"
            }
            span {
                class: "badge badge-xs badge-accent badge-soft font-mono justify-start min-w-[60px]",
                title: "输出（completion）token；含思考 token",
                "出 {output_text}"
            }
            span {
                class: "badge badge-xs badge-warning badge-soft font-mono justify-start min-w-[60px]",
                title: "思考/推理消耗的 token，已包含在输出 token 内",
                "思考 {reasoning_text}"
            }
        }
    }
}

/// 右栏：强度 + 耗时上下两行（逐轮时间线 + 折叠态概览共用）
///
/// `scope` 只影响悬停文案（"本轮" / "累计"）；`intensity_note` 在逐轮强度
/// 不一致时补一句说明（只有累计概览会传）。
#[component]
fn IntensityDurationColumn(
    intensity: Option<ThinkingIntensity>,
    duration_ms: u64,
    scope: &'static str,
    intensity_note: Option<&'static str>,
) -> Element {
    let intensity_text = intensity
        .map(thinking_intensity_label)
        .unwrap_or_else(|| "思考·未启用".to_string());
    let duration_text = format_duration(duration_ms);
    let intensity_title = format!(
        "{scope}实际下发的思考强度（0–100 分；未启用时请求体不下发该字段）{}",
        intensity_note.map_or(String::new(), |n| format!("；{n}"))
    );
    let duration_title = format!("{scope}模型调用耗时");
    rsx! {
        div { class: "flex flex-col items-end gap-1 shrink-0",
            span {
                class: "{intensity_class(intensity)}",
                title: "{intensity_title}",
                "{intensity_text}"
            }
            span {
                class: "text-[11px] text-base-content/50 font-mono",
                title: "{duration_title}",
                "{duration_text}"
            }
        }
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
                                    // token 明细 2×2：与折叠态概览共用同一组件（此处是单轮值）
                                    TokenBadgeGrid {
                                        input_tokens: r.input_tokens,
                                        cached_tokens: r.cached_tokens,
                                        output_tokens: r.output_tokens,
                                        reasoning_tokens: r.reasoning_tokens,
                                    }
                                    // 右端两行：上强度标签、下耗时（与折叠态概览共用组件）
                                    IntensityDurationColumn {
                                        intensity: r.thinking_intensity,
                                        duration_ms: r.duration_ms,
                                        scope: "本轮",
                                        intensity_note: None,
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

/// 折叠态的轮次概览：与 [`ThinkRoundsTimeline`] 顶行同构的三栏布局
///
/// 左 = 轮次进度 + 图执行信息，中 = token 2×2，右 = 强度 + 累计耗时；
/// 三栏各占两行高，与展开后每一轮的行完全对齐，展开前后不用重新认位置。
/// 复用 `TokenBadgeGrid` / `intensity_class` / `format_duration`，折叠态是
/// **累计值**、时间线是**单轮值**，只有数值口径不同，没有两套视觉读法。
#[component]
pub fn RoundStatsLine(
    rounds: Vec<ThinkTraceRoundDto>,
    finished: bool,
    steps: usize,
    advisor_calls: usize,
) -> Element {
    let stats = summarize_rounds(&rounds);
    let progress = round_progress_label(&rounds, finished);
    rsx! {
        div { class: "mt-1.5 flex flex-wrap items-start gap-2",
            // 左栏：对应时间线的「第 N 轮 + 裁决」竖排
            div { class: "flex flex-col items-start justify-center gap-1 shrink-0 self-stretch",
                span {
                    class: "text-sm font-semibold whitespace-nowrap leading-none",
                    title: "已记录轮数（进行中会继续增长）",
                    "{progress}"
                }
                if finished {
                    span { class: "badge badge-xs badge-ghost",
                        "{steps} 步 · advisor {advisor_calls}"
                    }
                } else {
                    // 图执行信息只有收尾行才写，增量行恒为 0；「0 步」会把
                    // 「还没落盘」说成「一步没走」，故只提示状态
                    span { class: "text-[11px] text-base-content/50", "图信息待收尾" }
                }
            }
            if stats.recorded > 0 {
                // 中栏：token 2×2（累计值），与时间线共用组件
                TokenBadgeGrid {
                    input_tokens: stats.input_tokens,
                    cached_tokens: stats.cached_tokens,
                    output_tokens: stats.output_tokens,
                    reasoning_tokens: stats.reasoning_tokens,
                }
                // 右栏：强度 + 累计耗时，上下两行（与逐轮时间线共用组件）
                IntensityDurationColumn {
                    intensity: stats.intensity,
                    duration_ms: stats.duration_ms,
                    scope: "累计",
                    intensity_note: stats.intensity_varies.then_some("逐轮下发的强度不同，此处显示最后一轮"),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_with(
        round: usize,
        input: u64,
        output: u64,
        cached: u64,
        reasoning: u64,
        duration_ms: u64,
        intensity: Option<u8>,
    ) -> ThinkTraceRoundDto {
        ThinkTraceRoundDto {
            round,
            input_tokens: input,
            output_tokens: output,
            cached_tokens: cached,
            reasoning_tokens: reasoning,
            duration_ms,
            thinking_intensity: intensity.map(ThinkingIntensity::new),
            ..Default::default()
        }
    }

    #[test]
    fn summarize_rounds_adds_up_tokens_and_duration() {
        let rounds = vec![
            round_with(1, 100, 20, 5, 4, 500, Some(80)),
            round_with(2, 200, 30, 0, 6, 1_500, Some(80)),
        ];
        let s = summarize_rounds(&rounds);
        assert_eq!(s.recorded, 2);
        assert_eq!(s.last_round, 2);
        assert_eq!(s.input_tokens, 300);
        assert_eq!(s.output_tokens, 50);
        assert_eq!(s.cached_tokens, 5);
        assert_eq!(s.reasoning_tokens, 10);
        assert_eq!(s.duration_ms, 2_000);
        assert_eq!(s.intensity.map(|i| i.score()), Some(80));
        assert!(!s.intensity_varies);
    }

    #[test]
    fn summarize_rounds_keeps_latest_intensity_and_flags_variance() {
        // 各轮一致：不标差异
        let same = vec![
            round_with(1, 0, 0, 0, 0, 0, Some(20)),
            round_with(2, 0, 0, 0, 0, 0, Some(20)),
        ];
        assert!(!summarize_rounds(&same).intensity_varies);
        // 逐轮调整：取最新一轮，并标出「不全相同」
        let varies = vec![
            round_with(1, 0, 0, 0, 0, 0, Some(20)),
            round_with(2, 0, 0, 0, 0, 0, Some(90)),
        ];
        let s = summarize_rounds(&varies);
        assert!(s.intensity_varies);
        assert_eq!(s.intensity.map(|i| i.score()), Some(90));
        // 全轮未启用：None 且不标差异
        let none = vec![round_with(1, 0, 0, 0, 0, 0, None)];
        let s = summarize_rounds(&none);
        assert_eq!(s.intensity, None);
        assert!(!s.intensity_varies);
    }

    #[test]
    fn round_progress_label_marks_running_and_cumulative_rounds() {
        let rounds = vec![
            round_with(1, 0, 0, 0, 0, 0, None),
            round_with(2, 0, 0, 0, 0, 0, None),
        ];
        assert_eq!(round_progress_label(&rounds, true), "2 轮");
        // 进行中：数字还会涨
        assert_eq!(round_progress_label(&rounds, false), "已 2 轮");
        assert_eq!(round_progress_label(&[], false), "尚未开始");
        assert_eq!(round_progress_label(&[], true), "0 轮");
        // 跨压缩：本片段 2 轮，但累计编号已到第 8 轮 —— 两个都要给
        let cumulative = vec![
            round_with(7, 0, 0, 0, 0, 0, None),
            round_with(8, 0, 0, 0, 0, 0, None),
        ];
        assert_eq!(
            round_progress_label(&cumulative, false),
            "已 2 轮（累计第 8 轮）"
        );
    }

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
