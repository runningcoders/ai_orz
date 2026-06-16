//! Attachment 子模块实现
//!
//! 通用上传文件资产管理，归属 Finance Domain。

use crate::error::AppError;
use crate::models::attachment::{Attachment, AttachmentUpload};
use crate::pkg::RequestContext;
use crate::service::dao::attachment::AttachmentQuery;
use crate::service::domain::finance::{AttachmentManage, FinanceDomainImpl};

#[async_trait::async_trait]
impl AttachmentManage for FinanceDomainImpl {
    async fn create_attachment(
        &self,
        ctx: RequestContext,
        upload: AttachmentUpload,
    ) -> Result<Attachment, AppError> {
        self.attachment_dal.create_from_upload(ctx, upload).await
    }

    async fn get_attachment(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<Attachment>, AppError> {
        self.attachment_dal.get_by_id(ctx, id).await
    }

    async fn query_attachments(
        &self,
        ctx: RequestContext,
        query: AttachmentQuery,
    ) -> Result<Vec<Attachment>, AppError> {
        self.attachment_dal.query(ctx, query).await
    }

    async fn delete_attachment(&self, ctx: RequestContext, id: &str) -> Result<(), AppError> {
        self.attachment_dal.delete(ctx, id).await
    }
}
