//! SQLite implementation of Attachment DAO

use super::{AttachmentDao, AttachmentQuery, AttachmentSearch};
use crate::models::attachment::{
    AttachmentDescriptionUpdate, AttachmentPo, attachment_file_abs_path,
};
use crate::pkg::RequestContext;
use crate::pkg::paths;
use crate::pkg::storage::{build_fts5_search_plan, merge_fts5_results, push_fts5_like_conditions};
use common::api::PagedResult;
use common::error::Result;
use sqlx::{FromRow, QueryBuilder, SqlitePool};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

// ==================== 工厂方法 + 单例 ====================

static DAO_INSTANCE: OnceLock<Arc<dyn AttachmentDao + Send + Sync>> = OnceLock::new();

/// 创建一个全新的 Attachment DAO 实例（用于测试）
pub fn new() -> Arc<dyn AttachmentDao + Send + Sync> {
    let base = crate::config::get().base_data_path();
    Arc::new(AttachmentDaoSqliteImpl::new(paths::attachments_dir(&base)))
}

/// 使用指定 attachments 根目录创建 DAO（用于测试隔离）
pub fn new_with_attachments_dir(attachments_dir: PathBuf) -> Arc<dyn AttachmentDao + Send + Sync> {
    Arc::new(AttachmentDaoSqliteImpl::new(attachments_dir))
}

/// Get the singleton Attachment DAO instance
pub fn dao() -> Arc<dyn AttachmentDao + Send + Sync> {
    DAO_INSTANCE
        .get()
        .expect("Attachment DAO not initialized")
        .clone()
}

/// Initialize the Attachment DAO
pub fn init() {
    let _ = DAO_INSTANCE.set(new());
}

#[derive(Debug)]
struct AttachmentDaoSqliteImpl {
    attachments_dir: PathBuf,
}

impl AttachmentDaoSqliteImpl {
    fn new(attachments_dir: PathBuf) -> Self {
        Self { attachments_dir }
    }

    /// 解析 relative_path → 绝对路径，并做输入合法性校验。
    ///
    /// 校验与拼串的唯一真相处在模型层（[`attachment_file_abs_path`]）；这里只是把
    /// DAO **实例注入的** `attachments_dir` 传进去——不能退化成读全局配置，否则会
    /// 算到真实目录（测试隔离依赖这个注入）。
    fn resolve_relative_path(&self, relative_path: &str) -> Result<PathBuf> {
        attachment_file_abs_path(&self.attachments_dir, relative_path)
    }

    /// 给读出的 PO 回填磁盘绝对路径，使上层拿到实体就能直接定位文件。
    ///
    /// 解析失败（DB 里 relative_path 非法，属数据损坏）只告警不回填——元信息读取
    /// 不该因一条脏数据整条失败；真正需要字节的调用方会在 `resolved_abs_path()`
    /// 处拿到明确错误。
    fn with_abs_path(&self, mut po: AttachmentPo) -> AttachmentPo {
        match self.resolve_relative_path(&po.relative_path) {
            Ok(abs_path) => po.abs_path = Some(abs_path),
            Err(error) => log_warn!(
                "附件 {} 的相对路径无法解析为绝对路径，已跳过路径回填: {}",
                po.id,
                error
            ),
        }
        po
    }
}

