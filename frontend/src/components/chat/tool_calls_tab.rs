//! 工具调用 Tab（聊天信息侧栏）
//!
//! 展示最近的工具调用记录（JSONL 扫描查询，limit 30），并与后台进程列表
//! 按 `call_id` join 出仍在运行的关联进程；点击 PID 徽标弹出进程详情
//! （复用 ProcessDetailContent）。行头可点击展开查看 input/output 摘要。
//!
//! ## 条目卡片：三行布局（对齐「思考轨迹」列表卡片的行结构）
//!
//! - **第一行**：执行状态徽章 + 发起 Agent 的 `IdentityChip`（`agent_id` 为 None
//!   〔系统级调用等〕不渲染）+ 末位展开箭头；点头像弹出与聊天页同源的信息小卡片
//!   （AvatarBubble 复用；点击经 stop_propagation 防误触发行展开）。
//! - **第二行**：工具名（占满剩余宽度、truncate）+ 运行中进程 PID 徽标
//!   （与进程列表 join 命中才有，见 [`running_pid_badge`]）。AMan 2026-11-06
//!   实机复核后定案「第二行保持名字和 pid 足够了」：project / task 维度的链路
//!   chip 已整块下线，不在本条目任何位置展示。
//! - **第三行**：开始时间靠左、用时靠右（`justify-between`）；时间戳缺失时
//!   `format_timestamp_opt` 渲染 `—`。
//!
//! ## 第二行为什么只有「名字 + PID」
//!
//! 历史上第二行曾挂一枚「长程链路标识」chip，按 `metadata.run_id` / `thread_id` /
//! `trace_id` → `task_id` → `project_id` 三级兜底取值。但工具调用条目
//! （`common::api::ToolCallEntryDetail`）没有原生 `thread_id` / `run_id`，现网
//! trace 实测 `metadata` 仅含 `caller_location`，兜底命中的多是 task / project
//! 维度的弱标识 —— AMan 定案：**不展示 project，有 pid 才展示 pid，名字 + pid
//! 足够**。故 chip 整块移除，弱标识不再冒充强语义；将来若后端真落盘 `run_id`，
//! 需重新评估展示形态（不沿用旧 chip 兜底）。

use std::collections::HashMap;
use std::time::Duration;

use dioxus::prelude::*;

use crate::api::finance::query_tool_call_entries;
use crate::api::system::list_processes;
use crate::components::avatar_bubble::{AvatarTone, BubbleAlign};
use crate::components::identity_chip::IdentityChip;
use crate::components::modal::Modal;
use crate::components::process_detail::ProcessDetailContent;
use crate::components::state::Loading;
use crate::store::toast::use_toast;
use crate::utils::format_timestamp_opt;
use crate::utils::status::{tool_call_status_badge, tool_call_status_text};
use common::api::{ProcessInfo, QueryToolCallEntriesRequest, ToolCallEntryDetail};

/// 工具调用记录查询条数上限（JSONL 扫描开销控制）
const ENTRY_LIMIT: usize = 30;

/// 防抖刷新等待时长（毫秒），与 ChatSidePanel 保持一致
const REFRESH_DEBOUNCE_MS: u64 = 2000;

/// input/output 摘要最大字符数
const SUMMARY_MAX_CHARS: usize = 300;

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

/// 第二行右侧唯一补充信息：运行中进程的 PID 徽标
///
/// AMan 2026-11-06 定案「第二行保持名字和 pid 足够了」：project / task 维度的
/// 链路 chip 全部下线（历史上曾按 `metadata.run_id` / `thread_id` / `trace_id`
/// → `task_id` → `project_id` 三级兜底，弱标识冒充强语义）。因此这里只剩一个
/// 判据：`call_id` 在「仍在运行的进程」join 命中 → 显示 PID；未命中（进程已退出 /
/// 非进程型工具 / 无关联 call_id）→ `None`，调用方整块不渲染（不补占位、不硬造）。
pub fn running_pid_badge(pid_by_call: &HashMap<String, u32>, call_id: &str) -> Option<u32> {
    pid_by_call.get(call_id).copied()
}

