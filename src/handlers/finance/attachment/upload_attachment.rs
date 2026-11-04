//! 上传 Attachment

use axum::{
    Json,
    extract::{Extension, Multipart},
    http::StatusCode,
};
use common::api::{ApiResponse, UploadAttachmentResponse};

use crate::models::attachment::AttachmentUpload;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use common::error::{bail_err, err};

use super::response::to_detail;

/// 上传 Attachment
/// POST /attachments/upload
pub async fn upload_attachment(
    Extension(ctx): Extension<RequestContext>,
    mut multipart: Multipart,
) -> std::result::Result<
    (StatusCode, Json<ApiResponse<UploadAttachmentResponse>>),
    common::error::Error,
> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let mut purpose = String::new();
    let mut file_upload: Option<AttachmentUpload> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| err!(InvalidRequest, "解析上传表单失败: {}", err))?
    {
        let field_name = field.name().unwrap_or_default().to_string();
        if field_name == "purpose" {
            purpose = field
                .text()
                .await
                .map_err(|err| err!(InvalidRequest, "解析 purpose 失败: {}", err))?
                .trim()
                .to_string();
            continue;
        }

        if field_name == "file" {
            let original_name = field.file_name().unwrap_or("upload").to_string();
            // 只做「原样透传」：客户端给的 `Content-Type` 可能就是泛型
            // `application/octet-stream` 甚至缺失，按文件名后缀补齐真实类型的兜底
            // 统一在写库口 `AttachmentDal::create_from_upload` 做（站内 / 渠道 / 文本
            // 附件都要经过它，兜底放一处才不会漏）。
            let mime_type = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string();
            let bytes = field
                .bytes()
                .await
                .map_err(|err| err!(InvalidRequest, "读取上传文件失败: {}", err))?
                .to_vec();
            file_upload = Some(AttachmentUpload {
                original_name,
                mime_type,
                purpose: String::new(),
                bytes,
            });
        }
    }

    let mut upload = file_upload.ok_or_else(|| err!(InvalidRequest, "缺少 file 字段"))?;
    upload.purpose = purpose;

    let attachment = domain()
        .attachment_manage()
        .create_attachment(ctx, upload)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(to_detail(&attachment))),
    ))
}
