//! Organization DAO SQLite 实现

use crate::error::AppError;
use crate::models::organization::OrganizationPo;
use common::enums::{OrganizationStatus, OrganizationScope};
use crate::pkg::RequestContext;
use crate::service::dao::organization::{OrganizationDao, OrganizationQuery};
use std::sync::{Arc, OnceLock};
use chrono::Utc;
// ==================== 工厂方法 + 单例管理 ====================

static ORGANIZATION_DAO: OnceLock<Arc<dyn OrganizationDao>> = OnceLock::new();

/// 创建一个全新的 Organization DAO 实例（用于测试）
pub fn new() -> Arc<dyn OrganizationDao> {
    Arc::new(OrganizationDaoSqliteImpl::new())
}

/// 获取 Organization DAO 单例
pub fn dao() -> Arc<dyn OrganizationDao> {
    ORGANIZATION_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = ORGANIZATION_DAO.set(new());
}

// ==================== 实现 ====================

struct OrganizationDaoSqliteImpl;

impl OrganizationDaoSqliteImpl {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl OrganizationDao for OrganizationDaoSqliteImpl {
    async fn insert(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<(), AppError> {
        let status = org.status as i32;
        let scope = org.scope as i32;
        sqlx::query!(
            "INSERT INTO organizations (id, name, description, base_url, status, scope, created_by, modified_by, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            org.id,
            org.name,
            org.description,
            org.base_url,
            status,
            scope,
            org.created_by,
            org.modified_by,
            org.created_at,
            org.updated_at
        )
            .execute(ctx.db_pool())
            .await?;

        Ok(())
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<OrganizationPo>, AppError> {
        let org = sqlx::query_as!(
            OrganizationPo,
            r#"
SELECT id, name, description, base_url, status as 'status: OrganizationStatus', scope as 'scope: OrganizationScope', created_by, modified_by, created_at, updated_at
FROM organizations WHERE id = ? AND status != 0
            "#,
            id
        )
            .fetch_optional(ctx.db_pool())
            .await?;

        Ok(org)
    }

    async fn query(&self, ctx: RequestContext, query: OrganizationQuery) -> Result<Vec<OrganizationPo>, AppError> {
        let pool = ctx.db_pool();
        let mut builder = sqlx::QueryBuilder::new(
            r#"SELECT id, name, description, base_url, status, scope, created_by, modified_by, created_at, updated_at FROM organizations WHERE status != 0"#
        );

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

    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<OrganizationPo>, AppError> {
        // 语法糖：调用通用查询
        self.query(ctx, OrganizationQuery::default()).await
    }

    async fn update(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<(), AppError> {
        let current_timestamp = Utc::now().timestamp();
        let uid = ctx.uid().to_string();
        let status = org.status as i32;
        let scope = org.scope as i32;
        sqlx::query!(
            r#"
UPDATE organizations
SET name = ?, description = ?, base_url = ?, status = ?, scope = ?, modified_by = ?, updated_at = ?
WHERE id = ?
            "#,
            org.name,
            org.description,
            org.base_url,
            status,
            scope,
            uid,
            current_timestamp,
            org.id
        )
            .execute(ctx.db_pool())
            .await?;

        Ok(())
    }

    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError> {
        let current_timestamp = Utc::now().timestamp();
        let uid = ctx.uid().to_string();
        sqlx::query!(
            r#"
UPDATE organizations SET status = 0, modified_by = ?, updated_at = ? WHERE id = ?
            "#,
            uid,
            current_timestamp,
            id
        )
            .execute(ctx.db_pool())
            .await?;

        Ok(())
    }

    async fn count_all(&self, ctx: RequestContext) -> Result<u64, AppError> {
        let count = sqlx::query!(
            r#"SELECT COUNT(*) as count FROM organizations WHERE status != 0"#
        )
            .fetch_one(ctx.db_pool())
            .await?;

        Ok(count.count as u64)
    }
}

fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
