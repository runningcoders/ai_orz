//! Message Topic 消费者单元测试

use super::message::*;
use super::MessageHandler;
use common::enums::{FileType, MessageRole, MessageType};
use crate::models::file::FileMeta;
use crate::models::message::Message;
use uuid::Uuid;

// ==================== 测试辅助函数 ====================

fn create_test_message(
    _task_id: &str,
    _from_id: &str,
    _to_id: &str,
    from_role: MessageRole,
    to_role: MessageRole,
    message_type: MessageType,
    content: String,
) -> Message {
    let id = Uuid::now_v7().to_string();
    let file_meta = FileMeta::default();
    Message::new_with_context(
        id,
        None,
        Some(_task_id.to_string()),
        _from_id.to_string(),
        _to_id.to_string(),
        from_role,
        to_role,
        message_type,
        content,
        Some(FileType::Document),
        file_meta,
        None,
        "admin".to_string(),
    )
}

// ==================== Handler 分发测试 ====================

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
            MessageType::Image, "image-data".to_string(),
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
            MessageType::File, "file-data".to_string(),
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
            MessageType::Audio, "audio-data".to_string(),
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
            MessageType::Video, "video-data".to_string(),
        );
        handler.handle(&msg).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_tool_call_request_dispatches_correctly() -> crate::error::Result<()> {
        let handler = MessageHandlerImpl;
        let msg = create_test_message(
            "task-1", "agent-1", "tool-1",
            MessageRole::Agent, MessageRole::Agent,
            MessageType::ToolCallRequest, "tool-call".to_string(),
        );
        handler.handle(&msg).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_tool_call_result_dispatches_correctly() -> crate::error::Result<()> {
        let handler = MessageHandlerImpl;
        let msg = create_test_message(
            "task-1", "tool-1", "agent-1",
            MessageRole::Agent, MessageRole::Agent,
            MessageType::ToolCallResult, "tool-result".to_string(),
        );
        handler.handle(&msg).await?;
        Ok(())
    }
}

// ==================== 单例测试 ====================

#[cfg(test)]
mod singleton_tests {
    use super::*;

    #[test]
    fn test_get_consumer_does_not_panic() {
        // 验证获取单例不会 panic
        let _ = get_consumer();
    }
}
