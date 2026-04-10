//! Organization DAO 接口和实现

pub mod sqlite;

use crate::error::AppError;
use crate::models::organization::OrganizationPo;
use common::constants::RequestContext;

// ==================== 接口 ====================

/// Organization DAO 接口
pub trait OrganizationDaoTrait: Send + Sync {
    /// 插入新组织
    fn insert(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<(), AppError>;

    /// 根据 ID 查询组织
    fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<OrganizationPo>, AppError>;

    /// 查询所有组织
    fn find_all(&self, ctx: RequestContext) -> Result<Vec<OrganizationPo>, AppError>;

    /// 更新组织
    fn update(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<(), AppError>;

    /// 删除组织（软删除）
    fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError>;

    /// 统计组织总数
    fn count_all(&self, ctx: RequestContext) -> Result<u64, AppError>;
}

pub use sqlite::dao;
pub use sqlite::init;

#[cfg(test)]
mod sqlite_test;
