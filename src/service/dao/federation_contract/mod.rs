//! FederationContract DAO 模块
//!
//! 联邦合约（federation_contracts 表）的读写：能力白名单的唯一事实源（S3）。
//! 合约随连接（organization_links）存在：按 (local_org_id, peer_org_id) 定位，
//! kind 维度 S3 只有 basic，查唯一行；未来 task_sla 合约加 kind 过滤即可。

use crate::models::federation_contract::FederationContractPo;
use crate::pkg::RequestContext;
use common::error::Result;

/// FederationContract DAO 接口
#[async_trait::async_trait]
pub trait FederationContractDao: Send + Sync {
    /// 插入合约（(local_org_id, peer_org_id, kind) 唯一约束，重复插入报错）
    async fn insert(&self, ctx: RequestContext, contract: &FederationContractPo) -> Result<()>;

    /// 按组织对查合约（kind=basic，任意状态；管理接口定位用）
    async fn find_by_pair(
        &self,
        ctx: RequestContext,
        local_org_id: &str,
        peer_org_id: &str,
    ) -> Result<Option<FederationContractPo>>;

    /// 按组织对查 active 合约（入站能力门禁数据源；无 active 合约返回 None = 无能力）
    async fn find_active_by_pair(
        &self,
        ctx: RequestContext,
        local_org_id: &str,
        peer_org_id: &str,
    ) -> Result<Option<FederationContractPo>>;

    /// 本组织的合约列表（管理页数据源，按创建时间倒序）
    async fn list_by_org(
        &self,
        ctx: RequestContext,
        local_org_id: &str,
    ) -> Result<Vec<FederationContractPo>>;

    /// 按 ID 查合约（属主校验由调用方按 local_org_id 完成）
    async fn find_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<FederationContractPo>>;

    /// 全量更新（capabilities / state），能力编辑与终止复用
    async fn update(&self, ctx: RequestContext, contract: &FederationContractPo) -> Result<()>;
}

pub mod sqlite;
pub use self::sqlite::{dao, init, new};

#[cfg(test)]
mod sqlite_test;
