//! Handler: GET /api/v1/hr/agents/runtime-list - 查询运行中 Agent 列表

use crate::pkg::RequestContext;
use crate::service::domain::runtime::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{
    RuntimeListRequest, RuntimeListResponse, RuntimeStatusResponse, ThinkRuntimeInfo,
};
use common::error::Result;

/// 查询运行中 Agent 列表（支持按 state/task_id/project_id 过滤）
#[register_handler_tool(
    id = "list_runtime_agents",
    name = "List Running Agents",
    description = "List all agents' runtime states (idle/busy/resting) with the same thinking snapshot detail as get_runtime_status, filterable by state, task_id, or project_id. Use it to see which agents are busy or working on a specific task; for a single agent query it by id with get_runtime_status.",
    params = "common::api::RuntimeListRequest",
    tags = "collaboration,query"
)]
#[generate_http_handler]
pub async fn runtime_list(
    ctx: RequestContext,
    params: RuntimeListRequest,
) -> Result<RuntimeListResponse> {
    let runtime = domain();
    let agents = runtime.list_runtime_agents(
        params.state.as_deref(),
        params.task_id.as_deref(),
        params.project_id.as_deref(),
    );

    let items: Vec<RuntimeStatusResponse> = agents
        .into_iter()
        .map(|(agent_id, info)| {
            let state_str = match info.state {
                common::enums::AgentRuntimeState::Idle => "idle",
                common::enums::AgentRuntimeState::Busy => "busy",
                common::enums::AgentRuntimeState::Resting => "resting",
            };
            let think_runtime = info.think_runtime.as_ref().map(|tr| {
                let snap = tr.snapshot();
                ThinkRuntimeInfo {
                    trace_id: snap.trace_id,
                    scene: snap.scene.as_str().to_string(),
                    round: snap.round,
                    max_rounds: snap.max_rounds,
                    tokens_input: snap.tokens_input,
                    tokens_output: snap.tokens_output,
                    total_tokens: snap.total_tokens,
                    tool_call_count: snap.tool_call_count,
                    status: match snap.status {
                        crate::pkg::agent_runtime_state::ThinkStatus::Thinking => "thinking",
                        crate::pkg::agent_runtime_state::ThinkStatus::Cancelled => "cancelled",
                        crate::pkg::agent_runtime_state::ThinkStatus::Finished => "finished",
                    }
                    .to_string(),
                    started_at: snap.started_at,
                    last_updated_at: snap.last_updated_at,
                }
            });
            RuntimeStatusResponse {
                agent_id,
                state: state_str.to_string(),
                current_message_id: info.current_message_id,
                task_id: info.task_id,
                project_id: info.project_id,
                state_started_at: info.state_started_at,
                think_runtime,
            }
        })
        .collect();

    let total = items.len();
    log_info!(&ctx, "runtime_list", "returned {} agents", total);

    Ok(RuntimeListResponse { items, total })
}
