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

    /// 按对端 DID 查 Active 连接（机器侧端点鉴权，S2 联邦签名）
    ///
    /// 对端调用本节点时携带 `X-Federation-Key-Id`（其组织 DID），据此查
    /// `peer_did`（建联时交换）；仅匹配 Active 连接。未命中统一返回 None
    /// （上层 401，防枚举）。
    async fn find_active_by_peer_did(
        &self,
        ctx: RequestContext,
        peer_did: &str,
    ) -> Result<Option<OrganizationLinkPo>>;

    /// 通用查询
    async fn query(
        &self,
        ctx: RequestContext,
        query: OrganizationLinkQuery,
    ) -> Result<Vec<OrganizationLinkPo>>;

    /// 全量更新（endpoint / 状态），建联续联复用
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
pub mod ws;
pub use self::sqlite::{dao, init, new};

#[cfg(test)]
mod sqlite_test;
