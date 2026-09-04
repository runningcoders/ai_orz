//! OrganizationLink DAO SQLite 实现

use crate::models::organization_link::OrganizationLinkPo;
use crate::pkg::RequestContext;
use crate::service::dao::organization_link::{
    OrganizationLinkDao, OrganizationLinkQuery, PeerOrgUpsert,
};
use chrono::Utc;
use common::enums::{OrganizationLinkStatus, OrganizationScope};
use common::error::Result;
use sqlx::SqlitePool;
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

impl OrganizationLinkDaoSqliteImpl {
    fn pool(ctx: &RequestContext) -> &SqlitePool {
        ctx.db_pool()
    }
}

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
        let pool = Self::pool(&ctx);
        let mut tx = pool.begin().await?;

        // 1) 连接置 Revoked
        let now = Utc::now().timestamp_millis();
        sqlx::query!(
            "UPDATE organization_links SET status = 0, updated_at = ? WHERE id = ?",
            now,
            link_id
        )
        .execute(&mut *tx)
        .await?;

        // 2) 对端影子记录 Linked → Remote（不删除，保留审计线索；只降级 Linked，不动其他 scope）
        //    注意：不动 organizations.updated_at——影子记录的 updated_at 语义是
        //    「对端数据版本」（新者胜比较基准），本地投影状态变更不参与该比较
        sqlx::query!(
            r#"
UPDATE organizations SET scope = ?
WHERE id = (SELECT peer_org_id FROM organization_links WHERE id = ?) AND scope = ?
            "#,
            OrganizationScope::Remote as i32,
            link_id,
            OrganizationScope::Linked as i32
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    async fn upsert_peer_org(&self, ctx: RequestContext, peer: &PeerOrgUpsert) -> Result<bool> {
        let status = peer.status as i32;
        // updated_at 存对端数据版本（新者胜比较基准）；created_at 为本地行创建时间
        let now = Utc::now().timestamp_millis();
        let result = sqlx::query!(
            r#"
INSERT INTO organizations (id, name, description, base_url, group_name, status, scope, created_by, modified_by, created_at, updated_at)
VALUES (?, ?, ?, ?, ?, ?, ?, '', '', ?, ?)
ON CONFLICT(id) DO UPDATE SET
    name = excluded.name,
    description = excluded.description,
    base_url = excluded.base_url,
    group_name = excluded.group_name,
    status = excluded.status,
    updated_at = excluded.updated_at
WHERE excluded.updated_at > organizations.updated_at
  AND organizations.scope != ?
            "#,
            peer.id,
            peer.name,
            peer.description,
            peer.base_url,
            peer.group_name,
            status,
            OrganizationScope::Remote as i32,
            now,
            peer.updated_at,
            OrganizationScope::Local as i32
        )
        .execute(ctx.db_pool())
        .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn upsert_linked_peer_org(
        &self,
        ctx: RequestContext,
        peer: &PeerOrgUpsert,
    ) -> Result<bool> {
        let status = peer.status as i32;
        // 直接建联：插入即 Linked；更新也强制 Linked（直接相连是权威动作，不依赖新者胜）
        // 仅保护本地组织（scope=Local）不被覆盖（评审稿 R5）
        let now = Utc::now().timestamp_millis();
        let result = sqlx::query!(
            r#"
INSERT INTO organizations (id, name, description, base_url, group_name, status, scope, created_by, modified_by, created_at, updated_at)
VALUES (?, ?, ?, ?, ?, ?, ?, '', '', ?, ?)
ON CONFLICT(id) DO UPDATE SET
    name = excluded.name,
    description = excluded.description,
    base_url = excluded.base_url,
    group_name = excluded.group_name,
    status = excluded.status,
    scope = ?,
    updated_at = excluded.updated_at
WHERE organizations.scope != ?
            "#,
            peer.id,
            peer.name,
            peer.description,
            peer.base_url,
            peer.group_name,
            status,
            OrganizationScope::Linked as i32,
            now,
            peer.updated_at,
            OrganizationScope::Linked as i32,
            OrganizationScope::Local as i32
        )
        .execute(ctx.db_pool())
        .await?;
        Ok(result.rows_affected() > 0)
    }
}
