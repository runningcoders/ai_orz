//! Attachment 模型
//!
//! Attachment 是通用上传文件资产，归属 Finance Domain 管理。

use std::path::{Component, Path, PathBuf};

use common::enums::FileType;
use common::error::{Result, bail_err};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// 在附件根目录 [`attachments_dir`] 下把相对路径解析为绝对路径。
///
/// ## 这里是附件路径推导的唯一真相
///
/// DAO 的读写/存在性判断，以及所有第三方持有的实体，都必须从这里取路径。
/// 分头拼串一旦不一致，症状同样是「写到 A、读到 B」的静默串档。
///
/// ## 为什么这里是 `attachments_dir` 而不是全局 base
///
/// 附件根目录是**可注入的**（`dao::attachment::new_with_attachments_dir`），
/// 测试用它隔离临时目录。因此本函数必须收 ROOT 作为参数，由调用方给出「实际
/// 生效的存储根」——**禁止**在这里读全局配置，否则注入场景会算到真实目录。
///
/// ## 安全
///
/// `relative_path` 来自 DB / 外部输入：绝对路径、含 `..` 或任何非 Normal 组件
/// 一律拒绝（这里不依赖 `Path::starts_with`——它不规范化 `..`，拦不住穿越）。
pub fn attachment_file_abs_path(attachments_dir: &Path, relative_path: &str) -> Result<PathBuf> {
    if relative_path.trim().is_empty() {
        bail_err!(InvalidRequest, "附件相对路径不能为空");
    }
    let path = Path::new(relative_path);
    if path.is_absolute() {
        bail_err!(InvalidRequest, "附件路径不能是绝对路径");
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            _ => bail_err!(InvalidRequest, "附件路径包含非法路径片段"),
        }
    }
    Ok(attachments_dir.join(path))
}

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
    /// 附件内容摘要（Agent 识别后写入，描述「看到了什么」）。
    pub description: String,
    /// 附件用途摘要（结合用户文字推断的用途）。
    pub description_usage: String,
    /// 摘要来源（如 agent_tool）。
    pub description_source: String,
    /// 产出摘要的模型标识。
    pub description_model: String,
    /// 产出摘要时的消息 ID。
    pub description_message_id: String,
    /// 摘要写入时间戳（毫秒）；0 表示未写入。
    pub described_at: i64,
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

/// 附件摘要写入命令。
#[derive(Debug, Clone)]
pub struct AttachmentDescriptionUpdate {
    /// 内容摘要（看到了什么）。
    pub description: String,
    /// 用途摘要（结合用户文字推断的用途）。
    pub usage: String,
    /// 摘要来源（如 agent_tool）。
    pub source: String,
    /// 产出摘要的模型标识。
    pub model: String,
    /// 产出摘要时的消息 ID。
    pub message_id: String,
    /// 摘要写入时间戳（毫秒）。
    pub described_at: i64,
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
    /// 更新时间戳（毫秒）。
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

    /// 获取附件实体文件在磁盘上的绝对路径。
    ///
    /// 等价于 [`AttachmentPo::file_abs_path`]，供持有 `Attachment` 的调用方直接使用。
    pub fn file_abs_path(&self, attachments_dir: &Path) -> Result<PathBuf> {
        self.po.file_abs_path(attachments_dir)
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
            description: String::new(),
            description_usage: String::new(),
            description_source: String::new(),
            description_model: String::new(),
            description_message_id: String::new(),
            described_at: 0,
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

    /// 获取附件实体文件在磁盘上的绝对路径。
    ///
    /// 详见 [`attachment_file_abs_path`]：`attachments_dir` 必须由调用方给出实际
    /// 生效的存储根（可注入），不要在本层取全局配置。
    pub fn file_abs_path(&self, attachments_dir: &Path) -> Result<PathBuf> {
        attachment_file_abs_path(attachments_dir, &self.relative_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn root() -> PathBuf {
        PathBuf::from("/srv/ai_orz/attachments")
    }

    #[test]
    fn resolves_under_given_attachments_root() {
        let abs = attachment_file_abs_path(&root(), "user-1/report.md").unwrap();
        assert_eq!(abs, root().join("user-1/report.md"));
    }

    /// 解析只相对**传入的**根目录，不得偷偷读全局配置——DAO 的测试隔离依赖这条。
    #[test]
    fn honors_injected_root() {
        let injected = PathBuf::from("/tmp/unit-xyz");
        let abs = attachment_file_abs_path(&injected, "a.txt").unwrap();
        assert!(abs.starts_with(&injected));
        assert_ne!(
            abs,
            attachment_file_abs_path(&root(), "a.txt").unwrap(),
            "不同 root 必须给出不同结果"
        );
    }

    /// relative_path 来自 DB / 外部输入，逃逸写法必须被拒。
    #[test]
    fn rejects_traversal_and_absolute_paths() {
        for evil in ["../secret.txt", "/etc/passwd", "a/../../b.txt", ""] {
            assert!(
                attachment_file_abs_path(&root(), evil).is_err(),
                "应拒绝非法附件路径: {evil:?}"
            );
        }
    }

    #[test]
    fn po_delegates_to_shared_resolver() {
        let po = AttachmentPo::new(
            "att-1".to_string(),
            "a.txt".to_string(),
            "stored".to_string(),
            "user-1/a.txt".to_string(),
            "text/plain".to_string(),
            FileType::Document,
            8,
            String::new(),
            "user-1".to_string(),
            "user-1".to_string(),
        );
        assert_eq!(
            po.file_abs_path(&root()).unwrap(),
            attachment_file_abs_path(&root(), "user-1/a.txt").unwrap()
        );
    }
}
