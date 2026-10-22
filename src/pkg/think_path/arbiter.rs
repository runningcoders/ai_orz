//! 仲裁器：决定「下一步跑谁」
//!
//! 分三层，成本递增，**只有最贵那层才可能调 LLM**：
//!
//! | 层 | 干什么 | 成本 |
//! |----|--------|------|
//! | L0 合法化 | 按准入规则 + 预算算出**合法候选集** | 零 |
//! | L1 唯一化 | 候选唯一 → 直接选（覆盖绝大多数流转） | 零 |
//! | L2 顾问 | 候选多个 → 构造 `RouterView` 问小脑/大脑 | 一次 LLM |
//!
//! ⚠️ **顾问只「选」，不「定」**：它只能在 L0 给出的合法候选集里挑，
//! 越界即降级为确定性选择。终止性因此仍由 L0 的 `ActivationBudget` 保证——
//! 这是把 LLM 的概率性关进笼子的关键，否则图 + LLM 会变成无法证明会停的东西。

use super::blueprint::{PathLimits, ThinkPathBlueprint};
use super::node::{ActivationBudget, ActivationRule, ThinkNode};
use super::policy::{ActivationGuard, GuardCtx, compile_guard};
use super::run::{RouteVia, ThinkRun};
use super::types::{NodeId, NodeSignal};
use async_trait::async_trait;
use common::error::{Result, bail_err, err};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// `RouterView` 里单条总线摘要的截断长度
pub const BUS_BRIEF_MAX_CHARS: usize = 200;
/// 刺激摘要截断长度
pub const STIMULUS_BRIEF_MAX_CHARS: usize = 300;

/// 节点池：逻辑实现 + **生效准入规则** + 预算 + 编译后的准入守卫
///
/// `rules` 是**声明**（可读、可覆盖），`guards` 是**可执行判定**（编译产物）。
/// 两者都在 `build` 时定稿：声明先合并（节点自带 ∪ 蓝图静态边 ∪ 预算覆盖），
/// 再一次性编译成 `pkg::policy::Policy` 集合，运行期零分配解释。
pub struct NodePool {
    order: Vec<NodeId>,
    nodes: HashMap<NodeId, Arc<dyn ThinkNode>>,
    /// 节点自带 `activation()` ∪ 蓝图静态边编译结果
    rules: HashMap<NodeId, ActivationRule>,
    budgets: HashMap<NodeId, ActivationBudget>,
    guards: HashMap<NodeId, ActivationGuard>,
}

impl NodePool {
    pub fn build(blueprint: &ThinkPathBlueprint, nodes: Vec<Arc<dyn ThinkNode>>) -> Result<Self> {
        let mut nodes_map: HashMap<NodeId, Arc<dyn ThinkNode>> = HashMap::new();
        let mut order = Vec::new();
        for node in nodes {
            let id = node.id();
            order.push(id.clone());
            nodes_map.insert(id, node);
        }

        for spec in &blueprint.nodes {
            let id = NodeId::new(spec.id.clone());
            if !nodes_map.contains_key(&id) {
                bail_err!(
                    Internal,
                    "think path blueprint declares a node without implementation: path={} node={}",
                    blueprint.id,
                    spec.id
                );
            }
        }

        let mut rules: HashMap<NodeId, ActivationRule> = HashMap::new();
        let mut budgets: HashMap<NodeId, ActivationBudget> = HashMap::new();
        for id in &order {
            rules.insert(id.clone(), nodes_map[id].activation());
            budgets.insert(id.clone(), nodes_map[id].budget());
        }

        // 蓝图配置覆盖节点自带声明（priority 为 0 表示不覆盖）
        for spec in &blueprint.nodes {
            let id = NodeId::new(spec.id.clone());
            if spec.priority != 0
                && let Some(rule) = rules.get_mut(&id)
            {
                rule.priority = spec.priority;
            }
            let slot = budgets.entry(id).or_default();
            if let Some(max) = spec.budget.max_activations {
                slot.max_activations = max;
            }
            if spec.budget.cooldown_steps > 0 {
                slot.cooldown_steps = spec.budget.cooldown_steps;
            }
        }

        // 静态边编译：`from → to` 翻转为 `to.activation.after / on`
        for edge in &blueprint.edges {
            let to = NodeId::new(edge.to.clone());
            let from = NodeId::new(edge.from.clone());
            let rule = rules.get_mut(&to).ok_or_else(|| {
                err!(
                    Internal,
                    "think path edge targets an unknown node: path={} node={}",
                    blueprint.id,
                    edge.to
                )
            })?;
            if !rule.after.contains(&from) {
                rule.after.push(from);
            }
            if let Some(tag) = &edge.on
                && !rule.on.contains(tag)
            {
                rule.on.push(tag.clone());
            }
        }

        // 声明定稿后一次性编译成策略守卫：运行期只求值，不再解释结构
        let mut guards: HashMap<NodeId, ActivationGuard> = HashMap::new();
        for id in &order {
            let rule = rules.get(id).cloned().unwrap_or_default();
            let budget = budgets.get(id).cloned().unwrap_or_default();
            guards.insert(id.clone(), compile_guard(id, &rule, &budget));
        }

        Ok(Self {
            order,
            nodes: nodes_map,
            rules,
            budgets,
            guards,
        })
    }

