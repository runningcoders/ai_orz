//! Message Delivery 具体实现

use common::error::{err, bail_err, Result};
use crate::models::message::Message;
use crate::models::message::MessagePo;
use crate::models::message::ToolCallMessage;
use crate::pkg::RequestContext;
use crate::service::domain::message::MessageDomainImpl;
use crate::service::domain::message::{
    DeliverMessageCommand, MessageDelivery, SendToAgentCommand, SendToUserCommand,
    SendToolCallRequestCommand, SendToolCallResultCommand, ToolCallExecutionOutcome,
};
use common::enums::{MessageRole, MessageStatus, MessageType};
use serde_json::json;

const TOOL_CALL_RESULT_INLINE_CONTENT_LIMIT: usize = 8 * 1024;

fn bounded_inline_tool_result(result: serde_json::Value) -> serde_json::Value {
    match serde_json::to_string(&result) {
        Ok(serialized) if serialized.len() <= TOOL_CALL_RESULT_INLINE_CONTENT_LIMIT => result,
        _ => json!({
            "truncated": true,
            "message": "tool result exceeded inline message limit"
        }),
    }
}

/// 生成新的消息 ID
fn generate_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

#[async_trait::async_trait]
impl MessageDelivery for MessageDomainImpl {
    async fn send_to_agent(
        &self,
        ctx: RequestContext,
        cmd: SendToAgentCommand<'_>,
    ) -> Result<Message> {
        let id = generate_id();
        let project_id = cmd
            .project_id
            .or_else(|| ctx.project_id().map(|s| s.as_str()))
            .map(|s| s.to_string());
        let task_id = cmd
            .task_id
            .or_else(|| ctx.task_id().map(|s| s.as_str()))
            .map(|s| s.to_string());
        let po = MessagePo::new(
            id.clone(),
            project_id,
            task_id,
            cmd.from_id.to_string(),
            cmd.to_agent_id.to_string(),
            cmd.from_role,
            MessageRole::Agent,
            MessageType::Text,
            cmd.content.to_string(),
            None,
            Default::default(),
            cmd.reply_to_id.map(|s| s.to_string()),
            Some(id),
            ctx.organization_id().cloned(),
            cmd.from_id.to_string(),
        );

        let message = Message::from_po(po);
        self.message_dal.save_message(ctx.clone(), &message).await?;

        Ok(message)
    }

    async fn send_to_user(
        &self,
        ctx: RequestContext,
        cmd: SendToUserCommand<'_>,
    ) -> Result<Message> {
        let id = generate_id();
        let project_id = cmd
            .project_id
            .or_else(|| ctx.project_id().map(|s| s.as_str()))
            .map(|s| s.to_string());
        let task_id = cmd
            .task_id
            .or_else(|| ctx.task_id().map(|s| s.as_str()))
            .map(|s| s.to_string());
        let po = MessagePo::new(
            id.clone(),
            project_id,
            task_id,
            cmd.from_agent_id.to_string(),
            cmd.to_user_id.to_string(),
            MessageRole::Agent,
            MessageRole::User,
            MessageType::Text,
            cmd.content.to_string(),
            None,
            Default::default(),
            cmd.reply_to_id.map(|s| s.to_string()),
            Some(id),
            ctx.organization_id().cloned(),
            cmd.from_agent_id.to_string(),
        );

        let message = Message::from_po(po);
        self.message_dal.save_message(ctx.clone(), &message).await?;

        Ok(message)
    }

