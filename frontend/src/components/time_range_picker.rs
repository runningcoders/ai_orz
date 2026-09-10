//! 通用时间区间筛选组件（预设快捷项 + 自定义起止）
//!
//! 设计约束（复用场景：详情页统计看板 / 日志检索 / 未来的报表页）：
//! - **值语义统一为毫秒时间戳闭区间** `[start_ms, end_ms]`，与后端
//!   `stats_time_start`/`stats_time_end`（model provider 为
//!   `stats_start_time`/`stats_end_time`）同口径。
//! - 组件自身**不发起任何请求**，只产出 `TimeRange`；由父级决定如何带参查询。
//! - props 仅 `value: TimeRange`（Copy + PartialEq）+ `on_change: EventHandler<TimeRange>`：
//!   父级用 `use_callback` 传入稳定 handler，避免父重渲染打断自定义输入
//!   （参见 dioxus-input-rerender-isolation）。
//! - 自定义输入草稿是组件内部状态，预设点击 / 应用 时与 `value` 对齐；
//!   父级不应在组件外部改写区间（组件不会反向同步草稿）。
//!
//! 用例：
//! ```ignore
//! let mut range = use_signal(TimeRange::default); // 默认最近 7 天
//! let on_range = use_callback(move |r: TimeRange| range.set(r));
//! rsx! {
//!     TimeRangePicker { value: range(), on_change: on_range }
//! }
//! ```

use dioxus::prelude::*;

use crate::utils::time::{
    format_datetime, ms_to_datetime_local_value, now_ms, parse_datetime_local_to_ms,
};

const HOUR_MS: i64 = 3_600_000;
const DAY_MS: i64 = 86_400_000;
/// 跨度超过该阈值建议按天聚合，否则按小时（避免「1 小时区间只出 1 个点」）
const HOURLY_MAX_SPAN_MS: i64 = 2 * DAY_MS;

/// 时间区间预设
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TimeRangePreset {
    /// 最近 1 小时
    LastHour,
    /// 最近 1 天
    LastDay,
    /// 最近 7 天
    LastWeek,
    /// 最近 30 天
    LastMonth,
    /// 自定义起止
    Custom,
}

impl TimeRangePreset {
    /// 快捷预设（不含 Custom，Custom 由组件单独渲染）
    pub const PRESETS: [TimeRangePreset; 4] = [
        TimeRangePreset::LastHour,
        TimeRangePreset::LastDay,
        TimeRangePreset::LastWeek,
        TimeRangePreset::LastMonth,
    ];

    /// 按钮文案
    pub fn label(self) -> &'static str {
        match self {
            TimeRangePreset::LastHour => "最近 1 小时",
            TimeRangePreset::LastDay => "最近 1 天",
            TimeRangePreset::LastWeek => "最近 7 天",
            TimeRangePreset::LastMonth => "最近 30 天",
            TimeRangePreset::Custom => "自定义",
        }
    }
}

/// 时间区间（毫秒时间戳闭区间 `[start_ms, end_ms]`）
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TimeRange {
    pub start_ms: i64,
    pub end_ms: i64,
    pub preset: TimeRangePreset,
}

impl TimeRange {
    /// 距离当前 `hours` 小时
    pub fn last_hours(hours: i64) -> Self {
        let end_ms = now_ms();
        Self {
            start_ms: end_ms - hours * HOUR_MS,
            end_ms,
            preset: TimeRangePreset::LastHour,
        }
    }

    /// 距离当前 `days` 天
    pub fn last_days(days: i64) -> Self {
        let end_ms = now_ms();
        Self {
            start_ms: end_ms - days * DAY_MS,
            end_ms,
            preset: TimeRangePreset::LastWeek,
        }
    }

    /// 预设 → 区间（Custom 无对应预设，退回默认 7 天）
    ///
    /// ⚠️ `last_hours` / `last_days` 内部的 `preset` 是各自硬编码的，这里必须按入参覆写：
    /// 否则「最近 1 天 / 30 天」走 `last_days(...)` 后 preset 恒为 `LastWeek`，
    /// 区间变了但按钮高亮仍停在「最近 7 天」。
    pub fn from_preset(preset: TimeRangePreset) -> Self {
        match preset {
            TimeRangePreset::LastHour => Self::last_hours(1),
            TimeRangePreset::LastDay => Self {
                preset,
                ..Self::last_days(1)
            },
            TimeRangePreset::LastWeek => Self::last_days(7),
            TimeRangePreset::LastMonth => Self {
                preset,
                ..Self::last_days(30)
            },
            TimeRangePreset::Custom => Self::last_days(7),
        }
    }

    /// 自定义区间（自动裁剪为 start <= end）
    pub fn custom(start_ms: i64, end_ms: i64) -> Self {
        Self {
            start_ms: start_ms.min(end_ms),
            end_ms: start_ms.max(end_ms),
            preset: TimeRangePreset::Custom,
        }
    }

    /// 区间跨度（毫秒）
    pub fn span_ms(&self) -> i64 {
        (self.end_ms - self.start_ms).max(0)
    }

