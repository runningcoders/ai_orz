//! 缓存管理页 - 进程内 TTL 缓存的查询与治理
//!
//! 与「日志查询 / AOP 监控」同属系统管理面：只读统计 + 清理动作，
//! 不提供按值读取（值可能含敏感信息）。清空后下一次业务请求会重新取数。

use dioxus::prelude::*;

use common::api::{CacheStatsItem, GetCachesResponse};

use crate::api::cache::{clear_cache, get_caches, prune_cache};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::hud::{HudPanel, PageHeader};
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;

/// 自动刷新间隔（毫秒）
const AUTO_REFRESH_MS: u32 = 15_000;

#[component]
pub fn SystemCache() -> Element {
    let toast = use_toast();
    let mut data = use_signal(|| Option::<GetCachesResponse>::None);
    let mut loading = use_signal(|| false);
    let mut busy = use_signal(|| Option::<String>::None);
    // 清空是破坏性操作（会让下一次请求集中回源），二次确认后才执行
    let mut pending_clear = use_signal(|| Option::<String>::None);

    // 拉取概览：手动刷新与自动刷新共用；上一轮未结束时直接跳过，避免请求堆叠
    let refresh = Callback::new(move |_: ()| {
        if *loading.peek() {
            return;
        }
        loading.set(true);
        spawn(async move {
            match get_caches().await {
                Ok(resp) => data.set(Some(resp)),
                Err(e) => toast.error(format!("加载缓存概览失败：{e}")),
            }
            loading.set(false);
        });
    });

    // 清理动作：clear / prune 共用一个入口，按缓存名串行
    let do_action = Callback::new(move |(name, clear): (String, bool)| {
        if busy.read().is_some() {
            return;
        }
        busy.set(Some(name.clone()));
        spawn(async move {
            let result = if clear {
                clear_cache(&name).await.map(|resp| resp.removed)
            } else {
                prune_cache(&name).await.map(|resp| resp.removed)
            };
            busy.set(None);
            match result {
                Ok(removed) => {
                    let action = if clear { "清空" } else { "清扫" };
                    toast.success(format!("{name} {action}完成：移除 {removed} 条"));
                    refresh.call(());
                }
                Err(e) => toast.error(format!("{name} 操作失败：{e}")),
            }
        });
    });
    let on_prune = Callback::new(move |name: String| do_action.call((name, false)));
    let mut pending_clear_sig = pending_clear;
    let on_clear = Callback::new(move |name: String| pending_clear_sig.set(Some(name)));

    // 首帧加载 + 页面可见时自动刷新
    use_future(move || async move {
        refresh.call(());
        loop {
            gloo_timers::future::TimeoutFuture::new(AUTO_REFRESH_MS).await;
            if !crate::utils::page_hidden() {
                refresh.call(());
            }
        }
    });

    let snapshot = data();
    let totals = snapshot.as_ref().map(summarize).unwrap_or_default();

    rsx! {
        AppLayout {
            div { class: "space-y-4",
                PageHeader {
                    eyebrow: Some("SYSTEM / CACHE".to_string()),
                    title: "缓存管理".to_string(),
                    actions: Some(rsx! {
                        button {
                            class: "btn hud-btn btn-ghost btn-sm",
                            onclick: move |_| refresh.call(()),
                            "🔄 刷新"
                        }
                    }),
                }

                div { class: "text-xs text-base-content/60 -mt-2",
                    "进程内 TTL 缓存。读取时懒过期，可手动清扫过期条目；清空后下一次业务请求会重新取数。"
                    " 每 {AUTO_REFRESH_MS / 1000} 秒自动刷新。"
                }

                HudPanel {
                    title: Some("已注册缓存".to_string()),
                    signal: Some(true),
                    actions: Some(rsx! {
                        span { class: "text-xs font-mono text-base-content/60",
                            "缓存 {totals.caches} · 条目 {totals.entries} · 过期 {totals.expired} · 命中率 {totals.hit_rate}"
                        }
                    }),

                    if snapshot.is_none() {
                        Loading {}
                    } else if let Some(resp) = snapshot {
                        if resp.caches.is_empty() {
                            EmptyState {
                                icon: "📦".to_string(),
                                message: "还没有注册任何缓存".to_string(),
                            }
                        } else {
                            table { class: "table hud-table table-zebra",
                                thead {
                                    tr {
                                        th { "缓存" }
                                        th { "TTL" }
                                        th { "条目" }
                                        th { "过期" }
                                        th { "命中率" }
                                        th { "命中 / 未命中" }
                                        th { "过期 / 写入 / 失效" }
                                        th { "操作" }
                                    }
                                }
                                tbody {
                                    for item in resp.caches.iter() {
                                        CacheRow {
                                            item: item.clone(),
                                            busy,
                                            on_prune,
                                            on_clear,
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                ConfirmDialog {
                    show: pending_clear.read().is_some(),
                    title: "确认清空缓存".to_string(),
                    message: format!(
                        "确定清空 {} 的全部条目？清空后下一次业务请求会重新取数。",
                        pending_clear.read().as_deref().unwrap_or("该缓存")
                    ),
                    confirm_text: Some("清空".to_string()),
                    on_confirm: move |_| {
                        let name = pending_clear.read().clone();
                        pending_clear.set(None);
                        if let Some(name) = name {
                            do_action.call((name, true));
                        }
                    },
                    on_cancel: move |_| pending_clear.set(None),
                }
            }
        }
    }
}

/// 单行缓存：键用胶囊展示（最多 6 个），操作按钮按需禁用
#[component]
fn CacheRow(
    item: CacheStatsItem,
    busy: Signal<Option<String>>,
    on_prune: Callback<String>,
    on_clear: Callback<String>,
) -> Element {
    let acting = busy.read().is_some();
    let prune_name = item.name.clone();
    let clear_name = item.name.clone();

    rsx! {
        tr {
            td {
                div { class: "font-mono text-sm", "{item.name}" }
                div { class: "flex flex-wrap gap-1 mt-1",
                    for key in item.keys.iter().take(6) {
                        span { class: "badge badge-ghost badge-sm font-mono", "{key}" }
                    }
                    if item.keys.len() > 6 {
                        span { class: "badge badge-ghost badge-sm", "+{item.keys.len() - 6}" }
                    }
                }
            }
            td { class: "font-mono text-sm", "{format_ttl(item.ttl_ms)}" }
            td { class: "font-mono", "{item.entries}" }
            td { class: "font-mono",
                if item.expired_entries > 0 {
                    span { class: "text-warning", "{item.expired_entries}" }
                } else {
                    "0"
                }
            }
            td { class: "font-mono", "{hit_rate(&item)}" }
            td { class: "font-mono text-xs", "{item.hits} / {item.misses}" }
            td { class: "font-mono text-xs",
                "{item.expirations} / {item.inserts} / {item.invalidations}"
            }
            td {
                div { class: "flex items-center gap-1",
                    button {
                        class: "btn btn-xs hud-btn btn-ghost",
                        disabled: acting || item.expired_entries == 0,
                        onclick: move |_| on_prune.call(prune_name.clone()),
                        "清扫"
                    }
                    button {
                        class: "btn btn-xs hud-btn btn-ghost text-error",
                        disabled: acting,
                        onclick: move |_| on_clear.call(clear_name.clone()),
                        "清空"
                    }
                }
            }
        }
    }
}

/// 概览合计
#[derive(Default)]
struct Totals {
    caches: usize,
    entries: u64,
    expired: u64,
    hit_rate: String,
}

fn summarize(resp: &GetCachesResponse) -> Totals {
    let entries = resp.caches.iter().map(|cache| cache.entries).sum();
    let expired = resp.caches.iter().map(|cache| cache.expired_entries).sum();
    let hits: u64 = resp.caches.iter().map(|cache| cache.hits).sum();
    let misses: u64 = resp.caches.iter().map(|cache| cache.misses).sum();
    Totals {
        caches: resp.caches.len(),
        entries,
        expired,
        hit_rate: percent(hits, misses),
    }
}

fn hit_rate(item: &CacheStatsItem) -> String {
    percent(item.hits, item.misses)
}

fn percent(hits: u64, misses: u64) -> String {
    let total = hits + misses;
    if total == 0 {
        "—".to_string()
    } else {
        format!("{:.1}%", hits as f64 * 100.0 / total as f64)
    }
}

fn format_ttl(ms: u64) -> String {
    if ms >= 60_000 && ms.is_multiple_of(60_000) {
        format!("{}m", ms / 60_000)
    } else if ms >= 1_000 && ms.is_multiple_of(1_000) {
        format!("{}s", ms / 1_000)
    } else {
        format!("{ms}ms")
    }
}
