//! Message Delivery 单元测试

use crate::models::cortex_types::{ThinkResult, ToolDescriptor};
use crate::models::message::{Message, TaskAssignmentMessage, ToolCallMessage};
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::RequestContext;
use crate::service::dao::cortex::CortexDao;
use crate::service::dao::model_provider::{ModelProviderDao, ModelProviderQuery};
use crate::service::domain::message::{
    DeliverMessageCommand, MessageDomain, SendTaskAssignmentCommand, SendToAgentCommand,
    SendToUserCommand, SendToolCallRequestCommand, SendToolCallResultCommand,
    ToolCallExecutionOutcome, ToolCallTraceRef,
};
use common::enums::{MessageRole, MessageStatus, MessageType};
use common::error::Result;
use serde_json::json;
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

// ========== Mock 实现（跳过向量依赖）==========

/// Mock ModelProviderDao（返回 None，跳过向量搜索）
struct MockModelProviderDao;

#[async_trait::async_trait]
impl ModelProviderDao for MockModelProviderDao {
    async fn insert(&self, _ctx: RequestContext, _provider: &ModelProviderPo) -> Result<()> {
        Ok(())
    }
    async fn find_by_id(&self, _ctx: RequestContext, _id: &str) -> Result<Option<ModelProviderPo>> {
        Ok(None)
    }
    async fn query(
        &self,
        _ctx: RequestContext,
        _query: ModelProviderQuery,
    ) -> Result<common::api::PagedResult<ModelProviderPo>> {
        Ok(common::api::PagedResult {
            items: Vec::new(),
            total: 0,
        })
    }
    async fn find_all(&self, _ctx: RequestContext) -> Result<Vec<ModelProviderPo>> {
        Ok(Vec::new())
    }
    async fn update(&self, _ctx: RequestContext, _provider: &ModelProviderPo) -> Result<()> {
        Ok(())
    }
    async fn delete(&self, _ctx: RequestContext, _provider: &ModelProviderPo) -> Result<()> {
        Ok(())
    }
    async fn get_default_embedding_provider(
        &self,
        _ctx: RequestContext,
    ) -> Result<Option<ModelProviderPo>> {
        Ok(None)
    }

    async fn find_enabled_embedding_provider(
        &self,
        _ctx: RequestContext,
    ) -> Result<Option<ModelProviderPo>> {
        Ok(None)
    }
}

/// Mock CortexDao（跳过向量搜索）
struct MockCortexDao;

#[async_trait::async_trait]
impl CortexDao for MockCortexDao {
    async fn think(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
        _messages: &[crate::models::cortex_types::ChatMessage],
        _tools: &[ToolDescriptor],
    ) -> Result<ThinkResult> {
        Ok(ThinkResult::Final {
            content: "".to_string(),
            usage: crate::models::cortex_types::TokenUsage::default(),
        })
    }

    async fn embed(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
        _texts: &[String],
    ) -> Result<Vec<Vec<f32>>> {
        Ok(Vec::new())
    }
}

fn new_ctx(user_id: &str, pool: sqlx::SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx(user_id, pool)
}

/// 初始化所有渠道 DAO 单例
fn init_all_channel_daos() {
    crate::service::dao::lark::init();
    crate::service::dao::wechat::init();
    crate::service::dao::slack::init();
    crate::service::dao::email::init();
    crate::service::dao::webhook::init();
    crate::service::dao::a2a_callback::init();
}

