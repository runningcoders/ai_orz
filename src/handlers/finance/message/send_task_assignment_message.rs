//! Handler: 发送任务分配消息（神经工具）

use crate::pkg::RequestContext;
use crate::service::domain::message::{self, SendTaskAssignmentCommand};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SendTaskAssignmentMessageParams, SendTaskAssignmentMessageResponse};
use common::error::Result;

/// 发送任务分配消息
///
/// Agent 通过此工具给其他 Agent 分配任务。
/// 消息发送后立即返回，接收 Agent 在下一轮 awaken 中收到任务分配通知。
#[register_handler_tool(
    id = "send_task_assignment_message",
    name = "Assign Task via Message",
    description = "Send a structured task assignment message (task_id, task_title, optional task_description) to another agent. Returns immediately with the message_id; the receiving agent picks up the assignment at its next awakening. Use send_message_to_agent for plain conversation.",
    params = "common::api::SendTaskAssignmentMessageParams",
    neural,
    tags = "messaging"
)]
#[generate_http_handler]
pub async fn send_task_assignment_message(
    ctx: RequestContext,
    params: SendTaskAssignmentMessageParams,
) -> Result<SendTaskAssignmentMessageResponse> {
    // 调用方身份由 ctx 封装方法统一提供
    let from_id = ctx.caller_id_or_system();
    let from_role = ctx.caller_role();

    let cmd = SendTaskAssignmentCommand {
        task_id: &params.task_id,
        task_title: &params.task_title,
        task_description: params.task_description.as_deref(),
        from_id: &from_id,
        from_role,
        to_agent_id: &params.to_agent_id,
        project_id: params.project_id.as_deref(),
    };

    let message = message::domain()
        .delivery()
        .send_task_assignment(ctx, cmd)
        .await?;

    Ok(SendTaskAssignmentMessageResponse {
        message_id: message.po.id,
    })
}
