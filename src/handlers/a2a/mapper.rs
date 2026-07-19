//! A2A 协议实体 ↔ ai_orz 内部实体转换
//!
//! 所有转换逻辑集中在此模块，domain 层不感知 A2A 概念。

use chrono::Utc;
use common::api::a2a::{
    A2aArtifact, A2aMessage, A2aMessagePart, A2aTask, A2aTaskStatus, A2aTaskState,
};
use common::enums::ProjectStatus;

use crate::models::artifact::Artifact;
use crate::models::message::Message;

/// 将 ai_orz ProjectStatus 转为 A2A TaskState
pub fn project_status_to_a2a_state(status: ProjectStatus) -> A2aTaskState {
    match status {
        ProjectStatus::Active | ProjectStatus::PendingReview => A2aTaskState::Submitted,
        ProjectStatus::InProgress => A2aTaskState::Working,
        ProjectStatus::Completed => A2aTaskState::Completed,
        ProjectStatus::Archived => A2aTaskState::Canceled,
        ProjectStatus::Deleted => A2aTaskState::Failed,
    }
}

/// 将 ai_orz Message 转为 A2A Message
///
/// from_role=User → role="user"，其余 → role="agent"
pub fn message_to_a2a(msg: &Message, task_id: &str) -> A2aMessage {
    use common::enums::MessageRole;
    let role = if msg.from_role() == MessageRole::User {
        "user".to_string()
    } else {
        "agent".to_string()
    };

    A2aMessage {
        role,
        parts: vec![A2aMessagePart::Text {
            text: msg.content().to_string(),
        }],
        message_id: Some(msg.po.id.clone()),
        task_id: Some(task_id.to_string()),
    }
}

/// 将 ai_orz Artifact 转为 A2A Artifact
pub fn artifact_to_a2a(artifact: &Artifact) -> A2aArtifact {
    A2aArtifact {
        artifact_id: artifact.po.id.clone(),
        name: artifact.po.name.clone(),
        parts: vec![A2aMessagePart::Text {
            text: artifact.po.description.clone(),
        }],
    }
}

/// 构建 A2aTask
pub fn build_a2a_task(
    task_id: &str,
    project_status: ProjectStatus,
    messages: &[Message],
    artifacts: &[Artifact],
    session_id: Option<String>,
) -> A2aTask {
    let state = project_status_to_a2a_state(project_status);
    let a2a_messages: Vec<A2aMessage> = messages
        .iter()
        .map(|m| message_to_a2a(m, task_id))
        .collect();
    let a2a_artifacts: Vec<A2aArtifact> = artifacts.iter().map(artifact_to_a2a).collect();

    A2aTask {
        id: task_id.to_string(),
        session_id,
        status: A2aTaskStatus {
            state,
            timestamp: Utc::now().to_rfc3339(),
            message: None,
        },
        messages: a2a_messages,
        artifacts: a2a_artifacts,
        metadata: serde_json::Value::Null,
    }
}

/// 从 A2A Message 提取文本内容
///
/// 拼接所有 Text part，忽略 File/Data part
pub fn extract_text_from_a2a_message(msg: &common::api::a2a::A2aMessage) -> String {
    msg.parts
        .iter()
        .filter_map(|part| match part {
            A2aMessagePart::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
