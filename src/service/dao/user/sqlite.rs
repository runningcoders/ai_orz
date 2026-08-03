//! User DAO SQLite 实现

use crate::models::user::UserPo;
use crate::pkg::RequestContext;
use crate::service::dao::user::{UserDao, UserQuery};
use chrono::Utc;
use common::api::PagedResult;
use common::enums::{UserRole, UserStatus};
use common::error::Result;
use sqlx::QueryBuilder;
use std::sync::{Arc, OnceLock};

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
    async fn insert(&self, ctx: RequestContext, user: &UserPo) -> Result<()> {
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

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<UserPo>> {
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

    async fn find_by_username(
        &self,
        ctx: RequestContext,
        username: &str,
    ) -> Result<Option<UserPo>> {
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

    async fn query(&self, ctx: RequestContext, query: UserQuery) -> Result<PagedResult<UserPo>> {
        let pool = ctx.db_pool();

        let mut count_builder = QueryBuilder::new("SELECT COUNT(*) FROM users WHERE status != 0");
        push_query_filters(&mut count_builder, &query);
        let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;

        let mut list_builder = QueryBuilder::new(
            r#"SELECT id, organization_id, username, display_name, email, password_hash, role, status, created_by, modified_by, created_at, updated_at FROM users WHERE status != 0"#,
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

    async fn find_by_organization_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<Vec<UserPo>> {
        // 语法糖：调用通用查询
        let page = self
            .query(
                ctx,
                UserQuery {
                    organization_id: Some(org_id.to_string()),
                    ..Default::default()
                },
            )
            .await?;
        Ok(page.items)
    }

    async fn update(&self, ctx: RequestContext, user: &UserPo) -> Result<()> {
        let current_timestamp = Utc::now().timestamp();
        let uid = ctx.caller_id_or_system();
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

    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()> {
        let current_timestamp = Utc::now().timestamp();
        let uid = ctx.caller_id_or_system();
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

    async fn exists_by_username(&self, ctx: RequestContext, username: &str) -> Result<bool> {
        let count = sqlx::query!(
            "SELECT COUNT(*) as count FROM users WHERE username = ?",
            username
        )
        .fetch_one(ctx.db_pool())
        .await?;

        Ok(count.count > 0)
    }

    async fn count_by_organization_id(&self, ctx: RequestContext, org_id: &str) -> Result<u64> {
        // 语法糖：调用通用 count
        self.count(
            ctx,
            UserQuery {
                organization_id: Some(org_id.to_string()),
                ..Default::default()
            },
        )
        .await
    }

    async fn count(&self, ctx: RequestContext, query: UserQuery) -> Result<u64> {
        let pool = ctx.db_pool();
        let mut count_builder = QueryBuilder::new("SELECT COUNT(*) FROM users WHERE status != 0");
        push_query_filters(&mut count_builder, &query);
        let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;
        Ok(total as u64)
    }
}

/// 推送查询过滤条件到 QueryBuilder（COUNT 和 LIST 查询复用）
fn push_query_filters<'args>(builder: &mut QueryBuilder<'args, sqlx::Sqlite>, query: &UserQuery) {
    if let Some(org_id) = &query.organization_id {
        builder
            .push(" AND organization_id = ")
            .push_bind(org_id.clone());
    }
}
