use axum::extract::Path;
use axum::Json;
use axum::response::IntoResponse;
use common::api::a2a::A2aTask;
use common::enums::TaskStatus;
use common::error::Error;
use serde_json::json;
use uuid::Uuid;

use crate::models::events::{A2aTaskUpdateEvent, A2aUpdateSource};
use crate::pkg::aop::publish;
use crate::pkg::RequestContext;
use crate::service::domain::project as project_domain;

pub async fn handle_a2a_callback(
    axum::Extension(ctx): axum::Extension<RequestContext>,
    Path(task_id): Path<String>,
    Json(task): Json<A2aTask>,
) -> common::error::Result<impl IntoResponse> {
    let Some(local_task) = project_domain::domain()
        .task_manage()
        .get(ctx.clone(), &task_id)
        .await?
    else {
        return Err(Error::not_found(format!("Task {} not found", task_id)));
    };

    if !matches!(
        local_task.po.status,
        TaskStatus::InProgress | TaskStatus::Pending
    ) {
        return Err(Error::bad_request(format!(
            "Task {} is not in an active state (status: {:?})",
            task_id, local_task.po.status
        )));
    }

    let tags = local_task.po.get_tags();
    let Some(expected_remote_task_id) = A2aTaskUpdateEvent::extract_a2a_task_id(&tags) else {
        return Err(Error::bad_request(format!(
            "Task {} does not have an associated A2A remote task ID",
            task_id
        )));
    };

    if expected_remote_task_id != task.id {
        return Err(Error::bad_request(format!(
            "Remote task ID mismatch: expected {}, got {}",
            expected_remote_task_id, task.id
        )));
    }

    let task_json = serde_json::to_string(&task)
        .map_err(|e| Error::internal(format!("Failed to serialize task: {}", e)))?;

    let event = A2aTaskUpdateEvent {
        event_id: Uuid::now_v7().to_string(),
        local_task_id: task_id,
        remote_agent_id: local_task.po.assignee_id.clone(),
        remote_task_id: task.id.clone(),
        source: A2aUpdateSource::Callback,
        task_json,
        created_at: common::constants::utils::current_timestamp(),
    };

    publish(event).await;

    Ok(Json(json!({"ok": true})))
}
