//! Cortex DAO - 模型调用接口
//!
//! 包级函数 `embed_entity` / `embed_text_for_search` 按 provider_type 路由到
//! `native::CortexDao` 实现。调用方负责获取 provider（如从 model_provider_dao 查询）。

pub mod external;
pub mod native;

pub use native::{CortexDao, CortexDaoRegistry, init, registry};

// ==================== 包级函数 ====================
//
// 便捷入口：按 provider.provider_type 路由到 native::CortexDao 实现。
// 调用方负责获取 provider（如从 model_provider_dao 查询）。

use crate::models::cortex_types::{ThinkResult, ToolDescriptor};
use crate::models::model_provider::ModelProviderPo;
use crate::models::vector::{VectorIndexParams, Vectorizable};
use crate::pkg::RequestContext;
use async_trait::async_trait;
use common::error::Result;
use std::sync::Arc;

/// 向量化实体（包级路由函数）
///
/// 按 provider.provider_type 路由到具体 CortexDao，执行向量化。
/// 调用方负责获取 provider（如从 model_provider_dao 查询）。
pub async fn embed_entity(
    ctx: RequestContext,
    provider: &ModelProviderPo,
    entity: &dyn Vectorizable,
) -> Result<VectorIndexParams> {
    let dao = native::registry().get(provider.provider_type);
    dao.embed_entity(ctx, provider, entity).await
}

/// 向量化搜索关键词（包级路由函数）
pub async fn embed_text_for_search(
    ctx: RequestContext,
    provider: &ModelProviderPo,
    text: &str,
) -> Result<VectorIndexParams> {
    let dao = native::registry().get(provider.provider_type);
    dao.embed_text_for_search(ctx, provider, text).await
}

// ==================== Dispatcher ====================
//
// Registry-based dispatcher 实现 CortexDao trait，用于 DAL 层持有 `Arc<dyn CortexDao>`。
// 所有方法按 provider.provider_type 路由到 registry 中的具体实现。

/// 获取全局 Cortex DAO dispatcher（按 provider_type 路由）
pub fn dao() -> Arc<dyn CortexDao + Send + Sync> {
    Arc::new(CortexDispatcher)
}

struct CortexDispatcher;

#[async_trait]
impl CortexDao for CortexDispatcher {
    async fn think(
        &self,
        ctx: RequestContext,
        provider: &ModelProviderPo,
        prompt: &str,
        tools: &[ToolDescriptor],
    ) -> Result<ThinkResult> {
        let dao = native::registry().get(provider.provider_type);
        dao.think(ctx, provider, prompt, tools).await
    }

    async fn embed(
        &self,
        ctx: RequestContext,
        provider: &ModelProviderPo,
        texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        let dao = native::registry().get(provider.provider_type);
        dao.embed(ctx, provider, texts).await
    }
}
