//! Handler: 沉淀记忆 - Neural Tool
//!
//! 触发 Agent 进入沉淀工作模式：查询未沉淀短期记忆，生成编号摘要，直接调用
//! RuntimeAwakening.sleep_and_settle 让 Agent 在 Resting 状态下用已有工具自主完成沉淀
//! （归纳总结、创建/更新节点、建关系、加 published 标签）。
//!
//! 与 awaken 对称：awaken 是醒来响应外部消息，sleep_and_settle 是沉睡整理内部记忆。
//! 沉淀约束模板（不发消息、只用记忆工具）内聚在 PromptBuilder.build_sleep_prompt，
//! 本模块只负责生成待沉淀记忆摘要。

use crate::models::agent::Agent;
use crate::models::memory::{Memory, MemoryPo, ShortTermMemoryIndexPo};
use crate::pkg::RequestContext;
use crate::service::dal::agent::AgentFetchOptions;
use crate::service::dao::memory::{MemoryQuery, MemorySortOrder};
use crate::service::domain::hr::domain as hr_domain;
use crate::service::domain::runtime::awakening::ThinkingOptions;
use crate::service::domain::runtime::domain as runtime_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SettleMemoryParams, SettleMemoryResponse};
use common::enums::ThinkingScene;
use common::enums::{MemoryStatus, MemoryType};
use common::error::{Result, bail_err};

/// 取出短期记忆索引（只读）
///
/// Domain 返回的是 `Memory` 业务实体而非 PO，短期记忆的具体字段在
/// `MemoryPo::ShortTerm` 变体内，这里统一收敛访问方式。
fn short_term_of(mem: &Memory) -> Option<&ShortTermMemoryIndexPo> {
    match &mem.po {
        MemoryPo::ShortTerm(index) => Some(index),
        _ => None,
    }
}

/// 取出短期记忆索引（可变）
///
/// 状态闭环已下沉到 Domain，本模块不再需要改状态，仅测试构造场景使用。
#[cfg(test)]
fn short_term_of_mut(mem: &mut Memory) -> Option<&mut ShortTermMemoryIndexPo> {
    match &mut mem.po {
        MemoryPo::ShortTerm(index) => Some(index),
        _ => None,
    }
}

/// 单批待沉淀记忆的条数上限（防呆，避免极端情况下无限拼接）
pub(crate) const PENDING_MAX_ITEMS: usize = 200;

/// 待沉淀列表最多占用「可用上下文」的比例
///
/// 沉淀 prompt 里还有 System（人设 + 技能）、通用上下文、【历史对话】，
/// 以及模型输出与工具往返，因此待沉淀列表不能吃满整个窗口。
const PENDING_BUDGET_RATIO: f64 = 0.4;

/// 上下文长度未知时，`max_context_length` 的可用比例（与 think_loop 的压缩阈值同源）
const CONTEXT_USABLE_RATIO: f64 = 0.6;

/// 上下文长度完全未知时的待沉淀字符预算兜底
const DEFAULT_PENDING_BUDGET_CHARS: usize = 12_000;

/// 计算待沉淀列表可用的字符预算
///
/// 优先级：模型推荐上下文长度 > `max_context_length * 60%` > 兜底常量，
/// 与 `run_think_loop` 的压缩阈值取法保持一致。
///
/// # 为什么按「字符」而不是「token」
///
/// 项目没有 tokenizer，硬造一个 risk 大于收益。这里保守假设
/// **1 字符 ≈ 1 token**（中文密集场景的上界）：对英文内容会明显偏保守，
/// 但绝不会把上下文撑爆 —— 沉淀场景宁可少处理几条，也不能劣化模型表现。
fn pending_budget_chars(agent: &Agent) -> usize {
    let Some(brain) = agent.brain.as_ref() else {
        return DEFAULT_PENDING_BUDGET_CHARS;
    };
    let Some(provider) = brain.model_provider() else {
        return DEFAULT_PENDING_BUDGET_CHARS;
    };

    let config = provider.config();
    let context_tokens = config
        .recommended_context_length
        .filter(|&v| v > 0)
        .map(|v| v as f64)
        .or_else(|| {
            config
                .max_context_length
                .filter(|&v| v > 0)
                .map(|v| v as f64 * CONTEXT_USABLE_RATIO)
        });

    match context_tokens {
        Some(tokens) => (tokens * PENDING_BUDGET_RATIO) as usize,
        None => DEFAULT_PENDING_BUDGET_CHARS,
    }
}

