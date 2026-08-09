//! 工具调用 Tab（聊天信息侧栏）
//!
//! 展示最近的工具调用记录（JSONL 扫描查询，limit 30），并与后台进程列表
//! 按 `call_id` join 出仍在运行的关联进程；点击 PID 徽标弹出进程详情
//! （复用 ProcessDetailContent）。行展开可查看 input/output 摘要。

use std::collections::HashMap;
use std::time::Duration;

use dioxus::prelude::*;

use crate::api::finance::query_tool_call_entries;
use crate::api::system::list_processes;
use crate::components::modal::Modal;
use crate::components::process_detail::ProcessDetailContent;
use crate::store::toast::use_toast;
use crate::utils::format_timestamp_opt;
use common::api::{
    ProcessInfo, QueryToolCallEntriesRequest, ToolCallEntryDetail, ToolCallStatusDto,
};

/// 工具调用记录查询条数上限（JSONL 扫描开销控制）
const ENTRY_LIMIT: usize = 30;

/// 防抖刷新等待时长（毫秒），与 ChatSidePanel 保持一致
const REFRESH_DEBOUNCE_MS: u64 = 2000;

/// input/output 摘要最大字符数
const SUMMARY_MAX_CHARS: usize = 300;

/// 工具调用状态徽标样式
pub fn tool_call_status_badge(status: ToolCallStatusDto) -> &'static str {
    match status {
        ToolCallStatusDto::Started => "badge badge-info",
        ToolCallStatusDto::Completed => "badge badge-success",
        ToolCallStatusDto::Failed => "badge badge-error",
    }
}

/// 工具调用状态文案
pub fn tool_call_status_text(status: ToolCallStatusDto) -> &'static str {
    match status {
        ToolCallStatusDto::Started => "执行中",
        ToolCallStatusDto::Completed => "已完成",
        ToolCallStatusDto::Failed => "失败",
    }
}

/// 耗时格式化（毫秒）：<1s 显示 ms，否则保留一位小数的秒
pub fn format_duration_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else {
        format!("{:.1}s", ms as f64 / 1000.0)
    }
}

/// JSON 摘要：紧凑序列化后按字符数截断
pub fn truncate_json_text(value: &serde_json::Value, max_chars: usize) -> String {
    let s = value.to_string();
    if s.chars().count() > max_chars {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}…", truncated)
    } else {
        s
    }
}

/// 按 call_id 收集仍在运行的进程 PID（仅 alive，用于与工具调用 join）
pub fn join_running_pids(processes: &[ProcessInfo]) -> HashMap<String, u32> {
    processes
        .iter()
        .filter(|p| p.alive)
        .map(|p| (p.call_id.clone(), p.pid))
        .collect()
}

