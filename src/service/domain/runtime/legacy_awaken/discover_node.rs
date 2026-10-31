//! 新发现识别节点（判断型）：审视本轮思考轨迹，判断是否出现值得再检索的新线索
//!
//! 短聚焦单轮无工具模型调用——刻意不走 PromptBuilder：本节点的价值就是**短**，
//! 全量上下文反而稀释重点（design §3.3）。输出经 visited 差集去重后有新词 →
//! `new_findings` 回 retrieve 再检索装配；无新词 / 软预算尽 / 模型抖动 /
//! 解析失败 → `no_new_findings` 走 force_compact 压缩退出——discover 是增强
//! 回环，不是闸门，任何失败都降级退出，不炸主流程。
//!
//! 观测口径（design §3.7）：日志 + fingerprint 覆盖，不新增事件类型。

use super::blueprint::{SIGNAL_NEW_FINDINGS, SIGNAL_NO_NEW_FINDINGS};
use super::context::{AwakenDeps, DiscoveryOutcome, RetrievalPack, read_bus};
use super::nodes::{Artifacts, BudgetState};
use super::retrieve_node::diff_terms;
use crate::pkg::think_path::{
    ContextKey, LoadDepth, NodeContract, NodeId, NodeOutcome, NodeSignal, PhaseId, ThinkBus,
    ThinkNode,
};
use async_trait::async_trait;
use common::error::Result;
use std::sync::Arc;

/// 节点 id：蓝图声明、runner 注册、轨迹指纹三处共用的同一真相
pub(crate) const NODE_DISCOVER: &str = "discover";

/// discover 软预算：最多 2 轮「再检索」
///
/// 双层防线（蓝图硬预算 = 软预算 + 1，见 blueprint.rs）：软预算先到时直接
/// 退出——第 3 次激活**零模型调用**走 no_new_findings 优雅收尾；硬预算只是
/// 兜底（软预算判断被绕过时引擎也不会排出第 4 个 discover）。硬预算必须比
/// 软预算大 1：同值时软预算的优雅退出永远先撞硬预算的「无合法候选 →
/// run 终止」，整条唤醒链会以 Internal 收场而不是 force_compact。
pub(crate) const MAX_DISCOVERY_ROUNDS: usize = 2;

/// 轨迹行数上限（防 prompt 失控：30 行 × 每行 200 chars）
const MAX_TRAIL_LINES: usize = 30;

/// 单条轨迹行的内容截断长度
const TRAIL_SUMMARY_CHARS: usize = 200;

/// 轨迹行：只保留最近的 `MAX_TRAIL_LINES` 条（新线索大概率出现在轨迹尾部），
/// 单行复用 `ChatMessage::to_summary_text` 的 `[role] content` 口径（含截断）
fn trail_lines(artifacts: &Artifacts) -> Vec<String> {
    let skip = artifacts.messages.len().saturating_sub(MAX_TRAIL_LINES);
    artifacts
        .messages
        .iter()
        .skip(skip)
        .map(|m| m.to_summary_text(TRAIL_SUMMARY_CHARS))
        .collect()
}

/// 短聚焦 prompt：只给三样东西——用户诉求、本轮思考轨迹摘要、已检索词
fn build_discover_prompt(
    user_message: &str,
    trail_lines: &[String],
    queried_terms: &[String],
) -> String {
    format!(
        "用户诉求：{user_message}\n\n本轮思考轨迹（按序）：\n{}\n\n已检索过的关键词：{}\n\n\
         任务：审视上述思考轨迹，判断是否出现了**值得检索但尚未检索**的新线索。\n\
         输出 JSON（不要输出其他内容）：\n\
         {{\"new_key_terms\": [\"...\"], \"prior_thinking_summary\": \"一句话概括本轮已尝试与已知的结论\"}}\n\
         规则：new_key_terms 只放未出现在已检索词里的新词；确实没有新线索就给空数组。",
        trail_lines.join("\n"),
        queried_terms.join("、")
    )
}

/// 解析 discover 输出（复用 intent 解析同套路：直接解析 → 括号提取 → 宽容字段）
///
/// 与 `parse_intent_analysis_json` 的关键差别：**全失败不报错**，返回
/// `DiscoveryOutcome::default()`——解析失败 = 无新息，走退出路径，
/// 模型输出抖动不该炸主流程（discover 是增强，不是闸门）。
pub(crate) fn parse_discovery_json(text: &str) -> DiscoveryOutcome {
    let text = text.trim();

    // Level 1: 整段文本直接解析
    if let Ok(outcome) = serde_json::from_str::<DiscoveryOutcome>(text) {
        return outcome;
    }

    // Level 2: 平衡括号法提取第一个完整 JSON 对象（markdown 代码块 / 前后散文包裹）
    if let Some(json_obj) =
        crate::service::domain::runtime::awakening::extract_first_json_object(text)
        && let Ok(outcome) = serde_json::from_str::<DiscoveryOutcome>(json_obj)
    {
        return outcome;
    }

    // Level 3: 宽容字段提取（数组混入非字符串 / 多余脏字段）
    try_lenient_parse(text).unwrap_or_default()
}