    /// 建议聚合粒度（后端 `stats_interval` 取值：`hourly` / `daily`）
    pub fn suggested_interval(&self) -> &'static str {
        if self.span_ms() <= HOURLY_MAX_SPAN_MS {
            "hourly"
        } else {
            "daily"
        }
    }

    /// 起止的可读描述（"YYYY-MM-DD HH:MM ~ YYYY-MM-DD HH:MM"）
    pub fn label(&self) -> String {
        format!(
            "{} ~ {}",
            format_datetime(self.start_ms),
            format_datetime(self.end_ms)
        )
    }
}

impl Default for TimeRange {
    /// 默认「最近 7 天」——与后端统计接口的缺省窗口一致
    fn default() -> Self {
        Self::last_days(7)
    }
}

/// 时间区间筛选器 Props
#[derive(Props, Clone, PartialEq)]
pub struct TimeRangePickerProps {
    /// 当前生效区间
    pub value: TimeRange,
    /// 区间变化回调（点击预设或应用自定义时触发）
    pub on_change: EventHandler<TimeRange>,
    /// 紧凑变体（窄容器用：更小的间距与字号）
    #[props(default = false)]
    pub compact: bool,
    /// 是否隐藏当前区间的文字描述（父级另行展示时用）
    #[props(default = false)]
    pub hide_label: bool,
}

/// 时间区间筛选器（预设快捷项 + 自定义起止）
#[component]
pub fn TimeRangePicker(props: TimeRangePickerProps) -> Element {
    // 自定义面板开合 + 输入草稿。草稿只在组件内部流转：预设点击 / 应用 时才写回，
    // 避免每次 keystroke 都回写父级信号（父级重渲染会打断输入）。
    let mut custom_open = use_signal(|| props.value.preset == TimeRangePreset::Custom);
    let mut draft_start = use_signal(|| ms_to_datetime_local_value(props.value.start_ms));
    let mut draft_end = use_signal(|| ms_to_datetime_local_value(props.value.end_ms));
    let mut custom_error = use_signal(String::new);

    let item_class = if props.compact {
        "hud-seg-item hud-seg-item-sm"
    } else {
        "hud-seg-item"
    };
    let input_class = if props.compact {
        "input input-bordered input-xs font-mono w-full"
    } else {
        "input input-bordered input-sm font-mono w-52"
    };
    // 紧凑变体（窄侧栏）：起止竖向排列，避免 2 × 输入框横向溢出
    let group_class = if props.compact {
        "flex flex-col gap-1.5"
    } else {
        "flex flex-none items-center gap-1.5"
    };

    rsx! {
        div { class: if props.compact { "flex flex-col gap-1.5" } else { "flex flex-wrap items-center gap-x-3 gap-y-2" },
            div { class: "hud-seg",
                for preset in TimeRangePreset::PRESETS {
                    button {
                        key: "{preset.label()}",
                        class: if props.value.preset == preset { "{item_class} is-active" } else { "{item_class}" },
                        onclick: move |_| {
                            let range = TimeRange::from_preset(preset);
                            // 同步草稿：用户从预设切到「自定义」时可接着微调
                            draft_start.set(ms_to_datetime_local_value(range.start_ms));
                            draft_end.set(ms_to_datetime_local_value(range.end_ms));
                            custom_error.set(String::new());
                            custom_open.set(false);
                            props.on_change.call(range);
                        },
                        "{preset.label()}"
                    }
                }
                button {
                    class: if custom_open() { "{item_class} is-active" } else { "{item_class}" },
                    onclick: move |_| {
                        let open = !custom_open();
                        custom_open.set(open);
                        custom_error.set(String::new());
                    },
                    "{TimeRangePreset::Custom.label()}"
                }
            }

            if custom_open() {
                div { class: "flex flex-wrap items-center gap-1.5",
                    // 「起 ~ 止」作为不可拆分组：窄容器整体换行，不会把起止时间拆散
                    div { class: "{group_class}",
                        input {
                            class: "{input_class}",
                            r#type: "datetime-local",
                            value: "{draft_start}",
                            oninput: move |e| draft_start.set(e.value())
                        }
                        if !props.compact {
                            span { class: "text-xs text-base-content/40", "~" }
                        }
                        input {
                            class: "{input_class}",
                            r#type: "datetime-local",
                            value: "{draft_end}",
                            oninput: move |e| draft_end.set(e.value())
                        }
                    }
                    button {
                        class: "btn hud-btn btn-primary btn-xs",
                        onclick: move |_| {
                            match (
                                parse_datetime_local_to_ms(&draft_start()),
                                parse_datetime_local_to_ms(&draft_end()),
                            ) {
                                (Some(s), Some(e)) if s < e => {
                                    custom_error.set(String::new());
                                    props.on_change.call(TimeRange::custom(s, e));
                                }
                                (Some(_), Some(_)) => {
                                    custom_error.set("起始时间必须早于结束时间".to_string());
                                }
                                _ => {
                                    custom_error.set("请填写完整的起止时间".to_string());
                                }
                            }
                        },
                        "应用"
                    }
                    if !custom_error().is_empty() {
                        span { class: "text-xs text-error", "{custom_error()}" }
                    }
                }
            }

            if !props.hide_label {
                span { class: "hud-eyebrow", "{props.value.label()}" }
            }
        }
    }
}
