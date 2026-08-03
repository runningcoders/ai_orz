//! Handler: 发送任务分配消息（神经工具）

use crate::pkg::RequestContext;
use crate::service::domain::message::{self, SendTaskAssignmentCommand};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SendTaskAssignmentMessageParams, SendTaskAssignmentMessageResponse};
use common::enums::CallerType;
use common::error::Result;

/// 发送任务分配消息
///
/// Agent 通过此工具给其他 Agent 分配任务。
/// 消息发送后立即返回，接收 Agent 在下一轮 awaken 中收到任务分配通知。
#[register_handler_tool(
    id = "send_task_assignment_message",
    name = "send_task_assignment_message",
    description = "Send a task assignment message to another agent",
    params = "common::api::SendTaskAssignmentMessageParams",
    neural,
    tags = "messaging"
)]
#[generate_http_handler]
pub async fn send_task_assignment_message(
    ctx: RequestContext,
    params: SendTaskAssignmentMessageParams,
) -> Result<SendTaskAssignmentMessageResponse> {
    // 根据 caller_type 选择 from_id 来源和 from_role（补齐 System 分支）
    let (from_id, from_role) = match ctx.caller_type() {
        CallerType::Agent => (
            ctx.agent_id().map(|s| s.to_string()).unwrap_or_default(),
            common::enums::MessageRole::Agent,
        ),
        CallerType::User => (ctx.uid(), common::enums::MessageRole::User),
        CallerType::System => ("system".to_string(), common::enums::MessageRole::System),
    };

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
