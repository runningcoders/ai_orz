//! OrganizationLink DAO 模块
//!
//! 组织连接契约的读写：连接 CRUD + 对端目录影子 upsert。
//! 实体（organizations）与契约（organization_links）分离，见评审稿 D4。

use crate::models::organization_link::OrganizationLinkPo;
use crate::pkg::RequestContext;
use common::enums::OrganizationLinkStatus;
use common::enums::OrganizationStatus;
use common::error::Result;

/// 连接查询参数
#[derive(Debug, Clone, Default)]
pub struct OrganizationLinkQuery {
    pub local_org_id: Option<String>,
    pub status: Option<OrganizationLinkStatus>,
    pub limit: Option<usize>,
}

/// 对端组织目录条目（影子 upsert 载荷）
///
/// 字段 = 目录同步白名单（评审稿 §5.1）：仅目录元信息，绝不携带业务数据。
#[derive(Debug, Clone)]
pub struct PeerOrgUpsert {
    pub id: String,
    pub name: String,
    pub description: String,
    pub base_url: String,
    pub group_name: Option<String>,
    pub status: OrganizationStatus,
    /// 对端侧 updated_at（毫秒）：新者胜的比较基准
    pub updated_at: i64,
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

    /// 通用查询
    async fn query(
        &self,
        ctx: RequestContext,
        query: OrganizationLinkQuery,
    ) -> Result<Vec<OrganizationLinkPo>>;

    /// 全量更新（endpoint / 凭证 / 状态），建联续联与凭证重置复用
    async fn update(&self, ctx: RequestContext, link: &OrganizationLinkPo) -> Result<()>;

    /// 断联（事务）：连接置 Revoked + 对端影子记录 Linked → Remote（不删除，保留审计线索）
    async fn revoke(&self, ctx: RequestContext, link_id: &str) -> Result<()>;

    /// 对端组织目录影子 upsert
    ///
    /// 写入规则（评审稿 §5.2）：
    /// - 本地不存在 → 插入 `scope=Remote` 影子
    /// - 本地已存在（含 Linked）→ 仅更新目录元信息，**不动 scope**
    /// - 按 `updated_at` 新者胜：对端值不比本地新则跳过
    /// - 本地 `scope=Local` 的组织（本节点自己的组织）**绝不覆盖**（id 撞车防护）
    ///
    /// 返回是否发生了写入（false = 跳过），供上层审计/冲突上报（评审稿 R5）。
    async fn upsert_peer_org(&self, ctx: RequestContext, peer: &PeerOrgUpsert) -> Result<bool>;
}

pub mod sqlite;
pub use self::sqlite::{dao, init, new};

#[cfg(test)]
mod sqlite_test;
