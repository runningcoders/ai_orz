//! Handler: 沉淀记忆 - Neural Tool
//!
//! 触发 Agent 进入沉淀工作模式：直接调用 `RuntimeAwakening.settle_pending`
//! 让 Agent 在 Resting 状态下用已有工具自主完成沉淀（归纳总结、创建/更新节点、
//! 建关系、加 published 标签）。
//!
//! 与 awaken 对称：awaken 是醒来响应外部消息，settle_pending 是沉睡整理内部记忆。
//! 沉淀主体（取数、Budget 分批、Prompt、Trace、状态闭环）已全部下沉到
//! `service/domain/runtime/legacy_settle` 图编排，本模块只保留 Adapter 层职责：
//! 加载 Agent、抢占运行时状态。

use crate::pkg::RequestContext;
use crate::service::dal::agent::AgentFetchOptions;
use crate::service::domain::hr::domain as hr_domain;
use crate::service::domain::runtime::awakening::ThinkingOptions;
use crate::service::domain::runtime::busy_guard::BusyGuard;
use crate::service::domain::runtime::domain as runtime_domain;
use crate::service::domain::runtime::legacy_settle::pending::PENDING_MAX_ITEMS;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SettleMemoryParams, SettleMemoryResponse};
use common::enums::ThinkingScene;
use common::error::{Result, bail_err};

// 单向再导出：CronTrigger 的 agent_rest 全局沉淀仍通过本模块取 `agent_id`，
// 保持既有引用路径不变；实现只有 Domain 里一份。
pub(crate) use crate::service::domain::runtime::legacy_settle::pending::short_term_of;

/// 取出短期记忆索引（可变）
///
/// 状态闭环已下沉到 Domain，本模块不再需要改状态，仅测试构造场景使用。
#[cfg(test)]
fn short_term_of_mut(
    mem: &mut crate::models::memory::Memory,
) -> Option<&mut crate::models::memory::ShortTermMemoryIndexPo> {
    match &mut mem.po {
        crate::models::memory::MemoryPo::ShortTerm(index) => Some(index),
        _ => None,
    }
}

/// 加载 Agent（含 tools + skills）并唤醒 Brain，然后调用 `settle_pending` 执行沉淀
///
/// **神经工具路径入口**（Agent 在自己的思考里主动调 `settle_memory`）。
/// 定时触发路径请用 [`settle_agent_exclusive`]（忙时不丢弃、交给队列重排）。
///
/// 一次 run 会按上下文预算分批扫完所有未沉淀记忆；**只有终态 `Final` 的批次**
/// 才把记忆标记为 Settled，溢出 / 超轮次 / 取消的批次保持 Active 留给下一次。
///
/// # 返回
/// 本次真正沉淀（标记 Settled）的记忆条数（0 表示无待沉淀，或被状态挡下已跳过）
pub(crate) async fn load_and_settle(
    ctx: RequestContext,
    agent_id: &str,
    settle_limit: usize,
) -> Result<usize> {
    // 预检查：Agent 必须空闲才能进入睡眠，避免覆盖 Busy 状态
    //
    // ⚠️ 这条预检查只服务**神经工具路径**（Agent 在自己的思考里主动调 settle_memory）。
    // 定时触发路径不要在这里「忙就跳过」——跳过即静默丢失（触发器已把 next_run_at 推到
    // 下一个 cron 点，日触发 = 次日），那条链路走 [`settle_agent_exclusive`]，
    // 由调用方（AOP 队列消费者）在抢占失败时排期重试。
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

    settle_body(ctx, agent_id, settle_limit).await
}

/// 沉淀尝试结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettleAttempt {
    /// 已沉淀 N 条；N = 0 表示无待沉淀记忆（正常空跑）
    Settled(usize),
    /// Agent 正忙 / 休息中，本次未执行 —— 调用方应排期重试
    ///
    /// 合并消费者后此分支在正常路径**不可达**：`agent.settle.requested` 与
    /// `message.created` 同队列同 `order_key = agent_id`，且本消费者是唯一会把 Agent
    /// 置忙的链路。保留为「有人绕过队列把 Agent 置忙」的防御性不变量，不静默跳过。
    Busy,
}

/// 独占式沉淀：先原子抢占 Agent，抢不到即回 [`SettleAttempt::Busy`]（调用方排期重试）
///
/// **定时触发链路专用**（`consumer/message.rs` 的 `agent.awakening` 消费者，
/// 见其 `handle_settle_request`）。与 [`load_and_settle`] 的差别：
/// - 不「返回 0 了事」，而是把「Agent 正忙」如实告诉调用方 → 上抛 nack 重投，不静默丢
/// - 用 `try_set_resting` 原子抢占，避免「先查询、后设状态」之间被消息链路插入，
///   反过来覆盖一场正在跑的唤醒（与消息侧 `try_set_busy` 同构）
///
/// 抢占成功后立刻挂 [`BusyGuard`]：从抢占点到 `settle_pending` 自己 `set_resting`
/// 之间的任何 `?` 提早返回（Agent 不存在 / Brain 装配失败 / 无待沉淀记忆）都必须把
/// Agent 放回 Idle，否则它会永久卡在 Resting，后续消息全部 nack 重投。
pub(crate) async fn settle_agent_exclusive(
    ctx: RequestContext,
    agent_id: &str,
    settle_limit: usize,
) -> Result<SettleAttempt> {
    let acquired = crate::pkg::agent_runtime_state::AgentRuntimeStateManager::global()
        .try_set_resting(agent_id);
    if !acquired {
        return Ok(SettleAttempt::Busy);
    }
    let _guard = BusyGuard::new(agent_id.to_string());

    Ok(SettleAttempt::Settled(
        settle_body(ctx, agent_id, settle_limit).await?,
    ))
}

/// 沉淀正文：加载 Agent → 唤醒 Brain → settle_pending
///
/// 调用方**必须已经持有** Agent 状态（唤醒中的 Busy，或抢占到的 Resting），
/// 本函数内部不再做任何状态判定。
///
/// 取数 / 按上下文预算分批 / Prompt 装配 / 每批写 Trace / 状态闭环全部在
/// Domain 图编排 `legacy_settle` 里，本函数只负责 Adapter 侧的入参准备。
async fn settle_body(ctx: RequestContext, agent_id: &str, settle_limit: usize) -> Result<usize> {
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
    // 必须先装配 Brain：分批评判依赖模型的上下文预算，
    // 而预算要从 brain.model_provider 的配置里取。
    let ctx = runtime_domain()
        .awakening()
        .wake_agent_brain(ctx, &mut agent)
        .await?;

    // 3. 沉淀主体（一次 run 扫完所有未沉淀记忆，按上下文预算分批）
    let options = ThinkingOptions::for_scene(ThinkingScene::Settle);
    let settled = runtime_domain()
        .awakening()
        .settle_pending(ctx.clone(), &agent, &options, settle_limit)
        .await?;

    Ok(settled)
}

#[register_handler_tool(
    id = "settle_memory",
    name = "Settle Working Memory",
    description = "Trigger the agent's rest-and-consolidate cycle: pending working-memory entries are handed to the agent, which autonomously distills them into long-term knowledge nodes and relations; processed entries are then marked Settled. Returns the number settled (0 if nothing is pending or the agent is not idle).",
    params = "common::api::SettleMemoryParams",
    neural,
    tags = "ontology"
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
    // 不传 limit 时用自适应上限：实际批量由 token 预算决定（见 settle_budget_tokens），
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
#[path = "settle_memory_tests.rs"]
mod tests;