    pub fn get(&self, id: &NodeId) -> Option<&Arc<dyn ThinkNode>> {
        self.nodes.get(id)
    }

    pub fn ids(&self) -> &[NodeId] {
        &self.order
    }

    pub fn rule(&self, id: &NodeId) -> Option<&ActivationRule> {
        self.rules.get(id)
    }

    pub fn budget(&self, id: &NodeId) -> ActivationBudget {
        self.budgets.get(id).cloned().unwrap_or_default()
    }

    pub fn guard(&self, id: &NodeId) -> Option<&ActivationGuard> {
        self.guards.get(id)
    }
}

/// 候选摘要：顾问看到的「可选项」
#[derive(Debug, Clone, Default, Serialize)]
pub struct CandidateBrief {
    pub id: String,
    pub phase: String,
    pub purpose: String,
    pub why_eligible: String,
}

/// 交给大小脑的路由视图：**只给摘要，不给全量**
///
/// 这张表就是「引擎的数据流转是否足够全面顺滑」的落地——
/// 它决定了顾问能做出多好的判断，也决定了每次选路要烧多少 token。
#[derive(Debug, Clone, Default, Serialize)]
pub struct RouterView {
    pub run_id: String,
    pub path_id: String,
    pub stimulus_brief: String,
    pub step: usize,
    pub steps_left: usize,
    /// 已走过的 occurrence 序列（压缩表示）
    pub visited: Vec<String>,
    pub bus_brief: Vec<(String, String)>,
    pub candidates: Vec<CandidateBrief>,
    pub last_signal: Option<String>,
}

/// 顾问的结构化输出
///
/// ⚠️ 协议层护栏：`choose` 必须以候选 id 枚举约束（schema enum）下发，
/// 而不是让模型自由输出字符串 —— 从协议层杜绝非法选择，别靠事后校验兜底。
#[derive(Debug, Clone, Deserialize)]
pub struct RouteAdvice {
    pub choose: String,
    pub reason: String,
}

/// 路由顾问：小脑（廉价）或大脑（昂贵）实现
#[async_trait]
pub trait RouteAdvisor: Send + Sync + std::fmt::Debug {
    async fn advise(&self, view: &RouterView) -> Result<RouteAdvice>;
}

#[derive(Debug, Clone)]
pub struct RouteOutcome {
    pub chosen: NodeId,
    pub candidates: Vec<NodeId>,
    pub via: RouteVia,
    pub reason: String,
}

pub struct Arbiter {
    advisor: Option<Arc<dyn RouteAdvisor>>,
}

impl Default for Arbiter {
    fn default() -> Self {
        Self::deterministic()
    }
}

