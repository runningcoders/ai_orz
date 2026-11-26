//! 触发器表单公共字段：名称 + 时间规则（cron / interval / once）。
//!
//! 新建弹窗（triggers.rs）与详情页编辑（trigger_detail.rs）共用这一段，
//! 避免两处重复渲染、cron 预设口径漂移。身份 / 动作字段（业务形态、行为、
//! Agent、payload）各页面语义不同，留在各自页面；业务形态可选传入，用于
//! 新建弹窗里与「时间规则」并排展示。

use crate::components::date_time_input::DateTimeInput;
use dioxus::prelude::*;

/// cron 快捷预设（分 时 日 月 周）
const CRON_PRESETS: [(&str, &str); 6] = [
    ("每分钟", "* * * * *"),
    ("每小时", "0 * * * *"),
    ("每天 0 点", "0 0 * * *"),
    ("每天 9 点", "0 9 * * *"),
    ("每周一 9 点", "0 9 * * 1"),
    ("每月 1 号", "0 0 1 * *"),
];

pub const RULE_CRON: &str = "cron";
pub const RULE_INTERVAL: &str = "interval";
pub const RULE_ONCE: &str = "once";

/// datetime-local 字符串 → Unix 毫秒（本地时区）。
/// 与后端 run_at 口径一致（`datetime-local` value 形如 "2026-11-25T09:00"）。
pub fn parse_local_datetime_ms(value: &str) -> Option<i64> {
    use chrono::{Local, TimeZone};
    let naive = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M").ok()?;
    Local
        .from_local_datetime(&naive)
        .single()
        .map(|dt| dt.timestamp_millis())
}

/// Unix 毫秒 → datetime-local 字符串（一次性规则回填用）。
pub fn format_local_datetime(ts_ms: i64) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_opt(ts_ms / 1000, 0) {
        chrono::LocalResult::Single(dt) => dt.format("%Y-%m-%dT%H:%M").to_string(),
        _ => String::new(),
    }
}

/// 名称 + 业务形态（可选）+ 时间规则三选一 + 对应输入控件。
///
/// 状态全部由父组件持有的 Signal 驱动，本组件只负责渲染与回填，
/// 提交 / 校验仍在各页面 —— 这样新建与编辑能共用 UI，又各自保留语义。
#[component]
pub fn TriggerBasicFields(
    name: Signal<String>,
    rule: Signal<String>,
    cron: Signal<String>,
    interval: Signal<String>,
    run_at_local: Signal<String>,
    /// 业务形态（日程 / 系统定时任务）：仅新建弹窗传入，与时间规则并排
    #[props(default = None)]
    shape: Option<Signal<String>>,
    /// 是否开放「一次性（指定时刻）」：默认开放，两页都可改 run_at
    #[props(default = true)]
    allow_once: bool,
    /// 名称占位提示
    #[props(default = "例：每日 9 点沉淀 Agent 记忆".to_string())]
    name_placeholder: String,
    /// 时间规则下方的说明文字（新建弹窗用于解释两个维度）
    #[props(default = None)]
    hint: Option<String>,
) -> Element {
    rsx! {
        div { class: "form-control w-full",
            label { class: "form-label", "名称 *" }
            input {
                class: "input input-bordered w-full",
                value: "{name}",
                placeholder: "{name_placeholder}",
                oninput: move |e| name.set(e.value()),
            }
        }
        // 两个维度分开选：业务类型是「日程 / 系统定时任务」，时间规则只决定何时触发
        div { class: "grid grid-cols-1 md:grid-cols-2 gap-3",
            if let Some(mut shape) = shape {
                div { class: "form-control w-full",
                    label { class: "form-label", "类型（业务）" }
                    select {
                        class: "select select-bordered w-full",
                        value: "{shape}",
                        onchange: move |e| shape.set(e.value()),
                        option { value: "system", "系统定时任务" }
                        option { value: "schedule", "日程" }
                    }
                }
            }
            div { class: "form-control w-full",
                label { class: "form-label", "时间规则" }
                select {
                    class: "select select-bordered w-full",
                    value: "{rule}",
                    onchange: move |e| rule.set(e.value()),
                    option { value: RULE_CRON, "Cron 表达式" }
                    option { value: RULE_INTERVAL, "固定间隔" }
                    if allow_once {
                        option { value: RULE_ONCE, "一次性（指定时刻）" }
                    }
                }
            }
        }
        if let Some(hint) = hint {
            div { class: "form-hint mb-2", "{hint}" }
        }
        if rule() == RULE_CRON {
            div { class: "form-control w-full",
                label { class: "form-label", "Cron 表达式 *" }
                input {
                    class: "form-input font-mono",
                    value: "{cron}",
                    placeholder: "0 9 * * *（分 时 日 月 周）",
                    oninput: move |e| cron.set(e.value()),
                }
                div { class: "cron-presets",
                    for (label, expr) in CRON_PRESETS.iter() {
                        {
                            let expr = expr.to_string();
                            rsx! {
                                button {
                                    class: "btn hud-btn btn-ghost btn-sm cron-preset-btn",
                                    onclick: move |_| cron.set(expr.clone()),
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            }
        }
        if rule() == RULE_INTERVAL {
            div { class: "form-control w-full max-w-xs",
                label { class: "form-label", "间隔秒数 *" }
                input {
                    class: "input input-bordered w-full",
                    r#type: "number",
                    value: "{interval}",
                    placeholder: "300（即 5 分钟）",
                    oninput: move |e| interval.set(e.value()),
                }
            }
        }
        if allow_once && rule() == RULE_ONCE {
            div { class: "form-control w-full",
                label { class: "form-label", "执行时刻 *" }
                DateTimeInput {
                    value: run_at_local(),
                    on_change: move |v| run_at_local.set(v),
                }
            }
        }
    }
}
