//! 图引擎策略集：把「准入条件」编译成 `pkg::policy::Policy`
//!
//! 为什么不自己再写一套谓词引擎：
//! - 准入判断与 `think_loop` 的退出判断、shell 拦截、消息路由**同构**：
//!   都是「读一组运行时算子 → 命中 / 不命中」。
//! - 复用 `Policy` 白送 `id / name / condition_desc / required_metrics`，
//!   其中 `condition_desc` 正是「为什么没走到这个节点」的可解释性来源——
//!   这是动态路由最缺、也最容易事后补不上的东西。
//!
//! ⚠️ 语义约定：准入策略一律**正向**表述（命中 = 该条准入成立）。
//! `pkg::policy` 没有 Not 组合子，所以「未达上限」「不在冷却期内」都写成正向条件，
//! 由 `ActivationGuard`（And 语义）合成为整体准入。不要让某条策略表达「拒绝」，
//! 否则 And 组合的语义会反直觉。

use super::node::{ActivationBudget, BusPredicate};
use super::run::ThinkRun;
use super::types::{ContextKey, LoadDepth, NodeId, NodeSignal};
use crate::pkg::policy::{Metrics, Policy};
use serde_json::Value;

/// 本次 run 已走步数
pub const M_STEPS: &str = "steps";
/// 距 `max_steps` 的剩余步数
pub const M_STEPS_LEFT: &str = "steps_left";
/// 该候选节点已激活次数
pub const M_ACTIVATIONS: &str = "activations";
/// 距该节点上次激活的步数；从未激活为 `u64::MAX`
pub const M_SINCE_LAST: &str = "since_last";
/// 上一个执行的节点 id（首步为空串）
pub const M_PREV_NODE: &str = "prev_node";
/// 上一个节点产出的信号标签（首步为空串）
pub const M_SIGNAL: &str = "signal";
/// 该节点的激活次数上限
pub const M_MAX_ACTIVATIONS: &str = "budget.max_activations";
/// 该节点的激活冷却步数
pub const M_COOLDOWN_STEPS: &str = "budget.cooldown_steps";

/// 总线分区是否已加载：`bus.<key>.loaded`（0 / 1）
pub fn bus_loaded_key(key: ContextKey) -> String {
    format!("bus.{}.loaded", key.as_str())
}

/// 总线分区已达深度：`bus.<key>.depth`（0 = 未加载，1 = Shallow，2 = Full）
pub fn bus_depth_key(key: ContextKey) -> String {
    format!("bus.{}.depth", key.as_str())
}

/// 总线布尔字段：`flag.<key>#<json pointer>`
///
/// 用 `#` 而非 `/` 分隔指针，避免与指针自身的 `/` 混淆。
pub fn flag_key(key: ContextKey, pointer: &str) -> String {
    format!("flag.{}#{}", key.as_str(), pointer)
}

fn depth_rank(depth: LoadDepth) -> u64 {
    match depth {
        LoadDepth::Shallow => 1,
        LoadDepth::Full => 2,
    }
}

/// 准入策略求值时能看到的全部运行时事实
///
/// 刻意**不**持有 `&mut`：策略是纯函数、同步、只读，只能读这里投影出的算子。
pub struct GuardCtx<'a> {
    pub run: &'a ThinkRun,
    /// 正在判定是否可激活的候选节点
    pub node: &'a NodeId,
    pub last: Option<(&'a NodeId, &'a NodeSignal)>,
    pub budget: &'a ActivationBudget,
}

impl<'a> GuardCtx<'a> {
    fn steps(&self) -> u64 {
        self.run.trace.steps as u64
    }
}

