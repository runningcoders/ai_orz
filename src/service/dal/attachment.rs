//! Attachment DAL 模块
//!
//! 职责：上传编排、文件名/路径生成、文件类型推断、元数据与文件写入组合。

use crate::models::attachment::{
    Attachment, AttachmentDescriptionUpdate, AttachmentPo, AttachmentUpload, TextAttachmentCreate,
};
use crate::pkg::RequestContext;
use crate::service::dao::attachment;
use crate::service::dao::attachment::{AttachmentDao, AttachmentQuery};
use common::enums::FileType;
use common::error::{Result, bail_err, err};
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

/// 测试专用：设置一个全新的 Attachment DAL 单例（绕过 init 流程，便于测试隔离）
pub fn set_for_test(dal: Arc<dyn AttachmentDal + Send + Sync>) {
    // OnceLock 不支持覆盖，所以使用一个独立的"测试模式"标志
    // 这里直接 set，如果已设置会忽略，便于每个测试用临时 DAL
    // 注意：OnceLock::set 返回 Result，如果已初始化则忽略新值
    let _ = ATTACHMENT_DAL.set(dal);
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
    async fn get_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Attachment>>;

    /// 通用查询。
    async fn query(
        &self,
        ctx: RequestContext,
        query: AttachmentQuery,
    ) -> Result<common::api::PagedResult<Attachment>>;

    /// 删除 Attachment（软删除元数据，不物理删除文件）。
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 写入附件摘要元信息（覆盖旧值；关键词索引由 DAO 触发器同步）。
    async fn describe_attachment(
        &self,
        ctx: RequestContext,
        id: &str,
        update: AttachmentDescriptionUpdate,
    ) -> Result<Attachment>;

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
        // ⚠️ 上传方的 `mime_type` 不可全信：浏览器里手工构造的 Blob 若没设 type、
        // 微信 / 飞书下载响应、非浏览器调用方都会给 `application/octet-stream` 甚至空串。
        // 直接落库会让「图片」永远进不了内联白名单（`/file` 还会带 `nosniff`，连浏览器
        // 嗅探都被禁），所以**写库前**按原文件名后缀补齐真实类型；调用方给了具体类型
        // 则原样尊重。这是所有上传入口（站内 / 渠道入站 / 文本附件）的唯一写库口。
        let mime_type = common::mime::normalize_or_infer(&upload.mime_type, &upload.original_name);
        let extension = infer_extension(&upload.original_name, &mime_type);
        let stored_name = format!("{id}{extension}");
        let relative_path = generate_relative_path(&id, &extension);
        let file_type = infer_file_type(&mime_type, &extension);
        let size = upload.bytes.len() as i64;
        let purpose = upload.purpose.trim().to_string();

        let po = AttachmentPo::new(
            id.clone(),
            upload.original_name,
            stored_name,
            relative_path.clone(),
            mime_type,
            file_type,
            size,
            purpose,
            user_id.clone(),
            user_id,
        );

        self.attachment_dao
            .write_file(&relative_path, &upload.bytes)?;
        self.attachment_dao.insert(ctx.clone(), &po).await?;

        // 落库后回读「真正的 PO」：DAO 会用它自己注入的 `attachments_dir` 回填绝对路径
        // （见 `AttachmentPo::abs_path`），因此返回的实体是完成态——不会出现「刚创建的
        // 对象取不到 file_abs_path」这种半成品。代价是一次主键 SELECT；创建路径频率低，
        // 换来调用方不必自己补路径，值得。
        self.get_by_id(ctx, &id)
            .await?
            .ok_or_else(|| err!(Internal, "Attachment {} 落库后回读失败", id))
    }

    async fn create_from_text(
        &self,
        ctx: RequestContext,
        create: TextAttachmentCreate,
    ) -> Result<Attachment> {
        // 文本附件按内容来源创建，后缀往往缺失；推断不出具体类型时退回 `text/plain`
        // （而不是泛型 octet-stream），否则纯文本会掉进 Binary 分支。
        let mime_type = create.mime_type.unwrap_or_else(|| {
            match common::mime::infer_mime_type(&create.file_name) {
                common::mime::OCTET_STREAM => "text/plain".to_string(),
                inferred => inferred.to_string(),
            }
        });
        let upload = AttachmentUpload {
            original_name: create.file_name,
            mime_type,
            purpose: create.purpose.unwrap_or_default(),
            bytes: create.content.into_bytes(),
        };
        self.create_from_upload(ctx, upload).await
    }

    async fn get_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Attachment>> {
        let po = self.attachment_dao.find_by_id(ctx, id).await?;
        Ok(po.map(Attachment::from_po))
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: AttachmentQuery,
    ) -> Result<common::api::PagedResult<Attachment>> {
        let page = self.attachment_dao.query(ctx, query).await?;
        Ok(page.map(Attachment::from_po))
    }

    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.attachment_dao.delete(ctx, id).await
    }

    async fn describe_attachment(
        &self,
        ctx: RequestContext,
        id: &str,
        update: AttachmentDescriptionUpdate,
    ) -> Result<Attachment> {
        let attachment = self
            .attachment_dao
            .find_by_id(ctx.clone(), id)
            .await?
            .ok_or_else(|| err!(NotFound, "Attachment {} not found", id))?;

        // 归属校验：系统上下文（cron / 内部任务）放行，其余仅允许资产所有者写入。
        let is_system = ctx.caller_type() == common::enums::CallerType::System;
        if !is_system && attachment.root_user_id != ctx.uid() {
            bail_err!(NotFound, "Attachment {} not found", id);
        }

        self.attachment_dao
            .update_description(ctx.clone(), id, &update)
            .await?;
        self.get_by_id(ctx, id)
            .await?
            .ok_or_else(|| err!(NotFound, "Attachment {} not found", id))
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

fn sanitize_extension(extension: &str) -> String {
    extension
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(16)
        .collect::<String>()
        .to_ascii_lowercase()
}

fn infer_file_type(mime_type: &str, extension: &str) -> FileType {
    // 先按 MIME 归类：客户端 / 渠道给出的具体类型比文件名后缀更可信。
    // ⚠️ 文档类也必须在这里认——否则「`text/markdown` + `.bin` 文件名」会掉进
    // Binary 分支（详情页与前端渲染都按 `FileType` 分派，会当成二进制下载）。
    let normalized_mime = common::mime::normalize(mime_type);
    if normalized_mime.starts_with("image/") {
        return FileType::Image;
    }
    if normalized_mime.starts_with("audio/") {
        return FileType::Audio;
    }
    if normalized_mime.starts_with("video/") {
        return FileType::Video;
    }
    let is_document_mime = normalized_mime.starts_with("text/")
        || matches!(
            normalized_mime,
            "application/pdf"
                | "application/json"
                | "application/xml"
                | "application/yaml"
                | "application/x-yaml"
                | "application/toml"
                | "application/sql"
                | "application/rtf"
                | "application/epub+zip"
                | "application/msword"
                | "application/vnd.ms-excel"
                | "application/vnd.ms-powerpoint"
                | "application/javascript"
        )
        // docx / xlsx / pptx 共用 OOXML 后缀
        || normalized_mime.contains("officedocument");
    if is_document_mime {
        return FileType::Document;
    }

    // MIME 泛型 / 未知：退回扩展名兜底
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
