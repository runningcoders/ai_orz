//! SQLite implementation of Attachment DAO

use super::{AttachmentDao, AttachmentQuery};
use crate::models::attachment::AttachmentPo;
use crate::pkg::RequestContext;
use common::api::PagedResult;
use common::error::{Result, bail_err};
use sqlx::{QueryBuilder, SqlitePool};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, OnceLock};

// ==================== 工厂方法 + 单例 ====================

static DAO_INSTANCE: OnceLock<Arc<dyn AttachmentDao + Send + Sync>> = OnceLock::new();

/// 创建一个全新的 Attachment DAO 实例（用于测试）
pub fn new() -> Arc<dyn AttachmentDao + Send + Sync> {
    Arc::new(AttachmentDaoSqliteImpl::new(
        crate::config::get().attachments_dir(),
    ))
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

    fn resolve_relative_path(&self, relative_path: &str) -> Result<PathBuf> {
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
                _ => {
                    bail_err!(InvalidRequest, "附件路径包含非法路径片段");
                }
            }
        }
        Ok(self.attachments_dir.join(path))
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
SELECT id, original_name, stored_name, relative_path, mime_type, file_type, size, purpose, status, root_user_id, created_by, modified_by, created_at, updated_at
FROM attachments
WHERE id = ? AND "status" != 0
"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        Ok(attachment)
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
            r#"SELECT id, original_name, stored_name, relative_path, mime_type, file_type, size, purpose, status, root_user_id, created_by, modified_by, created_at, updated_at FROM attachments WHERE "status" != 0"#,
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

        let items = list_builder.build_query_as().fetch_all(pool).await?;
        Ok(PagedResult {
            items,
            total: total as usize,
        })
    }

    async fn update_status(&self, ctx: RequestContext, id: &str, status: i32) -> Result<()> {
        let pool = ctx.db_pool();
        let now = common::constants::utils::current_timestamp_ms();
        let modified_by = ctx.uid();
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
        let modified_by = ctx.uid();
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
}
