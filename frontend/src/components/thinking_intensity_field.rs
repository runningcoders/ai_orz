//! 思考强度（0–100 分）配置控件
//!
//! 平台内部统一 0–100 分刻度；下游厂商的 wire 形式（完整六档
//! `minimal`/`low`/`medium`/`high`/`xhigh`/`max`）由 Provider 配置决定，
//! 转换集中在 [`ThinkingIntensity::to_reasoning_effort_wire`]。
//! 本控件只负责「启用开关 + 滑块 + 当前生效档位提示」，分值状态由调用方持有。

use common::enums::ThinkingIntensity;
use dioxus::prelude::*;

/// 分数 → 下游六档文案（与 `to_reasoning_effort_wire` 同源，避免 UI 自行切档）
pub fn thinking_intensity_tier_label(intensity: ThinkingIntensity) -> &'static str {
    match intensity.to_reasoning_effort_wire() {
        "minimal" => "极低",
        "low" => "低",
        "medium" => "中",
        "high" => "高",
        "xhigh" => "超高",
        _ => "最高",
    }
}

/// 分数 → 徽标文案（列表 / 详情展示共用，含具体分值）
pub fn thinking_intensity_label(intensity: ThinkingIntensity) -> String {
    format!(
        "思考·{} {}",
        thinking_intensity_tier_label(intensity),
        intensity.score()
    )
}

/// 「启用开关 + 0–100 滑块 + 下游档位提示」组合控件
///
/// - `enabled` / `score` 由调用方 state 提供，回调只上报变更
/// - 未启用时不渲染滑块，并明确提示「请求体不下发该字段」
#[component]
pub fn ThinkingIntensityField(
    enabled: bool,
    score: u8,
    /// 控件说明（如「思考强度」「思考强度覆盖」）
    label: String,
    /// data-testid 前缀（如 `mp-create-thinking-intensity`）
    testid: String,
    on_toggle: EventHandler<bool>,
    on_score: EventHandler<u8>,
) -> Element {
    let testid_enabled = format!("{testid}-enabled");
    let testid_score = format!("{testid}-score");
    rsx! {
        div { class: "form-control w-full",
            label { class: "flex cursor-pointer select-none items-center gap-2",
                input {
                    class: "checkbox checkbox-sm checkbox-primary",
                    r#type: "checkbox",
                    checked: enabled,
                    "data-testid": "{testid_enabled}",
                    onchange: move |e| on_toggle.call(e.checked()),
                }
                span { class: "text-sm font-medium", "启用{label}（0–100 分）" }
            }
            if enabled {
                div { class: "mt-2 flex items-center gap-3",
                    input {
                        class: "range range-primary range-sm flex-1",
                        r#type: "range",
                        min: "0",
                        max: "100",
                        step: "1",
                        value: "{score}",
                        "data-testid": "{testid_score}",
                        oninput: move |e| {
                            if let Ok(v) = e.value().parse::<u8>() {
                                on_score.call(v);
                            }
                        },
                    }
                    span { class: "badge hud-badge badge-ghost badge-sm font-mono", "{score}" }
                }
                p { class: "mt-1 text-xs text-base-content/60",
                    "下游当前生效档位：{thinking_intensity_tier_label(ThinkingIntensity::new(score))}"
                    "（0–16 = 极低 / 17–33 = 低 / 34–50 = 中 / 51–66 = 高 / 67–83 = 超高 / 84–100 = 最高）"
                }
            } else {
                p { class: "mt-1 text-xs text-base-content/60",
                    "未启用：请求体不下发思考强度字段，Agent 覆盖与场景偏移也不会生效"
                }
            }
        }
    }
}
