use common::api::{GetProjectResponse, ProjectListItem};

use crate::models::project::Project;

pub(super) fn to_list_item(project: &Project) -> ProjectListItem {
    ProjectListItem {
        id: project.po.id.clone(),
        name: project.po.name.clone(),
        description: optional_string(&project.po.description),
        status: project.po.status as i32,
        priority: project.po.priority,
        tags: project.po.get_tags(),
        root_user_id: project.po.root_user_id.clone(),
        owner_agent_id: project.po.owner_agent_id.clone(),
        created_at: project.po.created_at,
        updated_at: project.po.updated_at,
    }
}

pub(super) fn to_detail(project: &Project) -> GetProjectResponse {
    GetProjectResponse {
        id: project.po.id.clone(),
        name: project.po.name.clone(),
        description: optional_string(&project.po.description),
        workflow: project.po.workflow.clone(),
        guidance: project.po.guidance.clone(),
        status: project.po.status as i32,
        priority: project.po.priority,
        tags: project.po.get_tags(),
        root_user_id: project.po.root_user_id.clone(),
        owner_agent_id: project.po.owner_agent_id.clone(),
        start_at: project.po.start_at,
        due_at: project.po.due_at,
        end_at: project.po.end_at,
        created_at: project.po.created_at,
        updated_at: project.po.updated_at,
        stats: project.stats.clone(),
        model_call_stats: project.model_call_stats.clone(),
        task_graph: project.task_graph.clone(),
        artifacts: project.artifacts.clone(),
        progress_summary: project.progress_summary.clone(),
    }
}

fn optional_string(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}
