//! Message Topic 消费者单元测试

use super::MessageHandler;
use super::message::*;
use common::error::{Error, ErrorField, Result};
use crate::models::agent::Agent;
use crate::models::file::FileMeta;
use crate::models::memory::{Memory, MemoryTrace};
use crate::models::message::{Message, ToolCallMessage};
use crate::models::tool::{Tool, ToolCallTraceRef, ToolExecutionResult};
use crate::pkg::RequestContext;
use crate::service::dao::message::MessageQuery;
use crate::service::domain::message::{
    DeliverMessageCommand, DeliveryResult, MessageDelivery, MessageDomain, MessageManagement,
    SendToAgentCommand, SendToUserCommand, SendToolCallRequestCommand, SendToolCallResultCommand,
    ToolCallExecutionOutcome,
};
use crate::service::domain::runtime::{
    AwakeningResult, RuntimeAwakening, RuntimeDomain, RuntimeMemory, RuntimeToolExecution,
};
use async_trait::async_trait;
use common::enums::{MessageRole, MessageStatus, MessageType};
use rig::tool::ToolError;
use serde_json::{Value, json};
use std::fmt;
use std::sync::{Arc, Mutex};
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

fn create_tool_call_request_message() -> Message {
    let payload = ToolCallMessage::new_request(
        "tool-request-001".to_string(),
        "tool-mcp-weather".to_string(),
        "weather_lookup".to_string(),
        Some("project-001".to_string()),
        Some("task-001".to_string()),
        "agent-001".to_string(),
        "tool-executor".to_string(),
        Some("parent-message-001".to_string()),
        json!({ "city": "Shanghai" }),
    );

    Message::new_with_context(
        "message-tool-request-001".to_string(),
        payload.project_id.clone(),
        payload.task_id.clone(),
        payload.from_id.clone(),
        payload.to_id.clone(),
        MessageRole::Agent,
        MessageRole::System,
        MessageType::ToolCallRequest,
        serde_json::to_string(&payload).unwrap(),
        None,
        FileMeta::default(),
        payload.reply_to_id.clone(),
        "agent-001".to_string(),
    )
}

fn test_handler(
    runtime_domain: Arc<RecordingRuntimeDomain>,
    message_domain: Arc<RecordingMessageDomain>,
) -> MessageHandlerImpl {
    MessageHandlerImpl::new_for_test(runtime_domain, message_domain)
}

async fn init_storage_for_test() {
    crate::pkg::storage::init_for_test().await;
}

// ==================== Mock Domain ====================

struct RecordingRuntimeDomain {
    calls: Mutex<Vec<(String, Value)>>,
    manual_calls: Mutex<Vec<(String, String, Value)>>,
    result: Mutex<std::result::Result<ToolExecutionResult, common::error::Error>>,
}

impl RecordingRuntimeDomain {
    fn success(result: Value) -> Arc<Self> {
        Arc::new(Self {
            calls: Mutex::new(Vec::new()),
            manual_calls: Mutex::new(Vec::new()),
            result: Mutex::new(Ok(ToolExecutionResult::new(
                result,
                "tool-mcp-weather".to_string(),
                "trace-call-001".to_string(),
            ))),
        })
    }

    fn failure(error_message: &str) -> Arc<Self> {
        Arc::new(Self {
            calls: Mutex::new(Vec::new()),
            manual_calls: Mutex::new(Vec::new()),
            result: Mutex::new(Err(Error::tool_call_failed(error_message.to_string()))),
        })
    }

    fn failure_with_trace(error_message: &str, tool_id: &str, call_id: &str) -> Arc<Self> {
        let trace_ref = ToolCallTraceRef {
            tool_id: tool_id.to_string(),
            call_id: call_id.to_string(),
        };
        let mut field = ErrorField::new();
        field.set_trace_ref(trace_ref);
        let err = Error::tool_call_failed(error_message.to_string())
            .with_field(field);
        Arc::new(Self {
            calls: Mutex::new(Vec::new()),
            manual_calls: Mutex::new(Vec::new()),
            result: Mutex::new(Err(err)),
        })
    }

    fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len() + self.manual_calls.lock().unwrap().len()
    }

    fn legacy_call_by_id_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }

    fn first_manual_call(&self) -> (String, String, Value) {
        self.manual_calls.lock().unwrap()[0].clone()
    }
}

impl fmt::Debug for RecordingRuntimeDomain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RecordingRuntimeDomain")
            .finish_non_exhaustive()
    }
}

