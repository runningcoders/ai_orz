//! Handler: GET /api/v1/model-providers/{id} - Get model provider detailed information

use crate::pkg::RequestContext;
use crate::service::dal::model_provider::ModelProviderFetchOptions;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{GetModelProviderRequest, GetModelProviderResponse};
use common::error::Result;
use common::models::StatsInterval;

/// Get detailed information about a specific model provider configuration
#[register_handler_tool(
    id = "get_model_provider",
    name = "get_model_provider",
    description = "Get detailed information about a specific model provider configuration",
    params = "common::api::GetModelProviderRequest"
)]
#[generate_http_handler]
pub async fn get_model_provider(
    ctx: RequestContext,
    params: GetModelProviderRequest,
) -> Result<GetModelProviderResponse> {
    // 构建 FetchOptions
    let options = ModelProviderFetchOptions {
        with_model_call_stats: params.with_model_call_stats,
        stats_time_range: params.stats_start_time.zip(params.stats_end_time),
        stats_interval: params.stats_interval.and_then(|s| match s.as_str() {
            "Hourly" | "hourly" => Some(StatsInterval::Hourly),
            "Daily" | "daily" => Some(StatsInterval::Daily),
            _ => None,
        }),
    };

    let provider = domain()
        .model_provider_manage()
        .get_model_provider_with_options(ctx, &params.id, options)
        .await?
        .ok_or_else(|| {
            common::error::Error::not_found(format!("ModelProvider {} not found", params.id))
        })?;

    Ok(GetModelProviderResponse {
        id: provider.po.id.clone(),
        name: provider.po.name.clone(),
        provider_type: provider.po.provider_type,
        capability: provider.po.capability,
        model_name: provider.po.model_name.clone(),
        base_url: if provider.po.base_url.as_ref().map_or(true, |d| d.is_empty()) {
            None
        } else {
            provider.po.base_url.clone()
        },
        description: if provider
            .po
            .description
            .as_ref()
            .map_or(true, |d| d.is_empty())
        {
            None
        } else {
            provider.po.description.clone()
        },
        status: provider.po.status as i32,
        created_at: provider.po.created_at,
        updated_at: provider.po.updated_at,
        stats: provider.stats,
    })
}
