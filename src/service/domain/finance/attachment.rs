//! Attachment 子模块实现
//!
//! 通用上传文件资产管理，归属 Finance Domain。

use common::bail_err;
use crate::models::attachment::{
    Attachment, AttachmentGetOptions, AttachmentReadResult, AttachmentTextContent,
    AttachmentUpload, TextAttachmentCreate, TextContentUpdate,
};
use crate::pkg::RequestContext;
use crate::service::dao::attachment::AttachmentQuery;
use crate::service::domain::finance::{AttachmentManage, FinanceDomainImpl};
use std::path::{Component, Path};
use common::error::Result;
use common::err;

const MAX_TEXT_CONTENT_BYTES: usize = 64 * 1024;

#[async_trait::async_trait]
impl AttachmentManage for FinanceDomainImpl {
    async fn create_attachment(
        &self,
        ctx: RequestContext,
        upload: AttachmentUpload,
    ) -> Result<Attachment> {
        self.attachment_dal.create_from_upload(ctx, upload).await
    }

    async fn create_text_attachment(
        &self,
        ctx: RequestContext,
        create: TextAttachmentCreate,
    ) -> Result<Attachment> {
        validate_file_name(&create.file_name)?;
        validate_text_content(&create.content)?;
        validate_text_size(create.content.as_bytes().len())?;
        if let Some(mime_type) = &create.mime_type {
            validate_text_mime(mime_type)?;
        }
        self.attachment_dal.create_from_text(ctx, create).await
    }

    async fn get_attachment(
        &self,
        ctx: RequestContext,
        id: &str,
        options: AttachmentGetOptions,
    ) -> Result<Option<Attachment>> {
        let Some(attachment) = self.attachment_dal.get_by_id(ctx.clone(), id).await? else {
            return Ok(None);
        };

        if attachment.po.root_user_id != ctx.uid() {
            return Ok(None);
        }

        if !options.include_file_content {
            return Ok(Some(attachment));
        }

        let bytes = self.attachment_dal.read_file(&attachment)?;
        let read_result = AttachmentReadResult {
            relative_path: attachment.po.relative_path.clone(),
            size: bytes.len(),
            bytes,
        };
        Ok(Some(attachment.with_read_result(read_result)))
    }

    async fn get_attachment_text_content(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<AttachmentTextContent>> {
        let Some(attachment) = self
            .get_attachment(
                ctx,
                id,
                AttachmentGetOptions {
                    include_file_content: true,
                },
            )
            .await?
        else {
            return Ok(None);
        };
        attachment_to_text_content(attachment).map(Some)
    }

    async fn update_attachment_text_content(
        &self,
        ctx: RequestContext,
        id: &str,
        update: TextContentUpdate,
    ) -> Result<Option<AttachmentTextContent>> {
        validate_text_content(&update.content)?;
        validate_text_size(update.content.as_bytes().len())?;

        let Some(attachment) = self
            .attachment_dal
            .get_by_id(ctx.clone(), id)
            .await?
            .filter(|attachment| attachment.po.root_user_id == ctx.uid())
        else {
            return Ok(None);
        };

        validate_attachment_is_text(&attachment)?;
        if let Some(expected_updated_at) = update.expected_updated_at {
            if expected_updated_at != attachment.po.updated_at {
                bail_err!(Conflict, "Attachment 内容已被其他请求修改，请刷新后重试");
            }
        }

        let updated = self
            .attachment_dal
            .update_file_content(ctx, &attachment, update.content.as_bytes())
            .await?;
        Ok(Some(AttachmentTextContent {
            size: update.content.as_bytes().len() as u64,
            content: update.content,
            encoding: "utf-8".to_string(),
            updated_at: updated.po.updated_at,
            attachment: updated,
        }))
    }

    async fn query_attachments(
        &self,
        ctx: RequestContext,
        query: AttachmentQuery,
    ) -> Result<Vec<Attachment>> {
        self.attachment_dal.query(ctx, query).await
    }

    async fn delete_attachment(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.attachment_dal.delete(ctx, id).await
    }
}

fn attachment_to_text_content(attachment: Attachment) -> Result<AttachmentTextContent> {
    validate_attachment_is_text(&attachment)?;
    let read_result = attachment
        .read_results
        .first()
        .ok_or_else(|| bail_err!(Internal, "Attachment 文件内容未装配"))?;
    validate_text_size(read_result.bytes.len())?;
    let content = std::str::from_utf8(&read_result.bytes)
        .map_err(|_| bail_err!(InvalidRequest, "Attachment 内容不是 UTF-8 文本"))?
        .to_string();
    validate_text_content(&content)?;
    Ok(AttachmentTextContent {
        size: read_result.size as u64,
        content,
        encoding: "utf-8".to_string(),
        updated_at: attachment.po.updated_at,
        attachment,
    })
}

fn validate_file_name(file_name: &str) -> Result<()> {
    let trimmed = file_name.trim();
    if trimmed.is_empty() {
        bail_err!(InvalidRequest, "file_name 不能为空");
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
        bail_err!(InvalidRequest, "file_name 不能包含路径分隔符或路径穿越片段");
    }
    let path = Path::new(trimmed);
    if path.is_absolute() {
        bail_err!(InvalidRequest, "file_name 不能是绝对路径");
    }
    if path.components().count() != 1 {
        bail_err!(InvalidRequest, "file_name 必须是单个文件名");
    }
    if !matches!(path.components().next(), Some(Component::Normal(_))) {
        bail_err!(InvalidRequest, "file_name 包含非法路径片段");
    }
    Ok(())
}

fn validate_text_content(content: &str) -> Result<()> {
    if content.as_bytes().contains(&0) {
        bail_err!(InvalidRequest, "文本内容包含二进制 NUL 字节");
    }
    Ok(())
}

fn validate_text_size(size: usize) -> Result<()> {
    if size > MAX_TEXT_CONTENT_BYTES {
        bail_err!(PayloadTooLarge, "文本内容超过 {} bytes 限制", MAX_TEXT_CONTENT_BYTES);
    }
    Ok(())
}

fn validate_text_mime(mime_type: &str) -> Result<()> {
    let normalized = mime_type.split(';').next().unwrap_or_default().trim();
    if is_text_mime(normalized) {
        return Ok(());
    }
    bail_err!(InvalidRequest, "不支持的文本 MIME 类型: {}", mime_type);
}

fn validate_attachment_is_text(attachment: &Attachment) -> Result<()> {
    if attachment.po.file_type != common::enums::FileType::Document {
        bail_err!(InvalidRequest, "Attachment 不是文本类文件");
    }
    let mime_type = attachment
        .po
        .mime_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim();
    if is_text_mime(mime_type) || is_text_extension(&attachment.po.original_name) {
        return Ok(());
    }
    bail_err!(InvalidRequest, "Attachment 不支持作为简单文本读取");
}

fn is_text_mime(mime_type: &str) -> bool {
    mime_type.starts_with("text/")
        || matches!(
            mime_type,
            "application/json"
                | "application/yaml"
                | "application/x-yaml"
                | "application/toml"
                | "application/xml"
                | "application/javascript"
                | "application/x-javascript"
        )
}

fn is_text_extension(file_name: &str) -> bool {
    matches!(
        Path::new(file_name)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "txt" | "md" | "json" | "yaml" | "yml" | "toml" | "csv" | "xml" | "rs" | "py" | "js" | "ts"
    )
}