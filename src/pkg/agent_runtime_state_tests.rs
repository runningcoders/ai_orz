//! tests 单元测试（拆分自 agent_runtime_state.rs）
//!
//! 文件瘦身：原 861 行 → 531 行，测试体 331 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;

#[test]
fn test_set_busy_records_task_and_project() {
    let mgr = AgentRuntimeStateManager::new();
    mgr.set_busy("agent-1", "msg-1", Some("task-1"), Some("proj-1"));
    let info = mgr.get("agent-1").unwrap();
    assert_eq!(info.state, AgentRuntimeState::Busy);
    assert_eq!(info.current_message_id, Some("msg-1".to_string()));
    assert_eq!(info.task_id, Some("task-1".to_string()));
    assert_eq!(info.project_id, Some("proj-1".to_string()));
}

#[test]
fn test_set_busy_with_none_context() {
    let mgr = AgentRuntimeStateManager::new();
    mgr.set_busy("agent-1", "msg-1", None, None);
    let info = mgr.get("agent-1").unwrap();
    assert_eq!(info.task_id, None);
    assert_eq!(info.project_id, None);
}

#[test]
fn test_try_set_busy_records_task_and_project() {
    let mgr = AgentRuntimeStateManager::new();
    let acquired = mgr.try_set_busy("agent-1", "msg-1", Some("task-1"), Some("proj-1"));
    assert!(acquired);
    let info = mgr.get("agent-1").unwrap();
    assert_eq!(info.task_id, Some("task-1".to_string()));
    assert_eq!(info.project_id, Some("proj-1".to_string()));
}

/// 按「正在处理的消息」反查 Agent：撤回在飞消息时的定位依据
#[test]
fn test_find_busy_agent_by_message() {
    let mgr = AgentRuntimeStateManager::new();
    mgr.set_busy("agent-1", "msg-1", None, None);
    mgr.set_busy("agent-2", "msg-2", None, None);

    assert_eq!(
        mgr.find_busy_agent_by_message("msg-2"),
        Some("agent-2".to_string())
    );
    // 无人处理的消息 → None（撤回会走「排队中」分支）
    assert_eq!(mgr.find_busy_agent_by_message("msg-404"), None);

    // 思考结束（转 Idle）后反向索引必须失效，否则撤回会去取消一个已经空闲的 Agent
    mgr.set_idle("agent-2");
    assert_eq!(mgr.find_busy_agent_by_message("msg-2"), None);
}

#[test]
fn test_try_set_resting_acquires_only_when_idle() {
    let mgr = AgentRuntimeStateManager::new();

    // Idle → 抢占成功
    assert!(mgr.try_set_resting("agent-1"));
    assert_eq!(mgr.get_state("agent-1"), AgentRuntimeState::Resting);

    // 已在 Resting → 抢占失败，状态不变
    assert!(!mgr.try_set_resting("agent-1"));
    assert_eq!(mgr.get_state("agent-1"), AgentRuntimeState::Resting);

    // Busy → 抢占失败，且不覆盖正在跑的唤醒
    mgr.set_busy("agent-2", "msg-2", None, None);
    assert!(!mgr.try_set_resting("agent-2"));
    let info = mgr.get("agent-2").unwrap();
    assert_eq!(info.state, AgentRuntimeState::Busy);
    assert_eq!(info.current_message_id, Some("msg-2".to_string()));
}

/// 抢占后的可观测语义：状态为 Resting、current_message_id 清空。
///
/// 不构造「Idle 且带 task/project」的 Agent —— `set_idle` 必清空二者，这种状态不可达；
/// `try_set_resting` 只是不去显式清空它们（与 `set_resting` 对齐）。
#[test]
fn test_try_set_resting_clears_message_id() {
    let mgr = AgentRuntimeStateManager::new();
    mgr.set_busy("agent-1", "msg-1", Some("task-1"), Some("proj-1"));
    mgr.set_idle("agent-1");

    assert!(mgr.try_set_resting("agent-1"));
    let info = mgr.get("agent-1").unwrap();
    assert_eq!(info.state, AgentRuntimeState::Resting);
    assert_eq!(info.current_message_id, None);
    // 进入休息即被视为不可用：消息链路据此 nack 重投，不会与沉淀并发
    assert!(mgr.is_unavailable("agent-1"));
}

#[test]
fn test_set_idle_clears_context() {
    let mgr = AgentRuntimeStateManager::new();
    mgr.set_busy("agent-1", "msg-1", Some("task-1"), Some("proj-1"));
    mgr.set_idle("agent-1");
    let info = mgr.get("agent-1").unwrap();
    assert_eq!(info.state, AgentRuntimeState::Idle);
    assert_eq!(info.current_message_id, None);
    assert_eq!(info.task_id, None);
    assert_eq!(info.project_id, None);
}

