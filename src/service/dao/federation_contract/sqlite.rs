//! FederationContract DAO SQLite 实现

use crate::models::federation_contract::FederationContractPo;
use crate::pkg::RequestContext;
use crate::service::dao::federation_contract::FederationContractDao;
use common::enums::{FederationContractKind, FederationContractState};
use common::error::Result;
use std::sync::OnceLock;

// ==================== 工厂方法 + 单例管理 ====================

static FEDERATION_CONTRACT_DAO: OnceLock<std::sync::Arc<dyn FederationContractDao>> =
    OnceLock::new();

/// 创建一个全新的 FederationContract DAO 实例（用于测试）
pub fn new() -> std::sync::Arc<dyn FederationContractDao> {
    std::sync::Arc::new(FederationContractDaoSqliteImpl)
}

/// 获取 FederationContract DAO 单例
pub fn dao() -> std::sync::Arc<dyn FederationContractDao> {
    FEDERATION_CONTRACT_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = FEDERATION_CONTRACT_DAO.set(new());
}

// ==================== 实现 ====================

struct FederationContractDaoSqliteImpl;

#[async_trait::async_trait]
impl FederationContractDao for FederationContractDaoSqliteImpl {
    async fn insert(&self, ctx: RequestContext, contract: &FederationContractPo) -> Result<()> {
        let kind = contract.kind as i32;
        let state = contract.state as i32;
        sqlx::query!(
            "INSERT INTO federation_contracts (id, local_org_id, peer_org_id, kind, state, capabilities, terms_hash, local_signature, peer_signature, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            contract.id,
            contract.local_org_id,
            contract.peer_org_id,
            kind,
            state,
            contract.capabilities,
            contract.terms_hash,
            contract.local_signature,
            contract.peer_signature,
            contract.created_at,
            contract.updated_at
        )
        .execute(ctx.db_pool())
        .await?;
        Ok(())
    }

    async fn find_by_pair(
        &self,
        ctx: RequestContext,
        local_org_id: &str,
        peer_org_id: &str,
    ) -> Result<Option<FederationContractPo>> {
        let contract = sqlx::query_as!(
            FederationContractPo,
            r#"
SELECT id, local_org_id, peer_org_id,
       kind as 'kind: FederationContractKind',
       state as 'state: FederationContractState',
       capabilities, terms_hash, local_signature, peer_signature,
       created_at, updated_at
FROM federation_contracts
WHERE kind = 1 AND local_org_id = ? AND peer_org_id = ? LIMIT 1
            "#,
            local_org_id,
            peer_org_id
        )
        .fetch_optional(ctx.db_pool())
        .await?;
        Ok(contract)
    }

    async fn find_active_by_pair(
        &self,
        ctx: RequestContext,
        local_org_id: &str,
        peer_org_id: &str,
    ) -> Result<Option<FederationContractPo>> {
        let contract = sqlx::query_as!(
            FederationContractPo,
            r#"
SELECT id, local_org_id, peer_org_id,
       kind as 'kind: FederationContractKind',
       state as 'state: FederationContractState',
       capabilities, terms_hash, local_signature, peer_signature,
       created_at, updated_at
FROM federation_contracts
WHERE kind = 1 AND local_org_id = ? AND peer_org_id = ? AND state = 1 LIMIT 1
            "#,
            local_org_id,
            peer_org_id
        )
        .fetch_optional(ctx.db_pool())
        .await?;
        Ok(contract)
    }

    async fn list_by_org(
        &self,
        ctx: RequestContext,
        local_org_id: &str,
    ) -> Result<Vec<FederationContractPo>> {
        let contracts = sqlx::query_as!(
            FederationContractPo,
            r#"
SELECT id, local_org_id, peer_org_id,
       kind as 'kind: FederationContractKind',
       state as 'state: FederationContractState',
       capabilities, terms_hash, local_signature, peer_signature,
       created_at, updated_at
FROM federation_contracts
WHERE kind = 1 AND local_org_id = ? ORDER BY created_at DESC
            "#,
            local_org_id
        )
        .fetch_all(ctx.db_pool())
        .await?;
        Ok(contracts)
    }

    async fn find_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<FederationContractPo>> {
        let contract = sqlx::query_as!(
            FederationContractPo,
            r#"
SELECT id, local_org_id, peer_org_id,
       kind as 'kind: FederationContractKind',
       state as 'state: FederationContractState',
       capabilities, terms_hash, local_signature, peer_signature,
       created_at, updated_at
FROM federation_contracts
WHERE kind = 1 AND id = ? LIMIT 1
            "#,
            id
        )
        .fetch_optional(ctx.db_pool())
        .await?;
        Ok(contract)
    }

    async fn update(&self, ctx: RequestContext, contract: &FederationContractPo) -> Result<()> {
        let kind = contract.kind as i32;
        let state = contract.state as i32;
        sqlx::query!(
            "UPDATE federation_contracts SET kind = ?, state = ?, capabilities = ?, terms_hash = ?, local_signature = ?, peer_signature = ?, updated_at = ? WHERE id = ?",
            kind,
            state,
            contract.capabilities,
            contract.terms_hash,
            contract.local_signature,
            contract.peer_signature,
            contract.updated_at,
            contract.id
        )
        .execute(ctx.db_pool())
        .await?;
        Ok(())
    }
}
