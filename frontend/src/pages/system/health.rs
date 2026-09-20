//! 系统健康监控 - HUD 仪表盘墙
//!
//! 用通用 Gauge 组件展示系统各维度健康指标：
//! - 后端服务状态（绿/红）
//! - AOP 队列深度（绿/黄/橙/红）
//! - 活跃 Agent 比例
//! - 活跃项目比例
//! - 待处理任务数
//! - 运行时长
//! - 飞书 WS 监听连接（活跃连接数 + per-app state/重连次数明细）
//! - 微信 iLink 长轮询（活跃轮询数 + per-channel 轮次/入站/失败/超时/最近成功轮询）
//! - 工具日志存储（① 运行时输出层：占用统计 + 手动清理）
//!
//! 健康指标 10 秒轮询刷新（use_effect + spawn + loop + sleep_ms）；
//! 工具日志存储为磁盘扫描（低频），挂载时加载一次 + 清理后手动刷新。

use crate::components::hud::PageHeader;
use crate::components::hud::{HudPanel, HudSection, StatGrid, StatReadout};
use dioxus::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::api::system::{
    CleanupToolLogsRequest, HealthMetricsResponse, ToolLogStorageResponse,
    WechatPollChannelMetrics, check_health, cleanup_tool_logs, get_health_metrics,
    get_tool_log_storage,
};
use crate::components::gauge::Gauge;
use crate::components::state::Loading;
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use crate::utils::file::format_file_size;

fn aop_color(pending: u64) -> String {
    if pending >= 10 {
        "#ef4444".to_string()
    } else if pending > 0 {
        "#fa520f".to_string()
    } else {
        "#10b981".to_string()
    }
}

fn ratio_color(ratio: f64) -> String {
    if ratio >= 0.8 {
        "#10b981".to_string()
    } else if ratio >= 0.5 {
        "#f59e0b".to_string()
    } else {
        "#fa520f".to_string()
    }
}

fn task_color(pending: u64) -> String {
    if pending == 0 {
        "#10b981".to_string()
    } else if pending < 10 {
        "#f59e0b".to_string()
    } else {
        "#ef4444".to_string()
    }
}

/// 飞书 WS 连接阶段 → 状态徽标样式
fn ws_state_badge(state: &str) -> &'static str {
    match state {
        "connected" => "badge hud-badge badge-success badge-sm",
        "connecting" => "badge hud-badge badge-warning badge-sm",
        "reconnecting" => "badge hud-badge badge-error badge-sm",
        "failed" => "badge hud-badge badge-error badge-sm",
        _ => "badge hud-badge badge-ghost badge-sm",
    }
}

/// 飞书 WS 连接阶段 → 中文文案
fn ws_state_text(state: &str) -> &'static str {
    match state {
        "connected" => "已连接",
        "connecting" => "连接中",
        "reconnecting" => "重连中",
        "failed" => "已停机",
        _ => "未知",
    }
}

/// 飞书 WS「无帧」阈值（ms）
///
/// **不能照抄微信的 90s**：飞书服务端按 `PingInterval`（默认 **120s**）主动 ping，
/// 空闲渠道两次 pong 之间本就合法地无帧。阈值取 2.5 个心跳周期（300s）——
/// 超过它仍无任何帧（含 pong），`state=connected` 即判「疑似半开连接」。
const LARK_WS_STALE_MS: i64 = 300_000;

/// 飞书 WS 渠道是否疑似半开（已连接但帧停走）
///
/// `last_frame_at_ms == 0`（本连接从未收到任何帧）同样按 stale 处理；
/// 仅对 `connected` 判——重连中无帧是正常现象，`failed` 由 `terminal_reason` 呈现。
fn lark_channel_stale(state: &str, last_frame_at_ms: i64, now_ms: i64) -> bool {
    if state != "connected" {
        return false;
    }
    last_frame_at_ms == 0 || now_ms.saturating_sub(last_frame_at_ms) > LARK_WS_STALE_MS
}

