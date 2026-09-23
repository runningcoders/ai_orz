//! Handler: POST /api/v1/messages/agents - Send message to an agent
//!
//! 支持两种对话上下文（协作关系类比）：
//! - 默认对话框（无 project_id）：与前台 Agent 直接沟通
//! - Project 对话框（有 project_id）：在 Project 上下文中沟通
//!
//! to_agent_id 路由优先级：
//! 1. 显式指定优先（用户在前端选定 Agent）
//! 2. 否则查 project（Project 对话框场景），用 project.owner_agent_id
//! 3. 若 project.owner_agent_id 为 None 或 project_id 未指定（默认对话框场景）→ 调 resolve_agent(ctx) 兜底
//!
//! ⚠️ `project_id` 传「默认会话哨兵」等同于不传：入口处先用
//! `common::constants::message::normalize_project_id` 折叠回 `None`，再走上面第 3 条路由。

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain as hr_domain;
use crate::service::domain::message::{self, SendToAgentCommand};
use crate::service::domain::project::domain as project_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{AgentMatchCriteria, SendMessageToAgentParams, SendMessageToAgentResponse};
use common::constants::agent_roles::ROLE_RECEPTION;
use common::error::Result;

/// Send a message to another AI agent (for collaboration)
///
/// `neural` 必需：本工具原先只挂 `collaboration` tag，而该 tag 从未装进任何 Agent 的
/// `installed_tags`（也不在 `BASE_AGENT_PACKS`）⇒ Agent 的工具面里**根本没有这个工具**，
/// 但系统提示词（`builder/default.rs` §2）与 `send_message` 的描述都在叫模型用它。
/// 结果是模型只能退回 `send_message` 并把对方 Agent 的 ID 塞进 `to_user_id`，
/// 消息落成 `to_role=User + to_id=<Agent>` → 投递全失败 → 重试 8 次后静默丢弃。
/// 与孪生工具 `send_message` / `send_task_assignment_message` 对齐打上 `neural`，
/// 让提示词的说法重新可执行。
#[register_handler_tool(
    id = "send_message_to_agent",
    name = "Send Message to Agent",
    description = "Send a text message to another AI agent, which will be awakened to process it; supports attachments and reply threading. If to_agent_id is omitted it resolves to the project's owner agent or the default receptionist agent. Returns message_id. For assigning tasks use send_task_assignment_message. To message a human user use send_message instead.",
    params = "common::api::SendMessageToAgentParams",
    neural,
    tags = "collaboration"
)]
#[generate_http_handler]
pub async fn send_message_to_agent(
    ctx: RequestContext,
    params: SendMessageToAgentParams,
) -> Result<SendMessageToAgentResponse> {
    // 发送方 = 消息的发送者本人：后台唤醒场景 ctx 的 caller_type 是 System，
    // 但执行者是被唤醒的 Agent（项目 / 任务 owner Agent），见 message_sender_id /
    // message_sender_role（from_id 落 Agent 时 from_role 必须同为 Agent，避免
    // 「Agent ID + System 角色」的错位记录）
    let from_id = ctx.message_sender_id();
    let from_role = ctx.message_sender_role();

    // 写路径归一化：默认会话哨兵（`__default__`）折叠回 None。否则它会被当成真实
    // project id 送去查项目（直接 404），或原样落库污染 `messages.project_id`。
    // 见 `common::constants::message::DEFAULT_CONVERSATION_PROJECT_ID`。
    let project_id = common::constants::message::normalize_project_id(params.project_id.as_deref());

    // 路由 to_agent_id（协作关系类比）：
    // 1. 显式指定优先（用户在前端选定 Agent）
    // 2. 否则查 project（Project 对话框场景），用 project.owner_agent_id
    // 3. 若 project.owner_agent_id 为 None 或 project_id 未指定（默认对话框场景）→ 调 resolve_agent(ctx) 兜底
    //
    // 协作关系类比：
    // - 默认对话框=与前台直接沟通（无 project_id），后端走 resolve_agent 兜底
    // - Project 对话框=Project 上下文沟通（有 project_id），后端从 project.owner_agent_id 取
    // - Project 创建由 Agent 内部决策触发，不在本次范围
    let to_agent_id = match params.to_agent_id.as_deref() {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => {
            // 优先从 project.owner_agent_id 取（Project 对话框场景）
            if let Some(pid) = project_id {
                let project = project_domain()
                    .project_manage()
                    .get(ctx.clone(), pid)
                    .await?
                    .ok_or_else(|| {
                        common::error::Error::not_found(format!("Project {} not found", pid))
                    })?;

                if let Some(agent_id) = project.po.owner_agent_id {
                    agent_id
                } else {
                    // project 未绑定 agent，走 resolve_agent 兜底（web 前台角色）
                    let agent = hr_domain()
                        .resolve_agent(ctx.clone(), AgentMatchCriteria::by_role(ROLE_RECEPTION))
                        .await?
                        .ok_or_else(|| common::error::Error::not_found("无可用前台 Agent"))?;
                    agent.po.id
                }
            } else {
                // 默认对话框场景（无 project_id），web 前台角色
                let agent = hr_domain()
                    .resolve_agent(ctx.clone(), AgentMatchCriteria::by_role(ROLE_RECEPTION))
                    .await?
                    .ok_or_else(|| common::error::Error::not_found("无可用前台 Agent"))?;
                agent.po.id
            }
        }
    };

    let reply_to_id = super::auto_reply_to_id(&ctx, params.reply_to_id.as_deref());
    // 知会模式：发送方声明「无需回复来源方」，落 AgentNotify 类型，
    // 接收方 Framework 侧据此跳过 Final 自动回发（防 Agent 间协作乒乓）
    let notify_only = params.notify_only.unwrap_or(false);
    let cmd = SendToAgentCommand {
        from_id: &from_id,
        from_role,
        to_agent_id: &to_agent_id,
        content: &params.content,
        project_id,
        task_id: params.task_id.as_deref(),
        reply_to_id: reply_to_id.as_deref(),
        external_key: None,
        attachment_ids: params.attachment_ids.as_deref(),
        message_type: if notify_only {
            common::enums::MessageType::AgentNotify
        } else {
            common::enums::MessageType::Text
        },
    };

    let message = message::domain().delivery().send_to_agent(ctx, cmd).await?;

    Ok(SendMessageToAgentResponse {
        message_id: message.po.id,
    })
}