/// 工具调用 Tab
///
/// - `project_id`：项目对话模式时按项目过滤
/// - `agent_ids`：默认对话模式时按「当前任务出现过的 Agent」集合过滤
///   （两者均为空时不查询）
/// - `refresh_tick`：SSE 消息/手动刷新计数器，变化时防抖 2s 后刷新
#[component]
pub fn ToolCallsTab(
    project_id: Option<String>,
    agent_ids: Option<Vec<String>>,
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
    let mut load = move |pid: Option<String>, aids: Option<Vec<String>>, debounce: bool| {
        if pid.is_none() && aids.as_ref().is_none_or(|ids| ids.is_empty()) {
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
                // 多 Agent 走逗号分隔的单键值（GET query 不支持重复键）
                agent_ids: aids.map(|ids| ids.join(",")),
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
    let aids_for_change = agent_ids.clone();
    let has_scope = project_id.is_some() || agent_ids.as_ref().is_some_and(|ids| !ids.is_empty());

    // 范围切换 → 立即加载；refresh_tick 变化 → 防抖刷新
    //
    // ⚠️ 必须用 `use_reactive` 显式声明依赖：`use_effect` 只对**信号**依赖重跑，
    // `project_id` / `agent_ids` / `refresh_tick` 都是普通 prop —— 只在闭包里读它们，
    // 值变了 effect 也不会重跑（SSE 与周期轮询都推不动这个 Tab）。
    use_effect(use_reactive(
        (&project_id, &agent_ids, &refresh_tick),
        move |(pid, aids, tick)| {
            let scope = format!("{:?}|{:?}", pid, aids);
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
                load(pid, aids, false);
            } else if tick_changed {
                load(pid, aids, true);
            }
        },
    ));

    if !has_scope {
        return rsx! {
            div { class: "text-center py-12 text-base-content/60 text-sm", "暂无作用域，无法加载工具调用记录" }
        };
    }

    let pid_by_call = join_running_pids(&processes.read());
    let list = entries.read().clone();
    let refresh_btn_pid = project_id.clone();
    let refresh_btn_aids = agent_ids.clone();

    rsx! {
        div { class: "space-y-2",
            // 头部：计数 + 手动刷新
            div { class: "flex items-center justify-between",
                span { class: "text-xs text-base-content/60",
                    "最近 {list.len()} 条工具调用"
                }
                button {
                    class: "btn hud-btn btn-ghost btn-xs",
                    title: "刷新",
                    disabled: loading(),
                    onclick: move |_| load(refresh_btn_pid.clone(), refresh_btn_aids.clone(), false),
                    "⟳"
                }
            }

            if loading() && !loaded() {
                div { class: "flex items-center justify-center py-12",
                    Loading { size: "md" }
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
                        let pid_opt = running_pid_badge(&pid_by_call, &call_id);
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
                                    // 第一行：执行状态 + 发起 Agent（+ 展开指示）
                                    div { class: "flex items-center gap-2 min-w-0",
                                        span {
                                            class: "{tool_call_status_badge(status)}",
                                            "{tool_call_status_text(status)}"
                                        }
                                        // 发起 Agent 身份（T3 需求②）：负责人样式同源，点小卡片看详情。
                                        // agent_id 为 None（系统级调用等）不渲染；chip 外层限宽 + 内部
                                        // truncate，窄侧栏下压缩身份而不是撑破卡片；点击不得触发行展开。
                                        if let Some(entry_agent_id) = e.agent_id.clone() {
                                            div {
                                                class: "flex-none min-w-0 max-w-[12rem]",
                                                onclick: move |evt: MouseEvent| evt.stop_propagation(),
                                                IdentityChip {
                                                    id: entry_agent_id,
                                                    tone: AvatarTone::Agent,
                                                    align: BubbleAlign::Start,
                                                }
                                            }
                                        }
                                        span { class: "flex-1" }
                                        span {
                                            class: "text-[11px] text-base-content/50 select-none shrink-0",
                                            if is_expanded { "▲" } else { "▼" }
                                        }
                                    }
                                    // 第二行：工具名 + 运行中进程 PID（无 pid 整块不渲染）
                                    div { class: "mt-1 flex items-center gap-2 min-w-0",
                                        span {
                                            class: "font-medium text-sm flex-1 truncate",
                                            title: "{tool_name}",
                                            "{tool_name}"
                                        }
                                        if let Some(pid) = pid_opt {
                                            button {
                                                class: "badge hud-badge badge-warning cursor-pointer shrink-0",
                                                title: "查看关联进程详情",
                                                onclick: move |evt: MouseEvent| {
                                                    evt.stop_propagation();
                                                    detail_pid.set(Some(pid));
                                                },
                                                "⚙ PID {pid}"
                                            }
                                        }
                                    }
                                    // 第三行：开始时间靠左、用时靠右
                                    div { class: "mt-1 flex items-center justify-between gap-2 text-[11px] text-base-content/60",
                                        span { class: "truncate", title: "开始时间", "{started}" }
                                        span { class: "shrink-0 font-mono", title: "用时", "⏱ {duration}" }
                                    }
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
                        load(pid_for_change.clone(), aids_for_change.clone(), false);
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
    fn pid_badge_shows_up_without_project_or_task() {
        // 有 pid、无 project/task：徽标照样出现——判据只看 call_id 与运行中进程 join
        let mut map = HashMap::new();
        map.insert("call-pid-only".to_string(), 9012u32);
        assert_eq!(running_pid_badge(&map, "call-pid-only"), Some(9012));
    }

    #[test]
    fn pid_badge_absent_when_no_pid() {
        // 无 pid：None → 调用方整块不渲染（不留空壳、不造占位）
        let map = HashMap::new();
        assert_eq!(running_pid_badge(&map, "call-unknown"), None);
    }

    #[test]
    fn pid_badge_absent_for_finished_process() {
        // 进程已退出：join 已按 alive 过滤，徽标随之消失
        let processes = vec![test_process(3301, "call-done", false)];
        let map = join_running_pids(&processes);
        assert_eq!(running_pid_badge(&map, "call-done"), None);
    }

    #[test]
    fn pid_badge_ignores_other_calls_pids() {
        // 不许串号：别的 call 的 pid 不能借给本条目
        let processes = vec![test_process(4401, "call-other", true)];
        let map = join_running_pids(&processes);
        assert_eq!(running_pid_badge(&map, "call-mine"), None);
        assert_eq!(running_pid_badge(&map, "call-other"), Some(4401));
    }

    #[test]
    fn second_row_keeps_only_tool_name_and_pid() {
        // 源码结构守卫（AMan 2026-11-06 定案）：链路 chip（含 project/task 维度）
        // 不得回流。针串一律运行时拼接 —— 本模块 include_str! 读到的就是本文件，
        // 直接写死字面量会自匹配、守卫失效。
        let src = include_str!("tool_calls_tab.rs");
        let chain_fn = concat!("chain", "_ref");
        let chain_ty = concat!("Chain", "Ref");
        let chip_icon = "\u{1F9F5}"; // 织线 emoji
        assert!(!src.contains(chain_fn), "链路标识函数应已下线");
        assert!(!src.contains(chain_ty), "链路标识类型应已下线");
        assert!(!src.contains(chip_icon), "链路 chip 图标不应回流");
        let pid_render = concat!("⚙ PID ", "{pid}");
        assert!(src.contains(pid_render), "PID 徽标文案不得丢失");
        let pid_call = concat!("running_pid_badge(&pid_by_call", ", &call_id)");
        assert!(src.contains(pid_call), "第二行必须用 PID 判据函数取 pid");
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
