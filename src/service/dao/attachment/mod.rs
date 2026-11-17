//! Attachment DAO layer
//! DAO 只负责 AttachmentPo 持久化和给定相对路径的基础文件读写。

use crate::models::attachment::{AttachmentDescriptionUpdate, AttachmentPo};
use crate::models::vector::{VectorIndexParams, VectorRow, VectorSearchHit};
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
    /// 按 ID 精确回填（混合搜索时批量取向量独有命中，避免 N+1）。
    pub ids: Option<Vec<String>>,
    /// 分页参数。
    pub pagination: PaginationParams,
}

/// Attachment 全文搜索参数（FTS5 关键词召回）。
///
/// `root_user_id` 是越权红线：由调用方强制注入当前用户，DAO 下推到 SQL，
/// 不接受外部传入，否则 BM25 top-K 会被他人附件占满。
#[derive(Debug, Clone, Default)]
pub struct AttachmentSearch {
    /// 搜索关键词。
    pub keyword: Option<String>,
    /// 归属用户（强制过滤）。
    pub root_user_id: Option<String>,
    /// 用途筛选。
    pub purpose: Option<String>,
    /// 文件类型筛选。
    pub file_type: Option<FileType>,
    /// 分页参数（同时作为 FTS 召回宽度）。
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

    /// FTS5 全文检索（MATCH + LIKE 双路），返回 (PO, fts_rank)。
    /// `root_user_id` 必须在 SQL 内下推，防止 top-K 被他人附件占满。
    async fn search(
        &self,
        ctx: RequestContext,
        search: AttachmentSearch,
    ) -> Result<Vec<(AttachmentPo, Option<f32>)>>;

    /// 更新状态。
    async fn update_status(&self, ctx: RequestContext, id: &str, status: i32) -> Result<()>;

    /// 更新文件元数据。
    async fn update_file_metadata(&self, ctx: RequestContext, id: &str, size: i64) -> Result<()>;

    /// 写入附件摘要元信息（关键词索引由 attachments_fts 触发器同步）。
    async fn update_description(
        &self,
        ctx: RequestContext,
        id: &str,
        update: &AttachmentDescriptionUpdate,
    ) -> Result<()>;

    /// 软删除。
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 写入文件 bytes 到给定相对路径。
    fn write_file(&self, relative_path: &str, bytes: &[u8]) -> Result<()>;

    /// 读取给定相对路径文件 bytes。
    fn read_file(&self, relative_path: &str) -> Result<Vec<u8>>;

    /// 判断给定相对路径文件是否存在。
    fn file_exists(&self, relative_path: &str) -> bool;
}

/// Attachment 向量 DAO trait：与基础数据解耦，唯一负责 "attachments" collection 的 CRUD。
#[async_trait]
pub trait AttachmentVectorDao: Send + Sync {
    /// 插入或更新向量索引。
    async fn upsert_vector(
        &self,
        ctx: RequestContext,
        attachment_id: &str,
        vector_params: &VectorIndexParams,
    ) -> Result<()>;

    /// 纯向量语义搜索，返回向量行 + 距离（业务过滤在 DAO 内转译为谓词下推）。
    async fn search_vector(
        &self,
        ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
        filters: &AttachmentQuery,
    ) -> Result<Vec<VectorSearchHit>>;

    /// 获取指定附件的完整向量行（三态判断取旧行 hash）。
    async fn get_vector_row(
        &self,
        ctx: RequestContext,
        attachment_id: &str,
    ) -> Result<Option<VectorRow>>;

    /// 删除单条向量。
    async fn delete_vector(&self, ctx: RequestContext, attachment_id: &str) -> Result<()>;

    /// 清空整个 collection（重建用）。
    async fn clear_collection(&self, ctx: RequestContext) -> Result<()>;
}

pub mod sqlite;
pub use self::sqlite::{dao, init, new, new_with_attachments_dir};

pub mod vector;
pub use self::vector::{dao as vector_dao, init as init_vector, new as new_vector_dao};

#[cfg(test)]
pub(crate) mod sqlite_test;