/// 构建待沉淀短期记忆的编号摘要（按上下文预算自适应批量）
///
/// 供 settle_memory handler 和 CronTrigger agent_rest 复用。
/// 约束模板已内聚到 `PromptBuilder.build_sleep_prompt`，本函数只返回记忆摘要。
///
/// # 批量策略
///
/// 一次最多取 [`PENDING_MAX_ITEMS`] 条，然后**按字符预算累加**：只要累计长度
/// 还在预算内就继续拼。相比固定 10 条，一次能整合更多记忆，又不会因上下文
/// 过长导致模型表现劣化 —— 预算本身就是护栏。
///
/// 排序用 **最早优先**：沉淀是队列语义，先进先出。
/// 若按最近优先，持续产生的新记忆会把老记忆挤出窗口，导致它们永远轮不到沉淀。
///
/// # 分层
///
/// 本模块位于 **Adapter 层**，按分层红线（`Adapter → Domain → DAL → DAO`）
/// 只通过 `runtime_domain().memory()`（Domain）访问记忆，**不直接依赖 DAL / DAO**，
/// 也不接触 PO —— Domain 返回的是 `Memory` 业务实体。
///
/// # 返回
/// - `Ok(None)` 表示无未沉淀记忆（调用方应跳过）
/// - `Ok(Some((summary, ids, truncated)))`：
///   - `summary` 编号摘要字符串（注入 prompt）
///   - `ids` 本批记忆 id，供沉淀结束后框架置状态使用
///   - `truncated` 是否因触达预算/上限而截断（true 表示还有剩余待沉淀）
pub(crate) async fn build_pending_memories_summary(
    ctx: &RequestContext,
    agent_id: &str,
    budget_chars: usize,
    max_items: usize,
) -> Result<Option<(String, Vec<String>, bool)>> {
    // 1. 取候选集：未沉淀（Active）、最早优先，最多 PENDING_MAX_ITEMS 条
    //
    // 不做分页：`MemoryQuery` 没有 offset，重复查询会一直拿到同一批
    // （本批此时还未置为 Settled）。单次取上限再由预算截断更简单也更省查询。
    let candidates = runtime_domain()
        .memory()
        .query(
            ctx.clone(),
            MemoryQuery {
                agent_id: Some(agent_id.to_string()),
                status: Some(MemoryStatus::Active),
                memory_type: Some(MemoryType::ShortTerm),
                limit: Some(max_items),
                order: MemorySortOrder::OldestFirst,
                ..Default::default()
            },
        )
        .await?;

    if candidates.is_empty() {
        return Ok(None);
    }

    // 2. 按预算累加（约束模板由 builder.build_sleep_prompt 注入）
    let budget = budget_chars.max(1);
    let mut summary = String::new();
    let mut ids = Vec::new();
    let mut truncated = false;

    for mem in &candidates {
        let Some(index) = short_term_of(mem) else {
            continue;
        };
        let line = format!("{}. [id={}] {}\n", ids.len() + 1, index.id, index.summary);

        // 预算用尽：停下，剩下的留给下一批
        if !summary.is_empty() && summary.chars().count() + line.chars().count() > budget {
            truncated = true;
            break;
        }
        // 条数上限：同样停下
        if ids.len() >= max_items {
            truncated = true;
            break;
        }

        summary.push_str(&line);
        ids.push(index.id.clone());
    }

    if ids.is_empty() {
        return Ok(None);
    }

    // 候选集本身被 limit 截断时，也算有剩余（无法确认后面还有没有，保守为 true）
    if candidates.len() >= max_items {
        truncated = true;
    }

    log_info!(
        ctx,
        "settle_memory",
        "agent_id={}, 候选 {} 条，本批纳入 {} 条，预算 {} 字符，仍有剩余={}",
        agent_id,
        candidates.len(),
        ids.len(),
        budget,
        truncated
    );

    Ok(Some((summary, ids, truncated)))
}

