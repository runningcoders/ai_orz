//! 定时触发器 / 日程详情页
//!
//! 三段式：① 详情信息（When/Who/What/How，只读）；② 编辑（安全字段）；
//! ③ 执行记录（DuckDB `trigger_exec_events`，按 attempt 一条，含概览与分页）。

use super::trigger_form_fields::{
    RULE_CRON, RULE_INTERVAL, RULE_ONCE, TriggerBasicFields, format_local_datetime,
    parse_local_datetime_ms,
};
use crate::api::system::{
    get_cron_trigger, pause_cron_trigger, query_trigger_exec, query_trigger_exec_stats,
    resume_cron_trigger, update_cron_trigger, update_schedule,
};
use crate::components::charts::donut_chart::{DonutChart, DonutSlice};
use crate::components::charts::line_chart::LineChart;
use crate::components::date_time_input::DateTimeInput;
use crate::components::hud::{HudPanel, PageHeader, StatReadout};
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{
    QueryTriggerExecResponse, QueryTriggerExecStatsResponse, UpdateCronTriggerRequest,
    UpdateScheduleRequest,
};
use common::enums::{CreatorType, TriggerBehavior, TriggerExecStatus, TriggerType};
use common::models::TimeSeriesPoint;
use dioxus::prelude::*;
use dioxus_router::Link;

const DEFAULT_LIMIT: usize = 50;

fn format_time(ts_ms: i64) -> String {
    if ts_ms <= 0 {
        return "-".to_string();
    }
    crate::utils::time::format_datetime_full(ts_ms)
}

fn trigger_type_text(t: TriggerType) -> &'static str {
    match t {
        TriggerType::Once => "一次性",
        TriggerType::Cron => "Cron",
        TriggerType::Interval => "间隔",
        TriggerType::Schedule => "日程",
    }
}

fn schedule_text(t: TriggerType, cron: &str, interval: Option<i64>, run_at: Option<i64>) -> String {
    match t {
        TriggerType::Cron => cron.to_string(),
        TriggerType::Interval => format!("每 {} 秒", interval.unwrap_or(0)),
        TriggerType::Once => run_at.map(format_time).unwrap_or_else(|| "-".to_string()),
        TriggerType::Schedule => {
            if !cron.is_empty() {
                cron.to_string()
            } else if let Some(seconds) = interval {
                format!("每 {} 秒", seconds)
            } else {
                run_at.map(format_time).unwrap_or_else(|| "-".to_string())
            }
        }
    }
}

fn who_text(
    creator_type: Option<CreatorType>,
    creator_id: Option<&str>,
    agent_id: Option<&str>,
    owner_user_id: Option<&str>,
) -> String {
    let owner = owner_user_id.filter(|s| !s.is_empty()).unwrap_or("-");
    let agent = agent_id.filter(|s| !s.is_empty()).unwrap_or("-");
    let creator = match creator_type {
        Some(CreatorType::Agent) => format!("Agent:{}", creator_id.unwrap_or("-")),
        Some(CreatorType::User) => format!("User:{}", creator_id.unwrap_or("-")),
        Some(CreatorType::System) | None => "System".to_string(),
    };
    format!("归属 {owner} · 负责 {agent} · 创建 {creator}")
}

fn status_badge(status: &str) -> (&'static str, String) {
    let Some(status) = TriggerExecStatus::from_value(status) else {
        return ("badge badge-ghost badge-sm", status.to_string());
    };
    let class = match status {
        TriggerExecStatus::Success => "badge badge-success badge-sm",
        TriggerExecStatus::Skipped => "badge badge-warning badge-sm",
        TriggerExecStatus::Failed => "badge badge-error badge-sm",
    };
    (class, status.label().to_string())
}

/// 行为标签：统计库按稳定 tag 落库，未知 tag 原样兜底（兼容跨版本数据）。
fn behavior_label(raw: &str) -> String {
    TriggerBehavior::from_tag(raw)
        .map(|b| b.label().to_string())
        .unwrap_or_else(|| raw.to_string())
}