/// 投影单个算子；未知 key 返回 `None`（等价于「该算子不存在」）
pub fn project(ctx: &GuardCtx<'_>, key: &str) -> Option<Value> {
    let value: Value = match key {
        M_STEPS => ctx.steps().into(),
        M_STEPS_LEFT => ctx
            .run
            .blueprint
            .limits
            .max_steps
            .saturating_sub(ctx.run.trace.steps)
            .into(),
        M_ACTIVATIONS => (ctx.run.trace.count(ctx.node) as u64).into(),
        M_SINCE_LAST => match ctx.run.trace.last_step_of(ctx.node) {
            Some(prev) => ctx.steps().saturating_sub(prev as u64).into(),
            None => u64::MAX.into(),
        },
        M_PREV_NODE => ctx.last.map(|(prev, _)| prev.as_str()).unwrap_or("").into(),
        M_SIGNAL => ctx
            .last
            .map(|(_, signal)| signal.tag())
            .unwrap_or("")
            .into(),
        M_MAX_ACTIVATIONS => (ctx.budget.max_activations as u64).into(),
        M_COOLDOWN_STEPS => (ctx.budget.cooldown_steps as u64).into(),
        other => {
            if let Some(rest) = other.strip_prefix("bus.")
                && let Some((key_text, suffix)) = rest.rsplit_once('.')
                && let Some(bus_key) = ContextKey::parse(key_text)
            {
                match suffix {
                    "loaded" => ctx.run.bus.depth_of(bus_key).is_some().into(),
                    "depth" => match ctx.run.bus.depth_of(bus_key) {
                        Some(depth) => depth_rank(depth).into(),
                        None => 0u64.into(),
                    },
                    _ => return None,
                }
            } else if let Some(rest) = other.strip_prefix("flag.")
                && let Some((key_text, pointer)) = rest.split_once('#')
                && let Some(bus_key) = ContextKey::parse(key_text)
            {
                ctx.run
                    .bus
                    .get(bus_key)
                    .and_then(|v| v.pointer(pointer))?
                    .clone()
            } else {
                return None;
            }
        }
    };
    Some(value)
}

/// 按策略声明的 `required_metrics` **惰性**构造算子表
///
/// 每次仲裁对每个候选都要构造一张表，全量投影会随总线分区增长变贵；
/// 只投影真正被声明的 key，成本与策略数量而非总线规模挂钩。
pub fn metrics_for(ctx: &GuardCtx<'_>, required: &[String]) -> Metrics {
    let mut metrics = Metrics::new();
    for key in required {
        if let Some(value) = project(ctx, key) {
            metrics = metrics.with(key, value);
        }
    }
    metrics
}

/// 「字符串算子 ∈ 允许集合」：前驱节点 / 信号标签两个场景共用
pub struct InSetPolicy {
    policy_id: String,
    policy_name: String,
    condition: String,
    metric: &'static str,
    allowed: Vec<String>,
}

impl InSetPolicy {
    fn new(
        policy_id: String,
        policy_name: String,
        metric: &'static str,
        allowed: Vec<String>,
    ) -> Self {
        let condition = format!("{metric} ∈ [{}]", allowed.join(", "));
        Self {
            policy_id,
            policy_name,
            condition,
            metric,
            allowed,
        }
    }

    /// 允许的前驱节点（空集表示不约束，调用方不应构造）
    pub fn after(node: &NodeId, allowed: Vec<NodeId>) -> Self {
        Self::new(
            format!("{}.after", node.as_str()),
            "前驱节点准入".to_string(),
            M_PREV_NODE,
            allowed.iter().map(|id| id.as_str().to_string()).collect(),
        )
    }

    /// 允许的前驱信号
    pub fn signal(node: &NodeId, allowed: Vec<String>) -> Self {
        Self::new(
            format!("{}.signal", node.as_str()),
            "前驱信号准入".to_string(),
            M_SIGNAL,
            allowed,
        )
    }
}

impl Policy for InSetPolicy {
    fn id(&self) -> &str {
        &self.policy_id
    }
    fn name(&self) -> &str {
        &self.policy_name
    }
    fn condition_desc(&self) -> &str {
        &self.condition
    }
    fn required_metrics(&self) -> Vec<String> {
        vec![self.metric.to_string()]
    }
    fn evaluate(&self, metrics: &Metrics) -> Vec<String> {
        let actual = metrics.get_str(self.metric).unwrap_or("");
        if self.allowed.iter().any(|item| item == actual) {
            vec![self.policy_id.clone()]
        } else {
            Vec::new()
        }
    }
}

