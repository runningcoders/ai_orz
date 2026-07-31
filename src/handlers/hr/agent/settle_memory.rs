//! Handler: 沉淀记忆 - Neural Tool
//!
//! 触发 Agent 进入沉淀工作模式：查询未沉淀短期记忆，生成编号摘要，直接调用
//! RuntimeAwakening.sleep_and_settle 让 Agent 在 Resting 状态下用已有工具自主完成沉淀
//! （归纳总结、创建/更新节点、建关系、加 published 标签）。
//!
//! 与 awaken 对称：awaken 是醒来响应外部消息，sleep_and_settle 是沉睡整理内部记忆。
//! 沉淀约束模板（不发消息、只用记忆工具）内聚在 PromptBuilder.build_sleep_prompt，
//! 本模块只负责生成待沉淀记忆摘要。

use crate::pkg::RequestContext;
use crate::service::dal::agent::AgentFetchOptions;
use crate::service::dao::memory::{MemoryQuery, dao as memory_dao};
use crate::service::domain::hr::domain as hr_domain;
use crate::service::domain::runtime::awakening::{ThinkingOptions, ThinkingScene};
use crate::service::domain::runtime::domain as runtime_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SettleMemoryParams, SettleMemoryResponse};
use common::enums::{MemoryStatus, MemoryType};
use common::error::{Result, bail_err};

/// 构建待沉淀短期记忆的编号摘要
///
/// 查询未沉淀的短期记忆（Active 状态），生成编号摘要字符串。
/// 约束模板已内聚到 PromptBuilder.build_sleep_prompt，本函数只返回记忆摘要。
/// 供 settle_memory handler 和 CronTrigger agent_rest 复用，避免重复。
///
/// # 参数
/// - ctx: 请求上下文
/// - agent_id: Agent ID
/// - limit: 每次处理的短期记忆数量上限
///
/// # 返回
/// - `Ok(None)` 表示无未沉淀记忆（调用方应跳过）
/// - `Ok(Some((summary, count)))` 表示有待沉淀记忆，summary 为编号摘要字符串
pub(crate) async fn build_pending_memories_summary(
    ctx: &RequestContext,
    agent_id: &str,
    limit: usize,
) -> Result<Option<(String, usize)>> {
    // 1. 查询未沉淀的短期记忆（Active 状态）
    let short_term_memories = memory_dao()
        .query_short_term(
            ctx.clone(),
            MemoryQuery {
                agent_id: Some(agent_id.to_string()),
                status: Some(MemoryStatus::Active),
                memory_type: Some(MemoryType::ShortTerm),
                limit: Some(limit),
                ..Default::default()
            },
        )
        .await?;

    let count = short_term_memories.len();
    if count == 0 {
        return Ok(None);
    }

    // 2. 拼接编号摘要（约束模板由 builder.build_sleep_prompt 注入）
    let mut summary = String::new();
    for (i, mem) in short_term_memories.iter().enumerate() {
        summary.push_str(&format!("{}. [id={}] {}\n", i + 1, mem.id, mem.summary));
    }
    Ok(Some((summary, count)))
}

/// 加载 Agent（含 tools + skills）并唤醒 Brain，然后调用 sleep_and_settle 执行沉淀
///
/// 供 settle_memory handler 和 CronTrigger agent_rest 复用。
///
/// # 返回
/// 待沉淀的短期记忆数量（0 表示无待沉淀，已跳过）
pub(crate) async fn load_and_settle(
    ctx: RequestContext,
    agent_id: &str,
    settle_limit: usize,
) -> Result<usize> {
    // 1. 查询未沉淀短期记忆 + 生成编号摘要
    let (summary, pending_count) =
        match build_pending_memories_summary(&ctx, agent_id, settle_limit).await? {
            Some(pair) => pair,
            None => return Ok(0),
        };

    // 2. 加载 Agent（含 tools + skills）
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

    // 3. 唤醒 Brain（装配 Cortex + 注入 Auto 工具到 Rig）
    //    Settle 场景：wake_agent_brain 会过滤 Auto 工具，只保留记忆相关
    let ctx = runtime_domain()
        .awakening()
        .wake_agent_brain(ctx, &mut agent, ThinkingScene::Settle)
        .await?;

    // 4. 沉睡沉淀（Resting 状态 + think + 写 Trace）
    //    sleep_and_settle 会过滤 Manual 工具和 skill（只保留记忆相关），
    //    调用 builder.build_sleep_prompt 生成沉淀 prompt（约束模板内聚在 builder）
    let options = ThinkingOptions::for_scene(ThinkingScene::Settle);
    runtime_domain()
        .awakening()
        .sleep_and_settle(ctx, &agent, &summary, &options)
        .await?;

    Ok(pending_count)
}

#[register_handler_tool(
    id = "settle_memory",
    name = "settle_memory",
    description = "Trigger the agent's 'rest' process to consolidate recent experiences into structured knowledge. Directly invokes sleep_and_settle, the symmetric counterpart of awaken, letting the agent autonomously use available tools to complete the settling process.",
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
    let limit = params.limit.unwrap_or(10);

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
