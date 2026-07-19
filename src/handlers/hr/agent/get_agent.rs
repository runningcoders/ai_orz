//! Handler: GET /api/v1/agents/{id} - Get agent detailed information

use common::enums::{AgentRuntimeState, AgentKind};
use common::error::Result;
use common::models::StatsInterval;
use crate::models::agent::ExternalAgentConfig;
use crate::pkg::RequestContext;
use crate::service::dal::agent::AgentFetchOptions;
use crate::service::domain::{finance::domain as finance_domain, hr::domain};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{AgentCliConfig, AgentExternalConfigInfo, AgentRemoteConfig, GetAgentRequest, GetAgentResponse};

/// Get detailed information about an AI agent
#[register_handler_tool(
    id = "get_agent",
    name = "get_agent",
    description = "Get detailed information about an AI agent by ID",
    params = "common::api::GetAgentRequest",
    tags = "collaboration"
)]
#[generate_http_handler]
pub async fn get_agent(
    ctx: RequestContext,
    params: GetAgentRequest,
) -> Result<GetAgentResponse> {
    let options = AgentFetchOptions {
        with_stats: params.with_stats,
        with_model_call_stats: params.with_model_call_stats,
        stats_time_range: match (params.stats_time_start, params.stats_time_end) {
            (Some(start), Some(end)) => Some((start, end)),
            _ => None,
        },
        stats_interval: params.stats_interval.as_deref().and_then(|s| match s.to_lowercase().as_str() {
            "hourly" => Some(StatsInterval::Hourly),
            "daily" => Some(StatsInterval::Daily),
            _ => None,
        }),
        ..Default::default()
    };

    let agent = domain()
        .agent_manage()
        .get_agent(ctx.clone(), &params.id, options)
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Agent {} not found", params.id)))?;

    let capabilities: Vec<String> = agent.po.get_capabilities();
    let roles: Vec<String> = agent.po.get_roles();
    let kind = agent.po.kind;

    // 构造外部配置信息（仅 cli/remote 类型有值）
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

    // 从 runtime_info 读取运行时状态
    let (runtime_state, current_message_id) = match &agent.runtime_info {
        Some(info) => (info.state as i32, info.current_message_id.clone()),
        None => (AgentRuntimeState::Idle as i32, None),
    };

    // 获取已绑定的工具 ID 列表
    let tools = finance_domain()
        .tool_provider_manage()
        .get_agent_bound_tool_ids(ctx, &params.id)
        .await
        .unwrap_or_default();

    Ok(GetAgentResponse {
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
        stats: agent.stats,
        model_call_stats: agent.model_call_stats,
    })
}