/// 初始化测试环境（每个测试新建独立实例，保证测试隔离）
fn init_test_env(pool: SqlitePool) -> (Arc<dyn MessageDomain>, RequestContext) {
    let message_dao = crate::service::dao::message::new();
    let message_vector_dao = crate::service::dao::message::vector::new();
    // 初始化 Attachment DAO/DAL（每个测试独立临时目录）
    let tmp_dir = std::env::temp_dir().join(format!("ai_orz_test_{}", uuid::Uuid::now_v7()));
    let attachment_dao = crate::service::dao::attachment::new_with_attachments_dir(tmp_dir);
    let attachment_dal = crate::service::dal::attachment::new(attachment_dao);
    crate::service::dal::attachment::set_for_test(attachment_dal.clone());
    let cortex_dao: Arc<dyn CortexDao> = Arc::new(MockCortexDao);
    let model_provider_dao: Arc<dyn ModelProviderDao> = Arc::new(MockModelProviderDao);
    let message_dal = crate::service::dal::message::new(
        message_dao,
        message_vector_dao,
        cortex_dao,
        model_provider_dao,
    );
    crate::service::dao::message_channel::init();
    let message_channel_dao = crate::service::dao::message_channel::new();
    init_all_channel_daos(); // 初始化所有渠道 DAO 单例
    let message_channel_dal = crate::service::dal::message_channel::new(message_channel_dao);
    // 初始化 MessageChannel DAL 单例（用于测试中创建渠道）
    crate::service::dal::message_channel::init();
    let message_push_dal = crate::service::dal::message_push::dal();
    // 注入 Attachment DAL（测试中如果用不到附件，可保持真实 DAL 即可，因为它只会在 attachment_ids 非空时调用）
    let attachment_dal = crate::service::dal::attachment::dal();
    let domain = crate::service::domain::message::new(
        message_dal,
        message_channel_dal,
        message_push_dal,
        attachment_dal,
    );
    let ctx = new_ctx("admin", pool);
    (domain, ctx)
}

#[sqlx::test]
async fn test_send_to_agent_and_send_to_user(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);

    let project_id = Uuid::now_v7().to_string();
    let task_id = Uuid::now_v7().to_string();

    // 用户发送给 Agent
    let sent_to_agent = domain
        .delivery()
        .send_to_agent(
            ctx.clone(),
            SendToAgentCommand {
                from_id: "user-1",
                from_role: MessageRole::User,
                to_agent_id: "agent-1",
                content: "User message to agent",
                project_id: Some(&project_id),
                task_id: Some(&task_id),
                reply_to_id: None,
                attachment_ids: None,
                message_type: MessageType::Text,
            },
        )
        .await
        .unwrap();

    assert_eq!(sent_to_agent.po.from_id, "user-1");
    assert_eq!(sent_to_agent.po.to_id, "agent-1");
    assert_eq!(sent_to_agent.po.from_role, MessageRole::User);
    assert_eq!(sent_to_agent.po.to_role, MessageRole::Agent);
    assert_eq!(sent_to_agent.po.message_type, MessageType::Text);
    assert_eq!(sent_to_agent.po.content, "User message to agent");
    assert_eq!(sent_to_agent.po.project_id, Some(project_id.clone()));
    assert_eq!(sent_to_agent.po.task_id, Some(task_id.clone()));
    assert_eq!(sent_to_agent.po.status, MessageStatus::Pending);

    // Agent 发送给用户
    let sent_to_user = domain
        .delivery()
        .send_to_user(
            ctx.clone(),
            SendToUserCommand {
                from_agent_id: "agent-1",
                to_user_id: "user-1",
                content: "Agent reply to user",
                project_id: Some(&project_id),
                task_id: Some(&task_id),
                reply_to_id: None,
            },
        )
        .await
        .unwrap();

    assert_eq!(sent_to_user.po.from_id, "agent-1");
    assert_eq!(sent_to_user.po.to_id, "user-1");
    assert_eq!(sent_to_user.po.from_role, MessageRole::Agent);
    assert_eq!(sent_to_user.po.to_role, MessageRole::User);
    assert_eq!(sent_to_user.po.content, "Agent reply to user");
    assert_eq!(sent_to_user.po.status, MessageStatus::Pending);
}

#[sqlx::test]
async fn test_send_without_project_and_task(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);

    // 不关联项目和任务
    let sent = domain
        .delivery()
        .send_to_agent(
            ctx.clone(),
            SendToAgentCommand {
                from_id: "user-1",
                from_role: MessageRole::User,
                to_agent_id: "agent-1",
                content: "Direct message without context",
                project_id: None,
                task_id: None,
                reply_to_id: None,
                attachment_ids: None,
                message_type: MessageType::Text,
            },
        )
        .await
        .unwrap();

    assert_eq!(sent.po.project_id, None);
    assert_eq!(sent.po.task_id, None);
    assert_eq!(sent.po.content, "Direct message without context");

    // 可以查询到
    let found = domain
        .management()
        .get_by_id(ctx.clone(), &sent.po.id)
        .await
        .unwrap();
    assert!(found.is_some());
}