/// 宽容解析：parse 成 `serde_json::Value` 后逐字段提取（口径照 intent 的同名先例）
fn try_lenient_parse(s: &str) -> Option<DiscoveryOutcome> {
    use serde_json::Value;

    let val: Value = serde_json::from_str(s).ok()?;
    let new_key_terms: Vec<String> = val
        .get("new_key_terms")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| match x {
                    Value::String(s) => Some(s.clone()),
                    Value::Number(n) => Some(n.to_string()),
                    Value::Bool(b) => Some(b.to_string()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();
    let prior_thinking_summary = val
        .get("prior_thinking_summary")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // 两个字段全空 = 没解析出任何有效约定，交回 None → default
    if new_key_terms.is_empty() && prior_thinking_summary.is_empty() {
        return None;
    }
    Some(DiscoveryOutcome {
        new_key_terms,
        prior_thinking_summary,
    })
}

#[derive(Debug)]
pub struct DiscoverNode {
    deps: Arc<AwakenDeps>,
}

impl DiscoverNode {
    pub fn new(deps: Arc<AwakenDeps>) -> Self {
        Self { deps }
    }
}

#[async_trait]
impl ThinkNode for DiscoverNode {
    fn id(&self) -> NodeId {
        NodeId::new(NODE_DISCOVER)
    }

    fn phase(&self) -> PhaseId {
        PhaseId::Review
    }

    fn purpose(&self) -> &str {
        "新发现识别：从思考轨迹提取新线索回环再检索，或无新息退出"
    }

    fn contract(&self) -> NodeContract {
        NodeContract {
            requires: vec![],
            reads: vec![
                ContextKey::Artifacts,
                ContextKey::Knowledge,
                ContextKey::Intent,
                // 唤醒总轮次耗尽时零模型调用直接退出（见 run 的 2.5 步）
                ContextKey::Budget,
            ],
            writes: vec![ContextKey::Knowledge],
        }
    }

    async fn run(&self, bus: &mut ThinkBus) -> Result<NodeOutcome> {
        let deps = &self.deps;
        let no_exit = || NodeOutcome::new(NodeSignal::Yield(SIGNAL_NO_NEW_FINDINGS.to_string()));

        // 1. 快照后立刻放锁：MutexGuard 不跨 await（防死锁，照 BusinessProvider 先例）
        let state_snapshot = {
            let state = match deps.retrieval.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            state.clone()
        };

        // 2. 软预算尽 → 直接退出（省一次模型调用；蓝图硬预算兜底）
        if state_snapshot.discovery_rounds >= MAX_DISCOVERY_ROUNDS {
            crate::log_info!(
                &deps.ctx,
                "legacy_awaken",
                "discover node budget exhausted, agent_id={}, discovery_rounds={}",
                deps.agent.po.id,
                state_snapshot.discovery_rounds
            );
            return Ok(no_exit());
        }

        // 2.5 唤醒总轮次已耗尽 → 回环没有意义：再检索也领不到思考预算，
        //     只会白打一次模型（轨迹里那条「第 11 轮」）。直接走退出压缩。
        let budget = read_bus::<BudgetState>(bus, ContextKey::Budget).unwrap_or_default();
        let total_cap = deps.options.effective_max_rounds();
        if budget.total_rounds >= total_cap {
            crate::log_info!(
                &deps.ctx,
                "legacy_awaken",
                "discover skipped: wake round budget exhausted, agent_id={}, used={}, cap={}",
                deps.agent.po.id,
                budget.total_rounds,
                total_cap
            );
            return Ok(no_exit());
        }

        // 3. 轨迹行 + 短聚焦 prompt（Artifacts 缺失 = 空轨迹，照常让模型判断）
        let artifacts = read_bus::<Artifacts>(bus, ContextKey::Artifacts).unwrap_or_default();
        let trails = trail_lines(&artifacts);
        let prompt = build_discover_prompt(
            deps.message.content(),
            &trails,
            &state_snapshot.queried_terms,
        );

        // 4. 模型调用：失败降级退出（模型抖动不炸主流程）
        let trace_id = format!("discover-{}", deps.trace_id);
        let text = match deps
            .backend
            .discover(
                deps,
                super::backend::DiscoverInput {
                    trace_id: trace_id.clone(),
                    prompt,
                },
            )
            .await
        {
            Ok(text) => text,
            Err(e) => {
                crate::log_error!(
                    &deps.ctx,
                    "legacy_awaken",
                    "discover degraded to exit: agent_id={}, trace_id={}, error={}",
                    deps.agent.po.id,
                    trace_id,
                    e
                );
                return Ok(no_exit());
            }
        };

        // 5. 宽容解析 + visited 差集（新词与已查词重复 = 无新息）
        let outcome = parse_discovery_json(&text);
        let new_terms = diff_terms(&outcome.new_key_terms, &state_snapshot.queried_terms);

        // 6. 无新词 → 退出；有新词 → 记轮次 + 写「新词 + 思考摘要」的过渡 pack
        //    （retrieve 会覆写成完整产物，见 RetrievalPack 契约）
        if new_terms.is_empty() {
            crate::log_info!(
                &deps.ctx,
                "legacy_awaken",
                "discover node no new findings, agent_id={}, trail_lines={}, outcome_terms={}",
                deps.agent.po.id,
                trails.len(),
                outcome.new_key_terms.len()
            );
            return Ok(no_exit());
        }

        {
            let mut state = match deps.retrieval.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            state.discovery_rounds += 1;
        }
        let pack = RetrievalPack {
            query_terms: new_terms.clone(),
            knowledge_hits: Vec::new(),
            method_skills: Vec::new(),
            prior_thinking_summary: Some(outcome.prior_thinking_summary),
        };
        let value = serde_json::to_value(&pack)
            .map_err(|e| common::error::err!(Internal, "encode discovery pack failed: {e}"))?;
        bus.insert(ContextKey::Knowledge, LoadDepth::Full, value);

        crate::log_info!(
            &deps.ctx,
            "legacy_awaken",
            "discover node new findings, agent_id={}, trail_lines={}, new_terms={}",
            deps.agent.po.id,
            trails.len(),
            new_terms.len()
        );
        Ok(NodeOutcome::new(NodeSignal::Yield(
            SIGNAL_NEW_FINDINGS.to_string(),
        )))
    }
}
