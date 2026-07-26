//! Message Management 具体实现

use crate::models::message::Message;
use crate::pkg::RequestContext;
use crate::service::dao::message::{MessageQuery, MessageSearch};
use crate::service::domain::message::MessageDomainImpl;
use crate::service::domain::message::MessageManagement;
use common::enums::MessageStatus;
use common::error::Result;

#[async_trait::async_trait]
impl MessageManagement for MessageDomainImpl {
    async fn query(&self, ctx: RequestContext, query: MessageQuery) -> Result<Vec<Message>> {
        // Domain 层可以在这里添加业务逻辑：
        // - 权限校验
        // - 数据过滤
        // - 查询前的业务规则验证
        self.message_dal.query(ctx, query).await
    }

    async fn list_by_task_id(&self, ctx: RequestContext, task_id: &str) -> Result<Vec<Message>> {
        let ctx = ctx.to_builder().task_id(task_id).build();
        // 语法糖：调用通用查询，默认不限制条数
        self.query(
            ctx,
            MessageQuery {
                task_id: Some(task_id.to_string()),
                limit: None,
                ..Default::default()
            },
        )
        .await
    }

    async fn list_by_project_id(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<Vec<Message>> {
        let ctx = ctx.to_builder().project_id(project_id).build();
        // 语法糖：调用通用查询，默认不限制条数
        self.query(
            ctx,
            MessageQuery {
                project_id: Some(project_id.to_string()),
                limit: None,
                ..Default::default()
            },
        )
        .await
    }

    async fn get_by_id(&self, ctx: RequestContext, message_id: &str) -> Result<Option<Message>> {
        self.message_dal.find_by_id(ctx, message_id).await
    }

    async fn update_status(
        &self,
        ctx: RequestContext,
        message_id: &str,
        status: MessageStatus,
    ) -> Result<()> {
        self.message_dal
            .update_status(ctx, message_id, status)
            .await
    }

    async fn delete_by_id(&self, ctx: RequestContext, message_id: &str) -> Result<()> {
        self.message_dal.delete_message(ctx, message_id).await
    }

    async fn cleanup_conversation(&self, ctx: RequestContext, task_id: &str) -> Result<()> {
        // DAL delete_by_task_id 直接完成删除
        self.message_dal.delete_by_task_id(ctx, task_id).await
    }

    async fn search(&self, ctx: RequestContext, search: MessageSearch) -> Result<Vec<Message>> {
        self.message_dal.search(ctx, search).await
    }
}
