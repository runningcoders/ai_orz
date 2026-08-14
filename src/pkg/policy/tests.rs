use super::*;
use builtin::*;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

#[test]
fn test_max_rounds_policy_triggered() {
    let policy = MaxRoundsPolicy::new(5);
    let metrics = Metrics::new()
        .with("round_number", 5u64)
        .with("max_rounds", 5u64);
    assert!(policy.is_triggered(&metrics));
}

#[test]
fn test_max_rounds_policy_not_triggered() {
    let policy = MaxRoundsPolicy::new(365);
    let metrics = Metrics::new()
        .with("round_number", 10u64)
        .with("max_rounds", 365u64);
    assert!(!policy.is_triggered(&metrics));
}

#[test]
fn test_timeout_policy_zero_disables() {
    let policy = TimeoutPolicy::new(0);
    let metrics = Metrics::new().with("elapsed_secs", 99999u64);
    assert!(!policy.is_triggered(&metrics));
}

#[test]
fn test_timeout_policy_triggered() {
    let policy = TimeoutPolicy::new(3600);
    let metrics = Metrics::new().with("elapsed_secs", 3600u64);
    assert!(policy.is_triggered(&metrics));
}

#[test]
fn test_context_overflow_policy() {
    let policy = ContextOverflowPolicy::new(8000);
    let metrics = Metrics::new().with("context_tokens", 8000u64);
    assert!(policy.is_triggered(&metrics));

    let metrics_low = Metrics::new().with("context_tokens", 7999u64);
    assert!(!policy.is_triggered(&metrics_low));
}

#[test]
fn test_user_cancel_policy() {
    let flag = Arc::new(AtomicBool::new(false));
    let policy = UserCancelPolicy::new(flag.clone());
    let metrics = Metrics::new();
    assert!(!policy.is_triggered(&metrics));

    flag.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(policy.is_triggered(&metrics));
}

#[test]
fn test_token_budget_policy() {
    let policy = TokenBudgetPolicy::new(10000);
    let metrics = Metrics::new().with("total_tokens", 10000u64);
    assert!(policy.is_triggered(&metrics));

    let metrics_low = Metrics::new().with("total_tokens", 9999u64);
    assert!(!policy.is_triggered(&metrics_low));
}

#[test]
fn test_policy_group_or() {
    let group = PolicyGroup::new(
        vec![
            Box::new(MaxRoundsPolicy::new(5)),
            Box::new(TimeoutPolicy::new(3600)),
        ],
        PolicyRelation::Or,
    );
    // 只命中一个 → Or 触发
    let metrics = Metrics::new()
        .with("round_number", 5u64)
        .with("max_rounds", 5u64)
        .with("elapsed_secs", 100u64);
    assert!(group.is_triggered(&metrics));
}

#[test]
fn test_policy_group_and() {
    let group = PolicyGroup::new(
        vec![
            Box::new(MaxRoundsPolicy::new(5)),
            Box::new(TimeoutPolicy::new(3600)),
        ],
        PolicyRelation::And,
    );
    // 只命中一个 → And 不触发
    let metrics = Metrics::new()
        .with("round_number", 5u64)
        .with("max_rounds", 5u64)
        .with("elapsed_secs", 100u64);
    assert!(!group.is_triggered(&metrics));

    // 两个都命中
    let metrics_both = Metrics::new()
        .with("round_number", 5u64)
        .with("max_rounds", 5u64)
        .with("elapsed_secs", 3600u64);
    assert!(group.is_triggered(&metrics_both));
}

#[test]
fn test_policy_builder_single() {
    let policy = PolicyBuilder::new()
        .with(Box::new(MaxRoundsPolicy::new(365)))
        .build();
    let metrics = Metrics::new()
        .with("round_number", 365u64)
        .with("max_rounds", 365u64);
    assert!(policy.is_triggered(&metrics));
}

#[test]
fn test_policy_builder_or() {
    let flag = Arc::new(AtomicBool::new(false));
    let policy = PolicyBuilder::new()
        .with(Box::new(UserCancelPolicy::new(flag.clone())))
        .with(Box::new(MaxRoundsPolicy::new(365)))
        .or();
    // max_rounds 命中
    let metrics = Metrics::new()
        .with("round_number", 365u64)
        .with("max_rounds", 365u64);
    assert!(policy.is_triggered(&metrics));

    // user_cancel 命中
    flag.store(true, std::sync::atomic::Ordering::Relaxed);
    let metrics_empty = Metrics::new();
    assert!(policy.is_triggered(&metrics_empty));
}

#[test]
fn test_policy_builder_and() {
    let policy = PolicyBuilder::new()
        .with(Box::new(MaxRoundsPolicy::new(365)))
        .with(Box::new(TimeoutPolicy::new(3600)))
        .build();
    // 只命中一个 → And 不触发
    let metrics = Metrics::new()
        .with("round_number", 365u64)
        .with("max_rounds", 365u64)
        .with("elapsed_secs", 100u64);
    assert!(!policy.is_triggered(&metrics));
}

#[test]
fn test_policy_group_auto_generated_fields() {
    let group = PolicyGroup::new(
        vec![
            Box::new(MaxRoundsPolicy::new(365)),
            Box::new(TimeoutPolicy::new(3600)),
        ],
        PolicyRelation::Or,
    );
    assert_eq!(group.id(), "or(max_rounds,timeout)");
    assert!(group.name().contains("Or["));
    assert!(group.condition_desc().contains("轮次"));
    assert!(group.condition_desc().contains("超时"));
    assert!(group
        .required_metrics()
        .contains(&"round_number".to_string()));
    assert!(group
        .required_metrics()
        .contains(&"elapsed_secs".to_string()));
}

#[test]
fn test_policy_builder_nested() {
    // 嵌套组合：上下文溢出 OR (token 超预算 AND 轮次超 50)
    let inner_and = PolicyBuilder::new()
        .with(Box::new(TokenBudgetPolicy::new(10000)))
        .with(Box::new(MaxRoundsPolicy::new(50)))
        .build(); // And 子组

    let outer_or = PolicyBuilder::new()
        .with(Box::new(ContextOverflowPolicy::new(8000)))
        .with(inner_and)
        .or(); // Or 顶层

    // 只触发 context_overflow
    let metrics = Metrics::new().with("context_tokens", 8000u64);
    assert!(outer_or.is_triggered(&metrics));

    // 只触发 token_budget（但 And 需要 max_rounds 也命中）
    let metrics = Metrics::new().with("total_tokens", 10000u64);
    assert!(!outer_or.is_triggered(&metrics));

    // 两个 And 子策略都命中
    let metrics = Metrics::new()
        .with("total_tokens", 10000u64)
        .with("round_number", 50u64)
        .with("max_rounds", 50u64);
    assert!(outer_or.is_triggered(&metrics));
}