fn recorded_error_to_tool_execution_error(error_message: String) -> Error {
    let parts = error_message.split('|').collect::<Vec<_>>();
    if parts.len() == 3 {
        let trace_ref = ToolCallTraceRef::new(
            parts[1].to_string(),
            parts[2].to_string(),
        );
        let mut err = Error::tool_call_failed(parts[0].to_string());
        err.set_tool_trace(serde_json::json!(trace_ref));
        err
    } else {
        Error::tool_call_failed(error_message)
    }
}

impl RuntimeDomain for RecordingRuntimeDomain {
    fn memory(&self) -> &dyn RuntimeMemory {
        self
    }

    fn awakening(&self) -> &dyn RuntimeAwakening {
        self
    }

    fn tool_execution(&self) -> &dyn RuntimeToolExecution {
        self
    }
}

#[async_trait]
impl RuntimeMemory for RecordingRuntimeDomain {
    async fn get_recent_context(
        &self,
        _ctx: RequestContext,
        _agent_id: &str,
        _task_id: Option<&str>,
        _limit: usize,
    ) -> std::result::Result<Vec<Memory>, common::error::Error> {
        unimplemented!("not needed by message consumer tests")
    }

    async fn write_thinking_trace(
        &self,
        _ctx: RequestContext,
        _trace: MemoryTrace,
    ) -> std::result::Result<Memory, common::error::Error> {
        unimplemented!("not needed by message consumer tests")
    }
}

#[async_trait]
impl RuntimeAwakening for RecordingRuntimeDomain {
    async fn awaken(
        &self,
        _ctx: RequestContext,
        _agent: &Agent,
        _message: &Message,
    ) -> std::result::Result<AwakeningResult, common::error::Error> {
        unimplemented!("not needed by message consumer tests")
    }
}
#[async_trait]
impl RuntimeToolExecution for RecordingRuntimeDomain {
    async fn call_tool_by_id(
        &self,
        _ctx: RequestContext,
        tool_id: String,
        args: Value,
    ) -> std::result::Result<ToolExecutionResult, common::error::Error> {
        self.calls.lock().unwrap().push((tool_id, args));
        match self.result.lock().unwrap().clone() {
            Ok(result) => Ok(result),
            Err(mut err) => {
                // If msg contains |tool_id|call_id format, parse it and set trace_ref to error field
                let msg = err.msg.clone();
                let parts: Vec<_> = msg.split('|').collect();
                if parts.len() == 3 {
                    let trace_ref = crate::models::tool::ToolCallTraceRef::new(
                        parts[1].to_string(),
                        parts[2].to_string(),
                    );
                    if let Some(field) = err.field.as_mut() {
                        field.set_trace_ref(trace_ref);
                    } else {
                        let mut field = ErrorField::default();
                        field.set_trace_ref(trace_ref);
                        err.field = Some(field);
                    }
                    err.msg = parts[0].to_string();
                }
                Err(err)
            },
        }
    }

    async fn call_tool(
        &self,
        _ctx: RequestContext,
        tool: &Tool,
        args: Value,
    ) -> std::result::Result<ToolExecutionResult, common::error::Error> {
        self.calls.lock().unwrap().push((tool.po.id.clone(), args));
        match self.result.lock().unwrap().clone() {
            Ok(result) => Ok(result),
            Err(mut err) => {
                // If msg contains |tool_id|call_id format, parse it and set trace_ref to error field
                let msg = err.msg.clone();
                let parts: Vec<_> = msg.split('|').collect();
                if parts.len() == 3 {
                    let trace_ref = crate::models::tool::ToolCallTraceRef::new(
                        parts[1].to_string(),
                        parts[2].to_string(),
                    );
                    if let Some(field) = err.field.as_mut() {
                        field.set_trace_ref(trace_ref);
                    } else {
                        let mut field = ErrorField::default();
                        field.set_trace_ref(trace_ref);
                        err.field = Some(field);
                    }
                    err.msg = parts[0].to_string();
                }
                Err(err)
            },
        }
    }

