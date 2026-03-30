//! Organization DAO 模块

use crate::error::AppError;
use crate::models::organization::OrganizationPo;
use crate::pkg::RequestContext;
use rusqlite::Connection;

/// Organization DAO 接口
pub trait OrganizationDaoTrait: Send + Sync {
    fn insert(
        &self,
        ctx: RequestContext,
        conn: &Connection,
        org: &OrganizationPo,
    ) -> Result<(), AppError>;
    fn find_by_id(
        &self,
        ctx: RequestContext,
        conn: &Connection,
        id: &str,
    ) -> Result<Option<OrganizationPo>, AppError>;
    fn find_all(
        &self,
        ctx: RequestContext,
        conn: &Connection,
    ) -> Result<Vec<OrganizationPo>, AppError>;
    fn update(
        &self,
        ctx: RequestContext,
        conn: &Connection,
        org: &OrganizationPo,
    ) -> Result<(), AppError>;
    fn delete(&self, ctx: RequestContext, conn: &Connection, id: &str) -> Result<(), AppError>;
}

pub mod sqlite;
pub use sqlite::dao;
pub use sqlite::init;
