//! Organization DAL——总 trait + 单例管理
//!
//! 按消费面拆分为多个子模块（学习 `agent/` / `lark/` 目录模式）：
//!
//! - **本文件（mod.rs）**：[`OrganizationDal`] 总 trait + 单例管理
//! - [`r#impl`]：`OrganizationDalImpl` 实现（组织 CRUD / 影子同步 / 联邦委派传输）
//! - [`link`]：组织连接 DAL（[`OrganizationLinkDal`]，links CRUD + 联邦出站 HTTP）
//! - [`pairing`]：组网配对码 DAL（[`OrganizationPairingDal`]）
//!
//! 职责：Organization 领域的数据访问层。User 相关操作已移至 User DAL，
//! 跨领域编排在 Domain 层完成。

mod r#impl;
pub mod link;
pub mod pairing;

pub use link::OrganizationLinkDal;
pub use pairing::OrganizationPairingDal;

use crate::models::organization::OrganizationPo;
use crate::models::organization_link::OrganizationLinkPo;
use crate::pkg::RequestContext;
use crate::service::dao::organization::{OrganizationDao, OrganizationQuery, PeerOrgUpsert};
use common::api::OrganizationConfig;
use common::error::Result;
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static ORGANIZATION_DAL: OnceLock<Arc<dyn OrganizationDal + Send + Sync>> = OnceLock::new();

/// 获取 Organization DAL 单例
pub fn dal() -> Arc<dyn OrganizationDal + Send + Sync> {
    ORGANIZATION_DAL.get().cloned().unwrap()
}

/// 初始化 Organization DAL（含 link / pairing 子 DAL）
pub fn init() {
    link::init();
    pairing::init();
    let _ = ORGANIZATION_DAL.set(new(
        crate::service::dao::organization::dao(),
        crate::service::dao::organization_link::dao(),
    ));
}

/// 创建 Organization DAL（返回 trait 对象）
pub fn new(
    organization_dao: Arc<dyn OrganizationDao + Send + Sync>,
    link_dao: Arc<dyn crate::service::dao::organization_link::OrganizationLinkDao + Send + Sync>,
) -> Arc<dyn OrganizationDal + Send + Sync> {
    Arc::new(r#impl::OrganizationDalImpl {
        organization_dao,
        link_dao,
    })
}

// ==================== DAL 接口 ====================

/// Organization DAL 接口
#[async_trait::async_trait]
pub trait OrganizationDal: Send + Sync {
    /// 检查系统是否已经初始化
    ///
    /// 通过检查 organizations 表是否有记录判断
    async fn is_initialized(&self, ctx: RequestContext) -> Result<bool>;

    /// 根据 ID 获取组织
    async fn get_by_id(&self, ctx: RequestContext, org_id: &str) -> Result<Option<OrganizationPo>>;

    /// 根据邀请码获取组织（仅返回未删除的有效组织）
    async fn find_by_invite_code(
        &self,
        ctx: RequestContext,
        invite_code: &str,
    ) -> Result<Option<OrganizationPo>>;

    /// 读取组织级配置（透传 DAO，带缓存）
    async fn get_org_config(&self, ctx: RequestContext, org_id: &str)
    -> Result<OrganizationConfig>;

    /// 写入组织级配置（透传 DAO，写穿缓存）
    async fn update_org_config(
        &self,
        ctx: RequestContext,
        org_id: &str,
        config: &OrganizationConfig,
    ) -> Result<()>;

    /// 创建组织
    async fn create(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<()>;

    /// 通用综合查询
    ///
    /// 支持组合查询条件，所有字段都是 Option
    async fn query(
        &self,
        ctx: RequestContext,
        query: OrganizationQuery,
    ) -> Result<Vec<OrganizationPo>>;

    /// 获取所有组织
    async fn list_all(&self, ctx: RequestContext) -> Result<Vec<OrganizationPo>>;

    /// 更新组织信息
    async fn update(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<()>;

    /// 删除组织（软删除）
    async fn delete(&self, ctx: RequestContext, org_id: &str) -> Result<()>;

    /// 统计组织总数
    async fn count_organizations(&self, ctx: RequestContext) -> Result<u64>;

    /// 统计符合查询条件的组织数量（透传 DAO count）
    async fn count(&self, ctx: RequestContext, query: OrganizationQuery) -> Result<u64>;

    // ==================== 联邦影子（静默写入：不发布 organization.changed）==========
    //
    // organizations 表承载两类数据：业务组织（Local/Linked，上方方法管，写后发事件）
    // 与远端影子（Remote，复制同步所得，下方方法管，静默写入）。
    // 影子同步是**复制，不是业务变更**——不发布事件是刻意的：若发布，
    // 对端推送 → 写影子 → 发事件 → consumer 再推送 → 对端又推回，形成事件风暴。
    // 递归防护是结构性的：影子写入路径不经过事件发布点，不依赖任何运行时开关。
    // 新增副作用（缓存/审计）时：业务方法加在 create/update/delete；影子方法另行评估。

    /// 目录同步所得 Remote 影子 upsert（新者胜 / 不动 scope / 护 Local，评审稿 §5.2）
    ///
    /// 返回是否发生写入（false = 跳过），供上层审计。
    async fn upsert_remote_shadow(&self, ctx: RequestContext, peer: &PeerOrgUpsert)
    -> Result<bool>;

    /// 直接建联的对端影子 upsert（强制 Linked，评审稿 R5 护 Local）
    ///
    /// 返回是否发生写入，供上层审计。
    async fn upsert_linked_shadow(&self, ctx: RequestContext, peer: &PeerOrgUpsert)
    -> Result<bool>;

    /// 断联组合操作：连接置 Revoked + 对端影子 Linked → Remote
    ///
    /// 组合 link DAO（links 表）与 organization DAO（organizations 表）：
    /// 先断链后降影，非同一事务（跨 DAO 不开分布式事务，YAGNI）。两步各自幂等，
    /// 第二步失败时重试本方法即可修复（重放断链无害）。不发布事件（断联是
    /// 链接层状态变更，不改变本地组织元信息，无需触发目录变更推送）。
    async fn revoke_link(
        &self,
        ctx: RequestContext,
        link_id: &str,
        peer_org_id: &str,
    ) -> Result<()>;

    /// 批量读取组织自报联邦地址全集（P7，透传 DAO）
    async fn list_addresses(&self, ctx: RequestContext) -> Result<Vec<(String, String)>>;

    /// 解析对端当前首选可达地址（P7 内外网可达性）
    ///
    /// 组合 organization DAO（对端自报地址候选池）与可达性解析器（内网优先
    /// first-match 探测 + TTL 缓存）。永返回值：对端无自报地址或全不通时
    /// 维持 `link.endpoint` 主地址，出站失败语义与 P7 之前一致。
    async fn resolve_peer_endpoint(&self, ctx: RequestContext, link: &OrganizationLinkPo)
    -> String;

    // ==================== 跨组织联邦调用（P4）==========

    /// 联邦 Agent 委派传输层：经指定连接调对端 A2A 出站（send → 轮询 tasks/get 到终态）
    ///
    /// endpoint / auth_token 取自 link（access_token = 对端所发出站凭证），
    /// 携带可选 `X-Federation-Caller` 声明头（已序列化 JSON）。连接的合法性
    /// （Active / 能力白名单）与路由决策由 domain 层完成，本方法只管传输。
    async fn send_federated_agent_task(
        &self,
        ctx: RequestContext,
        link: &OrganizationLinkPo,
        peer_agent_id: &str,
        prompt: &str,
        caller_declaration: Option<String>,
    ) -> Result<String>;
}
