//! Attachment DAL 模块
//!
//! 职责：上传编排、文件名/路径生成、文件类型推断、元数据与文件写入组合。

use common::error::{err, bail_err, Result};
use crate::models::attachment::{Attachment, AttachmentPo, AttachmentUpload, TextAttachmentCreate};
use crate::pkg::RequestContext;
use crate::service::dao::attachment;
use crate::service::dao::attachment::{AttachmentDao, AttachmentQuery};
use common::enums::FileType;
use std::path::Path;
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static ATTACHMENT_DAL: OnceLock<Arc<dyn AttachmentDal + Send + Sync>> = OnceLock::new();

/// 获取 Attachment DAL 单例。
pub fn dal() -> Arc<dyn AttachmentDal + Send + Sync> {
    ATTACHMENT_DAL.get().cloned().unwrap()
}

/// 初始化 Attachment DAL。
pub fn init() {
    let _ = ATTACHMENT_DAL.set(new(attachment::dao()));
}

/// 创建 Attachment DAL（返回 trait 对象）。
pub fn new(
    attachment_dao: Arc<dyn AttachmentDao + Send + Sync>,
) -> Arc<dyn AttachmentDal + Send + Sync> {
    Arc::new(AttachmentDalImpl { attachment_dao })
}

// ==================== DAL 接口 ====================

/// Attachment DAL 接口。
#[async_trait::async_trait]
pub trait AttachmentDal: Send + Sync {
    /// 从上传文件创建通用 Attachment 资产。
    async fn create_from_upload(
        &self,
        ctx: RequestContext,
        upload: AttachmentUpload,
    ) -> Result<Attachment>;

    /// 从小型 UTF-8 文本创建通用 Attachment 资产。
    async fn create_from_text(
        &self,
        ctx: RequestContext,
        create: TextAttachmentCreate,
    ) -> Result<Attachment>;

    /// 根据 ID 获取 Attachment。
    async fn get_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<Attachment>>;

    /// 通用查询。
    async fn query(
        &self,
        ctx: RequestContext,
        query: AttachmentQuery,
    ) -> Result<Vec<Attachment>>;

    /// 删除 Attachment（软删除元数据，不物理删除文件）。
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 读取 Attachment 文件 bytes。
    fn read_file(&self, attachment: &Attachment) -> Result<Vec<u8>>;

    /// 全量替换 Attachment 文件内容，并刷新文件元数据。
    async fn update_file_content(
        &self,
        ctx: RequestContext,
        attachment: &Attachment,
        bytes: &[u8],
    ) -> Result<Attachment>;
}

// ==================== DAL 实现 ====================

/// Attachment DAL 实现。
struct AttachmentDalImpl {
    attachment_dao: Arc<dyn AttachmentDao + Send + Sync>,
}

#[async_trait::async_trait]
impl AttachmentDal for AttachmentDalImpl {
    async fn create_from_upload(
        &self,
        ctx: RequestContext,
        upload: AttachmentUpload,
    ) -> Result<Attachment> {
        let user_id = ctx.uid();
        if user_id.is_empty() {
            bail_err!(InvalidRequest, "当前请求缺少用户上下文");
        }
        if upload.bytes.is_empty() {
            bail_err!(InvalidRequest, "上传文件不能为空");
        }

        let id = uuid::Uuid::now_v7().to_string();
        let extension = infer_extension(&upload.original_name, &upload.mime_type);
        let stored_name = format!("{id}{extension}");
        let relative_path = generate_relative_path(&id, &extension);
        let file_type = infer_file_type(&upload.mime_type, &extension);
        let size = upload.bytes.len() as i64;
        let purpose = upload.purpose.trim().to_string();

        let po = AttachmentPo::new(
            id.clone(),
            upload.original_name,
            stored_name,
            relative_path.clone(),
            upload.mime_type,
            file_type,
            size,
            purpose,
            user_id.clone(),
            user_id,
        );

        self.attachment_dao
            .write_file(&relative_path, &upload.bytes)?;
        self.attachment_dao.insert(ctx, &po).await?;
        Ok(Attachment::from_po(po))
    }

