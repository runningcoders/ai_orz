use super::*;
use builtin::*;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

// ==================== policy_set! 宏测试 ====================

#[test]
fn test_policy_set_macro_or() {
    let flag = Arc::new(AtomicBool::new(false));
    let policy = policy_set! {
        OR {
            UserCancelPolicy(flag.clone()),
            MaxRoundsPolicy(365),
        }
    };
    // max_rounds 命中 → Or 触发
    let metrics = Metrics::new()
        .with("round_number", 365u64)
        .with("max_rounds", 365u64);
    assert!(policy.is_triggered(&metrics));

    // user_cancel 命中 → Or 触发
    flag.store(true, std::sync::atomic::Ordering::Relaxed);
    let metrics_empty = Metrics::new();
    assert!(policy.is_triggered(&metrics_empty));
}

#[test]
fn test_policy_set_macro_and() {
    let policy = policy_set! {
        AND {
            MaxRoundsPolicy(365),
            TimeoutPolicy(3600),
        }
    };
    // 只命中一个 → And 不触发
    let metrics = Metrics::new()
        .with("round_number", 365u64)
        .with("max_rounds", 365u64)
        .with("elapsed_secs", 100u64);
    assert!(!policy.is_triggered(&metrics));

    // 两个都命中 → And 触发
    let metrics_both = Metrics::new()
        .with("round_number", 365u64)
        .with("max_rounds", 365u64)
        .with("elapsed_secs", 3600u64);
    assert!(policy.is_triggered(&metrics_both));
}

#[test]
fn test_policy_set_macro_single_policy() {
    // 宏也支持单个策略（等价于直接构造）
    let policy = policy_set! {
        OR {
            MaxRoundsPolicy(5),
        }
    };
    let metrics = Metrics::new()
        .with("round_number", 5u64)
        .with("max_rounds", 5u64);
    assert!(policy.is_triggered(&metrics));
}

#[test]
fn test_policy_set_macro_auto_generated_fields() {
    let policy = policy_set! {
        OR {
            MaxRoundsPolicy(365),
            TimeoutPolicy(3600),
        }
    };
    assert_eq!(policy.id(), "or(max_rounds,timeout)");
    assert!(policy.name().contains("Or["));
    assert!(policy.condition_desc().contains("轮次"));
    assert!(policy.condition_desc().contains("超时"));
}

// ==================== 混合模式测试 ====================

#[test]
fn test_policy_set_macro_mixed_flat_plus_or() {
    // MaxRounds AND Timeout AND (UserCancel OR TokenBudget)
    let flag = Arc::new(AtomicBool::new(false));
    let policy = policy_set! {
        MaxRoundsPolicy(365),
        TimeoutPolicy(3600),
        OR {
            UserCancelPolicy(flag.clone()),
            TokenBudgetPolicy(10000),
        }
    };

    // 只命中 max_rounds → AND 不触发（timeout 未命中，OR 子组未命中）
    let metrics = Metrics::new()
        .with("round_number", 365u64)
        .with("max_rounds", 365u64)
        .with("elapsed_secs", 100u64);
    assert!(!policy.is_triggered(&metrics));

    // max_rounds + timeout 都命中，但 OR 子组未命中 → AND 不触发
    let metrics = Metrics::new()
        .with("round_number", 365u64)
        .with("max_rounds", 365u64)
        .with("elapsed_secs", 3600u64);
    assert!(!policy.is_triggered(&metrics));

    // max_rounds + timeout + token_budget（OR 子组命中）→ 全部命中，AND 触发
    let metrics = Metrics::new()
        .with("round_number", 365u64)
        .with("max_rounds", 365u64)
        .with("elapsed_secs", 3600u64)
        .with("total_tokens", 10000u64);
    assert!(policy.is_triggered(&metrics));

    // max_rounds + timeout + user_cancel（OR 子组另一分支命中）→ AND 触发
    flag.store(true, std::sync::atomic::Ordering::Relaxed);
    let metrics = Metrics::new()
        .with("round_number", 365u64)
        .with("max_rounds", 365u64)
        .with("elapsed_secs", 3600u64);
    assert!(policy.is_triggered(&metrics));
}

#[test]
fn test_policy_set_macro_mixed_multiple_subgroups() {
    // MaxRounds AND (Timeout OR ContextOverflow) AND TokenBudget
    // 两个子组 + 两个平铺策略
    let policy = policy_set! {
        MaxRoundsPolicy(50),
        TokenBudgetPolicy(10000),
        OR {
            TimeoutPolicy(3600),
            ContextOverflowPolicy(8000),
        }
    };

    // 只命中 max_rounds → 不触发
    let metrics = Metrics::new()
        .with("round_number", 50u64)
        .with("max_rounds", 50u64);
    assert!(!policy.is_triggered(&metrics));

    // max_rounds + token_budget + context_overflow（OR 子组命中）→ 全部命中
    let metrics = Metrics::new()
        .with("round_number", 50u64)
        .with("max_rounds", 50u64)
        .with("total_tokens", 10000u64)
        .with("context_tokens", 8000u64);
    assert!(policy.is_triggered(&metrics));
}

#[test]
fn test_policy_set_macro_mixed_and_subgroup() {
    // 平铺策略 + AND 子组（显式 AND {} 虽然语义等同平铺，但用于嵌套场景）
    // Timeout OR (MaxRounds AND TokenBudget)
    let policy = policy_set! {
        TimeoutPolicy(3600),
        AND {
            MaxRoundsPolicy(50),
            TokenBudgetPolicy(10000),
        }
    };

    // 只命中 timeout → AND 子组未命中，整体 AND 不触发
    let metrics = Metrics::new().with("elapsed_secs", 3600u64);
    assert!(!policy.is_triggered(&metrics));

    // timeout + max_rounds（AND 子组缺 token_budget）→ 不触发
    let metrics = Metrics::new()
        .with("elapsed_secs", 3600u64)
        .with("round_number", 50u64)
        .with("max_rounds", 50u64);
    assert!(!policy.is_triggered(&metrics));

    // 全部命中 → 触发
    let metrics = Metrics::new()
        .with("elapsed_secs", 3600u64)
        .with("round_number", 50u64)
        .with("max_rounds", 50u64)
        .with("total_tokens", 10000u64);
    assert!(policy.is_triggered(&metrics));
}

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
    assert!(
        group
            .required_metrics()
            .contains(&"round_number".to_string())
    );
    assert!(
        group
            .required_metrics()
            .contains(&"elapsed_secs".to_string())
    );
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
