//! 策略引擎（通用判断框架，不感知业务 action）
//!
//! 设计要点：
//! - Policy trait：evaluate 返回命中的策略 id 列表（空 = 未命中）
//! - Metrics：HashMap 封装，think_loop 每轮构造
//! - PolicyGroup：本身实现 Policy，支持 And/Or 嵌套组合
//! - PolicyBuilder：with + build(And) / or(Or)

pub mod builtin;

use serde::Serialize;
use std::collections::HashMap;

/// 策略 trait（通用判断引擎，不感知业务 action）
///
/// 设计要点：
/// - evaluate 返回命中的策略 id 列表（空 = 未命中，非空 = 命中）
/// - is_triggered 是 trait 级默认方法，基于 evaluate 判断
/// - 策略不响应 action，action 映射由业务侧处理
pub trait Policy: Send + Sync + 'static {
    /// 策略唯一 ID（如 "max_rounds" / "timeout" / "context_overflow"）
    fn id(&self) -> &str;

    /// 策略名称（人类可读，用于前端展示）
    fn name(&self) -> &str;

    /// 策略条件描述（如 "轮次 >= 365"）
    fn condition_desc(&self) -> &str;

    /// 声明关注的算子名称（文档化依赖，开发时可校验 Metrics 是否包含）
    fn required_metrics(&self) -> Vec<String>;

    /// 评估：返回命中的策略 id 列表
    /// 空列表 = 未命中，非空 = 命中（可能多个策略同时命中）
    fn evaluate(&self, metrics: &Metrics) -> Vec<String>;

    /// 默认方法：是否命中（列表非空）
    fn is_triggered(&self, metrics: &Metrics) -> bool {
        !self.evaluate(metrics).is_empty()
    }
}

/// 运行时算子集合（HashMap 封装，灵活传递）
///
/// think_loop 每轮构造，传给策略引擎
/// 策略通过 required_metrics 声明依赖，运行时从 Metrics 中取值
#[derive(Debug, Clone, Default)]
pub struct Metrics {
    data: HashMap<String, serde_json::Value>,
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// 链式添加算子
    pub fn with(mut self, key: &str, value: impl Serialize) -> Self {
        if let Ok(v) = serde_json::to_value(value) {
            self.data.insert(key.to_string(), v);
        }
        self
    }

    pub fn get_u64(&self, key: &str) -> Option<u64> {
        self.data.get(key).and_then(|v| v.as_u64())
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.data.get(key).and_then(|v| v.as_bool())
    }

    pub fn get_f64(&self, key: &str) -> Option<f64> {
        self.data.get(key).and_then(|v| v.as_f64())
    }
}

/// 策略组关系
pub enum PolicyRelation {
    /// 所有子策略都命中才命中（默认）
    And,
    /// 任一子策略命中就命中
    Or,
}

/// 策略组：本身实现 Policy，可嵌套组合
///
/// 所有派生字段（id / name / condition_desc / required_metrics）从子策略自动拼接生成。
pub struct PolicyGroup {
    id: String,
    name: String,
    condition_desc: String,
    required_metrics: Vec<String>,
    policies: Vec<Box<dyn Policy>>,
    relation: PolicyRelation,
}

impl PolicyGroup {
    pub fn new(policies: Vec<Box<dyn Policy>>, relation: PolicyRelation) -> Self {
        let connector = match relation {
            PolicyRelation::And => "And",
            PolicyRelation::Or => "Or",
        };
        let id_connector = match relation {
            PolicyRelation::And => "and",
            PolicyRelation::Or => "or",
        };

        let id = format!(
            "{}({})",
            id_connector,
            policies
                .iter()
                .map(|p| p.id())
                .collect::<Vec<_>>()
                .join(",")
        );
        let name = format!(
            "{}[{}]",
            connector,
            policies
                .iter()
                .map(|p| p.name())
                .collect::<Vec<_>>()
                .join(", ")
        );
        let condition_desc = policies
            .iter()
            .map(|p| format!("({})", p.condition_desc()))
            .collect::<Vec<_>>()
            .join(format!(" {} ", connector).as_str());

        // 子策略 required_metrics 去重合并
        let mut required_metrics = Vec::new();
        for p in &policies {
            for m in p.required_metrics() {
                if !required_metrics.contains(&m) {
                    required_metrics.push(m);
                }
            }
        }

        Self {
            id,
            name,
            condition_desc,
            required_metrics,
            policies,
            relation,
        }
    }
}

impl Policy for PolicyGroup {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn condition_desc(&self) -> &str {
        &self.condition_desc
    }
    fn required_metrics(&self) -> Vec<String> {
        self.required_metrics.clone()
    }

    fn evaluate(&self, metrics: &Metrics) -> Vec<String> {
        match self.relation {
            PolicyRelation::And => {
                // 所有子策略都命中才返回合并列表
                let mut all_hits = Vec::new();
                for p in &self.policies {
                    let hits = p.evaluate(metrics);
                    if hits.is_empty() {
                        return Vec::new(); // 任一未命中 → 整体未命中
                    }
                    all_hits.extend(hits);
                }
                all_hits
            }
            PolicyRelation::Or => {
                // 合并所有命中的子策略 id
                self.policies
                    .iter()
                    .flat_map(|p| p.evaluate(metrics))
                    .collect()
            }
        }
    }
}

/// 策略构造器（通用，with + build/or）
///
/// build 默认 And 关系，or 方法构造 Or 关系
/// 单个 policy 直接返回，多个构造 PolicyGroup
pub struct PolicyBuilder {
    policies: Vec<Box<dyn Policy>>,
}