#[sqlx::test]
async fn test_send_to_agent_with_attachments_creates_attachment_messages(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);

    // 先创建两个附件（属于当前用户 admin）
    let att_dal = crate::service::dal::attachment::dal();
    let att1 = att_dal
        .create_from_text(
            ctx.clone(),
            crate::models::attachment::TextAttachmentCreate {
                file_name: "document.txt".to_string(),
                content: "test text content".to_string(),
                mime_type: None,
                purpose: Some("test".to_string()),
            },
        )
        .await
        .unwrap();
    let att2 = att_dal
        .create_from_upload(
            ctx.clone(),
            crate::models::attachment::AttachmentUpload {
                original_name: "image.png".to_string(),
                mime_type: "image/png".to_string(),
                purpose: "test".to_string(),
                bytes: b"fake-image-bytes".to_vec(),
            },
        )
        .await
        .unwrap();

    // 发送带附件的消息
    let att_ids = vec![att1.po.id.clone(), att2.po.id.clone()];
    let sent = domain
        .delivery()
        .send_to_agent(
            ctx.clone(),
            SendToAgentCommand {
                from_id: "admin",
                from_role: MessageRole::User,
                to_agent_id: "agent-1",
                content: "看这两份资料",
                project_id: None,
                task_id: None,
                reply_to_id: None,
                attachment_ids: Some(&att_ids),
                message_type: MessageType::Text,
            },
        )
        .await
        .unwrap();

    // 文本消息是 root
    assert_eq!(sent.po.message_type, MessageType::Text);
    assert_eq!(sent.po.content, "看这两份资料");
    let root_id = sent.po.id.clone();

    // 找到 2 条附件消息
    let msgs = domain
        .management()
        .query(
            ctx.clone(),
            crate::service::dao::message::MessageQuery {
                from_id: Some("admin".to_string()),
                to_id: Some("agent-1".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(msgs.len() >= 3, "应至少有 3 条消息，实际 {}", msgs.len());

    // 找到 2 条附件消息
    let image_msg = msgs
        .iter()
        .find(|m| m.po.message_type == MessageType::Image)
        .expect("应有一条图片附件消息");
    let file_msg = msgs
        .iter()
        .find(|m| m.po.message_type == MessageType::File)
        .expect("应有一条文件附件消息");
    // 附件消息的 reply_to_id 应指向文本消息
    assert_eq!(image_msg.po.reply_to_id.as_ref(), Some(&root_id));
    assert_eq!(file_msg.po.reply_to_id.as_ref(), Some(&root_id));
    // 附件消息的 root_id 应指向文本消息
    assert_eq!(image_msg.po.root_id.as_ref(), Some(&root_id));
    assert_eq!(file_msg.po.root_id.as_ref(), Some(&root_id));
    // 附件消息的 content 应为附件 ID
    assert_eq!(image_msg.po.content, att2.po.id);
    assert_eq!(file_msg.po.content, att1.po.id);
}

#[sqlx::test]
async fn test_send_tool_call_request_creates_pending_system_message(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);

    let request_message = domain
        .delivery()
        .send_tool_call_request(
            ctx.clone(),
            SendToolCallRequestCommand {
                request_id: "tool-request-003",
                tool_id: "tool-mcp-weather",
                tool_name: "weather_lookup",
                from_agent_id: "agent-003",
                to_executor_id: "tool-executor",
                project_id: Some("project-003"),
                task_id: Some("task-003"),
                reply_to_id: Some("parent-message-003"),
                args: json!({ "city": "Beijing" }),
            },
        )
        .await
        .unwrap();

    assert_eq!(
        request_message.po.message_type,
        MessageType::ToolCallRequest
    );
    assert_eq!(request_message.po.from_id, "agent-003");
    assert_eq!(request_message.po.to_id, "tool-executor");
    assert_eq!(request_message.po.from_role, MessageRole::Agent);
    assert_eq!(request_message.po.to_role, MessageRole::System);
    assert_eq!(
        request_message.po.project_id.as_deref(),
        Some("project-003")
    );
    assert_eq!(request_message.po.task_id.as_deref(), Some("task-003"));
    assert_eq!(
        request_message.po.reply_to_id.as_deref(),
        Some("parent-message-003")
    );
    assert_eq!(request_message.po.status, MessageStatus::Pending);

    let payload: ToolCallMessage = serde_json::from_str(&request_message.po.content).unwrap();
    assert_eq!(payload.request_id, "tool-request-003");
    assert_eq!(payload.tool_id, "tool-mcp-weather");
    assert_eq!(payload.tool_name, "weather_lookup");
    assert_eq!(payload.from_id, "agent-003");
    assert_eq!(payload.to_id, "tool-executor");
    assert_eq!(payload.project_id.as_deref(), Some("project-003"));
    assert_eq!(payload.task_id.as_deref(), Some("task-003"));
    assert_eq!(payload.reply_to_id.as_deref(), Some("parent-message-003"));
    assert_eq!(payload.args, Some(json!({ "city": "Beijing" })));
    assert!(payload.result.is_none());
    assert!(payload.is_success.is_none());
    assert!(payload.error_message.is_none());

    let found = domain
        .management()
        .get_by_id(ctx.clone(), request_message.id())
        .await
        .unwrap();
    assert!(found.is_some());
}

#[sqlx::test]
async fn test_send_tool_call_result_success_reuses_request_context(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);

    let request = ToolCallMessage::new_request(
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

    let request_content = serde_json::to_string(&request).unwrap();
    let request_message = Message::new_with_context(
        "message-tool-request-001".to_string(),
        request.project_id.clone(),
        request.task_id.clone(),
        request.from_id.clone(),
        request.to_id.clone(),
        MessageRole::Agent,
        MessageRole::System,
        MessageType::ToolCallRequest,
        request_content,
        None,
        Default::default(),
        request.reply_to_id.clone(),
        None, // root_id
        None, // organization_id
        request.from_id.clone(),
    );

    let result_message = domain
        .delivery()
        .send_tool_call_result(
            ctx.clone(),
            SendToolCallResultCommand {
                request_message: &request_message,
                outcome: ToolCallExecutionOutcome::Success {
                    result: json!({ "temperature": 23, "unit": "celsius" }),
                    result_file_meta: None,
                    trace_ref: Some(ToolCallTraceRef {
                        tool_id: "tool-mcp-weather".to_string(),
                        call_id: "trace-call-001".to_string(),
                    }),
                },
            },
        )
        .await
        .unwrap();

    assert_eq!(result_message.po.message_type, MessageType::ToolCallResult);
    assert_eq!(result_message.po.from_id, "tool-executor");
    assert_eq!(result_message.po.to_id, "agent-001");
    assert_eq!(result_message.po.from_role, MessageRole::System);
    assert_eq!(result_message.po.to_role, MessageRole::Agent);
    assert_eq!(result_message.po.project_id.as_deref(), Some("project-001"));
    assert_eq!(result_message.po.task_id.as_deref(), Some("task-001"));
    assert_eq!(
        result_message.po.reply_to_id.as_deref(),
        Some(request_message.id())
    );

    let payload: ToolCallMessage = serde_json::from_str(&result_message.po.content).unwrap();
    assert_eq!(payload.request_id, "tool-request-001");
    assert_eq!(payload.tool_id, "tool-mcp-weather");
    assert_eq!(payload.tool_name, "weather_lookup");
    assert_eq!(payload.from_id, "tool-executor");
    assert_eq!(payload.to_id, "agent-001");
    assert_eq!(payload.is_success, Some(true));
    assert_eq!(
        payload.result,
        Some(json!({ "temperature": 23, "unit": "celsius" }))
    );
    assert!(payload.error_message.is_none());

    assert_eq!(
        payload.trace_ref,
        Some(ToolCallTraceRef {
            tool_id: "tool-mcp-weather".to_string(),
            call_id: "trace-call-001".to_string(),
        })
    );

    let raw_payload: serde_json::Value = serde_json::from_str(&result_message.po.content).unwrap();
    assert_eq!(
        raw_payload.get("trace_ref"),
        Some(&json!({
            "tool_id": "tool-mcp-weather",
            "call_id": "trace-call-001"
        }))
    );
}

#[sqlx::test]
async fn test_send_tool_call_result_failure_reuses_request_context(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);

    let request = ToolCallMessage::new_request(
        "tool-request-002".to_string(),
        "tool-mcp-failing".to_string(),
        "failing_tool".to_string(),
        Some("project-002".to_string()),
        Some("task-002".to_string()),
        "agent-002".to_string(),
        "tool-executor".to_string(),
        None,
        json!({ "input": "safe-placeholder" }),
    );

    let request_content = serde_json::to_string(&request).unwrap();
    let request_message = Message::new_with_context(
        "message-tool-request-002".to_string(),
        request.project_id.clone(),
        request.task_id.clone(),
        request.from_id.clone(),
        request.to_id.clone(),
        MessageRole::Agent,
        MessageRole::System,
        MessageType::ToolCallRequest,
        request_content,
        None,
        Default::default(),
        request.reply_to_id.clone(),
        None, // root_id
        None, // organization_id
        request.from_id.clone(),
    );

    let result_message = domain
        .delivery()
        .send_tool_call_result(
            ctx.clone(),
            SendToolCallResultCommand {
                request_message: &request_message,
                outcome: ToolCallExecutionOutcome::Failure {
                    error_message: "tool execution failed".to_string(),
                    trace_ref: None,
                },
            },
        )
        .await
        .unwrap();

    assert_eq!(result_message.po.message_type, MessageType::ToolCallResult);
    assert_eq!(result_message.po.from_id, "tool-executor");
    assert_eq!(result_message.po.to_id, "agent-002");
    assert_eq!(result_message.po.from_role, MessageRole::System);
    assert_eq!(result_message.po.to_role, MessageRole::Agent);
    assert_eq!(result_message.po.project_id.as_deref(), Some("project-002"));
    assert_eq!(result_message.po.task_id.as_deref(), Some("task-002"));
    assert_eq!(
        result_message.po.reply_to_id.as_deref(),
        Some(request_message.id())
    );

    let payload: ToolCallMessage = serde_json::from_str(&result_message.po.content).unwrap();
    assert_eq!(payload.request_id, "tool-request-002");
    assert_eq!(payload.from_id, "tool-executor");
    assert_eq!(payload.to_id, "agent-002");
    assert_eq!(payload.is_success, Some(false));
    assert_eq!(
        payload.error_message.as_deref(),
        Some("tool execution failed")
    );
    assert!(payload.result.is_none());
    assert!(payload.trace_ref.is_none());
}

