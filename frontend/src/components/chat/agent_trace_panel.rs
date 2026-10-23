//! Agent 轨迹面板：思考轨迹 + 记忆轨迹 主从联动
//!
//! 思考轨迹与记忆轨迹在 awaken / settle / compact 场景下共享同一个 `run_id`
//! （`run_id == MemoryTrace.id`），因此按下述形态组织：
//!
//! - 宽屏：左侧 run 列表（分页），右侧上段记忆条目（input / output / metadata）、
//!   下段思考轮次时间线；
//! - 窄屏：退化为单列，记忆段天然位于轮次时间线上方。
//!
//! 记忆段仅在 awaken / settle / compact 三类场景渲染——其余场景的 run 不落记忆条目。

use dioxus::prelude::*;

use crate::api::hr::query_think_traces;
use crate::components::state::Loading;
use crate::store::toast::use_toast;
use crate::utils::format_timestamp_opt;
use common::api::{PaginationParams, QueryThinkTracesRequest, ThinkTraceListItem};

use super::memory_trace_detail::MemoryTraceDetail;
use super::think_rounds_timeline::ThinkRoundsTimeline;
use super::think_traces_tab::{page_bounds, scene_label};

/// 每页条数（JSONL 扫描开销控制）
const PAGE_SIZE: usize = 20;

/// 这些场景的 run_id 与记忆条目 id 同值，可回查记忆 input / output
fn has_memory_entry(scene: &str) -> bool {
    matches!(scene, "awaken" | "settle" | "compact")
}