impl PolicyBuilder {
    pub fn new() -> Self {
        Self {
            policies: Vec::new(),
        }
    }

    /// 注入具体 policy（接受已装箱的策略）
    pub fn with(mut self, policy: Box<dyn Policy>) -> Self {
        self.policies.push(policy);
        self
    }

    /// 注入具体 policy（泛型便捷方法，自动装箱，消除 Box::new 样板）
    pub fn with_policy<P: Policy + 'static>(mut self, policy: P) -> Self {
        self.policies.push(Box::new(policy));
        self
    }

    /// 构造策略组（默认 And 关系）
    pub fn build(self) -> Box<dyn Policy> {
        match self.policies.len() {
            0 => panic!("PolicyBuilder requires at least one policy"),
            1 => self.policies.into_iter().next().unwrap(),
            _ => Box::new(PolicyGroup::new(self.policies, PolicyRelation::And)),
        }
    }

    /// 构造 Or 关系策略组
    pub fn or(self) -> Box<dyn Policy> {
        match self.policies.len() {
            0 => panic!("PolicyBuilder requires at least one policy"),
            1 => self.policies.into_iter().next().unwrap(),
            _ => Box::new(PolicyGroup::new(self.policies, PolicyRelation::Or)),
        }
    }
}

impl Default for PolicyBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// 策略集合构建宏
///
/// 一步完成"策略初始化 + 组装 + 关系指定"，消除 `Box::new(XxxPolicy::new(...))` 样板。
///
/// 约定：内置策略通过 `::new` 构造，宏自动调用 `$Policy::new(args...)`。
/// 特殊构造场景请直接使用 `PolicyBuilder::with_policy`。
///
/// # 三种模式
///
/// ## 1. 纯 OR/AND（所有策略同一关系）
///
/// ```rust,ignore
/// // 任一命中即触发
/// let policy = policy_set! {
///     OR {
///         UserCancelPolicy(cancel_flag),
///         MaxRoundsPolicy(max_rounds),
///     }
/// };
///
/// // 全部命中才触发
/// let policy = policy_set! {
///     AND {
///         TokenBudgetPolicy(10000),
///         MaxRoundsPolicy(50),
///     }
/// };
/// ```
///
/// ## 2. 混合模式（平铺策略 + OR/AND 子组，外层默认 AND）
///
/// 平铺的策略参与外层 AND 组合，`OR {}` / `AND {}` 子组在内部按指定关系组合。
/// 适用于"大部分策略是 AND，但其中几个是 OR"的场景。
///
/// ```rust,ignore
/// // 等价于：MaxRounds AND Timeout AND (UserCancel OR TokenBudget)
/// let policy = policy_set! {
///     MaxRoundsPolicy(max_rounds),
///     TimeoutPolicy(timeout_secs),
///     OR {
///         UserCancelPolicy(cancel_flag),
///         TokenBudgetPolicy(10000),
///     }
/// };
/// ```
macro_rules! policy_set {
    // 纯 OR {} 模式
    (OR { $($policy:ident ( $($arg:expr),* $(,)? ) ),* $(,)? }) => {{
        let mut builder = $crate::pkg::policy::PolicyBuilder::new();
        $(
            builder = builder.with_policy($policy::new($($arg),*));
        )*
        builder.or()
    }};

    // 纯 AND {} 模式
    (AND { $($policy:ident ( $($arg:expr),* $(,)? ) ),* $(,)? }) => {{
        let mut builder = $crate::pkg::policy::PolicyBuilder::new();
        $(
            builder = builder.with_policy($policy::new($($arg),*));
        )*
        builder.build()
    }};

    // 混合模式入口：平铺策略 + OR/AND 子组，外层 AND 组合
    ( $($rest:tt)+ ) => {{
        let mut builder = $crate::pkg::policy::PolicyBuilder::new();
        $crate::pkg::policy::policy_set_mixed!(@accum builder; $($rest)+)
    }};
}

/// 混合模式递归辅助宏（TT munching）
///
/// 逐条处理条目，每条匹配后递归处理剩余条目。
/// 通过 `@accum $builder:ident` 传递累积的 builder 状态。
macro_rules! policy_set_mixed {
    // 终止：无剩余条目
    (@accum $builder:ident;) => {
        $builder.build()
    };

    // OR {} 子组 + 可选后续
    (@accum $builder:ident; OR { $($p:ident ( $($a:expr),* $(,)? ) ),* $(,)? } $(, $($rest:tt)*)?) => {{
        let sub = $crate::pkg::policy::policy_set!(OR { $($p ( $($a),* ) ),* });
        $builder = $builder.with(sub);
        $crate::pkg::policy::policy_set_mixed!(@accum $builder; $($($rest)*)?)
    }};

    // AND {} 子组 + 可选后续
    (@accum $builder:ident; AND { $($p:ident ( $($a:expr),* $(,)? ) ),* $(,)? } $(, $($rest:tt)*)?) => {{
        let sub = $crate::pkg::policy::policy_set!(AND { $($p ( $($a),* ) ),* });
        $builder = $builder.with(sub);
        $crate::pkg::policy::policy_set_mixed!(@accum $builder; $($($rest)*)?)
    }};

    // 平铺策略 + 可选后续
    (@accum $builder:ident; $p:ident ( $($a:expr),* $(,)? ) $(, $($rest:tt)*)?) => {{
        $builder = $builder.with_policy($p::new($($a),*));
        $crate::pkg::policy::policy_set_mixed!(@accum $builder; $($($rest)*)?)
    }};
}

pub(crate) use policy_set;
pub(crate) use policy_set_mixed;

#[cfg(test)]
mod tests;
