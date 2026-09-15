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
    // 发送方 = 消息的发送者本人：后台唤醒场景 ctx 的 caller_type 是 System，
    // 但执行者是被唤醒的 Agent（项目 / 任务 owner Agent），见 message_sender_id /
    // message_sender_role（from_id 落 Agent 时 from_role 必须同为 Agent，避免
    // 「Agent ID + System 角色」的错位记录）
    let from_id = ctx.message_sender_id();
    let from_role = ctx.message_sender_role();

    let cmd = SendTaskAssignmentCommand {
        task_id: &params.task_id,
        task_title: &params.task_title,
        task_description: params.task_description.as_deref(),
        from_id: &from_id,
        from_role,
        to_agent_id: &params.to_agent_id,
        // 写路径归一化：默认会话哨兵折叠回 None（落库仍为 NULL），见
        // `common::constants::message::DEFAULT_CONVERSATION_PROJECT_ID`
        project_id: common::constants::message::normalize_project_id(params.project_id.as_deref()),
    };

    let message = message::domain()
        .delivery()
        .send_task_assignment(ctx, cmd)
        .await?;

    Ok(SendTaskAssignmentMessageResponse {
        message_id: message.po.id,
    })
}
