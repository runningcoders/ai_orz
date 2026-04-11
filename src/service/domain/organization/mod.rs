//! Organization (组织管理) Domain 模块
//!
//! 组织管理模块，管理：
//! - Organization - 组织信息管理
//! - User - 用户信息管理

pub mod org;
pub mod user;

use crate::error::AppError;
use crate::models::organization::OrganizationPo;
use crate::pkg::RequestContext;
use std::sync::{Arc, OnceLock};
use async_trait::async_trait;
use crate::service::dal::organization;
use crate::service::dao::user as user_dao;
// ==================== 单例 ====================

static ORGANIZATION_DOMAIN: OnceLock<Arc<dyn OrganizationDomain>> = OnceLock::new();

/// 获取 Organization Domain 单例
pub fn domain() -> Arc<dyn OrganizationDomain> {
    ORGANIZATION_DOMAIN.get().cloned().unwrap()
}

/// 初始化 Organization Domain
pub fn init() {
    // 先初始化 DAL，DAL 会自动初始化 DAO
    crate::service::dal::organization::init();
    let domain = OrganizationDomainImpl::new(
        organization:: dal(),
        user_dao::dao(),
    );
    let _ = ORGANIZATION_DOMAIN.set(Arc::new(domain));
}

// ==================== 实现 ====================

/// Organization Domain 实现
///
/// 聚合所有组织管理子功能实现
pub struct OrganizationDomainImpl {
    dal: Arc<dyn organization::OrganizationDalTrait + Send + Sync>,
    user_dao: Arc<dyn user_dao::UserDaoTrait + Send + Sync>,
}

impl OrganizationDomainImpl {
    /// 创建 Domain 实例
    pub fn new(
        dal: Arc<dyn organization::OrganizationDalTrait + Send + Sync>,
        user_dao: Arc<dyn user_dao::UserDaoTrait + Send + Sync>,
    ) -> Self {
        Self { dal, user_dao }
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
    async fn check_initialized(&self, ctx: RequestContext) -> Result<bool, AppError>;

    /// 初始化系统：创建第一个组织和第一个超级管理员用户
    ///
    /// 返回: (organization_id, user_id)
    async fn initialize_system(
        &self,
        ctx: RequestContext,
        organization_name: String,
        description: Option<String>,
        username: String,
        password_hash: String,
        display_name: Option<String>,
        email: Option<String>,
    ) -> Result<(String, String), AppError>;

    /// 获取组织信息
    async fn get_by_id(&self, ctx: RequestContext, org_id: &str) -> Result<Option<OrganizationPo>, AppError>;

    /// 获取所有组织列表
    async fn list_all(&self, ctx: RequestContext) -> Result<Vec<OrganizationPo>, AppError>;

    /// 更新组织信息
    async fn update(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<(), AppError>;

    /// 删除组织（软删除）
    async fn delete(&self, ctx: RequestContext, org_id: &str) -> Result<(), AppError>;

    /// 统计组织总数
    async fn count_organizations(&self, ctx: RequestContext) -> Result<u64, AppError>;
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
    ) -> Result<Option<crate::models::user::UserPo>, AppError>;

    /// 根据组织 ID 查询所有用户
    async fn find_by_organization_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<Vec<crate::models::user::UserPo>, AppError>;

    /// 创建新用户
    async fn create_user(
        &self,
        ctx: RequestContext,
        user: crate::models::user::UserPo,
    ) -> Result<(), AppError>;

    /// 更新用户信息
    async fn update_user(
        &self,
        ctx: RequestContext,
        user: &crate::models::user::UserPo,
    ) -> Result<(), AppError>;

    /// 删除用户（软删除）
    async fn delete_user(
        &self,
        ctx: RequestContext,
        user_id: &str,
    ) -> Result<(), AppError>;

    /// 检查用户名是否已存在
    async fn exists_by_username(
        &self,
        ctx: RequestContext,
        username: &str,
    ) -> Result<bool, AppError>;

    /// 统计组织下用户总数
    async fn count_by_organization_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<u64, AppError>;

    /// 验证用户名密码（用于登录）
    /// 返回用户信息，如果验证成功
    async fn verify_password(
        &self,
        ctx: RequestContext,
        org_id: &str,
        username: &str,
        password_hash: &str,
    ) -> Result<crate::models::user::UserPo, AppError>;

    /// 根据用户 ID 获取用户信息
    async fn get_user_by_id(
        &self,
        ctx: RequestContext,
        user_id: &str,
    ) -> Result<Option<crate::models::user::UserPo>, AppError>;
}