/// 飞书收帧列文案（rsx `for` 循环体内禁 `let` 绑定 → 抽 helper）
fn lark_frame_text(frames_received: u64, last_frame_at_ms: i64, now_ms: i64) -> String {
    format!(
        "{} 帧 · 最近 {}",
        frames_received,
        poll_age_text(now_ms, last_frame_at_ms)
    )
}

/// 收帧列是否标红（疑似半开连接）
fn lark_frame_stale(state: &str, last_frame_at_ms: i64, now_ms: i64) -> bool {
    lark_channel_stale(state, last_frame_at_ms, now_ms)
}

/// 收帧列样式（stale → 红色文本；其余默认）
fn lark_frame_cell_class(state: &str, last_frame_at_ms: i64, now_ms: i64) -> &'static str {
    if lark_frame_stale(state, last_frame_at_ms, now_ms) {
        "text-error"
    } else {
        ""
    }
}

/// 最近 close / 终局列文案
fn lark_close_text(last_close_code: i64, terminal_reason: &Option<String>) -> String {
    if let Some(reason) = terminal_reason {
        return format!("已停机：{}", reason);
    }
    if last_close_code != 0 {
        return format!("close {}", last_close_code);
    }
    "-".to_string()
}

/// WS Gauge 颜色：终局停机/疑似半开 → 红；重连中 → 橙；有活跃连接 → 绿；无连接 → 灰
fn ws_gauge_color(m: &HealthMetricsResponse, now_ms: i64) -> String {
    let apps = &m.lark_ws.apps;
    if apps.iter().any(|a| a.terminal_reason.is_some())
        || apps
            .iter()
            .any(|a| lark_channel_stale(&a.state, a.last_frame_at_ms, now_ms))
    {
        "#ef4444".to_string()
    } else if apps.iter().any(|a| a.state == "reconnecting") {
        "#fa520f".to_string()
    } else if m.lark_ws.active_connections > 0 {
        "#10b981".to_string()
    } else {
        "#64748b".to_string()
    }
}

/// 微信长轮询「卡死」阈值（ms）
///
/// 正常时长轮询约 35s 一轮（服务端 hold 到 ~35s 返空），故 90s 无成功轮询即异常。
/// `last_poll_at_ms == 0`（从未成功轮询过）同样按卡死处理。
const WECHAT_POLL_STALE_MS: i64 = 90_000;

/// 微信渠道长轮询是否已卡死（心跳不新鲜）
fn wechat_channel_stale(last_poll_at_ms: i64, now_ms: i64) -> bool {
    last_poll_at_ms == 0 || now_ms.saturating_sub(last_poll_at_ms) > WECHAT_POLL_STALE_MS
}

/// 微信长轮询状态 → (徽标样式, 文案)
///
/// 与飞书的最大差异：WS 有连接阶段可直接看，长轮询**没有**——
/// 只能靠「轮次/心跳是否还在推进」判活，所以「卡死」判定优先于 `state`。
fn wechat_poll_badge(
    state: &str,
    last_poll_at_ms: i64,
    now_ms: i64,
) -> (&'static str, &'static str) {
    if wechat_channel_stale(last_poll_at_ms, now_ms) {
        ("badge hud-badge badge-error badge-sm", "疑似卡死")
    } else if state == "degraded" {
        ("badge hud-badge badge-warning badge-sm", "退避重试")
    } else {
        ("badge hud-badge badge-success badge-sm", "轮询中")
    }
}

/// 微信长轮询 Gauge 颜色：有卡死 → 红；有退避 → 橙；有监听且正常 → 绿；无监听 → 灰
fn wechat_gauge_color(m: &HealthMetricsResponse, now_ms: i64) -> String {
    if m.wechat_poll
        .channels
        .iter()
        .any(|c| wechat_channel_stale(c.last_poll_at_ms, now_ms))
    {
        "#ef4444".to_string()
    } else if m.wechat_poll.channels.iter().any(|c| c.state == "degraded") {
        "#fa520f".to_string()
    } else if m.wechat_poll.active_polls > 0 {
        "#10b981".to_string()
    } else {
        "#64748b".to_string()
    }
}