/// 总线谓词策略：`BusPredicate` 的策略化包装
pub struct BusPredicatePolicy {
    policy_id: String,
    policy_name: String,
    condition: String,
    metric: String,
    expect: Value,
}

impl BusPredicatePolicy {
    pub fn from_predicate(predicate: &BusPredicate) -> Self {
        let (metric, expect, name, condition) = match predicate {
            BusPredicate::KeyLoaded(key) => (
                bus_loaded_key(*key),
                Value::Bool(true),
                format!("{} 已加载", key.as_str()),
                format!("{} 已加载", key.as_str()),
            ),
            BusPredicate::KeyAtLeast(key, depth) => (
                bus_depth_key(*key),
                depth_rank(*depth).into(),
                format!("{} 深度达标", key.as_str()),
                format!("{} 深度 >= {:?}", key.as_str(), depth),
            ),
            BusPredicate::FlagEq { key, path, expect } => (
                flag_key(*key, path),
                Value::Bool(*expect),
                format!("{}{} == {}", key.as_str(), path, expect),
                format!("{}{} == {}", key.as_str(), path, expect),
            ),
        };
        Self {
            policy_id: format!("bus.{metric}"),
            policy_name: name,
            condition,
            metric,
            expect,
        }
    }
}

impl Policy for BusPredicatePolicy {
    fn id(&self) -> &str {
        &self.policy_id
    }
    fn name(&self) -> &str {
        &self.policy_name
    }
    fn condition_desc(&self) -> &str {
        &self.condition
    }
    fn required_metrics(&self) -> Vec<String> {
        vec![self.metric.clone()]
    }
    fn evaluate(&self, metrics: &Metrics) -> Vec<String> {
        let hit = match &self.expect {
            Value::Bool(want) => metrics.get_bool(&self.metric) == Some(*want),
            Value::Number(want) => {
                metrics.get_u64(&self.metric) == Some(want.as_u64().unwrap_or(0))
            }
            _ => false,
        };
        if hit {
            vec![self.policy_id.clone()]
        } else {
            Vec::new()
        }
    }
}

/// 激活预算：次数未达上限 **且** 不在冷却期内
///
/// 这是图 + 回环下**终止性**的唯一护栏，不是可关的配置项。
pub struct BudgetPolicy {
    policy_id: String,
    condition: String,
    max_activations: usize,
    cooldown_steps: usize,
}

impl BudgetPolicy {
    pub fn new(node: &NodeId, budget: &ActivationBudget) -> Self {
        Self {
            policy_id: format!("{}.budget", node.as_str()),
            condition: format!(
                "activations < {} 且 since_last > {}",
                budget.max_activations, budget.cooldown_steps
            ),
            max_activations: budget.max_activations,
            cooldown_steps: budget.cooldown_steps,
        }
    }
}

impl Policy for BudgetPolicy {
    fn id(&self) -> &str {
        &self.policy_id
    }
    fn name(&self) -> &str {
        "激活预算"
    }
    fn condition_desc(&self) -> &str {
        &self.condition
    }
    fn required_metrics(&self) -> Vec<String> {
        vec![
            M_ACTIVATIONS.to_string(),
            M_SINCE_LAST.to_string(),
            M_MAX_ACTIVATIONS.to_string(),
            M_COOLDOWN_STEPS.to_string(),
        ]
    }
    fn evaluate(&self, metrics: &Metrics) -> Vec<String> {
        let activations = metrics.get_u64(M_ACTIVATIONS).unwrap_or(0);
        let since_last = metrics.get_u64(M_SINCE_LAST).unwrap_or(u64::MAX);
        // 上限以算子里的实际值为准，构建期预算只作兜底：
        // 蓝图覆盖 / 策略补丁调整预算时，判定口径与算子表保持一致（单一真相）
        let max_activations = metrics
            .get_u64(M_MAX_ACTIVATIONS)
            .unwrap_or(self.max_activations as u64);
        let cooldown_steps = metrics
            .get_u64(M_COOLDOWN_STEPS)
            .unwrap_or(self.cooldown_steps as u64);
        if activations >= max_activations {
            return Vec::new();
        }
        if cooldown_steps > 0 && since_last <= cooldown_steps {
            return Vec::new();
        }
        vec![self.policy_id.clone()]
    }
}

