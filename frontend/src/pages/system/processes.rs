//! 后台进程管理
//!
//! 列出 shell_exec 启动的后台进程（后端按调用方身份过滤可见范围），
//! 支持自动/手动刷新、终止进程与弹窗详情（复用 ProcessDetailContent）。

use dioxus::prelude::*;

use crate::api::system::{kill_process, list_processes};
use crate::components::modal::Modal;
use crate::components::process_detail::{
    ProcessDetailContent, process_alive_badge, process_alive_text,
};
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use crate::utils::format_timestamp_opt;
use common::api::ProcessInfo;

/// 自动刷新间隔（毫秒）
const AUTO_REFRESH_INTERVAL_MS: u64 = 5000;

/// 命令截断展示（列表态保持紧凑）
pub fn truncate_command(cmd: &str, max: usize) -> String {
    let first_line = cmd.lines().next().unwrap_or(cmd);
    if first_line.chars().count() > max {
        let truncated: String = first_line.chars().take(max).collect();
        format!("{}…", truncated)
    } else {
        first_line.to_string()
    }
}

#[component]
pub fn SystemProcesses() -> Element {
    let toast = use_toast();
    let mut processes = use_signal(Vec::<ProcessInfo>::new);
    let mut loading = use_signal(|| true);
    let mut auto_refresh = use_signal(|| false);
    let mut detail_pid = use_signal(|| None::<u32>);

    let load = move || {
        spawn(async move {
            match list_processes().await {
                Ok(resp) => processes.set(resp.processes),
                Err(e) => toast.error(format!("加载进程列表失败: {}", e)),
            }
            loading.set(false);
        });
    };

    // 初始加载
    use_effect(move || {
        loading.set(true);
        load();
    });

    // 自动刷新轮询（开启后 5s 一次）
    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(AUTO_REFRESH_INTERVAL_MS as u32).await;
            if auto_refresh() {
                load();
            }
        }
    });

    let list = processes.read().clone();
    let running_count = list.iter().filter(|p| p.alive).count();

    rsx! {
        AppLayout {
            div { class: "flex justify-between items-center mb-4",
                div { class: "flex items-center gap-3",
                    h2 { class: "card-title", "后台进程管理" }
                    span { class: "badge badge-outline", "运行中 {running_count} / 共 {list.len()}" }
                }
                div { class: "flex items-center gap-3",
                    label { class: "label cursor-pointer gap-2 py-0",
                        input {
                            "type": "checkbox",
                            class: "checkbox checkbox-sm",
                            checked: auto_refresh(),
                            onchange: move |_| {
                                let v = !auto_refresh();
                                auto_refresh.set(v);
                            },
                        }
                        span { class: "label-text text-sm", "自动刷新（5s）" }
                    }
                    button {
                        class: "btn btn-ghost btn-sm",
                        onclick: move |_| {
                            loading.set(true);
                            load();
                        },
                        "刷新"
                    }
                }
            }

            div { class: "card bg-base-100 shadow-md",
                div { class: "card-body",
                    if loading() && list.is_empty() {
                        Loading {}
                    } else if list.is_empty() {
                        EmptyState {
                            icon: "🖥️".to_string(),
                            message: "暂无后台进程（shell_exec 启动的进程会显示在这里）".to_string(),
                        }
                    } else {
                        div { class: "overflow-x-auto",
                            table { class: "table table-zebra table-pin-rows",
                                thead { tr {
                                    th { "PID" }
                                    th { "命令" }
                                    th { "状态" }
                                    th { "退出码" }
                                    th { "call_id" }
                                    th { "启动时间" }
                                    th { "操作" }
                                }}
                                tbody {
                                    for p in list.iter() {
                                        {
                                            let pid = p.pid;
                                            let call_id = p.call_id.clone();
                                            let command_display = truncate_command(&p.command, 40);
                                            let call_id_display = truncate_command(&call_id, 16);
                                            let alive = p.alive;
                                            rsx! {
                                                tr { key: "{pid}",
                                                    td { class: "font-mono font-semibold", "{pid}" }
                                                    td {
                                                        class: "font-mono text-xs",
                                                        title: "{p.command}",
                                                        "{command_display}"
                                                    }
                                                    td {
                                                        span { class: process_alive_badge(alive),
                                                            "{process_alive_text(alive)}"
                                                        }
                                                        if p.background {
                                                            span { class: "badge badge-info badge-sm ml-1", "后台" }
                                                        }
                                                    }
                                                    td { class: "font-mono",
                                                        {p.exit_code.map(|c| c.to_string()).unwrap_or_else(|| "-".to_string())}
                                                    }
                                                    td { class: "font-mono text-xs", title: "{call_id}", "{call_id_display}" }
                                                    td { class: "text-xs", "{format_timestamp_opt(Some(p.started_at as i64))}" }
                                                    td { class: "flex gap-2 items-center",
                                                        button {
                                                            class: "btn btn-ghost btn-sm",
                                                            onclick: move |_| detail_pid.set(Some(pid)),
                                                            "详情"
                                                        }
                                                        if alive {
                                                            {
                                                                let kill_pid = pid;
                                                                rsx! {
                                                                    button {
                                                                        class: "btn btn-error btn-sm",
                                                                        onclick: move |_| {
                                                                            spawn(async move {
                                                                                match kill_process(kill_pid).await {
                                                                                    Ok(resp) if resp.killed => toast.success(format!("进程 {} 已终止", kill_pid)),
                                                                                    Ok(_) => toast.info(format!("进程 {} 已退出", kill_pid)),
                                                                                    Err(e) => toast.error(format!("终止失败: {}", e)),
                                                                                }
                                                                                load();
                                                                            });
                                                                        },
                                                                        "终止"
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
                        }
                    }
                }
            }
        }

        // 进程详情弹窗（复用共享组件）
        if let Some(pid) = detail_pid() {
            Modal {
                title: format!("进程详情 - PID {}", pid),
                show: true,
                on_close: move |_| detail_pid.set(None),
                ProcessDetailContent {
                    pid,
                    on_changed: move |_| load(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_command_keeps_short_unchanged() {
        assert_eq!(truncate_command("echo hello", 40), "echo hello");
    }

    #[test]
    fn truncate_command_cuts_long_with_ellipsis() {
        let long = "a".repeat(60);
        let out = truncate_command(&long, 40);
        assert_eq!(out.chars().count(), 41);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn truncate_command_takes_first_line_only() {
        assert_eq!(truncate_command("line1\nline2", 40), "line1");
    }
}
