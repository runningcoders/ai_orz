//! Message Delivery 具体实现

use crate::models::message::Message;
use crate::models::message::MessagePo;
use crate::pkg::RequestContext;
use crate::service::domain::message::MessageDomainImpl;
use crate::service::domain::message::{MessageDelivery, MessageManagement, SendToAgentCommand, SendToUserCommand};
use common::enums::{MessageRole, MessageStatus, MessageType};

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
    ) -> Result<Message, crate::error::AppError> {
        // 创建消息 PO - 使用 Builder 模式或直接 new
        let po = MessagePo::new(
            generate_id(),
            cmd.project_id.map(|s| s.to_string()),
            cmd.task_id.map(|s| s.to_string()),
            cmd.from_id.to_string(),
            cmd.to_agent_id.to_string(),
            cmd.from_role,
            MessageRole::Agent,
            MessageType::Text,
            cmd.content.to_string(),
            None, // file_type
            Default::default(), // file_meta - 需要 FileMeta 类型，用 Default
            cmd.reply_to_id.map(|s| s.to_string()),
            cmd.from_id.to_string(), // created_by
        );

        let message = Message::from_po(po);
        self.message_dal.save_message(ctx.clone(), &message).await?;

        Ok(message)
    }

    async fn send_to_user(
        &self,
        ctx: RequestContext,
        cmd: SendToUserCommand<'_>,
    ) -> Result<Message, crate::error::AppError> {
        // Agent 发送给用户，发送者角色固定为 Agent，接收者角色固定为 User
        let po = MessagePo::new(
            generate_id(),
            cmd.project_id.map(|s| s.to_string()),
            cmd.task_id.map(|s| s.to_string()),
            cmd.from_agent_id.to_string(),
            cmd.to_user_id.to_string(),
            MessageRole::Agent,
            MessageRole::User,
            MessageType::Text,
            cmd.content.to_string(),
            None, // file_type
            Default::default(), // file_meta
            cmd.reply_to_id.map(|s| s.to_string()),
            cmd.from_agent_id.to_string(), // created_by
        );

        let message = Message::from_po(po);
        self.message_dal.save_message(ctx.clone(), &message).await?;

        Ok(message)
    }

    async fn dequeue_next(
        &self,
        ctx: RequestContext,
    ) -> Result<Option<Message>, crate::error::AppError> {
        self.message_dal.dequeue_next_message(ctx).await
    }

    async fn ack(
        &self,
        ctx: RequestContext,
        message_id: &str,
    ) -> Result<(), crate::error::AppError> {
        // 先确认出队
        self.message_dal.ack_message(ctx.clone(), message_id).await?;
        // 更新消息状态为 Processed - clone ctx 因为需要用两次
        self.message_dal.update_status(ctx, message_id, MessageStatus::Processed).await?;
        Ok(())
    }

    async fn nack(
        &self,
        ctx: RequestContext,
        message_id: &str,
    ) -> Result<(), crate::error::AppError> {
        // 放回队列
        self.message_dal.nack_message(ctx.clone(), message_id).await?;
        // 更新消息状态回到 Pending - clone ctx 因为需要用两次
        self.message_dal.update_status(ctx, message_id, MessageStatus::Pending).await?;
        Ok(())
    }
}
