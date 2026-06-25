//! Attachment 模型
//!
//! Attachment 是通用上传文件资产，归属 Finance Domain 管理。

use common::enums::FileType;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use common::bail_err;

/// AttachmentPo 持久化对象。
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct AttachmentPo {
    /// 上传文件资产 ID。
    pub id: String,
    /// 用户上传时的原始文件名，仅作 metadata 展示。
    pub original_name: String,
    /// 系统生成的存储文件名。
    pub stored_name: String,
    /// 相对 attachments 根目录的路径。
    pub relative_path: String,
    /// MIME 类型。
    pub mime_type: String,
    /// 文件类型。
    pub file_type: FileType,
    /// 文件大小（bytes）。
    pub size: i64,
    /// 用途标记。
    pub purpose: String,
    /// 状态：0=已删除，1=正常。
    pub status: i32,
    /// 文件资产所属用户。
    pub root_user_id: String,
    /// 上传人。
    pub created_by: String,
    /// 最后修改人。
    pub modified_by: String,
    /// 创建时间戳（毫秒）。
    pub created_at: i64,
    /// 更新时间戳（毫秒）。
    pub updated_at: i64,
}

/// Attachment 业务实体。
#[derive(Debug, Clone)]
pub struct Attachment {
    /// 底层持久化对象。
    pub po: AttachmentPo,
    /// 按需装配的文件读取结果。
    pub read_results: Vec<AttachmentReadResult>,
}

/// Attachment 获取选项。
#[derive(Debug, Clone, Default)]
pub struct AttachmentGetOptions {
    /// 是否读取并返回文件内容。
    pub include_file_content: bool,
}

/// Attachment 文件读取结果。
#[derive(Debug, Clone)]
pub struct AttachmentReadResult {
    /// 相对 attachments 根目录的路径。
    pub relative_path: String,
    /// 文件 bytes。
    pub bytes: Vec<u8>,
    /// 文件大小（bytes）。
    pub size: usize,
}

/// 上传文件数据。
#[derive(Debug, Clone)]
pub struct AttachmentUpload {
    /// 原始文件名。
    pub original_name: String,
    /// MIME 类型。
    pub mime_type: String,
    /// 用途标记。
    pub purpose: String,
    /// 文件 bytes。
    pub bytes: Vec<u8>,
}

/// JSON 创建小型文本 Attachment 命令。
#[derive(Debug, Clone)]
pub struct TextAttachmentCreate {
    /// 安全文件名。
    pub file_name: String,
    /// UTF-8 文本内容。
    pub content: String,
    /// 可选 MIME 类型。
    pub mime_type: Option<String>,
    /// 可选用途。
    pub purpose: Option<String>,
}

/// 文本内容全量替换命令。
#[derive(Debug, Clone)]
pub struct TextContentUpdate {
    /// 新的 UTF-8 文本内容。
    pub content: String,
    /// 可选乐观锁时间戳。
    pub expected_updated_at: Option<i64>,
}

/// Attachment 文本内容业务返回对象。
#[derive(Debug, Clone)]
pub struct AttachmentTextContent {
    /// Attachment metadata。
    pub attachment: Attachment,
    /// UTF-8 文本内容。
    pub content: String,
    /// 编码名，当前固定为 utf-8。
    pub encoding: String,
    /// 内容大小（bytes）。
    pub size: u64,
    /// 更新时间戳。
    pub updated_at: i64,
}

impl Attachment {
    /// 从 PO 创建实体。
    pub fn from_po(po: AttachmentPo) -> Self {
        Self {
            po,
            read_results: Vec::new(),
        }
    }

    /// 添加文件读取结果。
    pub fn with_read_result(mut self, read_result: AttachmentReadResult) -> Self {
        self.read_results.push(read_result);
        self
    }

    /// 获取 ID。
    pub fn id(&self) -> &str {
        &self.po.id
    }

    /// 获取资产所属用户。
    pub fn root_user_id(&self) -> &str {
        &self.po.root_user_id
    }
}

impl AttachmentPo {
    /// 创建新的 AttachmentPo。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        original_name: String,
        stored_name: String,
        relative_path: String,
        mime_type: String,
        file_type: FileType,
        size: i64,
        purpose: String,
        root_user_id: String,
        created_by: String,
    ) -> Self {
        let now = common::constants::utils::current_timestamp_ms();
        Self {
            id,
            original_name,
            stored_name,
            relative_path,
            mime_type,
            file_type,
            size,
            purpose,
            status: 1,
            root_user_id,
            created_by: created_by.clone(),
            modified_by: created_by,
            created_at: now,
            updated_at: now,
        }
    }

    /// 标记为删除。
    pub fn mark_deleted(&mut self, modified_by: String) {
        self.status = 0;
        self.modified_by = modified_by;
        self.updated_at = common::constants::utils::current_timestamp_ms();
    }
}