#[test]
fn test_set_resting_preserves_task_and_project() {
    let mgr = AgentRuntimeStateManager::new();
    mgr.set_busy("agent-1", "msg-1", Some("task-1"), Some("proj-1"));
    mgr.set_resting("agent-1");
    let info = mgr.get("agent-1").unwrap();
    assert_eq!(info.state, AgentRuntimeState::Resting);
    // 沉淀场景：清空 message_id，但保留 task_id / project_id（同一业务上下文）
    assert_eq!(info.current_message_id, None);
    assert_eq!(info.task_id, Some("task-1".to_string()));
    assert_eq!(info.project_id, Some("proj-1".to_string()));
}

#[test]
fn test_set_think_runtime_attaches_to_busy_agent() {
    let mgr = AgentRuntimeStateManager::new();
    mgr.set_busy("agent-1", "msg-1", None, None);
    let tr = Arc::new(AgentThinkRuntime::new("agent-1".into(), "trace-1".into()));
    mgr.set_think_runtime("agent-1", tr.clone());

    let info = mgr.get("agent-1").unwrap();
    assert!(info.think_runtime.is_some());
    assert_eq!(info.think_runtime.as_ref().unwrap().agent_id(), "agent-1");

    let snap = mgr.get_think_runtime_snapshot("agent-1").unwrap();
    assert_eq!(snap.trace_id, "trace-1");
}

#[test]
fn test_set_idle_clears_think_runtime() {
    let mgr = AgentRuntimeStateManager::new();
    mgr.set_busy("agent-1", "msg-1", None, None);
    let tr = Arc::new(AgentThinkRuntime::new("agent-1".into(), "trace-1".into()));
    mgr.set_think_runtime("agent-1", tr);
    mgr.set_idle("agent-1");

    let info = mgr.get("agent-1").unwrap();
    assert!(info.think_runtime.is_none());
    assert!(mgr.get_think_runtime_snapshot("agent-1").is_none());
}

#[test]
fn test_cancel_thinking_signals_cancel() {
    let mgr = AgentRuntimeStateManager::new();
    mgr.set_busy("agent-1", "msg-1", None, None);
    let tr = Arc::new(AgentThinkRuntime::new("agent-1".into(), "trace-1".into()));
    let flag = tr.cancel_flag();
    mgr.set_think_runtime("agent-1", tr);

    assert_ne!(
        mgr.get_think_runtime_snapshot("agent-1").unwrap().status,
        ThinkStatus::Cancelled
    );
    assert!(mgr.cancel_thinking("agent-1"));
    assert!(flag.load(std::sync::atomic::Ordering::Relaxed));

    let snap = mgr.get_think_runtime_snapshot("agent-1").unwrap();
    assert_eq!(snap.status, ThinkStatus::Cancelled);
}

#[test]
fn test_cancel_thinking_returns_false_when_not_thinking() {
    let mgr = AgentRuntimeStateManager::new();
    // Idle agent
    assert!(!mgr.cancel_thinking("agent-1"));

    // Busy but no think_runtime attached
    mgr.set_busy("agent-1", "msg-1", None, None);
    assert!(!mgr.cancel_thinking("agent-1"));
}

#[test]
fn test_report_round_updates_snapshot() {
    let mgr = AgentRuntimeStateManager::new();
    mgr.set_busy("agent-1", "msg-1", None, None);
    let tr = Arc::new(AgentThinkRuntime::new("agent-1".into(), "trace-1".into()));
    mgr.set_think_runtime("agent-1", tr.clone());

    tr.report_round("trace-1", ThinkingScene::Awaken, 3, 365, 1000, 500, 1500, 2);
    let snap = mgr.get_think_runtime_snapshot("agent-1").unwrap();
    assert_eq!(snap.round, 3);
    assert_eq!(snap.max_rounds, 365);
    assert_eq!(snap.tokens_input, 1000);
    assert_eq!(snap.tokens_output, 500);
    assert_eq!(snap.total_tokens, 1500);
    assert_eq!(snap.tool_call_count, 2);
    assert_eq!(snap.scene, ThinkingScene::Awaken);
}

#[test]
fn test_record_context_length_is_pure_memory() {
    let mgr = AgentRuntimeStateManager::new();
    // 未记录过：get 返回 Some(info) 但上下文长度为 0
    mgr.set_busy("agent-1", "msg-1", None, None);
    assert_eq!(mgr.get("agent-1").unwrap().context_length, 0);

    // 每轮覆盖：保留最后一次的值
    mgr.record_context_length("agent-1", 12_800);
    assert_eq!(mgr.get("agent-1").unwrap().context_length, 12_800);
    mgr.record_context_length("agent-1", 25_600);
    assert_eq!(mgr.get("agent-1").unwrap().context_length, 25_600);
}

#[test]
fn test_record_context_length_survives_idle_and_resting() {
    let mgr = AgentRuntimeStateManager::new();
    mgr.set_busy("agent-1", "msg-1", None, None);
    mgr.record_context_length("agent-1", 3_200);

    // 思考结束 → Idle：上下文长度需保留（供回看最后一次上下文规模）
    mgr.set_idle("agent-1");
    assert_eq!(mgr.get("agent-1").unwrap().context_length, 3_200);

    // 沉淀 → Resting：同样保留
    mgr.set_busy("agent-1", "msg-2", None, None);
    mgr.set_resting("agent-1");
    assert_eq!(mgr.get("agent-1").unwrap().context_length, 3_200);
}

