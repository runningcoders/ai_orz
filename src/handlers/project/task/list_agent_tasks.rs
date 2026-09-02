//! Handler: GET /api/v1/agents/{agent_id}/tasks - List tasks assigned to an agent

use super::response;
use crate::pkg::RequestContext;
use crate::service::domain::project::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ListAgentTasksRequest, TaskListItem};
use common::enums::AssigneeType;
use common::error::Result;

/// List tasks assigned to a specific agent
#[register_handler_tool(
    id = "list_agent_tasks",
    name = "List Agent's Assigned Tasks",
    description = "List all tasks assigned to a specific agent, optionally filtered by a single status and capped by limit. Returns an array of task summaries. Use list_project_tasks for the project dimension, query_tasks for structured filtering, and search_tasks for keyword search.",
    params = "common::api::ListAgentTasksRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn list_agent_tasks(
    ctx: RequestContext,
    params: ListAgentTasksRequest,
) -> Result<Vec<TaskListItem>> {
    let tasks = domain()
        .task_manage()
        .list(
            ctx,
            None,
            Some(AssigneeType::Agent),
            Some(&params.agent_id),
            params.status,
            params.limit,
        )
        .await?;
    let response_items = tasks.iter().map(response::to_list_item).collect();

    Ok(response_items)
}
