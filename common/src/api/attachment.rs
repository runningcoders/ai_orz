//! Attachment upload/query API DTOs - shared between backend and frontend

use crate::api::{PaginationParams, TextContentResponse};
use crate::enums::FileType;
use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Attachment 列表查询参数。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct AttachmentListQuery {
    /// 可选用途筛选，如 skill/message/artifact/tool_result。
    #[param(source = "query")]
    pub purpose: Option<String>,
    /// 可选文件类型筛选。
    #[param(source = "query")]
    pub file_type: Option<FileType>,
    /// 分页参数。
    #[serde(flatten)]
    #[param(source = "query")]
    pub pagination: PaginationParams,
}

/// Attachment 详情响应。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

/// 获取 Attachment 请求（path 参数：id）。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetAttachmentRequest {
    /// Attachment ID。
    #[param(source = "path")]
    pub id: String,
}

/// 获取 Attachment 文本内容请求（path 参数：id）。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetAttachmentContentRequest {
    /// Attachment ID。
    #[param(source = "path")]
    pub id: String,
}

/// 删除 Attachment 请求（path 参数：id）。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteAttachmentRequest {
    /// Attachment ID。
    #[param(source = "path")]
    pub id: String,
}

/// 全量替换 Attachment UTF-8 文本内容请求（path 参数：id + body）。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateAttachmentContentRequest {
    /// Attachment ID。
    #[param(source = "path")]
    pub id: String,
    /// 新的完整文本内容。
    pub content: String,
    /// 可选乐观锁时间戳。
    pub expected_updated_at: Option<i64>,
}

/// JSON 创建小型 UTF-8 文本 Attachment 请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Params)]
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
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AttachmentContentResponse {
    /// Attachment metadata。
    pub attachment: AttachmentDetail,
    /// UTF-8 文本内容。
    pub text: TextContentResponse,
}
