//! tests 单元测试（拆分自 wake_gate_policy.rs）
//!
//! 文件瘦身：原 416 行 → 271 行，测试体 146 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;

fn gate(task_status: Option<TaskStatus>) -> WakeGateInput {
    WakeGateInput {
        task_status,
        pending_dependencies: Vec::new(),
        stale_dependencies: Vec::new(),
        wakeup_count: None,
        max_wakeups: 0,
    }
}

#[test]
fn no_task_and_no_signals_passes_gate() {
    assert_eq!(judge_wake_gate(gate(None)), None);
}

#[test]
fn active_task_status_passes_gate() {
    for status in [TaskStatus::Pending, TaskStatus::InProgress] {
        assert_eq!(
            judge_wake_gate(gate(Some(status))),
            None,
            "{status:?} 不应被拦下"
        );
    }
}

#[test]
fn terminal_task_status_skips_wake() {
    for status in [
        TaskStatus::Completed,
        TaskStatus::Cancelled,
        TaskStatus::Archived,
    ] {
        let skip =
            judge_wake_gate(gate(Some(status))).unwrap_or_else(|| panic!("{status:?} 应被拦下"));
        assert_eq!(skip.policy_id, "task_terminal");
    }
}

#[test]
fn unmet_dependency_skips_wake_with_ids() {
    let input = WakeGateInput {
        task_status: Some(TaskStatus::Pending),
        pending_dependencies: vec!["task-A".to_string(), "task-B".to_string()],
        stale_dependencies: Vec::new(),
        wakeup_count: Some(1),
        max_wakeups: 0,
    };
    let skip = judge_wake_gate(input).expect("未就绪依赖应跳过唤醒");
    assert_eq!(skip.policy_id, "dependency_unmet");
    assert!(skip.reason.contains("task-A"));
    assert!(skip.reason.contains("task-B"));
}

#[test]
fn terminal_task_takes_priority_over_unmet_dependency() {
    // 任务已终结时不必再去算依赖：声明顺序决定 task_terminal 优先
    let input = WakeGateInput {
        task_status: Some(TaskStatus::Completed),
        pending_dependencies: vec!["task-A".to_string()],
        stale_dependencies: Vec::new(),
        wakeup_count: None,
        max_wakeups: 0,
    };
    assert_eq!(judge_wake_gate(input).unwrap().policy_id, "task_terminal");
}

#[test]
fn stale_dependency_is_reported_separately_from_unmet() {
    let input = WakeGateInput {
        task_status: Some(TaskStatus::Pending),
        pending_dependencies: vec!["task-A".to_string()],
        stale_dependencies: vec!["task-Z".to_string()],
        wakeup_count: None,
        max_wakeups: 0,
    };
    // dependency_unmet 声明在前，两者同时成立时报前者；
    // 仅失效依赖成立时才显式暴露 dependency_stale
    assert_eq!(
        judge_wake_gate(input).unwrap().policy_id,
        "dependency_unmet"
    );

    let input = WakeGateInput {
        task_status: Some(TaskStatus::Pending),
        stale_dependencies: vec!["task-Z".to_string()],
        ..gate(None)
    };
    let skip = judge_wake_gate(input).expect("失效依赖应跳过唤醒");
    assert_eq!(skip.policy_id, "dependency_stale");
    assert!(skip.reason.contains("task-Z"));
}

#[test]
fn wakeup_budget_zero_means_disabled() {
    let input = WakeGateInput {
        task_status: None,
        pending_dependencies: Vec::new(),
        stale_dependencies: Vec::new(),
        wakeup_count: Some(999),
        max_wakeups: 0,
    };
    assert_eq!(judge_wake_gate(input), None);
}

#[test]
fn wakeup_budget_exhausted_skips_wake() {
    let input = WakeGateInput {
        task_status: Some(TaskStatus::InProgress),
        pending_dependencies: Vec::new(),
        stale_dependencies: Vec::new(),
        wakeup_count: Some(5),
        max_wakeups: 5,
    };
    let skip = judge_wake_gate(input).expect("达到唤醒上限应跳过");
    assert_eq!(skip.policy_id, WAKEUP_BUDGET_POLICY_ID);
    assert!(skip.reason.contains("5/5"));
}

#[test]
fn wakeup_budget_not_reached_passes_gate() {
    let input = WakeGateInput {
        task_status: Some(TaskStatus::InProgress),
        pending_dependencies: Vec::new(),
        stale_dependencies: Vec::new(),
        wakeup_count: Some(4),
        max_wakeups: 5,
    };
    assert_eq!(judge_wake_gate(input), None);
}

#[test]
fn wakeup_count_missing_passes_gate() {
    // 无统计数据时不能误判为 0 次唤醒之外的任何结论 —— 缺数据即放行，
    // 宁可多唤醒一次也不要静默吞掉任务
    let input = WakeGateInput {
        task_status: None,
        pending_dependencies: Vec::new(),
        stale_dependencies: Vec::new(),
        wakeup_count: None,
        max_wakeups: 1,
    };
    assert_eq!(judge_wake_gate(input), None);
}
