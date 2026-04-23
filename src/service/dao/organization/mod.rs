//! Organization DAO 模块

use crate::error::AppError;
use crate::models::organization::OrganizationPo;
use crate::pkg::RequestContext;


/// Organization DAO 接口
#[async_trait::async_trait]
pub trait OrganizationDao: Send + Sync {
    async fn insert(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<(), AppError>;
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<OrganizationPo>, AppError>;
    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<OrganizationPo>, AppError>;
    async fn update(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<(), AppError>;
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError>;
    async fn count_all(&self, ctx: RequestContext) -> Result<u64, AppError>;
}



pub mod sqlite;
pub use self::sqlite::{dao, init, new};

#[cfg(test)]
mod sqlite_test;