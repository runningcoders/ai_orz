//! AOP 队列监控页面

use crate::components::hud::{HudPanel, HudSection, StatGrid, StatReadout};
use dioxus::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::api::system::{
    AopStatsDistributionItem, AopStatsOverviewResponse, AopStatsTimeSeriesPoint,
    EventDetailResponse, EventSummaryResponse, QueueStatsResponse, get_all_queue_stats,
    get_aop_stats_distribution, get_aop_stats_overview, get_aop_stats_time_series, get_event,
    list_events,
};
use crate::components::aop_gauge::AopGauge;
use crate::components::charts::donut_chart::{DonutChart, DonutSlice};
use crate::components::charts::line_chart::LineChart;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;

fn status_badge_class(status: &str) -> &'static str {
    match status {
        "processing" => "badge badge-warning",
        "pending" => "badge badge-info",
        _ => "badge badge-neutral",
    }
}

fn format_created_at(ts: i64) -> String {
    if ts <= 0 {
        return "-".to_string();
    }
    let dt = chrono::DateTime::from_timestamp(ts, 0);
    match dt {
        Some(d) => d
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        None => ts.to_string(),
    }
}

#[component]
pub fn SystemAop() -> Element {
    let toast = use_toast();

    let mut stats = use_signal(Vec::<QueueStatsResponse>::new);
    let mut loading_stats = use_signal(|| false);

    let mut active_tab: Signal<&'static str> = use_signal(|| "monitor");

    let mut selected_consumer = use_signal(|| Option::<String>::None);
    let mut events = use_signal(Vec::<EventSummaryResponse>::new);
    let mut loading_events = use_signal(|| false);

    let mut selected_event = use_signal(|| Option::<EventDetailResponse>::None);
    let mut loading_detail = use_signal(|| false);

    let mut filter_status = use_signal(String::new);

    use_effect(move || {
        loading_stats.set(true);
        spawn(async move {
            match get_all_queue_stats().await {
                Ok(data) => stats.set(data),
                Err(e) => toast.error(format!("加载队列统计失败: {}", e)),
            }
            loading_stats.set(false);
        });
    });

    let stats_data = stats.cloned();
    let consumer = selected_consumer.cloned();
    let events_data = events.cloned();
    let detail = selected_event.cloned();
    let status_filter_val = filter_status.cloned();

    let close_modal = move |_| selected_event.set(None);

    let monitor_btn_class = if *active_tab.read() == "monitor" {
        "btn btn-sm btn-primary"
    } else {
        "btn btn-sm btn-ghost"
    };
    let stats_btn_class = if *active_tab.read() == "stats" {
        "btn btn-sm btn-primary"
    } else {
        "btn btn-sm btn-ghost"
    };

    rsx! {
        AppLayout {
            HudPanel { signal: None,
                div { class: "flex gap-2 mb-4",
                    button {
                        class: "{monitor_btn_class}",
                        onclick: move |_| active_tab.set("monitor"),
                        "实时监控"
                    }
                    button {
                        class: "{stats_btn_class}",
                        onclick: move |_| active_tab.set("stats"),
                        "统计图表"
                    }
                }

                if *active_tab.read() == "monitor" {
                    HudSection { title: "AOP 队列监控".to_string(),
                        actions: Some(rsx!{
                            button {
                                class: "btn btn-ghost btn-sm",
                                onclick: move |_| {
                                    loading_stats.set(true);
                                    spawn(async move {
                                        match get_all_queue_stats().await {
                                            Ok(data) => stats.set(data),
                                            Err(e) => toast.error(format!("加载队列统计失败: {}", e)),
                                        }
                                        loading_stats.set(false);
                                    });
                                },
                                "🔄 刷新"
                            }
                        }),
                    }

                    p { class: "text-base-content/70 mb-4",
                        "查看 AOP 事件中心各消费者队列的运行状态、事件堆积情况和内容排查。"
                    }

                    if loading_stats() {
                        Loading {}
                    } else if stats_data.is_empty() {
                        EmptyState { icon: "📭".to_string(), message: "暂无队列数据".to_string() }
                    } else {
                        div { class: "grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4",
                            for s in stats_data.iter() {
                                {
                                    let name = s.consumer_name.clone();
                                    let pending = s.pending_count;
                                    let in_progress = s.in_progress_count;
                                    let oldest = s.oldest_event_age_secs;
                                    let order_keys_count = s.order_keys.len();
                                    let is_selected = consumer.as_ref() == Some(&name);
                                    let card_name = name.clone();
                                    rsx! {
                                        AopGauge {
                                            consumer_name: name,
                                            pending: pending,
                                            in_progress: in_progress,
                                            oldest_age_secs: oldest,
                                            order_keys_count: order_keys_count,
                                            is_selected: is_selected,
                                            on_click: Some(EventHandler::new(move |_| {
                                                let consumer_name = card_name.clone();
                                                selected_consumer.set(Some(consumer_name.clone()));
                                                selected_event.set(None);
                                                let status = filter_status.cloned();
                                                loading_events.set(true);
                                                spawn(async move {
                                                    match list_events(common::api::ListEventsRequest {
                                                        consumer: consumer_name.clone(),
                                                        limit: Some(100),
                                                        offset: Some(0),
                                                        status: if status.is_empty() { None } else { Some(status.clone()) },
                                                        ..Default::default()
                                                    }).await {
                                                        Ok(data) => events.set(data),
                                                        Err(e) => toast.error(format!("加载事件列表失败: {}", e)),
                                                    }
                                                    loading_events.set(false);
                                                });
                                            })),
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if let Some(consumer_name) = consumer.clone() {
                        HudPanel { signal: Some(true), extra_class: Some("mt-6".to_string()),
                            HudSection { title: format!("事件列表 · {}", consumer_name),
                                actions: Some(rsx!{
                                    select {
                                        class: "form-select form-select-sm",
                                        value: "{status_filter_val}",
                                        onchange: move |e| {
                                            filter_status.set(e.value());
                                            let c = consumer.clone().unwrap();
                                            let s = e.value();
                                            loading_events.set(true);
                                            selected_event.set(None);
                                            spawn(async move {
                                                match list_events(common::api::ListEventsRequest {
                                                    consumer: c.clone(),
                                                    limit: Some(100),
                                                    offset: Some(0),
                                                    status: if s.is_empty() { None } else { Some(s.clone()) },
                                                    ..Default::default()
                                                }).await {
                                                    Ok(data) => events.set(data),
                                                    Err(e) => toast.error(format!("加载事件列表失败: {}", e)),
                                                }
                                                loading_events.set(false);
                                            });
                                        },
                                        option { value: "", "全部状态" }
                                        option { value: "pending", "待处理" }
                                        option { value: "processing", "处理中" }
                                    }
                                }),
                            }

                            if loading_events() {
                                Loading {}
                            } else if events_data.is_empty() {
                                EmptyState { icon: "📭".to_string(), message: "该队列暂无事件".to_string() }
                            } else {
                                div { style: "overflow-x: auto;",
                                    table { class: "table table-sm table-row-clickable",
                                        thead { tr {
                                            th { "事件 ID" }
                                            th { "类型" }
                                            th { "order_key" }
                                            th { "优先级" }
                                            th { "创建时间" }
                                            th { "状态" }
                                        }}
                                        tbody {
                                            for e in events_data.iter() {
                                                {
                                                    let event_id = e.event_id.clone();
                                                    let event_kind = e.event_kind.clone();
                                                    let order_key = e.order_key.clone();
                                                    let priority = e.priority;
                                                    let created_at = e.created_at;
                                                    let status = e.status.clone();
                                                    let cid = consumer_name.clone();
                                                    rsx! {
                                                        tr {
                                                            class: "table-row-clickable",
                                                            onclick: move |_| {
                                                                let c = cid.clone();
                                                                let eid = event_id.clone();
                                                                loading_detail.set(true);
                                                                spawn(async move {
                                                                    match get_event(common::api::GetEventRequest {
                                                                        consumer: c.clone(),
                                                                        event_id: eid.clone(),
                                                                    }).await {
                                                                        Ok(data) => selected_event.set(Some(data)),
                                                                        Err(e) => toast.error(format!("加载事件详情失败: {}", e)),
                                                                    }
                                                                    loading_detail.set(false);
                                                                });
                                                            },
                                                            td { class: "font-mono text-sm",
                                                                "{event_id}"
                                                            }
                                                            td { "{event_kind}" }
                                                            td { class: "font-mono text-sm",
                                                                if order_key.is_empty() {
                                                                    span { class: "text-base-content/70", "-" }
                                                                } else {
                                                                    "{order_key}"
                                                                }
                                                            }
                                                            td { "{priority}" }
                                                            td { class: "text-base-content/70 text-sm",
                                                                "{format_created_at(created_at)}"
                                                            }
                                                            td {
                                                                span { class: "{status_badge_class(&status)}", "{status}" }
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

                    if let Some(d) = detail {
                        Modal {
                            show: true,
                            title: "事件详情".to_string(),
                            on_close: close_modal,
                            div { class: "modal-body",
                                if loading_detail() {
                                    div { class: "text-center", Loading {} }
                                } else {
                                    div { class: "space-y-2",
                                        div { class: "flex justify-between",
                                            span { class: "text-base-content/70", "事件 ID" }
                                            span { class: "font-mono", "{d.event_id}" }
                                        }
                                        div { class: "flex justify-between",
                                            span { class: "text-base-content/70", "类型" }
                                            span { "{d.event_kind}" }
                                        }
                                        div { class: "flex justify-between",
                                            span { class: "text-base-content/70", "order_key" }
                                            span { class: "font-mono",
                                                if d.order_key.is_empty() {
                                                    "-"
                                                } else {
                                                    "{d.order_key}"
                                                }
                                            }
                                        }
                                        div { class: "flex justify-between",
                                            span { class: "text-base-content/70", "优先级" }
                                            span { "{d.priority}" }
                                        }
                                        div { class: "flex justify-between",
                                            span { class: "text-base-content/70", "创建时间" }
                                            span { "{format_created_at(d.created_at)}" }
                                        }
                                        div { class: "flex justify-between",
                                            span { class: "text-base-content/70", "状态" }
                                            span { class: "{status_badge_class(&d.status)}", "{d.status}" }
                                        }
                                        div { class: "mt-4",
                                            div { class: "text-base-content/70 text-sm mb-1", "内容预览" }
                                            pre { class: "font-mono text-sm",
                                                style: "background: var(--color-warm-ivory); padding: var(--space-3); border-radius: var(--radius-md); max-height: 300px; overflow: auto; white-space: pre-wrap; word-break: break-word;",
                                                "{d.payload_preview}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if *active_tab.read() == "stats" {
                    AopStatsPanel {}
                }
            }
        }
    }
}

/// AOP 统计面板（Tab 2 内容，5 秒轮询）
#[component]
fn AopStatsPanel() -> Element {
    let mut overview: Signal<Option<AopStatsOverviewResponse>> = use_signal(|| None);
    let mut time_series_points: Signal<Vec<AopStatsTimeSeriesPoint>> = use_signal(Vec::new);
    let mut consumer_dist: Signal<Vec<AopStatsDistributionItem>> = use_signal(Vec::new);
    let mut status_dist: Signal<Vec<AopStatsDistributionItem>> = use_signal(Vec::new);
    let mut last_updated: Signal<Option<String>> = use_signal(|| None);

    let load_data = move || {
        spawn(async move {
            let ov = get_aop_stats_overview().await;
            let ts =
                get_aop_stats_time_series(common::api::GetStatsTimeSeriesRequest::default()).await;
            let cd = get_aop_stats_distribution(common::api::GetStatsDistributionRequest {
                group_by: "consumer".to_string(),
                status: None,
            })
            .await;
            let sd = get_aop_stats_distribution(common::api::GetStatsDistributionRequest {
                group_by: "status".to_string(),
                status: None,
            })
            .await;

            if let Ok(o) = ov {
                overview.set(Some(o));
            }
            if let Ok(t) = ts {
                time_series_points.set(t.points);
            }
            if let Ok(c) = cd {
                consumer_dist.set(c.items);
            }
            if let Ok(s) = sd {
                status_dist.set(s.items);
            }
            // 记录最后更新时间
            let date_str = js_sys::Date::new_0().to_locale_string("zh-CN", &js_sys::Array::new());
            if let Some(s) = date_str.as_string() {
                last_updated.set(Some(s));
            }
        });
    };

    // 5 秒轮询：use_effect + spawn + loop + sleep_ms。
    // 用 Arc<AtomicBool> + use_drop 守卫轮询循环：组件卸载时置 false，
    // 避免 spawn 的 loop 在离开页面后永久运行（持续打请求 + 持有已卸载组件的信号）。
    let poll_running = Arc::new(AtomicBool::new(true));
    let poll_running_drop = poll_running.clone();
    use_effect(move || {
        let running = poll_running.clone();
        load_data();
        spawn(async move {
            loop {
                sleep_ms(5000).await;
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                load_data();
            }
        });
    });
    use_drop(move || {
        poll_running_drop.store(false, Ordering::SeqCst);
    });

    let ov = overview.read().clone();
    let avg_dur_str = ov
        .as_ref()
        .map(|o| format!("{:.0}", o.avg_duration_ms))
        .unwrap_or_default();
    let ts_points = time_series_points.read().clone();
    let cd_items = consumer_dist.read().clone();
    let sd_items = status_dist.read().clone();

    // 构造 LineChart 数据（Vec<TimeSeriesPoint>）
    let line_data: Vec<common::models::TimeSeriesPoint> = ts_points
        .iter()
        .map(|p| common::models::TimeSeriesPoint {
            interval_start: p.interval_start,
            tokens_input: 0,
            tokens_output: 0,
            call_count: p.call_count,
        })
        .collect();

    // 构造 status 分布 DonutChart 数据
    let status_slices: Vec<DonutSlice> = sd_items
        .iter()
        .map(|item| DonutSlice {
            label: item.label.clone(),
            value: item.value,
            color: aop_status_color(&item.label).to_string(),
        })
        .collect();

    // 构造 consumer 分布 DonutChart 数据
    let palette = [
        "#fa520f", "#10b981", "#3b82f6", "#f59e0b", "#8b5cf6", "#ec4899", "#14b8a6", "#6b7280",
    ];
    let consumer_slices: Vec<DonutSlice> = cd_items
        .iter()
        .enumerate()
        .map(|(i, item)| DonutSlice {
            label: item.label.clone(),
            value: item.value,
            color: palette[i % palette.len()].to_string(),
        })
        .collect();

    rsx! {
        div { class: "space-y-4",
            // 概览卡片
            if let Some(o) = &ov {
                StatGrid {
                    StatReadout { label: "总发布".to_string(), value: format!("{}", o.total_published), accent: Some("primary".to_string()) }
                    StatReadout { label: "总消费".to_string(), value: format!("{}", o.total_consumed), accent: Some("info".to_string()) }
                    StatReadout { label: "成功".to_string(), value: format!("{}", o.total_success), accent: Some("success".to_string()) }
                    StatReadout { label: "失败".to_string(), value: format!("{}", o.total_failed), accent: Some("error".to_string()) }
                    StatReadout { label: "平均耗时(ms)".to_string(), value: avg_dur_str, accent: Some("warning".to_string()) }
                }
            }

            // 时序折线图（最近 60 分钟，按分钟桶）
            if !line_data.is_empty() {
                HudPanel { signal: Some(true),
                    title: Some("事件趋势（最近 60 分钟，按分钟桶）".to_string()),
                    div { class: "card-body",
                        LineChart {
                            data: line_data,
                            width: Some(800.0),
                            height: Some(220.0),
                            title: Some("事件数量".to_string()),
                            value_label: Some("次数".to_string()),
                        }
                    }
                }
            }

            // 分布环形图（双列）
            div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                if !status_slices.is_empty() {
                    HudPanel { signal: Some(true),
                        title: Some("状态分布".to_string()),
                        div { class: "card-body",
                            DonutChart {
                                data: status_slices,
                                width: Some(240.0),
                                height: Some(240.0),
                                center_label: Some("事件数".to_string()),
                            }
                        }
                    }
                }
                if !consumer_slices.is_empty() {
                    HudPanel { signal: Some(true),
                        title: Some("消费者分布".to_string()),
                        div { class: "card-body",
                            DonutChart {
                                data: consumer_slices,
                                width: Some(240.0),
                                height: Some(240.0),
                                center_label: Some("事件数".to_string()),
                            }
                        }
                    }
                }
            }

            // 最后更新时间
            if let Some(t) = last_updated.read().as_ref() {
                div { class: "text-xs text-base-content/50 text-right",
                    "最后更新: {t}（5 秒自动刷新）"
                }
            }
        }
    }
}

/// AOP 事件状态对应的 HUD 风格颜色
fn aop_status_color(status: &str) -> &'static str {
    match status {
        "published" | "published_sync" => "#3b82f6",
        "consuming" => "#f59e0b",
        "success" => "#10b981",
        "failed" => "#ef4444",
        _ => "#6b7280",
    }
}

/// wasm 环境的 sleep（基于 js_sys::Promise + setTimeout，参考 components/toast.rs）
async fn sleep_ms(ms: u32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms as i32)
            .unwrap();
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}
