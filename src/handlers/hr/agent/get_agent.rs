//! Handler: GET /api/v1/agents/{id} - Get agent detailed information

use super::association::{build_flat_skills, build_flat_tools};
use crate::models::agent::ExternalAgentConfig;
use crate::pkg::RequestContext;
use crate::service::dal::agent::AgentFetchOptions;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{
    AgentCliConfig, AgentExternalConfigInfo, AgentRemoteConfig, AgentRuntimeConfigInfo,
    GetAgentRequest, GetAgentResponse,
};
use common::enums::{AgentKind, AgentRuntimeState};
use common::error::Result;
use common::models::StatsInterval;

/// Get detailed information about an AI agent
#[register_handler_tool(
    id = "get_agent",
    name = "get_agent",
    description = "Get detailed information about an AI agent by ID",
    params = "common::api::GetAgentRequest",
    tags = "collaboration"
)]
#[generate_http_handler]
pub async fn get_agent(ctx: RequestContext, params: GetAgentRequest) -> Result<GetAgentResponse> {
    let now = chrono::Utc::now().timestamp_millis();
    let default_range = (now - 7 * 24 * 3600 * 1000, now);

    let with_tools = params.with_tools.unwrap_or(false);
    let with_skills = params.with_skills.unwrap_or(false);

    let options = AgentFetchOptions {
        with_tools: params.with_tools,
        with_skills: params.with_skills,
        with_stats: params.with_stats,
        with_model_call_stats: params.with_model_call_stats,
        stats_time_range: match (params.stats_time_start, params.stats_time_end) {
            (Some(start), Some(end)) => Some((start, end)),
            _ if params.with_stats.unwrap_or(false)
                || params.with_model_call_stats.unwrap_or(false) =>
            {
                Some(default_range)
            }
            _ => None,
        },
        stats_interval: params.stats_interval.as_deref().and_then(|s| {
            match s.to_lowercase().as_str() {
                "hourly" => Some(StatsInterval::Hourly),
                "daily" => Some(StatsInterval::Daily),
                _ => None,
            }
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

    // 从 runtime_info 读取运行时状态
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

    // 装配 Agent 工具/技能扁平列表（后端只保证「去重后的实体全集」，
    // 分组交给前端按 installed pack tag 完成）。按 with_tools / with_skills 开关按需装配，
    // 关闭侧直接短路，不做任何工具/技能查询。工具额外经 runtime domain 做就绪探测，
    // 因此 runtime_ready 是真实值，而不是 domain 层硬编码的 Unknown。
    let tool_list = if with_tools {
        let ids = domain()
            .agent_manage()
            .get_agent_tool_list_ids(ctx.clone(), &agent)
            .await?;
        Some(build_flat_tools(ctx.clone(), ids).await?)
    } else {
        None
    };
    let skill_list = if with_skills {
        Some(build_flat_skills(ctx.clone(), &params.id).await?)
    } else {
        None
    };

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
        runtime_config,
        status: agent.po.status as i32,
        created_at: agent.po.created_at,
        updated_at: agent.po.updated_at,
        runtime_state,
        current_message_id,
        current_task_id,
        current_project_id,
        tool_list,
        skill_list,
        stats: agent.stats,
        model_call_stats: agent.model_call_stats,
    })
}
