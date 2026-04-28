//! Message Management 具体实现

use crate::error::AppError;
use crate::models::message::Message;
use crate::pkg::RequestContext;
use crate::service::domain::message::MessageDomainImpl;
use crate::service::domain::message::{MessageDelivery, MessageManagement};
use common::enums::MessageStatus;

#[async_trait::async_trait]
impl MessageManagement for MessageDomainImpl {
    async fn list_by_task_id(
        &self,
        ctx: RequestContext,
        task_id: &str,
    ) -> Result<Vec<Message>, AppError> {
        // 默认不限制条数，返回所有
        self.message_dal.list_by_task_id(ctx, task_id, None).await
    }

    async fn list_by_project_id(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<Vec<Message>, AppError> {
        // 默认不限制条数，返回所有
        self.message_dal.list_by_project_id(ctx, project_id, None).await
    }

    async fn get_by_id(
        &self,
        ctx: RequestContext,
        message_id: &str,
    ) -> Result<Option<Message>, AppError> {
        self.message_dal.find_by_id(ctx, message_id).await
    }

    async fn update_status(
        &self,
        ctx: RequestContext,
        message_id: &str,
        status: MessageStatus,
    ) -> Result<(), AppError> {
        self.message_dal.update_status(ctx, message_id, status).await
    }

    async fn delete_by_id(
        &self,
        ctx: RequestContext,
        message_id: &str,
    ) -> Result<(), AppError> {
        self.message_dal.delete_message(ctx, message_id).await
    }

    async fn cleanup_conversation(
        &self,
        ctx: RequestContext,
        task_id: &str,
    ) -> Result<(), AppError> {
        // DAL delete_by_task_id 直接完成删除
        self.message_dal.delete_by_task_id(ctx, task_id).await
    }
}
