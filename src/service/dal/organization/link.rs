//! 组织连接 DAL——links CRUD + 联邦出站 HTTP
//!
//! 组合 `OrganizationLinkDao`（organization_links 表）与 `FederationHttpClient`
//! （对端联邦端点出站调用），供 Organization Domain 消费：
//! Domain 不再直捅 DAO / HTTP 客户端。

use crate::models::organization_link::OrganizationLinkPo;
use crate::pkg::RequestContext;
use crate::service::dao::organization_link::http::FederationHttpClient;
use crate::service::dao::organization_link::{OrganizationLinkDao, OrganizationLinkQuery};
use common::error::Result;
use std::sync::{Arc, OnceLock};

// ==================== 单例管理 ====================

static LINK_DAL: OnceLock<Arc<dyn OrganizationLinkDal + Send + Sync>> = OnceLock::new();

/// 获取组织连接 DAL 单例
pub fn dal() -> Arc<dyn OrganizationLinkDal + Send + Sync> {
    LINK_DAL.get().cloned().unwrap()
}

/// 初始化组织连接 DAL 单例
pub fn init() {
    let _ = LINK_DAL.set(new(
        crate::service::dao::organization_link::dao(),
        crate::service::dao::organization_link::http::client(),
    ));
}

/// 创建组织连接 DAL（返回 trait 对象，测试可注入 mock HTTP 客户端）
pub fn new(
    link_dao: Arc<dyn OrganizationLinkDao + Send + Sync>,
    http_client: Arc<dyn FederationHttpClient>,
) -> Arc<dyn OrganizationLinkDal + Send + Sync> {
    Arc::new(OrganizationLinkDalImpl {
        link_dao,
        http_client,
    })
}

// ==================== DAL 接口 ====================

/// 组织连接 DAL 接口
#[async_trait::async_trait]
pub trait OrganizationLinkDal: Send + Sync {
    // ---------- 连接持久化（organization_links 表）----------

    /// 按 (local_org_id, peer_org_id) 查连接（幂等续联的存在性预检）
    async fn find_by_pair(
        &self,
        ctx: RequestContext,
        local_org_id: &str,
        peer_org_id: &str,
    ) -> Result<Option<OrganizationLinkPo>>;

    /// 插入连接
    async fn insert(&self, ctx: RequestContext, link: &OrganizationLinkPo) -> Result<()>;

    /// 更新连接
    async fn update(&self, ctx: RequestContext, link: &OrganizationLinkPo) -> Result<()>;

    /// 通用查询
    async fn query(
        &self,
        ctx: RequestContext,
        query: OrganizationLinkQuery,
    ) -> Result<Vec<OrganizationLinkPo>>;

    /// 按对端 DID 查 Active 连接（机器侧入站鉴权，S2 联邦签名）
    async fn find_active_by_peer_did(
        &self,
        ctx: RequestContext,
        peer_did: &str,
    ) -> Result<Option<OrganizationLinkPo>>;

    // ---------- 联邦出站（对端端点 HTTP）----------

    /// 出站调对端 verify：验证配对码 + 交换凭证
    async fn verify_pairing_code(
        &self,
        peer_endpoint: &str,
        req: &common::api::VerifyPairingCodeRequest,
    ) -> Result<common::api::VerifyPairingCodeResponse>;

    /// 拉取对端组织目录（联邦签名鉴权，signing_key 为本端联邦私钥明文 base64）
    async fn fetch_directory(
        &self,
        peer_endpoint: &str,
        signing_key: &str,
    ) -> Result<Vec<common::api::PeerOrgDirectoryEntry>>;

    /// 推送本地目录给对端（联邦签名鉴权）
    async fn push_directory(
        &self,
        peer_endpoint: &str,
        signing_key: &str,
        orgs: Vec<common::api::PeerOrgDirectoryEntry>,
    ) -> Result<()>;

    /// 拉取对端能力清单（联邦签名鉴权，P5 联邦 Agent 目录用）
    async fn fetch_capabilities(
        &self,
        peer_endpoint: &str,
        signing_key: &str,
    ) -> Result<common::api::CapabilitiesResponse>;
}

// ==================== DAL 实现 ====================

/// 组织连接 DAL 实现
struct OrganizationLinkDalImpl {
    /// 连接 DAO（organization_links 表，私有）
    link_dao: Arc<dyn OrganizationLinkDao + Send + Sync>,
    /// 联邦出站 HTTP 客户端（对端机器端点，私有）
    http_client: Arc<dyn FederationHttpClient>,
}

#[async_trait::async_trait]
impl OrganizationLinkDal for OrganizationLinkDalImpl {
    async fn find_by_pair(
        &self,
        ctx: RequestContext,
        local_org_id: &str,
        peer_org_id: &str,
    ) -> Result<Option<OrganizationLinkPo>> {
        self.link_dao
            .find_by_pair(ctx, local_org_id, peer_org_id)
            .await
    }

    async fn insert(&self, ctx: RequestContext, link: &OrganizationLinkPo) -> Result<()> {
        self.link_dao.insert(ctx, link).await
    }

    async fn update(&self, ctx: RequestContext, link: &OrganizationLinkPo) -> Result<()> {
        self.link_dao.update(ctx, link).await
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: OrganizationLinkQuery,
    ) -> Result<Vec<OrganizationLinkPo>> {
        self.link_dao.query(ctx, query).await
    }

    async fn find_active_by_peer_did(
        &self,
        ctx: RequestContext,
        peer_did: &str,
    ) -> Result<Option<OrganizationLinkPo>> {
        self.link_dao.find_active_by_peer_did(ctx, peer_did).await
    }

    async fn verify_pairing_code(
        &self,
        peer_endpoint: &str,
        req: &common::api::VerifyPairingCodeRequest,
    ) -> Result<common::api::VerifyPairingCodeResponse> {
        self.http_client
            .verify_pairing_code(peer_endpoint, req)
            .await
    }

    async fn fetch_directory(
        &self,
        peer_endpoint: &str,
        signing_key: &str,
    ) -> Result<Vec<common::api::PeerOrgDirectoryEntry>> {
        self.http_client
            .fetch_directory(peer_endpoint, signing_key)
            .await
    }

    async fn push_directory(
        &self,
        peer_endpoint: &str,
        signing_key: &str,
        orgs: Vec<common::api::PeerOrgDirectoryEntry>,
    ) -> Result<()> {
        self.http_client
            .push_directory(peer_endpoint, signing_key, orgs)
            .await
    }

    async fn fetch_capabilities(
        &self,
        peer_endpoint: &str,
        signing_key: &str,
    ) -> Result<common::api::CapabilitiesResponse> {
        self.http_client
            .fetch_capabilities(peer_endpoint, signing_key)
            .await
    }
}
