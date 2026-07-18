//! 日志查询页面 - 按关键词 / log_id / 级别 / 时间范围过滤，支持分页与调用链追踪

use dioxus::prelude::*;

use chrono::{Local, NaiveDateTime, TimeZone};

use crate::api::system::{query_logs, LogEntry, LogPageResult, LogQueryParams};
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;

/// 默认每页条数
const DEFAULT_PAGE_SIZE: usize = 20;

/// 根据日志级别返回 badge 样式类
fn level_badge_class(level: &str) -> &'static str {
    match level.to_uppercase().as_str() {
        "ERROR" => "badge badge-error",
        "WARN" => "badge badge-warning",
        "INFO" => "badge badge-info",
        "DEBUG" => "badge badge-neutral",
        "TRACE" => "badge badge-secondary",
        _ => "badge badge-neutral",
    }
}

/// 将 ISO8601 时间戳格式化为 "YYYY-MM-DD HH:MM:SS"（解析失败时原样返回）
fn format_timestamp(ts: &str) -> String {
    if ts.is_empty() {
        return "-".to_string();
    }
    // chrono 解析 RFC3339 / ISO8601
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        let local = dt.with_timezone(&Local);
        return local.format("%Y-%m-%d %H:%M:%S").to_string();
    }
    // 退而求其次：尝试直接截断到秒
    if ts.len() >= 19 {
        return ts[..19].replace('T', " ");
    }
    ts.to_string()
}

/// 把 datetime-local 输入值（YYYY-MM-DDTHH:MM）解析为 unix 毫秒
/// 输入视为本地时间，转换为 UTC 毫秒
fn parse_datetime_local_to_ms(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let ndt = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M").ok()?;
    let local_dt = Local.from_local_datetime(&ndt).single()?;
    Some(local_dt.timestamp_millis())
}

/// 把日志条目原始 JSON 美化为可读字符串
fn pretty_raw(raw: &Option<serde_json::Value>) -> String {
    match raw {
        Some(v) => serde_json::to_string_pretty(v).unwrap_or_else(|_| v.to_string()),
        None => String::new(),
    }
}