/// 加载 Agent（含 tools + skills）并唤醒 Brain，然后调用 sleep_and_settle 执行沉淀
///
/// 供 settle_memory handler 和 CronTrigger agent_rest 复用。
///
/// 沉淀完成后会对本批记忆做一次**状态兜底**：仍处于 Active 的会被标记为 Settled，
/// 避免 Agent 漏调 `update_memory` 导致同一批被反复处理（详见 `mark_pending_settled`）。
///
/// # 返回
/// 待沉淀的短期记忆数量（0 表示无待沉淀，已跳过）
pub(crate) async fn load_and_settle(
    ctx: RequestContext,
    agent_id: &str,
    settle_limit: usize,
) -> Result<usize> {
    // 预检查：Agent 必须空闲才能进入睡眠，避免覆盖 Busy 状态
    let state =
        crate::pkg::agent_runtime_state::AgentRuntimeStateManager::global().get_state(agent_id);
    if state.is_unavailable() {
        log_info!(
            &ctx,
            "settle_memory",
            "Agent {} 当前 {:?}，跳过睡眠",
            agent_id,
            state
        );
        return Ok(0);
    }

    // 1. 加载 Agent（含 tools + skills）
    let fetch_options = AgentFetchOptions {
        with_tools: Some(true),
        with_skills: Some(true),
        ..Default::default()
    };
    let mut agent = hr_domain()
        .agent_manage()
        .get_agent(ctx.clone(), agent_id, fetch_options)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Agent {} not found", agent_id)))?;

    // 2. 唤醒 Brain（装配 Cortex）
    //
    // 必须先装配 Brain 再构建待沉淀摘要：批量大小依赖模型上下文预算，
    // 而预算要从 brain.model_provider 的配置里取。
    let ctx = runtime_domain()
        .awakening()
        .wake_agent_brain(ctx, &mut agent)
        .await?;

    // 3. 查询未沉淀短期记忆 + 生成编号摘要（按上下文预算自适应批量）
    let budget = pending_budget_chars(&agent);
    // settle_limit 作为条数上限提示（调用方可传小值调优），再与硬上限取小
    let max_items = settle_limit.min(PENDING_MAX_ITEMS);
    let (summary, pending_ids, has_remaining) =
        match build_pending_memories_summary(&ctx, agent_id, budget, max_items).await? {
            Some(triple) => triple,
            None => return Ok(0),
        };
    let pending_count = pending_ids.len();
    if has_remaining {
        log_info!(
            ctx,
            "settle_memory",
            "agent_id={}, 本批处理 {} 条后仍有剩余，将在下次沉淀继续",
            agent_id,
            pending_count
        );
    }

    // 4. 沉睡沉淀（Resting 状态 + think + 写 Trace）
    //    sleep_and_settle 会过滤 Manual 工具和 skill（只保留记忆相关），
    //    调用 builder.build_sleep_prompt 生成沉淀 prompt（约束模板内聚在 builder）
    //    trace_ids 传空：独立沉淀场景无父 trace，沉淀自身会生成 trace_id 记录到 prompt
    let options = ThinkingOptions::for_scene(ThinkingScene::Settle);
    runtime_domain()
        .awakening()
        .sleep_and_settle(ctx.clone(), &agent, &summary, &options, &[])
        .await?;

    // 5. 状态兜底：把本批中仍为 Active 的短期记忆标记为 Settled
    //
    // 状态闭环由框架负责，不依赖 LLM 自觉调用 update_memory。
    // 用 `?` 传播的失败（如 brain 装配失败）不会走到这里，此时记忆保持 Active，
    // 下次沉淀会重新处理——这是期望行为。
    match runtime_domain()
        .memory()
        .mark_short_term_settled(ctx.clone(), agent_id, &pending_ids)
        .await
    {
        Ok(marked) => {
            log_info!(
                ctx,
                "settle_memory",
                "agent_id={}, 本批 {} 条短期记忆，框架兜底置为已沉淀 {} 条",
                agent_id,
                pending_ids.len(),
                marked
            );
        }
        Err(e) => {
            // 兜底失败不阻断：沉淀本身已成功，状态留给下次沉淀重试
            log_warn!(
                ctx,
                "settle_memory",
                error = ?e,
                "兜底标记短期记忆为已沉淀失败"
            );
        }
    }

    Ok(pending_count)
}

