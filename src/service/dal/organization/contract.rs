//! 联邦合约 DAL——能力白名单的唯一事实源（S3 合约授权）
//!
//! 组合 `FederationContractDao`（federation_contracts 表），供 Organization
//! Domain 消费：能力门禁、建联自动建约、管理员编辑都从这里走。

use crate::models::federation_contract::FederationContractPo;
use crate::pkg::RequestContext;
use crate::service::dao::federation_contract::FederationContractDao;
use common::constants::utils;
use common::enums::FederationContractState;
use common::error::{Error, Result};
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static CONTRACT_DAL: OnceLock<Arc<dyn FederationContractDal + Send + Sync>> = OnceLock::new();

/// 获取联邦合约 DAL 单例
pub fn dal() -> Arc<dyn FederationContractDal + Send + Sync> {
    CONTRACT_DAL.get().cloned().unwrap()
}

/// 初始化联邦合约 DAL 单例
pub fn init() {
    let _ = CONTRACT_DAL.set(new(crate::service::dao::federation_contract::dao()));
}

/// 创建联邦合约 DAL（返回 trait 对象，测试可注入 mock DAO）
pub fn new(
    contract_dao: Arc<dyn FederationContractDao + Send + Sync>,
) -> Arc<dyn FederationContractDal + Send + Sync> {
    Arc::new(FederationContractDalImpl { contract_dao })
}

// ==================== DAL 接口 ====================

/// 联邦合约 DAL 接口
#[async_trait::async_trait]
pub trait FederationContractDal: Send + Sync {
    /// 建联后确保存在 basic 合约（幂等；已存在则不动，保留管理员编辑过的能力集）
    async fn ensure_basic(
        &self,
        ctx: RequestContext,
        local_org_id: &str,
        peer_org_id: &str,
    ) -> Result<()>;

    /// 该组织对某对端的 active 合约能力集
    ///
    /// 无 active 合约 = 空集（fail-closed 由调用方按空集拒绝）。
    async fn active_capabilities(
        &self,
        ctx: RequestContext,
        local_org_id: &str,
        peer_org_id: &str,
    ) -> Result<Vec<String>>;

    /// 本组织的合约列表（管理页数据源）
    async fn list_by_org(
        &self,
        ctx: RequestContext,
        local_org_id: &str,
    ) -> Result<Vec<FederationContractPo>>;

    /// 更新 active 合约的能力集（属主校验：contract.local_org_id 必须匹配）
    ///
    /// 合约不存在 / 非本组织 / 已终止 → 404 或 400；返回更新后的合约。
    async fn update_capabilities(
        &self,
        ctx: RequestContext,
        local_org_id: &str,
        contract_id: &str,
        capabilities: Vec<String>,
    ) -> Result<FederationContractPo>;

    /// 终止与对端的 basic 合约（幂等；无合约静默成功，断联清理复用）
    async fn terminate(
        &self,
        ctx: RequestContext,
        local_org_id: &str,
        peer_org_id: &str,
    ) -> Result<()>;
}

// ==================== DAL 实现 ====================

struct FederationContractDalImpl {
    contract_dao: Arc<dyn FederationContractDao + Send + Sync>,
}

#[async_trait::async_trait]
impl FederationContractDal for FederationContractDalImpl {
    async fn ensure_basic(
        &self,
        ctx: RequestContext,
        local_org_id: &str,
        peer_org_id: &str,
    ) -> Result<()> {
        if self
            .contract_dao
            .find_by_pair(ctx.clone(), local_org_id, peer_org_id)
            .await?
            .is_some()
        {
            return Ok(());
        }
        let contract = FederationContractPo::new(local_org_id.to_string(), peer_org_id.to_string());
        self.contract_dao.insert(ctx, &contract).await
    }

    async fn active_capabilities(
        &self,
        ctx: RequestContext,
        local_org_id: &str,
        peer_org_id: &str,
    ) -> Result<Vec<String>> {
        Ok(self
            .contract_dao
            .find_active_by_pair(ctx, local_org_id, peer_org_id)
            .await?
            .map(|c| c.capabilities_list())
            .unwrap_or_default())
    }

    async fn list_by_org(
        &self,
        ctx: RequestContext,
        local_org_id: &str,
    ) -> Result<Vec<FederationContractPo>> {
        self.contract_dao.list_by_org(ctx, local_org_id).await
    }

    async fn update_capabilities(
        &self,
        ctx: RequestContext,
        local_org_id: &str,
        contract_id: &str,
        capabilities: Vec<String>,
    ) -> Result<FederationContractPo> {
        let mut contract = self
            .contract_dao
            .find_by_id(ctx.clone(), contract_id)
            .await?
            .ok_or_else(|| Error::not_found(format!("合约 {} 不存在", contract_id)))?;
        if contract.local_org_id != local_org_id {
            return Err(Error::not_found(format!("合约 {} 不存在", contract_id)));
        }
        if contract.state != FederationContractState::Active {
            return Err(Error::bad_request("已终止的合约不可编辑，请先重新建联"));
        }
        contract.capabilities = serde_json::to_string(&capabilities)
            .map_err(|e| Error::internal(format!("能力集序列化失败: {}", e)))?;
        contract.updated_at = utils::current_timestamp_ms();
        self.contract_dao.update(ctx.clone(), &contract).await?;
        Ok(contract)
    }

    async fn terminate(
        &self,
        ctx: RequestContext,
        local_org_id: &str,
        peer_org_id: &str,
    ) -> Result<()> {
        let Some(mut contract) = self
            .contract_dao
            .find_by_pair(ctx.clone(), local_org_id, peer_org_id)
            .await?
        else {
            return Ok(());
        };
        if contract.state == FederationContractState::Active {
            contract.state = FederationContractState::Terminated;
            contract.updated_at = utils::current_timestamp_ms();
            self.contract_dao.update(ctx, &contract).await?;
        }
        Ok(())
    }
}
