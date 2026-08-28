//! 内置策略实现
//!
//! 5 个内置策略：
//! - MaxRoundsPolicy：轮次上限
//! - TimeoutPolicy：超时
//! - ContextOverflowPolicy：上下文溢出
//! - UserCancelPolicy：用户取消（检查 Arc<AtomicBool>）
//! - TokenBudgetPolicy：Token 预算

use super::{Metrics, Policy};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// 轮次上限策略
pub struct MaxRoundsPolicy {
    desc: &'static str,
}

impl MaxRoundsPolicy {
    pub fn new(max_rounds: usize) -> Self {
        Self {
            desc: Box::leak(format!("轮次 >= {}", max_rounds).into_boxed_str()),
        }
    }
}

impl Policy for MaxRoundsPolicy {
    fn id(&self) -> &str {
        "max_rounds"
    }
    fn name(&self) -> &str {
        "MaxRounds"
    }
    fn condition_desc(&self) -> &str {
        self.desc
    }
    fn required_metrics(&self) -> Vec<String> {
        vec!["round_number".into(), "max_rounds".into()]
    }
    fn evaluate(&self, metrics: &Metrics) -> Vec<String> {
        let round = metrics.get_u64("round_number").unwrap_or(0);
        let max = metrics.get_u64("max_rounds").unwrap_or(u64::MAX);
        if round >= max {
            vec![self.id().to_string()]
        } else {
            vec![]
        }
    }
}

/// 超时策略
pub struct TimeoutPolicy {
    timeout_secs: u64,
    desc: &'static str,
}

impl TimeoutPolicy {
    pub fn new(timeout_secs: u64) -> Self {
        let desc = if timeout_secs > 0 {
            Box::leak(format!("超时 >= {}s", timeout_secs).into_boxed_str())
        } else {
            "超时（未启用）"
        };
        Self { timeout_secs, desc }
    }
}

impl Policy for TimeoutPolicy {
    fn id(&self) -> &str {
        "timeout"
    }
    fn name(&self) -> &str {
        "Timeout"
    }
    fn condition_desc(&self) -> &str {
        self.desc
    }
    fn required_metrics(&self) -> Vec<String> {
        vec!["elapsed_secs".into()]
    }
    fn evaluate(&self, metrics: &Metrics) -> Vec<String> {
        let elapsed = metrics.get_u64("elapsed_secs").unwrap_or(0);
        if self.timeout_secs > 0 && elapsed >= self.timeout_secs {
            vec![self.id().to_string()]
        } else {
            vec![]
        }
    }
}

/// 上下文溢出策略
pub struct ContextOverflowPolicy {
    threshold: u64,
    desc: &'static str,
}

impl ContextOverflowPolicy {
    pub fn new(threshold: u64) -> Self {
        Self {
            threshold,
            desc: Box::leak(format!("上下文溢出 >= {}", threshold).into_boxed_str()),
        }
    }
}

impl Policy for ContextOverflowPolicy {
    fn id(&self) -> &str {
        "context_overflow"
    }
    fn name(&self) -> &str {
        "ContextOverflow"
    }
    fn condition_desc(&self) -> &str {
        self.desc
    }
    fn required_metrics(&self) -> Vec<String> {
        vec!["context_tokens".into()]
    }
    fn evaluate(&self, metrics: &Metrics) -> Vec<String> {
        let tokens = metrics.get_u64("context_tokens").unwrap_or(0);
        if tokens >= self.threshold {
            vec![self.id().to_string()]
        } else {
            vec![]
        }
    }
}

/// 用户取消策略（检查 Arc<AtomicBool>）
pub struct UserCancelPolicy {
    cancel_flag: Arc<AtomicBool>,
}

impl UserCancelPolicy {
    pub fn new(cancel_flag: Arc<AtomicBool>) -> Self {
        Self { cancel_flag }
    }
}

impl Policy for UserCancelPolicy {
    fn id(&self) -> &str {
        "user_cancel"
    }
    fn name(&self) -> &str {
        "UserCancel"
    }
    fn condition_desc(&self) -> &str {
        "用户取消"
    }
    fn required_metrics(&self) -> Vec<String> {
        vec![]
    }
    fn evaluate(&self, _metrics: &Metrics) -> Vec<String> {
        if self.cancel_flag.load(Ordering::Relaxed) {
            vec![self.id().to_string()]
        } else {
            vec![]
        }
    }
}

/// Token 预算策略
///
/// budget = 0 表示不启用（evaluate 恒不命中），允许无条件装配进策略组。
pub struct TokenBudgetPolicy {
    budget: u64,
    desc: &'static str,
}

impl TokenBudgetPolicy {
    pub fn new(budget: u64) -> Self {
        let desc = if budget > 0 {
            Box::leak(format!("Token >= {}", budget).into_boxed_str())
        } else {
            "Token 预算（未启用）"
        };
        Self { budget, desc }
    }
}

impl Policy for TokenBudgetPolicy {
    fn id(&self) -> &str {
        "token_budget"
    }
    fn name(&self) -> &str {
        "TokenBudget"
    }
    fn condition_desc(&self) -> &str {
        self.desc
    }
    fn required_metrics(&self) -> Vec<String> {
        vec!["total_tokens".into()]
    }
    fn evaluate(&self, metrics: &Metrics) -> Vec<String> {
        let total = metrics.get_u64("total_tokens").unwrap_or(0);
        if self.budget > 0 && total >= self.budget {
            vec![self.id().to_string()]
        } else {
            vec![]
        }
    }
}

/// 无进展策略：按工具差异化限制累计调用次数
///
/// 数据源为每个工具自身的运行时配置（ToolPo.config.no_progress_max_calls），
/// 由 build_policy_for_scene 从 agent.tools() 收集后传入。
/// 只限制配置了该键的工具（典型为记忆检索类——死循环重灾区），
/// 未配置的工具（如代码执行、文件编辑等高频合法工具）完全不受限，
/// 避免复杂编码场景被一刀切上限误伤。
/// 空表 = 不启用；某工具 limit = 0 = 该工具不限制。
pub struct NoProgressPolicy {
    tool_limits: std::collections::HashMap<String, usize>,
    desc: &'static str,
}

impl NoProgressPolicy {
    pub fn new(tool_limits: std::collections::HashMap<String, usize>) -> Self {
        let desc = if tool_limits.is_empty() {
            "无进展检测（未启用）".to_string()
        } else {
            let mut parts: Vec<String> = tool_limits
                .iter()
                .map(|(name, limit)| format!("{name}<{limit}"))
                .collect();
            parts.sort();
            format!("单工具调用上限 [{}]", parts.join(", "))
        };
        Self {
            tool_limits,
            desc: Box::leak(desc.into_boxed_str()),
        }
    }
}

impl Policy for NoProgressPolicy {
    fn id(&self) -> &str {
        "no_progress"
    }
    fn name(&self) -> &str {
        "NoProgress"
    }
    fn condition_desc(&self) -> &str {
        self.desc
    }
    fn required_metrics(&self) -> Vec<String> {
        self.tool_limits
            .keys()
            .map(|name| format!("tool_calls.{name}"))
            .collect()
    }
    fn evaluate(&self, metrics: &Metrics) -> Vec<String> {
        for (name, limit) in &self.tool_limits {
            if *limit == 0 {
                continue;
            }
            let key = format!("tool_calls.{name}");
            if let Some(count) = metrics.get_u64(&key) {
                if count >= *limit as u64 {
                    return vec![self.id().to_string()];
                }
            }
        }
        vec![]
    }
}
