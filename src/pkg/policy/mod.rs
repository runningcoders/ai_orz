//! 策略引擎（通用判断框架，不感知业务语义）
//!
//! 设计要点：
//! - [`Policy`]：evaluate 返回命中的策略 id 列表（空 = 未命中）
//! - [`Metrics`]：HashMap 封装的本轮事实桶，每轮构造
//! - `PolicyGroup`：本身实现 Policy，支持 And/Or 嵌套组合
//! - `action()`：命中后可选携带的通用处置动作（Deny/Confirm/Audit），
//!   默认 None = 策略仅输出命中、动作由业务侧映射（原行为）；
//!   新领域（如 shell 拦截）可覆写 action() 声明式携带动作。
//!
//! 策略是分方向的：**同一方向上的策略共享一套骨架**，不同方向各自扩展，
//! 但都长在 [`Policy`] 这棵树上，而不是彼此无关的平行 trait。
//! 当前唯一的分支是「需要外部决策」：[`DecisionPolicy`] 继承 [`Policy`]，
//! 把「问什么 → 读答案 → 判命中」固定成默认实现，场景只填三个点。
//! 决策契约（请求/答案的数据形状 + 事实桶）在本模块的 [`decision`] 子模块，
//! 执行在 DAL。
//!
//! 为什么契约不单独开一个 `pkg::decision` 包：`Metrics` 本身就是策略的输入桶
//! （`Policy::evaluate(&Metrics)`），决策契约也只有策略和它的执行方（DAL）在认。
//! 拆成同级包会让同一个概念分居两个目录，且多出一条 `policy → decision` 的
//! 包内依赖——合并成子模块后对外路径仍是 `pkg::policy::{DecisionRequest, Metrics}`。

pub mod builtin;
pub mod decision;
pub mod forms;

pub use decision::{
    DecisionAnswers, DecisionRequest, DecisionValue, Grade, Metrics, PickOne, QuestionSpec,
};
pub use forms::{
    Candidate, KEY_CANDIDATES, KEY_QUESTION, PickOnePolicy, PresencePolicy, RankPolicy, ScorePolicy,
};

/// 引擎级通用处置动作（命中后可选携带）
///
/// 引擎只认识这三个词，不感知具体业务语义（如 shell 拦截、think_loop 终止）；
/// 业务侧可选择忽略 action 自行按命中 id 映射（向后兼容）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyAction {
    /// 拒绝执行（携带给调用方的提示文案）
    Deny(String),
    /// 需要用户确认后才可放行（携带确认原因）
    Confirm(String),
    /// 放行但记录审计（携带审计说明）
    Audit(String),
}

impl PolicyAction {
    /// 动作是否阻断执行（Deny/Confirm 需要短路，Audit 放行）
    pub fn is_blocking(&self) -> bool {
        matches!(self, PolicyAction::Deny(_) | PolicyAction::Confirm(_))
    }

    /// 动作携带的说明文案
    pub fn reason(&self) -> &str {
        match self {
            PolicyAction::Deny(r) | PolicyAction::Confirm(r) | PolicyAction::Audit(r) => r,
        }
    }
}

/// 策略 trait（通用判断引擎，不感知业务语义）
///
/// 设计要点：
/// - evaluate 返回命中的策略 id 列表（空 = 未命中，非空 = 命中）
/// - is_triggered 是 trait 级默认方法，基于 evaluate 判断
/// - action 是 trait 级默认方法（默认 None）：策略可选择命中后携带
///   通用处置动作；未覆写时动作映射仍由业务侧处理（向后兼容）
/// - `prepare` 是可选能力：策略若需外部决策，覆写它声明请求
///   （默认空 = 纯本地可算）。需要外部决策的策略**不要直接实现本 trait**，
///   而是实现 [`DecisionPolicy`] —— 那是本 trait 在「做决策」方向上的分支，
///   骨架（prepare/evaluate）已经写好，只需填场景相关部分。
pub trait Policy: Send + Sync + 'static {
    /// 策略唯一 ID（如 "max_rounds" / "timeout" / "context_overflow"）
    fn id(&self) -> &str;

    /// 策略名称（人类可读，用于前端展示）
    fn name(&self) -> &str;

    /// 策略条件描述（如 "轮次 >= 365"）
    fn condition_desc(&self) -> &str;

    /// 声明关注的算子名称（文档化依赖，开发时可校验 Metrics 是否包含）
    fn required_metrics(&self) -> Vec<String>;

    /// 阶段一（可选）：声明本次判断需要的外部决策请求
    ///
    /// 默认返回空 = 纯本地可算（内置阈值策略零改动）。
    /// 决策型策略在此声明「我要问什么」，编排层收集后批量执行，
    /// 答案写入 `Metrics::decisions`，再由 `evaluate` 消费。
    ///
    /// ⚠️ 本方法**不执行任何 I/O**：执行发生在编排层（唯一 async 点）。
    /// 策略在这里只产出契约，因此仍可纯函数单测。
    fn prepare(&self, metrics: &Metrics) -> Vec<DecisionRequest> {
        let _ = metrics;
        vec![]
    }

    /// 评估：返回命中的策略 id 列表
    /// 空列表 = 未命中，非空 = 命中（可能多个策略同时命中）
    fn evaluate(&self, metrics: &Metrics) -> Vec<String>;

    /// 默认方法：是否命中（列表非空）
    fn is_triggered(&self, metrics: &Metrics) -> bool {
        !self.evaluate(metrics).is_empty()
    }

    /// 默认方法：命中后建议的处置动作（可选能力）
    ///
    /// 默认返回 None = 不携带动作，业务侧按命中 id 自行映射（原行为）；
    /// 声明式策略可覆写此方法返回通用动作。调用方应配合
    /// `is_triggered`/`evaluate` 使用：仅在命中时解释 action。
    fn action(&self, metrics: &Metrics) -> Option<PolicyAction> {
        let _ = metrics;
        None
    }
}

