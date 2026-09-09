//! FederationContract 持久化对象
//!
//! 对应 SQL 建表语句：`migrations/20260909000003_federation_contracts.sql`
//!
//! S3 合约授权：连接级能力白名单从 organization_links.capabilities 下沉到此
//! （SSOT 约束——能力由合约派生，不得另行配置）。S3 只立合约的形状：
//! `kind=basic`（建联即自动成立）+ `state=active/terminated`；`terms_hash` /
//! 双签列为未来非互信合同机制的占位（basic 时不校验）。

use common::constants::utils;
use common::enums::{FederationContractKind, FederationContractState};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// FederationContractPo 持久化对象
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct FederationContractPo {
    /// 合约 ID
    pub id: String,
    /// 本端组织 ID（合约属主；管理接口按本端组织隔离）
    pub local_org_id: String,
    /// 对端组织 ID
    pub peer_org_id: String,
    /// 合约类型
    pub kind: FederationContractKind,
    /// 合约状态
    pub state: FederationContractState,
    /// 能力白名单（JSON 字符串数组，如 `["a2a_task"]`）
    ///
    /// 本节点开放给这条连接的能力清单；入站调用按此门禁（白名单外 403）。
    pub capabilities: String,
    /// 条款哈希（basic = 固定常量占位，保证列存在）
    pub terms_hash: String,
    /// 本端签名（basic 留空；未来合同机制双签复用 S2 签名代码）
    pub local_signature: String,
    /// 对端签名（basic 留空）
    pub peer_signature: String,
    /// 创建时间戳（毫秒）
    pub created_at: i64,
    /// 更新时间戳（毫秒）
    pub updated_at: i64,
}

impl FederationContractPo {
    /// 创建新的 basic 合约（state=active，默认能力白名单）
    pub fn new(local_org_id: String, peer_org_id: String) -> Self {
        let now = utils::current_timestamp_ms();
        Self {
            id: uuid::Uuid::now_v7().to_string(),
            local_org_id,
            peer_org_id,
            kind: FederationContractKind::Basic,
            state: FederationContractState::Active,
            capabilities: utils::DEFAULT_LINK_CAPABILITIES.to_string(),
            terms_hash: utils::BASIC_CONTRACT_TERMS_HASH.to_string(),
            local_signature: String::new(),
            peer_signature: String::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// 解析能力白名单（非法 JSON 回退为空 = 全部拒绝，fail-closed）
    pub fn capabilities_list(&self) -> Vec<String> {
        serde_json::from_str(&self.capabilities).unwrap_or_default()
    }

    /// 是否开放指定能力（仅 active 合约承载能力，terminated 由调用方先行拒绝）
    pub fn has_capability(&self, capability: &str) -> bool {
        self.capabilities_list().iter().any(|c| c == capability)
    }
}

impl crate::pkg::request_context::EnrichContext for FederationContractPo {
    fn enrich(
        &self,
        builder: crate::pkg::request_context::RequestContextBuilder,
    ) -> crate::pkg::request_context::RequestContextBuilder {
        builder.organization_id(self.local_org_id.clone())
    }
}
