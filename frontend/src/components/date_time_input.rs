//! 通用日期时间输入（HUD 风格自绘日期时间面板）
//!
//! 浏览器原生 `<input type="datetime-local">` 的选择器是亮色系统面板，
//! 与暗色 HUD 主题割裂（实测在 orz-hud 下是一块刺眼白板）。本组件自绘
//! 暗色面板（复用 `.orz-popover` 切角样式），点击输入框任意位置就地展开：
//! 上/下月切换、今天快捷键、点日期选日、时/分滚轮微调、点「完成」应用。
//!
//! 值语义仍为 `"YYYY-MM-DDTHH:MM"` 字符串，与 `utils::time` 的
//! `parse_datetime_local_to_ms` / `ms_to_datetime_local_value` 直接互通；
//! 对外 props 与原生版本完全一致，调用方无需改动。

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, Timelike};
use dioxus::prelude::*;

const VALUE_FORMAT: &str = "%Y-%m-%dT%H:%M";

#[derive(Props, Clone, PartialEq)]
pub struct DateTimeInputProps {
    /// `datetime-local` 口径的值："YYYY-MM-DDTHH:MM"，空串表示未选
    pub value: String,
    /// 值变化回调（选日或滚轮改时间时即时触发）
    pub on_change: EventHandler<String>,
    /// 输入框样式类（默认 DaisyUI 全宽输入框）
    #[props(default = "input input-bordered w-full cursor-pointer".to_string())]
    pub class: String,
    #[props(default)]
    pub id: String,
    #[props(default = false)]
    pub disabled: bool,
}

fn parse_value(value: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(value, VALUE_FORMAT).ok()
}

fn display_value(value: &str) -> String {
    parse_value(value)
        .map(|dt| dt.format("%Y-%m-%d  %H:%M").to_string())
        .unwrap_or_default()
}

/// 日期 + "HH:MM" → 对外值；非法时间返回 None（不提交脏值）
fn serialize(date: NaiveDate, time: &str) -> Option<String> {
    let (hour, minute) = time.split_once(':')?;
    let hour: u32 = hour.trim().parse().ok()?;
    let minute: u32 = minute.trim().parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some(format!("{}T{hour:02}:{minute:02}", date.format("%Y-%m-%d")))
}

/// 由滚轮的时/分直接提交当前选中日期
fn commit(date: NaiveDate, hour: i32, minute: i32, on_change: &EventHandler<String>) {
    if let Some(value) = serialize(date, &format!("{hour:02}:{minute:02}")) {
        on_change.call(value);
    }
}

/// 滚轮取模：到边界循环（23 往下是 00，59 往上是 00）
fn wheel_step(current: i32, delta: i32, len: i32) -> i32 {
    if len <= 0 {
        return 0;
    }
    (current + delta).rem_euclid(len)
}

/// 月份平移（chrono 没有 add_months，手动按年月计算，回落月初）
fn shift_month(date: NaiveDate, delta: i32) -> NaiveDate {
    let total = date.year() * 12 + date.month() as i32 - 1 + delta;
    let year = total.div_euclid(12);
    let month = total.rem_euclid(12) as u32 + 1;
    NaiveDate::from_ymd_opt(year, month, 1).unwrap_or(date)
}

/// 单个滚轮列：中间为当前值，上下各露出两格；滚轮与点击相邻格都能改值
#[derive(Props, Clone, PartialEq)]
struct WheelColumnProps {
    value: i32,
    len: i32,
    label: &'static str,
    on_select: EventHandler<i32>,
}

struct WheelItem {
    offset: i32,
    value: i32,
}