impl std::fmt::Debug for Arbiter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Arbiter")
            .field("has_advisor", &self.advisor.is_some())
            .finish()
    }
}

impl Arbiter {
    /// 纯确定性仲裁：不挂顾问，成本为零
    pub fn deterministic() -> Self {
        Self { advisor: None }
    }

    pub fn with_advisor(advisor: Arc<dyn RouteAdvisor>) -> Self {
        Self {
            advisor: Some(advisor),
        }
    }

    /// `None` = 无合法候选，run 自然终止
    pub async fn select(
        &self,
        pool: &NodePool,
        run: &ThinkRun,
        last: Option<(&NodeId, &NodeSignal)>,
        limits: &PathLimits,
    ) -> Result<Option<RouteOutcome>> {
        let mut legal = self.legal_candidates(pool, run, last, true);
        if legal.is_empty() {
            // 护栏：策略补丁把所有候选都禁用时，忽略 disabled 覆盖，避免「改主意」把 run 掐死
            let relaxed = self.legal_candidates(pool, run, last, false);
            if relaxed.is_empty() {
                let why = self
                    .rejections(pool, run, last)
                    .into_iter()
                    .map(|(id, reasons)| format!("{id}: {}", reasons.join("; ")))
                    .collect::<Vec<_>>()
                    .join(" | ");
                crate::log_debug!("think path: 无合法候选，终止：{why}");
                return Ok(None);
            }
            crate::log_warn!("think path: 策略补丁禁用导致无候选，本次忽略 disabled 覆盖");
            legal = relaxed;
        }

        if legal.len() == 1 {
            return Ok(Some(RouteOutcome {
                chosen: legal[0].clone(),
                candidates: legal,
                via: RouteVia::Unique,
                reason: "唯一合法候选".to_string(),
            }));
        }

        let deterministic = self.pick_deterministic(pool, run, &legal);
        let advisor = match self.advisor.as_ref() {
            Some(a) if run.trace.advisor_calls < limits.advisor_calls_budget => a,
            _ => {
                return Ok(Some(RouteOutcome {
                    chosen: deterministic,
                    candidates: legal,
                    via: RouteVia::Deterministic,
                    reason: "多候选，按确定性优先级选中".to_string(),
                }));
            }
        };

        let view = build_router_view(pool, run, &legal, last);
        match advisor.advise(&view).await {
            Ok(advice) => {
                let chosen = NodeId::new(advice.choose);
                if legal.contains(&chosen) {
                    Ok(Some(RouteOutcome {
                        chosen,
                        candidates: legal,
                        via: RouteVia::Advisor,
                        reason: advice.reason,
                    }))
                } else {
                    crate::log_warn!("think path: 顾问返回非法节点，降级为确定性选择");
                    Ok(Some(RouteOutcome {
                        chosen: deterministic,
                        candidates: legal,
                        via: RouteVia::Fallback,
                        reason: format!("顾问越界，已降级：{}", advice.reason),
                    }))
                }
            }
            Err(e) => {
                crate::log_warn!("think path: 顾问调用失败，降级为确定性选择");
                Ok(Some(RouteOutcome {
                    chosen: deterministic,
                    candidates: legal,
                    via: RouteVia::Fallback,
                    reason: format!("顾问失败，已降级：{e}"),
                }))
            }
        }
    }

    fn legal_candidates(
        &self,
        pool: &NodePool,
        run: &ThinkRun,
        last: Option<(&NodeId, &NodeSignal)>,
        respect_disabled: bool,
    ) -> Vec<NodeId> {
        pool.ids()
            .iter()
            .filter(|id| {
                if respect_disabled && run.overlay.disabled.contains(*id) {
                    return false;
                }
                self.is_eligible(pool, run, id, last)
            })
            .cloned()
            .collect()
    }