    async fn create_from_text(
        &self,
        ctx: RequestContext,
        create: TextAttachmentCreate,
    ) -> Result<Attachment> {
        let mime_type = create
            .mime_type
            .unwrap_or_else(|| infer_mime_type(&create.file_name));
        let upload = AttachmentUpload {
            original_name: create.file_name,
            mime_type,
            purpose: create.purpose.unwrap_or_default(),
            bytes: create.content.into_bytes(),
        };
        self.create_from_upload(ctx, upload).await
    }

    async fn get_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<Attachment>> {
        let po = self.attachment_dao.find_by_id(ctx, id).await?;
        Ok(po.map(Attachment::from_po))
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: AttachmentQuery,
    ) -> Result<Vec<Attachment>> {
        let list = self.attachment_dao.query(ctx, query).await?;
        Ok(list.into_iter().map(Attachment::from_po).collect())
    }

    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.attachment_dao.delete(ctx, id).await
    }

    fn read_file(&self, attachment: &Attachment) -> Result<Vec<u8>> {
        self.attachment_dao.read_file(&attachment.po.relative_path)
    }

    async fn update_file_content(
        &self,
        ctx: RequestContext,
        attachment: &Attachment,
        bytes: &[u8],
    ) -> Result<Attachment> {
        self.attachment_dao
            .write_file(&attachment.po.relative_path, bytes)?;
        self.attachment_dao
            .update_file_metadata(ctx.clone(), attachment.id(), bytes.len() as i64)
            .await?;
        self.get_by_id(ctx, attachment.id())
            .await?
            .ok_or_else(|| err!(ResourceNotFound, "Attachment {} not found", attachment.id()))
    }
}

fn generate_relative_path(file_id: &str, extension: &str) -> String {
    let now = chrono::Utc::now();
    let date = now.format("%Y%m%d");
    format!("{}/{file_id}{extension}", date)
}

fn infer_extension(original_name: &str, mime_type: &str) -> String {
    if let Some(ext) = Path::new(original_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(sanitize_extension)
        .filter(|value| !value.is_empty())
    {
        return format!(".{ext}");
    }

    match mime_type.split(';').next().unwrap_or_default().trim() {
        "text/markdown" => ".md".to_string(),
        "text/plain" => ".txt".to_string(),
        "text/csv" => ".csv".to_string(),
        "application/json" => ".json".to_string(),
        "application/yaml" => ".yaml".to_string(),
        "application/toml" => ".toml".to_string(),
        "application/pdf" => ".pdf".to_string(),
        "image/png" => ".png".to_string(),
        "image/jpeg" => ".jpg".to_string(),
        "image/gif" => ".gif".to_string(),
        "audio/mpeg" => ".mp3".to_string(),
        "audio/wav" => ".wav".to_string(),
        "video/mp4" => ".mp4".to_string(),
        _ => String::new(),
    }
}

fn infer_mime_type(file_name: &str) -> String {
    match Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "md" => "text/markdown".to_string(),
        "json" => "application/json".to_string(),
        "yaml" | "yml" => "application/yaml".to_string(),
        "toml" => "application/toml".to_string(),
        "csv" => "text/csv".to_string(),
        _ => "text/plain".to_string(),
    }
}

fn sanitize_extension(extension: &str) -> String {
    extension
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(16)
        .collect::<String>()
        .to_ascii_lowercase()
}

fn infer_file_type(mime_type: &str, extension: &str) -> FileType {
    let normalized_mime = mime_type.split(';').next().unwrap_or_default().trim();
    if normalized_mime.starts_with("image/") {
        return FileType::Image;
    }
    if normalized_mime.starts_with("audio/") {
        return FileType::Audio;
    }
    if normalized_mime.starts_with("video/") {
        return FileType::Video;
    }

    match extension
        .trim_start_matches('.')
        .to_ascii_lowercase()
        .as_str()
    {
        "md" | "txt" | "pdf" | "doc" | "docx" | "json" | "csv" | "yaml" | "yml" | "toml" => {
            FileType::Document
        }
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" => FileType::Image,
        "mp3" | "wav" | "flac" | "ogg" => FileType::Audio,
        "mp4" | "mov" | "webm" | "avi" => FileType::Video,
        _ => FileType::Binary,
    }
}