/// 决策型策略：通用策略在「需要外部决策」这一方向上的分支
///
/// 它把一次外部决策的完整生命周期收拢成骨架，场景只回答三件事：
///
/// 1. [`scene`](DecisionPolicy::scene)：我是谁（日志/计量口径，也是 qid 命名空间）
/// 2. [`request`](DecisionPolicy::request)：这一轮要问什么（纯函数，无 I/O）
/// 3. [`judge`](DecisionPolicy::judge)：答案落到我身上算不算命中（纯函数）
///
/// 其余都由默认实现完成：
/// - `prepare`（[`decision_prepare`](DecisionPolicy::decision_prepare)）：
///   把 `request` 的可选结果摊平成批次；
/// - `evaluate`（[`decision_evaluate`](DecisionPolicy::decision_evaluate)）：
///   没拿到答案 → 未命中，拿到了 → 交给 `judge`；
/// - [`pick_ranked`](DecisionPolicy::pick_ranked)：一次 Pick 的全选项概率
///   按阈值 + 上限截成保留集（「重排」型场景的通用取值逻辑）。
///
/// 💡 实现者仍须写 `impl Policy`（trait 继承不继承默认体），
/// 把 `prepare` / `evaluate` 两行转发到骨架即可，描述类方法按场景填。
/// 这样所有「做决策」的策略都在这一个分支下，既能被 `PolicyGroup` 组合、
/// 也能被编排层统一用 `prepare → decide → evaluate` 驱动。
pub trait DecisionPolicy: Policy {
    /// 场景标识（日志/计量口径，也是 qid 命名空间前缀）
    ///
    /// ⚠️ qid 必须带场景前缀（如 `memory_relevance.primary`）：
    /// 所有场景的答案落在同一张表里，裸 id 会跨场景静默覆盖。
    fn scene(&self) -> &'static str;

    /// 构造本轮决策请求；`None` = 本轮无需决策（一次调用都不发）
    ///
    /// ⚠️ 纯函数：不执行 I/O。执行发生在编排层（唯一 async 点）。
    fn request(&self, metrics: &Metrics) -> Option<DecisionRequest>;

    /// 答案 → 命中信号（纯函数）
    ///
    /// 只有确实拿到答案时才会被调用（空答案由骨架拦截），
    /// 因此这里可以放心读 `answers`；但仍需区分题型/字段是否可用。
    fn judge(&self, metrics: &Metrics, answers: &DecisionAnswers) -> Vec<String>;

    /// 场景内的题 id（统一加前缀，避免跨场景撞名字）
    fn qid(&self, name: &str) -> String {
        format!("{}.{}", self.scene(), name)
    }

    /// 主问题 id：`request` 与取值逻辑共用同一道题
    fn primary_qid(&self) -> String {
        self.qid("primary")
    }

    /// 骨架①：把场景请求摊平成批次（`Policy::prepare` 的转发目标）
    fn decision_prepare(&self, metrics: &Metrics) -> Vec<DecisionRequest> {
        self.request(metrics).into_iter().collect()
    }

    /// 骨架②：答案注入后的判定（`Policy::evaluate` 的转发目标）
    ///
    /// 「没问」与「问了没答」都算未命中，绝不压成「判断为否」。
    fn decision_evaluate(&self, metrics: &Metrics) -> Vec<String> {
        match metrics.decisions() {
            Some(answers) if !answers.is_empty() => self.judge(metrics, answers),
            _ => Vec::new(),
        }
    }