#[sqlx::test]
async fn test_send_tool_call_result_failure_can_include_trace_ref(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);

    let request = ToolCallMessage::new_request(
        "tool-request-failed-traced".to_string(),
        "tool-mcp-failing".to_string(),
        "failing_tool".to_string(),
        Some("project-failed-traced".to_string()),
        Some("task-failed-traced".to_string()),
        "agent-failed-traced".to_string(),
        "tool-executor".to_string(),
        None,
        json!({ "input": "safe-placeholder" }),
    );

    let request_message = Message::new_with_context(
        "message-tool-request-failed-traced".to_string(),
        request.project_id.clone(),
        request.task_id.clone(),
        request.from_id.clone(),
        request.to_id.clone(),
        MessageRole::Agent,
        MessageRole::System,
        MessageType::ToolCallRequest,
        serde_json::to_string(&request).unwrap(),
        None,
        Default::default(),
        request.reply_to_id.clone(),
        None, // root_id
        None, // organization_id
        request.from_id.clone(),
    );

    let result_message = domain
        .delivery()
        .send_tool_call_result(
            ctx.clone(),
            SendToolCallResultCommand {
                request_message: &request_message,
                outcome: ToolCallExecutionOutcome::Failure {
                    error_message: "tool execution failed after start".to_string(),
                    trace_ref: Some(ToolCallTraceRef {
                        tool_id: "tool-mcp-failing".to_string(),
                        call_id: "trace-call-failed-001".to_string(),
                    }),
                },
            },
        )
        .await
        .unwrap();

    let payload: ToolCallMessage = serde_json::from_str(&result_message.po.content).unwrap();
    assert_eq!(payload.is_success, Some(false));
    assert_eq!(
        payload.error_message.as_deref(),
        Some("tool execution failed after start")
    );

    assert_eq!(
        payload.trace_ref,
        Some(ToolCallTraceRef {
            tool_id: "tool-mcp-failing".to_string(),
            call_id: "trace-call-failed-001".to_string(),
        })
    );

    let raw_payload: serde_json::Value = serde_json::from_str(&result_message.po.content).unwrap();
    assert_eq!(
        raw_payload.get("trace_ref"),
        Some(&json!({
            "tool_id": "tool-mcp-failing",
            "call_id": "trace-call-failed-001"
        }))
    );
}

