//! 内置策略实现
//!
//! 通用抽象（跨领域复用的判定形态）：
//! - ThresholdPolicy：单键数值阈值（`metric >= threshold`，threshold = 0 未启用），
//!   think_loop 的超时 / 上下文溢出 / Token 预算 / LLM 连续失败、消息路由的
//!   回复链深度兜底都是该形态
//! - FieldEqualsPolicy：单键字符串等值（`metric == expected`），如 Final 输出判定
//!
//! 领域策略（不具备跨领域共性的判定）保留专用实现：
//! - MaxRoundsPolicy：阈值本身来自 Metrics（双键），且随 Agent 配置变化
//! - UserCancelPolicy：状态源是 `Arc<AtomicBool>` 而非 Metrics
//! - NoProgressPolicy：多键聚合 + 工具差异化限流
//!
//! 8 个内置策略：
//! - MaxRoundsPolicy：轮次上限（保留专用：阈值来自 Metrics 双键）
//! - TimeoutPolicy：超时（ThresholdPolicy 薄包装）
//! - ContextOverflowPolicy：上下文溢出（ThresholdPolicy 薄包装）
//! - UserCancelPolicy：用户取消（保留专用：状态源非 Metrics）
//! - TokenBudgetPolicy：Token 预算（ThresholdPolicy 薄包装）
//! - FinalAnswerPolicy：模型输出 Final（FieldEqualsPolicy 薄包装）
//! - ConsecutiveLlmErrorsPolicy：LLM 连续失败预算（ThresholdPolicy 薄包装）
//! - NoProgressPolicy：单工具调用上限（保留专用：多键聚合 + 工具差异化）
//!
//! 薄包装保留原类型名与 `new` 签名：think_loop 的 `policy_set!` 与引擎测试
//! 的调用点零改动，包装仅做参数翻译 + Policy 委托（`impl_policy_delegate!`）。
//!
//! 优先级语义：`policy_set!(OR { .. })` 的声明顺序即命中顺序即优先级——
//! evaluate 按声明顺序返回命中列表，消费方（如 think_loop 的
//! map_triggered_to_result）按列表顺序取首个可执行的分派。

use super::{Metrics, Policy, PolicyAction};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// 薄包装的 Policy 委托实现：全部方法转发给内部通用策略（`.0`）
macro_rules! impl_policy_delegate {
    ($Type:ident) => {
        impl Policy for $Type {
            fn id(&self) -> &str {
                self.0.id()
            }
            fn name(&self) -> &str {
                self.0.name()
            }
            fn condition_desc(&self) -> &str {
                self.0.condition_desc()
            }
            fn required_metrics(&self) -> Vec<String> {
                self.0.required_metrics()
            }
            fn evaluate(&self, metrics: &Metrics) -> Vec<String> {
                self.0.evaluate(metrics)
            }
            fn action(&self, metrics: &Metrics) -> Option<PolicyAction> {
                self.0.action(metrics)
            }
        }
    };
}

/// 通用阈值策略：单个数值型 Metrics 键 `>=` 构造阈值即命中
///
/// 跨领域共性的抽象——「某指标达到上限即触发处置」。`threshold = 0` 表示
/// 未启用（evaluate 恒不命中），允许调用方无条件装配进策略组（如
/// think_loop 在模型未配置上下文窗口时仍装配 ContextOverflow）。
/// `unit` 仅用于 condition_desc 展示（如 `"s"` 渲染成 `>= 3600s`）。
pub struct ThresholdPolicy {
    policy_id: &'static str,
    policy_name: &'static str,
    metric_key: &'static str,
    threshold: u64,
    desc: &'static str,
}

impl ThresholdPolicy {
    pub fn new(
        policy_id: &'static str,
        policy_name: &'static str,
        metric_key: &'static str,
        threshold: u64,
        label: &'static str,
        unit: &'static str,
    ) -> Self {
        let desc = if threshold > 0 {
            Box::leak(format!("{label} >= {threshold}{unit}").into_boxed_str())
        } else {
            Box::leak(format!("{label}（未启用）").into_boxed_str())
        };
        Self {
            policy_id,
            policy_name,
            metric_key,
            threshold,
            desc,
        }
    }
}

impl Policy for ThresholdPolicy {
    fn id(&self) -> &str {
        self.policy_id
    }
    fn name(&self) -> &str {
        self.policy_name
    }
    fn condition_desc(&self) -> &str {
        self.desc
    }
    fn required_metrics(&self) -> Vec<String> {
        vec![self.metric_key.into()]
    }
    fn evaluate(&self, metrics: &Metrics) -> Vec<String> {
        let value = metrics.get_u64(self.metric_key).unwrap_or(0);
        if self.threshold > 0 && value >= self.threshold {
            vec![self.policy_id.to_string()]
        } else {
            vec![]
        }
    }
}

