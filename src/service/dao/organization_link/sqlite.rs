//! OrganizationLink DAO SQLite 实现

use crate::models::organization_link::OrganizationLinkPo;
use crate::pkg::RequestContext;
use crate::service::dao::organization_link::{OrganizationLinkDao, OrganizationLinkQuery};
use chrono::Utc;
use common::enums::OrganizationLinkStatus;
use common::error::Result;
use std::sync::OnceLock;

// ==================== 工厂方法 + 单例管理 ====================

static ORGANIZATION_LINK_DAO: OnceLock<std::sync::Arc<dyn OrganizationLinkDao>> = OnceLock::new();

/// 创建一个全新的 OrganizationLink DAO 实例（用于测试）
pub fn new() -> std::sync::Arc<dyn OrganizationLinkDao> {
    std::sync::Arc::new(OrganizationLinkDaoSqliteImpl)
}

/// 获取 OrganizationLink DAO 单例
pub fn dao() -> std::sync::Arc<dyn OrganizationLinkDao> {
    ORGANIZATION_LINK_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = ORGANIZATION_LINK_DAO.set(new());
}

// ==================== 实现 ====================

struct OrganizationLinkDaoSqliteImpl;

#[async_trait::async_trait]
impl OrganizationLinkDao for OrganizationLinkDaoSqliteImpl {
    async fn insert(&self, ctx: RequestContext, link: &OrganizationLinkPo) -> Result<()> {
        let status = link.status as i32;
        sqlx::query!(
            "INSERT INTO organization_links (id, local_org_id, peer_org_id, endpoint, access_token, peer_token_hash, status, created_by, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            link.id,
            link.local_org_id,
            link.peer_org_id,
            link.endpoint,
            link.access_token,
            link.peer_token_hash,
            status,
            link.created_by,
            link.created_at,
            link.updated_at
        )
        .execute(ctx.db_pool())
        .await?;
        Ok(())
    }

    async fn find_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<OrganizationLinkPo>> {
        let link = sqlx::query_as!(
            OrganizationLinkPo,
            r#"
SELECT id, local_org_id, peer_org_id, endpoint, access_token, peer_token_hash,
       status as 'status: OrganizationLinkStatus', created_by, created_at, updated_at
FROM organization_links WHERE id = ?
            "#,
            id
        )
        .fetch_optional(ctx.db_pool())
        .await?;
        Ok(link)
    }

    async fn find_by_pair(
        &self,
        ctx: RequestContext,
        local_org_id: &str,
        peer_org_id: &str,
    ) -> Result<Option<OrganizationLinkPo>> {
        let link = sqlx::query_as!(
            OrganizationLinkPo,
            r#"
SELECT id, local_org_id, peer_org_id, endpoint, access_token, peer_token_hash,
       status as 'status: OrganizationLinkStatus', created_by, created_at, updated_at
FROM organization_links WHERE local_org_id = ? AND peer_org_id = ?
            "#,
            local_org_id,
            peer_org_id
        )
        .fetch_optional(ctx.db_pool())
        .await?;
        Ok(link)
    }

    async fn find_active_by_peer_token_hash(
        &self,
        ctx: RequestContext,
        peer_token_hash: &str,
    ) -> Result<Option<OrganizationLinkPo>> {
        let link = sqlx::query_as!(
            OrganizationLinkPo,
            r#"
SELECT id, local_org_id, peer_org_id, endpoint, access_token, peer_token_hash,
       status as 'status: OrganizationLinkStatus', created_by, created_at, updated_at
FROM organization_links WHERE peer_token_hash = ? AND status = 1 LIMIT 1
            "#,
            peer_token_hash
        )
        .fetch_optional(ctx.db_pool())
        .await?;
        Ok(link)
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: OrganizationLinkQuery,
    ) -> Result<Vec<OrganizationLinkPo>> {
        let pool = ctx.db_pool();
        let mut builder = sqlx::QueryBuilder::new(
            r#"SELECT id, local_org_id, peer_org_id, endpoint, access_token, peer_token_hash, status, created_by, created_at, updated_at FROM organization_links WHERE 1=1"#,
        );

        if let Some(local_org_id) = &query.local_org_id {
            builder.push(" AND local_org_id = ").push_bind(local_org_id);
        }
        if let Some(status) = query.status {
            builder.push(" AND status = ").push_bind(status as i32);
        }
        builder.push(" ORDER BY created_at DESC");
        if let Some(limit) = query.limit {
            builder.push(" LIMIT ").push_bind(limit as i64);
        }

        let rows = builder.build_query_as().fetch_all(pool).await?;
        Ok(rows)
    }

    async fn update(&self, ctx: RequestContext, link: &OrganizationLinkPo) -> Result<()> {
        let status = link.status as i32;
        let current_timestamp = Utc::now().timestamp_millis();
        sqlx::query!(
            r#"
UPDATE organization_links
SET endpoint = ?, access_token = ?, peer_token_hash = ?, status = ?, updated_at = ?
WHERE id = ?
            "#,
            link.endpoint,
            link.access_token,
            link.peer_token_hash,
            status,
            current_timestamp,
            link.id
        )
        .execute(ctx.db_pool())
        .await?;
        Ok(())
    }

    async fn revoke(&self, ctx: RequestContext, link_id: &str) -> Result<()> {
        // 仅置 links 表 Revoked（幂等，重放无害）；
        // 对端影子 Linked → Remote 降级在 organization DAL 的 revoke_link 组合方法中
        let now = Utc::now().timestamp_millis();
        sqlx::query!(
            "UPDATE organization_links SET status = 0, updated_at = ? WHERE id = ?",
            now,
            link_id
        )
        .execute(ctx.db_pool())
        .await?;
        Ok(())
    }
}