    async fn call_manual_tool_for_agent(
        &self,
        _ctx: RequestContext,
        agent_id: String,
        tool_id: String,
        args: Value,
    ) -> std::result::Result<ToolExecutionResult, common::error::Error> {
        self.manual_calls.lock().unwrap().push((agent_id, tool_id, args));
        match self.result.lock().unwrap().clone() {
            Ok(result) => Ok(result),
            Err(mut err) => {
                // If msg contains |tool_id|call_id format, parse it and set trace_ref to error field
                let msg = err.msg.clone();
                let parts: Vec<_> = msg.split('|').collect();
                if parts.len() == 3 {
                    let trace_ref = crate::models::tool::ToolCallTraceRef::new(
                        parts[1].to_string(),
                        parts[2].to_string(),
                    );
                    if let Some(field) = err.field.as_mut() {
                        field.set_trace_ref(trace_ref);
                    } else {
                        let mut field = ErrorField::default();
                        field.set_trace_ref(trace_ref);
                        err.field = Some(field);
                    }
                    err.msg = parts[0].to_string();
                }
                Err(err)
            },
        }
    }

    async fn query_tool_call_entries(
        &self,
        _ctx: RequestContext,
        _query: crate::pkg::tool_tracing::logger::ToolCallQuery,
    ) -> std::result::Result<Vec<crate::pkg::tool_tracing::entry::ToolCallEntry>, common::error::Error> {
        unimplemented!("not needed by message consumer tests")
    }

    async fn get_tool_call_entry_by_id(
        &self,
        _ctx: RequestContext,
        _query: crate::pkg::tool_tracing::logger::ToolCallQuery,
    ) -> std::result::Result<Option<crate::pkg::tool_tracing::entry::ToolCallEntry>, common::error::Error> {
        unimplemented!("not needed by message consumer tests")
    }
}

struct RecordingMessageDomain {
    sent_results: Mutex<Vec<(String, ToolCallExecutionOutcome)>>,
}

impl RecordingMessageDomain {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            sent_results: Mutex::new(Vec::new()),
        })
    }

    fn result_count(&self) -> usize {
        self.sent_results.lock().unwrap().len()
    }

    fn first_result(&self) -> (String, ToolCallExecutionOutcome) {
        self.sent_results.lock().unwrap()[0].clone()
    }
}

impl MessageDomain for RecordingMessageDomain {
    fn delivery(&self) -> &dyn MessageDelivery {
        self
    }

    fn management(&self) -> &dyn MessageManagement {
        self
    }
}

#[async_trait]
impl MessageDelivery for RecordingMessageDomain {
    async fn send_to_agent(
        &self,
        _ctx: RequestContext,
        _cmd: SendToAgentCommand<'_>,
    ) -> std::result::Result<Message, common::error::Error> {
        unimplemented!("not needed by message consumer tests")
    }

    async fn send_to_user(
        &self,
        _ctx: RequestContext,
        _cmd: SendToUserCommand<'_>,
    ) -> std::result::Result<Message, common::error::Error> {
        unimplemented!("not needed by message consumer tests")
    }

    async fn send_tool_call_request(
        &self,
        _ctx: RequestContext,
        _cmd: SendToolCallRequestCommand<'_>,
    ) -> std::result::Result<Message, common::error::Error> {
        unimplemented!("not needed by message consumer tests")
    }

    async fn send_tool_call_result(
        &self,
        _ctx: RequestContext,
        cmd: SendToolCallResultCommand<'_>,
    ) -> std::result::Result<Message, common::error::Error> {
        self.sent_results
            .lock()
            .unwrap()
            .push((cmd.request_message.id().to_string(), cmd.outcome));
        Ok(cmd.request_message.clone())
    }

    async fn dequeue_next(
        &self,
        _ctx: RequestContext,
    ) -> std::result::Result<Option<Message>, common::error::Error> {
        unimplemented!("not needed by message consumer tests")
    }

    async fn ack(
        &self,
        _ctx: RequestContext,
        _message_id: &str,
    ) -> std::result::Result<(), common::error::Error> {
        unimplemented!("not needed by message consumer tests")
    }

    async fn nack(
        &self,
        _ctx: RequestContext,
        _message_id: &str,
    ) -> std::result::Result<(), common::error::Error> {
        unimplemented!("not needed by message consumer tests")
    }

    async fn deliver_message(
        &self,
        _ctx: RequestContext,
        _cmd: DeliverMessageCommand<'_>,
    ) -> std::result::Result<DeliveryResult, common::error::Error> {
        unimplemented!("not needed by message consumer tests")
    }
}

#[async_trait]
impl MessageManagement for RecordingMessageDomain {
    async fn query(
        &self,
        _ctx: RequestContext,
        _query: MessageQuery,
    ) -> std::result::Result<Vec<Message>, common::error::Error> {
        unimplemented!("not needed by message consumer tests")
    }

