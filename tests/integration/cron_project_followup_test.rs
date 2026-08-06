//! Integration test for `CronTriggerConsumer` `project_followup` action.
//!
//! Verifies the Agent Loop Engine "scenario 3" (定时补偿) flow:
//! - Create a project with `owner_agent_id` set and transition it to InProgress
//! - Ensure the Owner Agent runtime state is Idle
//! - Fire a `CronTriggerEvent` with payload `{"action":"project_followup","extra":{}}`
//! - `CronTriggerConsumer::on_event` should send a `ProjectFollowupNotification`
//!   message to the Owner Agent with `project_id` filled

#[path = "../common/mod.rs"]
mod common;

use crate::common::TestApp;
use ai_orz::consumer::scheduler::CronTriggerConsumer;
use ai_orz::pkg::agent_runtime_state::AgentRuntimeStateManager;
use ai_orz::pkg::aop::Consumer;
use ai_orz::service::domain::message;
use common::enums::{MessageRole, MessageType};
use serde_json::json;
use sqlx::SqlitePool;

/// Create a project via HTTP API with the given `owner_agent_id` (optional).
///
/// Returns the new project's ID.
async fn create_project_with_owner(
    app: &TestApp,
    jwt: &str,
    name: &str,
    owner_agent_id: Option<&str>,
) -> String {
    let mut req = json!({
        "name": name,
        "description": "Test project for cron project_followup",
    });
    if let Some(agent_id) = owner_agent_id {
        req["owner_agent_id"] = json!(agent_id);
    }
    let (status, body) = app.post_with_jwt("/api/v1/projects", &req, jwt).await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("id")
        .and_then(|v| v.as_str())
        .expect("missing id in project create response")
        .to_string()
}

/// Transition a project's status via HTTP API (PUT /projects/{id}/status).
async fn transition_project_status(app: &TestApp, jwt: &str, project_id: &str, status: &str) {
    let req = json!({
        "id": project_id,
        "status": status,
    });
    let (status_code, body) = app
        .put_with_jwt(
            &format!("/api/v1/projects/{}/status", project_id),
            &req,
            jwt,
        )
        .await;
    assert_eq!(
        status_code,
        axum::http::StatusCode::OK,
        "status transition to {} should succeed, body: {}",
        status,
        body
    );
}

/// `CronTriggerConsumer` `project_followup` action sends a
/// `ProjectFollowupNotification` message to the Owner Agent of every InProgress
/// project, with `project_id` filled.
#[sqlx::test]
async fn test_cron_project_followup_sends_notification(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // Create an Agent to use as owner_agent_id
    let agent_id = crate::common::factories::create_test_agent(
        &app,
        &jwt,
        &bs.chat_provider_id,
        &format!("FollowupOwner-{}", uuid::Uuid::now_v7()),
    )
    .await;

    // Create a project owned by the agent and transition it to InProgress
    let project_id = create_project_with_owner(
        &app,
        &jwt,
        &format!("FollowupProject-{}", uuid::Uuid::now_v7()),
        Some(&agent_id),
    )
    .await;
    transition_project_status(&app, &jwt, &project_id, "InProgress").await;

    // Ensure Owner Agent state is Idle so the pre-check passes
    AgentRuntimeStateManager::global().set_idle(&agent_id);

    // Construct a CronTriggerEvent with payload {"action":"project_followup","extra":{}}
    let payload = json!({"action":"project_followup","extra":{}}).to_string();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let event_value = json!({
        "event_id": uuid::Uuid::now_v7().to_string(),
        "trigger_id": "test-trigger-project-followup",
        "trigger_name": "test project followup trigger",
        "payload": payload,
        "created_at": now_ms,
    });

    // Call CronTriggerConsumer::on_event
    let consumer = CronTriggerConsumer::new();
    consumer
        .on_event(event_value)
        .await
        .expect("on_event project_followup should succeed");

    // Verify the Owner Agent received a ProjectFollowupNotification message
    // with project_id filled.
    let messages = message::domain()
        .management()
        .list_by_project_id(ctx, &project_id)
        .await
        .expect("list_by_project_id should succeed");

    let followup = messages.iter().find(|m| {
        m.po.to_id == agent_id && m.po.message_type == MessageType::ProjectFollowupNotification
    });
    let followup = followup.unwrap_or_else(|| {
        panic!(
            "expected ProjectFollowupNotification for agent {} on project {}, got: {:?}",
            agent_id,
            project_id,
            messages
                .iter()
                .map(|m| (m.po.message_type, m.po.to_id.clone()))
                .collect::<Vec<_>>()
        );
    });

    assert_eq!(
        followup.po.project_id,
        Some(project_id.clone()),
        "project_id should be filled on the followup message"
    );
    assert_eq!(followup.po.to_id, agent_id);
    assert_eq!(followup.po.from_id, "system");
    assert_eq!(followup.po.from_role, MessageRole::System);
    assert_eq!(
        followup.po.message_type,
        MessageType::ProjectFollowupNotification
    );
    // Content should be produced by build_project_followup_content
    assert!(followup.po.content.contains("项目进度定期检查"));
    assert!(followup.po.content.contains("get_project"));
}
