//! OrganizationLink 持久化对象
//!
//! 对应 SQL 建表语句：`migrations/20260904000002_create_organization_links.sql`
//!
//! 连接契约（谁跟谁连、怎么连）与实体（组织）分离：organizations 只描述组织
//! 本身，本表承载点对点连接的 endpoint 与双向凭证。凭证不进 organizations 表
//! （该表被全系统高频 join，混入凭证会放大泄漏面）。

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
    /// 出站凭证：本端调用对端时携带（32 字节随机，hex 明文，对端只存其哈希）
    pub access_token: String,
    /// 入站校验：对端调用本端时携带凭证的 SHA-256 哈希（不存明文）
    pub peer_token_hash: String,
    /// 连接级能力白名单（JSON 字符串数组，如 `["a2a_task"]`；P3）
    ///
    /// 本节点开放给这条连接的能力清单；入站调用按此门禁（白名单外 403）。
    pub capabilities: String,
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
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        local_org_id: String,
        peer_org_id: String,
        endpoint: String,
        access_token: String,
        peer_token_hash: String,
        created_by: String,
    ) -> Self {
        let now = utils::current_timestamp_ms();
        Self {
            id,
            local_org_id,
            peer_org_id,
            endpoint,
            access_token,
            peer_token_hash,
            capabilities: common::constants::utils::DEFAULT_LINK_CAPABILITIES.to_string(),
            status: OrganizationLinkStatus::default(),
            created_by,
            created_at: now,
            updated_at: now,
        }
    }

    /// 解析能力白名单（非法 JSON 回退为空 = 全部拒绝，fail-closed）
    pub fn capabilities_list(&self) -> Vec<String> {
        serde_json::from_str(&self.capabilities).unwrap_or_default()
    }

    /// 是否开放指定能力
    pub fn has_capability(&self, capability: &str) -> bool {
        self.capabilities_list().iter().any(|c| c == capability)
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
