//! mapper 单元测试

use super::mapper::*;
use common::api::a2a::{A2aMessage, A2aMessagePart, A2aTaskState};
use common::enums::{MessageRole, MessageType, ProjectStatus};

use crate::models::file::FileMeta;
use crate::models::message::Message;

#[test]
fn project_status_active_maps_to_submitted() {
    let state = project_status_to_a2a_state(ProjectStatus::Active);
    assert!(matches!(state, A2aTaskState::Submitted));
}

#[test]
fn project_status_in_progress_maps_to_working() {
    let state = project_status_to_a2a_state(ProjectStatus::InProgress);
    assert!(matches!(state, A2aTaskState::Working));
}

#[test]
fn project_status_completed_maps_to_completed() {
    let state = project_status_to_a2a_state(ProjectStatus::Completed);
    assert!(matches!(state, A2aTaskState::Completed));
}

#[test]
fn project_status_archived_maps_to_canceled() {
    let state = project_status_to_a2a_state(ProjectStatus::Archived);
    assert!(matches!(state, A2aTaskState::Canceled));
}

#[test]
fn extract_text_from_single_text_part() {
    let msg = A2aMessage {
        role: "user".to_string(),
        parts: vec![A2aMessagePart::Text {
            text: "你好".to_string(),
        }],
        message_id: None,
        task_id: None,
    };
    assert_eq!(extract_text_from_a2a_message(&msg), "你好");
}

#[test]
fn extract_text_from_multiple_text_parts() {
    let msg = A2aMessage {
        role: "user".to_string(),
        parts: vec![
            A2aMessagePart::Text {
                text: "第一行".to_string(),
            },
            A2aMessagePart::Text {
                text: "第二行".to_string(),
            },
        ],
        message_id: None,
        task_id: None,
    };
    assert_eq!(extract_text_from_a2a_message(&msg), "第一行\n第二行");
}

#[test]
fn extract_text_ignores_non_text_parts() {
    let msg = A2aMessage {
        role: "user".to_string(),
        parts: vec![
            A2aMessagePart::Text {
                text: "文本".to_string(),
            },
            A2aMessagePart::Data {
                data: serde_json::json!({"key": "value"}),
            },
        ],
        message_id: None,
        task_id: None,
    };
    assert_eq!(extract_text_from_a2a_message(&msg), "文本");
}

/// 构造测试用 Message（使用 new_with_context 构造函数）
fn make_test_message(from_role: MessageRole, content: &str) -> Message {
    Message::new_with_context(
        format!("msg-{}", uuid::Uuid::now_v7()),
        None,                     // project_id
        None,                     // task_id
        "test-user".to_string(),  // from_id
        "test-agent".to_string(), // to_id
        from_role,                // from_role
        MessageRole::Agent,       // to_role
        MessageType::Text,        // message_type
        content.to_string(),      // content
        None,                     // file_type
        FileMeta::default(),      // file_meta
        None,                     // reply_to_id
        None,                     // root_id
        None,                     // organization_id
        "test-user".to_string(),  // created_by
    )
}

#[test]
fn user_message_maps_to_user_role() {
    let msg = make_test_message(MessageRole::User, "用户消息");
    let a2a_msg = message_to_a2a(&msg, "task-1");
    assert_eq!(a2a_msg.role, "user");
    assert_eq!(a2a_msg.task_id, Some("task-1".to_string()));
}

#[test]
fn agent_message_maps_to_agent_role() {
    let msg = make_test_message(MessageRole::Agent, "Agent 回复");
    let a2a_msg = message_to_a2a(&msg, "task-1");
    assert_eq!(a2a_msg.role, "agent");
}

#[test]
fn build_a2a_task_assembles_correctly() {
    let messages = vec![
        make_test_message(MessageRole::User, "问题"),
        make_test_message(MessageRole::Agent, "回答"),
    ];
    let task = build_a2a_task(
        "project-1",
        ProjectStatus::Completed,
        &messages,
        &[],
        Some("session-1".to_string()),
    );
    assert_eq!(task.id, "project-1");
    assert_eq!(task.session_id, Some("session-1".to_string()));
    assert!(matches!(task.status.state, A2aTaskState::Completed));
    assert_eq!(task.messages.len(), 2);
    assert_eq!(task.messages[0].role, "user");
    assert_eq!(task.messages[1].role, "agent");
}