/// 通用字段等值策略：单个字符串型 Metrics 键 `==` 期望值即命中
///
/// 跨领域共性的抽象——「某字段取特定值即触发」。复合条件（多键同时成立）
/// 由调用方用 `PolicyGroup` And 组合，或用领域规则表的声明式条件列表表达。
pub struct FieldEqualsPolicy {
    policy_id: &'static str,
    policy_name: &'static str,
    metric_key: &'static str,
    expected: &'static str,
    desc: &'static str,
}

impl FieldEqualsPolicy {
    pub fn new(
        policy_id: &'static str,
        policy_name: &'static str,
        metric_key: &'static str,
        expected: &'static str,
        desc: &'static str,
    ) -> Self {
        Self {
            policy_id,
            policy_name,
            metric_key,
            expected,
            desc,
        }
    }
}

impl Policy for FieldEqualsPolicy {
    fn id(&self) -> &str {
        self.policy_id
    }
    fn name(&self) -> &str {
        self.policy_name
    }
    fn condition_desc(&self) -> &str {
        self.desc
    }
    fn required_metrics(&self) -> Vec<String> {
        vec![self.metric_key.into()]
    }
    fn evaluate(&self, metrics: &Metrics) -> Vec<String> {
        if metrics.get_str(self.metric_key) == Some(self.expected) {
            vec![self.policy_id.to_string()]
        } else {
            vec![]
        }
    }
}

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

/// 超时策略（ThresholdPolicy 薄包装：`elapsed_secs >= timeout_secs`）
pub struct TimeoutPolicy(ThresholdPolicy);

impl TimeoutPolicy {
    pub fn new(timeout_secs: u64) -> Self {
        Self(ThresholdPolicy::new(
            "timeout",
            "Timeout",
            "elapsed_secs",
            timeout_secs,
            "超时",
            "s",
        ))
    }
}

impl_policy_delegate!(TimeoutPolicy);

/// 上下文溢出策略（ThresholdPolicy 薄包装：`context_tokens >= threshold`）
///
/// threshold = 0 表示未启用（evaluate 恒不命中），允许无条件装配进策略组。
pub struct ContextOverflowPolicy(ThresholdPolicy);

impl ContextOverflowPolicy {
    pub fn new(threshold: u64) -> Self {
        Self(ThresholdPolicy::new(
            "context_overflow",
            "ContextOverflow",
            "context_tokens",
            threshold,
            "上下文溢出",
            "",
        ))
    }
}

impl_policy_delegate!(ContextOverflowPolicy);

/// Final 输出策略（FieldEqualsPolicy 薄包装：`output_kind == "final"`）
///
/// think_loop 的正常退出裁决——「输出为 Final」与轮次/超时/溢出一样
/// 都只是算子计算，命中即退出循环进入下一步（正常总结路径）。
pub struct FinalAnswerPolicy(FieldEqualsPolicy);

impl Default for FinalAnswerPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl FinalAnswerPolicy {
    pub fn new() -> Self {
        Self(FieldEqualsPolicy::new(
            "final_answer",
            "FinalAnswer",
            "output_kind",
            "final",
            "模型输出 Final",
        ))
    }
}

impl_policy_delegate!(FinalAnswerPolicy);

/// LLM 连续失败策略（ThresholdPolicy 薄包装：`llm_consecutive_errors >= budget`）
///
/// think_loop 的重试预算裁决——LLM 调用失败不再直接传播，计入
/// `llm_consecutive_errors` 因子后交由策略评估：预算内立即重试，
/// 命中则传播错误（下游 abort_summary 走无 LLM 兜底存档）。
/// budget = 0 表示不启用（evaluate 恒不命中）。
pub struct ConsecutiveLlmErrorsPolicy(ThresholdPolicy);

impl ConsecutiveLlmErrorsPolicy {
    pub fn new(budget: u64) -> Self {
        Self(ThresholdPolicy::new(
            "llm_error_budget",
            "ConsecutiveLlmErrors",
            "llm_consecutive_errors",
            budget,
            "LLM 连续失败",
            "",
        ))
    }
}

impl_policy_delegate!(ConsecutiveLlmErrorsPolicy);

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

/// Token 预算策略（ThresholdPolicy 薄包装：`total_tokens >= budget`）
///
/// budget = 0 表示不启用（evaluate 恒不命中），允许无条件装配进策略组。
pub struct TokenBudgetPolicy(ThresholdPolicy);

impl TokenBudgetPolicy {
    pub fn new(budget: u64) -> Self {
        Self(ThresholdPolicy::new(
            "token_budget",
            "TokenBudget",
            "total_tokens",
            budget,
            "Token 预算",
            "",
        ))
    }
}

impl_policy_delegate!(TokenBudgetPolicy);

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
            // 折叠嵌套 if（clippy::collapsible_if）
            if let Some(count) = metrics.get_u64(&key)
                && count >= *limit as u64
            {
                return vec![self.id().to_string()];
            }
        }
        vec![]
    }
}