    /// 准入判定：交给编译好的策略守卫
    ///
    /// 这里不再有任何 `if` —— 拓扑与预算的语义全部在 `policy.rs` 的策略里，
    /// 新增一种准入形态（如「仅当顾问置信度 > x」）只需加一个 `Policy`。
    fn is_eligible(
        &self,
        pool: &NodePool,
        run: &ThinkRun,
        id: &NodeId,
        last: Option<(&NodeId, &NodeSignal)>,
    ) -> bool {
        let Some(guard) = pool.guard(id) else {
            return false;
        };
        let budget = pool.budget(id);
        let ctx = GuardCtx {
            run,
            node: id,
            last,
            budget: &budget,
        };
        guard.is_admitted(&ctx)
    }

    /// 诊断用：每个节点「为什么没被准入」
    ///
    /// 动态路由下这是唯一的排障抓手 —— 图跑成什么样只能运行时解释，
    /// 因此未准入原因必须能打出来，而不是只返回一个空候选集。
    pub fn rejections(
        &self,
        pool: &NodePool,
        run: &ThinkRun,
        last: Option<(&NodeId, &NodeSignal)>,
    ) -> Vec<(String, Vec<String>)> {
        pool.ids()
            .iter()
            .filter_map(|id| {
                let guard = pool.guard(id)?;
                let budget = pool.budget(id);
                let ctx = GuardCtx {
                    run,
                    node: id,
                    last,
                    budget: &budget,
                };
                let reasons = guard.rejections(&ctx);
                if reasons.is_empty() {
                    None
                } else {
                    Some((id.as_str().to_string(), reasons))
                }
            })
            .collect()
    }

    fn pick_deterministic(&self, pool: &NodePool, run: &ThinkRun, legal: &[NodeId]) -> NodeId {
        let mut best: Option<(i32, NodeId)> = None;
        for id in legal {
            let base = pool.rule(id).map(|r| r.priority).unwrap_or(0);
            let score = run.overlay.effective_priority(base, id);
            if best.as_ref().is_none_or(|(top, _)| score > *top) {
                best = Some((score, id.clone()));
            }
        }
        best.map(|(_, id)| id).unwrap_or_default()
    }
}

/// 构造顾问视图
pub fn build_router_view(
    pool: &NodePool,
    run: &ThinkRun,
    legal: &[NodeId],
    last: Option<(&NodeId, &NodeSignal)>,
) -> RouterView {
    let bus_brief = run
        .bus
        .loaded_keys()
        .into_iter()
        .filter_map(|key| {
            run.bus
                .summary(key, BUS_BRIEF_MAX_CHARS)
                .map(|text| (key.as_str().to_string(), text))
        })
        .collect();

    let candidates = legal
        .iter()
        .map(|id| {
            let (phase, purpose) = match pool.get(id) {
                Some(node) => (
                    node.phase().as_str().to_string(),
                    node.purpose().to_string(),
                ),
                None => (String::new(), String::new()),
            };
            let priority = pool.rule(id).map(|r| r.priority).unwrap_or(0);
            // 准入条件的自然语言描述：顾问凭它判断「这个候选凭什么合法」
            let admitted = pool
                .guard(id)
                .map(|guard| guard.describe())
                .unwrap_or_default();
            CandidateBrief {
                id: id.as_str().to_string(),
                phase,
                purpose,
                why_eligible: format!("priority={priority}; {admitted}"),
            }
        })
        .collect();

    RouterView {
        run_id: run.run_id.clone(),
        path_id: run.blueprint.id.clone(),
        stimulus_brief: truncate(&run.stimulus.text, STIMULUS_BRIEF_MAX_CHARS),
        step: run.trace.steps,
        steps_left: run
            .blueprint
            .limits
            .max_steps
            .saturating_sub(run.trace.steps),
        visited: run
            .trace
            .occurrences
            .iter()
            .map(|o| format!("{}#{}", o.def, o.seq))
            .collect(),
        bus_brief,
        candidates,
        last_signal: last.map(|(_, signal)| signal.tag().to_string()),
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        text.chars().take(max_chars).collect()
    }
}
