//! Attachment upload/query API DTOs - shared between backend and frontend

use crate::enums::FileType;
use serde::{Deserialize, Serialize};

/// Attachment 列表查询参数。
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AttachmentListQuery {
    /// 可选用途筛选，如 skill/message/artifact/tool_result。
    pub purpose: Option<String>,
    /// 可选文件类型筛选。
    pub file_type: Option<FileType>,
    /// 返回数量限制。
    pub limit: Option<usize>,
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
