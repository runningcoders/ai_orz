//! Attachment 模型
//!
//! Attachment 是通用上传文件资产，归属 Finance Domain 管理。

use common::enums::FileType;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

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

impl Attachment {
    /// 从 PO 创建实体。
    pub fn from_po(po: AttachmentPo) -> Self {
        Self { po }
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
