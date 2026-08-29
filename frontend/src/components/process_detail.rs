//! 后台进程详情（共享组件）
//!
//! 进程管理列表页 Modal 与聊天侧栏工具调用 Tab 弹窗共用。
//! 数据来源 shell_status（探活 + 日志尾部），支持手动刷新与终止进程。

use dioxus::prelude::*;

use crate::api::system::{get_process_status, kill_process};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::state::ErrorAlert;
use crate::store::toast::use_toast;
use crate::utils::format_timestamp_opt;
use common::api::ShellStatusResponse;

/// 进程状态徽标样式
pub fn process_alive_badge(alive: bool) -> &'static str {
    if alive {
        "badge badge-success"
    } else {
        "badge badge-ghost"
    }
}

/// 进程状态文案
pub fn process_alive_text(alive: bool) -> &'static str {
    if alive { "运行中" } else { "已退出" }
}

/// 进程详情内容（懒加载 shell_status，含日志尾部 + 刷新/终止操作）
#[component]
pub fn ProcessDetailContent(
    pid: u32,
    #[props(default = None)] on_changed: Option<EventHandler<()>>,
) -> Element {
    let toast = use_toast();
    let mut detail = use_signal(|| None::<ShellStatusResponse>);
    let mut failed = use_signal(String::new);
    let mut loading = use_signal(|| false);
    let mut show_kill_confirm = use_signal(|| false);
    let mut killing = use_signal(|| false);

    let load = move |p: u32| {
        spawn(async move {
            loading.set(true);
            match get_process_status(p, Some(50)).await {
                Ok(d) => {
                    detail.set(Some(d));
                    failed.set(String::new());
                }
                Err(e) => failed.set(format!("{}", e)),
            }
            loading.set(false);
        });
    };

    use_effect(move || {
        load(pid);
    });

    let do_kill = move |_| {
        show_kill_confirm.set(false);
        killing.set(true);
        spawn(async move {
            match kill_process(pid).await {
                Ok(resp) => {
                    if resp.killed {
                        toast.success(format!("进程 {} 已终止", pid));
                    } else {
                        toast.info(format!("进程 {} 已退出，无需终止", pid));
                    }
                    if let Some(cb) = on_changed {
                        cb.call(());
                    }
                    load(pid);
                }
                Err(e) => toast.error(format!("终止失败: {}", e)),
            }
            killing.set(false);
        });
    };

    rsx! {
        if loading() && detail().is_none() {
            div { class: "text-sm text-base-content/60 py-4 text-center", "加载中..." }
        } else if !failed().is_empty() {
            ErrorAlert { message: failed() }
        } else if let Some(d) = detail().clone() {
            div { class: "space-y-3",
                // 概要信息
                div { class: "grid grid-cols-2 gap-x-4 gap-y-2 text-sm",
                    div { class: "flex items-center gap-2",
                        span { class: "text-base-content/60", "PID" }
                        span { class: "font-mono font-semibold", "{d.pid}" }
                        span { class: process_alive_badge(d.alive), "{process_alive_text(d.alive)}" }
                    }
                    div {
                        span { class: "text-base-content/60", "退出码: " }
                        span { class: "font-mono",
                            {d.exit_code.map(|c| c.to_string()).unwrap_or_else(|| "-".to_string())}
                        }
                    }
                    div { class: "col-span-2",
                        span { class: "text-base-content/60", "命令: " }
                        span { class: "font-mono text-xs break-all", "{d.command}" }
                    }
                    div {
                        span { class: "text-base-content/60", "启动时间: " }
                        span { "{format_timestamp_opt(Some(d.started_at as i64))}" }
                    }
                    div { class: "break-all",
                        span { class: "text-base-content/60", "call_id: " }
                        span { class: "font-mono text-xs", "{d.call_id}" }
                    }
                    div { class: "col-span-2 break-all",
                        span { class: "text-base-content/60", "日志: " }
                        span { class: "font-mono text-xs", "{d.log_path}" }
                    }
                }

                // 操作按钮
                div { class: "flex gap-2",
                    button {
                        class: "btn hud-btn btn-ghost btn-sm",
                        disabled: loading(),
                        onclick: move |_| load(pid),
                        "刷新"
                    }
                    if d.alive {
                        button {
                            class: "btn hud-btn btn-error btn-sm",
                            disabled: killing(),
                            onclick: move |_| show_kill_confirm.set(true),
                            if killing() { "终止中..." } else { "终止进程" }
                        }
                    }
                }

                // 日志尾部
                div {
                    label { class: "form-label", "日志尾部（最近 50 行）" }
                    pre {
                        class: "bg-base-200 rounded p-3 text-xs font-mono overflow-x-auto max-h-64 overflow-y-auto whitespace-pre-wrap",
                        if d.log_tail.trim().is_empty() { "（暂无输出）" } else { "{d.log_tail}" }
                    }
                }
            }

            ConfirmDialog {
                show: show_kill_confirm(),
                title: "确认终止".to_string(),
                message: format!("确定终止进程 {}（{}）？此操作不可撤销。", d.pid, d.command),
                on_confirm: do_kill,
                on_cancel: move |_| show_kill_confirm.set(false),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alive_badge_and_text_variants() {
        assert_eq!(process_alive_badge(true), "badge badge-success");
        assert_eq!(process_alive_badge(false), "badge badge-ghost");
        assert_eq!(process_alive_text(true), "运行中");
        assert_eq!(process_alive_text(false), "已退出");
    }
}
