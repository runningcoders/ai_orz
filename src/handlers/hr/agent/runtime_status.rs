//! Handler: GET /api/v1/hr/agents/{id}/runtime-status - 查询 Agent 运行时状态

use crate::pkg::RequestContext;
use crate::service::domain::runtime::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{RuntimeStatusRequest, RuntimeStatusResponse, ThinkRuntimeInfo};
use common::error::Result;

/// 查询 Agent 运行时状态 + 思考运行时快照
#[register_handler_tool(
    id = "get_runtime_status",
    name = "Get System Runtime Status",
    description = "Get an agent's runtime status and thinking snapshot (current round, tokens, tool calls, trace_id). Useful for checking if an agent is busy before sending it a message.",
    params = "common::api::RuntimeStatusRequest",
    tags = "collaboration,query"
)]
#[generate_http_handler]
pub async fn runtime_status(
    ctx: RequestContext,
    params: RuntimeStatusRequest,
) -> Result<RuntimeStatusResponse> {
    let runtime = domain();
    let (state, current_message_id, task_id, project_id, state_started_at, think_runtime) =
        runtime.get_runtime_status(&params.id);

    let state_str = match state {
        common::enums::AgentRuntimeState::Idle => "idle",
        common::enums::AgentRuntimeState::Busy => "busy",
        common::enums::AgentRuntimeState::Resting => "resting",
    };

    let think_runtime_info = think_runtime.map(|snap| ThinkRuntimeInfo {
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
    });

    log_debug!(
        &ctx,
        "runtime_status",
        "agent_id={}, state={}",
        params.id,
        state_str
    );

    Ok(RuntimeStatusResponse {
        agent_id: params.id,
        state: state_str.to_string(),
        current_message_id,
        task_id,
        project_id,
        state_started_at,
        think_runtime: think_runtime_info,
    })
}