/// 距今时长文案（监控用：长轮询判活全靠「多久没成功轮询过」）
fn poll_age_text(now_ms: i64, ts_ms: i64) -> String {
    if ts_ms == 0 {
        return "从未".to_string();
    }
    let secs = now_ms.saturating_sub(ts_ms) / 1000;
    if secs < 60 {
        format!("{} 秒前", secs)
    } else if secs < 3600 {
        format!("{} 分钟前", secs / 60)
    } else {
        format!("{} 小时前", secs / 3600)
    }
}

/// 微信长轮询明细行
///
/// 抽成函数而非内联：rsx 的 `for` 循环体内**不能写 `let` 绑定**（宏会报
/// `expected identifier`），派生展示字段只能放到循环之外算。
fn wechat_poll_row(c: &WechatPollChannelMetrics, now_ms: i64) -> Element {
    let (badge_cls, badge_text) = wechat_poll_badge(&c.state, c.last_poll_at_ms, now_ms);
    let cursor = c.cursor.clone().unwrap_or_else(|| "-".to_string());
    let fail_cls = if c.consecutive_failures > 0 {
        "text-error font-semibold"
    } else {
        ""
    };
    let timeout_cls = if c.client_timeouts > 0 {
        "text-warning font-semibold"
    } else {
        ""
    };
    let last_poll = poll_age_text(now_ms, c.last_poll_at_ms);
    rsx! {
        tr {
            td { "{c.channel_name}" }
            td { class: "font-mono text-sm", "{c.bot_id}" }
            td { span { class: "{badge_cls}", "{badge_text}" } }
            td { "{c.rounds}" }
            td { "{c.inbound_messages}" }
            td { class: "{fail_cls}", "{c.consecutive_failures}" }
            td { class: "{timeout_cls}", "{c.client_timeouts}" }
            td { class: "text-sm", "{last_poll}" }
            td { class: "font-mono text-xs", "{cursor}" }
        }
    }
}

/// 工具日志保留天数 → 展示文案（0 = 不清理）
fn retention_text(days: u32) -> String {
    if days == 0 {
        "不清理".to_string()
    } else {
        format!("{} 天", days)
    }
}