#[component]
pub fn SystemLogs() -> Element {
    let toast = use_toast();

    // 表单状态
    let mut form_keyword = use_signal(String::new);
    let mut form_log_id = use_signal(String::new);
    let mut form_level = use_signal(|| String::new()); // 空字符串表示"全部"
    let mut form_start = use_signal(String::new);
    let mut form_end = use_signal(String::new);

    // 当前生效的查询（用于触发刷新 / 翻页）
    let mut active_query = use_signal(|| Option::<LogQueryParams>::None);
    let mut current_page = use_signal(|| 1usize);

    // 结果
    let result = use_signal(|| Option::<LogPageResult>::None);
    let loading = use_signal(|| false);

    // 展开的日志行索引（按 entries 中的位置）
    let mut expanded = use_signal(|| std::collections::HashSet::<usize>::new());

    /// 执行一次查询
    fn do_query(
        params: LogQueryParams,
        page: usize,
        mut loading: Signal<bool>,
        mut result: Signal<Option<LogPageResult>>,
        toast: crate::store::toast::ToastState,
    ) {
        loading.set(true);
        spawn(async move {
            let mut p = params.clone();
            p.page = page;
            match query_logs(&p).await {
                Ok(r) => result.set(Some(r)),
                Err(e) => toast.error(&format!("查询日志失败: {}", e)),
            }
            loading.set(false);
        });
    }

    // 首次进入时自动加载第一页
    use_effect(move || {
        if active_query().is_none() {
            let p = LogQueryParams {
                page: 1,
                page_size: DEFAULT_PAGE_SIZE,
                ..Default::default()
            };
            active_query.set(Some(p.clone()));
            current_page.set(1);
            do_query(p, 1, loading, result, toast);
        }
    });

    // 点击查询按钮
    let mut handle_search = move |_| {
        let params = LogQueryParams {
            keyword: if form_keyword().trim().is_empty() {
                None
            } else {
                Some(form_keyword().trim().to_string())
            },
            log_id: if form_log_id().trim().is_empty() {
                None
            } else {
                Some(form_log_id().trim().to_string())
            },
            level: if form_level().is_empty() {
                None
            } else {
                Some(form_level().clone())
            },
            start_time: parse_datetime_local_to_ms(&form_start()),
            end_time: parse_datetime_local_to_ms(&form_end()),
            page: 1,
            page_size: DEFAULT_PAGE_SIZE,
        };
        active_query.set(Some(params.clone()));
        current_page.set(1);
        expanded.set(std::collections::HashSet::new());
        do_query(params, 1, loading, result, toast);
    };

    // 重置表单
    let handle_reset = move |_| {
        form_keyword.set(String::new());
        form_log_id.set(String::new());
        form_level.set(String::new());
        form_start.set(String::new());
        form_end.set(String::new());
    };

    // 翻页
    let go_prev = move |_| {
        let cur = current_page();
        if cur <= 1 {
            return;
        }
        let new_page = cur - 1;
        if let Some(q) = active_query() {
            current_page.set(new_page);
            expanded.set(std::collections::HashSet::new());
            do_query(q, new_page, loading, result, toast);
        }
    };

    let go_next = move |_| {
        let cur = current_page();
        let total_pages = total_pages(result());
        if cur >= total_pages {
            return;
        }
        let new_page = cur + 1;
        if let Some(q) = active_query() {
            current_page.set(new_page);
            expanded.set(std::collections::HashSet::new());
            do_query(q, new_page, loading, result, toast);
        }
    };

    // 点击 log_id 触发新查询（调用链追踪）
    let mut on_click_log_id = move |id: String| {
        form_log_id.set(id.clone());
        form_keyword.set(String::new());
        form_level.set(String::new());
        form_start.set(String::new());
        form_end.set(String::new());
        let params = LogQueryParams {
            keyword: None,
            log_id: Some(id),
            level: None,
            start_time: None,
            end_time: None,
            page: 1,
            page_size: DEFAULT_PAGE_SIZE,
        };
        active_query.set(Some(params.clone()));
        current_page.set(1);
        expanded.set(std::collections::HashSet::new());
        do_query(params, 1, loading, result, toast);
    };

    let res_opt = result();
    let entries: Vec<LogEntry> = res_opt
        .as_ref()
        .map(|r| r.entries.clone())
        .unwrap_or_default();
    let total = res_opt.as_ref().map(|r| r.total).unwrap_or(0);
    let page = res_opt.as_ref().map(|r| r.page).unwrap_or(1);
    let total_pages = total_pages(res_opt.clone());
    let cur_page = current_page();
    let expanded_set = expanded.read().clone();

    rsx! {
        AppLayout {
            div { class: "card",
                div { class: "card-header",
                    h2 { class: "card-title", "日志查询" }
                    div { class: "page-header-actions",
                        button {
                            class: "btn btn-ghost btn-sm",
                            onclick: move |_| {
                                if let Some(q) = active_query() {
                                    do_query(q, current_page(), loading, result, toast);
                                }
                            },
                            "🔄 刷新"
                        }
                    }
                }

                // 查询表单
                div { class: "filter-row",
                    div { class: "filter-item",
                        label { class: "form-label", "关键词" }
                        input {
                            class: "form-input",
                            value: "{form_keyword}",
                            placeholder: "message 包含的关键词",
                            oninput: move |e| form_keyword.set(e.value()),
                            onkeydown: move |evt| {
                                if evt.key() == Key::Enter {
                                    handle_search(());
                                }
                            }
                        }
                    }
                    div { class: "filter-item",
                        label { class: "form-label", "Log ID（调用链）" }
                        input {
                            class: "form-input text-mono",
                            value: "{form_log_id}",
                            placeholder: "精确匹配 log_id",
                            oninput: move |e| form_log_id.set(e.value()),
                            onkeydown: move |evt| {
                                if evt.key() == Key::Enter {
                                    handle_search(());
                                }
                            }
                        }
                    }
                    div { class: "filter-item",
                        label { class: "form-label", "级别" }
                        select {
                            class: "form-select",
                            value: "{form_level}",
                            onchange: move |e| form_level.set(e.value()),
                            option { value: "", "全部" }
                            option { value: "INFO", "INFO" }
                            option { value: "WARN", "WARN" }
                            option { value: "ERROR", "ERROR" }
                            option { value: "DEBUG", "DEBUG" }
                        }
                    }
                    div { class: "filter-item",
                        label { class: "form-label", "开始时间" }
                        input {
                            class: "form-input",
                            r#type: "datetime-local",
                            value: "{form_start}",
                            oninput: move |e| form_start.set(e.value())
                        }
                    }
                    div { class: "filter-item",
                        label { class: "form-label", "结束时间" }
                        input {
                            class: "form-input",
                            r#type: "datetime-local",
                            value: "{form_end}",
                            oninput: move |e| form_end.set(e.value())
                        }
                    }
                }
                div { class: "filter-row",
                    div { class: "page-header-actions",
                        button {
                            class: "btn btn-accent",
                            onclick: move |_| handle_search(()),
                            "🔍 查询"
                        }
                        button {
                            class: "btn btn-ghost",
                            onclick: handle_reset,
                            "清空"
                        }
                    }
                }

                // 结果区
                if loading() {
                    Loading {}
                } else if entries.is_empty() {
                    EmptyState { icon: "📋".to_string(), message: "没有匹配的日志".to_string() }
                } else {
                    div { style: "margin-top: var(--space-4);",
                        table { class: "table",
                            thead { tr {
                                th { "时间" }
                                th { "级别" }
                                th { "Log ID" }
                                th { "操作" }
                                th { "消息" }
                            }}
                            tbody {
                                for (idx, entry) in entries.iter().enumerate() {
                                    {
                                        let ts = entry.timestamp.clone();
                                        let level = entry.level.clone();
                                        let log_id = entry.log_id.clone();
                                        let operation = entry.operation.clone().unwrap_or_default();
                                        let message = entry.message.clone();
                                        let raw_pretty = pretty_raw(&entry.raw);
                                        let is_expanded = expanded_set.contains(&idx);
                                        let log_id_for_click = log_id.clone();

                                        rsx! {
                                            tr { key: "{idx}",
                                                td { class: "text-mono text-muted", style: "white-space: nowrap;", "data-label": "时间",
                                                    "{format_timestamp(&ts)}"
                                                }
                                                td { "data-label": "级别",
                                                    span { class: "{level_badge_class(&level)}", "{level}" }
                                                }
                                                td { "data-label": "Log ID",
                                                    match log_id.as_deref() {
                                                        Some(id) if !id.is_empty() => rsx! {
                                                            span {
                                                                class: "text-mono",
                                                                style: "color: var(--color-mistral-orange); cursor: pointer; text-decoration: underline dotted;",
                                                                title: "点击按 log_id 查询",
                                                                onclick: move |_| {
                                                                    on_click_log_id(log_id_for_click.clone().unwrap_or_default());
                                                                },
                                                                "{id}"
                                                            }
                                                        },
                                                        _ => rsx! { span { class: "text-muted", "-" } },
                                                    }
                                                }
                                                td { class: "text-mono", "data-label": "操作",
                                                    if operation.is_empty() {
                                                        span { class: "text-muted", "-" }
                                                    } else {
                                                        "{operation}"
                                                    }
                                                }
                                                td { "data-label": "消息",
                                                    div {
                                                        style: "cursor: pointer; max-width: 480px; word-break: break-word;",
                                                        onclick: move |_| {
                                                            let mut s = expanded.write();
                                                            if s.contains(&idx) {
                                                                s.remove(&idx);
                                                            } else {
                                                                s.insert(idx);
                                                            }
                                                        },
                                                        "{message}"
                                                    }
                                                    if is_expanded && !raw_pretty.is_empty() {
                                                        pre {
                                                            class: "text-mono",
                                                            style: "margin-top: var(--space-2); padding: var(--space-2); background: var(--color-warm-ivory); border-radius: var(--radius-md); max-height: 240px; overflow: auto; white-space: pre-wrap; word-break: break-word;",
                                                            "{raw_pretty}"
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

                    // 分页
                    div {
                        style: "display: flex; justify-content: space-between; align-items: center; margin-top: var(--space-4);",
                        div { class: "text-muted",
                            "共 {total} 条 · 第 {cur_page} / {total_pages} 页"
                        }
                        div { class: "page-header-actions",
                            button {
                                class: "btn btn-ghost btn-sm",
                                disabled: cur_page <= 1,
                                onclick: go_prev,
                                "上一页"
                            }
                            button {
                                class: "btn btn-ghost btn-sm",
                                disabled: cur_page >= total_pages,
                                onclick: go_next,
                                "下一页"
                            }
                        }
                    }
                    div { class: "form-hint",
                        "提示：点击 Log ID 可发起调用链追踪；点击消息可展开原始 JSON。当前页码: {page}"
                    }
                }
            }
        }
    }
}

/// 根据总数与 page_size 计算总页数（至少为 1）
fn total_pages(res: Option<LogPageResult>) -> usize {
    match res {
        Some(r) => {
            let ps = if r.page_size == 0 { DEFAULT_PAGE_SIZE } else { r.page_size };
            (r.total + ps - 1) / ps.max(1)
        }
        None => 1,
    }
}
