//! Organization (组织管理) Domain 模块
//!
//! 组织管理模块，管理：
//! - Organization - 组织信息管理
//! - User - 用户信息管理

pub mod org;
pub mod user;

use crate::models::organization::OrganizationPo;
use crate::pkg::RequestContext;
use crate::service::dal::organization;
use crate::service::dal::organization::{OrganizationLinkDal, OrganizationPairingDal};
use crate::service::dal::user as user_dal;
use async_trait::async_trait;
use common::api::OrganizationConfig;
use common::error::Result;
use std::sync::{Arc, OnceLock};
// ==================== 单例 ====================

static ORGANIZATION_DOMAIN: OnceLock<Arc<dyn OrganizationDomain>> = OnceLock::new();

/// 获取 Organization Domain 单例
pub fn domain() -> Arc<dyn OrganizationDomain> {
    ORGANIZATION_DOMAIN.get().cloned().unwrap()
}

/// S1 密钥底座：启动自检，确保 Local 组织都有联邦身份密钥对（幂等，失败仅告警）
pub async fn init_base_data() {
    let ctx = RequestContext::new_system();
    match org::ensure_local_federation_identity(
        &ctx,
        crate::service::dao::organization::dao().as_ref(),
    )
    .await
    {
        Ok(()) => sys_info!("organization domain 基础数据初始化完成（联邦身份密钥自检）"),
        Err(e) => sys_warn!(
            "organization domain 基础数据初始化失败（联邦身份密钥自检）: {}",
            e
        ),
    }
}

/// 初始化 Organization Domain
pub fn init() {
    // 本端自报联邦地址（P7 多地址模型）：配置启动后不变，组装点一次推导。
    // federation_base_url = 内网/主监听地址（private），public_base_url 配置了
    // 才报（public）。目录导出时随 Local 组织条目自报，供对端探测候选。
    let server = &crate::config::get().server;
    let mut self_addresses = vec![common::api::organization_link::FederationAddress {
        url: server.federation_base_url(),
        scope: common::api::organization_link::ADDRESS_SCOPE_PRIVATE.to_string(),
    }];
    if let Some(public) = server
        .public_base_url
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        self_addresses.push(common::api::organization_link::FederationAddress {
            url: public.to_string(),
            scope: common::api::organization_link::ADDRESS_SCOPE_PUBLIC.to_string(),
        });
    }

    let domain = OrganizationDomainImpl::new(
        organization::dal(),
        user_dal::dal(),
        organization::link::dal(),
        organization::pairing::dal(),
        organization::contract::dal(),
        self_addresses,
    );
    let _ = ORGANIZATION_DOMAIN.set(Arc::new(domain));
}

// ==================== 实现 ====================

/// Organization Domain 实现
///
/// 聚合所有组织管理子功能实现
struct OrganizationDomainImpl {
    org_dal: Arc<dyn organization::OrganizationDal + Send + Sync>,
    user_dal: Arc<dyn user_dal::UserDal + Send + Sync>,
    link_dal: Arc<dyn OrganizationLinkDal + Send + Sync>,
    pairing_dal: Arc<dyn OrganizationPairingDal + Send + Sync>,
    /// 联邦合约 DAL（能力白名单唯一事实源，S3）
    contract_dal: Arc<dyn organization::FederationContractDal + Send + Sync>,
    /// 本端自报联邦地址（P7）：目录导出时随 Local 组织条目携带
    self_addresses: Vec<common::api::organization_link::FederationAddress>,
}

impl OrganizationDomainImpl {
    /// 创建 Domain 实例
    fn new(
        org_dal: Arc<dyn organization::OrganizationDal + Send + Sync>,
        user_dal: Arc<dyn user_dal::UserDal + Send + Sync>,
        link_dal: Arc<dyn OrganizationLinkDal + Send + Sync>,
        pairing_dal: Arc<dyn OrganizationPairingDal + Send + Sync>,
        contract_dal: Arc<dyn organization::FederationContractDal + Send + Sync>,
        self_addresses: Vec<common::api::organization_link::FederationAddress>,
    ) -> Self {
        Self {
            org_dal,
            user_dal,
            link_dal,
            pairing_dal,
            contract_dal,
            self_addresses,
        }
    }
}

impl OrganizationDomain for OrganizationDomainImpl {
    /// 组织管理能力
    fn organization_manage(&self) -> &dyn OrganizationManage {
        self
    }

    /// 用户管理能力
    fn user_manage(&self) -> &dyn UserManage {
        self
    }
}

// ==================== traits 定义 ====================

