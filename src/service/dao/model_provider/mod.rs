//! Model Provider DAO 模块

use crate::error::AppError;
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::RequestContext;

/// Model Provider DAO 接口
#[async_trait::async_trait]
pub trait ModelProviderDao: Send + Sync {
    async fn insert(&self, ctx: RequestContext, provider: &ModelProviderPo) -> Result<(), AppError>;
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<ModelProviderPo>, AppError>;
    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<ModelProviderPo>, AppError>;
    async fn update(&self, ctx: RequestContext, provider: &ModelProviderPo) -> Result<(), AppError>;
    async fn delete(&self, ctx: RequestContext, provider: &ModelProviderPo) -> Result<(), AppError>;
}


mod sqlite;
pub use self::sqlite::{dao,init};

#[cfg(test)]
mod sqlite_test;