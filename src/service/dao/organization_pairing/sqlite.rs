//! OrganizationPairingCode DAO SQLite 实现

use crate::models::organization_pairing_code::OrganizationPairingCodePo;
use crate::pkg::RequestContext;
use crate::service::dao::organization_pairing::OrganizationPairingDao;
use common::error::Result;
use std::sync::{Arc, OnceLock};

// ==================== 工厂方法 + 单例管理 ====================

static ORGANIZATION_PAIRING_DAO: OnceLock<Arc<dyn OrganizationPairingDao>> = OnceLock::new();

/// 创建一个全新的 OrganizationPairingCode DAO 实例（用于测试）
pub fn new() -> Arc<dyn OrganizationPairingDao> {
    Arc::new(OrganizationPairingDaoSqliteImpl)
}

/// 获取 OrganizationPairingCode DAO 单例
pub fn dao() -> Arc<dyn OrganizationPairingDao> {
    ORGANIZATION_PAIRING_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = ORGANIZATION_PAIRING_DAO.set(new());
}

// ==================== 实现 ====================

struct OrganizationPairingDaoSqliteImpl;

#[async_trait::async_trait]
impl OrganizationPairingDao for OrganizationPairingDaoSqliteImpl {
    async fn insert(&self, ctx: RequestContext, code: &OrganizationPairingCodePo) -> Result<()> {
        sqlx::query!(
            r#"
INSERT INTO organization_pairing_codes (id, org_id, code_hash, expires_at, consumed_at, created_by, created_at)
VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
            code.id,
            code.org_id,
            code.code_hash,
            code.expires_at,
            code.consumed_at,
            code.created_by,
            code.created_at
        )
        .execute(ctx.db_pool())
        .await?;
        Ok(())
    }

    async fn consume(
        &self,
        ctx: RequestContext,
        code_hash: &str,
        now: i64,
    ) -> Result<Option<String>> {
        // 原子操作：仅当「存在 + 未消费 + 未过期」时置 consumed_at，并返回签发组织 ID。
        // 一次失败的并发消费会被唯一索引 + 此 WHERE 天然串行化；第二个消费因
        // consumed_at 已非 NULL 而返回 None（单用途保障）。
        let row = sqlx::query!(
            r#"
UPDATE organization_pairing_codes
SET consumed_at = ?
WHERE code_hash = ? AND consumed_at IS NULL AND expires_at > ?
RETURNING org_id
            "#,
            now,
            code_hash,
            now
        )
        .fetch_optional(ctx.db_pool())
        .await?;
        Ok(row.map(|r| r.org_id))
    }
}
