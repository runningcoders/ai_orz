//! Attachment DAO layer
//! DAO 只负责 AttachmentPo 持久化和给定相对路径的基础文件读写。

use crate::models::attachment::AttachmentPo;
use crate::pkg::RequestContext;
use async_trait::async_trait;
use common::api::PaginationParams;
use common::enums::FileType;
use common::error::Result;

/// Attachment 查询参数。
#[derive(Debug, Clone, Default)]
pub struct AttachmentQuery {
    /// 文件资产所属用户。
    pub root_user_id: Option<String>,
    /// 用途筛选。
    pub purpose: Option<String>,
    /// 文件类型筛选。
    pub file_type: Option<FileType>,
    /// 分页参数。
    pub pagination: PaginationParams,
}

/// Attachment DAO trait。
#[async_trait]
pub trait AttachmentDao: Send + Sync + std::fmt::Debug {
    /// 插入 Attachment 元数据。
    async fn insert(&self, ctx: RequestContext, attachment: &AttachmentPo) -> Result<()>;

    /// 根据 ID 获取 Attachment，自动过滤已删除记录。
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<AttachmentPo>>;

    /// 通用查询，自动过滤已删除记录。
    async fn query(
        &self,
        ctx: RequestContext,
        query: AttachmentQuery,
    ) -> Result<common::api::PagedResult<AttachmentPo>>;

    /// 更新状态。
    async fn update_status(&self, ctx: RequestContext, id: &str, status: i32) -> Result<()>;

    /// 更新文件元数据。
    async fn update_file_metadata(&self, ctx: RequestContext, id: &str, size: i64) -> Result<()>;

    /// 软删除。
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 写入文件 bytes 到给定相对路径。
    fn write_file(&self, relative_path: &str, bytes: &[u8]) -> Result<()>;

    /// 读取给定相对路径文件 bytes。
    fn read_file(&self, relative_path: &str) -> Result<Vec<u8>>;

    /// 判断给定相对路径文件是否存在。
    fn file_exists(&self, relative_path: &str) -> bool;
}

pub mod sqlite;
pub use self::sqlite::{dao, init, new, new_with_attachments_dir};

#[cfg(test)]
pub(crate) mod sqlite_test;
