//! 思考轨迹 Tab（聊天信息侧栏 / Agent 详情页共用）
//!
//! 数据源为 `pkg::think_trace` 按天落盘的 JSONL 轨迹，经运行时域
//! `GET /api/v1/hr/agents/think-traces` 分页查询：
//! - 单 Agent 视角：传 `agent_id`（Agent 详情页）；
//! - 多 Agent 聚合视角：传 `project_id` 或都不传（对话页项目模式 / 默认对话）。
//!
//! 行头渲染思考场景徽章 + 发起 Agent 身份 chip，点击展开节点 occurrence
//! 序列与选路记录（展开即用，不二次请求）。场景筛选与分页在前端驱动。

use std::time::Duration;

use dioxus::prelude::*;

use crate::api::hr::query_think_traces;
use crate::components::avatar_bubble::AvatarTone;
use crate::components::identity_chip::IdentityChip;
use crate::components::modal::Modal;
use crate::components::state::Loading;
use crate::store::toast::use_toast;
use crate::utils::format_timestamp_opt;
use common::api::{
    PaginationParams, QueryThinkTracesRequest, ThinkTraceListItem, ThinkTraceRouteDto,
};

use super::ThinkPathGraphView;
use super::memory_trace_detail::MemoryTraceDetail;
use super::think_rounds_timeline::ThinkRoundsTimeline;

/// 每页条数（JSONL 扫描开销控制）
const PAGE_SIZE: usize = 20;

/// 防抖刷新等待时长（毫秒），与其他侧栏 Tab 保持一致
const REFRESH_DEBOUNCE_MS: u64 = 2000;

/// 思考场景 → 中文文案
pub fn scene_label(scene: &str) -> &'static str {
    match scene {
        "awaken" => "唤醒",
        "settle" => "沉淀",
        "summary" => "总结",
        "compact" => "压缩",
        "intent-analyze" => "意图",
        _ => "其他",
    }
}

/// 选路来源 → 中文文案
pub fn via_label(via: &str) -> &'static str {
    match via {
        "unique" => "唯一路径",
        "deterministic" => "确定性",
        "advisor" => "advisor",
        "fallback" => "兜底",
        _ => "其他",
    }
}

/// 分页区间（1-based，含首尾）：空页返回 (0, 0)
pub fn page_bounds(offset: usize, returned: usize, total: usize) -> (usize, usize) {
    if returned == 0 {
        return (0, 0);
    }
    let start = offset + 1;
    let end = if total == 0 {
        offset + returned
    } else {
        (offset + returned).min(total)
    };
    (start, end)
}

/// 选路记录单行文案：`步骤 step: candidates → chosen（via）`
///
/// `pub` 供思考路径图组件（图/列表双视图的列表侧）复用：同一段文案两处各写一套
/// 必然漂移（改了 via 的中文说法，另一处还是旧词）。
pub fn route_label(r: &ThinkTraceRouteDto) -> String {
    let candidates = if r.candidates.is_empty() {
        "-".to_string()
    } else {
        r.candidates.join(" / ")
    };
    format!(
        "步骤 {}: {} → {}（{}）",
        r.step,
        candidates,
        r.chosen,
        via_label(&r.via)
    )
}

