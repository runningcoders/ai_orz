//! 组织管理 trait 实现
//!
//! 定义组织相关业务接口实现

use crate::error::AppError;
use crate::models::organization::OrganizationPo;
use crate::pkg::RequestContext;
use async_trait::async_trait;

#[async_trait]
impl super::OrganizationManage for super::OrganizationDomainImpl {
    /// 检查系统是否已经初始化
    async fn check_initialized(&self, ctx: RequestContext) -> Result<bool, AppError> {
        let count = self.dal.count_organizations(ctx).await?;
        Ok(count > 0)
    }

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
    ) -> Result<(String, String), AppError> {
        self.dal.initialize_system(
            ctx,
            organization_name,
            description,
            username,
            password_hash,
            display_name,
            email,
        ).await
    }

    /// 获取组织信息
    async fn get_by_id(&self, ctx: RequestContext, org_id: &str) -> Result<Option<OrganizationPo>, AppError> {
        self.dal.get_by_id(ctx, org_id).await
    }

    /// 获取所有组织列表
    async fn list_all(&self, ctx: RequestContext) -> Result<Vec<OrganizationPo>, AppError> {
        self.dal.list_all(ctx).await
    }

    /// 更新组织信息
    async fn update(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<(), AppError> {
        self.dal.update(ctx, org).await
    }

    /// 删除组织（软删除）
    async fn delete(&self, ctx: RequestContext, org_id: &str) -> Result<(), AppError> {
        self.dal.delete(ctx, org_id).await
    }

    /// 统计组织总数
    async fn count_organizations(&self, ctx: RequestContext) -> Result<u64, AppError> {
        self.dal.count_organizations(ctx).await
    }
}
