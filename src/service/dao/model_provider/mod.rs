//! Model Provider DAO 模块

use common::error::{Error, Result};
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::RequestContext;
use common::enums::{ModelCapability, ModelProviderStatus, ProviderType};

/// ModelProvider 查询参数
#[derive(Debug, Clone, Default)]
pub struct ModelProviderQuery {
    pub provider_type: Option<ProviderType>,
    pub capability: Option<ModelCapability>,
    pub status: Option<ModelProviderStatus>,
    pub exclude_status: Option<ModelProviderStatus>,
    pub limit: Option<usize>,
}

/// Model Provider DAO 接口
#[async_trait::async_trait]
pub trait ModelProviderDao: Send + Sync {
    async fn insert(&self, ctx: RequestContext, provider: &ModelProviderPo)
    -> Result<()>;
    async fn find_by_id(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<ModelProviderPo>>;

    /// 通用查询
    async fn query(
        &self,
        ctx: RequestContext,
        query: ModelProviderQuery,
    ) -> Result<Vec<ModelProviderPo>>;

    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<ModelProviderPo>>;
    async fn update(&self, ctx: RequestContext, provider: &ModelProviderPo)
    -> Result<()>;
    async fn delete(&self, ctx: RequestContext, provider: &ModelProviderPo)
    -> Result<()>;

    /// 获取默认的 Embedding Provider（第一个可用的）
    async fn get_default_embedding_provider(
        &self,
        ctx: RequestContext,
    ) -> Result<Option<ModelProviderPo>>;
}

mod sqlite;
pub use self::sqlite::{dao, init};

#[cfg(test)]
mod sqlite_test;
