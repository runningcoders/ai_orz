//! SQLite implementation of Artifact DAO

use std::sync::OnceLock;
use std::sync::Arc;
use crate::pkg::RequestContext;
use sqlx::types::Json;
use sqlx::SqlitePool;
use common::enums::FileType;
use crate::models::{artifact::ArtifactPo, file::FileMeta};
use crate::error::Result;
use super::{ArtifactDao, ArtifactQuery};

// ==================== 工厂方法 + 单例 ====================

static DAO_INSTANCE: OnceLock<Arc<dyn ArtifactDao + Send + Sync>> = OnceLock::new();

/// 创建一个全新的 Artifact DAO 实例（用于测试）
pub fn new() -> Arc<dyn ArtifactDao + Send + Sync> {
    Arc::new(ArtifactDaoSqliteImpl::new())
}

/// Get the singleton Artifact DAO instance
pub fn dao() -> Arc<dyn ArtifactDao + Send + Sync> {
    DAO_INSTANCE.get().expect("Artifact DAO not initialized").clone()
}

/// Initialize the Artifact DAO
pub fn init() {
    let _ = DAO_INSTANCE.set(new());
}

#[derive(Debug, Default)]
struct ArtifactDaoSqliteImpl;

impl ArtifactDaoSqliteImpl {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl ArtifactDao for ArtifactDaoSqliteImpl {
    async fn insert(
        &self,
        ctx: RequestContext,
        artifact: &ArtifactPo,
    ) -> Result<()> {
        let pool = ctx.db_pool();
        let ft = artifact.file_type as i32;
        sqlx::query!(
r#"
INSERT INTO artifacts (id, task_id, name, description, file_type, file_meta, status, created_by, modified_by, created_at, updated_at) VALUES (
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
            artifact.task_id,
            artifact.name,
            artifact.description,
            ft,
            artifact.file_meta,
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

    async fn find_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<ArtifactPo>> {
        let pool = ctx.db_pool();
        let artifact = sqlx::query_as!(
            ArtifactPo,
r#"
SELECT id, task_id, name, description, file_type as "file_type: FileType", file_meta as "file_meta: Json<FileMeta>", status as "status: i32", created_by, modified_by, created_at, updated_at
FROM artifacts
WHERE id = ? AND "status" != 0
"#,
            id
        )
        .fetch_optional(pool)
        .await?;
        Ok(artifact)
    }

    async fn query(&self, ctx: RequestContext, query: ArtifactQuery) -> Result<Vec<ArtifactPo>> {
        let pool = ctx.db_pool();
        let mut builder = sqlx::QueryBuilder::new(
            r#"SELECT id, task_id, name, description, file_type, file_meta, status, created_by, modified_by, created_at, updated_at FROM artifacts WHERE "status" != 0"#
        );

        // 任务过滤
        if let Some(task_id) = &query.task_id {
            builder.push(" AND task_id = ").push_bind(task_id);
        }

        // 排序
        builder.push(" ORDER BY created_at DESC");

        // 限制数量
        if let Some(limit) = query.limit {
            builder.push(" LIMIT ").push_bind(limit as i64);
        }

        let rows = builder.build_query_as()
            .fetch_all(pool)
            .await?;

        Ok(rows)
    }

    async fn list_by_task(&self, ctx: RequestContext, task_id: &str) -> Result<Vec<ArtifactPo>> {
        // 语法糖：调用通用查询
        self.query(ctx, ArtifactQuery {
            task_id: Some(task_id.to_string()),
            ..Default::default()
        }).await
    }

    async fn count_by_task(
        &self,
        ctx: RequestContext,
        task_id: &str,
    ) -> Result<i64> {
        let pool = ctx.db_pool();
        let row = sqlx::query!(
r#"
SELECT COUNT(*) as count FROM artifacts WHERE task_id = ? AND "status" != 0
"#,
            task_id
        )
        .fetch_one(pool)
        .await?;
        Ok(row.count)
    }

    async fn update_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: i32,
    ) -> Result<()> {
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

    async fn delete(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<()> {
        self.update_status(ctx, id, 0).await
    }
}