#[register_handler_tool(
    id = "settle_memory",
    name = "Settle Working Memory",
    description = "Trigger the agent's rest-and-consolidate cycle: pending working-memory entries are handed to the agent, which autonomously distills them into long-term knowledge nodes and relations; processed entries are then marked Settled. Returns the number settled (0 if nothing is pending or the agent is not idle).",
    params = "common::api::SettleMemoryParams",
    neural
)]
#[generate_http_handler]
pub async fn settle_memory(
    ctx: RequestContext,
    params: SettleMemoryParams,
) -> Result<SettleMemoryResponse> {
    let agent_id = ctx.agent_id().cloned().unwrap_or_default();
    if agent_id.is_empty() {
        bail_err!(InvalidRequest, "settle_memory 需要 agent 上下文");
    }
    // 不传 limit 时用自适应上限：实际批量由上下文预算决定（见 pending_budget_chars），
    // limit 仅作为条数上限提示，供调用方按需压小（如调优或限流）。
    let limit = params.limit.unwrap_or(PENDING_MAX_ITEMS);

    let settled_count = load_and_settle(ctx.clone(), &agent_id, limit).await?;

    log_info!(
        ctx,
        "settle_memory",
        "agent_id={}, 沉淀完成，处理 {} 条短期记忆",
        agent_id,
        settled_count
    );

    Ok(SettleMemoryResponse { settled_count })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::agent::Agent;
    use crate::models::memory::{MemoryCreateParams, ShortTermMemoryIndexPo};

    /// 初始化测试环境：config + 所有 DAO/DAL 单例 + Runtime Domain
    ///
    /// 本模块是 Adapter 层，只通过 Domain 访问记忆，因此测试也必须把 Domain
    /// 依赖的单例全部拉起。池由 `new_test_ctx` 注入（sqlx::test 的内存库），
    /// 各 DAO 单例本身无状态，不会串到真实库。
    fn init_settle_test_env(pool: sqlx::SqlitePool) -> RequestContext {
        let _ = crate::config::init();
        let base_path = crate::config::get().base_data_path();
        crate::pkg::tool_tracing::logger::ToolCallLogger::init(base_path);

        crate::service::dao::init_all();
        crate::service::dal::init_all();
        crate::service::domain::runtime::init();

        crate::pkg::request_context_test_support::new_test_ctx("test-user", pool)
    }

    async fn seed(ctx: &RequestContext, id: &str, offset_ms: i64) {
        let now = chrono::Utc::now().timestamp_millis() + offset_ms;
        runtime_domain()
            .memory()
            .create(
                ctx.clone(),
                MemoryCreateParams::CreateShortTerm(ShortTermMemoryIndexPo {
                    id: id.to_string(),
                    agent_id: "agent-settle".to_string(),
                    task_id: None,
                    role: "user".to_string(),
                    summary: format!("摘要 {}", id),
                    tags: "[]".to_string(),
                    trace_ids: "[]".to_string(),
                    status: MemoryStatus::Active,
                    created_at: now,
                    updated_at: now,
                }),
            )
            .await
            .unwrap();
    }

    async fn status_of(ctx: &RequestContext, id: &str) -> MemoryStatus {
        let list = runtime_domain()
            .memory()
            .query(
                ctx.clone(),
                MemoryQuery {
                    ids: Some(vec![id.to_string()]),
                    agent_id: Some("agent-settle".to_string()),
                    memory_type: Some(MemoryType::ShortTerm),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        short_term_of(list.first().expect("memory should exist"))
            .expect("should be short term")
            .status
    }

    /// 核心语义：兜底只翻转仍为 Active 的，Agent 已处理的不覆盖，批次外的不误伤
    #[sqlx::test]
    async fn mark_pending_settled_only_flips_still_active(pool: sqlx::SqlitePool) {
        let ctx = init_settle_test_env(pool);

        // st-1: 模型漏调 update_memory，仍为 Active → 应被兜底置为 Settled
        // st-2: 模型已自行置为 Settled → 不应被改动
        // st-3: 不在本批 id 列表里 → 不应被误伤
        seed(&ctx, "st-1", 0).await;
        seed(&ctx, "st-2", 1).await;
        seed(&ctx, "st-3", 2).await;

        // 模拟模型已处理 st-2
        let mut done = runtime_domain()
            .memory()
            .query(
                ctx.clone(),
                MemoryQuery {
                    ids: Some(vec!["st-2".to_string()]),
                    agent_id: Some("agent-settle".to_string()),
                    memory_type: Some(MemoryType::ShortTerm),
                    ..Default::default()
                },
            )
            .await
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        short_term_of_mut(&mut done).unwrap().status = MemoryStatus::Settled;
        runtime_domain()
            .memory()
            .update(ctx.clone(), done)
            .await
            .unwrap();

        let marked = runtime_domain()
            .memory()
            .mark_short_term_settled(
                ctx.clone(),
                "agent-settle",
                &["st-1".to_string(), "st-2".to_string()],
            )
            .await
            .unwrap();

        // 只翻转了 st-1
        assert_eq!(marked, 1);
        assert_eq!(status_of(&ctx, "st-1").await, MemoryStatus::Settled);
        // 模型已置位的保持 Settled，且未被重复处理
        assert_eq!(status_of(&ctx, "st-2").await, MemoryStatus::Settled);
        // 批次外不受影响
        assert_eq!(status_of(&ctx, "st-3").await, MemoryStatus::Active);
    }

    /// 空批次直接返回
    #[sqlx::test]
    async fn mark_pending_settled_noops_on_empty(pool: sqlx::SqlitePool) {
        let ctx = init_settle_test_env(pool);
        assert_eq!(
            runtime_domain()
                .memory()
                .mark_short_term_settled(ctx.clone(), "agent-settle", &[])
                .await
                .unwrap(),
            0
        );
    }

    /// 待沉淀队列按最早优先取，避免老记忆被新记忆挤出窗口
    #[sqlx::test]
    async fn build_pending_summary_uses_oldest_first(pool: sqlx::SqlitePool) {
        let ctx = init_settle_test_env(pool);

        seed(&ctx, "p-old", 0).await;
        seed(&ctx, "p-mid", 10).await;
        seed(&ctx, "p-new", 20).await;

        // 条数上限压到 2，预算给足
        let (summary, ids, truncated) =
            build_pending_memories_summary(&ctx, "agent-settle", 10_000, 2)
                .await
                .unwrap()
                .expect("应有待沉淀记忆");

        assert_eq!(ids, vec!["p-old".to_string(), "p-mid".to_string()]);
        assert!(summary.contains("[id=p-old]"));
        assert!(!summary.contains("[id=p-new]"));
        // 候选集被 limit 截断，保守标记为仍有剩余
        assert!(truncated);
    }

    /// 预算自适应：只要累计长度还在预算内就继续拼，而不是固定条数
    #[sqlx::test]
    async fn build_pending_summary_accumulates_within_budget(pool: sqlx::SqlitePool) {
        let ctx = init_settle_test_env(pool);

        for i in 0..5 {
            seed(&ctx, &format!("b-{}", i), i).await;
        }

        // 预算充足 → 全部纳入
        let (_summary, ids, truncated) =
            build_pending_memories_summary(&ctx, "agent-settle", 100_000, PENDING_MAX_ITEMS)
                .await
                .unwrap()
                .expect("应有待沉淀记忆");
        assert_eq!(ids.len(), 5);
        assert!(!truncated);

        // 预算极小 → 只纳入能放下的前几条，并标记截断
        let (summary, ids, truncated) =
            build_pending_memories_summary(&ctx, "agent-settle", 60, PENDING_MAX_ITEMS)
                .await
                .unwrap()
                .expect("应有待沉淀记忆");
        assert!(truncated, "预算不足应标记截断");
        assert!(ids.len() < 5, "预算不足应少纳入：实际 {}", ids.len());
        assert!(summary.chars().count() <= 60 + 40, "不应明显超出预算");
        // 仍按最早优先
        assert_eq!(ids[0], "b-0");
    }

    /// 无待沉淀记忆时返回 None，调用方据此跳过
    #[sqlx::test]
    async fn build_pending_summary_returns_none_when_empty(pool: sqlx::SqlitePool) {
        let ctx = init_settle_test_env(pool);
        assert!(
            build_pending_memories_summary(&ctx, "agent-settle", 10_000, PENDING_MAX_ITEMS)
                .await
                .unwrap()
                .is_none()
        );
    }

    /// 预算取法：拿不到模型配置时回落到兜底常量，且恒为正
    #[test]
    fn pending_budget_chars_falls_back_without_brain() {
        let mut po = crate::models::agent::AgentPo::new(
            "Test".to_string(),
            vec!["assistant".to_string()],
            "desc".to_string(),
            vec![],
            "".to_string(),
            "provider-001".to_string(),
            "test-user".to_string(),
        );
        po.id = "agent-budget".to_string();
        let agent = Agent::from_po(po);

        // 无 brain → 兜底
        assert_eq!(pending_budget_chars(&agent), DEFAULT_PENDING_BUDGET_CHARS);
        assert!(pending_budget_chars(&agent) > 0);
    }
}
