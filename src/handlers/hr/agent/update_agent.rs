//! Handler: PUT /api/v1/agents/{id} - Update agent information

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{AgentRuntimeConfigInfo, UpdateAgentRequest, UpdateAgentResponse};
use common::error::Result;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::enrich_ctx;

/// Update the metadata and configuration of an existing AI agent
#[register_handler_tool(
    id = "update_agent",
    name = "update_agent",
    description = "Update the metadata and configuration of an existing AI agent",
    params = "common::api::UpdateAgentRequest"
)]
#[generate_http_handler]
pub async fn update_agent(
    ctx: RequestContext,
    params: UpdateAgentRequest,
) -> Result<UpdateAgentResponse> {
    fn current_timestamp() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }

    let mut agent = domain()
        .agent_manage()
        .get_agent(ctx.clone(), &params.id, Default::default())
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Agent {} not found", params.id)))?;

    let ctx = enrich_ctx!(&ctx, &agent);

    // Update fields
    if let Some(name) = params.name {
        agent.po.name = name;
    }
    if let Some(roles) = params.roles {
        agent.po.role = serde_json::to_string(&roles).unwrap_or_else(|_| "[]".to_string());
    }
    if let Some(description) = params.description {
        agent.po.description = description;
    }
    if let Some(capabilities) = params.capabilities {
        agent.po.capabilities =
            serde_json::to_string(&capabilities).unwrap_or_else(|_| "[]".to_string());
    }
    if let Some(soul) = params.soul {
        agent.po.soul = soul;
    }
    if let Some(model_provider_id) = params.model_provider_id {
        agent.po.model_provider_id = model_provider_id;
    }
    // 更新运行时配置（整体替换：前端传入完整的 runtime_config 对象）
    if let Some(rc_info) = params.runtime_config {
        let mut rc = agent.po.get_runtime_config();
        rc.max_thinking_rounds = rc_info.max_thinking_rounds;
        rc.intent_analyze_max_rounds = rc_info.intent_analyze_max_rounds;
        rc.summary_max_rounds = rc_info.summary_max_rounds;
        rc.think_timeout_secs = rc_info.think_timeout_secs;
        agent.po.set_runtime_config(&rc);
    }
    // Update modified_by and updated_at
    agent.po.modified_by = ctx.uid();
    agent.po.updated_at = current_timestamp();

    domain().agent_manage().update_agent(ctx, &agent).await?;

    let roles: Vec<String> = agent.po.get_roles();
    let capabilities: Vec<String> = agent.po.get_capabilities();

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

    Ok(UpdateAgentResponse {
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
        kind: agent.po.kind.to_string(),
        model_provider_id: agent.po.model_provider_id.clone(),
        runtime_config,
        updated_at: agent.po.updated_at,
    })
}
