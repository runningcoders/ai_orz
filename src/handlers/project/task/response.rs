use common::api::{GetTaskResponse, TaskListItem};

use crate::models::task::Task;
use common::bail_err;

pub(super) fn to_list_item(task: &Task) -> TaskListItem {
    TaskListItem {
        id: task.po.id.clone(),
        title: task.po.title.clone(),
        description: optional_string(&task.po.description),
        status: task.po.status as i32,
        priority: task.po.priority,
        tags: task.po.get_tags(),
        root_user_id: task.po.root_user_id.clone(),
        assignee_type: task.po.assignee_type as i32,
        assignee_id: task.po.assignee_id.clone(),
        project_id: task.po.project_id.clone(),
        thinking_depth: task.po.thinking_depth,
        created_at: task.po.created_at,
        updated_at: task.po.updated_at,
    }
}

pub(super) fn to_detail(task: &Task) -> GetTaskResponse {
    GetTaskResponse {
        id: task.po.id.clone(),
        title: task.po.title.clone(),
        description: optional_string(&task.po.description),
        status: task.po.status as i32,
        priority: task.po.priority,
        tags: task.po.get_tags(),
        due_at: task.po.due_at,
        start_at: task.po.start_at,
        end_at: task.po.end_at,
        dependencies: task.po.get_dependencies(),
        root_user_id: task.po.root_user_id.clone(),
        assignee_type: task.po.assignee_type as i32,
        assignee_id: task.po.assignee_id.clone(),
        project_id: task.po.project_id.clone(),
        thinking_depth: task.po.thinking_depth,
        created_by: task.po.created_by.clone(),
        modified_by: task.po.modified_by.clone(),
        created_at: task.po.created_at,
        updated_at: task.po.updated_at,
    }
}

fn optional_string(value: &str) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}