#[sqlx::test]
async fn test_send_tool_call_result_failure_does_not_leak_request_args(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);

    let request = ToolCallMessage::new_request(
        "tool-request-sensitive".to_string(),
        "tool-mcp-sensitive".to_string(),
        "sensitive_tool".to_string(),
        Some("project-sensitive".to_string()),
        Some("task-sensitive".to_string()),
        "agent-sensitive".to_string(),
        "tool-executor".to_string(),
        None,
        json!({
            "credential": "placeholder-value",
            "path": "/tmp/placeholder-input"
        }),
    );

    let request_message = Message::new_with_context(
        "message-tool-request-sensitive".to_string(),
        request.project_id.clone(),
        request.task_id.clone(),
        request.from_id.clone(),
        request.to_id.clone(),
        MessageRole::Agent,
        MessageRole::System,
        MessageType::ToolCallRequest,
        serde_json::to_string(&request).unwrap(),
        None,
        Default::default(),
        request.reply_to_id.clone(),
        None, // root_id
        None, // organization_id
        request.from_id.clone(),
    );

    let result_message = domain
        .delivery()
        .send_tool_call_result(
            ctx.clone(),
            SendToolCallResultCommand {
                request_message: &request_message,
                outcome: ToolCallExecutionOutcome::Failure {
                    error_message: "tool execution failed".to_string(),
                    trace_ref: None,
                },
            },
        )
        .await
        .unwrap();

    assert!(!result_message.po.content.contains("placeholder-value"));
    assert!(!result_message.po.content.contains("/tmp/placeholder-input"));

    let payload: ToolCallMessage = serde_json::from_str(&result_message.po.content).unwrap();
    assert_eq!(payload.is_success, Some(false));
    assert!(payload.args.is_none());
    assert_eq!(
        payload.error_message.as_deref(),
        Some("tool execution failed")
    );
}

