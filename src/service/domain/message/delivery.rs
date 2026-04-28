//! Message Delivery 具体实现

use crate::models::message::Message;
use crate::models::message::MessagePo;
use crate::pkg::RequestContext;
use crate::service::domain::message::MessageDomainImpl;
use crate::service::domain::message::{MessageDelivery, MessageManagement};
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
        from_id: &str,
        from_role: MessageRole,
        to_agent_id: &str,
        content: &str,
        project_id: Option<&str>,
        task_id: Option<&str>,
    ) -> Result<Message, crate::error::AppError> {
        // 创建消息 PO - 参数顺序匹配 MessagePo::new
        // id, project_id, task_id, from_id, to_id, from_role, to_role, message_type, content, file_type, file_meta, created_by
        let po = MessagePo::new(
            generate_id(),
            project_id.map(|s| s.to_string()),
            task_id.map(|s| s.to_string()),
            from_id.to_string(),
            to_agent_id.to_string(),
            from_role,
            MessageRole::Agent,
            MessageType::Text,
            content.to_string(),
            None, // file_type
            Default::default(), // file_meta - 需要 FileMeta 类型，用 Default
            from_id.to_string(), // created_by
        );

        let message = Message::from_po(po);
        self.message_dal.save_message(ctx.clone(), &message).await?;

        Ok(message)
    }

    async fn send_to_user(
        &self,
        ctx: RequestContext,
        from_agent_id: &str,
        to_user_id: &str,
        content: &str,
        project_id: Option<&str>,
        task_id: Option<&str>,
    ) -> Result<Message, crate::error::AppError> {
        // Agent 发送给用户，发送者角色固定为 Agent，接收者角色固定为 User
        let po = MessagePo::new(
            generate_id(),
            project_id.map(|s| s.to_string()),
            task_id.map(|s| s.to_string()),
            from_agent_id.to_string(),
            to_user_id.to_string(),
            MessageRole::Agent,
            MessageRole::User,
            MessageType::Text,
            content.to_string(),
            None, // file_type
            Default::default(), // file_meta
            from_agent_id.to_string(), // created_by
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
