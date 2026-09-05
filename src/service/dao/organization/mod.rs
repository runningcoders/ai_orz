//! Organization DAO 模块

use crate::models::organization::OrganizationPo;
use crate::pkg::RequestContext;
use common::api::OrganizationConfig;
use common::enums::{OrganizationScope, OrganizationStatus};
use common::error::Result;

/// Organization 查询参数
#[derive(Debug, Clone, Default)]
pub struct OrganizationQuery {
    pub scope: Option<OrganizationScope>,
    pub limit: Option<usize>,
}

/// 对端组织目录条目（联邦影子 upsert 载荷）
///
/// 字段 = 目录同步白名单（评审稿 §5.1）：仅目录元信息，绝不携带业务数据。
/// 由 organization DAL 的静默方法消费（不发布 `organization.changed`——
/// 影子同步是复制，不是业务变更），见 `dal/organization.rs` 联邦影子一节。
#[derive(Debug, Clone)]
pub struct PeerOrgUpsert {
    pub id: String,
    pub name: String,
    pub description: String,
    pub base_url: String,
    pub group_name: Option<String>,
    /// 自报联邦地址全集（P7 多地址模型①层；None = 对端旧版本未上报）
    pub addresses: Option<Vec<common::api::organization_link::FederationAddress>>,
    pub status: OrganizationStatus,
    /// 对端侧 updated_at（毫秒）：新者胜的比较基准
    pub updated_at: i64,
}

/// Organization DAO 接口
#[async_trait::async_trait]
pub trait OrganizationDao: Send + Sync {
    async fn insert(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<()>;
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<OrganizationPo>>;

    /// 根据邀请码查组织（公开注册用，公开路由也能调用）
    async fn find_by_invite_code(
        &self,
        ctx: RequestContext,
        invite_code: &str,
    ) -> Result<Option<OrganizationPo>>;

    /// 读取组织级配置（读穿缓存：命中直接返回，未命中回退 DB 并回填）
    async fn get_org_config(&self, ctx: RequestContext, org_id: &str)
    -> Result<OrganizationConfig>;

    /// 写入组织级配置（写穿缓存：DB 落盘后同步刷新缓存）
    async fn set_org_config(
        &self,
        ctx: RequestContext,
        org_id: &str,
        config: &OrganizationConfig,
    ) -> Result<()>;

    /// 通用查询
    async fn query(
        &self,
        ctx: RequestContext,
        query: OrganizationQuery,
    ) -> Result<Vec<OrganizationPo>>;

    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<OrganizationPo>>;
    async fn update(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<()>;
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;
    async fn count_all(&self, ctx: RequestContext) -> Result<u64>;

    /// 统计符合查询条件的组织数量（复用 query 的 filter 逻辑，只跑 COUNT 不跑 LIST）
    async fn count(&self, ctx: RequestContext, query: OrganizationQuery) -> Result<u64>;

    // ==================== 联邦影子（organizations 表的第二类数据：Remote 复制）===
    //
    // 写入方是 organization DAL 的静默方法（不发布事件，防对端推送 → 写影子 →
    // 发事件 → 再推送的递归风暴）。链接/目录协议知识留在 domain + link DAO，
    // 这里只提供 organizations 表属主范围内的原子写入语义。

    /// 目录同步所得 Remote 影子 upsert
    ///
    /// 写入规则（评审稿 §5.2）：
    /// - 本地不存在 → 插入 `scope=Remote` 影子
    /// - 本地已存在（含 Linked）→ 仅更新目录元信息，**不动 scope**
    /// - 按 `updated_at` 新者胜：对端值不比本地新则跳过
    /// - 本地 `scope=Local` 的组织（本节点自己的组织）**绝不覆盖**（id 撞车防护）
    ///
    /// 返回是否发生了写入（false = 跳过），供上层审计/冲突上报（评审稿 R5）。
    async fn upsert_remote_shadow(&self, ctx: RequestContext, peer: &PeerOrgUpsert)
    -> Result<bool>;

    /// 直接建联的对端影子 upsert
    ///
    /// 与 `upsert_remote_shadow`（目录同步所得 Remote 影子）不同：直接建联必为 `Linked`。
    /// - 本地不存在 → 插入 `scope=Linked` 影子
    /// - 本地已存在（含 Remote/Linked）→ 更新目录元信息并**置 `scope=Linked`**
    /// - 直接建联是权威动作，不依赖 `updated_at` 新者胜（直接相连即认对端当前发布信息）
    /// - 本地 `scope=Local` 的组织（本节点自己的组织）**绝不覆盖**（id 撞车防护，评审稿 R5）
    ///
    /// 返回是否发生了写入，供上层审计。
    async fn upsert_linked_shadow(&self, ctx: RequestContext, peer: &PeerOrgUpsert)
    -> Result<bool>;

    /// 断联降级：对端影子 Linked → Remote（不删除，保留审计线索）
    ///
    /// 只降级 `scope=Linked` 的行（幂等）；不动 `organizations.updated_at`——
    /// 影子行的 updated_at 语义是「对端数据版本」（新者胜比较基准），
    /// 本地投影状态变更不参与该比较。返回是否发生了降级。
    async fn degrade_shadow_to_remote(
        &self,
        ctx: RequestContext,
        peer_org_id: &str,
    ) -> Result<bool>;

    /// 批量读取组织自报联邦地址全集（P7 多地址模型①层）
    ///
    /// 裸 SQL 读取 `organizations.addresses` JSON 列（不进 PO，先例 config 列），
    /// 返回 (org_id, addresses JSON 原文)；`addresses = '[]'` 的行不返回。
    /// 调用方自行反序列化（非法 JSON 由解析器侧容错）。
    async fn list_addresses(&self, ctx: RequestContext) -> Result<Vec<(String, String)>>;
}

pub mod sqlite;
pub use self::sqlite::{dao, init, new};

#[cfg(test)]
mod sqlite_test;