/// 工具调用 Tab
///
/// - `project_id`：项目对话模式时按项目过滤
/// - `agent_id`：默认对话模式时按前台 Agent 过滤（两者均为 None 时不查询）
/// - `refresh_tick`：SSE 消息/手动刷新计数器，变化时防抖 2s 后刷新
#[component]
pub fn ToolCallsTab(
    project_id: Option<String>,
    agent_id: Option<String>,
    refresh_tick: u64,
) -> Element {
    let toast = use_toast();
    let mut entries = use_signal(Vec::<ToolCallEntryDetail>::new);
    let mut processes = use_signal(Vec::<ProcessInfo>::new);
    let mut loading = use_signal(|| false);
    let mut loaded = use_signal(|| false);
    // 加载代际计数：防抖窗口内被更新的请求直接丢弃
    let mut load_gen = use_signal(|| 0u64);
    let mut prev_scope = use_signal(String::new);
    let mut prev_tick = use_signal(|| 0u64);
    let mut expanded_call_id = use_signal(|| None::<String>);
    let mut detail_pid = use_signal(|| None::<u32>);

    // 加载：工具调用记录与进程列表并行请求（代际校验丢弃过期结果）
    let mut load = move |pid: Option<String>, aid: Option<String>, debounce: bool| {
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
            let req = QueryToolCallEntriesRequest {
                project_id: pid,
                agent_id: aid,
                limit: Some(ENTRY_LIMIT),
                ..Default::default()
            };
            match query_tool_call_entries(&req).await {
                Ok(list) => entries.set(list),
                Err(e) => toast.error(format!("加载工具调用记录失败: {}", e)),
            }
            if load_gen() != my_gen {
                return;
            }
            loading.set(false);
            loaded.set(true);
        });
        spawn(async move {
            if let Ok(resp) = list_processes().await
                && load_gen() == my_gen
            {
                processes.set(resp.processes);
            }
        });
    };

    // 供 Modal 内进程变更后重拉 join 数据的参数副本
    let pid_for_change = project_id.clone();
    let aid_for_change = agent_id.clone();
    let pid_for_effect = project_id.clone();
    let aid_for_effect = agent_id.clone();
    let has_scope = project_id.is_some() || agent_id.is_some();

    // 范围切换 → 立即加载；refresh_tick 变化 → 防抖刷新
    use_effect(move || {
        let scope = format!("{:?}|{:?}", pid_for_effect, aid_for_effect);
        let tick = refresh_tick;
        let scope_changed = prev_scope() != scope;
        let tick_changed = prev_tick() != tick;
        // 修复 E2E-1：仅在值真正变化时写回。Signal::set 不做相等去重，
        // 无条件写回本 effect 自己订阅的信号会触发 effect 重跑 → 无限循环卡死主线程
        if scope_changed {
            prev_scope.set(scope);
        }
        if tick_changed {
            prev_tick.set(tick);
        }
        if scope_changed {
            expanded_call_id.set(None);
            detail_pid.set(None);
            load(pid_for_effect.clone(), aid_for_effect.clone(), false);
        } else if tick_changed {
            load(pid_for_effect.clone(), aid_for_effect.clone(), true);
        }
    });

    if !has_scope {
        return rsx! {
            div { class: "text-center py-12 text-base-content/60 text-sm", "暂无前台 Agent，无法加载工具调用记录" }
        };
    }

    let pid_by_call = join_running_pids(&processes.read());
    let list = entries.read().clone();
    let refresh_btn_pid = project_id.clone();
    let refresh_btn_aid = agent_id.clone();

    rsx! {
        div { class: "space-y-2",
            // 头部：计数 + 手动刷新
            div { class: "flex items-center justify-between",
                span { class: "text-xs text-base-content/60",
                    "最近 {list.len()} 条工具调用"
                }
                button {
                    class: "btn btn-ghost btn-xs",
                    title: "刷新",
                    disabled: loading(),
                    onclick: move |_| load(refresh_btn_pid.clone(), refresh_btn_aid.clone(), false),
                    "⟳"
                }
            }

            if loading() && !loaded() {
                div { class: "flex items-center justify-center py-12",
                    span { class: "loading loading-spinner loading-md" }
                    span { class: "ml-2 text-sm text-base-content/60", "加载中..." }
                }
            } else if list.is_empty() {
                div { class: "text-center py-12 text-base-content/60 text-sm", "暂无工具调用记录" }
            } else {
                for e in list.iter() {
                    {
                        let call_id = e.call_id.clone();
                        let tool_name = e.tool_name.clone();
                        let status = e.status;
                        let duration = format_duration_ms(e.duration_ms);
                        let started = format_timestamp_opt(Some(e.started_at as i64));
                        let pid_opt = pid_by_call.get(&call_id).copied();
                        let is_expanded = expanded_call_id() == Some(call_id.clone());
                        let input_summary = truncate_json_text(&e.input, SUMMARY_MAX_CHARS);
                        let output_summary = e
                            .output
                            .as_ref()
                            .map(|v| truncate_json_text(v, SUMMARY_MAX_CHARS));
                        let error = e.error.clone();
                        rsx! {
                            div {
                                key: "{call_id}",
                                class: "rounded-lg border border-base-300 bg-base-100",
                                // 行头：点击切换展开
                                div {
                                    class: "p-2 cursor-pointer hover:bg-base-200",
                                    onclick: move |_| {
                                        if expanded_call_id() == Some(call_id.clone()) {
                                            expanded_call_id.set(None);
                                        } else {
                                            expanded_call_id.set(Some(call_id.clone()));
                                        }
                                    },
                                    div { class: "flex items-center gap-2 flex-wrap",
                                        span {
                                            class: "{tool_call_status_badge(status)}",
                                            "{tool_call_status_text(status)}"
                                        }
                                        span { class: "font-medium text-sm flex-1 truncate", "{tool_name}" }
                                        if let Some(pid) = pid_opt {
                                            button {
                                                class: "badge badge-warning cursor-pointer",
                                                title: "查看关联进程详情",
                                                onclick: move |evt: MouseEvent| {
                                                    evt.stop_propagation();
                                                    detail_pid.set(Some(pid));
                                                },
                                                "⚙ PID {pid}"
                                            }
                                        }
                                        if is_expanded { "▲" } else { "▼" }
                                    }
                                    div { class: "text-xs text-base-content/60 mt-1", "{duration} · {started}" }
                                }
                                // 展开：input/output 摘要
                                if is_expanded {
                                    div { class: "p-2 border-t border-base-300 space-y-2",
                                        div {
                                            label { class: "form-label", "输入" }
                                            pre { class: "bg-base-200 rounded p-2 text-xs font-mono whitespace-pre-wrap break-all", "{input_summary}" }
                                        }
                                        if let Some(err) = error.clone() {
                                            div {
                                                label { class: "form-label", "错误" }
                                                pre { class: "bg-error/10 text-error rounded p-2 text-xs font-mono whitespace-pre-wrap break-all", "{err}" }
                                            }
                                        }
                                        if let Some(out) = output_summary {
                                            div {
                                                label { class: "form-label", "输出" }
                                                pre { class: "bg-base-200 rounded p-2 text-xs font-mono whitespace-pre-wrap break-all", "{out}" }
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

        // 进程详情弹窗（复用共享组件；进程变更后重拉 join 数据）
        if let Some(pid) = detail_pid() {
            Modal {
                title: format!("进程详情 - PID {}", pid),
                show: true,
                on_close: move |_| detail_pid.set(None),
                ProcessDetailContent {
                    pid,
                    on_changed: move |_| {
                        load(pid_for_change.clone(), aid_for_change.clone(), false);
                    },
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_process(pid: u32, call_id: &str, alive: bool) -> ProcessInfo {
        ProcessInfo {
            pid,
            call_id: call_id.to_string(),
            tool_id: "shell_exec".to_string(),
            agent_id: None,
            command: "sleep 10".to_string(),
            working_dir: "/tmp".to_string(),
            background: true,
            started_at: 0,
            alive,
            exit_code: None,
            log_path: String::new(),
        }
    }

    #[test]
    fn status_badge_and_text_variants() {
        assert_eq!(
            tool_call_status_badge(ToolCallStatusDto::Started),
            "badge badge-info"
        );
        assert_eq!(
            tool_call_status_badge(ToolCallStatusDto::Completed),
            "badge badge-success"
        );
        assert_eq!(
            tool_call_status_badge(ToolCallStatusDto::Failed),
            "badge badge-error"
        );
        assert_eq!(tool_call_status_text(ToolCallStatusDto::Started), "执行中");
        assert_eq!(
            tool_call_status_text(ToolCallStatusDto::Completed),
            "已完成"
        );
        assert_eq!(tool_call_status_text(ToolCallStatusDto::Failed), "失败");
    }

    #[test]
    fn format_duration_ms_boundaries() {
        assert_eq!(format_duration_ms(250), "250ms");
        assert_eq!(format_duration_ms(999), "999ms");
        assert_eq!(format_duration_ms(1500), "1.5s");
        assert_eq!(format_duration_ms(65000), "65.0s");
    }

    #[test]
    fn truncate_json_text_short_unchanged() {
        let v = serde_json::json!({"city": "sh"});
        assert_eq!(truncate_json_text(&v, 100), "{\"city\":\"sh\"}");
    }

    #[test]
    fn truncate_json_text_long_truncated_with_ellipsis() {
        let v = serde_json::json!("a".repeat(500));
        let out = truncate_json_text(&v, 50);
        assert_eq!(out.chars().count(), 51);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn join_running_pids_keeps_alive_only() {
        let processes = vec![
            test_process(101, "call-a", true),
            test_process(102, "call-b", false),
            test_process(103, "call-c", true),
        ];
        let map = join_running_pids(&processes);
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("call-a"), Some(&101));
        assert_eq!(map.get("call-c"), Some(&103));
        assert!(!map.contains_key("call-b"));
    }
}
