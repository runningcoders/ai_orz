//! Handler: PUT /api/v1/agents/{id}/status - Update agent status

use crate::models::agent::ExternalAgentConfig;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{
    AgentCliConfig, AgentExternalConfigInfo, AgentRemoteConfig, AgentRuntimeConfigInfo,
    UpdateAgentStatusRequest, UpdateAgentStatusResponse,
};
use common::enums::{AgentKind, AgentRuntimeState};
use common::error::Result;

use crate::enrich_ctx;

/// Update the status of an AI agent (active/disabled)
#[register_handler_tool(
    id = "update_agent_status",
    name = "Toggle Agent Status",
    description = "Transition an agent's lifecycle status (Interviewing, PendingOnboard, Onboarded, PendingOffboard, Offboarded) and return the updated agent. Use it to onboard a newly created agent or to take one out of service.",
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
                Some(ExternalAgentConfig::Cli {
                    command,
                    args,
                    work_dir,
                    env: _,
                    timeout_secs,
                    prompt_template,
                }) => Some(AgentExternalConfigInfo {
                    cli: Some(AgentCliConfig {
                        command,
                        args,
                        work_dir,
                        timeout_secs,
                        prompt_template,
                    }),
                    remote: None,
                }),
                Some(ExternalAgentConfig::Remote {
                    endpoint,
                    agent_name,
                    auth_token: _,
                    timeout_secs,
                }) => Some(AgentExternalConfigInfo {
                    cli: None,
                    remote: Some(AgentRemoteConfig {
                        endpoint,
                        agent_name,
                        timeout_secs,
                    }),
                }),
                None => None,
            }
        }
    };

    let (runtime_state, current_message_id, current_task_id, current_project_id) =
        match &agent.runtime_info {
            Some(info) => (
                info.state as i32,
                info.current_message_id.clone(),
                info.task_id.clone(),
                info.project_id.clone(),
            ),
            None => (AgentRuntimeState::Idle as i32, None, None, None),
        };

    // 构造运行时配置信息（思考轮次 / 超时等用户可调参数）
    let runtime_config = {
        let rc = agent.po.get_runtime_config();
        Some(AgentRuntimeConfigInfo {
            max_thinking_rounds: rc.max_thinking_rounds,
            intent_analyze_max_rounds: rc.intent_analyze_max_rounds,
            summary_max_rounds: rc.summary_max_rounds,
            think_timeout_secs: rc.think_timeout_secs,
        })
    };

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
        runtime_config,
        status: agent.po.status as i32,
        created_at: agent.po.created_at,
        updated_at: agent.po.updated_at,
        runtime_state,
        current_message_id,
        current_task_id,
        current_project_id,
        tool_list: None,
        skill_list: None,
        stats: None,
        model_call_stats: None,
    })
}