#[sqlx::test]
async fn test_send_tool_call_result_large_success_uses_safe_inline_marker(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);

    let request = ToolCallMessage::new_request(
        "tool-request-large".to_string(),
        "tool-mcp-large".to_string(),
        "large_tool".to_string(),
        Some("project-large".to_string()),
        Some("task-large".to_string()),
        "agent-large".to_string(),
        "tool-executor".to_string(),
        None,
        json!({ "input": "safe-placeholder" }),
    );

    let request_message = Message::new_with_context(
        "message-tool-request-large".to_string(),
        request.project_id.clone(),
        request.task_id.clone(),
        request.from_id.clone(),
        request.to_id.clone(),
        MessageRole::Agent,
        MessageRole::System,
        MessageType::ToolCallRequest,
        serde_json::to_string(&request).unwrap(),
        None,
        Default::default(),
        request.reply_to_id.clone(),
        None, // root_id
        None, // organization_id
        request.from_id.clone(),
    );

    let huge_output = "x".repeat(70_000);
    let result_message = domain
        .delivery()
        .send_tool_call_result(
            ctx.clone(),
            SendToolCallResultCommand {
                request_message: &request_message,
                outcome: ToolCallExecutionOutcome::Success {
                    result: json!({ "output": huge_output }),
                    result_file_meta: None,
                    trace_ref: None,
                },
            },
        )
        .await
        .unwrap();

    assert!(result_message.po.content.len() < 8_000);
    let payload: ToolCallMessage = serde_json::from_str(&result_message.po.content).unwrap();
    assert_eq!(payload.is_success, Some(true));
    assert!(payload.args.is_none());
    assert_eq!(
        payload.result.as_ref().and_then(|v| v.get("truncated")),
        Some(&json!(true))
    );
    assert_eq!(
        payload.result.as_ref().and_then(|v| v.get("message")),
        Some(&json!("tool result exceeded inline message limit"))
    );
}