/// 准入守卫：一组正向条件的 And 组合
///
/// 语义与 `PolicyGroup(And)` 一致，额外提供 `rejections()`——
/// 逐条评估并给出未命中项的自然语言描述。动态路由下「为什么没走到这一步」
/// 只能靠它回答，事后补不上。
pub struct ActivationGuard {
    policies: Vec<Box<dyn Policy>>,
    desc: String,
}

impl ActivationGuard {
    pub fn and(policies: Vec<Box<dyn Policy>>) -> Self {
        let desc = if policies.is_empty() {
            "无条件".to_string()
        } else {
            policies
                .iter()
                .map(|policy| format!("({})", policy.condition_desc()))
                .collect::<Vec<_>>()
                .join(" 且 ")
        };
        Self { policies, desc }
    }

    pub fn is_empty(&self) -> bool {
        self.policies.is_empty()
    }

    /// 整体是否准入
    pub fn is_admitted(&self, ctx: &GuardCtx<'_>) -> bool {
        self.is_triggered(&metrics_for(ctx, &self.required_metrics()))
    }

    /// 未准入原因：返回所有未命中条件的 `condition_desc`
    ///
    /// 准入时返回空列表。
    pub fn rejections(&self, ctx: &GuardCtx<'_>) -> Vec<String> {
        let metrics = metrics_for(ctx, &self.required_metrics());
        self.policies
            .iter()
            .filter(|policy| policy.evaluate(&metrics).is_empty())
            .map(|policy| format!("{}：{}", policy.name(), policy.condition_desc()))
            .collect()
    }

    /// 准入条件的完整描述（进 `RouterView`，供大小脑判断候选是否可信）
    pub fn describe(&self) -> String {
        self.condition_desc().to_string()
    }
}

impl Policy for ActivationGuard {
    fn id(&self) -> &str {
        "activation_guard"
    }
    fn name(&self) -> &str {
        "节点准入"
    }
    fn condition_desc(&self) -> &str {
        &self.desc
    }
    fn required_metrics(&self) -> Vec<String> {
        let mut keys: Vec<String> = Vec::new();
        for policy in &self.policies {
            for metric in policy.required_metrics() {
                if !keys.contains(&metric) {
                    keys.push(metric);
                }
            }
        }
        keys
    }
    fn evaluate(&self, metrics: &Metrics) -> Vec<String> {
        let mut hits = Vec::new();
        for policy in &self.policies {
            let hit = policy.evaluate(metrics);
            if hit.is_empty() {
                return Vec::new();
            }
            hits.extend(hit);
        }
        hits
    }
}

/// 把节点声明的准入规则编译成守卫
///
/// 空集合表示「不约束该项」，因此不生成对应策略（而不是生成一个恒假的策略）。
pub fn compile_guard(
    node: &NodeId,
    rule: &super::node::ActivationRule,
    budget: &ActivationBudget,
) -> ActivationGuard {
    let mut policies: Vec<Box<dyn Policy>> = Vec::new();
    if !rule.after.is_empty() {
        policies.push(Box::new(InSetPolicy::after(node, rule.after.clone())));
    }
    if !rule.on.is_empty() {
        policies.push(Box::new(InSetPolicy::signal(node, rule.on.clone())));
    }
    for predicate in &rule.when {
        policies.push(Box::new(BusPredicatePolicy::from_predicate(predicate)));
    }
    policies.push(Box::new(BudgetPolicy::new(node, budget)));
    ActivationGuard::and(policies)
}