    /// 形态读取①：单选结论（置信度达标才算拿到）
    ///
    /// 与 [`pick_ranked`](Self::pick_ranked) 的分工：那个要「全序」，这个只要「选中项」。
    /// `None` = 没答 / 形态不符 / 置信度不足，调用方按「没有判断」处理。
    fn pick_one(&self, metrics: &Metrics, min_confidence: f32) -> Option<PickOne> {
        match metrics.decision(&self.primary_qid())? {
            DecisionValue::Pick {
                chosen, confidence, ..
            } if *confidence >= min_confidence => Some(PickOne {
                option: chosen.clone(),
                confidence: *confidence,
            }),
            _ => None,
        }
    }

    /// 形态读取②：分级结论（分数 + 置信度）
    ///
    /// `None` = 没答 / 形态不符 / 置信度不足。
    fn grade(&self, metrics: &Metrics, min_confidence: f32) -> Option<Grade> {
        match metrics.decision(&self.primary_qid())? {
            DecisionValue::Grade { score, confidence } if *confidence >= min_confidence => {
                Some(Grade {
                    score: *score,
                    confidence: *confidence,
                })
            }
            _ => None,
        }
    }

    /// 形态读取③：存在性概率（`noul` 不带置信度，所以只给阈值判断）
    fn presence(&self, metrics: &Metrics) -> Option<f32> {
        match metrics.decision(&self.primary_qid())? {
            DecisionValue::Presence { value } => Some(*value),
            _ => None,
        }
    }

    /// 骨架③：一次 Pick 的全选项概率 → 保留集（按相关性降序）
    ///
    /// - `options`：合法选项 id，**顺序即语义序**（同分兜底 + 幻觉过滤）
    /// - `threshold`：低于此权重视为不相关
    /// - `keep`：保留上限
    ///
    /// 返回 `None` = 答案不可用（没答 / 不是 Pick / 权重表为空 / 无合法 id），
    /// 调用方必须退回老路径，不得当成「全部不相关」。
    /// 被模型选中的那条**无条件保留**：整批权重都偏低时也不清空召回。
    fn pick_ranked(
        &self,
        metrics: &Metrics,
        options: &[String],
        threshold: f32,
        keep: usize,
    ) -> Option<Vec<String>> {
        let qid = self.primary_qid();
        let Some(DecisionValue::Pick {
            chosen, weights, ..
        }) = metrics.decision(&qid)
        else {
            return None;
        };
        if weights.is_empty() || keep == 0 {
            return None;
        }

        // 模型可能编造 id：只认选项集内的，顺序按选项原序兜底
        let rank_of: std::collections::HashMap<&str, usize> = options
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect();
        let mut ranked: Vec<(&String, f32)> = weights
            .iter()
            .filter(|(id, _)| rank_of.contains_key(id.as_str()))
            .map(|(id, w)| (id, *w))
            .collect();
        if ranked.is_empty() {
            return None;
        }
        ranked.sort_by(|a, b| {
            let by_weight = b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal);
            let pos = |id: &str| rank_of.get(id).copied().unwrap_or(usize::MAX);
            by_weight.then_with(|| pos(a.0.as_str()).cmp(&pos(b.0.as_str())))
        });

        let mut kept: Vec<String> = Vec::new();
        for (id, weight) in ranked {
            if kept.len() >= keep {
                break;
            }
            if weight >= threshold || id.as_str() == chosen.as_str() {
                kept.push(id.clone());
            }
        }
        (!kept.is_empty()).then_some(kept)
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
    /// 按关系组合一组策略
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

    /// 收集全部子策略的决策请求
    ///
    /// 与 And / Or 关系无关：每个子判断都需要自己的资料，
    /// 组合语义只在命中合并阶段生效（`evaluate` 已天然处理）。
    fn prepare(&self, metrics: &Metrics) -> Vec<DecisionRequest> {
        self.policies
            .iter()
            .flat_map(|p| p.prepare(metrics))
            .collect()
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

    /// 组合动作：子策略按声明顺序取首个 Some（仅在被命中时解释）
    /// - And：全部命中时取首个 Some；任一未命中 → None
    /// - Or：按子策略顺序，首个「命中且携带动作」的 Some 上浮
    fn action(&self, metrics: &Metrics) -> Option<PolicyAction> {
        match self.relation {
            PolicyRelation::And => {
                let all_hit = self
                    .policies
                    .iter()
                    .all(|p| !p.evaluate(metrics).is_empty());
                if !all_hit {
                    return None;
                }
                self.policies.iter().find_map(|p| p.action(metrics))
            }
            PolicyRelation::Or => self.policies.iter().find_map(|p| {
                if p.evaluate(metrics).is_empty() {
                    None
                } else {
                    p.action(metrics)
                }
            }),
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
    /// 空构造器
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
#[allow(unused_macros)]
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
#[allow(unused_imports)]
pub(crate) use policy_set_mixed;

#[cfg(test)]
mod tests;