/// 联邦入站请求签名证明（S2：四个 `X-Federation-*` 头的纯数据形态）
///
/// 跨传输层复用：HTTP 中间件（/a2a）与机器侧 handler（directory 等）及 WS 握手
/// 均从 header 提取后传入 domain；验签逻辑全部收敛在
/// `authenticate_federation_request`，避免两套鉴权实现漂移。
#[derive(Debug, Clone)]
pub struct FederationRequestProof {
    /// 发起方组织 DID（`did:key:z...`，与 link.peer_did 比对防跨连接冒充）
    pub key_id: String,
    /// Unix 秒级时间戳（±300s 窗口）
    pub timestamp: i64,
    /// 随机 16 字节 hex（进程内去重防重放）
    pub nonce: String,
    /// Ed25519 签名（base64url 无 padding）
    pub signature: String,
}

/// Organization Domain 总 trait
///
/// 聚合组织管理模块所有子功能 trait
pub trait OrganizationDomain: Send + Sync {
    /// 组织管理能力
    fn organization_manage(&self) -> &dyn OrganizationManage;

    /// 用户管理能力
    fn user_manage(&self) -> &dyn UserManage;
}

/// 组织管理 trait
///
/// 定义组织相关的业务接口
#[async_trait]
pub trait OrganizationManage: Send + Sync {
    /// 检查系统是否已经初始化
    async fn check_initialized(&self, ctx: RequestContext) -> Result<bool>;

    /// 创建组织 + Owner（超级管理员角色），不含 ModelProvider
    ///
    /// 通用方法：可用于系统初始化，也可用于后续创建新组织。
    /// 返回 (organization_id, user_id)
    /// ModelProvider 的创建由 handler 编排 finance domain 完成
    async fn create_org_and_owner(
        &self,
        ctx: RequestContext,
        params: common::api::InitializeSystemRequest,
    ) -> Result<(String, String)>;

    /// 获取组织信息
    async fn get_by_id(&self, ctx: RequestContext, org_id: &str) -> Result<Option<OrganizationPo>>;

    /// 通用综合查询
    ///
    /// 支持组合查询条件，所有字段都是 Option
    async fn query(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::organization::OrganizationQuery,
    ) -> Result<Vec<OrganizationPo>>;

    /// 获取所有组织列表
    async fn list_all(&self, ctx: RequestContext) -> Result<Vec<OrganizationPo>>;

    /// 根据邀请码获取组织（公开注册用，仅返回未删除的有效组织）
    async fn find_org_by_invite_code(
        &self,
        ctx: RequestContext,
        invite_code: &str,
    ) -> Result<Option<OrganizationPo>>;

    /// 读取组织级配置（透传 DAL → DAO，带缓存）
    async fn get_org_config(&self, ctx: RequestContext, org_id: &str)
    -> Result<OrganizationConfig>;

    /// 写入组织级配置（透传 DAL → DAO，写穿缓存）
    async fn update_org_config(
        &self,
        ctx: RequestContext,
        org_id: &str,
        config: &OrganizationConfig,
    ) -> Result<()>;

