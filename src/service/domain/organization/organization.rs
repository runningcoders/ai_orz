//! Organization 组织设置领域
//!
//! 包含组织设置相关功能：
//! - 检查系统是否已初始化
//! - 初始化系统（创建第一个组织和超级管理员
//! - 获取组织信息
//! - 更新组织信息
//! - 删除组织

use crate::error::AppError;
use crate::models::organization::OrganizationPo;
use crate::pkg::RequestContext;
use crate::service::dal::organization;
use std::sync::Arc;

/// Organization 领域接口
pub trait OrganizationDomain: Send + Sync {
    /// 检查系统是否已经初始化
    fn check_initialized(&self, ctx: RequestContext) -> Result<bool, AppError>;

    /// 初始化系统：创建第一个组织和第一个超级管理员用户
    ///
    /// 返回: (organization_id, user_id)
    fn initialize_system(
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
    fn get_by_id(&self, ctx: RequestContext, org_id: &str) -> Result<Option<OrganizationPo>, AppError>;

    /// 获取所有组织列表
    fn list_all(&self, ctx: RequestContext) -> Result<Vec<OrganizationPo>, AppError>;

    /// 更新组织信息
    fn update(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<(), AppError>;

    /// 删除组织（软删除）
    fn delete(&self, ctx: RequestContext, org_id: &str) -> Result<(), AppError>;

    /// 统计组织总数
    fn count_organizations(&self, ctx: RequestContext) -> Result<u64, AppError>;
}

/// Organization 领域实现
pub struct OrganizationDomainImpl {
    dal: Arc<dyn organization::OrganizationDalTrait + Send + Sync>,
}

impl OrganizationDomainImpl {
    /// 创建 domain 实例
    pub fn new(dal: Arc<dyn organization::OrganizationDalTrait + Send + Sync>) -> Self {
        Self { dal }
    }
}

impl OrganizationDomain for OrganizationDomainImpl {
    fn check_initialized(&self, ctx: RequestContext) -> Result<bool, AppError> {
        self.dal.is_initialized(ctx)
    }

    fn initialize_system(
        &self,
        ctx: RequestContext,
        organization_name: String,
        description: Option<String>,
        username: String,
        password_hash: String,
        display_name: Option<String>,
        email: Option<String>,
    ) -> Result<(String, String), AppError> {
        self.dal.initialize_system(
            ctx,
            organization_name,
            description,
            username,
            password_hash,
            display_name,
            email,
        )
    }

    fn get_by_id(&self, ctx: RequestContext, org_id: &str) -> Result<Option<OrganizationPo>, AppError> {
        self.dal.get_by_id(ctx, org_id)
    }

    fn list_all(&self, ctx: RequestContext) -> Result<Vec<OrganizationPo>, AppError> {
        self.dal.list_all(ctx)
    }

    fn update(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<(), AppError> {
        self.dal.update(ctx, org)
    }

    fn delete(&self, ctx: RequestContext, org_id: &str) -> Result<(), AppError> {
        self.dal.delete(ctx, org_id)
    }

    fn count_organizations(&self, ctx: RequestContext) -> Result<u64, AppError> {
        self.dal.count_organizations(ctx)
    }
}
