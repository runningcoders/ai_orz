//! Handler: GET /api/v1/agents/{id} - Get agent detailed information

use super::association::{build_skills_overview, build_tools_overview};
use crate::models::agent::ExternalAgentConfig;
use crate::pkg::RequestContext;
use crate::service::dal::agent::AgentFetchOptions;
use crate::service::domain::{finance::domain as finance_domain, hr::domain};
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

    // 获取已绑定的工具 ID 列表（保留：聊天侧面板计数使用）
    let tools = finance_domain()
        .tool_provider_manage()
        .get_agent_bound_tool_ids(ctx.clone(), &params.id)
        .await
        .unwrap_or_default();

    // 装配 Agent 全景视图（工具三分组 + 技能三分组），分两步：
    //   1) Hr domain 产出 **ID 分组**（neural/bound/pack 的分组规则归它）；
    //   2) association 模块跨领域编排：调专业领域查询实体并打包成 DTO
    //      —— 工具额外经 runtime domain 做就绪探测，因此 runtime_ready 是真实值，
    //        而不是 domain 层硬编码的 Unknown。
    // 全景数据体量较大，按 with_tools / with_skills 开关按需装配：
    // 两侧均关闭时后端直接短路，不做任何工具/技能查询。
    let (tool_groups, skill_groups) = domain()
        .agent_manage()
        .get_agent_association_groups(ctx.clone(), &agent, with_tools, with_skills)
        .await?;

    let tools_overview = match tool_groups {
        Some(groups) => Some(build_tools_overview(ctx.clone(), groups).await?),
        None => None,
    };
    let skills_overview = match skill_groups {
        Some(groups) => Some(build_skills_overview(ctx.clone(), &params.id, groups).await?),
        None => None,
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
        tools,
        tools_overview,
        skills_overview,
        stats: agent.stats,
        model_call_stats: agent.model_call_stats,
    })
}
