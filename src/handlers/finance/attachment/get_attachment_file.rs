//! Handler: GET /api/v1/finance/attachments/{id}/file - 读取 Attachment 原始二进制流
//!
//! 与 `/attachments/{id}/content` 分工不同：
//! - `/content` 返回 `AttachmentContentResponse`（metadata + UTF-8 文本），供详情页
//!   文本编辑器加载/保存（带 `updated_at` 乐观锁）。**不是**字节流，`<img>` 用不了。
//! - `/file` 返回**原始字节流** + 真实 `Content-Type`，供 `<img> / <video> / <audio> /
//!   <a>` 直接渲染或下载，覆盖文档、图片、音视频等所有文件类型；支持 `Range`
//!   （视频/音频可拖动进度条）。
//!
//! 响应构造（流式读取、Range、`inline`/`attachment` 安全判定、`nosniff`）全部交给
//! [`crate::pkg::http::file::serve_file`]，本 handler 只负责「鉴权 + 定位文件」。
//!
//! 不注册为 `neural` 工具：返回原始字节不适合作为 LLM 工具结果；Agent 读文本请用
//! `get_attachment_content`，读元信息/摘要请用 `get_attachment`。

use axum::{
    extract::{Extension, Path},
    http::{HeaderMap, header},
    response::Response,
};
use common::error::{Result, bail_err, err};

use crate::models::attachment::AttachmentGetOptions;
use crate::pkg::RequestContext;
use crate::pkg::http::file::{FileServeOptions, serve_file};
use crate::service::domain::finance::domain;

/// 读取 Attachment 原始字节流（任意文件类型，支持 Range）。
pub async fn get_attachment_file(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response> {
    if ctx.uid().is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    // 复用既有领域读侧：内部已按 `root_user_id` 校验归属，非本人一律 None（不泄露存在性）。
    // 这里**不装配文件字节**——流式读取只需要磁盘路径，读全量进内存是多余的。
    let Some(attachment) = domain()
        .attachment_manage()
        .get_attachment(
            ctx,
            &id,
            AttachmentGetOptions {
                include_file_content: false,
            },
        )
        .await?
    else {
        return Err(err!(NotFound, "Attachment {} not found", id));
    };

    // 路径随实体一起返回：DAO 读取时用**它自己注入的** `attachments_dir` 解析好，
    // 所以这里不需要（也不应该）再去读全局配置拼路径。
    let abs_path = attachment.file_abs_path()?;

    let range = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok());

    // 历史数据兜底：写库归一（`AttachmentDal::create_from_upload`）上线前落库的附件，
    // `mime_type` 可能是泛型 `application/octet-stream`（前端 Blob 未设 type / 渠道
    // 下载响应只给泛型）。响应头带 `nosniff`，泛型类型会让 `<img>` 直接拒绝渲染，
    // 所以在出口再按原文件名补齐一次——旧附件不必重传即可恢复内联展示。
    let mime_type = attachment.po.effective_mime_type();

    serve_file(
        &abs_path,
        range,
        FileServeOptions {
            mime_type: &mime_type,
            file_name: &attachment.po.original_name,
            // 私有资产：允许浏览器短期缓存以减少重复拉取，但禁止共享缓存留存。
            cache_control: Some("private, max-age=3600"),
            force_download: false,
        },
    )
    .await
}
