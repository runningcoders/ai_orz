//! Handler: PUT /api/v1/agents/{id}/status - Update agent status

use common::enums::{AgentRuntimeState, AgentKind};
use common::error::Result;
use crate::models::agent::ExternalAgentConfig;
use crate::pkg::RequestContext;
use crate::service::domain::{finance::domain as finance_domain, hr::domain};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{AgentCliConfig, AgentExternalConfigInfo, AgentRemoteConfig, UpdateAgentStatusRequest, UpdateAgentStatusResponse};

use crate::enrich_ctx;

/// Update the status of an AI agent (active/disabled)
#[register_handler_tool(
    id = "update_agent_status",
    name = "update_agent_status",
    description = "Update the status of an AI agent (active/disabled)",
    params = "common::api::UpdateAgentStatusRequest"
)]
#[generate_http_handler]
pub async fn update_agent_status(
    ctx: RequestContext,
    params: UpdateAgentStatusRequest,
) -> Result<UpdateAgentStatusResponse> {
    let agent = domain()
        .agent_manage()
        .get_agent(ctx.clone(), &params.id, Default::default())
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Agent {} not found", params.id)))?;

    let ctx = enrich_ctx!(&ctx, &agent);
    let mut agent = agent;

    domain()
        .agent_manage()
        .transition_status(ctx.clone(), &mut agent, params.status)
        .await?;

    let capabilities: Vec<String> = agent.po.get_capabilities();
    let roles: Vec<String> = agent.po.get_roles();
    let kind = agent.po.kind;

    let external_config = match kind {
        AgentKind::Local => None,
        AgentKind::Cli | AgentKind::Remote => {
            let runtime_config = agent.po.get_runtime_config();
            match runtime_config.external_config {
                Some(ExternalAgentConfig::Cli { command, args, work_dir, env: _, timeout_secs, prompt_template }) => {
                    Some(AgentExternalConfigInfo {
                        cli: Some(AgentCliConfig {
                            command,
                            args,
                            work_dir,
                            timeout_secs,
                            prompt_template,
                        }),
                        remote: None,
                    })
                }
                Some(ExternalAgentConfig::Remote { endpoint, agent_name, auth_token: _, timeout_secs }) => {
                    Some(AgentExternalConfigInfo {
                        cli: None,
                        remote: Some(AgentRemoteConfig {
                            endpoint,
                            agent_name,
                            timeout_secs,
                        }),
                    })
                }
                None => None,
            }
        }
    };

    let (runtime_state, current_message_id) = match &agent.runtime_info {
        Some(info) => (info.state as i32, info.current_message_id.clone()),
        None => (AgentRuntimeState::Idle as i32, None),
    };

    let tools = finance_domain()
        .tool_provider_manage()
        .get_agent_bound_tool_ids(ctx, &params.id)
        .await
        .unwrap_or_default();

    Ok(UpdateAgentStatusResponse {
        id: agent.id().to_string(),
        name: agent.name().to_string(),
        roles,
        description: if agent.po.description.is_empty() {
            None
        } else {
            Some(agent.po.description.clone())
        },
        capabilities: if capabilities.is_empty() {
            None
        } else {
            Some(capabilities)
        },
        soul: if agent.po.soul.is_empty() {
            None
        } else {
            Some(agent.po.soul.clone())
        },
        kind: kind.to_string(),
        model_provider_id: agent.po.model_provider_id.clone(),
        external_config,
        status: agent.po.status as i32,
        created_at: agent.po.created_at,
        updated_at: agent.po.updated_at,
        runtime_state,
        current_message_id,
        tools,
        stats: None,
        model_call_stats: None,
    })
}
