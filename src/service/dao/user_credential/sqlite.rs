//! UserCredential DAO SQLite 实现

use crate::models::user_credential::UserCredentialPo;
use crate::pkg::RequestContext;
use crate::service::dao::user_credential::{UserCredentialDao, UserCredentialQuery};
use async_trait::async_trait;
use common::api::PagedResult;
use common::constants::utils;
use common::error::{Result, err};
use common::models::{CredentialKind, CredentialVisibility};
use sqlx::QueryBuilder;
use sqlx::types::Json;
use std::sync::{Arc, OnceLock};

// ==================== 工厂方法 + 单例 ====================

static USER_CREDENTIAL_DAO: OnceLock<Arc<dyn UserCredentialDao>> = OnceLock::new();

/// 创建一个全新的 UserCredential DAO 实例（用于测试）
pub fn new() -> Arc<dyn UserCredentialDao> {
    Arc::new(UserCredentialDaoSqliteImpl)
}

/// 获取全局 UserCredential DAO 单例
pub fn dao() -> Arc<dyn UserCredentialDao> {
    USER_CREDENTIAL_DAO.get().cloned().unwrap()
}

/// 初始化全局 UserCredential DAO
pub fn init() {
    let _ = USER_CREDENTIAL_DAO.set(new());
}

// ==================== 实现 ====================

struct UserCredentialDaoSqliteImpl;

/// QueryBuilder 运行时查询列集（纯列名；类型解码由 FromRow + sqlx::Type 完成，
/// `as "col: Type"` 标注语法仅 query! 宏有效，运行时会把列名变成字面量）
const CREDENTIAL_COLUMNS: &str = "id, org_id, user_id, kind, name, detail, \
    visibility, is_default, status, created_by, modified_by, created_at, updated_at";

#[async_trait]
impl UserCredentialDao for UserCredentialDaoSqliteImpl {
    async fn insert(&self, ctx: RequestContext, po: &UserCredentialPo) -> Result<()> {
        sqlx::query!(
            r#"
INSERT INTO user_credentials (
    id, org_id, user_id, kind, name, detail, visibility, is_default, status,
    created_by, modified_by, created_at, updated_at
)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            po.id,
            po.org_id,
            po.user_id,
            po.kind as _,
            po.name,
            po.detail as _,
            po.visibility as _,
            po.is_default,
            po.status,
            po.created_by,
            po.modified_by,
            po.created_at,
            po.updated_at,
        )
        .execute(ctx.db_pool())
        .await?;

        Ok(())
    }

    async fn update(&self, ctx: RequestContext, po: &UserCredentialPo) -> Result<()> {
        let now = utils::current_timestamp_ms();
        sqlx::query!(
            r#"
UPDATE user_credentials
SET name = ?, detail = ?, visibility = ?, is_default = ?, modified_by = ?, updated_at = ?
WHERE id = ?
            "#,
            po.name,
            po.detail as _,
            po.visibility as _,
            po.is_default,
            po.modified_by,
            now,
            po.id,
        )
        .execute(ctx.db_pool())
        .await?;

        Ok(())
    }