#[component]
pub fn SystemHealth() -> Element {
    let mut loading = use_signal(|| false);
    let mut metrics: Signal<Option<HealthMetricsResponse>> = use_signal(|| None);
    let toast = use_toast();

    // 工具日志存储（① 运行时输出层）：占用统计 + 手动清理
    // 磁盘扫描低频数据，不进 10 秒轮询，挂载加载一次 + 清理后刷新
    let mut storage: Signal<Option<ToolLogStorageResponse>> = use_signal(|| None);
    let mut cleaning = use_signal(|| false);
    // 本次清理的保留天数覆盖（空 = 用服务端 [tool_log].retention_days 配置）
    let mut retention_input = use_signal(String::new);

    let mut load_metrics = move || {
        loading.set(true);
        spawn(async move {
            match get_health_metrics().await {
                Ok(m) => metrics.set(Some(m)),
                Err(e) => toast.error(format!("加载系统指标失败: {}", e)),
            }
            loading.set(false);
        });
    };

    let load_storage = move || {
        spawn(async move {
            match get_tool_log_storage().await {
                Ok(s) => storage.set(Some(s)),
                Err(e) => toast.error(format!("加载工具日志存储统计失败: {}", e)),
            }
        });
    };

    // 初始加载 + 10 秒轮询（健康指标）。
    // 用 Arc<AtomicBool> + use_drop 守卫轮询循环：组件卸载时置 false，
    // 避免 spawn 的 loop 在离开页面后永久运行（持续打请求 + 持有已卸载组件的信号）。
    let poll_running = Arc::new(AtomicBool::new(true));
    let poll_running_drop = poll_running.clone();
    use_effect(move || {
        let running = poll_running.clone();
        load_metrics();
        spawn(async move {
            loop {
                sleep_ms(10_000).await;
                if !running.load(Ordering::SeqCst) {
                    break;
                }
                load_metrics();
            }
        });
        load_storage();
    });
    use_drop(move || {
        poll_running_drop.store(false, Ordering::SeqCst);
    });

    // 手动清理超期工具日志（保留天数可用输入框覆盖；0 = 清理关闭空跑）
    let handle_cleanup_tool_logs = move |_| {
        if cleaning() {
            return;
        }
        let retention_override = retention_input.read().trim().parse::<u32>().ok();
        cleaning.set(true);
        spawn(async move {
            match cleanup_tool_logs(CleanupToolLogsRequest {
                retention_days: retention_override,
            })
            .await
            {
                Ok(r) => {
                    if r.success {
                        toast.success(format!(
                            "工具日志清理完成：删除 {} 个日期目录 / {} 个文件，释放 {}（{} 个目录因运行中进程保护跳过）",
                            r.removed_dirs,
                            r.removed_files,
                            format_file_size(r.freed_bytes),
                            r.skipped_dirs
                        ));
                    } else {
                        toast.error("清理未执行：保留天数为 0（自动清理已关闭）");
                    }
                    match get_tool_log_storage().await {
                        Ok(s) => storage.set(Some(s)),
                        Err(e) => toast.error(format!("刷新工具日志统计失败: {}", e)),
                    }
                }
                Err(e) => toast.error(format!("工具日志清理失败: {}", e)),
            }
            cleaning.set(false);
        });
    };

    let m_opt = metrics.read().clone();

    rsx! {
        AppLayout {
        div { class: "space-y-4",
            PageHeader {
                eyebrow: Some("SYSTEM".to_string()),
                title: "系统健康监控".to_string(),
                actions: Some(rsx!{
                button {
                    class: "btn hud-btn btn-ghost btn-sm",
                    onclick: move |_| {
                        spawn(async move {
                            match check_health().await {
                                Ok(msg) => toast.success(format!("服务正常: {}", msg)),
                                Err(e) => toast.error(format!("健康检查失败: {}", e)),
                            }
                        });
                    },
                    "手动检查"
                }
                }),
            },

            if loading() && metrics.read().is_none() {
                div { class: "flex justify-center py-12",
                    Loading { size: "lg" }
                }
            } else if let Some(m) = m_opt.as_ref() {
                div { class: "grid grid-cols-2 md:grid-cols-3 lg:grid-cols-4 gap-4",
                    // 后端服务
                    Gauge {
                        title: "后端服务".to_string(),
                        center_value: if m.backend_online { "OK".to_string() } else { "DOWN".to_string() },
                        center_label: "status".to_string(),
                        color: if m.backend_online { "#10b981".to_string() } else { "#ef4444".to_string() },
                        badge: None,
                        footer: None,
                        is_selected: false,
                        width: 180.0,
                        height: 180.0,
                        on_click: None,
                    }
                    // AOP 队列
                    Gauge {
                        title: "AOP 队列".to_string(),
                        center_value: m.aop_pending.to_string(),
                        center_label: "pending".to_string(),
                        color: aop_color(m.aop_pending),
                        badge: if m.aop_in_progress > 0 {
                            Some(format!("⚙ {}", m.aop_in_progress))
                        } else {
                            None
                        },
                        footer: None,
                        is_selected: false,
                        width: 180.0,
                        height: 180.0,
                        on_click: None,
                    }
                    // 活跃 Agent
                    Gauge {
                        title: "活跃 Agent".to_string(),
                        center_value: format!("{}", m.active_agents),
                        center_label: format!("/ {}", m.total_agents),
                        color: ratio_color(
                            if m.total_agents > 0 {
                                m.active_agents as f64 / m.total_agents as f64
                            } else {
                                0.0
                            }
                        ),
                        badge: None,
                        footer: Some(format!(
                            "{:.0}% 活跃",
                            if m.total_agents > 0 {
                                m.active_agents as f64 / m.total_agents as f64 * 100.0
                            } else {
                                0.0
                            }
                        )),
                        is_selected: false,
                        width: 180.0,
                        height: 180.0,
                        on_click: None,
                    }
                    // 活跃项目
                    Gauge {
                        title: "活跃项目".to_string(),
                        center_value: format!("{}", m.active_projects),
                        center_label: format!("/ {}", m.total_projects),
                        color: ratio_color(
                            if m.total_projects > 0 {
                                m.active_projects as f64 / m.total_projects as f64
                            } else {
                                0.0
                            }
                        ),
                        badge: None,
                        footer: Some(format!(
                            "{:.0}% 活跃",
                            if m.total_projects > 0 {
                                m.active_projects as f64 / m.total_projects as f64 * 100.0
                            } else {
                                0.0
                            }
                        )),
                        is_selected: false,
                        width: 180.0,
                        height: 180.0,
                        on_click: None,
                    }
                    // 待处理任务
                    Gauge {
                        title: "待处理任务".to_string(),
                        center_value: m.pending_tasks.to_string(),
                        center_label: format!("/ {}", m.total_tasks),
                        color: task_color(m.pending_tasks),
                        badge: None,
                        footer: None,
                        is_selected: false,
                        width: 180.0,
                        height: 180.0,
                        on_click: None,
                    }
                    // 运行时长
                    Gauge {
                        title: "运行时长".to_string(),
                        center_value: format!("{}", m.uptime_secs / 3600),
                        center_label: "hours".to_string(),
                        color: "#10b981".to_string(),
                        badge: None,
                        footer: Some(format!("{}s", m.uptime_secs % 3600)),
                        is_selected: false,
                        width: 180.0,
                        height: 180.0,
                        on_click: None,
                    }
                    // 飞书 WS 监听连接
                    Gauge {
                        title: "飞书 WS 连接".to_string(),
                        center_value: m.lark_ws.active_connections.to_string(),
                        center_label: "active".to_string(),
                        color: ws_gauge_color(m, crate::utils::time::now_ms()),
                        badge: None,
                        footer: Some(format!("{} 个应用监听中", m.lark_ws.apps.len())),
                        is_selected: false,
                        width: 180.0,
                        height: 180.0,
                        on_click: None,
                    }
                    // 微信 iLink 长轮询（客户端拉，无连接阶段 → 靠轮次/心跳判活）
                    Gauge {
                        title: "微信长轮询".to_string(),
                        center_value: m.wechat_poll.active_polls.to_string(),
                        center_label: "active".to_string(),
                        color: wechat_gauge_color(m, crate::utils::time::now_ms()),
                        badge: None,
                        footer: Some(format!("{} 个渠道监听中", m.wechat_poll.channels.len())),
                        is_selected: false,
                        width: 180.0,
                        height: 180.0,
                        on_click: None,
                    }
                }

                // 飞书 WS 连接明细（per-app state + 累计重连次数）
                HudPanel { signal: Some(true),
                    div { class: "card-body",
                        HudSection { title: "飞书渠道 WS 监听明细".to_string() }
                        if m.lark_ws.apps.is_empty() {
                            div { class: "text-base-content/50 text-sm py-2",
                                "暂无活跃监听连接（启用飞书渠道并开启入站监听后自动建连）"
                            }
                        } else {
                            div { class: "overflow-x-auto",
                                table { class: "table hud-table table-zebra table-sm",
                                    thead { tr {
                                        th { "App ID" }
                                        th { "连接状态" }
                                        th { "累计重连" }
                                        th { "最近收帧" }
                                        th { "close / 终局" }
                                    } }
                                    tbody {
                                        for app in m.lark_ws.apps.iter() {
                                            tr {
                                                td { class: "font-mono text-sm", "{app.app_id}" }
                                                td { span { class: "{ws_state_badge(&app.state)}", "{ws_state_text(&app.state)}" } }
                                                td { "{app.reconnect_count}" }
                                                td { class: "{lark_frame_cell_class(&app.state, app.last_frame_at_ms, crate::utils::time::now_ms())}",
                                                    "{lark_frame_text(app.frames_received, app.last_frame_at_ms, crate::utils::time::now_ms())}"
                                                }
                                                td { class: "text-sm", "{lark_close_text(app.last_close_code, &app.terminal_reason)}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // 微信长轮询明细（per-channel 运行态）
                //
                // 关键判据与飞书不同：WS 断连会自己反映到连接阶段上，长轮询**不会**——
                // 任务卡死时句柄仍在册，「活跃数」照样是 1。所以这张表要的是
                // 「轮次有没有在涨 + 最近一次成功轮询多久以前」，而不是「在不在册」。
                HudPanel { signal: Some(true),
                    div { class: "card-body",
                        HudSection { title: "微信渠道长轮询明细".to_string() }
                        if m.wechat_poll.channels.is_empty() {
                            div { class: "text-base-content/50 text-sm py-2",
                                "暂无活跃长轮询（启用微信渠道并开启入站监听后自动建连）"
                            }
                        } else {
                            div { class: "overflow-x-auto",
                                table { class: "table hud-table table-zebra table-sm",
                                    thead { tr {
                                        th { "渠道" }
                                        th { "bot_id" }
                                        th { "状态" }
                                        th { "轮次" }
                                        th { "累计入站" }
                                        th { "连续失败" }
                                        th { "客户端超时" }
                                        th { "最近成功轮询" }
                                        th { "游标" }
                                    } }
                                    tbody {
                                        for c in m.wechat_poll.channels.iter() {
                                            {wechat_poll_row(c, crate::utils::time::now_ms())}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                div { class: "text-center py-12 text-base-content/50", "暂无数据" }
            }

            // 工具日志存储（① 运行时输出层治理：占用统计 + 按天明细 + 手动清理）
            HudPanel { signal: Some(true),
                div { class: "card-body",
                    HudSection { title: "工具日志存储".to_string(),
                        actions: Some(rsx!{
                            input {
                                class: "input input-sm input-bordered w-28",
                                r#type: "number",
                                min: "0",
                                placeholder: "保留天数",
                                title: "本次清理的保留天数覆盖（留空 = 服务端配置；0 = 清理关闭）",
                                value: "{retention_input}",
                                oninput: move |e| retention_input.set(e.value()),
                            }
                            button {
                                class: "btn hud-btn btn-warning btn-sm",
                                disabled: cleaning(),
                                onclick: handle_cleanup_tool_logs,
                                if cleaning() { "清理中..." } else { "立即清理" }
                            }
                        }),
                    }

                    if let Some(s) = storage.read().clone() {
                        // 占用概览
                        StatGrid {
                            StatReadout { label: "总占用".to_string(), value: format_file_size(s.total_bytes) }
                            StatReadout { label: "日志文件数".to_string(), value: format!("{}", s.total_files) }
                            StatReadout { label: "保留策略".to_string(), value: retention_text(s.retention_days),
                                delta: Some("每日 05:00 自动清理（ai_orz.toml [tool_log] 可配，运行中进程日志受保护）".to_string()) }
                        }

                        // 按天占用明细（降序：最新在前）
                        if s.by_day.is_empty() {
                            div { class: "text-base-content/50 text-sm py-2", "暂无工具运行日志" }
                        } else {
                            div { class: "overflow-x-auto",
                                table { class: "table hud-table table-zebra table-sm",
                                    thead { tr { th { "日期" }, th { "文件数" }, th { "占用" } } }
                                    tbody {
                                        for day in s.by_day.iter().rev() {
                                            tr {
                                                td { class: "font-mono text-sm", "{day.day}" }
                                                td { "{day.files}" }
                                                td { "{format_file_size(day.bytes)}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        div { class: "flex justify-center py-6",
                            Loading { size: "md" }
                        }
                    }
                }
            }
        }
        }
    }
}

/// wasm 环境的 sleep（基于 js_sys::Promise + setTimeout，参考 pages/system/aop.rs）
async fn sleep_ms(ms: u32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms as i32)
            .unwrap();
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ws_state_badge_and_text_known_phases() {
        assert_eq!(
            ws_state_badge("connected"),
            "badge hud-badge badge-success badge-sm"
        );
        assert_eq!(
            ws_state_badge("connecting"),
            "badge hud-badge badge-warning badge-sm"
        );
        assert_eq!(
            ws_state_badge("reconnecting"),
            "badge hud-badge badge-error badge-sm"
        );
        assert_eq!(ws_state_text("connected"), "已连接");
        assert_eq!(ws_state_text("connecting"), "连接中");
        assert_eq!(ws_state_text("reconnecting"), "重连中");
    }

    #[test]
    fn test_ws_state_unknown_falls_back() {
        assert_eq!(
            ws_state_badge("other"),
            "badge hud-badge badge-ghost badge-sm"
        );
        assert_eq!(ws_state_text("other"), "未知");
    }

    #[test]
    fn test_lark_channel_stale_threshold() {
        // connected + 帧新鲜 → 不判 stale
        assert!(!lark_channel_stale("connected", 1_000, 60_000));
        // connected + 帧停走（>300s）→ stale
        assert!(lark_channel_stale("connected", 1_000, 301_000 + 1_000));
        // 从未收到帧 → 直接判 stale
        assert!(lark_channel_stale("connected", 0, 60_000));
        // 非 connected 阶段不判（重连中无帧是正常现象，failed 由 terminal 呈现）
        assert!(!lark_channel_stale("reconnecting", 0, 60_000));
        assert!(!lark_channel_stale("failed", 0, 60_000));
    }

    #[test]
    fn test_lark_close_text() {
        // 终局优先
        assert_eq!(
            lark_close_text(1006, &Some("连接冲突".to_string())),
            "已停机：连接冲突"
        );
        // 无终局但有 close code
        assert_eq!(lark_close_text(1006, &None), "close 1006");
        // 什么都没有
        assert_eq!(lark_close_text(0, &None), "-");
    }

    #[test]
    fn test_retention_text() {
        assert_eq!(retention_text(0), "不清理");
        assert_eq!(retention_text(30), "30 天");
    }

    /// ws_gauge_color 优先级：终局/半开（红）> 重连（橙）> 活跃（绿）> 空闲（灰）
    #[test]
    fn test_ws_gauge_color_priority() {
        use common::api::{HealthMetricsResponse, LarkWsAppMetrics, LarkWsMetrics};

        fn metrics_with(
            apps: Vec<LarkWsAppMetrics>,
            active_connections: u64,
        ) -> HealthMetricsResponse {
            HealthMetricsResponse {
                backend_online: true,
                aop_pending: 0,
                aop_in_progress: 0,
                active_agents: 0,
                total_agents: 0,
                active_projects: 0,
                total_projects: 0,
                pending_tasks: 0,
                total_tasks: 0,
                uptime_secs: 0,
                lark_ws: LarkWsMetrics {
                    active_connections,
                    apps,
                },
                wechat_poll: Default::default(),
            }
        }

        fn app(
            state: &str,
            last_frame_at_ms: i64,
            terminal_reason: Option<String>,
        ) -> LarkWsAppMetrics {
            LarkWsAppMetrics {
                app_id: "cli_test00000000000".to_string(),
                state: state.to_string(),
                reconnect_count: 0,
                frames_received: 1,
                last_frame_at_ms,
                last_close_code: 0,
                last_close_reason: None,
                terminal_reason,
            }
        }

        let now = 1_000_000_i64;

        // 终局 → 红（优先于重连/活跃）
        let m = metrics_with(vec![app("failed", 0, Some("exceed_conn_limit".into()))], 1);
        assert_eq!(ws_gauge_color(&m, now), "#ef4444");

        // 已连接但帧停走（半开：>300s 无帧）→ 红
        let m = metrics_with(vec![app("connected", now - 301_000, None)], 1);
        assert_eq!(ws_gauge_color(&m, now), "#ef4444");

        // 重连中 → 橙（优先于活跃）
        let m = metrics_with(vec![app("reconnecting", 0, None)], 1);
        assert_eq!(ws_gauge_color(&m, now), "#fa520f");

        // 已连接且帧新鲜 → 绿
        let m = metrics_with(vec![app("connected", now - 1_000, None)], 1);
        assert_eq!(ws_gauge_color(&m, now), "#10b981");

        // 无监听 → 灰
        let m = metrics_with(vec![], 0);
        assert_eq!(ws_gauge_color(&m, now), "#64748b");
    }
}