/// 思考轨迹 Tab
///
/// - `project_id`：按项目过滤（项目对话模式）
/// - `agent_id`：按 Agent 过滤（Agent 详情页 / 默认对话前台 Agent）
/// - `refresh_tick`：SSE 消息/手动刷新计数器，变化时防抖 2s 后刷新
///
/// 两者均为 None 时不查询（保持与 ToolCallsTab 同源的 fail-closed 行为）。
#[component]
pub fn ThinkTracesTab(
    project_id: Option<String>,
    agent_id: Option<String>,
    refresh_tick: u64,
) -> Element {
    let toast = use_toast();
    let mut entries = use_signal(Vec::<ThinkTraceListItem>::new);
    let mut total = use_signal(|| 0usize);
    let mut offset = use_signal(|| 0usize);
    let mut scene_filter = use_signal(|| None::<String>);
    let mut loading = use_signal(|| false);
    let mut loaded = use_signal(|| false);
    // 加载代际计数：防抖窗口内被更新的请求直接丢弃
    let mut load_gen = use_signal(|| 0u64);
    let mut prev_scope = use_signal(String::new);
    let mut prev_tick = use_signal(|| 0u64);
    let mut expanded_run_id = use_signal(|| None::<String>);
    // 记忆条目详情弹窗目标：(agent_id, trace_id)。run_id 在 awaken/settle/compact
    // 三类场景下与 MemoryTrace.id 同值，故可直接回查 input/output。
    let mut memory_target = use_signal(|| None::<(String, String)>);

    let mut load = move |pid: Option<String>,
                         aid: Option<String>,
                         scene: Option<String>,
                         page_offset: usize,
                         debounce: bool| {
        if pid.is_none() && aid.is_none() {
            return;
        }
        let my_gen = load_gen() + 1;
        load_gen.set(my_gen);
        loading.set(true);
        spawn(async move {
            if debounce {
                gloo_timers::future::sleep(Duration::from_millis(REFRESH_DEBOUNCE_MS)).await;
                if load_gen() != my_gen {
                    return;
                }
            }
            let req = QueryThinkTracesRequest {
                project_id: pid,
                agent_id: aid,
                scene,
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

    let pid_for_effect = project_id.clone();
    let aid_for_effect = agent_id.clone();
    let has_scope = project_id.is_some() || agent_id.is_some();

    // 范围切换 → 立即加载并重置分页/筛选；refresh_tick 变化 → 防抖刷新当前页
    use_effect(move || {
        let scope = format!("{:?}|{:?}", pid_for_effect, aid_for_effect);
        let tick = refresh_tick;
        let scope_changed = prev_scope() != scope;
        let tick_changed = prev_tick() != tick;
        // 仅在值真正变化时写回，避免 Signal::set 触发 effect 自订阅循环
        if scope_changed {
            prev_scope.set(scope);
        }
        if tick_changed {
            prev_tick.set(tick);
        }
        if scope_changed {
            offset.set(0);
            scene_filter.set(None);
            expanded_run_id.set(None);
            load(
                pid_for_effect.clone(),
                aid_for_effect.clone(),
                None,
                0,
                false,
            );
        } else if tick_changed {
            load(
                pid_for_effect.clone(),
                aid_for_effect.clone(),
                scene_filter(),
                offset(),
                true,
            );
        }
    });

    if !has_scope {
        return rsx! {
            div { class: "text-center py-12 text-base-content/60 text-sm", "暂无作用域，无法加载思考轨迹" }
        };
    }

    let list = entries.read().clone();
    let total_now = total();
    let offset_now = offset();
    let scene_now = scene_filter();
    let (start_no, end_no) = page_bounds(offset_now, list.len(), total_now);
    let page_no = offset_now / PAGE_SIZE + 1;
    let has_prev = offset_now > 0;
    let has_next = offset_now + list.len() < total_now;

    // 头部手动刷新 / 筛选 / 分页按钮需要各自捕获作用域副本
    let pid_refresh = project_id.clone();
    let aid_refresh = agent_id.clone();
    let pid_scene = project_id.clone();
    let aid_scene = agent_id.clone();
    let pid_prev = project_id.clone();
    let aid_prev = agent_id.clone();
    let pid_next = project_id.clone();
    let aid_next = agent_id.clone();

    rsx! {
        div { class: "space-y-2",
            // 头部：计数 + 手动刷新
            div { class: "flex items-center justify-between",
                span { class: "text-xs text-base-content/60",
                    "共 {total_now} 条思考轨迹"
                }
                button {
                    class: "btn hud-btn btn-ghost btn-xs",
                    title: "刷新",
                    disabled: loading(),
                    onclick: move |_| load(
                        pid_refresh.clone(),
                        aid_refresh.clone(),
                        scene_filter(),
                        offset(),
                        false,
                    ),
                    "⟳"
                }
            }

            // 场景筛选行
            div { class: "flex flex-wrap gap-1",
                for (label, value) in [
                    ("全部", None),
                    ("唤醒", Some("awaken")),
                    ("沉淀", Some("settle")),
                    ("总结", Some("summary")),
                    ("压缩", Some("compact")),
                    ("意图", Some("intent-analyze")),
                ] {
                    {
                        let selected = scene_now.as_deref() == value;
                        let pid = pid_scene.clone();
                        let aid = aid_scene.clone();
                        let value = value.map(|v| v.to_string());
                        rsx! {
                            button {
                                key: "{label}",
                                class: if selected { "btn hud-btn btn-xs btn-primary" } else { "btn hud-btn btn-xs btn-ghost" },
                                onclick: move |_| {
                                    let next = value.clone();
                                    scene_filter.set(next.clone());
                                    offset.set(0);
                                    expanded_run_id.set(None);
                                    load(pid.clone(), aid.clone(), next, 0, false);
                                },
                                "{label}"
                            }
                        }
                    }
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
                for e in list.iter() {
                    {
                        let run_id = e.run_id.clone();
                        let memory_run_id = e.run_id.clone();
                        let entry_agent_id = e.agent_id.clone();
                        let scene = e.scene.clone();
                        let steps = e.steps;
                        let advisor_calls = e.advisor_calls;
                        let created = format_timestamp_opt(Some(e.created_at as i64));
                        let is_expanded = expanded_run_id() == Some(run_id.clone());
                        let occurrences = e.occurrences.clone();
                        let routes = e.routes.clone();
                        let rounds = e.rounds.clone();
                        let path = e.path.clone();
                        let fingerprint = e.fingerprint.clone();
                        rsx! {
                            div {
                                key: "{run_id}",
                                class: "rounded-lg border border-base-300 bg-base-100",
                                div {
                                    class: "p-2 cursor-pointer hover:bg-base-200",
                                    onclick: move |_| {
                                        if expanded_run_id() == Some(run_id.clone()) {
                                            expanded_run_id.set(None);
                                        } else {
                                            expanded_run_id.set(Some(run_id.clone()));
                                        }
                                    },
                                    div { class: "flex items-center gap-2 flex-wrap",
                                        span { class: "badge orz-tag badge-sm", "{scene_label(&scene)}" }
                                        div {
                                            class: "flex-none",
                                            IdentityChip {
                                                id: e.agent_id.clone(),
                                                tone: AvatarTone::Agent,
                                            }
                                        }
                                        span { class: "text-xs text-base-content/70 flex-1 truncate",
                                            "{steps} 步 · advisor {advisor_calls}"
                                        }
                                        if is_expanded { "▲" } else { "▼" }
                                    }
                                    div { class: "text-xs text-base-content/60 mt-1",
                                        "{created} · 蓝图 {path}"
                                    }
                                }
                                if is_expanded {
                                    div { class: "p-2 border-t border-base-300 space-y-2",
                                        // 节点运行图放最上方：它是这次 run 的总览（几个节点、
                                        // 怎么转移），必须第一眼看到。此前排在轮次时间线之后，
                                        // 时间线一长就把图顶到下面去了（AMan 体验反馈①）。
                                        // 图/列表双视图 + 缩略图 + 放大弹窗：同名节点合成一卡
                                        // 带 ×N，转移边按 (from,to) 聚合，列表侧保留作降级
                                        // （老记录无 occurrences 亦不崩）
                                        ThinkPathGraphView {
                                            occurrences: occurrences.clone(),
                                            routes: routes.clone(),
                                        }
                                        // 轮次时间线：模型每一轮想了什么、调了什么、如何裁决
                                        ThinkRoundsTimeline { rounds: rounds.clone() }
                                        div { class: "text-xs text-base-content/50 font-mono break-all",
                                            "run: {run_id} · fp: {fingerprint}"
                                        }
                                        if scene == "awaken" || scene == "settle" || scene == "compact" {
                                            button {
                                                class: "btn hud-btn btn-ghost btn-xs",
                                                title: "查看本次 run 对应的记忆条目（input / output）",
                                                onclick: move |_| {
                                                    memory_target.set(Some((
                                                        entry_agent_id.clone(),
                                                        memory_run_id.clone(),
                                                    )));
                                                },
                                                "🧾 memory: {run_id}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // 分页
                div { class: "flex items-center justify-between pt-1",
                    span { class: "text-xs text-base-content/60",
                        "第 {page_no} 页 · {start_no}-{end_no} 条 / 共 {total_now} 条"
                    }
                    div { class: "flex gap-1",
                        button {
                            class: "btn hud-btn btn-ghost btn-xs",
                            disabled: !has_prev || loading(),
                            onclick: move |_| {
                                let next = offset().saturating_sub(PAGE_SIZE);
                                offset.set(next);
                                expanded_run_id.set(None);
                                load(pid_prev.clone(), aid_prev.clone(), scene_filter(), next, false);
                            },
                            "上一页"
                        }
                        button {
                            class: "btn hud-btn btn-ghost btn-xs",
                            disabled: !has_next || loading(),
                            onclick: move |_| {
                                let next = offset() + PAGE_SIZE;
                                offset.set(next);
                                expanded_run_id.set(None);
                                load(pid_next.clone(), aid_next.clone(), scene_filter(), next, false);
                            },
                            "下一页"
                        }
                    }
                }
            }

            // 记忆条目详情弹窗：与 Agent 详情页记忆段复用同一组件
            if let Some((modal_agent_id, modal_trace_id)) = memory_target() {
                Modal {
                    show: true,
                    title: "记忆轨迹详情".to_string(),
                    on_close: move |_| memory_target.set(None),
                    div { class: "max-h-[70vh] overflow-auto",
                        MemoryTraceDetail {
                            // props 变更不会自动重新拉取，靠 key 强制重挂载
                            key: "{modal_trace_id}",
                            agent_id: modal_agent_id,
                            trace_id: modal_trace_id,
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
    fn scene_label_maps_known_scenes() {
        assert_eq!(scene_label("awaken"), "唤醒");
        assert_eq!(scene_label("settle"), "沉淀");
        assert_eq!(scene_label("summary"), "总结");
        assert_eq!(scene_label("compact"), "压缩");
        assert_eq!(scene_label("intent-analyze"), "意图");
        assert_eq!(scene_label("unknown"), "其他");
    }

    #[test]
    fn via_label_maps_known_sources() {
        assert_eq!(via_label("unique"), "唯一路径");
        assert_eq!(via_label("deterministic"), "确定性");
        assert_eq!(via_label("advisor"), "advisor");
        assert_eq!(via_label("fallback"), "兜底");
        assert_eq!(via_label("other"), "其他");
    }

    #[test]
    fn page_bounds_reports_one_based_range() {
        assert_eq!(page_bounds(0, 20, 45), (1, 20));
        assert_eq!(page_bounds(20, 20, 45), (21, 40));
        assert_eq!(page_bounds(40, 5, 45), (41, 45));
        assert_eq!(page_bounds(60, 0, 45), (0, 0));
    }

    #[test]
    fn route_label_renders_candidates_and_via() {
        let route = ThinkTraceRouteDto {
            step: 3,
            candidates: vec!["plan".to_string(), "execute".to_string()],
            chosen: "execute".to_string(),
            via: "advisor".to_string(),
            reason: None,
        };
        assert_eq!(
            route_label(&route),
            "步骤 3: plan / execute → execute（advisor）"
        );
    }
}
