//! AOP 队列监控页面

use dioxus::prelude::*;

use crate::api::system::{
    get_all_queue_stats, get_event, list_events, EventDetailResponse, EventSummaryResponse,
    QueueStatsResponse,
};
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
        Some(d) => d.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string(),
        None => ts.to_string(),
    }
}

#[component]
pub fn SystemAop() -> Element {
    let toast = use_toast();

    let mut stats = use_signal(Vec::<QueueStatsResponse>::new);
    let mut loading_stats = use_signal(|| false);

    let mut selected_consumer = use_signal(|| Option::<String>::None);
    let mut events = use_signal(Vec::<EventSummaryResponse>::new);
    let mut loading_events = use_signal(|| false);

    let mut selected_event = use_signal(|| Option::<EventDetailResponse>::None);
    let mut loading_detail = use_signal(|| false);

    let mut filter_status = use_signal(|| String::new());

    use_effect(move || {
        loading_stats.set(true);
        spawn(async move {
            match get_all_queue_stats().await {
                Ok(data) => stats.set(data),
                Err(e) => toast.error(&format!("加载队列统计失败: {}", e)),
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

    rsx! {
        AppLayout {
            div { class: "card",
                div { class: "card-header",
                    h2 { class: "card-title", "AOP 队列监控" }
                    div { class: "page-header-actions",
                        button {
                            class: "btn btn-ghost btn-sm",
                            onclick: move |_| {
                                loading_stats.set(true);
                                spawn(async move {
                                    match get_all_queue_stats().await {
                                        Ok(data) => stats.set(data),
                                        Err(e) => toast.error(&format!("加载队列统计失败: {}", e)),
                                    }
                                    loading_stats.set(false);
                                });
                            },
                            "🔄 刷新"
                        }
                    }
                }

                p { class: "text-secondary mb-4",
                    "查看 AOP 事件中心各消费者队列的运行状态、事件堆积情况和内容排查。"
                }

                if loading_stats() {
                    Loading {}
                } else if stats_data.is_empty() {
                    EmptyState { icon: "📭".to_string(), message: "暂无队列数据".to_string() }
                } else {
                    div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                        for s in stats_data.iter() {
                            {
                                let name = s.consumer_name.clone();
                                let pending = s.pending_count;
                                let in_progress = s.in_progress_count;
                                let oldest = s.oldest_event_age_secs;
                                let order_keys = s.order_keys.clone();
                                let is_selected = consumer.as_ref() == Some(&name);
                                let card_name = name.clone();
                                let card_class = if is_selected {
                                    "card card-hover card-selected"
                                } else {
                                    "card card-hover"
                                };
                                rsx! {
                                    div {
                                        class: "{card_class}",
                                        style: "cursor: pointer;",
                                        onclick: move |_| {
                                            let consumer_name = card_name.clone();
                                            selected_consumer.set(Some(consumer_name.clone()));
                                            selected_event.set(None);
                                            let status = filter_status.cloned();
                                            loading_events.set(true);
                                            spawn(async move {
                                                let status_ref = if status.is_empty() { None } else { Some(status.as_str()) };
                                                match list_events(&consumer_name, None, status_ref, 100, 0).await {
                                                    Ok(data) => events.set(data),
                                                    Err(e) => toast.error(&format!("加载事件列表失败: {}", e)),
                                                }
                                                loading_events.set(false);
                                            });
                                        },
                                        div { class: "flex justify-between items-start",
                                            h3 { class: "font-semibold", "{name}" }
                                            if in_progress > 0 {
                                                span { class: "badge badge-warning", "处理中: {in_progress}" }
                                            }
                                        }
                                        div { class: "mt-2",
                                            div { class: "text-2xl font-bold", "{pending}" }
                                            div { class: "text-sm text-muted", "待处理事件" }
                                        }
                                        if let Some(age) = oldest {
                                            div { class: "text-xs text-muted mt-1",
                                                "最老事件: {age} 秒前"
                                            }
                                        }
                                        if !order_keys.is_empty() {
                                            div { class: "mt-2 text-xs",
                                                for ok in order_keys.iter().take(3) {
                                                    span { class: "badge badge-neutral mr-1",
                                                        "{ok.order_key}: {ok.pending_count}"
                                                    }
                                                }
                                                if order_keys.len() > 3 {
                                                    span { class: "text-muted", "+{order_keys.len() - 3} 更多" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(consumer_name) = consumer.clone() {
                    div { class: "card mt-6",
                        div { class: "card-header",
                            h3 { class: "card-title", "事件列表 · {consumer_name}" }
                            div { class: "page-header-actions",
                                select {
                                    class: "form-select form-select-sm",
                                    value: "{status_filter_val}",
                                    onchange: move |e| {
                                        filter_status.set(e.value());
                                        let c = consumer_name.clone();
                                        let s = e.value();
                                        loading_events.set(true);
                                        selected_event.set(None);
                                        spawn(async move {
                                            let status_ref = if s.is_empty() { None } else { Some(s.as_str()) };
                                            match list_events(&c, None, status_ref, 100, 0).await {
                                                Ok(data) => events.set(data),
                                                Err(e) => toast.error(&format!("加载事件列表失败: {}", e)),
                                            }
                                            loading_events.set(false);
                                        });
                                    },
                                    option { value: "", "全部状态" }
                                    option { value: "pending", "待处理" }
                                    option { value: "processing", "处理中" }
                                }
                            }
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
                                                                match get_event(&c, &eid).await {
                                                                    Ok(data) => selected_event.set(Some(data)),
                                                                    Err(e) => toast.error(&format!("加载事件详情失败: {}", e)),
                                                                }
                                                                loading_detail.set(false);
                                                            });
                                                        },
                                                        td { class: "text-mono text-sm",
                                                            "{event_id}"
                                                        }
                                                        td { "{event_kind}" }
                                                        td { class: "text-mono text-sm",
                                                            if order_key.is_empty() {
                                                                span { class: "text-muted", "-" }
                                                            } else {
                                                                "{order_key}"
                                                            }
                                                        }
                                                        td { "{priority}" }
                                                        td { class: "text-muted text-sm",
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
                                        span { class: "text-muted", "事件 ID" }
                                        span { class: "text-mono", "{d.event_id}" }
                                    }
                                    div { class: "flex justify-between",
                                        span { class: "text-muted", "类型" }
                                        span { "{d.event_kind}" }
                                    }
                                    div { class: "flex justify-between",
                                        span { class: "text-muted", "order_key" }
                                        span { class: "text-mono",
                                            if d.order_key.is_empty() {
                                                "-"
                                            } else {
                                                "{d.order_key}"
                                            }
                                        }
                                    }
                                    div { class: "flex justify-between",
                                        span { class: "text-muted", "优先级" }
                                        span { "{d.priority}" }
                                    }
                                    div { class: "flex justify-between",
                                        span { class: "text-muted", "创建时间" }
                                        span { "{format_created_at(d.created_at)}" }
                                    }
                                    div { class: "flex justify-between",
                                        span { class: "text-muted", "状态" }
                                        span { class: "{status_badge_class(&d.status)}", "{d.status}" }
                                    }
                                    div { class: "mt-4",
                                        div { class: "text-muted text-sm mb-1", "内容预览" }
                                        pre { class: "text-mono text-sm",
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
        }
    }
}
