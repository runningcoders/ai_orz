//! Organization DAL 模块
//!
//! 职责：Organization 领域的数据访问层，封装 OrganizationDao 提供统一的查询接口
//! 注意：User 相关操作已移至 User DAL，跨领域编排在 Domain 层完成

use crate::error::AppError;
use crate::models::organization::OrganizationPo;
use crate::pkg::RequestContext;
use crate::service::dao::organization::{OrganizationDao, OrganizationQuery};
use std::sync::{Arc, OnceLock};
use crate::service::dao::organization;

// ==================== 单例管理 ====================

static ORGANIZATION_DAL: OnceLock<Arc<dyn OrganizationDal + Send + Sync>> = OnceLock::new();

/// 获取 Organization DAL 单例
pub fn dal() -> Arc<dyn OrganizationDal + Send + Sync> {
    ORGANIZATION_DAL.get().cloned().unwrap()
}

/// 初始化 Organization DAL
pub fn init() {
    let _ = ORGANIZATION_DAL.set(new(organization::dao()));
}

/// 创建 Organization DAL（返回 trait 对象）
pub fn new(organization_dao: Arc<dyn OrganizationDao + Send + Sync>) -> Arc<dyn OrganizationDal + Send + Sync> {
    Arc::new(OrganizationDalImpl { organization_dao })
}

// ==================== DAL 接口 ====================

/// Organization DAL 接口
#[async_trait::async_trait]
pub trait OrganizationDal: Send + Sync {
    /// 检查系统是否已经初始化
    ///
    /// 通过检查 organizations 表是否有记录判断
    async fn is_initialized(&self, ctx: RequestContext) -> Result<bool, AppError>;

    /// 根据 ID 获取组织
    async fn get_by_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<Option<OrganizationPo>, AppError>;

    /// 创建组织
    async fn create(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<(), AppError>;

    /// 通用综合查询
    ///
    /// 支持组合查询条件，所有字段都是 Option
    async fn query(
        &self,
        ctx: RequestContext,
        query: OrganizationQuery,
    ) -> Result<Vec<OrganizationPo>, AppError>;

    /// 获取所有组织
    async fn list_all(
        &self,
        ctx: RequestContext,
    ) -> Result<Vec<OrganizationPo>, AppError>;

    /// 更新组织信息
    async fn update(
        &self,
        ctx: RequestContext,
        org: &OrganizationPo,
    ) -> Result<(), AppError>;

    /// 删除组织（软删除）
    async fn delete(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<(), AppError>;

    /// 统计组织总数
    async fn count_organizations(
        &self,
        ctx: RequestContext,
    ) -> Result<u64, AppError>;
}

// ==================== DAL 实现 ====================

/// Organization DAL 实现
struct OrganizationDalImpl {
    organization_dao: Arc<dyn OrganizationDao + Send + Sync>,
}

#[async_trait::async_trait]
impl OrganizationDal for OrganizationDalImpl {
    async fn is_initialized(&self, ctx: RequestContext) -> Result<bool, AppError> {
        let count = self.organization_dao.count_all(ctx).await?;
        Ok(count > 0)
    }

    async fn get_by_id(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<Option<OrganizationPo>, AppError> {
        self.organization_dao.find_by_id(ctx, org_id).await
    }

    async fn create(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<(), AppError> {
        self.organization_dao.insert(ctx, org).await
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: OrganizationQuery,
    ) -> Result<Vec<OrganizationPo>, AppError> {
        self.organization_dao.query(ctx, query).await
    }

    async fn list_all(
        &self,
        ctx: RequestContext,
    ) -> Result<Vec<OrganizationPo>, AppError> {
        self.query(ctx, OrganizationQuery::default()).await
    }

    async fn update(
        &self,
        ctx: RequestContext,
        org: &OrganizationPo,
    ) -> Result<(), AppError> {
        self.organization_dao.update(ctx, org).await
    }

    async fn delete(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<(), AppError> {
        self.organization_dao.delete(ctx, org_id).await
    }

    async fn count_organizations(
        &self,
        ctx: RequestContext,
    ) -> Result<u64, AppError> {
        self.organization_dao.count_all(ctx).await
    }
}
