//! 系统健康监控 - HUD 仪表盘墙
//!
//! 用通用 Gauge 组件展示系统各维度健康指标：
//! - 后端服务状态（绿/红）
//! - AOP 队列深度（绿/黄/橙/红）
//! - 活跃 Agent 比例
//! - 活跃项目比例
//! - 待处理任务数
//! - 运行时长
//!
//! 10 秒轮询刷新（use_effect + spawn + loop + sleep_ms）。

use dioxus::prelude::*;

use crate::api::system::{HealthMetricsResponse, check_health, get_health_metrics};
use crate::components::gauge::Gauge;
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;

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

#[component]
pub fn SystemHealth() -> Element {
    let mut loading = use_signal(|| false);
    let mut metrics: Signal<Option<HealthMetricsResponse>> = use_signal(|| None);
    let toast = use_toast();

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

    // 初始加载 + 10 秒轮询
    use_effect(move || {
        load_metrics();
        spawn(async move {
            loop {
                sleep_ms(10_000).await;
                load_metrics();
            }
        });
    });

    let m_opt = metrics.read().clone();

    rsx! {
        AppLayout {
        div { class: "space-y-4",
            div { class: "flex justify-between items-center",
                h2 { class: "card-title", "系统健康监控" }
                button {
                    class: "btn btn-ghost btn-sm",
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
            }

            if loading() && metrics.read().is_none() {
                div { class: "flex justify-center py-12",
                    span { class: "loading loading-spinner loading-lg" }
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
                }
            } else {
                div { class: "text-center py-12 text-base-content/50", "暂无数据" }
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
