//! User DAO SQLite 实现

use crate::error::AppError;
use crate::models::user::UserPo;
use common::enums::{UserRole, UserStatus};
use crate::pkg::RequestContext;
use crate::service::dao::user::UserDaoTrait;
use std::sync::{Arc, OnceLock};
use chrono::Utc;
// ==================== 单例管理 ====================

static USER_DAO: OnceLock<Arc<dyn UserDaoTrait>> = OnceLock::new();

/// 获取 User DAO 单例
pub fn dao() -> Arc<dyn UserDaoTrait> {
    USER_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = USER_DAO.set(Arc::new(UserDaoImpl::new()));
}

// ==================== 实现 ====================

pub struct UserDaoImpl;

impl UserDaoImpl {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl UserDaoTrait for UserDaoImpl {
    async fn insert(&self, ctx: RequestContext, user: &UserPo) -> Result<(), AppError> {
        let role = user.role as i32;
        let status = user.status as i32;
        sqlx::query!(
            "INSERT INTO users (id, organization_id, username, display_name, email, password_hash, role, status, created_by, modified_by, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            user.id,
            user.organization_id,
            user.username,
            user.display_name,
            user.email,
            user.password_hash,
            role,
            status,
            user.created_by,
            user.modified_by,
            user.created_at,
            user.updated_at
        )
            .execute(ctx.db_pool())
            .await?;

        Ok(())
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<UserPo>, AppError> {
        let user = sqlx::query_as!(
            UserPo,
            r#"
SELECT id, organization_id, username, display_name, email, password_hash,
       role as 'role: UserRole', status as 'status: UserStatus', created_by, modified_by, created_at, updated_at
FROM users WHERE id = ?
            "#,
            id
        )
            .fetch_optional(ctx.db_pool())
            .await?;

        Ok(user)
    }

    async fn find_by_username(&self, ctx: RequestContext, username: &str) -> Result<Option<UserPo>, AppError> {
        let user = sqlx::query_as!(
            UserPo,
            r#"
SELECT id, organization_id, username, display_name, email, password_hash,
       role as 'role: UserRole', status as 'status: UserStatus', created_by, modified_by, created_at, updated_at
FROM users WHERE username = ?
            "#,
            username
        )
            .fetch_optional(ctx.db_pool())
            .await?;

        Ok(user)
    }

    async fn find_by_organization_id(&self, ctx: RequestContext, org_id: &str) -> Result<Vec<UserPo>, AppError> {
        let users = sqlx::query_as!(
            UserPo,
            r#"
SELECT id, organization_id, username, display_name, email, password_hash,
       role as 'role: UserRole', status as 'status: UserStatus', created_by, modified_by, created_at, updated_at
FROM users WHERE organization_id = ? AND status != 0
            "#,
            org_id
        )
            .fetch_all(ctx.db_pool())
            .await?;

        Ok(users)
    }

    async fn update(&self, ctx: RequestContext, user: &UserPo) -> Result<(), AppError> {
        let current_timestamp = Utc::now().timestamp();
        let uid = ctx.uid().to_string();
        let role = user.role as i32;
        let status = user.status as i32;
        sqlx::query!(
            r#"
UPDATE users
SET organization_id = ?, username = ?, display_name = ?, email = ?, password_hash = ?,
    role = ?, status = ?, modified_by = ?, updated_at = ?
WHERE id = ?
            "#,
            user.organization_id,
            user.username,
            user.display_name,
            user.email,
            user.password_hash,
            role,
            status,
            uid,
            current_timestamp,
            user.id
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
UPDATE users SET status = 0, modified_by = ?, updated_at = ? WHERE id = ?
            "#,
            uid,
            current_timestamp,
            id
        )
            .execute(ctx.db_pool())
            .await?;

        Ok(())
    }

    async fn exists_by_username(&self, ctx: RequestContext, username: &str) -> Result<bool, AppError> {
        let count = sqlx::query!(
            "SELECT COUNT(*) as count FROM users WHERE username = ?",
            username
        )
            .fetch_one(ctx.db_pool())
            .await?;

        Ok(count.count > 0)
    }

    async fn count_by_organization_id(&self, ctx: RequestContext, org_id: &str) -> Result<u64, AppError> {
        let count = sqlx::query!(
            "SELECT COUNT(*) as count FROM users WHERE organization_id = ? AND status != 0",
            org_id
        )
            .fetch_one(ctx.db_pool())
            .await?;

        Ok(count.count as u64)
    }
}