    async fn list_by_task_id(
        &self,
        _ctx: RequestContext,
        _task_id: &str,
    ) -> std::result::Result<Vec<Message>, common::error::Error> {
        unimplemented!("not needed by message consumer tests")
    }

    async fn list_by_project_id(
        &self,
        _ctx: RequestContext,
        _project_id: &str,
    ) -> std::result::Result<Vec<Message>, common::error::Error> {
        unimplemented!("not needed by message consumer tests")
    }

    async fn get_by_id(
        &self,
        _ctx: RequestContext,
        _message_id: &str,
    ) -> std::result::Result<Option<Message>, common::error::Error> {
        unimplemented!("not needed by message consumer tests")
    }

    async fn update_status(
        &self,
        _ctx: RequestContext,
        _message_id: &str,
        _status: MessageStatus,
    ) -> std::result::Result<(), common::error::Error> {
        unimplemented!("not needed by message consumer tests")
    }

    async fn delete_by_id(
        &self,
        _ctx: RequestContext,
        _message_id: &str,
    ) -> std::result::Result<(), common::error::Error> {
        unimplemented!("not needed by message consumer tests")
    }

    async fn cleanup_conversation(
        &self,
        _ctx: RequestContext,
        _task_id: &str,
    ) -> std::result::Result<(), common::error::Error> {
        unimplemented!("not needed by message consumer tests")
    }
}

// ==================== 分发逻辑测试 ====================

#[cfg(test)]
mod dispatch_tests {
    use super::*;

    fn noop_handler() -> MessageHandlerImpl {
        test_handler(
            RecordingRuntimeDomain::success(json!({ "ok": true })),
            RecordingMessageDomain::new(),
        )
    }

