//! SQLite implementation of Artifact DAO

use super::{ArtifactDao, ArtifactQuery};
use common::error::{bail_err, Result};
use crate::models::{artifact::ArtifactPo, file::FileMeta};
use crate::pkg::RequestContext;
use common::api::PagedResult;
use common::enums::{ArtifactSourceType, FileType};
use sqlx::types::Json;
use sqlx::QueryBuilder;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;

// ==================== 工厂方法 + 单例 ====================

static DAO_INSTANCE: OnceLock<Arc<dyn ArtifactDao + Send + Sync>> = OnceLock::new();

/// Create a new Artifact DAO instance
pub fn new() -> Arc<dyn ArtifactDao + Send + Sync> {
    Arc::new(ArtifactDaoSqliteImpl)
}

/// Get the singleton Artifact DAO instance
pub fn dao() -> Arc<dyn ArtifactDao + Send + Sync> {
    DAO_INSTANCE
        .get()
        .expect("Artifact DAO not initialized")
        .clone()
}

/// Initialize the Artifact DAO
pub fn init() {
    let _ = DAO_INSTANCE.set(new());
}

#[derive(Debug)]
struct ArtifactDaoSqliteImpl;

impl ArtifactDaoSqliteImpl {
    fn resolve_generated_content_path(&self, artifact: &ArtifactPo) -> Result<PathBuf> {
        // Use Config's built-in method to get the correct path:
        // {base_data_dir}/artifacts/projects/{project_id}/{artifact_id}/{file_path}
        let config = crate::config::get();
        let dir_path = config
            .artifact_project_dir(&artifact.project_id)
            .join(&artifact.id);
        let path = dir_path.join(&artifact.file_meta.0.file_path);

        // Check for path traversal (double-check)
        if !path.starts_with(config.artifacts_dir()) {
            bail_err!(InvalidRequest, "Invalid artifact file path: path traversal attempt detected");
        }

        Ok(path)
    }
}

