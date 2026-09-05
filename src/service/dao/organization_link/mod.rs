//! OrganizationLink DAO 模块
//!
//! 组织连接契约（organization_links 表）的读写：连接 CRUD。
//! 实体（organizations）与契约（organization_links）分离，见评审稿 D4。
//! 对端组织影子的写入不在本 DAO——organizations 表属主是 organization DAO，
//! 影子写入走 organization DAL 的静默方法（不发事件，见该文件联邦影子一节）。

use crate::models::organization_link::OrganizationLinkPo;
use crate::pkg::RequestContext;
use common::enums::OrganizationLinkStatus;
use common::error::Result;

/// 连接查询参数
#[derive(Debug, Clone, Default)]
pub struct OrganizationLinkQuery {
    pub local_org_id: Option<String>,
    pub status: Option<OrganizationLinkStatus>,
    pub limit: Option<usize>,
}

/// OrganizationLink DAO 接口
#[async_trait::async_trait]
pub trait OrganizationLinkDao: Send + Sync {
    async fn insert(&self, ctx: RequestContext, link: &OrganizationLinkPo) -> Result<()>;

    async fn find_by_id(&self, ctx: RequestContext, id: &str)
    -> Result<Option<OrganizationLinkPo>>;

    /// 按组织对查连接（唯一约束 (local_org_id, peer_org_id)）
    async fn find_by_pair(
        &self,
        ctx: RequestContext,
        local_org_id: &str,
        peer_org_id: &str,
    ) -> Result<Option<OrganizationLinkPo>>;

    /// 按对端出站凭证哈希查连接（机器侧端点鉴权）
    ///
    /// 对端调用本节点时携带其 access_token（= 本节点为对端生成的 token），
    /// 本节点哈希后查 `peer_token_hash`；仅匹配 Active 连接。
    /// 无效/吊销凭证统一返回 None（防枚举）。
    async fn find_active_by_peer_token_hash(
        &self,
        ctx: RequestContext,
        peer_token_hash: &str,
    ) -> Result<Option<OrganizationLinkPo>>;

    /// 通用查询
    async fn query(
        &self,
        ctx: RequestContext,
        query: OrganizationLinkQuery,
    ) -> Result<Vec<OrganizationLinkPo>>;

    /// 全量更新（endpoint / 凭证 / 状态），建联续联与凭证重置复用
    async fn update(&self, ctx: RequestContext, link: &OrganizationLinkPo) -> Result<()>;

    /// 断联：连接置 Revoked（仅 links 表；幂等，重放无害）
    ///
    /// 对端影子的 Linked → Remote 降级由 organization DAL 的
    /// `revoke_link` 组合方法完成（该表属主在 organization DAO）。
    async fn revoke(&self, ctx: RequestContext, link_id: &str) -> Result<()>;
}

pub mod http;
pub mod resolver;
pub mod sqlite;
pub use self::sqlite::{dao, init, new};

#[cfg(test)]
mod sqlite_test;