    /// 测试：用户 → Agent 的消息（触发 handle_agent_message）
    #[tokio::test]
    async fn test_user_to_agent_dispatches_to_agent_handler() -> Result<()> {
        let handler = noop_handler();
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
    async fn test_agent_to_user_dispatches_to_user_handler() -> Result<()> {
        let handler = noop_handler();
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

    /// 测试：Agent → System 的工具调用请求（触发 ToolCallRequest 编排）
    #[tokio::test]
    async fn test_agent_to_system_tool_call_dispatches_to_system() -> Result<()> {
        init_storage_for_test().await;
        let runtime_domain = RecordingRuntimeDomain::success(json!({ "ok": true }));
        let message_domain = RecordingMessageDomain::new();
        let handler = test_handler(runtime_domain.clone(), message_domain.clone());
        let message = create_tool_call_request_message();

        assert_eq!(message.to_role(), MessageRole::System);
        handler.handle(&message).await?;

        assert_eq!(runtime_domain.call_count(), 1);
        assert_eq!(message_domain.result_count(), 1);
        Ok(())
    }

    /// 测试：System → Agent 的工具调用结果（触发 handle_agent_message）
    #[tokio::test]
    async fn test_system_to_agent_tool_result_dispatches_to_agent() -> Result<()> {
        let handler = noop_handler();
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
    async fn test_agent_image_to_user_dispatches_to_user() -> Result<()> {
        let handler = noop_handler();
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
    async fn test_agent_file_to_user_dispatches_to_user() -> Result<()> {
        let handler = noop_handler();
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
    async fn test_system_to_user_notification_dispatches_to_user() -> Result<()> {
        let handler = noop_handler();
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

// ==================== ToolCallRequest 编排测试 ====================

#[cfg(test)]
mod tool_call_request_tests {
    use super::*;

    #[tokio::test]
    async fn test_tool_call_request_success_sends_success_result() -> Result<()> {
        init_storage_for_test().await;
        let runtime_domain = RecordingRuntimeDomain::success(json!({ "temperature": 23 }));
        let message_domain = RecordingMessageDomain::new();
        let handler = test_handler(runtime_domain.clone(), message_domain.clone());
        let message = create_tool_call_request_message();

        handler.handle(&message).await?;

        assert_eq!(runtime_domain.call_count(), 1);
        assert_eq!(runtime_domain.legacy_call_by_id_count(), 0);
        let (agent_id, tool_id, args) = runtime_domain.first_manual_call();
        assert_eq!(agent_id, "agent-001");
        assert_eq!(tool_id, "tool-mcp-weather");
        assert_eq!(args, json!({ "city": "Shanghai" }));

        assert_eq!(message_domain.result_count(), 1);
        let (request_message_id, outcome) = message_domain.first_result();
        assert_eq!(request_message_id, message.id());
        match outcome {
            ToolCallExecutionOutcome::Success {
                result,
                result_file_meta,
                trace_ref,
            } => {
                assert_eq!(result, json!({ "temperature": 23 }));
                assert!(result_file_meta.is_none());
                assert_eq!(
                    trace_ref,
                    Some(ToolCallTraceRef {
                        tool_id: "tool-mcp-weather".to_string(),
                        call_id: "trace-call-001".to_string(),
                    })
                );
            }
            ToolCallExecutionOutcome::Failure { error_message, .. } => {
                panic!("expected success result, got failure: {}", error_message);
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_tool_call_request_runtime_failure_sends_failure_result_and_acks_request()
    -> Result<()> {
        init_storage_for_test().await;
        let runtime_domain =
            RecordingRuntimeDomain::failure("MCP tool call failed for tool_id: tool-mcp-weather");
        let message_domain = RecordingMessageDomain::new();
        let handler = test_handler(runtime_domain.clone(), message_domain.clone());
        let message = create_tool_call_request_message();

        handler.handle(&message).await?;

        assert_eq!(runtime_domain.call_count(), 1);
        assert_eq!(message_domain.result_count(), 1);
        let (request_message_id, outcome) = message_domain.first_result();
        assert_eq!(request_message_id, message.id());
        match outcome {
            ToolCallExecutionOutcome::Success { result, .. } => {
                panic!("expected failure result, got success: {}", result);
            }
            ToolCallExecutionOutcome::Failure {
                error_message,
                trace_ref,
            } => {
                assert_eq!(
                    error_message,
                    "MCP tool call failed for tool_id: tool-mcp-weather"
                );
                assert!(trace_ref.is_none());
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_tool_call_request_started_failure_preserves_trace_ref() -> Result<()> {
        init_storage_for_test().await;
        let runtime_domain = RecordingRuntimeDomain::failure_with_trace(
            "MCP tool call failed after execution started",
            "tool-mcp-weather",
            "real-call-999",
        );
        let message_domain = RecordingMessageDomain::new();
        let handler = test_handler(runtime_domain.clone(), message_domain.clone());
        let message = create_tool_call_request_message();

        handler.handle(&message).await?;

        assert_eq!(runtime_domain.call_count(), 1);
        assert_eq!(message_domain.result_count(), 1);
        let (_request_message_id, outcome) = message_domain.first_result();
        match outcome {
            ToolCallExecutionOutcome::Success { result, .. } => {
                panic!("expected failure result, got success: {}", result);
            }
            ToolCallExecutionOutcome::Failure {
                error_message,
                trace_ref,
            } => {
                assert_eq!(
                    error_message,
                    "MCP tool call failed after execution started"
                );
                assert_eq!(
                    trace_ref,
                    Some(ToolCallTraceRef {
                        tool_id: "tool-mcp-weather".to_string(),
                        call_id: "real-call-999".to_string(),
                    })
                );
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_tool_call_request_invalid_content_returns_error_for_nack() {
        init_storage_for_test().await;
        let runtime_domain = RecordingRuntimeDomain::success(json!({ "ok": true }));
        let message_domain = RecordingMessageDomain::new();
        let handler = test_handler(runtime_domain.clone(), message_domain.clone());
        let message = create_test_message(
            "task-1",
            MessageRole::Agent,
            MessageRole::System,
            MessageType::ToolCallRequest,
            "not-json",
        );

        let result = handler.handle(&message).await;

        assert!(result.is_err());
        assert_eq!(runtime_domain.call_count(), 0);
        assert_eq!(message_domain.result_count(), 0);
    }

    #[tokio::test]
    async fn test_non_tool_call_system_message_is_ignored() -> Result<()> {
        init_storage_for_test().await;
        let runtime_domain = RecordingRuntimeDomain::success(json!({ "ok": true }));
        let message_domain = RecordingMessageDomain::new();
        let handler = test_handler(runtime_domain.clone(), message_domain.clone());
        let message = create_test_message(
            "task-1",
            MessageRole::Agent,
            MessageRole::System,
            MessageType::Text,
            "system maintenance",
        );

        handler.handle(&message).await?;

        assert_eq!(runtime_domain.call_count(), 0);
        assert_eq!(message_domain.result_count(), 0);
        Ok(())
    }
}

// ==================== 单例测试 ====================

#[cfg(test)]
mod singleton_tests {
    use super::*;
use common::bail_err;

    #[test]
    fn test_get_consumer_does_not_panic() {
        // 验证获取单例不会 panic（即使未初始化）
        let _ = get_consumer();
    }
}
