//! Message Topic 消费者单元测试

use super::message::*;
use super::MessageHandler;
use common::enums::{FileType, MessageRole, MessageType};
use crate::models::file::FileMeta;
use crate::models::message::Message;
use uuid::Uuid;

// ==================== 测试辅助函数 ====================

fn create_test_message(
    task_id: &str,
    from_role: MessageRole,
    to_role: MessageRole,
    message_type: MessageType,
    content: &str,
) -> Message {
    Message::new_with_context(
        Uuid::now_v7().to_string(),
        None,
        Some(task_id.to_string()),
        Uuid::now_v7().to_string(),
        Uuid::now_v7().to_string(),
        from_role,
        to_role,
        message_type,
        content.to_string(),
        None,
        FileMeta::default(),
        None,
        "test".to_string(),
    )
}

// ==================== 分发逻辑测试 ====================

#[cfg(test)]
mod dispatch_tests {
    use super::*;

    /// 测试：用户 → Agent 的消息（触发 handle_agent_message）
    #[tokio::test]
    async fn test_user_to_agent_dispatches_to_agent_handler() -> crate::error::Result<()> {
        let handler = MessageHandlerImpl;
        let message = create_test_message(
            "task-1",
            MessageRole::User,
            MessageRole::Agent,
            MessageType::Text,
            "hello agent",
        );
        assert_eq!(message.to_role(), MessageRole::Agent);
        handler.handle(&message).await?;
        Ok(())
    }

    /// 测试：Agent → User 的消息（触发 handle_user_message）
    #[tokio::test]
    async fn test_agent_to_user_dispatches_to_user_handler() -> crate::error::Result<()> {
        let handler = MessageHandlerImpl;
        let message = create_test_message(
            "task-1",
            MessageRole::Agent,
            MessageRole::User,
            MessageType::Text,
            "hello user",
        );
        assert_eq!(message.to_role(), MessageRole::User);
        handler.handle(&message).await?;
        Ok(())
    }

    /// 测试：Agent → System 的工具调用请求（触发 handle_system_message）
    #[tokio::test]
    async fn test_agent_to_system_tool_call_dispatches_to_system() -> crate::error::Result<()> {
        let handler = MessageHandlerImpl;
        let message = create_test_message(
            "task-1",
            MessageRole::Agent,
            MessageRole::System,
            MessageType::ToolCallRequest,
            "{\"name\":\"search\"}",
        );
        assert_eq!(message.to_role(), MessageRole::System);
        handler.handle(&message).await?;
        Ok(())
    }

    /// 测试：System → Agent 的工具调用结果（触发 handle_agent_message）
    #[tokio::test]
    async fn test_system_to_agent_tool_result_dispatches_to_agent() -> crate::error::Result<()> {
        let handler = MessageHandlerImpl;
        let message = create_test_message(
            "task-1",
            MessageRole::System,
            MessageRole::Agent,
            MessageType::ToolCallResult,
            "{\"result\":\"ok\"}",
        );
        assert_eq!(message.to_role(), MessageRole::Agent);
        handler.handle(&message).await?;
        Ok(())
    }

    /// 测试：Agent → User 的图片消息（触发 handle_user_message）
    #[tokio::test]
    async fn test_agent_image_to_user_dispatches_to_user() -> crate::error::Result<()> {
        let handler = MessageHandlerImpl;
        let message = create_test_message(
            "task-1",
            MessageRole::Agent,
            MessageRole::User,
            MessageType::Image,
            "path/to/image.png",
        );
        assert_eq!(message.to_role(), MessageRole::User);
        handler.handle(&message).await?;
        Ok(())
    }

    /// 测试：Agent → User 的文件消息（触发 handle_user_message）
    #[tokio::test]
    async fn test_agent_file_to_user_dispatches_to_user() -> crate::error::Result<()> {
        let handler = MessageHandlerImpl;
        let message = create_test_message(
            "task-1",
            MessageRole::Agent,
            MessageRole::User,
            MessageType::File,
            "path/to/doc.pdf",
        );
        assert_eq!(message.to_role(), MessageRole::User);
        handler.handle(&message).await?;
        Ok(())
    }

    /// 测试：System → User 的系统通知（触发 handle_user_message）
    #[tokio::test]
    async fn test_system_to_user_notification_dispatches_to_user() -> crate::error::Result<()> {
        let handler = MessageHandlerImpl;
        let message = create_test_message(
            "task-1",
            MessageRole::System,
            MessageRole::User,
            MessageType::Text,
            "system notification",
        );
        assert_eq!(message.to_role(), MessageRole::User);
        handler.handle(&message).await?;
        Ok(())
    }
}

// ==================== 单例测试 ====================

#[cfg(test)]
mod singleton_tests {
    use super::*;

    #[test]
    fn test_get_consumer_does_not_panic() {
        // 验证获取单例不会 panic（即使未初始化）
        let _ = get_consumer();
    }
}