fn duration_text(ms: Option<u64>) -> String {
    ms.map(|v| format!("{} ms", v))
        .unwrap_or_else(|| "-".to_string())
}

fn optional_text(v: Option<&String>) -> String {
    v.filter(|s| !s.is_empty())
        .cloned()
        .unwrap_or_else(|| "-".to_string())
}

/// 时间规则三选一 → 表单单选值（与共享组件 RULE_* 同口径）
fn time_rule_of(cron: Option<&str>, interval: Option<i64>) -> &'static str {
    if cron.is_some_and(|c| !c.trim().is_empty()) {
        RULE_CRON
    } else if interval.is_some() {
        RULE_INTERVAL
    } else {
        RULE_ONCE
    }
}

#[component]
pub fn SystemTriggerDetail(id: String) -> Element {
    let toast = use_toast();

    // 直接用路由传入的 id 初始化，避免首帧用空 id 打一次无效请求
    let mut rid = use_signal(|| id.clone());
    let mut detail_res = use_resource(move || {
        let id = rid();
        async move { get_cron_trigger(&id).await }
    });

    // 执行记录：分页 + 可选时间范围（缺省后端兜底最近 7 天）
    let mut exec_page = use_signal(|| 0usize);
    let mut exec_res = use_resource(move || {
        let id = rid();
        let offset = exec_page() * DEFAULT_LIMIT;
        async move { query_trigger_exec(&id, None, None, Some(offset), Some(DEFAULT_LIMIT)).await }
    });
    // 统计概览 + 时序分布：与明细分页独立拉取，互不阻塞
    let mut stats_res = use_resource(move || {
        let id = rid();
        async move { query_trigger_exec_stats(&id, None, None, None).await }
    });

    // 编辑表单信号（详情加载后回填一次）
    let mut form_name = use_signal(String::new);
    let mut form_rule = use_signal(|| "cron".to_string());
    let mut form_cron = use_signal(String::new);
    let mut form_interval = use_signal(String::new);
    // 共享组件要求：一次性规则的执行时刻，详情回填 + 编辑提交
    let mut form_run_at_local = use_signal(String::new);
    let mut form_payload = use_signal(String::new);
    // 日程专属可编辑字段（系统项不开放，后端 UpdateCronTriggerRequest 无此语义）
    let mut form_description = use_signal(String::new);
    let mut form_start_at_local = use_signal(String::new);
    let mut form_end_at_local = use_signal(String::new);
    let mut form_max_runs = use_signal(String::new);
    let mut form_agent_id = use_signal(String::new);
    let mut initialized = use_signal(|| false);
    // 编辑态：默认只读，点「编辑」才展开表单；再次展开时强制回填最新详情
    let mut editing = use_signal(|| false);
    let mut saving = use_signal(|| false);

    // 响应式 :id：路由切换时必须清空上一项的表单/分页状态，否则会把 A 的内容
    // 提交到 B（initialized 为 true 会挡住新详情回填，rid 却已是 B）。
    let route = dioxus_router::use_route::<crate::pages::Route>();
    if let crate::pages::Route::SystemTriggerDetail { id: route_id } = &route
        && *rid.peek() != *route_id
    {
        rid.set(route_id.clone());
        initialized.set(false);
        editing.set(false);
        exec_page.set(0);
        stats_res.restart();
        form_name.set(String::new());
        form_rule.set("cron".to_string());
        form_cron.set(String::new());
        form_interval.set(String::new());
        form_run_at_local.set(String::new());
        form_payload.set(String::new());
        form_description.set(String::new());
        form_start_at_local.set(String::new());
        form_end_at_local.set(String::new());
        form_max_runs.set(String::new());
        form_agent_id.set(String::new());
        saving.set(false);
    }

    // 详情到手后回填表单（只回填一次，避免覆盖用户正在编辑的内容）
    if !initialized()
        && let Some(Ok(detail)) = detail_res.read().as_ref()
    {
        form_name.set(detail.name.clone());
        form_rule.set(
            time_rule_of(detail.cron_expression.as_deref(), detail.interval_seconds).to_string(),
        );
        form_cron.set(detail.cron_expression.clone().unwrap_or_default());
        form_interval.set(
            detail
                .interval_seconds
                .map(|v| v.to_string())
                .unwrap_or_default(),
        );
        form_run_at_local.set(detail.run_at.map(format_local_datetime).unwrap_or_default());
        form_payload.set(detail.payload.clone());
        form_description.set(detail.description.clone());
        form_start_at_local.set(
            detail
                .start_at
                .map(format_local_datetime)
                .unwrap_or_default(),
        );
        form_end_at_local.set(detail.end_at.map(format_local_datetime).unwrap_or_default());
        form_max_runs.set(detail.max_runs.map(|v| v.to_string()).unwrap_or_default());
        form_agent_id.set(detail.agent_id.clone().unwrap_or_default());
        initialized.set(true);
    }

    // 提前算出形态 bool，避免 on_save 闭包捕获 GenerationalRef（生命周期问题）
    let is_schedule = detail_res
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .is_some_and(|d| d.behavior != TriggerBehavior::SyncMaintenance);

    let detail_view = detail_res.read();
    let exec_view = exec_res.read();
    let stats_view = stats_res.read();

    let on_save = move |_| {
        let id = rid();
        let name = form_name();
        let rule = form_rule();
        let cron = form_cron();
        let interval = form_interval();
        let run_at_local = form_run_at_local();
        let payload = form_payload();
        let description = form_description();
        let start_local = form_start_at_local();
        let end_local = form_end_at_local();
        let max_runs_text = form_max_runs();
        let agent_id = form_agent_id();
        // 提交前校验：schedule 的三态字段无法区分「没填」与「填错」，
        // 不拦的话非法日期会被当成日志清空请求、非法次数会被当成「清除上限」。
        if rule == RULE_CRON && cron.trim().is_empty() {
            toast.error("Cron 表达式不能为空");
            return;
        }
        if rule == RULE_INTERVAL && !interval.trim().parse::<i64>().is_ok_and(|n| n > 0) {
            toast.error("间隔秒数必须为正整数");
            return;
        }
        if rule == RULE_ONCE && parse_local_datetime_ms(&run_at_local).is_none() {
            toast.error("执行时刻不能为空或格式非法");
            return;
        }
        if is_schedule {
            if !start_local.trim().is_empty() && parse_local_datetime_ms(&start_local).is_none() {
                toast.error("生效起点格式非法");
                return;
            }
            if !end_local.trim().is_empty() && parse_local_datetime_ms(&end_local).is_none() {
                toast.error("生效终点格式非法");
                return;
            }
            if !max_runs_text.trim().is_empty()
                && !max_runs_text.trim().parse::<i32>().is_ok_and(|n| n > 0)
            {
                toast.error("最大触发次数必须为正整数（留空=不限）");
                return;
            }
        }
        // 系统项 payload 是 SyncMaintenance 的分派外框，空/非法 JSON 会让消费者
        // 每个 tick 都解析失败；日程 payload 允许为空（空串=清空备注）。
        if payload.trim().is_empty() {
            if !is_schedule {
                toast.error("系统定时任务的 Payload 不能为空");
                return;
            }
        } else if serde_json::from_str::<serde_json::Value>(&payload).is_err() {
            toast.error("Payload 不是合法 JSON");
            return;
        }
        saving.set(true);
        spawn(async move {
            let (cron_v, interval_v, run_at_v) = match rule.as_str() {
                RULE_CRON => (Some(cron), None, None),
                RULE_INTERVAL => {
                    let n = interval.trim().parse::<i64>().unwrap_or(0);
                    (None, Some(n), None)
                }
                RULE_ONCE => (None, None, parse_local_datetime_ms(&run_at_local)),
                _ => (None, None, None),
            };
            let result = if is_schedule {
                // 日程走 update_schedule：时间规则整体替换 + 标题/备注，不开放身份字段。
                // payload 三态：空串→显式清空（Some(None)）；合法 JSON→覆盖（非法已在上面拦截）。
                let payload_value = if payload.trim().is_empty() {
                    Some(None)
                } else {
                    serde_json::from_str::<serde_json::Value>(&payload)
                        .ok()
                        .map(Some)
                };
                update_schedule(UpdateScheduleRequest {
                    trigger_id: id,
                    title: Some(name),
                    description: Some(description),
                    payload: payload_value,
                    cron_expression: cron_v,
                    interval_seconds: interval_v,
                    run_at: run_at_v,
                    // 三态：空串→清窗口/上限；非法输入→不动（后端保持原值）
                    start_at: Some(parse_local_datetime_ms(&start_local)),
                    end_at: Some(parse_local_datetime_ms(&end_local)),
                    max_runs: Some(max_runs_text.trim().parse::<i32>().ok()),
                    agent_id: if agent_id.trim().is_empty() {
                        None
                    } else {
                        Some(agent_id)
                    },
                })
                .await
                .map(|_| ())
            } else {
                update_cron_trigger(UpdateCronTriggerRequest {
                    trigger_id: id,
                    name: Some(name),
                    trigger_type: Some(if cron_v.is_some() {
                        TriggerType::Cron
                    } else if interval_v.is_some() {
                        TriggerType::Interval
                    } else {
                        TriggerType::Once
                    }),
                    cron_expression: cron_v,
                    interval_seconds: interval_v,
                    run_at: run_at_v,
                    payload: Some(payload),
                })
                .await
                .map(|_| ())
            };
            match result {
                Ok(_) => {
                    toast.success("保存成功");
                    editing.set(false);
                    detail_res.restart();
                    exec_res.restart();
                    stats_res.restart();
                }
                Err(e) => toast.error(format!("保存失败: {}", e)),
            }
            saving.set(false);
        });
    };

    rsx! {
        AppLayout {
            PageHeader {
                eyebrow: "SYSTEM".to_string(),
                title: "触发器详情".to_string(),
                actions: Some(rsx! {
                    Link { class: "btn hud-btn btn-ghost", to: crate::pages::Route::SystemTriggers {}, "← 返回列表" }
                }),
            }
            match detail_view.as_ref() {
                None => rsx! { Loading {} },
                Some(Err(e)) => rsx! {
                    EmptyState { message: format!("加载失败: {}", e) }
                },
                Some(Ok(detail)) => {
                    let detail = detail.clone();
                    let exec_id = detail.id.clone();
                    rsx! {
                        // ① 详情信息
                        HudPanel {
                            title: Some(detail.name.clone()),
                            eyebrow: Some("DETAIL".to_string()),
                            signal: Some(true),
                            actions: Some(rsx! {
                                if detail.is_enabled {
                                    button {
                                        class: "btn hud-btn btn-ghost btn-sm",
                                        onclick: move |_| {
                                            let id = exec_id.clone();
                                            spawn(async move {
                                                match pause_cron_trigger(&id).await {
                                                    Ok(_) => { toast.success("已暂停"); detail_res.restart(); }
                                                    Err(e) => toast.error(format!("暂停失败: {}", e)),
                                                }
                                            });
                                        },
                                        "暂停"
                                    }
                                } else {
                                    button {
                                        class: "btn hud-btn btn-ghost btn-sm",
                                        onclick: move |_| {
                                            let id = exec_id.clone();
                                            spawn(async move {
                                                match resume_cron_trigger(&id).await {
                                                    Ok(_) => { toast.success("已恢复"); detail_res.restart(); }
                                                    Err(e) => toast.error(format!("恢复失败: {}", e)),
                                                }
                                            });
                                        },
                                        "恢复"
                                    }
                                }
                                button {
                                    class: "btn hud-btn btn-primary btn-sm",
                                    onclick: move |_| {
                                        // 复位回填锁：每次进入编辑都从当前详情重新填一遍，
                                        // 避免上一次未保存的改动被带进新一轮编辑。
                                        initialized.set(false);
                                        editing.set(true);
                                    },
                                    "编辑"
                                }
                            }),
                            div { class: "grid grid-cols-1 md:grid-cols-2 gap-x-6 gap-y-2 text-sm",
                                div { class: "flex gap-2", span { class: "text-base-content/50 w-12", "类型" } span { "{trigger_type_text(detail.trigger_type)}" } }
                                div { class: "flex gap-2", span { class: "text-base-content/50 w-12", "行为" } span { "{detail.behavior.label()}" } }
                                div { class: "flex gap-2", span { class: "text-base-content/50 w-12", "When" } span { class: "font-mono", "{schedule_text(detail.trigger_type, detail.cron_expression.as_deref().unwrap_or(\"\"), detail.interval_seconds, detail.run_at)}" } }
                                div { class: "flex gap-2", span { class: "text-base-content/50 w-12", "状态" } span { if detail.is_enabled { "启用" } else { "暂停" } } }
                                div { class: "flex gap-2 md:col-span-2", span { class: "text-base-content/50 w-12", "Who" } span { "{who_text(detail.creator_type, detail.creator_id.as_deref(), detail.agent_id.as_deref(), detail.owner_user_id.as_deref())}" } }
                                div { class: "flex gap-2 md:col-span-2", span { class: "text-base-content/50 w-12", "动作" } span { class: "font-mono", "{optional_text(detail.action.as_ref())}" } }
                                if !detail.description.is_empty() {
                                    div { class: "flex gap-2 md:col-span-2", span { class: "text-base-content/50 w-12", "说明" } span { "{detail.description}" } }
                                }
                                div { class: "flex gap-2", span { class: "text-base-content/50 w-12", "下次" } span { class: "font-mono", "{format_time(detail.next_run_at)}" } }
                                div { class: "flex gap-2", span { class: "text-base-content/50 w-12", "上次" } span { class: "font-mono", "{format_time(detail.last_run_at.unwrap_or(0))}" } }
                                div { class: "flex gap-2", span { class: "text-base-content/50 w-12", "累计" } span { "{detail.total_runs} 次" } }
                                if let Some(max) = detail.max_runs {
                                    div { class: "flex gap-2", span { class: "text-base-content/50 w-12", "上限" } span { "{max} 次" } }
                                }
                                if detail.start_at.is_some() || detail.end_at.is_some() {
                                    div { class: "flex gap-2 md:col-span-2", span { class: "text-base-content/50 w-12", "窗口" } span { class: "font-mono", "{format_time(detail.start_at.unwrap_or(0))} ~ {format_time(detail.end_at.unwrap_or(0))}" } }
                                }
                                if let Some(err) = detail.last_error.as_ref().filter(|s| !s.is_empty()) {
                                    div { class: "flex gap-2 md:col-span-2", span { class: "text-base-content/50 w-12", "错误" } span { class: "text-error", "{err}" } }
                                }
                            }
                        }

                        // ② 编辑（安全字段）：默认收起，点「编辑」才展开。
                        // 公共字段复用 TriggerBasicFields。
                        if editing() {
                            HudPanel {
                                title: Some("编辑".to_string()),
                                eyebrow: Some("EDIT".to_string()),
                                signal: None,
                                actions: Some(rsx! {
                                    button { class: "btn hud-btn btn-ghost btn-sm", onclick: move |_| editing.set(false), "取消" }
                                }),
                                div { class: "flex flex-col gap-3",
                                    TriggerBasicFields {
                                        name: form_name,
                                        rule: form_rule,
                                        cron: form_cron,
                                        interval: form_interval,
                                        run_at_local: form_run_at_local,
                                        name_placeholder: "触发器名称".to_string(),
                                    }
                                    div { class: "form-control w-full",
                                        label { class: "form-label", "Payload（JSON）" }
                                        textarea { class: "textarea textarea-bordered w-full font-mono", rows: 3, value: "{form_payload}", oninput: move |e| form_payload.set(e.value()) }
                                    }
                                    if is_schedule {
                                        div { class: "form-control w-full",
                                            label { class: "form-label", "说明（回灌给 Agent 的意图）" }
                                            textarea { class: "textarea textarea-bordered w-full", rows: 2, value: "{form_description}", oninput: move |e| form_description.set(e.value()) }
                                        }
                                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-3",
                                            div { class: "form-control w-full",
                                                label { class: "form-label", "生效起点（留空=清除）" }
                                                DateTimeInput { value: form_start_at_local(), on_change: move |v| form_start_at_local.set(v) }
                                            }
                                            div { class: "form-control w-full",
                                                label { class: "form-label", "生效终点（留空=清除）" }
                                                DateTimeInput { value: form_end_at_local(), on_change: move |v| form_end_at_local.set(v) }
                                            }
                                        }
                                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-3",
                                            div { class: "form-control w-full",
                                                label { class: "form-label", "最大触发次数（留空=不限）" }
                                                input { class: "input input-bordered w-full", r#type: "number", value: "{form_max_runs}", placeholder: "10", oninput: move |e| form_max_runs.set(e.value()) }
                                            }
                                            div { class: "form-control w-full",
                                                label { class: "form-label", "负责 Agent ID（留空=不改）" }
                                                input { class: "input input-bordered w-full font-mono", value: "{form_agent_id}", placeholder: "留空保留当前负责 Agent", oninput: move |e| form_agent_id.set(e.value()) }
                                            }
                                        }
                                    }
                                    div { class: "flex gap-2 justify-end",
                                        button { class: "btn hud-btn btn-primary btn-sm", disabled: saving(), onclick: on_save,
                                            if saving() { "保存中..." } else { "保存" }
                                        }
                                    }
                                }
                            }
                        }

                        // ③ 执行统计（概览 + 趋势，独立接口）
                        HudPanel {
                            title: Some("执行统计".to_string()),
                            eyebrow: Some("EXECUTION STATS".to_string()),
                            signal: None,
                            actions: None,
                            match stats_view.as_ref() {
                                None => rsx! { Loading {} },
                                Some(Err(e)) => rsx! { EmptyState { message: format!("执行统计加载失败: {}", e) } },
                                Some(Ok(stats)) => rsx! { ExecStatsView { stats: stats.clone() } },
                            }
                        }

                        // ④ 执行明细（分页）
                        HudPanel {
                            title: Some("执行记录".to_string()),
                            eyebrow: Some("EXECUTION LOG".to_string()),
                            signal: None,
                            actions: None,
                            match exec_view.as_ref() {
                                None => rsx! { Loading {} },
                                Some(Err(e)) => rsx! { EmptyState { message: format!("执行记录加载失败: {}", e) } },
                                Some(Ok(log)) => {
                                    let total_pages = log.total.div_ceil(DEFAULT_LIMIT);
                                    rsx! {
                                        ExecLogView { log: log.clone() }
                                        if total_pages > 1 {
                                            div { class: "flex justify-center mt-4",
                                                div { class: "join",
                                                    button { class: "join-item btn btn-sm", disabled: exec_page() == 0, onclick: move |_| exec_page.set(exec_page().saturating_sub(1)), "«" }
                                                    button { class: "join-item btn btn-sm", "第 {exec_page() + 1} 页 / {total_pages}" }
                                                    button { class: "join-item btn btn-sm", disabled: exec_page() + 1 >= total_pages, onclick: move |_| exec_page.set(exec_page() + 1), "»" }
                                                }
                                            }
                                        }
                                    }
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 执行明细表（统计概览在 ExecStatsView，随独立接口刷新）
#[component]
fn ExecLogView(log: QueryTriggerExecResponse) -> Element {
    rsx! {
        if log.records.is_empty() {
            EmptyState { message: "触发器触发后会在此显示执行记录（成功/跳过/失败）".to_string() }
        } else {
            div { class: "overflow-x-auto",
                table { class: "table table-sm table-zebra",
                    thead { tr {
                        th { "时间" }
                        th { "状态" }
                        th { "动作" }
                        th { "行为" }
                        th { "耗时" }
                        th { "说明" }
                    } }
                    tbody {
                        for (i, r) in log.records.iter().enumerate() {
                            tr { key: "{i}-{r.timestamp}",
                                td { class: "font-mono whitespace-nowrap", "{format_time(r.timestamp)}" }
                                td {
                                    {
                                        let (cls, label) = status_badge(&r.status);
                                        rsx! { span { class: cls, "{label}" } }
                                    }
                                }
                                td { "{optional_text(r.action.as_ref())}" }
                                td { "{behavior_label(&r.behavior)}" }
                                td { "{duration_text(r.duration_ms)}" }
                                td { class: "max-w-xs truncate", title: r.skip_reason.clone().unwrap_or_default(), "{optional_text(r.skip_reason.as_ref())}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// 执行统计：概览五格 + 时序折线 + 状态环形图（独立接口数据）
#[component]
fn ExecStatsView(stats: QueryTriggerExecStatsResponse) -> Element {
    let s = &stats.summary;
    let line_data: Vec<TimeSeriesPoint> = stats
        .buckets
        .iter()
        .map(|b| TimeSeriesPoint {
            interval_start: b.interval_start,
            tokens_input: 0,
            tokens_output: 0,
            tokens_cached: 0,
            tokens_reasoning: 0,
            call_count: b.total,
            avg_duration_ms: None,
            duration_sample_count: 0,
        })
        .collect();
    let status_slices: Vec<DonutSlice> = [
        ("成功", s.success, "#10b981"),
        ("跳过", s.skipped, "#f59e0b"),
        ("失败", s.failed, "#ef4444"),
    ]
    .into_iter()
    .filter(|(_, v, _)| *v > 0)
    .map(|(label, value, color)| DonutSlice {
        label: label.to_string(),
        value,
        color: color.to_string(),
    })
    .collect();
    let avg_text = s
        .avg_duration_ms
        .map(|v| format!("{v:.0}"))
        .unwrap_or_else(|| "-".to_string());

    rsx! {
        // 5 格读数：StatGrid 最多 4 列会把第五格挤到单独一行，这里单独指定 5 列
        div { class: "grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-5 gap-4",
            StatReadout { label: "总执行".to_string(), value: s.total.to_string(), unit: None, icon: None, delta: None, accent: Some("primary".to_string()), compact: None }
            StatReadout { label: "成功".to_string(), value: s.success.to_string(), unit: None, icon: None, delta: None, accent: Some("success".to_string()), compact: None }
            StatReadout { label: "跳过".to_string(), value: s.skipped.to_string(), unit: None, icon: None, delta: None, accent: Some("warning".to_string()), compact: None }
            StatReadout { label: "失败".to_string(), value: s.failed.to_string(), unit: None, icon: None, delta: None, accent: Some("error".to_string()), compact: None }
            StatReadout { label: "平均耗时".to_string(), value: avg_text, unit: Some("ms".to_string()), icon: None, delta: None, accent: None, compact: None }
        }
        div { class: "grid grid-cols-1 lg:grid-cols-3 gap-4 mt-4",
            div { class: "lg:col-span-2",
                if line_data.is_empty() {
                    EmptyState { message: "暂无执行趋势数据".to_string() }
                } else {
                    LineChart {
                        data: line_data,
                        width: Some(900.0),
                        height: Some(240.0),
                        title: Some("执行趋势（最近 7 天，按小时桶）".to_string()),
                        value_label: Some("次数".to_string()),
                    }
                }
            }
            if !status_slices.is_empty() {
                DonutChart {
                    data: status_slices,
                    width: Some(240.0),
                    height: Some(240.0),
                    center_label: Some("执行数".to_string()),
                }
            }
        }
    }
}