    async fn send_tool_call_request(
        &self,
        ctx: RequestContext,
        cmd: SendToolCallRequestCommand<'_>,
    ) -> Result<Message> {
        let id = generate_id();
        let project_id = cmd
            .project_id
            .or_else(|| ctx.project_id().map(|s| s.as_str()))
            .map(|s| s.to_string());
        let task_id = cmd
            .task_id
            .or_else(|| ctx.task_id().map(|s| s.as_str()))
            .map(|s| s.to_string());
        let payload = ToolCallMessage::new_request(
            cmd.request_id.to_string(),
            cmd.tool_id.to_string(),
            cmd.tool_name.to_string(),
            project_id,
            task_id,
            cmd.from_agent_id.to_string(),
            cmd.to_executor_id.to_string(),
            cmd.reply_to_id.map(|s| s.to_string()),
            cmd.args,
        );

         let content = serde_json::to_string(&payload)
            .map_err(|e| err!(Internal, "failed to serialize tool call request").with_source(e))?;

        let po = MessagePo::new(
            id.clone(),
            payload.project_id.clone(),
            payload.task_id.clone(),
            payload.from_id.clone(),
            payload.to_id.clone(),
            MessageRole::Agent,
            MessageRole::System,
            MessageType::ToolCallRequest,
            content,
            None,
            Default::default(),
            payload.reply_to_id.clone(),
            Some(id), // root_id
            ctx.organization_id().cloned(),
            payload.from_id.clone(),
        );

        let message = Message::from_po(po);
        self.message_dal.save_message(ctx.clone(), &message).await?;

        Ok(message)
    }

    async fn send_tool_call_result(
        &self,
        ctx: RequestContext,
        cmd: SendToolCallResultCommand<'_>,
    ) -> Result<Message> {
        if cmd.request_message.po.message_type != MessageType::ToolCallRequest {
            bail_err!(InvalidRequest, "request_message must be ToolCallRequest");
        }

         let request: ToolCallMessage = serde_json::from_str(&cmd.request_message.po.content)
            .map_err(|e| err!(InvalidRequest, "invalid tool call request message").with_source(e))?;

        let (mut result_payload, trace_ref) = match cmd.outcome {
            ToolCallExecutionOutcome::Success {
                result,
                result_file_meta,
                trace_ref,
            } => (
                request.new_success_result(bounded_inline_tool_result(result), result_file_meta),
                trace_ref,
            ),
            ToolCallExecutionOutcome::Failure {
                error_message,
                trace_ref,
            } => (request.new_error_result(error_message), trace_ref),
        };
        if let Some(trace_ref) = trace_ref {
            result_payload.trace_ref = Some(trace_ref);
        }

         let content = serde_json::to_string(&result_payload)
            .map_err(|e| err!(Internal, "failed to serialize tool call result").with_source(e))?;

        let id = generate_id();
        let po = MessagePo::new(
            id.clone(),
            result_payload.project_id.clone(),
            result_payload.task_id.clone(),
            result_payload.from_id.clone(),
            result_payload.to_id.clone(),
            MessageRole::System,
            MessageRole::Agent,
            MessageType::ToolCallResult,
            content,
            None,
            Default::default(),
            Some(cmd.request_message.id().to_string()),
            cmd.request_message.po.root_id.clone().or(Some(id)),
            cmd.request_message.po.organization_id.clone(),
            result_payload.from_id.clone(),
        );

        let message = Message::from_po(po);
        self.message_dal.save_message(ctx.clone(), &message).await?;

        Ok(message)
    }

    async fn dequeue_next(
        &self,
        ctx: RequestContext,
    ) -> Result<Option<Message>> {
        self.message_dal.dequeue_next_message(ctx).await
    }

    async fn ack(
        &self,
        ctx: RequestContext,
        message_id: &str,
    ) -> Result<()> {
        // 先确认出队
        self.message_dal
            .ack_message(ctx.clone(), message_id)
            .await?;
        // 更新消息状态为 Processed - clone ctx 因为需要用两次
        self.message_dal
            .update_status(ctx, message_id, MessageStatus::Processed)
            .await?;
        Ok(())
    }

    async fn nack(
        &self,
        ctx: RequestContext,
        message_id: &str,
    ) -> Result<()> {
        // 放回队列
        self.message_dal
            .nack_message(ctx.clone(), message_id)
            .await?;
        // 更新消息状态回到 Pending - clone ctx 因为需要用两次
        self.message_dal
            .update_status(ctx, message_id, MessageStatus::Pending)
            .await?;
        Ok(())
    }

    async fn deliver_message(
        &self,
        ctx: RequestContext,
        cmd: DeliverMessageCommand<'_>,
    ) -> Result<crate::service::dal::message_channel::DeliveryResult> {
        self.message_channel_dal
            .deliver_message(ctx, cmd.message, cmd.user_id)
            .await
    }
}
