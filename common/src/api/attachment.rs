//! Attachment upload/query API DTOs - shared between backend and frontend

use crate::api::{PaginationParams, TextContentResponse};
use crate::enums::FileType;
use serde::{Deserialize, Serialize};

/// Attachment 列表查询参数。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AttachmentListQuery {
    /// 可选用途筛选，如 skill/message/artifact/tool_result。
    pub purpose: Option<String>,
    /// 可选文件类型筛选。
    pub file_type: Option<FileType>,
    /// 分页参数。
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

/// Attachment 详情响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentDetail {
    /// 上传文件资产 ID。
    pub id: String,
    /// 用户上传时的原始文件名，仅用于展示。
    pub original_name: String,
    /// 系统生成的存储文件名。
    pub stored_name: String,
    /// 相对 attachments 根目录的内部路径。
    pub relative_path: String,
    /// MIME 类型。
    pub mime_type: String,
    /// 文件类型。
    pub file_type: FileType,
    /// 文件大小（bytes）。
    pub size: u64,
    /// 用途标记。
    pub purpose: String,
    /// 文件资产所属用户 ID。
    pub root_user_id: String,
    /// 上传人 ID。
    pub created_by: String,
    /// 创建时间戳。
    pub created_at: i64,
    /// 更新时间戳。
    pub updated_at: i64,
}

/// 上传 Attachment 响应。
pub type UploadAttachmentResponse = AttachmentDetail;

/// 获取 Attachment 响应。
pub type GetAttachmentResponse = AttachmentDetail;

/// JSON 创建小型 UTF-8 文本 Attachment 请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateTextAttachmentRequest {
    /// 安全文件名，不能包含路径分隔符或路径穿越片段。
    pub file_name: String,
    /// UTF-8 文本内容。
    pub content: String,
    /// 可选 MIME 类型，不传时按扩展名推断。
    pub mime_type: Option<String>,
    /// 可选用途，如 skill/message/artifact/tool_result。
    pub purpose: Option<String>,
}

/// JSON 创建小型 UTF-8 文本 Attachment 响应。
pub type CreateTextAttachmentResponse = AttachmentDetail;

/// Attachment 文本内容响应，组合 Attachment metadata 与文本内容。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentContentResponse {
    /// Attachment metadata。
    pub attachment: AttachmentDetail,
    /// UTF-8 文本内容。
    pub text: TextContentResponse,
}
