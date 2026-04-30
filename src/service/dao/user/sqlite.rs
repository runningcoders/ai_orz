//! User DAO SQLite 实现

use crate::error::AppError;
use crate::models::user::UserPo;
use common::enums::{UserRole, UserStatus};
use crate::pkg::RequestContext;
use crate::service::dao::user::{UserDao, UserQuery};
use std::sync::{Arc, OnceLock};
use chrono::Utc;

// ==================== 工厂方法 + 单例 ====================

static USER_DAO: OnceLock<Arc<dyn UserDao>> = OnceLock::new();

/// 创建一个全新的 User DAO 实例（用于测试）
pub fn new() -> Arc<dyn UserDao> {
    Arc::new(UserDaoSqliteImpl::new())
}

/// 获取 User DAO 单例
pub fn dao() -> Arc<dyn UserDao> {
    USER_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = USER_DAO.set(new());
}

// ==================== 实现 ====================

struct UserDaoSqliteImpl;

impl UserDaoSqliteImpl {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl UserDao for UserDaoSqliteImpl {
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
FROM users WHERE id = ? AND status != 0
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
FROM users WHERE username = ? AND status != 0
            "#,
            username
        )
            .fetch_optional(ctx.db_pool())
            .await?;

        Ok(user)
    }

    async fn query(&self, ctx: RequestContext, query: UserQuery) -> Result<Vec<UserPo>, AppError> {
        let pool = ctx.db_pool();
        let mut builder = sqlx::QueryBuilder::new(
            r#"SELECT id, organization_id, username, display_name, email, password_hash, role, status, created_by, modified_by, created_at, updated_at FROM users WHERE status != 0"#
        );

        // 组织过滤
        if let Some(org_id) = &query.organization_id {
            builder.push(" AND organization_id = ").push_bind(org_id);
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

    async fn find_by_organization_id(&self, ctx: RequestContext, org_id: &str) -> Result<Vec<UserPo>, AppError> {
        // 语法糖：调用通用查询
        self.query(ctx, UserQuery {
            organization_id: Some(org_id.to_string()),
            ..Default::default()
        }).await
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
