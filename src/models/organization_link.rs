//! OrganizationLink 持久化对象
//!
//! 对应 SQL 建表语句：`migrations/20260904000002_create_organization_links.sql`
//!
//! 连接契约（谁跟谁连、怎么连）与实体（组织）分离：organizations 只描述组织
//! 本身，本表承载点对点连接的 endpoint 与对端身份。S2 起鉴权凭据为「对端
//! DID + 公钥」（建联时交换，每请求 Ed25519 验签），共享密钥 access_token /
//! peer_token_hash 已删除（migrations/20260909000002）。S3 起能力白名单下沉到
//! federation_contracts（能力唯一事实源），本表不再存储 capabilities。

use common::constants::utils;
use common::enums::OrganizationLinkStatus;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// OrganizationLinkPo 持久化对象
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OrganizationLinkPo {
    /// 连接 ID
    pub id: String,
    /// 本端组织 ID
    pub local_org_id: String,
    /// 对端组织 ID（organizations 中 scope=Linked 的影子记录）
    pub peer_org_id: String,
    /// 对端 API 基址（组网通信地址；organizations.base_url 仅用于展示）
    pub endpoint: String,
    /// 对端组织 DID（S2 建联交换公钥后填充；入站验签时与 `X-Federation-Key-Id` 比对）
    pub peer_did: Option<String>,
    /// 对端联邦公钥（Ed25519 32B base64，入站验签依据；缺失 = 旧目录未上报，S2 fail-closed）
    pub peer_verification_key: Option<String>,
    /// 连接状态
    pub status: OrganizationLinkStatus,
    /// 创建人
    pub created_by: String,
    /// 创建时间戳（毫秒）
    pub created_at: i64,
    /// 更新时间戳（毫秒）
    pub updated_at: i64,
}

impl OrganizationLinkPo {
    /// 创建新的 OrganizationLinkPo
    pub fn new(
        id: String,
        local_org_id: String,
        peer_org_id: String,
        endpoint: String,
        created_by: String,
    ) -> Self {
        let now = utils::current_timestamp_ms();
        Self {
            id,
            local_org_id,
            peer_org_id,
            endpoint,
            peer_did: None,
            peer_verification_key: None,
            status: OrganizationLinkStatus::default(),
            created_by,
            created_at: now,
            updated_at: now,
        }
    }
}

impl crate::pkg::request_context::EnrichContext for OrganizationLinkPo {
    fn enrich(
        &self,
        builder: crate::pkg::request_context::RequestContextBuilder,
    ) -> crate::pkg::request_context::RequestContextBuilder {
        builder.organization_id(self.local_org_id.clone())
    }
}
