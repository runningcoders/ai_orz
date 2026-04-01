//! ModelProvider DAO 模块

use crate::error::AppError;
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::RequestContext;

/// ModelProvider DAO 接口
pub trait ModelProviderDaoTrait: Send + Sync {
    fn insert(&self, ctx: RequestContext, provider: &ModelProviderPo) -> Result<(), AppError>;
    fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<ModelProviderPo>, AppError>;
    fn find_all(&self, ctx: RequestContext) -> Result<Vec<ModelProviderPo>, AppError>;
    fn update(&self, ctx: RequestContext, provider: &ModelProviderPo) -> Result<(), AppError>;
    fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError>;
}

pub mod sqlite;
pub use sqlite::dao;
pub use sqlite::init;
