//! Message Management 具体实现

use crate::error::AppError;
use crate::models::message::Message;
use crate::pkg::RequestContext;
use crate::service::dao::message::MessageQuery;
use crate::service::domain::message::MessageDomainImpl;
use crate::service::domain::message::{MessageDelivery, MessageManagement};
use common::enums::MessageStatus;

#[async_trait::async_trait]
impl MessageManagement for MessageDomainImpl {
    async fn query(
        &self,
        ctx: RequestContext,
        query: MessageQuery,
    ) -> Result<Vec<Message>, AppError> {
        // Domain 层可以在这里添加业务逻辑：
        // - 权限校验
        // - 数据过滤
        // - 查询前的业务规则验证
        self.message_dal.query(ctx, query).await
    }

    async fn list_by_task_id(
        &self,
        ctx: RequestContext,
        task_id: &str,
    ) -> Result<Vec<Message>, AppError> {
        // 语法糖：调用通用查询，默认不限制条数
        self.query(ctx, MessageQuery {
            task_id: Some(task_id.to_string()),
            limit: None,
            ..Default::default()
        }).await
    }

    async fn list_by_project_id(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<Vec<Message>, AppError> {
        // 语法糖：调用通用查询，默认不限制条数
        self.query(ctx, MessageQuery {
            project_id: Some(project_id.to_string()),
            limit: None,
            ..Default::default()
        }).await
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
