//! Handler: POST /api/v1/hr/agents/external - 创建外部 Agent（Cli/Remote）
//!
//! 这是一个用户行为导向的 Handler：将 HTTP 请求参数转换为通用的 Agent 实体
//! （设置 kind + external_config），然后调用 Domain 层通用的 create_agent 方法。
//!
//! Domain 层不提供 create_external_agent 等同作用语法糖方法，
//! 用户行为差异通过不同 Handler 处理。

use crate::models::agent::{Agent, AgentPo, AgentRuntimeConfig, ExternalAgentConfig};
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{CreateExternalAgentRequest, CreateExternalAgentResponse};
use common::enums::AgentKind;
use common::error::{Error, Result, bail_err, err};

/// Create a new external AI agent (Cli or Remote kind)
#[register_handler_tool(
    id = "create_external_agent",
    name = "Register External Agent",
    description = "Create a new external AI agent (CLI or A2A Remote)",
    params = "common::api::CreateExternalAgentRequest"
)]
#[generate_http_handler]
pub async fn create_external_agent(
    ctx: RequestContext,
    params: CreateExternalAgentRequest,
) -> Result<CreateExternalAgentResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    // 解析 kind
    let kind = match params.kind.as_str() {
        "cli" => AgentKind::Cli,
        "remote" => AgentKind::Remote,
        other => {
            return Err(Error::bad_request(format!(
                "Invalid kind '{}', expected 'cli' or 'remote'",
                other
            )));
        }
    };

    // 按 kind 构造 external_config
    let external_config = match kind {
        AgentKind::Cli => {
            let command = params
                .command
                .as_ref()
                .ok_or_else(|| Error::bad_request("command is required for cli kind"))?;
            let work_dir = params
                .work_dir
                .as_ref()
                .ok_or_else(|| Error::bad_request("work_dir is required for cli kind"))?;
            ExternalAgentConfig::Cli {
                command: command.clone(),
                args: params.args.clone().unwrap_or_default(),
                work_dir: work_dir.clone(),
                env: params.env.clone().unwrap_or_default(),
                timeout_secs: params.timeout_secs.unwrap_or(300),
                prompt_template: params.prompt_template.clone(),
            }
        }
        AgentKind::Remote => {
            let endpoint = params
                .endpoint
                .as_ref()
                .ok_or_else(|| Error::bad_request("endpoint is required for remote kind"))?;
            let agent_name = params
                .agent_name
                .as_ref()
                .ok_or_else(|| Error::bad_request("agent_name is required for remote kind"))?;
            ExternalAgentConfig::Remote {
                endpoint: endpoint.clone(),
                agent_name: agent_name.clone(),
                auth_token: params.auth_token.clone(),
                timeout_secs: params.timeout_secs.unwrap_or(300),
            }
        }
        AgentKind::Local => unreachable!(),
    };

    // 构造 AgentPo（外部 agent 的 model_provider_id 留空）
    let mut po = AgentPo::new(
        params.name.clone(),
        params.roles.unwrap_or_default(),
        params.description.unwrap_or_default(),
        params.capabilities.unwrap_or_default(),
        params.soul.unwrap_or_default(),
        String::new(),
        user_id.to_string(),
    );
    po.kind = kind;

    // 写入 external_config 到 runtime_config
    let mut runtime_config: AgentRuntimeConfig = po.get_runtime_config();
    runtime_config.external_config = Some(external_config);
    po.set_runtime_config(&runtime_config);

    let agent = Agent::from_po(po);

    // 调用通用 create_agent（Domain 层按 kind 跳过 model_provider_id 校验）
    domain()
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await?;

    // 重新查询拿到 created_at
    let created = domain()
        .agent_manage()
        .get_agent(ctx, agent.id(), Default::default())
        .await?
        .ok_or_else(|| err!(NotFound, "Agent {} not found", agent.id()))?;

    Ok(CreateExternalAgentResponse {
        id: created.id().to_string(),
        name: created.name().to_string(),
        kind: params.kind,
        created_at: created.po.created_at,
    })
}