/// Agent 轨迹面板
#[component]
pub fn AgentTracePanel(agent_id: String) -> Element {
    let toast = use_toast();
    let mut entries = use_signal(Vec::<ThinkTraceListItem>::new);
    let mut total = use_signal(|| 0usize);
    let mut offset = use_signal(|| 0usize);
    let mut loading = use_signal(|| false);
    let mut loaded = use_signal(|| false);
    let mut selected_run_id = use_signal(|| None::<String>);
    // 加载代际计数：快速切换 Agent / 翻页时丢弃过期响应
    let mut load_gen = use_signal(|| 0u64);
    let mut reload_tick = use_signal(|| 0u64);
    let mut prev_scope = use_signal(String::new);

    let mut load = move |aid: String, page_offset: usize| {
        let my_gen = load_gen() + 1;
        load_gen.set(my_gen);
        loading.set(true);
        spawn(async move {
            let req = QueryThinkTracesRequest {
                agent_id: Some(aid),
                pagination: PaginationParams {
                    limit: Some(PAGE_SIZE),
                    offset: Some(page_offset),
                },
                ..Default::default()
            };
            match query_think_traces(&req).await {
                Ok(page) => {
                    if load_gen() != my_gen {
                        return;
                    }
                    // 选中项仍在当前页则保留，否则回落到首条
                    let current = selected_run_id();
                    let still_present = current
                        .as_ref()
                        .is_some_and(|id| page.items.iter().any(|i| &i.run_id == id));
                    if !still_present {
                        selected_run_id.set(page.items.first().map(|i| i.run_id.clone()));
                    }
                    entries.set(page.items);
                    total.set(page.total);
                }
                Err(e) => {
                    if load_gen() != my_gen {
                        return;
                    }
                    toast.error(format!("加载思考轨迹失败: {}", e));
                }
            }
            loading.set(false);
            loaded.set(true);
        });
    };

    let aid_effect = agent_id.clone();
    // Agent 切换 / 手动刷新 → 重置分页与选中并加载
    use_effect(move || {
        let scope = format!("{}|{}", aid_effect, reload_tick());
        if prev_scope() == scope {
            return;
        }
        prev_scope.set(scope);
        offset.set(0);
        selected_run_id.set(None);
        load(aid_effect.clone(), 0);
    });

    let list = entries.read().clone();
    let total_now = total();
    let offset_now = offset();
    let selected_run_id_now = selected_run_id();
    let selected = list
        .iter()
        .find(|e| Some(&e.run_id) == selected_run_id_now.as_ref())
        .cloned();
    let (start_no, end_no) = page_bounds(offset_now, list.len(), total_now);
    let page_no = offset_now / PAGE_SIZE + 1;
    let has_prev = offset_now > 0;
    let has_next = offset_now + list.len() < total_now;

    let aid_page = agent_id.clone();

    rsx! {
        div { class: "grid grid-cols-1 lg:grid-cols-[20rem_minmax(0,1fr)] gap-2",
            // ===== 左：run 列表 =====
            div { class: "space-y-1",
                div { class: "flex items-center justify-between",
                    span { class: "text-xs text-base-content/60", "共 {total_now} 条思考轨迹" }
                    button {
                        class: "btn hud-btn btn-ghost btn-xs",
                        title: "刷新",
                        disabled: loading(),
                        onclick: move |_| reload_tick += 1,
                        "⟳"
                    }
                }

                if loading() && !loaded() {
                    div { class: "flex items-center justify-center py-12",
                        Loading { size: "md" }
                        span { class: "ml-2 text-sm text-base-content/60", "加载中..." }
                    }
                } else if list.is_empty() {
                    div { class: "text-center py-12 text-base-content/60 text-sm", "暂无思考轨迹" }
                } else {
                    div { class: "space-y-1",
                        for e in list.iter() {
                            {
                                let run_id = e.run_id.clone();
                                let select_run_id = e.run_id.clone();
                                let scene = e.scene.clone();
                                let steps = e.steps;
                                let advisor_calls = e.advisor_calls;
                                let created = format_timestamp_opt(Some(e.created_at as i64));
                                let is_selected = selected_run_id_now.as_ref() == Some(&run_id);
                                rsx! {
                                    button {
                                        key: "{run_id}",
                                        class: if is_selected {
                                            "w-full text-left rounded-lg border border-primary bg-primary/10 p-2 space-y-1"
                                        } else {
                                            "w-full text-left rounded-lg border border-base-300 bg-base-100 p-2 space-y-1 hover:bg-base-200"
                                        },
                                        onclick: move |_| selected_run_id.set(Some(select_run_id.clone())),
                                        div { class: "flex items-center gap-2 flex-wrap",
                                            span { class: "badge orz-tag badge-sm", "{scene_label(&scene)}" }
                                            if has_memory_entry(&scene) {
                                                span {
                                                    class: "badge badge-xs badge-info badge-soft",
                                                    title: "该 run 同时存在记忆条目",
                                                    "🧾"
                                                }
                                            }
                                            span { class: "text-[11px] text-base-content/60 flex-1 text-right",
                                                "{steps} 步 · advisor {advisor_calls}"
                                            }
                                        }
                                        div { class: "text-[11px] text-base-content/50", "{created}" }
                                        div { class: "text-[10px] text-base-content/30 font-mono truncate",
                                            title: "{run_id}",
                                            "{run_id}"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    div { class: "flex items-center justify-between pt-1",
                        span { class: "text-xs text-base-content/60",
                            "第 {page_no} 页 · {start_no}-{end_no} 条"
                        }
                        div { class: "flex gap-1",
                            button {
                                class: "btn hud-btn btn-ghost btn-xs",
                                disabled: !has_prev || loading(),
                                onclick: {
                                    let aid = aid_page.clone();
                                    move |_| {
                                        let next = offset().saturating_sub(PAGE_SIZE);
                                        offset.set(next);
                                        load(aid.clone(), next);
                                    }
                                },
                                "上一页"
                            }
                            button {
                                class: "btn hud-btn btn-ghost btn-xs",
                                disabled: !has_next || loading(),
                                onclick: {
                                    let aid = aid_page.clone();
                                    move |_| {
                                        let next = offset() + PAGE_SIZE;
                                        offset.set(next);
                                        load(aid.clone(), next);
                                    }
                                },
                                "下一页"
                            }
                        }
                    }
                }
            }

            // ===== 右：选中 run 的记忆段 + 轮次时间线 =====
            div { class: "space-y-2",
                if let Some(sel) = selected {
                    // 记忆段在上：与下段轮次时间线同源于同一个 run_id
                    if has_memory_entry(&sel.scene) {
                        MemoryTraceDetail {
                            key: "{sel.run_id}",
                            agent_id: sel.agent_id.clone(),
                            trace_id: sel.run_id.clone(),
                        }
                    }

                    div { class: "rounded-lg border border-base-300 bg-base-100 p-2 space-y-1",
                        div { class: "flex items-center gap-2 flex-wrap",
                            span { class: "badge orz-tag badge-sm", "{scene_label(&sel.scene)}" }
                            span { class: "text-xs text-base-content/60",
                                "{sel.steps} 步 · advisor {sel.advisor_calls} · {format_timestamp_opt(Some(sel.created_at as i64))}"
                            }
                        }
                        div { class: "text-[10px] text-base-content/40 font-mono break-all",
                            "run: {sel.run_id} · fp: {sel.fingerprint}"
                        }
                        div { class: "text-[10px] text-base-content/40 font-mono break-all",
                            "蓝图 {sel.path}"
                        }
                    }

                    if sel.rounds.is_empty() {
                        div { class: "rounded-lg border border-base-300 bg-base-100 p-2 space-y-1",
                            label { class: "form-label", "节点序列" }
                            div { class: "text-xs text-base-content/60", "该轨迹未采集轮级明细，仅记录节点序列" }
                            div { class: "flex flex-wrap gap-1",
                                for o in sel.occurrences.iter() {
                                    span {
                                        key: "{o.node}-{o.seq}",
                                        class: "badge orz-tag badge-sm",
                                        title: "{o.signal.clone().unwrap_or_default()}",
                                        "{o.node}#{o.seq}"
                                    }
                                }
                            }
                        }
                    } else {
                        div { class: "rounded-lg border border-base-300 bg-base-100 p-2",
                            ThinkRoundsTimeline { rounds: sel.rounds.clone() }
                        }
                    }
                } else if loaded() {
                    div { class: "text-center py-12 text-base-content/60 text-sm",
                        "从左侧选择一条轨迹查看记忆与轮次明细"
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
    fn memory_entry_only_for_memory_backed_scenes() {
        assert!(has_memory_entry("awaken"));
        assert!(has_memory_entry("settle"));
        assert!(has_memory_entry("compact"));
        assert!(!has_memory_entry("summary"));
        assert!(!has_memory_entry("intent-analyze"));
    }
}
