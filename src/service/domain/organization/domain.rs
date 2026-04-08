//! Organization domain 领域接口
//!
//! 组织管理领域：包含组织设置管理和用户管理

use crate::error::AppError;
use crate::models::organization::OrganizationPo;
use crate::pkg::RequestContext;
use crate::service::dal::organization;
use std::sync::Arc;

/// Organization domain 接口
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

/// Organization domain 实现
pub struct OrganizationDomainImpl {
    organization_dal: Arc<dyn organization::OrganizationDalTrait + Send + Sync>,
}

impl OrganizationDomainImpl {
    /// 创建 domain 实例
    pub fn new(dal: Arc<dyn organization::OrganizationDalTrait + Send + Sync>) -> Self {
        Self {
            organization_dal: dal,
        }
    }
}

impl OrganizationDomain for OrganizationDomainImpl {
    fn check_initialized(&self, ctx: RequestContext) -> Result<bool, AppError> {
        self.organization_dal.is_initialized(ctx)
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
        self.organization_dal.initialize_system(
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
        self.organization_dal.get_by_id(ctx, org_id)
    }

    fn list_all(&self, ctx: RequestContext) -> Result<Vec<OrganizationPo>, AppError> {
        self.organization_dal.list_all(ctx)
    }

    fn update(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<(), AppError> {
        self.organization_dal.update(ctx, org)
    }

    fn delete(&self, ctx: RequestContext, org_id: &str) -> Result<(), AppError> {
        self.organization_dal.delete(ctx, org_id)
    }

    fn count_organizations(&self, ctx: RequestContext) -> Result<u64, AppError> {
        self.organization_dal.count_organizations(ctx)
    }
}