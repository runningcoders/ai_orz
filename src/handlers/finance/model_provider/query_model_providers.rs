//! Handler: POST /api/v1/model-providers/query - ModelProvider 通用查询接口
//!
//! 与 list_model_providers 的区别：list 是语法糖（GET，返回所有），
//! query 是完整查询能力（POST + body），支持按类型、能力、状态过滤。

use crate::pkg::RequestContext;
use crate::service::dao::model_provider::ModelProviderQuery;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ModelProviderListItem, ModelProviderQueryRequest, PagedResult};
use common::error::Result;

/// ModelProvider 通用查询（POST body，支持完整查询能力）
#[register_handler_tool(
    id = "query_model_providers",
    name = "Query Providers (Advanced)",
    description = "Filter model providers by structured criteria (provider_type, capability, status) with pagination. Best when you know the exact type or capability; use list_model_providers to browse all.",
    params = "common::api::ModelProviderQueryRequest",
    neural
)]
#[generate_http_handler]
pub async fn query_model_providers(
    ctx: RequestContext,
    params: ModelProviderQueryRequest,
) -> Result<PagedResult<ModelProviderListItem>> {
    let page = domain()
        .model_provider_manage()
        .query(
            ctx,
            ModelProviderQuery {
                provider_type: params.provider_type,
                capability: params.capability,
                status: params.status,
                exclude_status: params.exclude_status,
                pagination: params.pagination,
            },
        )
        .await?;

    Ok(page.map(|provider| ModelProviderListItem {
        id: provider.po.id.clone(),
        name: provider.po.name.clone(),
        provider_type: provider.po.provider_type,
        capability: provider.po.capability,
        model_name: provider.po.model_name.clone(),
        description: if provider
            .po
            .description
            .as_ref()
            .is_none_or(|d| d.is_empty())
        {
            None
        } else {
            provider.po.description.clone()
        },
        status: provider.po.status as i32,
        created_at: provider.po.created_at,
    }))
}