#[async_trait::async_trait]
impl AttachmentDao for AttachmentDaoSqliteImpl {
    async fn insert(&self, ctx: RequestContext, attachment: &AttachmentPo) -> Result<()> {
        let pool: &SqlitePool = ctx.db_pool();
        let file_type = attachment.file_type as i32;
        sqlx::query(
            r#"
INSERT INTO attachments (id, original_name, stored_name, relative_path, mime_type, file_type, size, purpose, status, root_user_id, created_by, modified_by, created_at, updated_at) VALUES (
?,
?,
?,
?,
?,
?,
?,
?,
?,
?,
?,
?,
?,
?
)
"#,
        )
        .bind(&attachment.id)
        .bind(&attachment.original_name)
        .bind(&attachment.stored_name)
        .bind(&attachment.relative_path)
        .bind(&attachment.mime_type)
        .bind(file_type)
        .bind(attachment.size)
        .bind(&attachment.purpose)
        .bind(attachment.status)
        .bind(&attachment.root_user_id)
        .bind(&attachment.created_by)
        .bind(&attachment.modified_by)
        .bind(attachment.created_at)
        .bind(attachment.updated_at)
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<AttachmentPo>> {
        let pool = ctx.db_pool();
        let attachment = sqlx::query_as::<_, AttachmentPo>(
            r#"
SELECT id, original_name, stored_name, relative_path, mime_type, file_type, size, purpose, description, description_usage, description_source, description_model, description_message_id, described_at, status, root_user_id, created_by, modified_by, created_at, updated_at
FROM attachments
WHERE id = ? AND "status" != 0
"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        Ok(attachment.map(|po| self.with_abs_path(po)))
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: AttachmentQuery,
    ) -> Result<PagedResult<AttachmentPo>> {
        let pool = ctx.db_pool();

        let mut count_builder =
            QueryBuilder::new(r#"SELECT COUNT(*) FROM attachments WHERE "status" != 0"#);
        push_query_filters(&mut count_builder, &query);
        let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;

        let mut list_builder = QueryBuilder::new(
            r#"SELECT id, original_name, stored_name, relative_path, mime_type, file_type, size, purpose, description, description_usage, description_source, description_model, description_message_id, described_at, status, root_user_id, created_by, modified_by, created_at, updated_at FROM attachments WHERE "status" != 0"#,
        );
        push_query_filters(&mut list_builder, &query);
        list_builder.push(" ORDER BY created_at DESC");

        if let Some(limit) = query.pagination.limit {
            list_builder.push(" LIMIT ").push_bind(limit as i64);
        } else if query.pagination.offset.is_some() {
            list_builder.push(" LIMIT -1");
        }
        if let Some(offset) = query.pagination.offset {
            list_builder.push(" OFFSET ").push_bind(offset as i64);
        }

        let items: Vec<AttachmentPo> = list_builder.build_query_as().fetch_all(pool).await?;
        let items = items.into_iter().map(|po| self.with_abs_path(po)).collect();
        Ok(PagedResult {
            items,
            total: total as usize,
        })
    }

    async fn search(
        &self,
        ctx: RequestContext,
        search: AttachmentSearch,
    ) -> Result<Vec<(AttachmentPo, Option<f32>)>> {
        let pool = ctx.db_pool();
        let Some(plan) = build_fts5_search_plan(search.keyword.as_deref().unwrap_or_default())
        else {
            return Ok(Vec::new());
        };
        let limit = search.pagination.limit.unwrap_or(20).clamp(1, 100) as i64;

        // 归属/用途/类型过滤：两路共用，统一带 `a.` 前缀（JOIN 与裸表都成立）。
        // ⚠️ root_user_id 必须下推到 SQL：BM25 top-K 若被他人附件占满，
        // 应用层再过滤也拿不到自己的命中（越权红线 + 正确性）。
        let apply_filters = |builder: &mut QueryBuilder<'_, sqlx::Sqlite>| {
            builder.push(r#" AND a."status" != 0"#);
            if let Some(root_user_id) = &search.root_user_id {
                builder
                    .push(" AND a.root_user_id = ")
                    .push_bind(root_user_id.clone());
            }
            if let Some(purpose) = &search.purpose {
                builder.push(" AND a.purpose = ").push_bind(purpose.clone());
            }
            if let Some(file_type) = search.file_type {
                builder
                    .push(" AND a.file_type = ")
                    .push_bind(file_type as i32);
            }
        };

        // 路径一：FTS5 MATCH（左表必须写完整表名 attachments_fts，不能用别名）
        let mut primary: Vec<(AttachmentPo, Option<f32>)> = Vec::new();
        if let Some(match_expr) = &plan.match_expr {
            let mut builder = QueryBuilder::new(
                r#"SELECT a.id, a.original_name, a.stored_name, a.relative_path, a.mime_type, a.file_type, a.size, a.purpose, a.description, a.description_usage, a.description_source, a.description_model, a.description_message_id, a.described_at, a."status", a.root_user_id, a.created_by, a.modified_by, a.created_at, a.updated_at, attachments_fts.rank AS fts_rank FROM attachments_fts JOIN attachments a ON attachments_fts.rowid = a.rowid WHERE attachments_fts MATCH "#,
            );
            builder.push_bind(match_expr);
            apply_filters(&mut builder);
            builder
                .push(" ORDER BY attachments_fts.rank LIMIT ")
                .push_bind(limit);
            let rows: Vec<AttachmentFtsRow> = builder.build_query_as().fetch_all(pool).await?;
            primary = rows
                .into_iter()
                .map(|row| {
                    let (po, rank) = row.into_po_and_rank();
                    (self.with_abs_path(po), rank)
                })
                .collect();
        }

        // 路径二：LIKE 兜底（trigram 对 <3 字符词元无法成 token）
        let mut secondary: Vec<(AttachmentPo, Option<f32>)> = Vec::new();
        if !plan.like_patterns.is_empty() {
            let mut builder = QueryBuilder::new(
                r#"SELECT a.id, a.original_name, a.stored_name, a.relative_path, a.mime_type, a.file_type, a.size, a.purpose, a.description, a.description_usage, a.description_source, a.description_model, a.description_message_id, a.described_at, a."status", a.root_user_id, a.created_by, a.modified_by, a.created_at, a.updated_at, NULL AS fts_rank FROM attachments a WHERE ("#,
            );
            push_fts5_like_conditions(
                &mut builder,
                "a",
                &[
                    "original_name",
                    "description",
                    "description_usage",
                    "purpose",
                ],
                &plan.like_patterns,
            );
            builder.push(")");
            apply_filters(&mut builder);
            builder
                .push(" ORDER BY a.updated_at DESC, a.id DESC LIMIT ")
                .push_bind(limit);
            let rows: Vec<AttachmentFtsRow> = builder.build_query_as().fetch_all(pool).await?;
            secondary = rows
                .into_iter()
                .map(|row| {
                    let (po, rank) = row.into_po_and_rank();
                    (self.with_abs_path(po), rank)
                })
                .collect();
        }

        Ok(merge_fts5_results(
            primary,
            secondary,
            |po: &AttachmentPo| po.id.clone(),
            limit as usize,
        ))
    }

    async fn update_status(&self, ctx: RequestContext, id: &str, status: i32) -> Result<()> {
        let pool = ctx.db_pool();
        let now = common::constants::utils::current_timestamp_ms();
        let modified_by = ctx.caller_id_or_system();
        sqlx::query(
            r#"
UPDATE attachments SET "status" = ?, modified_by = ?, updated_at = ? WHERE id = ?
"#,
        )
        .bind(status)
        .bind(modified_by)
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.update_status(ctx, id, 0).await
    }

    async fn update_file_metadata(&self, ctx: RequestContext, id: &str, size: i64) -> Result<()> {
        let pool = ctx.db_pool();
        let now = common::constants::utils::current_timestamp_ms();
        let modified_by = ctx.caller_id_or_system();
        sqlx::query(
            r#"
UPDATE attachments SET size = ?, modified_by = ?, updated_at = ? WHERE id = ? AND "status" != 0
"#,
        )
        .bind(size)
        .bind(modified_by)
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn update_description(
        &self,
        ctx: RequestContext,
        id: &str,
        update: &AttachmentDescriptionUpdate,
    ) -> Result<()> {
        let pool = ctx.db_pool();
        let now = common::constants::utils::current_timestamp_ms();
        let modified_by = ctx.caller_id_or_system();
        sqlx::query(
            r#"
UPDATE attachments SET description = ?, description_usage = ?, description_source = ?, description_model = ?, description_message_id = ?, described_at = ?, modified_by = ?, updated_at = ? WHERE id = ? AND "status" != 0
"#,
        )
        .bind(&update.description)
        .bind(&update.usage)
        .bind(&update.source)
        .bind(&update.model)
        .bind(&update.message_id)
        .bind(update.described_at)
        .bind(modified_by)
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    fn write_file(&self, relative_path: &str, bytes: &[u8]) -> Result<()> {
        let path = self.resolve_relative_path(relative_path)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, bytes)?;
        Ok(())
    }

    fn read_file(&self, relative_path: &str) -> Result<Vec<u8>> {
        let path = self.resolve_relative_path(relative_path)?;
        Ok(std::fs::read(path)?)
    }

    fn file_exists(&self, relative_path: &str) -> bool {
        self.resolve_relative_path(relative_path)
            .map(|path| path.exists())
            .unwrap_or(false)
    }
}

fn push_query_filters(builder: &mut QueryBuilder<'_, sqlx::Sqlite>, query: &AttachmentQuery) {
    if let Some(root_user_id) = &query.root_user_id {
        builder
            .push(" AND root_user_id = ")
            .push_bind(root_user_id.clone());
    }
    if let Some(purpose) = &query.purpose {
        builder.push(" AND purpose = ").push_bind(purpose.clone());
    }
    if let Some(file_type) = query.file_type {
        builder
            .push(" AND file_type = ")
            .push_bind(file_type as i32);
    }
    if let Some(ids) = &query.ids
        && !ids.is_empty()
    {
        builder.push(" AND id IN (");
        let mut separated = builder.separated(", ");
        for id in ids {
            separated.push_bind(id.clone());
        }
        separated.push_unseparated(")");
    }
}

/// FTS5 JOIN 查询的承接行：AttachmentPo 全列 + bm25 rank（LIKE 路为 NULL）。
#[derive(Debug, FromRow)]
struct AttachmentFtsRow {
    id: String,
    original_name: String,
    stored_name: String,
    relative_path: String,
    mime_type: String,
    file_type: common::enums::FileType,
    size: i64,
    purpose: String,
    description: String,
    description_usage: String,
    description_source: String,
    description_model: String,
    description_message_id: String,
    described_at: i64,
    #[sqlx(rename = "status")]
    status_value: i32,
    root_user_id: String,
    created_by: String,
    modified_by: String,
    created_at: i64,
    updated_at: i64,
    fts_rank: Option<f64>,
}

impl AttachmentFtsRow {
    fn into_po_and_rank(self) -> (AttachmentPo, Option<f32>) {
        let rank = self.fts_rank.map(|r| r as f32);
        let po = AttachmentPo {
            id: self.id,
            original_name: self.original_name,
            stored_name: self.stored_name,
            relative_path: self.relative_path,
            mime_type: self.mime_type,
            file_type: self.file_type,
            size: self.size,
            purpose: self.purpose,
            description: self.description,
            description_usage: self.description_usage,
            description_source: self.description_source,
            description_model: self.description_model,
            description_message_id: self.description_message_id,
            described_at: self.described_at,
            status: self.status_value,
            root_user_id: self.root_user_id,
            created_by: self.created_by,
            modified_by: self.modified_by,
            created_at: self.created_at,
            updated_at: self.updated_at,
            abs_path: None,
        };
        (po, rank)
    }
}