#[test]
fn test_record_context_length_threshold_pure_memory() {
    let mgr = AgentRuntimeStateManager::new();
    // 未记录过：0
    assert_eq!(mgr.get("agent-1").map(|i| i.context_length_threshold), None);

    // 写入阈值：纯内存、不随 set_idle 清零
    mgr.record_context_length_threshold("agent-1", 32_000);
    assert_eq!(mgr.get("agent-1").unwrap().context_length_threshold, 32_000);

    // 思考结束 → Idle：保留
    mgr.set_busy("agent-1", "msg-1", None, None);
    mgr.set_idle("agent-1");
    assert_eq!(mgr.get("agent-1").unwrap().context_length_threshold, 32_000);
}

#[test]
fn test_clear_think_runtime_explicit() {
    let mgr = AgentRuntimeStateManager::new();
    mgr.set_busy("agent-1", "msg-1", None, None);
    let tr = Arc::new(AgentThinkRuntime::new("agent-1".into(), "trace-1".into()));
    mgr.set_think_runtime("agent-1", tr);
    assert!(mgr.get_think_runtime_snapshot("agent-1").is_some());

    mgr.clear_think_runtime("agent-1");
    assert!(mgr.get_think_runtime_snapshot("agent-1").is_none());
}

/// 构造测试数据：4 个 Agent 覆盖 busy / resting / idle 与不同 task/project 组合
fn setup_list_runtime_agents(mgr: &AgentRuntimeStateManager) {
    // agent-1: busy, task-1, proj-1
    mgr.set_busy("agent-1", "msg-1", Some("task-1"), Some("proj-1"));
    // agent-2: busy, task-2, proj-1
    mgr.set_busy("agent-2", "msg-2", Some("task-2"), Some("proj-1"));
    // agent-3: resting, task-1, proj-2（沉淀保留 task/project）
    mgr.set_busy("agent-3", "msg-3", Some("task-1"), Some("proj-2"));
    mgr.set_resting("agent-3");
    // agent-4: idle（无上下文）
    mgr.set_busy("agent-4", "msg-4", None, None);
    mgr.set_idle("agent-4");
}

#[test]
fn test_list_runtime_agents_returns_all_when_no_filter() {
    let mgr = AgentRuntimeStateManager::new();
    setup_list_runtime_agents(&mgr);

    let agents = mgr.list_runtime_agents(None, None, None);
    assert_eq!(agents.len(), 4);
    let ids: Vec<&str> = agents.iter().map(|(id, _)| id.as_str()).collect();
    assert!(ids.contains(&"agent-1"));
    assert!(ids.contains(&"agent-2"));
    assert!(ids.contains(&"agent-3"));
    assert!(ids.contains(&"agent-4"));
}

#[test]
fn test_list_runtime_agents_filters_by_state() {
    let mgr = AgentRuntimeStateManager::new();
    setup_list_runtime_agents(&mgr);

    // busy：agent-1, agent-2
    let busy = mgr.list_runtime_agents(Some("busy"), None, None);
    assert_eq!(busy.len(), 2);

    // resting：agent-3
    let resting = mgr.list_runtime_agents(Some("resting"), None, None);
    assert_eq!(resting.len(), 1);
    assert_eq!(resting[0].0, "agent-3");

    // idle：agent-4
    let idle = mgr.list_runtime_agents(Some("idle"), None, None);
    assert_eq!(idle.len(), 1);
    assert_eq!(idle[0].0, "agent-4");

    // 不存在的 state：空结果
    let none = mgr.list_runtime_agents(Some("unknown"), None, None);
    assert!(none.is_empty());
}

#[test]
fn test_list_runtime_agents_filters_by_task_and_project_and_combination() {
    let mgr = AgentRuntimeStateManager::new();
    setup_list_runtime_agents(&mgr);

    // task-1：agent-1, agent-3
    let task1 = mgr.list_runtime_agents(None, Some("task-1"), None);
    assert_eq!(task1.len(), 2);

    // proj-1：agent-1, agent-2
    let proj1 = mgr.list_runtime_agents(None, None, Some("proj-1"));
    assert_eq!(proj1.len(), 2);

    // task-1 + proj-1：仅 agent-1
    let both = mgr.list_runtime_agents(None, Some("task-1"), Some("proj-1"));
    assert_eq!(both.len(), 1);
    assert_eq!(both[0].0, "agent-1");

    // busy + task-1：仅 agent-1（agent-3 是 resting 被排除）
    let combined = mgr.list_runtime_agents(Some("busy"), Some("task-1"), None);
    assert_eq!(combined.len(), 1);
    assert_eq!(combined[0].0, "agent-1");

    // busy + proj-1：agent-1, agent-2
    let busy_proj1 = mgr.list_runtime_agents(Some("busy"), None, Some("proj-1"));
    assert_eq!(busy_proj1.len(), 2);

    // 不存在的 task：空结果
    let empty = mgr.list_runtime_agents(None, Some("not-exist"), None);
    assert!(empty.is_empty());
}
