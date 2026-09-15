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
/// 跨度不超过该阈值建议按分钟聚合
///
/// 取 3 小时（最多 180 个桶）：既覆盖「最近 1 小时」这个主用例，也覆盖手选的
/// 1~3 小时自定义区间；再往上分钟桶就只剩密集噪声了。后端另有一道**独立**的桶数
/// 护栏（`common::models::STATS_MAX_BUCKETS`）兜住手写 query 的超限请求 ——
/// 前端这道只是启发式，不能替代后端自兜底。
const MINUTELY_MAX_SPAN_MS: i64 = 3 * HOUR_MS;
/// 跨度超过该阈值建议按天聚合，否则按小时（避免「宽窗口配细粒度」只出密集噪声）
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

    /// 建议聚合粒度（后端 `stats_interval` 取值：`minutely` / `hourly` / `daily`）
    ///
    /// 三档与后端白名单一一对应：≤3h 分钟桶 / ≤2d 小时桶 / 否则天桶。
    /// 跨度落在档位边界附近时宁可选粗 —— 后端按桶数上限收敛只会更粗，
    /// 前端选细反而会拿到一张与预期不符的图（数据点被静默抽稀）。
    pub fn suggested_interval(&self) -> &'static str {
        let span = self.span_ms();
        if span <= MINUTELY_MAX_SPAN_MS {
            "minutely"
        } else if span <= HOURLY_MAX_SPAN_MS {
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

#[cfg(test)]
mod tests {
    use super::*;
    use common::models::{StatsInterval, clamp_interval_to_span};

    // ⚠️ 本模块在 host（native）上运行：**不可调用 `now_ms()` 及其下游**。
    // 它最终落到 `js_sys::Date::now()`，非 wasm 目标会直接 panic
    // （"cannot call wasm-bindgen imported functions on non-wasm targets"）。
    // 因此 `TimeRange::from_preset` 在本模块里不可用，只能用下面的
    // `range_of_span` 按「预设对应的跨度」构造区间（见 `from_preset`：
    // 1 小时 / 1 天 / 7 天 / 30 天）。

    /// 造一个指定跨度的区间（固定基准时间，避免测试依赖真实时钟）
    fn range_of_span(span_ms: i64) -> TimeRange {
        let end_ms = 1_700_000_000_000;
        TimeRange {
            start_ms: end_ms - span_ms,
            end_ms,
            preset: TimeRangePreset::Custom,
        }
    }

    /// 前端粒度字符串 → 枚举（顺带校验取值确实落在后端白名单内）
    fn as_interval(name: &str) -> StatsInterval {
        match name {
            "minutely" => StatsInterval::Minutely,
            "hourly" => StatsInterval::Hourly,
            "daily" => StatsInterval::Daily,
            other => panic!("suggested_interval 返回了白名单外的取值: {other}"),
        }
    }

    /// 各预设跨度对应的档位（跨度取 `from_preset` 的口径：1 小时 / 1 天 / 7 天 / 30 天）
    ///
    /// 「最近 1 小时」是本次改造的主用例：1 小时窗口若用小时桶，整段只剩 1 个点、画不出线。
    #[test]
    fn preset_spans_map_to_expected_tier() {
        assert_eq!(range_of_span(HOUR_MS).suggested_interval(), "minutely");
        assert_eq!(range_of_span(DAY_MS).suggested_interval(), "hourly");
        assert_eq!(range_of_span(7 * DAY_MS).suggested_interval(), "daily");
        assert_eq!(range_of_span(30 * DAY_MS).suggested_interval(), "daily");
    }

    #[test]
    fn tier_boundaries_are_inclusive() {
        assert_eq!(
            range_of_span(MINUTELY_MAX_SPAN_MS).suggested_interval(),
            "minutely"
        );
        assert_eq!(
            range_of_span(MINUTELY_MAX_SPAN_MS + 1).suggested_interval(),
            "hourly"
        );
        assert_eq!(
            range_of_span(HOURLY_MAX_SPAN_MS).suggested_interval(),
            "hourly"
        );
        assert_eq!(
            range_of_span(HOURLY_MAX_SPAN_MS + 1).suggested_interval(),
            "daily"
        );
    }

    /// 前端选出的粒度不应被后端桶数护栏改写
    ///
    /// 一旦被改写，界面拿到的是一张分辨率与预期不符、且看不出原因的图
    /// （后端只会留一条 debug 日志）。这条测试把两半契约钉在一起：
    /// 前端调档时若超出后端预算，这里会先红。
    #[test]
    fn suggested_tier_survives_backend_bucket_guard() {
        // 四个预设跨度 + 两个档位边界 + 一个自定义上限（29 天）
        let spans = [
            HOUR_MS,
            DAY_MS,
            7 * DAY_MS,
            30 * DAY_MS,
            MINUTELY_MAX_SPAN_MS,
            HOURLY_MAX_SPAN_MS,
            29 * DAY_MS,
        ];
        for span in spans {
            let range = range_of_span(span);
            let requested = as_interval(range.suggested_interval());
            assert_eq!(
                clamp_interval_to_span(requested, range.span_ms()),
                requested,
                "跨度 {span}ms 的粒度会被后端护栏改写"
            );
        }
    }
}