    /// 更新组织信息
    async fn update(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<()>;

    /// 删除组织（软删除）
    async fn delete(&self, ctx: RequestContext, org_id: &str) -> Result<()>;

    /// 统计符合查询条件的组织数量（透传 DAL count）
    async fn count_organizations(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::organization::OrganizationQuery,
    ) -> Result<u64>;

    /// 签发组网配对码（用户侧，需管理员权限）
    ///
    /// 生成 24 字符配对码（去 0/O/1/I）、10 分钟 TTL、单用途；仅存哈希，
    /// 返回明文 + 过期绝对时间。可选钉住预期对端 DID（§2.1）。签发记审计。
    async fn issue_pairing_code(
        &self,
        ctx: RequestContext,
        expected_peer_did: Option<String>,
    ) -> Result<common::api::IssuePairingCodeResponse>;

    /// 验证配对码 + 交换身份（机器侧，配对码鉴权）
    ///
    /// 消费配对码（单用途 + TTL），验调用方交叉签名，落对端 link（含对端
    /// DID / 公钥）+ Linked 影子，返回对端目录条目 + 本端交叉签名。
    /// 无效 / 过期 / 已用 / 签名不符统一返回 unauthorized（防枚举）。
    async fn verify_pairing_code(
        &self,
        ctx: RequestContext,
        req: common::api::VerifyPairingCodeRequest,
    ) -> Result<common::api::VerifyPairingCodeResponse>;

    /// 发起建联（用户侧，JWT）
    ///
    /// 凭对端配对码出站调对端 verify 完成双向凭证交换，落本地 link + Linked 影子。
    /// `local_endpoint` 为本端联邦地址（adapter 层从配置解析后传入）。
    async fn create_link(
        &self,
        ctx: RequestContext,
        req: common::api::CreateLinkRequest,
        local_endpoint: String,
    ) -> Result<common::api::CreateLinkResponse>;

    /// 已建联列表（用户侧，JWT，前端"关联组织"页数据源）
    async fn list_links(&self, ctx: RequestContext) -> Result<common::api::ListLinksResponse>;

    /// 联邦入站请求验签（S2，机器侧端点统一鉴权入口）
    ///
    /// 流程（任一环节失败统一 401，无回退路径）：按 `key_id` 查 Active 连接
    /// → 时间窗 ±300s → nonce 去重 → 用连接上的对端公钥验签规范串
    /// `{method}\n{path}\n{timestamp}\n{nonce}\n{sha256(body)}`。
    /// 返回命中的连接（携带对端 org id 与本端 org id）。
    async fn authenticate_federation_request(
        &self,
        ctx: RequestContext,
        proof: &FederationRequestProof,
        method: &str,
        path_with_query: &str,
        body_hash: &str,
    ) -> Result<crate::models::organization_link::OrganizationLinkPo>;

    /// 验证异步回调任务令牌（S2 自签发，`/a2a/callback` 唯一鉴权手段）
    ///
    /// 用**本端**公钥验签（签发方 = 本端 = 回调接收方，零状态）+ exp 未过期 +
    /// `task_id` claim 与路径一致 + `aud` 为本端 DID；任一不符即 401。
    async fn verify_callback_task_token(
        &self,
        ctx: RequestContext,
        token: &str,
        task_id: &str,
    ) -> Result<()>;

    /// 本节点组织目录（机器侧 GET /directory 数据源，白名单字段，评审稿 §5.1）
    async fn get_directory(
        &self,
        ctx: RequestContext,
    ) -> Result<Vec<common::api::PeerOrgDirectoryEntry>>;

    /// 接收对端推送的目录（机器侧 POST /directory/sync）
    ///
    /// 逐条走 org DAL 静默方法 `upsert_remote_shadow`（Remote 影子语义：新者胜、
    /// 不动既有 scope、保护 Local 组织），返回实际写入条数供审计。
    async fn handle_directory_sync(
        &self,
        ctx: RequestContext,
        req: common::api::DirectorySyncRequest,
    ) -> Result<usize>;

    /// 断联（用户侧，JWT，本端管理员）
    ///
    /// 置连接 Revoked + org DAL 组合方法将对端影子 Linked → Remote（不删除记录，
    /// 保留审计线索）。断联后对端出站调用本节点时凭证鉴权失败（401 → 惰性感知）。
    async fn revoke_link(&self, ctx: RequestContext, peer_org_id: &str) -> Result<()>;

    // ==================== 联邦合约（S3 合约授权，用户侧管理员接口）==========

    /// 本组织的合约列表（管理员"联邦合约"数据源）
    async fn list_contracts(
        &self,
        ctx: RequestContext,
    ) -> Result<common::api::ListContractsResponse>;

    /// 更新合约能力集（管理员；下一次入站请求即按新能力集判定）
    async fn update_contract_capabilities(
        &self,
        ctx: RequestContext,
        req: common::api::UpdateContractCapabilitiesRequest,
    ) -> Result<common::api::UpdateContractCapabilitiesResponse>;

    /// 终止合约（管理员；fail-closed 熔断开关，不删记录保留审计线索）
    async fn terminate_contract(
        &self,
        ctx: RequestContext,
        req: common::api::TerminateContractRequest,
    ) -> Result<common::api::TerminateContractResponse>;

    /// 连接的能力集（机器侧门禁数据源；无 active 合约 = 空集，fail-closed）
    async fn contract_capabilities(
        &self,
        ctx: RequestContext,
        local_org_id: &str,
        peer_org_id: &str,
    ) -> Result<Vec<String>>;

    /// 跨组织 Agent 委派（P4）：路由决策 + 联邦 A2A 出站
    ///
    /// 返回：`Ok(Some(reply))` = 对端回复文本；`Ok(None)` = 对端不可路由
    /// （无 Active 连接或连接未开放 `a2a_task` 能力），调用方降级为普通提及
    /// （仅上下文注入）；`Err` = 已建联但调用失败（网络 / 对端执行错误）。
    async fn delegate_agent_task(
        &self,
        ctx: RequestContext,
        peer_org_id: &str,
        peer_agent_id: &str,
        prompt: &str,
        caller_user: Option<String>,
    ) -> Result<Option<String>>;

    /// 联邦 Agent 目录（P5）：聚合所有 Active 连接对端开放的可调用 Agent
    ///
    /// 实时向各对端拉 capabilities（连接级凭证鉴权），单个对端失败仅记 WARN
    /// 跳过（部分可用优于整体为空）。mention picker 候选数据源。
    async fn list_federation_agents(
        &self,
        ctx: RequestContext,
    ) -> Result<common::api::ListFederationAgentsResponse>;

    /// 变更推送：本地目录全量推给所有 Active 对端（best-effort）
    ///
    /// 由 `organization.changed` 事件消费者调用（组织元信息变更后异步触发）。
    /// 全量推送 + 对端 `updated_at` 新者胜，天然幂等；单个对端失败仅记 WARN，
    /// 由下一次 cron 对账补齐。返回推送成功的对端数。
    async fn push_directory_to_peers(&self, ctx: RequestContext) -> Result<usize>;

    /// 定时对账：逐 Active 对端双向同步（推本地目录 + 拉对端目录）
    ///
    /// 由 cron 触发（`directory_reconcile` action），兜底推送丢失/失败的场景，
    /// 保证目录最终一致。只 upsert 不删除（网络抖动不误删本地影子）。
    async fn reconcile_directories(&self, ctx: RequestContext) -> Result<DirectoryReconcileReport>;
}

/// 目录对账报告（审计用）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirectoryReconcileReport {
    /// 参与对账的 Active 对端数（按 endpoint 去重）
    pub peers: usize,
    /// 推送成功的对端数
    pub pushed: usize,
    /// 从对端拉取后实际写入的影子条数
    pub pulled_written: usize,
}

