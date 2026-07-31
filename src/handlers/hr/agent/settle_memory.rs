//! Handler: 沉淀记忆 - Neural Tool
//!
//! 触发 Agent 进入沉淀工作模式：拼装场景 prompt，通过消息系统给 Agent 自己发消息，
//! Agent 在 awaken 中用已有工具自主完成沉淀（归纳总结、创建/更新节点、建关系、加 published 标签）。

use crate::pkg::RequestContext;
use crate::service::dao::memory::{MemoryQuery, dao as memory_dao};
use crate::service::domain::message::domain as message_domain;
use crate::service::domain::message::SendToAgentCommand;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SettleMemoryParams, SettleMemoryResponse};
use common::enums::{MemoryStatus, MemoryType, MessageRole};
use common::error::{Result, bail_err};

#[register_handler_tool(
    id = "settle_memory",
    name = "settle_memory",
    description = "Trigger the agent's 'rest' process to consolidate recent experiences into structured knowledge. Sends a settlement scenario message to the agent, who will autonomously use available tools to complete the settling process.",
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

    // 1. 查询未沉淀的短期记忆（Active 状态）
    let short_term_memories = memory_dao()
        .query_short_term(
            ctx.clone(),
            MemoryQuery {
                agent_id: Some(agent_id.clone()),
                status: Some(MemoryStatus::Active),
                memory_type: Some(MemoryType::ShortTerm),
                limit: Some(limit),
                ..Default::default()
            },
        )
        .await?;

    let pending_count = short_term_memories.len();
    if pending_count == 0 {
        log_info!(ctx, "settle_memory", "agent_id={}, 无未沉淀的短期记忆", agent_id);
        return Ok(SettleMemoryResponse { settled_count: 0 });
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
        pending_count,
        memories_summary
    );

    // 3. 给 Agent 自己发消息触发 awaken
    let cmd = SendToAgentCommand {
        from_id: &agent_id,
        from_role: MessageRole::System,
        to_agent_id: &agent_id,
        content: &settle_prompt,
        project_id: None,
        task_id: None,
        reply_to_id: None,
        attachment_ids: None,
    };
    message_domain()
        .delivery()
        .send_to_agent(ctx.clone(), cmd)
        .await?;

    log_info!(
        ctx,
        "settle_memory",
        "agent_id={}, 触发沉淀工作模式，待沉淀 {} 条短期记忆",
        agent_id,
        pending_count
    );

    Ok(SettleMemoryResponse { settled_count: pending_count })
}
