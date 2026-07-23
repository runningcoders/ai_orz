//! Message Delivery 具体实现

use common::error::{bail_err, err, Result};
use crate::models::file::FileMeta;
use crate::models::message::Message;
use crate::models::message::MessagePo;
use crate::models::message::TaskAssignmentMessage;
use crate::models::message::ToolCallMessage;
use crate::pkg::RequestContext;
use crate::service::domain::message::MessageDomainImpl;
use crate::service::domain::message::{
    DeliverMessageCommand, MessageDelivery, SendTaskAssignmentCommand, SendToAgentCommand,
    SendToUserCommand, SendToolCallRequestCommand, SendToolCallResultCommand,
    ToolCallExecutionOutcome,
};
use common::enums::{FileType, MessageRole, MessageType};
use serde_json::json;

use crate::enrich_ctx;

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

/// 将 Attachment FileType 映射到对应的 MessageType
fn map_file_type_to_message_type(file_type: FileType) -> MessageType {
    match file_type {
        FileType::Image => MessageType::Image,
        FileType::Audio => MessageType::Audio,
        FileType::Video => MessageType::Video,
        // 文档与二进制文件统一归类为 File
        FileType::Document | FileType::Binary => MessageType::File,
    }
}

#[async_trait::async_trait]
impl MessageDelivery for MessageDomainImpl {
    async fn send_to_agent(
        &self,
        ctx: RequestContext,
        cmd: SendToAgentCommand<'_>,
    ) -> Result<Message> {
        let project_id = cmd
            .project_id
            .or_else(|| ctx.project_id().map(|s| s.as_str()))
            .map(|s| s.to_string());
        let task_id = cmd
            .task_id
            .or_else(|| ctx.task_id().map(|s| s.as_str()))
            .map(|s| s.to_string());

        // 先生成根消息 ID（文本消息的 ID），附件消息使用 reply_to_id 链回根
        let root_msg_id = generate_id();

        // root_id 继承：如果有 reply_to_id，查询父消息的 root_id 作为链根；
        // 否则当前消息自身为链根。修复：之前 root_id 始终为自身 ID，多轮对话
        // 每条消息都是独立 root，无法按 root_id 拉取完整对话链
        let chain_root_id = match cmd.reply_to_id {
            Some(parent_id) => {
                match self.message_dal.find_by_id(ctx.clone(), parent_id).await {
                    Ok(Some(parent)) => {
                        parent.po.root_id.unwrap_or_else(|| root_msg_id.clone())
                    }
                    _ => root_msg_id.clone(),
                }
            }
            None => root_msg_id.clone(),
        };

        // 1. 处理附件消息：按数组顺序创建 N 条附件消息
        if let Some(att_ids) = cmd.attachment_ids {
            for att_id in att_ids {
                let attachment = self
                    .attachment_dal
                    .get_by_id(ctx.clone(), att_id)
                    .await?
                    .ok_or_else(|| err!(ResourceNotFound, "Attachment {} not found", att_id))?;

                // 校验归属：附件必须属于当前用户
                if !cmd.from_id.is_empty() && attachment.po.root_user_id != cmd.from_id && ctx.uid() != attachment.po.root_user_id {
                    bail_err!(InvalidRequest, "Attachment {} 不属于当前用户", att_id);
                }

                let att_msg_id = generate_id();
                let file_type = map_file_type_to_message_type(attachment.po.file_type);
                let file_meta = FileMeta::new(
                    attachment.po.relative_path.clone(),
                    attachment.po.mime_type.clone(),
                    attachment.po.size as u64,
                );

                let att_po = MessagePo::new(
                    att_msg_id.clone(),
                    project_id.clone(),
                    task_id.clone(),
                    cmd.from_id.to_string(),
                    cmd.to_agent_id.to_string(),
                    cmd.from_role,
                    MessageRole::Agent,
                    file_type,
                    attachment.po.id.clone(),
                    Some(attachment.po.file_type),
                    file_meta,
                    Some(root_msg_id.clone()),
                    Some(chain_root_id.clone()),
                    ctx.organization_id().cloned(),
                    cmd.from_id.to_string(),
                );

                let att_message = Message::from_po(att_po);
                let att_ctx = enrich_ctx!(&ctx, &att_message);
                self.message_dal.save_message(att_ctx.clone(), &att_message).await?;
            }
        }

        // 2. 创建文本消息（root_id 继承自父消息或自身）
        let po = MessagePo::new(
            root_msg_id.clone(),
            project_id,
            task_id,
            cmd.from_id.to_string(),
            cmd.to_agent_id.to_string(),
            cmd.from_role,
            MessageRole::Agent,
            MessageType::Text,
            cmd.content.to_string(),
            None,
            FileMeta::default(),
            cmd.reply_to_id.map(|s| s.to_string()),
            Some(chain_root_id.clone()),
            ctx.organization_id().cloned(),
            cmd.from_id.to_string(),
        );

        let message = Message::from_po(po);
        let ctx = enrich_ctx!(&ctx, &message);
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

        // root_id 继承：如果有 reply_to_id，查询父消息的 root_id；否则自身为 root
        let root_id = match cmd.reply_to_id {
            Some(parent_id) => {
                match self.message_dal.find_by_id(ctx.clone(), parent_id).await {
                    Ok(Some(parent)) => parent.po.root_id.unwrap_or_else(|| id.clone()),
                    _ => id.clone(),
                }
            }
            None => id.clone(),
        };

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
            Some(root_id),
            ctx.organization_id().cloned(),
            cmd.from_agent_id.to_string(),
        );