#[async_trait::async_trait]
impl ArtifactDao for ArtifactDaoSqliteImpl {
    async fn insert(&self, ctx: RequestContext, artifact: &ArtifactPo) -> Result<()> {
        let pool = ctx.db_pool();
        let ft = artifact.file_type as i32;
        let source_type = artifact.source_type as i32;
        sqlx::query!(
r#"
INSERT INTO artifacts (id, project_id, task_id, name, description, file_type, file_meta, source_type, tags, status, created_by, modified_by, created_at, updated_at) VALUES (
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
            artifact.id,
            artifact.project_id,
            artifact.task_id,
            artifact.name,
            artifact.description,
            ft,
            artifact.file_meta,
            source_type,
            artifact.tags,
            artifact.status,
            artifact.created_by,
            artifact.modified_by,
            artifact.created_at,
            artifact.updated_at
        )
            .execute(pool)
            .await?;
        Ok(())
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<ArtifactPo>> {
        let pool = ctx.db_pool();
        let artifact = sqlx::query_as!(
            ArtifactPo,
r#"
SELECT id, project_id, task_id, name, description, file_type as "file_type: FileType", file_meta as "file_meta: Json<FileMeta>", source_type as "source_type: ArtifactSourceType", tags, status as "status: i32", created_by, modified_by, created_at, updated_at
FROM artifacts
WHERE id = ? AND "status" != 0
"#,
            id
        )
        .fetch_optional(pool)
        .await?;
        Ok(artifact)
    }

    async fn query(&self, ctx: RequestContext, query: ArtifactQuery) -> Result<PagedResult<ArtifactPo>> {
        let pool = ctx.db_pool();

        let mut count_builder = QueryBuilder::new(
            r#"SELECT COUNT(*) FROM artifacts WHERE "status" != 0"#,
        );
        push_query_filters(&mut count_builder, &query);
        let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;

        let mut list_builder = QueryBuilder::new(
            r#"SELECT id, project_id, task_id, name, description, file_type, file_meta, source_type, tags, status, created_by, modified_by, created_at, updated_at FROM artifacts WHERE "status" != 0"#,
        );
        push_query_filters(&mut list_builder, &query);

        // 排序
        list_builder.push(" ORDER BY created_at DESC");

        // 分页
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

    async fn list_by_project(
        &self,
        ctx: RequestContext,
        project_id: &str,
    ) -> Result<Vec<ArtifactPo>> {
        // 语法糖：调用通用查询
        let page = self
            .query(
                ctx,
                ArtifactQuery {
                    project_id: Some(project_id.to_string()),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    async fn list_by_task(&self, ctx: RequestContext, task_id: &str) -> Result<Vec<ArtifactPo>> {
        // 语法糖：调用通用查询
        let page = self
            .query(
                ctx,
                ArtifactQuery {
                    task_id: Some(task_id.to_string()),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    async fn count_by_project(&self, ctx: RequestContext, project_id: &str) -> Result<u64> {
        // 语法糖：调用通用 count
        self.count(
            ctx,
            ArtifactQuery {
                project_id: Some(project_id.to_string()),
                ..Default::default()
            },
        )
        .await
    }

    async fn count_by_task(&self, ctx: RequestContext, task_id: &str) -> Result<u64> {
        // 语法糖：调用通用 count
        self.count(
            ctx,
            ArtifactQuery {
                task_id: Some(task_id.to_string()),
                ..Default::default()
            },
        )
        .await
    }

    async fn count(&self, ctx: RequestContext, query: ArtifactQuery) -> Result<u64> {
        let pool = ctx.db_pool();
        let mut count_builder = QueryBuilder::new(
            r#"SELECT COUNT(*) FROM artifacts WHERE "status" != 0"#,
        );
        push_query_filters(&mut count_builder, &query);
        let total: i64 = count_builder
            .build_query_scalar()
            .fetch_one(pool)
            .await?;
        Ok(total as u64)
    }

    async fn update_status(&self, ctx: RequestContext, id: &str, status: i32) -> Result<()> {
        let pool = ctx.db_pool();
        let now = common::constants::utils::current_timestamp_ms();
        sqlx::query!(
            r#"
UPDATE artifacts SET "status" = ?, updated_at = ? WHERE id = ?
"#,
            status,
            now,
            id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.update_status(ctx, id, 0).await
    }

    async fn update(&self, ctx: RequestContext, artifact: &ArtifactPo) -> Result<()> {
        let pool = ctx.db_pool();
        let ft = artifact.file_type as i32;
        let source_type = artifact.source_type as i32;
        sqlx::query!(
            r#"
UPDATE artifacts SET
    project_id = ?,
    task_id = ?,
    name = ?,
    description = ?,
    file_type = ?,
    file_meta = ?,
    source_type = ?,
    tags = ?,
    status = ?,
    created_by = ?,
    modified_by = ?,
    created_at = ?,
    updated_at = ?
WHERE id = ?
"#,
            artifact.project_id,
            artifact.task_id,
            artifact.name,
            artifact.description,
            ft,
            artifact.file_meta,
            source_type,
            artifact.tags,
            artifact.status,
            artifact.created_by,
            artifact.modified_by,
            artifact.created_at,
            artifact.updated_at,
            artifact.id
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn read_content(
        &self,
        _ctx: RequestContext,
        artifact: &ArtifactPo,
    ) -> Result<Option<Vec<u8>>> {
        // Only generated content is stored on disk
        if artifact.source_type != common::enums::ArtifactSourceType::GeneratedContent {
            return Ok(None);
        }

        let file_path = self.resolve_generated_content_path(artifact)?;

        if !file_path.exists() {
            return Ok(None);
        }

        let content = std::fs::read(file_path)?;
        Ok(Some(content))
    }

    async fn write_content(
        &self,
        _ctx: RequestContext,
        artifact: &ArtifactPo,
        content: &[u8],
    ) -> Result<()> {
        // Only generated content can be written to disk
        // (Attachment content is handled separately by finance attachment)
        if artifact.source_type != common::enums::ArtifactSourceType::GeneratedContent {
            bail_err!(InvalidRequest, "Cannot write content to artifact of source type {:?}", artifact.source_type);
        }

        let file_path = self.resolve_generated_content_path(artifact)?;

        // Ensure parent directory exists
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(file_path, content)?;

        Ok(())
    }
}

/// 推送查询过滤条件到 QueryBuilder（COUNT 和 LIST 查询复用）
fn push_query_filters<'args>(
    builder: &mut QueryBuilder<'args, sqlx::Sqlite>,
    query: &ArtifactQuery,
) {
    if let Some(project_id) = &query.project_id {
        builder
            .push(" AND project_id = ")
            .push_bind(project_id.clone());
    }
    if let Some(task_id) = &query.task_id {
        builder.push(" AND task_id = ").push_bind(task_id.clone());
    }
    if let Some(file_type) = query.file_type {
        builder
            .push(" AND file_type = ")
            .push_bind(file_type as i32);
    }
    if let Some(source_type) = query.source_type {
        builder
            .push(" AND source_type = ")
            .push_bind(source_type as i32);
    }
}