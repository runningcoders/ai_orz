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
use crate::service::dal::user as user_dal;
use crate::service::dao::organization_link;
use crate::service::dao::organization_link::http::FederationHttpClient;
use crate::service::dao::organization_pairing;
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

/// 初始化 Organization Domain
pub fn init() {
    let domain = OrganizationDomainImpl::new(
        organization::dal(),
        user_dal::dal(),
        organization_link::dao(),
        organization_pairing::dao(),
        organization_link::http::client(),
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
    link_dao: Arc<dyn organization_link::OrganizationLinkDao + Send + Sync>,
    pairing_dao: Arc<dyn organization_pairing::OrganizationPairingDao + Send + Sync>,
    http_client: Arc<dyn FederationHttpClient>,
}

impl OrganizationDomainImpl {
    /// 创建 Domain 实例
    #[allow(clippy::too_many_arguments)]
    fn new(
        org_dal: Arc<dyn organization::OrganizationDal + Send + Sync>,
        user_dal: Arc<dyn user_dal::UserDal + Send + Sync>,
        link_dao: Arc<dyn organization_link::OrganizationLinkDao + Send + Sync>,
        pairing_dao: Arc<dyn organization_pairing::OrganizationPairingDao + Send + Sync>,
        http_client: Arc<dyn FederationHttpClient>,
    ) -> Self {
        Self {
            org_dal,
            user_dal,
            link_dao,
            pairing_dao,
            http_client,
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
    /// 返回明文 + 过期绝对时间。签发记审计（评审稿 §4.1 / §6.3）。
    async fn issue_pairing_code(
        &self,
        ctx: RequestContext,
    ) -> Result<common::api::IssuePairingCodeResponse>;

    /// 验证配对码 + 交换凭证（机器侧，配对码鉴权）
    ///
    /// 消费配对码（单用途 + TTL），生成对端出站 token，落对端 link + Linked 影子，
    /// 返回对端目录条目 + token。无效 / 过期 / 已用统一返回 unauthorized（防枚举）。
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