        let message = Message::from_po(po);
        let ctx = enrich_ctx!(&ctx, &message);
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

        let mut payload = ToolCallMessage::new_request(
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
        // 填充 ctx 字段，供 consumer 异步路径重建 ctx
        // 修复：之前 from_role=Agent 时 user_id 不设置，log_id/model_* 全部丢失
        payload.from_log_id = Some(ctx.log_id.clone());
        payload.from_user_id = ctx.user_id().cloned();
        payload.from_model_provider_id = ctx.model_provider_id().cloned();
        payload.from_model_name = ctx.model_name().cloned();

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
        let ctx = enrich_ctx!(&ctx, &message);
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
        let ctx = enrich_ctx!(&ctx, &message);
        self.message_dal.save_message(ctx.clone(), &message).await?;

        Ok(message)
    }

    async fn send_task_assignment(
        &self,
        ctx: RequestContext,
        cmd: SendTaskAssignmentCommand<'_>,
    ) -> Result<Message> {
        let id = generate_id();
        let project_id = cmd
            .project_id
            .or_else(|| ctx.project_id().map(|s| s.as_str()))
            .map(|s| s.to_string());

        let payload = TaskAssignmentMessage::new(
            cmd.task_id.to_string(),
            cmd.task_title.to_string(),
            cmd.task_description.map(|s| s.to_string()),
            project_id.clone(),
            cmd.from_id.to_string(),
            cmd.to_agent_id.to_string(),
        );

        let content = serde_json::to_string(&payload)
            .map_err(|e| err!(Internal, "failed to serialize task assignment message").with_source(e))?;

        let po = MessagePo::new(
            id.clone(),
            project_id,
            Some(cmd.task_id.to_string()),
            cmd.from_id.to_string(),
            cmd.to_agent_id.to_string(),
            cmd.from_role,
            MessageRole::Agent,
            MessageType::TaskAssignment,
            content,
            None,
            Default::default(),
            None,
            Some(id),
            ctx.organization_id().cloned(),
            cmd.from_id.to_string(),
        );

        let message = Message::from_po(po);
        let ctx = enrich_ctx!(&ctx, &message);
        self.message_dal.save_message(ctx.clone(), &message).await?;

        Ok(message)
    }

    async fn deliver_message(
        &self,
        ctx: RequestContext,
        cmd: DeliverMessageCommand<'_>,
    ) -> Result<crate::service::dal::message_channel::DeliveryResult> {
        // 1. 投递到已配置的消息渠道（飞书/微信/钉钉等）
        let channel_result = self.message_channel_dal
            .deliver_message(ctx.clone(), cmd.message, cmd.user_id)
            .await?;

        // 2. 投递到 SSE 长连接（如果用户有在线连接）
        let file_meta = cmd.message.file_meta().map(|fm| {
            let name = fm.file_path.rsplit('/').next().unwrap_or(&fm.file_path).to_string();
            common::api::message::FileMetaInfo {
                name,
                mime_type: fm.mime_type.clone(),
                size: fm.file_size,
            }
        });
        let sse_payload = crate::service::dal::message_push::SsePushPayload {
            message_id: cmd.message.id().to_string(),
            project_id: cmd.message.project_id().map(|s| s.to_string()),
            task_id: cmd.message.task_id().map(|s| s.to_string()),
            from_id: cmd.message.from_id().to_string(),
            from_role: cmd.message.from_role() as i32,
            to_id: cmd.message.to_id().to_string(),
            to_role: cmd.message.to_role() as i32,
            message_type: cmd.message.message_type() as i32,
            status: cmd.message.status() as i32,
            content: cmd.message.content().to_string(),
            reply_to_id: cmd.message.reply_to_id().map(|s| s.to_string()),
            created_at: cmd.message.created_at(),
            file_type: cmd.message.file_type().map(|ft| ft as i32),
            file_meta,
        };
        let sse_result = self.message_push_dal
            .push_to_sse(ctx, cmd.user_id, &sse_payload)
            .await;

        let sse_delivered = sse_result.map(|r| r.delivered_count).unwrap_or(0);

        Ok(crate::service::dal::message_channel::DeliveryResult {
            total: channel_result.total,
            success: channel_result.success,
            failed: channel_result.failed,
            details: channel_result.details,
            sse_delivered,
        })
    }

    async fn subscribe_sse(
        &self,
        ctx: RequestContext,
        user_id: &str,
    ) -> Result<super::SubscribeResult> {
        let connection_id = uuid::Uuid::now_v7().to_string();
        let receiver = self.message_push_dal
            .subscribe_sse(ctx, user_id, &connection_id)
            .await;
        Ok(super::SubscribeResult {
            connection_id,
            receiver,
        })
    }

    async fn unsubscribe_sse(
        &self,
        ctx: RequestContext,
        connection_id: &str,
    ) -> Result<()> {
        self.message_push_dal.unsubscribe_sse(ctx, connection_id).await;
        Ok(())
    }
}