#[sqlx::test]
async fn test_deliver_message_to_channels(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let user_id = "test-delivery-user";
    let org_id = "test-org-delivery";

    use crate::models::message::Message;
    use crate::models::message_channel::{ChannelConfig, MessageChannel, MessageChannelPo};
    use common::enums::message_channel::ChannelType;
    use common::enums::{MessageRole, MessageType};

    // 创建两个启用的渠道
    for i in 0..2 {
        let po = MessageChannelPo::new(
            format!("delivery-channel-{}", i),
            org_id.to_string(),
            user_id.to_string(),
            None,
            ChannelType::Webhook,
            format!("Delivery Channel {}", i),
            Some(format!("https://example.com/webhook/{}", i)),
            None,
            None,
            ChannelConfig::default(),
            user_id.to_string(),
        );
        let channel = MessageChannel::from_po(po);
        // 直接使用 DAL 创建渠道（MessageManagement 已经不包含渠道管理功能）
        crate::service::dal::message_channel::dal()
            .create_channel(ctx.clone(), &channel)
            .await
            .unwrap();
    }

    // 创建一条测试消息
    let message_id = "test-message-001";
    use crate::models::file::FileMeta;
    let message = Message::new_with_context(
        message_id.to_string(),
        None,                 // project_id
        Some("".to_string()), // task_id
        "agent-1".to_string(),
        user_id.to_string(),
        MessageRole::Agent,
        MessageRole::User,
        MessageType::Text,
        "这是一条测试消息内容".to_string(),
        None,                // file_type
        FileMeta::default(), // file_meta
        None,                // reply_to_id
        None,                // root_id
        None,                // organization_id
        user_id.to_string(), // created_by
    );

    // 多渠道投递
    let result = domain
        .delivery()
        .deliver_message(
            ctx.clone(),
            DeliverMessageCommand {
                message: &message,
                user_id,
            },
        )
        .await
        .unwrap();

    // 验证投递结果
    assert_eq!(result.total, 2);
    // 因为渠道没有实际配置（webhook_url 等为空），所以会投递失败
    // 这是预期行为：渠道存在但配置无效会导致 failed
    assert_eq!(result.success, 0);
    assert_eq!(result.failed, 2);
}

#[sqlx::test]
async fn test_send_task_assignment(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);

    let task_message = domain
        .delivery()
        .send_task_assignment(
            ctx.clone(),
            SendTaskAssignmentCommand {
                task_id: "task-001",
                task_title: "完成项目文档",
                task_description: Some("编写项目技术文档和 API 文档"),
                from_id: "agent-manager",
                from_role: MessageRole::Agent,
                to_agent_id: "agent-worker",
                project_id: Some("project-001"),
            },
        )
        .await
        .unwrap();

    assert_eq!(task_message.po.message_type, MessageType::TaskAssignment);
    assert_eq!(task_message.po.from_id, "agent-manager");
    assert_eq!(task_message.po.to_id, "agent-worker");
    assert_eq!(task_message.po.from_role, MessageRole::Agent);
    assert_eq!(task_message.po.to_role, MessageRole::Agent);
    assert_eq!(task_message.po.project_id.as_deref(), Some("project-001"));
    assert_eq!(task_message.po.task_id.as_deref(), Some("task-001"));
    assert_eq!(task_message.po.status, MessageStatus::Pending);

    let payload: TaskAssignmentMessage = serde_json::from_str(&task_message.po.content).unwrap();
    assert_eq!(payload.task_id, "task-001");
    assert_eq!(payload.task_title, "完成项目文档");
    assert_eq!(
        payload.task_description.as_deref(),
        Some("编写项目技术文档和 API 文档")
    );
    assert_eq!(payload.project_id.as_deref(), Some("project-001"));
    assert_eq!(payload.from_id, "agent-manager");
    assert_eq!(payload.to_agent_id, "agent-worker");

    let found = domain
        .management()
        .get_by_id(ctx, task_message.id())
        .await
        .unwrap();
    assert!(found.is_some());
}

#[sqlx::test]
async fn test_send_task_assignment_from_user(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);

    let task_message = domain
        .delivery()
        .send_task_assignment(
            ctx.clone(),
            SendTaskAssignmentCommand {
                task_id: "task-user-assign",
                task_title: "用户分配的任务",
                task_description: None,
                from_id: "user-admin",
                from_role: MessageRole::User,
                to_agent_id: "agent-worker",
                project_id: None,
            },
        )
        .await
        .unwrap();

    assert_eq!(task_message.po.message_type, MessageType::TaskAssignment);
    assert_eq!(task_message.po.from_id, "user-admin");
    assert_eq!(task_message.po.to_id, "agent-worker");
    assert_eq!(task_message.po.from_role, MessageRole::User);
    assert_eq!(task_message.po.to_role, MessageRole::Agent);
    assert_eq!(task_message.po.project_id, None);
    assert_eq!(task_message.po.task_id.as_deref(), Some("task-user-assign"));

    let payload: TaskAssignmentMessage = serde_json::from_str(&task_message.po.content).unwrap();
    assert_eq!(payload.task_id, "task-user-assign");
    assert_eq!(payload.task_title, "用户分配的任务");
    assert_eq!(payload.task_description, None);
    assert_eq!(payload.project_id, None);
    assert_eq!(payload.from_id, "user-admin");
    assert_eq!(payload.to_agent_id, "agent-worker");
}