#[component]
fn WheelColumn(props: WheelColumnProps) -> Element {
    let current = props.value.rem_euclid(props.len.max(1));
    let rows: Vec<WheelItem> = (-2..=2)
        .map(|offset| WheelItem {
            offset,
            value: wheel_step(current, offset, props.len),
        })
        .collect();

    rsx! {
        div { class: "hud-wheel-col",
            span { class: "hud-wheel-label", "{props.label}" }
            div {
                class: "hud-wheel-strip",
                onwheel: move |event| {
                    // 阻止页面/弹窗跟着滚动；只认垂直方向
                    event.prevent_default();
                    let delta_y = event.delta().strip_units().y;
                    if delta_y == 0.0 {
                        return;
                    }
                    let step = if delta_y > 0.0 { 1 } else { -1 };
                    props.on_select.call(wheel_step(current, step, props.len));
                },
                for item in rows {
                    button {
                        key: "{item.offset}",
                        class: if item.offset == 0 {
                            "hud-wheel-item is-current"
                        } else {
                            "hud-wheel-item"
                        },
                        onclick: move |_| props.on_select.call(item.value),
                        "{item.value:02}"
                    }
                }
            }
        }
    }
}

/// 日期时间输入：整框点击就地展开 HUD 面板
///
/// 面板走文档流（不做 fixed 坐标计算）：弹窗 `.hud-modal` 带 backdrop-filter，
/// 会让后代 fixed 参照系从视口变成弹窗，叠加弹窗内部滚动后坐标漂移很大。
#[component]
pub fn DateTimeInput(props: DateTimeInputProps) -> Element {
    let mut open = use_signal(|| false);
    let initial_date = parse_value(&props.value)
        .map(|dt| dt.date())
        .unwrap_or_else(|| Local::now().date_naive());
    let mut view_date = use_signal(|| initial_date);
    let mut draft_date = use_signal(|| initial_date);
    let mut draft_hour = use_signal(|| {
        parse_value(&props.value)
            .map(|dt| dt.hour() as i32)
            .unwrap_or(9)
    });
    let mut draft_minute = use_signal(|| {
        parse_value(&props.value)
            .map(|dt| dt.minute() as i32)
            .unwrap_or(0)
    });

    let year = view_date().year();
    let month = view_date().month();
    let month_first = match NaiveDate::from_ymd_opt(year, month, 1) {
        Some(date) => date,
        None => view_date(),
    };
    let weekday_offset = month_first.weekday().num_days_from_sunday() as i64;
    let grid_start = month_first - Duration::days(weekday_offset);
    let cells: Vec<NaiveDate> = (0..42)
        .map(|index| grid_start + Duration::days(index))
        .collect();

    let selected_date = draft_date();
    let today = Local::now().date_naive();
    let input_class = format!("{} pr-9", props.class);

    rsx! {
        div { class: "relative",
            input {
                class: "{input_class}",
                id: "{props.id}",
                r#type: "text",
                readonly: true,
                disabled: props.disabled,
                value: display_value(&props.value),
                onclick: move |_| {
                    if props.disabled {
                        return;
                    }
                    if let Some(dt) = parse_value(&props.value) {
                        view_date.set(dt.date());
                        draft_date.set(dt.date());
                        draft_hour.set(dt.hour() as i32);
                        draft_minute.set(dt.minute() as i32);
                    }
                    open.set(!open());
                },
            }
            span {
                class: "absolute right-2.5 top-1/2 -translate-y-1/2 pointer-events-none opacity-55",
                svg {
                    width: 14, height: 14, view_box: "0 0 24 24",
                    fill: "none", stroke: "currentColor", stroke_width: 2,
                    rect { x: 3, y: 5, width: 18, height: 16, rx: 2 }
                    path { d: "M8 3v4M16 3v4M3 10h18" }
                }
            }

            if open() {
                // 点面板外关闭；面板在文档流中就地展开，不计算视口坐标
                div {
                    class: "fixed inset-0 z-20",
                    onclick: move |_| open.set(false),
                }
                div {
                    class: "orz-popover relative z-30 mt-1 p-3 w-64",
                    onmounted: move |evt: MountedEvent| {
                        if let Some(element) = evt.data().downcast::<web_sys::Element>() {
                            element.scroll_into_view();
                        }
                    },
                    div { class: "flex items-center justify-between mb-2",
                        div { class: "flex items-center gap-1",
                            button {
                                class: "btn hud-btn btn-ghost btn-xs",
                                onclick: move |_| view_date.set(shift_month(view_date(), -1)),
                                "‹"
                            }
                            strong { class: "font-display text-sm tabular-nums px-1", "{year} 年 {month} 月" }
                            button {
                                class: "btn hud-btn btn-ghost btn-xs",
                                onclick: move |_| view_date.set(shift_month(view_date(), 1)),
                                "›"
                            }
                        }
                        button {
                            class: "btn hud-btn btn-ghost btn-xs",
                            onclick: move |_| {
                                view_date.set(today);
                                draft_date.set(today);
                                commit(today, draft_hour(), draft_minute(), &props.on_change);
                                open.set(false);
                            },
                            "今天"
                        }
                    }

                    div { class: "hud-datepicker-week",
                        for label in ["日", "一", "二", "三", "四", "五", "六"] {
                            span { "{label}" }
                        }
                    }
                    div { class: "hud-datepicker-grid",
                        for date in cells {
                            button {
                                key: "{date}",
                                class: {
                                    let mut classes = String::from("hud-datepicker-cell");
                                    if date.month() != month { classes.push_str(" is-outside"); }
                                    if date == today { classes.push_str(" is-today"); }
                                    if selected_date == date { classes.push_str(" is-selected"); }
                                    classes
                                },
                                onclick: move |_| {
                                    // 选日后留在面板里继续用滚轮调时间，值即时提交
                                    draft_date.set(date);
                                    commit(date, draft_hour(), draft_minute(), &props.on_change);
                                },
                                "{date.day()}"
                            }
                        }
                    }

                    div { class: "hud-time-wheel mt-3 pt-2 border-t border-base-content/10",
                        div { class: "flex items-center justify-between mb-1",
                            span { class: "hud-eyebrow", "TIME · 滚轮调节" }
                            strong { class: "font-display text-sm tabular-nums",
                                "{draft_hour():02}:{draft_minute():02}"
                            }
                        }
                        div { class: "flex items-center justify-center gap-2",
                            WheelColumn {
                                value: draft_hour(),
                                len: 24,
                                label: "时",
                                on_select: move |value| {
                                    draft_hour.set(value);
                                    commit(draft_date(), value, draft_minute(), &props.on_change);
                                },
                            }
                            span { class: "hud-wheel-sep", ":" }
                            WheelColumn {
                                value: draft_minute(),
                                len: 60,
                                label: "分",
                                on_select: move |value| {
                                    draft_minute.set(value);
                                    commit(draft_date(), draft_hour(), value, &props.on_change);
                                },
                            }
                        }
                    }

                    div { class: "flex items-center justify-between mt-2 pt-2 border-t border-base-content/10",
                        span { class: "text-xs text-base-content/40", "滚轮或点相邻数字调节" }
                        button {
                            class: "btn hud-btn btn-primary btn-xs",
                            onclick: move |_| {
                                commit(draft_date(), draft_hour(), draft_minute(), &props.on_change);
                                open.set(false);
                            },
                            "完成"
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
    fn parse_and_render_roundtrip() {
        let value = "2026-11-26T12:30";
        let parsed = parse_value(value).unwrap();
        assert_eq!(parsed.format(VALUE_FORMAT).to_string(), value);
        assert_eq!(display_value(value), "2026-11-26  12:30");
    }

    #[test]
    fn serialize_rejects_invalid_time() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 26).unwrap();
        assert_eq!(serialize(date, "9:05").as_deref(), Some("2026-11-26T09:05"));
        assert_eq!(serialize(date, "24:00"), None);
        assert_eq!(serialize(date, "12:60"), None);
        assert_eq!(serialize(date, "oops"), None);
    }

    #[test]
    fn shift_month_handles_year_boundary() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        assert_eq!(
            shift_month(date, -1),
            NaiveDate::from_ymd_opt(2025, 12, 1).unwrap()
        );
        assert_eq!(
            shift_month(date, 11),
            NaiveDate::from_ymd_opt(2026, 12, 1).unwrap()
        );
    }

    #[test]
    fn wheel_step_wraps_at_both_ends() {
        assert_eq!(wheel_step(0, -1, 24), 23);
        assert_eq!(wheel_step(23, 1, 24), 0);
        assert_eq!(wheel_step(59, 1, 60), 0);
        assert_eq!(wheel_step(0, -1, 60), 59);
        assert_eq!(wheel_step(7, 2, 60), 9);
        assert_eq!(wheel_step(3, 1, 0), 0);
    }
}
