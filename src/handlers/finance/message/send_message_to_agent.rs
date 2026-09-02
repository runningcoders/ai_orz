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

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain as hr_domain;
use crate::service::domain::message::{self, SendToAgentCommand};
use crate::service::domain::project::domain as project_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{AgentMatchCriteria, SendMessageToAgentParams, SendMessageToAgentResponse};
use common::constants::agent_roles::ROLE_RECEPTION;
use common::error::Result;

/// Send a message to another AI agent (for collaboration)
#[register_handler_tool(
    id = "send_message_to_agent",
    name = "Send Message to Agent",
    description = "Send a text message to another AI agent, which will be awakened to process it; supports attachments and reply threading. If to_agent_id is omitted it resolves to the project's owner agent or the default receptionist agent. Returns message_id. For assigning tasks use send_task_assignment_message.",
    params = "common::api::SendMessageToAgentParams",
    tags = "collaboration"
)]
#[generate_http_handler]
pub async fn send_message_to_agent(
    ctx: RequestContext,
    params: SendMessageToAgentParams,
) -> Result<SendMessageToAgentResponse> {
    // 调用方身份由 ctx 封装方法统一提供
    let from_id = ctx.caller_id_or_system();
    let from_role = ctx.caller_role();

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
            if let Some(project_id) = params.project_id.as_deref() {
                let project = project_domain()
                    .project_manage()
                    .get(ctx.clone(), project_id)
                    .await?
                    .ok_or_else(|| {
                        common::error::Error::not_found(format!("Project {} not found", project_id))
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

    let cmd = SendToAgentCommand {
        from_id: &from_id,
        from_role,
        to_agent_id: &to_agent_id,
        content: &params.content,
        project_id: params.project_id.as_deref(),
        task_id: params.task_id.as_deref(),
        reply_to_id: params.reply_to_id.as_deref(),
        attachment_ids: params.attachment_ids.as_deref(),
        message_type: common::enums::MessageType::Text,
    };

    let message = message::domain().delivery().send_to_agent(ctx, cmd).await?;

    Ok(SendMessageToAgentResponse {
        message_id: message.po.id,
    })
}