    async fn soft_delete(&self, ctx: RequestContext, id: &str) -> Result<()> {
        let uid = ctx.caller_id_or_system();
        let now = utils::current_timestamp_ms();
        sqlx::query!(
            r#"
UPDATE user_credentials SET status = 0, is_default = 0, modified_by = ?, updated_at = ?
WHERE id = ?
            "#,
            uid,
            now,
            id,
        )
        .execute(ctx.db_pool())
        .await?;

        Ok(())
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: UserCredentialQuery,
    ) -> Result<PagedResult<UserCredentialPo>> {
        let pool = ctx.db_pool();

        let mut count_builder = QueryBuilder::new("SELECT COUNT(*) FROM user_credentials WHERE 1=1");
        push_query_filters(&mut count_builder, &query);
        let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;

        let mut list_builder = QueryBuilder::new(format!(
            "SELECT {CREDENTIAL_COLUMNS} FROM user_credentials WHERE 1=1"
        ));
        push_query_filters(&mut list_builder, &query);

        // 排序（默认创建序，与解析链创建序语义一致）
        if let Some(order_by) = &query.order_by {
            list_builder.push(" ORDER BY ").push(order_by.clone());
        } else {
            list_builder.push(" ORDER BY created_at ASC");
        }

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

    async fn count(&self, ctx: RequestContext, query: UserCredentialQuery) -> Result<u64> {
        let pool = ctx.db_pool();
        let mut count_builder = QueryBuilder::new("SELECT COUNT(*) FROM user_credentials WHERE 1=1");
        push_query_filters(&mut count_builder, &query);
        let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;
        Ok(total as u64)
    }

    async fn find_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<UserCredentialPo>> {
        let po = sqlx::query_as!(
            UserCredentialPo,
            r#"
SELECT id, org_id, user_id,
    kind as "kind: CredentialKind", name, detail as "detail: Json<common::models::CredentialDetail>",
    visibility as "visibility: CredentialVisibility", is_default as "is_default: bool", status as "status: i32",
    created_by, modified_by, created_at, updated_at
FROM user_credentials WHERE id = ? AND status != 0
            "#,
            id
        )
        .fetch_optional(ctx.db_pool())
        .await?;

        Ok(po)
    }

    async fn find_default(
        &self,
        ctx: RequestContext,
        user_id: &str,
        kind: CredentialKind,
    ) -> Result<Option<UserCredentialPo>> {
        // 解析链 §2.3 链 2→5 单点（作用域优先）：
        // 候选 = 本人活跃凭据 + 同 org public 活跃凭据（org 经 JOIN users 取得）；
        // 排序键依次为：个人作用域优先（user_id 匹配 DESC）→ 作用域内默认优先
        // （is_default DESC）→ 创建序（created_at ASC）
        let po = sqlx::query_as!(
            UserCredentialPo,
            r#"
SELECT uc.id, uc.org_id, uc.user_id,
    uc.kind as "kind: CredentialKind", uc.name, uc.detail as "detail: Json<common::models::CredentialDetail>",
    uc.visibility as "visibility: CredentialVisibility", uc.is_default as "is_default: bool", uc.status as "status: i32",
    uc.created_by, uc.modified_by, uc.created_at, uc.updated_at
FROM user_credentials uc, users u
WHERE u.id = ? AND uc.kind = ? AND uc.status = 1
  AND (uc.user_id = u.id OR (uc.org_id = u.organization_id AND uc.visibility = ?))
ORDER BY (uc.user_id = u.id) DESC, uc.is_default DESC, uc.created_at ASC
LIMIT 1
            "#,
            user_id,
            kind as _,
            CredentialVisibility::Public as _,
        )
        .fetch_optional(ctx.db_pool())
        .await?;

        Ok(po)
    }

    async fn set_default(&self, ctx: RequestContext, credential_id: &str) -> Result<()> {
        // 作用域由目标凭据 visibility 派生：private=个人默认(user_id+kind) /
        // public=组织默认(org_id+kind)；先取目标再同事务清旧立新
        let target = sqlx::query_as!(
            UserCredentialPo,
            r#"
SELECT id, org_id, user_id,
    kind as "kind: CredentialKind", name, detail as "detail: Json<common::models::CredentialDetail>",
    visibility as "visibility: CredentialVisibility", is_default as "is_default: bool", status as "status: i32",
    created_by, modified_by, created_at, updated_at
FROM user_credentials WHERE id = ? AND status != 0
            "#,
            credential_id
        )
        .fetch_optional(ctx.db_pool())
        .await?
        .ok_or_else(|| err!(NotFound, "凭证不存在 credential_id={}", credential_id))?;

        let now = utils::current_timestamp_ms();
        let operator = ctx.caller_id_or_system();
        let mut tx = ctx.db_pool().begin().await?;

        // 清同作用域旧默认（作用域列组合由 visibility 派生）
        match target.visibility {
            CredentialVisibility::Private => {
                sqlx::query!(
                    r#"
UPDATE user_credentials SET is_default = 0, modified_by = ?, updated_at = ?
WHERE user_id = ? AND kind = ? AND visibility = ? AND is_default = 1 AND status = 1
                    "#,
                    operator,
                    now,
                    target.user_id,
                    target.kind as _,
                    CredentialVisibility::Private as _,
                )
                .execute(&mut *tx)
                .await?;
            }
            CredentialVisibility::Public => {
                sqlx::query!(
                    r#"
UPDATE user_credentials SET is_default = 0, modified_by = ?, updated_at = ?
WHERE org_id = ? AND kind = ? AND visibility = ? AND is_default = 1 AND status = 1
                    "#,
                    operator,
                    now,
                    target.org_id,
                    target.kind as _,
                    CredentialVisibility::Public as _,
                )
                .execute(&mut *tx)
                .await?;
            }
        }

        // 立新默认
        sqlx::query!(
            r#"
UPDATE user_credentials SET is_default = 1, modified_by = ?, updated_at = ?
WHERE id = ? AND status = 1
            "#,
            operator,
            now,
            credential_id,
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn clear_default(
        &self,
        ctx: RequestContext,
        user_id: &str,
        kind: CredentialKind,
    ) -> Result<()> {
        let operator = ctx.caller_id_or_system();
        let now = utils::current_timestamp_ms();
        sqlx::query!(
            r#"
UPDATE user_credentials SET is_default = 0, modified_by = ?, updated_at = ?
WHERE user_id = ? AND kind = ? AND visibility = ? AND is_default = 1 AND status = 1
            "#,
            operator,
            now,
            user_id,
            kind as _,
            CredentialVisibility::Private as _,
        )
        .execute(ctx.db_pool())
        .await?;

        Ok(())
    }
}

/// 推送查询过滤条件到 QueryBuilder（COUNT 和 LIST 查询复用，AGENTS §4.9）
fn push_query_filters<'args>(
    builder: &mut QueryBuilder<'args, sqlx::Sqlite>,
    query: &UserCredentialQuery,
) {
    if let Some(id) = &query.id {
        builder.push(" AND id = ").push_bind(id.clone());
    }
    if let Some(org_id) = &query.org_id {
        builder.push(" AND org_id = ").push_bind(org_id.clone());
    }
    if let Some(user_id) = &query.user_id {
        builder.push(" AND user_id = ").push_bind(user_id.clone());
    }
    if let Some(kind) = query.kind {
        builder.push(" AND kind = ").push_bind(kind);
    }
    if let Some(visibility) = query.visibility {
        builder
            .push(" AND visibility = ")
            .push_bind(visibility);
    }
    if let Some(is_default) = query.is_default {
        builder
            .push(" AND is_default = ")
            .push_bind(is_default as i64);
    }
    // 软删默认过滤：未显式指定 status_in 时只看活跃凭证
    match &query.status_in {
        Some(status_in) if !status_in.is_empty() => {
            builder.push(" AND status IN (");
            let mut separated = builder.separated(", ");
            for s in status_in {
                separated.push_bind(*s as i64);
            }
            builder.push(")");
        }
        _ => {
            builder.push(" AND status != 0");
        }
    }
    if let Some(keyword) = &query.keyword {
        let pattern = format!("%{}%", keyword.trim());
        builder.push(" AND name LIKE ").push_bind(pattern);
    }
}
