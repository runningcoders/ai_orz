use axum::Json;
use axum::extract::Path;
use axum::response::IntoResponse;
use common::api::a2a::A2aTask;
use common::enums::TaskStatus;
use common::error::Error;
use serde_json::json;

use crate::models::events::{
    A2A_SYNCED_MSG_COUNT_PREFIX, extract_a2a_task_id, extract_text_from_parts,
    get_synced_msg_count, make_synced_msg_tag,
};
use crate::pkg::RequestContext;
use crate::service::domain::message::{self as message_domain, SendToUserCommand};
use crate::service::domain::project as project_domain;

pub async fn handle_a2a_callback(
    axum::Extension(ctx): axum::Extension<RequestContext>,
    Path(task_id): Path<String>,
    Json(task): Json<A2aTask>,
) -> common::error::Result<impl IntoResponse> {
    let Some(mut local_task) = project_domain::domain()
        .task_manage()
        .get(ctx.clone(), &task_id)
        .await?
    else {
        return Err(Error::not_found(format!("Task {} not found", task_id)));
    };

    if matches!(
        local_task.po.status,
        TaskStatus::Completed | TaskStatus::Cancelled | TaskStatus::Archived
    ) {
        return Ok(Json(
            json!({"ok": true, "skipped": true, "reason": "task already terminal"}),
        ));
    }

    let tags = local_task.po.get_tags();
    let Some(expected_remote_task_id) = extract_a2a_task_id(&tags) else {
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

    let agent_id = local_task.po.assignee_id.clone();

    let mut task_ctx_builder = RequestContext::builder();
    task_ctx_builder = task_ctx_builder.agent_id(agent_id.clone());
    task_ctx_builder = task_ctx_builder.task_id(task_id.clone());
    if let Some(pid) = &local_task.po.project_id {
        task_ctx_builder = task_ctx_builder.project_id(pid.clone());
    }
    let task_ctx = task_ctx_builder.build();

    let already_synced = get_synced_msg_count(&tags);
    let agent_messages: Vec<_> = task
        .messages
        .iter()
        .filter(|msg| msg.role == "agent" || msg.role == "assistant")
        .collect();
    let total_agent_msgs = agent_messages.len();
    let mut new_sent = 0usize;

    if total_agent_msgs > already_synced {
        let new_messages = &agent_messages[already_synced..];
        for msg in new_messages {
            let text = extract_text_from_parts(&msg.parts);
            if text.is_empty() {
                continue;
            }

            let cmd = SendToUserCommand {
                from_agent_id: &agent_id,
                to_user_id: &local_task.po.root_user_id,
                content: &text,
                project_id: local_task.po.project_id.as_deref(),
                task_id: Some(&task_id),
                reply_to_id: None,
            };

            if let Err(e) = message_domain::domain()
                .delivery()
                .send_to_user(task_ctx.clone(), cmd)
                .await
            {
                log_warn!(
                    &task_ctx,
                    "a2a_callback",
                    "Failed to send message for task {}: {}",
                    task_id,
                    e
                );
            } else {
                new_sent += 1;
            }
        }
    }

    if new_sent > 0 {
        let new_total = already_synced + new_sent;
        let mut new_tags: Vec<String> = tags
            .iter()
            .filter(|t| !t.starts_with(A2A_SYNCED_MSG_COUNT_PREFIX))
            .cloned()
            .collect();
        new_tags.push(make_synced_msg_tag(new_total));

        if let Err(e) = project_domain::domain()
            .task_manage()
            .update_basic(
                task_ctx.clone(),
                &task_id,
                None,
                None,
                None,
                Some(new_tags),
                None,
                None,
            )
            .await
        {
            log_warn!(
                &task_ctx,
                "a2a_callback",
                "Failed to update synced msg count for task {}: {}",
                task_id,
                e
            );
        }
    }

    let target_status = match task.status.state {
        common::api::a2a::A2aTaskState::Completed => Some(TaskStatus::Completed),
        common::api::a2a::A2aTaskState::Failed => Some(TaskStatus::Cancelled),
        common::api::a2a::A2aTaskState::Canceled => Some(TaskStatus::Cancelled),
        common::api::a2a::A2aTaskState::Working
        | common::api::a2a::A2aTaskState::Submitted
        | common::api::a2a::A2aTaskState::InputRequired => {
            if local_task.po.status == TaskStatus::Pending {
                Some(TaskStatus::InProgress)
            } else {
                None
            }
        }
    };

    if let Some(target) = target_status {
        if local_task.po.status != target {
            if let Err(e) = project_domain::domain()
                .task_manage()
                .transition_status(task_ctx.clone(), &mut local_task, target)
                .await
            {
                log_warn!(
                    &task_ctx,
                    "a2a_callback",
                    "Failed to transition task {} to {:?}: {}",
                    task_id,
                    target,
                    e
                );
            } else {
                log_info!(
                    &task_ctx,
                    "a2a_callback",
                    "Task {} transitioned to {:?} via callback",
                    task_id,
                    target
                );
            }
        }
    }

    log_info!(
        &ctx,
        "a2a_callback",
        "task={} state={:?} new_msgs={}",
        task_id,
        task.status.state,
        new_sent
    );

    Ok(Json(json!({"ok": true})))
}
