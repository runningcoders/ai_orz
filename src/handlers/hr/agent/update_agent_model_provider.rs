//! Handler: PUT /api/v1/hr/agents/{id}/model-provider - 只改 Agent 的模型绑定

use crate::enrich_ctx;
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain as finance_domain;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UpdateAgentModelProviderRequest, UpdateAgentModelProviderResponse};
use common::error::{Result, ensure_err, err};

/// 把指定 Agent 的对话模型换到另一个 ModelProvider（管理面窄接口）
///
/// 与 `update_agent` 的关系：复用其 domain 路径，但**不能**包装调用 ——
/// `update_agent` 在 Agent 上下文会静默忽略 `model_provider_id`。本 handler
/// 只暴露这一个字段并挂 `admin` 工具包，因此对 Agent 上下文正常写入；
/// 可达性完全由「管理员显式授予 admin 包」决定（另见 `bind_tool_to_agent`
/// 的受限工具封堵，堵住 Agent 自行绑定的旁路）。
#[register_handler_tool(
    id = "update_agent_model_provider",
    name = "Set Agent Model Provider",
    description = "Rebind an agent's chat model to another model provider by id. Only this field changes; all other agent fields are preserved. The provider must exist and have Agent capability (Embedding and Decision providers are rejected). Administrative tool: only agents explicitly granted the admin tool pack by a human administrator can call it.",
    params = "common::api::UpdateAgentModelProviderRequest",
    tags = "admin"
)]
#[generate_http_handler]
pub async fn update_agent_model_provider(
    ctx: RequestContext,
    params: UpdateAgentModelProviderRequest,
) -> Result<UpdateAgentModelProviderResponse> {
    // ⚠️ 后端校验不能省：现有 `update_agent` 的人路径不校验 provider，只有前端
    // `BindModelModal` 自己筛 `capability.is_agent()`。工具链路没有前端兜底，
    // 少了这一步会把 Embedding / Decision provider 绑成对话模型，Agent 思考直接失败。
    let provider = finance_domain()
        .model_provider_manage()
        .get_model_provider(ctx.clone(), &params.model_provider_id)
        .await?
        .ok_or_else(|| {
            err!(
                NotFound,
                "ModelProvider {} not found",
                params.model_provider_id
            )
        })?;
    ensure_err!(
        provider.po.capability.is_agent(),
        InvalidRequest,
        "ModelProvider {} capability is {:?}, only Agent capability can be bound to an agent",
        params.model_provider_id,
        provider.po.capability
    );

    let mut agent = domain()
        .agent_manage()
        .get_agent(ctx.clone(), &params.id, Default::default())
        .await?
        .ok_or_else(|| err!(NotFound, "Agent {} not found", params.id))?;

    let ctx = enrich_ctx!(&ctx, &agent);

    // 只改模型绑定一个字段；身份 / 运行时配置一律不碰。
    // `modified_by` 用 caller_id_or_system：纯 Agent 上下文常无 user_id。
    agent.po.model_provider_id = params.model_provider_id.clone();
    agent.po.modified_by = ctx.caller_id_or_system();
    agent.po.updated_at = common::constants::utils::current_timestamp_ms();

    domain().agent_manage().update_agent(ctx, &agent).await?;

    Ok(UpdateAgentModelProviderResponse {
        id: agent.po.id.clone(),
        model_provider_id: agent.po.model_provider_id.clone(),
    })
}

#[cfg(test)]
#[path = "update_agent_model_provider_tests.rs"]
mod tests;
