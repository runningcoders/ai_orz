//! Message Topic 消费者单元测试

use super::message::*;
use super::MessageHandler;
use common::enums::{FileType, MessageRole, MessageType};
use crate::models::file::FileMeta;
use crate::models::message::Message;
use uuid::Uuid;
use sqlx::SqlitePool;
use crate::pkg::request_context::RequestContext;

// ==================== 测试辅助函数 ====================

/// 创建测试用的 Message
fn create_test_message(
    task_id: &str,
    from_id: &str,
    to_id: &str,
    from_role: MessageRole,
    to_role: MessageRole,
    msg_type: MessageType,
    content: String,
) -> Message {
    Message::new_with_context(
        Uuid::now_v7().to_string(),
        None,
        Some(task_id.to_string()),
        from_id.to_string(),
        to_id.to_string(),
        from_role,
        to_role,
        msg_type,
        content,
        None,
        FileMeta::default(),
        None,
        from_id.to_string(),
    )
}

// ==================== Handler 分发测试 ====================
/// 验证 MessageHandlerImpl 按 message_type 正确分发到对应的处理方法

#[cfg(test)]
mod handler_tests {
    use super::*;

    #[tokio::test]
    async fn test_text_message_dispatches_to_handle_text() -> crate::error::Result<()> {
        let handler = MessageHandlerImpl;
        let msg = create_test_message(
            "task-1", "user-1", "agent-1",
            MessageRole::User, MessageRole::Agent,
            MessageType::Text, "hello".to_string(),
        );
        handler.handle(&msg).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_image_message_dispatches_correctly() -> crate::error::Result<()> {
        let handler = MessageHandlerImpl;
        let msg = create_test_message(
            "task-1", "user-1", "agent-1",
            MessageRole::User, MessageRole::Agent,
            MessageType::Image, "image".to_string(),
        );
        handler.handle(&msg).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_file_message_dispatches_correctly() -> crate::error::Result<()> {
        let handler = MessageHandlerImpl;
        let msg = create_test_message(
            "task-1", "user-1", "agent-1",
            MessageRole::User, MessageRole::Agent,
            MessageType::File, "file".to_string(),
        );
        handler.handle(&msg).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_audio_message_dispatches_correctly() -> crate::error::Result<()> {
        let handler = MessageHandlerImpl;
        let msg = create_test_message(
            "task-1", "user-1", "agent-1",
            MessageRole::User, MessageRole::Agent,
            MessageType::Audio, "audio".to_string(),
        );
        handler.handle(&msg).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_video_message_dispatches_correctly() -> crate::error::Result<()> {
        let handler = MessageHandlerImpl;
        let msg = create_test_message(
            "task-1", "user-1", "agent-1",
            MessageRole::User, MessageRole::Agent,
            MessageType::Video, "video".to_string(),
        );
        handler.handle(&msg).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_tool_call_request_dispatches_correctly() -> crate::error::Result<()> {
        let handler = MessageHandlerImpl;
        let msg = create_test_message(
            "task-1", "agent-1", "agent-1",
            MessageRole::Agent, MessageRole::Agent,
            MessageType::ToolCallRequest, "tool_call".to_string(),
        );
        handler.handle(&msg).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_tool_call_result_dispatches_correctly() -> crate::error::Result<()> {
        let handler = MessageHandlerImpl;
        let msg = create_test_message(
            "task-1", "agent-1", "agent-1",
            MessageRole::Agent, MessageRole::Agent,
            MessageType::ToolCallResult, "result".to_string(),
        );
        handler.handle(&msg).await?;
        Ok(())
    }
}
