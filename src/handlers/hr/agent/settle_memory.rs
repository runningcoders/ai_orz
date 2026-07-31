//! Handler: 沉淀记忆 - Neural Tool
//!
//! 触发 Agent 进入沉淀工作模式：拼装场景 prompt，直接调用 RuntimeAwakening.sleep_and_settle
//! 让 Agent 在 Resting 状态下用已有工具自主完成沉淀（归纳总结、创建/更新节点、建关系、加 published 标签）。
//!
//! 与 awaken 对称：awaken 是醒来响应外部消息，sleep_and_settle 是沉睡整理内部记忆。

use crate::pkg::RequestContext;
use crate::service::dal::agent::AgentFetchOptions;
use crate::service::dao::memory::{MemoryQuery, dao as memory_dao};
use crate::service::domain::hr::domain as hr_domain;
use crate::service::domain::runtime::domain as runtime_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SettleMemoryParams, SettleMemoryResponse};
use common::enums::{MemoryStatus, MemoryType};
use common::error::{Result, bail_err};

/// 构建沉淀场景 prompt
///
/// 查询未沉淀的短期记忆（Active 状态），拼装为沉淀场景 prompt。
/// 供 settle_memory handler 和 CronTrigger agent_rest 复用，避免重复。
///
/// # 参数
/// - ctx: 请求上下文
/// - agent_id: Agent ID
/// - limit: 每次处理的短期记忆数量上限
///
/// # 返回
/// - `Ok(None)` 表示无未沉淀记忆（调用方应跳过）
/// - `Ok(Some((prompt, pending_count)))` 表示有待沉淀记忆，prompt 为拼装好的沉淀场景
pub(crate) async fn build_settle_prompt(
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

    let pending_count = short_term_memories.len();
    if pending_count == 0 {
        return Ok(None);
    }

    // 2. 拼装沉淀场景 prompt
    let memories_summary = short_term_memories
        .iter()
        .map(|m| format!("- [id={}] {}", m.id, m.summary))
        .collect::<Vec<_>>()
        .join("\n");

    let settle_prompt = format!(
        r#"【沉淀工作模式触发】

你收到这个消息是因为触发了沉淀流程（类似人脑的睡眠整理记忆）。请进入沉淀工作模式，对以下未沉淀的短期记忆进行归纳整理：

## 待沉淀的短期记忆（{} 条）
{}

## 你的任务

请用已有工具自主完成沉淀：

1. **归纳总结**：对上述短期记忆进行归纳，提炼核心概念、抽象经验、可复用模式（不要记具体细节）
2. **查询已有图谱**：用 search_memory 检查是否已有相关知识点（避免重复节点）
3. **创建/更新节点**：
   - 新知识 → save_long_term_memory 创建节点
   - 已有相似节点 → update_memory 更新节点内容
   - 过大且可拆分的旧节点 → 拆分为子节点 + 概述父节点 + contains 关系
4. **建立关系**：用 save_long_term_memory 的 relations 参数建立节点间关系（related/contains/depends 等）
5. **评估共享**：判断哪些节点对蜂巢有共享价值，用 update_memory 的 node_tags 字段加 'published' 标签
6. **标记完成**：每条短期记忆沉淀完成后，用 update_memory 把它的 status 改为 'settled'

## 认知要点

- 图谱是活的，每次沉淀都是迭代优化，不是机械合并
- 记抽象不记细节，可复用模式才沉淀
- 新老知识交替不是覆盖是迭代，推翻时用 opposite 关系保留痕迹
- published 标签让节点全局共享，通过共享节点作为桥梁发现跨 Agent 的知识网络
- 详见"记忆认知"技能的沉淀机制和新老知识交替章节

开始沉淀吧。"#,
        pending_count, memories_summary
    );

    Ok(Some((settle_prompt, pending_count)))
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
    // 1. 查询未沉淀短期记忆 + 拼装沉淀场景 prompt
    let (settle_prompt, pending_count) =
        match build_settle_prompt(&ctx, agent_id, settle_limit).await? {
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
    let ctx = runtime_domain()
        .awakening()
        .wake_agent_brain(ctx, &mut agent)
        .await?;

    // 4. 沉睡沉淀（Resting 状态 + think + 写 Trace）
    runtime_domain()
        .awakening()
        .sleep_and_settle(ctx, &agent, &settle_prompt)
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