/// 用户管理 trait
///
/// 定义用户相关的业务接口
#[async_trait]
pub trait UserManage: Send + Sync {
    /// 根据用户名查询用户（用于登录）
    async fn find_by_username(
        &self,
        ctx: RequestContext,
        username: &str,
    ) -> Result<Option<crate::models::user::UserPo>>;

    /// 通用综合查询
    ///
    /// 支持组合查询条件，所有字段都是 Option
    async fn query(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::user::UserQuery,
    ) -> Result<common::api::PagedResult<crate::models::user::UserPo>>;

    /// 根据组织 ID 查询所有用户
    async fn find_by_organization_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<Vec<crate::models::user::UserPo>>;

    /// 获取组织接待用户（联邦访客的内部对接身份，P6）
    ///
    /// 外部组织的请求不由请求侧指定服务者——由被访组织决定谁来接待。
    /// 当前策略：组织内权限最高的管理员（创建者，凭据最全），user 表
    /// 不加新字段；后续引入专用接待用户配置时只改实现，调用方零感知。
    /// 联邦请求以该用户身份落地后，project/消息/权限与本地用户路径完全同构。
    async fn reception_user(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<crate::models::user::UserPo>;

    /// 创建新用户
    async fn create_user(
        &self,
        ctx: RequestContext,
        user: crate::models::user::UserPo,
    ) -> Result<()>;

    /// 更新用户信息
    async fn update_user(
        &self,
        ctx: RequestContext,
        user: &crate::models::user::UserPo,
    ) -> Result<()>;

    /// 删除用户（软删除）
    async fn delete_user(&self, ctx: RequestContext, user_id: &str) -> Result<()>;

    /// 检查用户名是否已存在
    async fn exists_by_username(&self, ctx: RequestContext, username: &str) -> Result<bool>;

    /// 统计组织下用户总数
    async fn count_by_organization_id(&self, ctx: RequestContext, org_id: &str) -> Result<u64>;

    /// 统计符合查询条件的用户数量（透传 DAL count）
    async fn count_users(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::user::UserQuery,
    ) -> Result<u64>;

    /// 验证用户名密码（用于登录）
    /// 返回用户信息，如果验证成功
    async fn verify_password(
        &self,
        ctx: RequestContext,
        org_id: &str,
        username: &str,
        password_hash: &str,
    ) -> Result<crate::models::user::UserPo>;

    /// 根据用户 ID 获取用户信息
    async fn get_user_by_id(
        &self,
        ctx: RequestContext,
        user_id: &str,
    ) -> Result<Option<crate::models::user::UserPo>>;

    /// 邀请码注册新成员（公开接口）
    ///
    /// 业务规则全部收敛在 Domain 层：
    /// - 邀请码归一化与有效性校验
    /// - 用户名非空 / 全局唯一预检
    /// - 密码最小长度校验
    /// 返回创建成功的用户（含生成的 ID 与固定 Member 角色），供 handler 签发 JWT
    async fn register_member(
        &self,
        ctx: RequestContext,
        req: common::api::RegisterByInviteRequest,
    ) -> Result<crate::models::user::UserPo>;
}
