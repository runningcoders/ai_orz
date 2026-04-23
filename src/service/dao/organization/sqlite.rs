//! Organization DAO SQLite 实现

use crate::error::AppError;
use crate::models::organization::OrganizationPo;
use common::enums::{OrganizationStatus, OrganizationScope};
use crate::pkg::RequestContext;
use crate::service::dao::organization::OrganizationDaoTrait;
use std::sync::{Arc, OnceLock};
use chrono::Utc;
// ==================== 工厂方法 + 单例管理 ====================

static ORGANIZATION_DAO: OnceLock<Arc<dyn OrganizationDaoTrait>> = OnceLock::new();

/// 创建一个全新的 Organization DAO 实例（用于测试）
pub fn new() -> Arc<dyn OrganizationDaoTrait> {
    Arc::new(OrganizationDaoImpl::new())
}

/// 获取 Organization DAO 单例
pub fn dao() -> Arc<dyn OrganizationDaoTrait> {
    ORGANIZATION_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = ORGANIZATION_DAO.set(new());
}

// ==================== 实现 ====================

struct OrganizationDaoImpl;

impl OrganizationDaoImpl {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl OrganizationDaoTrait for OrganizationDaoImpl {
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

    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<OrganizationPo>, AppError> {
        let orgs = sqlx::query_as!(
            OrganizationPo,
            r#"
SELECT id, name, description, base_url, status as 'status: OrganizationStatus', scope as 'scope: OrganizationScope', created_by, modified_by, created_at, updated_at
FROM organizations WHERE status != 0
            "#
        )
            .fetch_all(ctx.db_pool())
            .await?;

        Ok(orgs)
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
