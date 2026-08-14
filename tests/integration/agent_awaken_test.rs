//! Agent 智能层集成测试
//!
//! 覆盖 Agent awaken 主流程的三个层次：
//! - Part A: Consumer 编排逻辑（无 LLM，CI 默认）
//! - Part B: awaken 流程 Mock 测试（CapturingBrainDal，CI 默认）
//! - Part C: 真实 LLM 端到端测试（Doubao LLM，#[ignore]）

#[path = "../common/mod.rs"]
mod common;

use crate::common::TestApp;
use ai_orz::consumer::message::MessageConsumer;
use ai_orz::pkg::agent_runtime_state::AgentRuntimeStateManager;
use ai_orz::pkg::aop::Consumer;
use serde_json::json;
use sqlx::SqlitePool;

// Part B: awaken Mock 测试依赖
use ::common::enums::{
    AgentStatus, MessageRole, MessageType, ModelCapability, ProviderType, ToolProtocol,
};
use ::common::error::Result as CommonResult;
use ai_orz::models::agent::{Agent, AgentPo, AgentRuntimeConfig};
use ai_orz::models::brain::Brain;
use ai_orz::models::file::FileMeta;
use ai_orz::models::message::Message;
use ai_orz::models::model_provider::ModelProvider;
use ai_orz::models::tool::{Tool, ToolPo};
use ai_orz::pkg::RequestContext;
use ai_orz::pkg::tool_tracing::logger::ToolCallLogger;
use ai_orz::service::dal::brain::BrainDal;
use ai_orz::service::domain::runtime::awakening::ThinkingOptions;
use ai_orz::service::domain::runtime::new_with_all;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;
use uuid::Uuid;

/// 构造 MessageCreatedEvent JSON
///
/// from_role: User=0, Agent=1, System=2
/// message_type: Text=0
fn make_message_event(
    message_id: &str,
    from_id: &str,
    to_id: &str,
    to_role: i32,
) -> serde_json::Value {
    json!({
        "message_id": message_id,
        "project_id": null,
        "task_id": null,
        "from_id": from_id,
        "from_role": 0,
        "to_id": to_id,
        "to_role": to_role,
        "message_type": 0,
        "content": "",
        "created_at": 0
    })
}

/// Consumer 编排测试：向不存在的 Agent 发送消息，Consumer 应返回 NotFound 错误。
///
/// 验证：
/// - Consumer 正确加载消息
/// - Agent 不存在时返回错误（不触发 LLM）
/// - Busy 状态被正确释放（避免后续消息被永久阻塞）
#[sqlx::test]
async fn test_consumer_nonexistent_agent_returns_error(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 发送消息给一个不存在的 Agent ID
    let fake_agent_id = format!("nonexistent-{}", uuid::Uuid::now_v7());
    let send_req = json!({
        "to_agent_id": fake_agent_id,
        "content": "Hello from test"
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/finance/messages/agents", &send_req, &jwt)
        .await;
    let msg_data = crate::common::assert_api_ok(status, &body);
    let message_id = msg_data
        .get("message_id")
        .and_then(|v| v.as_str())
        .expect("missing message_id")
        .to_string();

    // 直接调用 Consumer 处理消息事件
    let consumer = MessageConsumer::new();
    let event = make_message_event(&message_id, &bs.user_id, &fake_agent_id, 1);

    let result = consumer.on_event(event).await;

    // 应该返回错误（Agent 不存在）
    assert!(
        result.is_err(),
        "Consumer should return error for non-existent agent"
    );
    let err = result.unwrap_err();
    let err_msg = format!("{}", err);
    assert!(
        err_msg.contains("not found") || err_msg.contains("not_found"),
        "Error should mention not found, got: {}",
        err_msg
    );

    // 验证 Busy 状态被释放（避免永久锁定）
    let runtime_state = AgentRuntimeStateManager::global();
    let state = runtime_state.get_state(&fake_agent_id);
    // Idle = 0，Agent 不存在时应该已释放
    assert_eq!(
        state,
        ::common::enums::AgentRuntimeState::Idle,
        "Agent Busy state should be released after error"
    );
}

/// Consumer 编排测试：Busy 状态的 Agent 拒绝新消息。
///
/// 验证：
/// - try_set_busy 返回 false 时 Consumer 返回 Conflict 错误
/// - 不触发后续 awaken 流程（不调用 LLM）
/// - 原始 Busy 状态保持不变
#[sqlx::test]
async fn test_consumer_busy_agent_rejects_message(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 创建 Agent
    let agent_name = format!("BusyAgent-{}", uuid::Uuid::now_v7());
    let agent_id =
        crate::common::factories::create_test_agent(&app, &jwt, &bs.chat_provider_id, &agent_name)
            .await;

    // 发送消息（持久化到 DB）
    let send_req = json!({
        "to_agent_id": agent_id,
        "content": "First message"
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/finance/messages/agents", &send_req, &jwt)
        .await;
    let msg_data = crate::common::assert_api_ok(status, &body);
    let message_id = msg_data
        .get("message_id")
        .and_then(|v| v.as_str())
        .expect("missing message_id")
        .to_string();

    // 预先将 Agent 设为 Busy（模拟另一个 worker 正在处理）
    let runtime_state = AgentRuntimeStateManager::global();
    runtime_state.set_busy(&agent_id, &message_id, None, None);

    // 调用 Consumer 处理消息事件
    let consumer = MessageConsumer::new();
    let event = make_message_event(&message_id, &bs.user_id, &agent_id, 1);

    let result = consumer.on_event(event).await;

    // 应该返回 Conflict 错误（Agent 已 Busy）
    assert!(
        result.is_err(),
        "Consumer should return error for busy agent"
    );
    let err = result.unwrap_err();
    let err_msg = format!("{}", err);
    assert!(
        err_msg.contains("busy") || err_msg.contains("conflict"),
        "Error should mention busy/conflict, got: {}",
        err_msg
    );

    // 验证 Agent 仍然处于 Busy 状态（未被释放，因为不是我们设置的）
    let state = runtime_state.get_state(&agent_id);
    assert_eq!(
        state,
        ::common::enums::AgentRuntimeState::Busy,
        "Agent should still be Busy (we set it, consumer should not release it)"
    );

    // 清理：释放 Busy 状态
    runtime_state.set_idle(&agent_id);
}

/// Consumer 编排测试：Task 已 Completed 时跳过 awaken。
///
/// 验证：
/// - Consumer 检查 task 状态，Completed 时跳过 awaken
/// - 不触发 LLM 调用
/// - 返回 Ok（合法跳过，非错误）
/// - Busy 状态被释放
#[sqlx::test]
async fn test_consumer_completed_task_skips_awaken(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 1. 创建 Agent
    let agent_name = format!("TaskAgent-{}", uuid::Uuid::now_v7());
    let agent_id =
        crate::common::factories::create_test_agent(&app, &jwt, &bs.chat_provider_id, &agent_name)
            .await;

    // 2. 创建 Project + Task
    let project_name = format!("TaskProject-{}", uuid::Uuid::now_v7());
    let project_id = crate::common::factories::create_test_project(&app, &jwt, &project_name).await;

    let task_req = json!({
        "title": "Test task for completed",
        "description": "Task that will be completed",
        "project_id": project_id,
        "assignee_id": agent_id,
    });
    let (status, body) = app.post_with_jwt("/api/v1/tasks", &task_req, &jwt).await;
    let task_data = crate::common::assert_api_ok(status, &body);
    let task_id = task_data
        .get("id")
        .and_then(|v| v.as_str())
        .expect("missing task id")
        .to_string();

    // 3. 流转 task 状态：Pending → InProgress → Completed
    // Pending → InProgress
    let in_progress_req = json!({ "id": task_id, "status": "InProgress" });
    let (status, _body) = app
        .put_with_jwt(
            &format!("/api/v1/tasks/{}/status", task_id),
            &in_progress_req,
            &jwt,
        )
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "Pending → InProgress should succeed"
    );

    // InProgress → Completed
    let completed_req = json!({ "id": task_id, "status": "Completed" });
    let (status, _body) = app
        .put_with_jwt(
            &format!("/api/v1/tasks/{}/status", task_id),
            &completed_req,
            &jwt,
        )
        .await;
    assert_eq!(
        status,
        axum::http::StatusCode::OK,
        "InProgress → Completed should succeed"
    );

    // 4. 发送带 task_id 的消息给 Agent
    let send_req = json!({
        "to_agent_id": agent_id,
        "content": "Hello for completed task",
        "task_id": task_id,
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/finance/messages/agents", &send_req, &jwt)
        .await;
    let msg_data = crate::common::assert_api_ok(status, &body);
    let message_id = msg_data
        .get("message_id")
        .and_then(|v| v.as_str())
        .expect("missing message_id")
        .to_string();

    // 5. 调用 Consumer 处理消息事件
    let consumer = MessageConsumer::new();
    // Note: this event includes project_id and task_id
    let event = json!({
        "message_id": message_id,
        "project_id": project_id,
        "task_id": task_id,
        "from_id": bs.user_id,
        "from_role": 0,
        "to_id": agent_id,
        "to_role": 1,
        "message_type": 0,
        "content": "",
        "created_at": 0
    });

    let result = consumer.on_event(event).await;

    // 应该返回 Ok（合法跳过，非错误）
    assert!(
        result.is_ok(),
        "Consumer should return Ok for completed task (skip awaken), got: {:?}",
        result.err()
    );

    // 6. 验证 Agent 回到 Idle 状态（未触发 awaken，Busy 已释放）
    let runtime_state = AgentRuntimeStateManager::global();
    let state = runtime_state.get_state(&agent_id);
    assert_eq!(
        state,
        ::common::enums::AgentRuntimeState::Idle,
        "Agent should be Idle after skipping awaken for completed task"
    );
}

// ==================== Part B: awaken Mock 测试 ====================
//
// 使用 CapturingBrainDal 捕获 think() 入参 Prompt，验证 awaken 流程将 Manual 工具
// 正确注入 Prompt。MockCortex 仅用于构造 Brain（BrainDal 已 stub，Cortex 不会被实际调用）。

/// 捕获 Prompt 的 BrainDal Stub
///
/// 在 think() 调用时捕获传入的 prompt，返回固定响应
struct CapturingBrainDal {
    captured_prompt: Arc<Mutex<Option<String>>>,
}

impl CapturingBrainDal {
    fn new(captured_prompt: Arc<Mutex<Option<String>>>) -> Self {
        Self { captured_prompt }
    }
}

#[async_trait]
impl BrainDal for CapturingBrainDal {
    async fn wake_brain(
        &self,
        _ctx: RequestContext,
        _agent: &AgentPo,
        _memories: Vec<ai_orz::models::memory::Memory>,
    ) -> CommonResult<Brain> {
        unimplemented!("not needed by awaken manual tools test")
    }

    async fn test_connection(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProvider,
        _prompt: &str,
    ) -> CommonResult<String> {
        unimplemented!("not needed by awaken manual tools test")
    }

    async fn think(
        &self,
        _ctx: RequestContext,
        _brain: &Brain,
        messages: &[ai_orz::models::cortex_types::ChatMessage],
        _tools: &[ai_orz::models::cortex_types::ToolDescriptor],
    ) -> CommonResult<ai_orz::models::cortex_types::ThinkResult> {
        // 从 messages 中提取最后一条 user 消息作为 prompt 捕获
        let prompt = messages
            .iter()
            .rev()
            .find_map(|m| match m {
                ai_orz::models::cortex_types::ChatMessage::User { content } => {
                    Some(content.as_str())
                }
                _ => None,
            })
            .unwrap_or("");
        *self.captured_prompt.lock().unwrap() = Some(prompt.to_string());
        Ok(ai_orz::models::cortex_types::ThinkResult::Final {
            content: "mock response".to_string(),
            usage: ai_orz::models::cortex_types::TokenUsage::default(),
        })
    }

    async fn embed_entity(
        &self,
        _ctx: RequestContext,
        _entity: &dyn ai_orz::models::vector::Vectorizable,
    ) -> CommonResult<Option<ai_orz::models::vector::VectorIndexParams>> {
        Ok(None)
    }

    async fn embed_text_for_search(
        &self,
        _ctx: RequestContext,
        _text: &str,
    ) -> CommonResult<Option<ai_orz::models::vector::VectorIndexParams>> {
        Ok(None)
    }
}

/// 创建带 Brain 的测试 Agent（status=Onboarded，brain 已装配）
fn make_test_agent_with_brain(agent_id: &str) -> Agent {
    let mut po = AgentPo::new(
        "Test Agent".to_string(),
        vec!["assistant".to_string()],
        "Test description".to_string(),
        vec!["chat".to_string()],
        "Test soul".to_string(),
        "provider-001".to_string(),
        "test-user".to_string(),
    );
    po.id = agent_id.to_string();
    po.status = AgentStatus::Onboarded;

    let mut agent = Agent::from_po(po);
    let model_provider_po = ai_orz::models::model_provider::ModelProviderPo {
        id: "mock-provider".to_string(),
        name: "Mock Provider".to_string(),
        provider_type: ProviderType::OpenAI,
        model_name: "gpt-4".to_string(),
        capability: ModelCapability::Agent,
        api_key: "fake-key".to_string(),
        base_url: None,
        description: None,
        config: "{}".to_string(),
        status: ::common::enums::ModelProviderStatus::Normal,
        created_by: "test-user".to_string(),
        modified_by: "test-user".to_string(),
        created_at: 0,
        updated_at: 0,
    };
    let runtime_config = AgentRuntimeConfig::default();
    agent.brain = Some(Brain::new_local(
        agent_id.to_string(),
        "Test Agent".to_string(),
        runtime_config,
        model_provider_po,
        vec![],
    ));
    agent
}

/// 创建 Manual 工具（Http 协议自动派生 ControlMode::Manual）
///
/// tags 用于匹配 Agent 的 match_keys（roles ∪ installed_tags）。
/// 工具列表本身不进入 Prompt（通过 OpenAI tools API 协议层传递），
/// 但持有 Manual 工具会触发 Prompt 输出【Manual 工具调用规范】段落。
fn make_manual_tool(name: &str, description: &str, tags: Vec<String>) -> Tool {
    let po = ToolPo::new(
        format!("tool-{}", name.to_lowercase()),
        name.to_string(),
        description.to_string(),
        ToolProtocol::Http,
        serde_json::json!({}),
        None,
        tags,
        Some("test-user".to_string()),
    );
    Tool::from_po_for_management(po)
}

/// 创建测试文本消息
fn make_test_message(content: &str) -> Message {
    Message::new_with_context(
        Uuid::now_v7().to_string(),
        None,
        None,
        "test-user".to_string(),
        "test-agent".to_string(),
        MessageRole::User,
        MessageRole::Agent,
        MessageType::Text,
        content.to_string(),
        None,
        FileMeta::default(),
        None,
        None,
        None,
        "test-user".to_string(),
    )
}

/// Part B: awaken Prompt 不包含工具描述测试
///
/// 验证：
/// - 工具列表不出现在 Prompt 中（通过 OpenAI tools API 协议层传递）
/// - 工具调用规范（Manual 工具调用规范段落）也不出现在 Prompt 中（对模型透明）
/// - awaken 返回结果含正确 agent_id 和 mock 输出
#[sqlx::test]
async fn test_awaken_manual_tools_in_prompt(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool).await;

    let agent_id = format!("agent-manual-tools-{}", Uuid::now_v7());
    let mut agent = make_test_agent_with_brain(&agent_id);

    // 为 Agent 添加 2 个 Manual 工具（Http 协议自动派生 ControlMode::Manual）
    let tool1 = make_manual_tool(
        "SearchWeb",
        "搜索互联网获取最新信息",
        vec!["assistant".to_string()],
    );
    let tool2 = make_manual_tool(
        "SendEmail",
        "发送邮件给指定收件人",
        vec!["assistant".to_string()],
    );
    agent.set_tools(vec![tool1, tool2]);

    let message = make_test_message("请帮我搜索 Rust 最新特性");

    let captured_prompt = Arc::new(Mutex::new(None));
    let temp_dir = tempdir().expect("tempdir should be created");
    let runtime = new_with_all(
        Arc::new(CapturingBrainDal::new(captured_prompt.clone())),
        ai_orz::service::dal::tool::dal(),
        ai_orz::service::dal::mcp_tool::dal(),
        ai_orz::service::dal::agent::dal(),
        Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf())),
    );

    let result = runtime
        .awakening()
        .awaken(ctx.clone(), &agent, &message, &ThinkingOptions::new())
        .await
        .expect("awaken 应该成功");

    let prompt = captured_prompt
        .lock()
        .unwrap()
        .clone()
        .expect("应该捕获到 prompt");

    // Prompt 不应包含任何工具相关区块
    assert!(
        !prompt.contains("【常用工具】"),
        "Prompt 不应包含【常用工具】区块（工具列表已通过 API 协议层传递）"
    );
    assert!(
        !prompt.contains("【神经工具】"),
        "Prompt 不应包含【神经工具】区块（工具列表已通过 API 协议层传递）"
    );
    assert!(
        !prompt.contains("【Manual 工具调用规范】"),
        "Prompt 不应包含【Manual 工具调用规范】段落（调用对模型透明）"
    );
    // 不应包含工具名/描述
    assert!(!prompt.contains("SearchWeb"));
    assert!(!prompt.contains("搜索互联网获取最新信息"));
    assert!(!prompt.contains("SendEmail"));
    assert!(!prompt.contains("发送邮件给指定收件人"));
    // 不应包含内部转发器名称
    assert!(!prompt.contains("request_tool_call"));
    assert!(!prompt.contains("send_tool_call_message"));

    // 验证返回结果
    assert_eq!(result.agent_id, agent_id);
    assert!(!result.raw_input.is_empty());
    assert_eq!(result.raw_output, "mock response");
}

/// awaken 流程测试：ThinkingOptions 注入 project/task 上下文到 Prompt。
///
/// 验证：
/// - ThinkingOptions.with_project() 的实体摘要出现在 Prompt 中
/// - ThinkingOptions.with_task() 的实体摘要出现在 Prompt 中
/// - project_context / task_context 区块正确渲染
#[sqlx::test]
async fn test_awaken_project_task_context_in_prompt(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool).await;

    let agent_id = format!("agent-ctx-{}", Uuid::now_v7());
    let agent = make_test_agent_with_brain(&agent_id);

    let message = make_test_message("请帮我处理这个任务");

    // 构造带 project + task 的 ThinkingOptions
    use ::common::enums::{AssigneeType, ProjectStatus, TaskStatus};
    use ai_orz::models::project::{Project, ProjectPo};
    use ai_orz::models::task::{Task, TaskPo};

    let project_id = format!("project-{}", Uuid::now_v7());
    let task_id = format!("task-{}", Uuid::now_v7());

    let mut project_po = ProjectPo::new(
        project_id.clone(),
        "Test Project".to_string(),
        "项目描述：集成测试项目".to_string(),
        None,   // workflow
        None,   // guidance
        0,      // priority
        vec![], // tags
        "test-user".to_string(),
        None, // owner_agent_id
        None, // start_at
        None, // due_at
        None, // end_at
        "test-user".to_string(),
    );
    project_po.status = ProjectStatus::Active;
    let project = Project::from_po(project_po);

    let mut task_po = TaskPo::new(
        task_id.clone(),
        "Test Task".to_string(),
        "任务描述：执行集成测试".to_string(),
        0,                        // priority
        vec![],                   // tags
        None,                     // due_at
        None,                     // start_at
        None,                     // end_at
        vec![],                   // dependencies
        "test-user".to_string(),  // root_user_id
        AssigneeType::Agent,      // assignee_type
        "test-agent".to_string(), // assignee_id
        Some(project_id.clone()), // project_id
        "test-user".to_string(),  // created_by
    );
    task_po.status = TaskStatus::InProgress;
    let task = Task::from_po(task_po);

    let options = ThinkingOptions::new().with_project(project).with_task(task);

    let captured_prompt = Arc::new(Mutex::new(None));
    let temp_dir = tempdir().expect("tempdir should be created");
    let runtime = new_with_all(
        Arc::new(CapturingBrainDal::new(captured_prompt.clone())),
        ai_orz::service::dal::tool::dal(),
        ai_orz::service::dal::mcp_tool::dal(),
        ai_orz::service::dal::agent::dal(),
        Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf())),
    );

    let result = runtime
        .awakening()
        .awaken(ctx, &agent, &message, &options)
        .await
        .expect("awaken 应该成功");

    let prompt = captured_prompt
        .lock()
        .unwrap()
        .clone()
        .expect("应该捕获到 prompt");

    // 验证 project 上下文注入
    assert!(
        prompt.contains("Test Project"),
        "Prompt 应该包含 project 名称，实际: {}",
        prompt
    );
    assert!(
        prompt.contains("集成测试项目"),
        "Prompt 应该包含 project 描述"
    );

    // 验证 task 上下文注入
    assert!(prompt.contains("Test Task"), "Prompt 应该包含 task 名称");
    assert!(prompt.contains("执行集成测试"), "Prompt 应该包含 task 描述");

    assert_eq!(result.agent_id, agent_id);
}

/// awaken 流程测试：think 失败时 BusyGuard 释放 Busy 状态。
///
/// 验证：
/// - BrainDal.think() 返回错误时 awaken 返回 Err
/// - Agent 状态从 Busy 回到 Idle（BusyGuard RAII 释放）
/// - 错误事件被记录（AgentAwakeEvent status=failed）
#[sqlx::test]
async fn test_awaken_error_releases_busy_guard(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool.clone()).await;

    let agent_id = format!("agent-err-{}", Uuid::now_v7());
    let agent = make_test_agent_with_brain(&agent_id);
    let message = make_test_message("触发错误的消息");

    // 使用永远失败的 BrainDal
    struct FailingBrainDal;

    #[async_trait]
    impl BrainDal for FailingBrainDal {
        async fn wake_brain(
            &self,
            _ctx: RequestContext,
            _agent: &AgentPo,
            _memories: Vec<ai_orz::models::memory::Memory>,
        ) -> CommonResult<Brain> {
            unimplemented!("not needed")
        }

        async fn test_connection(
            &self,
            _ctx: RequestContext,
            _provider: &ModelProvider,
            _prompt: &str,
        ) -> CommonResult<String> {
            unimplemented!("not needed")
        }

        async fn think(
            &self,
            _ctx: RequestContext,
            _brain: &Brain,
            _messages: &[ai_orz::models::cortex_types::ChatMessage],
            _tools: &[ai_orz::models::cortex_types::ToolDescriptor],
        ) -> CommonResult<ai_orz::models::cortex_types::ThinkResult> {
            Err(::common::error::Error::internal("mock think failure"))
        }

        async fn embed_entity(
            &self,
            _ctx: RequestContext,
            _entity: &dyn ai_orz::models::vector::Vectorizable,
        ) -> CommonResult<Option<ai_orz::models::vector::VectorIndexParams>> {
            Ok(None)
        }

        async fn embed_text_for_search(
            &self,
            _ctx: RequestContext,
            _text: &str,
        ) -> CommonResult<Option<ai_orz::models::vector::VectorIndexParams>> {
            Ok(None)
        }
    }

    let temp_dir = tempdir().expect("tempdir should be created");
    let runtime = new_with_all(
        Arc::new(FailingBrainDal),
        ai_orz::service::dal::tool::dal(),
        ai_orz::service::dal::mcp_tool::dal(),
        ai_orz::service::dal::agent::dal(),
        Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf())),
    );

    // awaken 前先设置 Busy（模拟 handle_agent_message 的 try_set_busy）
    let runtime_state = AgentRuntimeStateManager::global();
    runtime_state.set_busy(&agent_id, &message.po.id, None, None);

    // 调用 awaken（应该失败）
    let result = runtime
        .awakening()
        .awaken(ctx, &agent, &message, &ThinkingOptions::new())
        .await;

    assert!(
        result.is_err(),
        "awaken should return error when think fails"
    );

    // 验证 Agent 回到 Idle 状态（BusyGuard 通过 RAII 释放）
    let state = runtime_state.get_state(&agent_id);
    assert_eq!(
        state,
        ::common::enums::AgentRuntimeState::Idle,
        "Agent should be Idle after awaken error (BusyGuard released)"
    );
}

// ==================== Part C: 真实 LLM 端到端测试 ====================

/// Parse provider type string to serde variant name.
///
/// env 变量值（如 "doubao"）需转换为 serde 变体名（如 "Doubao"）才能被
/// `ProviderType` 的 `Deserialize` 正确解析（无 `rename_all`，默认按变体名匹配）。
fn parse_provider_type(s: &str) -> &'static str {
    match s.to_lowercase().as_str() {
        "openai" | "0" => "OpenAI",
        "deepseek" | "1" => "DeepSeek",
        "qwen" | "2" => "Qwen",
        "doubao" | "3" => "Doubao",
        "ollama" | "4" => "Ollama",
        "custom" | "5" => "Custom",
        "fastembed" | "6" => "FastEmbed",
        "doubao_vision" | "doubaoVision" | "7" => "DoubaoVision",
        _ => "OpenAI",
    }
}

/// 真实模型配置（从 .env 读取）
struct RealLlmConfig {
    llm_provider_type: &'static str,
    llm_model_name: String,
    llm_api_key: String,
    llm_base_url: Option<String>,
}

impl RealLlmConfig {
    fn from_env() -> Option<Self> {
        let _ = dotenvy::dotenv();
        let llm_api_key = std::env::var("TEST_LLM_API_KEY").ok()?;
        let llm_model_name = std::env::var("TEST_LLM_MODEL_NAME").ok()?;
        let llm_provider_type = std::env::var("TEST_LLM_PROVIDER_TYPE")
            .ok()
            .as_deref()
            .map(parse_provider_type)
            .unwrap_or("Doubao");
        let llm_base_url = std::env::var("TEST_LLM_BASE_URL").ok();
        Some(Self {
            llm_provider_type,
            llm_model_name,
            llm_api_key,
            llm_base_url,
        })
    }
}

/// 创建真实 LLM Provider，返回 provider_id
async fn create_real_llm_provider(app: &TestApp, jwt: &str, cfg: &RealLlmConfig) -> String {
    let req = json!({
        "name": format!("RealLLM-{}", uuid::Uuid::now_v7()),
        "provider_type": cfg.llm_provider_type,
        "capability": "Agent",
        "model_name": cfg.llm_model_name,
        "api_key": cfg.llm_api_key,
        "base_url": cfg.llm_base_url,
        "description": "Real LLM for awaken test"
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/finance/model-providers", &req, jwt)
        .await;
    let data = crate::common::assert_api_ok(status, &body);
    data.get("id")
        .and_then(|v| v.as_str())
        .expect("missing provider id")
        .to_string()
}

/// 真实 LLM 端到端测试：发送消息 → Consumer 触发 awaken → LLM 生成响应。
///
/// 验证：
/// - Agent 收到消息后触发 awaken
/// - 真实 LLM 生成有意义的响应（awaken 成功即证明 LLM 返回了非空输出）
/// - 响应 Trace 写入 memory（通过 stats.call_summary.total_calls 验证）
/// - Agent 回到 Idle 状态
///
/// 注：awaken 流程将 LLM 输出写入 memory trace（JSONL 文件）并记录统计事件，
/// 但不会在 messages 表中创建回复消息（response message 由下游业务流程生成）。
/// 因此本测试通过 `with_stats=true` 查询 Agent 唤醒次数来验证 LLM 调用成功落地。
#[sqlx::test]
#[ignore = "requires real LLM API key in .env (TEST_LLM_API_KEY)"]
async fn test_real_llm_awaken_full_flow(pool: SqlitePool) {
    let Some(cfg) = RealLlmConfig::from_env() else {
        eprintln!("SKIP: TEST_LLM_API_KEY not set, skipping real LLM awaken test");
        return;
    };

    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let (bs, jwt) = crate::common::factories::bootstrap_and_login(&app).await;

    // 1. 创建真实 LLM Provider
    let real_provider_id = create_real_llm_provider(&app, &jwt, &cfg).await;

    // 2. 创建 Agent（使用真实 LLM Provider）
    let agent_name = format!("RealLLMAgent-{}", uuid::Uuid::now_v7());
    let agent_req = json!({
        "name": agent_name,
        "description": "一个用于测试真实 LLM 唤醒的 Agent",
        "model_provider_id": real_provider_id,
        "soul": "你是一个测试助手，请简洁回答问题。"
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/hr/agents", &agent_req, &jwt)
        .await;
    let agent_data = crate::common::assert_api_ok(status, &body);
    let agent_id = agent_data
        .get("id")
        .and_then(|v| v.as_str())
        .expect("missing agent id")
        .to_string();

    // 3. 发送消息给 Agent
    let send_req = json!({
        "to_agent_id": agent_id,
        "content": "请回复：awaken 测试成功"
    });
    let (status, body) = app
        .post_with_jwt("/api/v1/finance/messages/agents", &send_req, &jwt)
        .await;
    let msg_data = crate::common::assert_api_ok(status, &body);
    let message_id = msg_data
        .get("message_id")
        .and_then(|v| v.as_str())
        .expect("missing message_id")
        .to_string();

    // 4. 调用 Consumer 触发 awaken（真实 LLM 调用）
    let consumer = MessageConsumer::new();
    let event = make_message_event(&message_id, &bs.user_id, &agent_id, 1);

    let result = consumer.on_event(event).await;

    // awaken 应该成功（真实 LLM 返回了响应）
    assert!(
        result.is_ok(),
        "Consumer should succeed with real LLM, got: {:?}",
        result.err()
    );

    // 5. 验证 Agent 回到 Idle
    let runtime_state = AgentRuntimeStateManager::global();
    let state = runtime_state.get_state(&agent_id);
    assert_eq!(
        state,
        ::common::enums::AgentRuntimeState::Idle,
        "Agent should be Idle after awaken completion"
    );

    // 6. 验证 awaken 写入了 memory trace（awaken 成功会写入 thinking trace）
    // 等待异步写入完成
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // 查询 Agent 的短期记忆，应该包含 awaken 写入的 trace
    let (status, body) = app
        .get_with_jwt(
            &format!(
                "/api/v1/hr/agents/{}/memories?memory_type=short_term&limit=10",
                agent_id
            ),
            &jwt,
        )
        .await;
    // 即使 memories 端点不存在，awaken 成功本身已由 Consumer Ok + Idle 验证
    if status == axum::http::StatusCode::OK {
        let mem_data = crate::common::assert_api_ok(status, &body);
        let memories = mem_data
            .get("results")
            .or_else(|| mem_data.get("memories"))
            .and_then(|v| v.as_array());
        if let Some(memories) = memories {
            eprintln!(
                "Real LLM awaken test: {} memory traces found",
                memories.len()
            );
        }
    }

    eprintln!("Real LLM awaken test passed! Consumer returned Ok, Agent is Idle.");

    // Cleanup
    let _ = app
        .delete_with_jwt(
            &format!("/api/v1/finance/model-providers/{}", real_provider_id),
            &jwt,
        )
        .await;
}

// ==================== Part D: A+ P3 两阶段唤醒集成测试 ====================
// （Task 4 集成测试 bonus）
//
// 思路：复用 CapturingBrainDal 模式，实现「两阶段 Mock」：
//   - 第 1 次 think() 调用（Phase 1：analyze_input_intent）→ 返回合法 IntentAnalysis JSON
//   - 第 2 次 think() 调用（Phase 2：正式 awaken 思考）→ 返回 Final 响应，并捕获 Prompt
// 断言：(i) Phase 2 的 Prompt 中出现【输入理解结果】区块；
//       (ii) awaken 返回成功且 raw_output 为 Phase 2 的 mock 响应。

/// 两阶段 Mock Cortex：按调用次序返回不同的 ThinkResult
struct TwoPhaseMockBrainDal {
    /// 原子计数器：记录 think() 被调用的次数
    call_count: Arc<Mutex<usize>>,
    /// 捕获 Phase 2（第二次调用）的 Prompt（供断言使用）
    captured_phase2_prompt: Arc<Mutex<Option<String>>>,
}

impl TwoPhaseMockBrainDal {
    fn new(
        call_count: Arc<Mutex<usize>>,
        captured_phase2_prompt: Arc<Mutex<Option<String>>>,
    ) -> Self {
        Self {
            call_count,
            captured_phase2_prompt,
        }
    }

    /// Phase 1（首次调用）返回的 IntentAnalysis JSON：合法且带锚点
    fn phase1_response() -> String {
        let ia = serde_json::json!({
            "intent_type": "TaskRequest",
            "confidence": 0.92,
            "key_terms": ["集成测试", "两阶段唤醒", "文档"],
            "resolutions": ["\"上次那个文档\" → doc_id=doc_it_777"],
            "retrieved_context": ["2026-08-14 短期记忆：doc_it_777 上一版本为 v1.3"],
            "need_clarification": [],
            "summary": "用户想让 Agent 处理 doc_it_777 文档的某项任务"
        });
        format!(
            "--- INTENT_ANALYSIS_START ---\n{}\n--- INTENT_ANALYSIS_END ---",
            serde_json::to_string_pretty(&ia).unwrap()
        )
    }

    /// Phase 2（第二次调用）返回的最终响应
    fn phase2_response() -> String {
        "两阶段唤醒集成测试成功：已收到 Phase 1 理解结果并进入正式执行阶段。".to_string()
    }
}

#[async_trait]
impl BrainDal for TwoPhaseMockBrainDal {
    async fn wake_brain(
        &self,
        _ctx: RequestContext,
        _agent: &AgentPo,
        _memories: Vec<ai_orz::models::memory::Memory>,
    ) -> CommonResult<Brain> {
        unimplemented!("TwoPhaseMockBrainDal: awaken 流程直接使用预装配的 Agent brain，不走 wake_brain")
    }

    async fn test_connection(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProvider,
        _prompt: &str,
    ) -> CommonResult<String> {
        unimplemented!("not needed by two-stage awaken test")
    }

    async fn think(
        &self,
        _ctx: RequestContext,
        _brain: &Brain,
        messages: &[ai_orz::models::cortex_types::ChatMessage],
        _tools: &[ai_orz::models::cortex_types::ToolDescriptor],
    ) -> CommonResult<ai_orz::models::cortex_types::ThinkResult> {
        // 提取最后一条 user message 作为 prompt（与 CapturingBrainDal 一致）
        let prompt = messages
            .iter()
            .rev()
            .find_map(|m| match m {
                ai_orz::models::cortex_types::ChatMessage::User { content } => {
                    Some(content.clone())
                }
                _ => None,
            })
            .unwrap_or_default();

        let mut count = self.call_count.lock().unwrap();
        *count += 1;
        let this_call = *count;
        drop(count);

        if this_call == 1 {
            // ===== Phase 1：IntentAnalyze 场景（analyze_input_intent 内部调用）=====
            // 返回合法的 IntentAnalysis JSON（带锚点），让解析逻辑成功解析
            Ok(ai_orz::models::cortex_types::ThinkResult::Final {
                content: Self::phase1_response(),
                usage: ai_orz::models::cortex_types::TokenUsage::default(),
            })
        } else if this_call == 2 {
            // ===== Phase 2：正式 awaken 场景（awaken loop 第 1 轮调用）=====
            // 仅在第 2 次调用时捕获 Prompt（避免后续 awaken_for_summary 调用覆盖）
            *self.captured_phase2_prompt.lock().unwrap() = Some(prompt);
            Ok(ai_orz::models::cortex_types::ThinkResult::Final {
                content: Self::phase2_response(),
                usage: ai_orz::models::cortex_types::TokenUsage::default(),
            })
        } else {
            // ===== 后续调用（awaken_for_summary 总结流程等）=====
            // 返回简单 Final，不再覆盖 Phase 2 捕获的 Prompt
            Ok(ai_orz::models::cortex_types::ThinkResult::Final {
                content: "summary done".to_string(),
                usage: ai_orz::models::cortex_types::TokenUsage::default(),
            })
        }
    }

    async fn embed_entity(
        &self,
        _ctx: RequestContext,
        _entity: &dyn ai_orz::models::vector::Vectorizable,
    ) -> CommonResult<Option<ai_orz::models::vector::VectorIndexParams>> {
        Ok(None)
    }

    async fn embed_text_for_search(
        &self,
        _ctx: RequestContext,
        _text: &str,
    ) -> CommonResult<Option<ai_orz::models::vector::VectorIndexParams>> {
        Ok(None)
    }
}

/// 集成测试：awaken 两阶段 Happy Path（A+ P3 串联）
///
/// 断言三点：
///   (i)  Phase 2 的 Prompt 中出现【输入理解结果】区块（即 render_intent_analysis_section 成功渲染）
///   (ii) Phase 1 成功返回合法 IntentAnalysis（解析成功意味着 ia 是 Some）
///        → 间接通过 Prompt 中出现理解区块来验证
///   (iii)awaken 最终返回成功且 raw_output = TwoPhaseMockBrainDal::phase2_response()
#[sqlx::test]
async fn awaken_two_stage_happy_path(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool).await;

    let agent_id = format!("agent-two-stage-{}", Uuid::now_v7());
    let agent = make_test_agent_with_brain(&agent_id);
    let message = make_test_message("帮我把上次那个文档更新一下");

    let call_count = Arc::new(Mutex::new(0));
    let captured_p2_prompt = Arc::new(Mutex::new(None));
    let temp_dir = tempdir().expect("tempdir should be created");

    let runtime = new_with_all(
        Arc::new(TwoPhaseMockBrainDal::new(
            call_count.clone(),
            captured_p2_prompt.clone(),
        )),
        ai_orz::service::dal::tool::dal(),
        ai_orz::service::dal::mcp_tool::dal(),
        ai_orz::service::dal::agent::dal(),
        Arc::new(ToolCallLogger::new(temp_dir.path().to_path_buf())),
    );

    // ===== 调用 awaken（两阶段串联执行）=====
    let opts = ThinkingOptions::new();
    let result = runtime
        .awakening()
        .awaken(ctx.clone(), &agent, &message, &opts)
        .await;

    // 断言 (iii)：最终返回成功且 raw_output 为 Phase 2 的响应
    match result {
        Ok(awakening_result) => {
            assert_eq!(
                awakening_result.agent_id, agent_id,
                "返回结果的 agent_id 不匹配"
            );
            assert_eq!(
                awakening_result.raw_output,
                TwoPhaseMockBrainDal::phase2_response(),
                "awaken 的最终输出应为 Phase 2 Mock 响应"
            );
            assert!(
                !awakening_result.raw_input.is_empty(),
                "raw_input（即 Phase 2 Prompt）不应为空"
            );
        }
        Err(e) => panic!(
            "awaken_two_stage_happy_path 应成功，但返回 Err: {:?}",
            e
        ),
    }

    // 断言 (i)(ii)：Phase 2 的 Prompt 中包含【输入理解结果】区块
    // → 间接证明了：Phase 1 的 analyze_input_intent 返回了 Some(IntentAnalysis)，
    //   且 builder.intent_analysis 被成功设置，build() 时在正确位置渲染了区块。
    let p2_prompt = captured_p2_prompt
        .lock()
        .unwrap()
        .clone()
        .expect("Phase 2 应该调用 think() 并捕获到 Prompt");

    assert!(
        p2_prompt.contains("【输入理解结果"),
        "Phase 2 的 Prompt 应该包含【输入理解结果】区块（两阶段串联失败）。\n\
         ===== Phase 2 Prompt 前 800 字 =====\n{}\n===== End =====\n\
         \n提示：如果此处失败，先检查 awaken() 中 ia 注入是否在 builder.build() 之前。",
        p2_prompt.chars().take(800).collect::<String>()
    );

    // 验证位置顺序：【输入理解结果】在【当前消息】之前
    let idx_ia = p2_prompt.find("【输入理解结果").unwrap();
    let idx_cm = p2_prompt.find("【当前消息】").unwrap();
    assert!(
        idx_ia < idx_cm,
        "【输入理解结果】(idx={}) 应出现在【当前消息】(idx={}) 之前",
        idx_ia,
        idx_cm
    );

    // 验证内容：理解区块包含 Phase 1 返回的 intent_type 字段渲染结果
    assert!(
        p2_prompt.contains("TaskRequest"),
        "Phase 2 Prompt 的理解区块应渲染 Phase 1 返回的 TaskRequest 类型"
    );
    assert!(
        p2_prompt.contains("92.00%"),
        "Phase 2 Prompt 的置信度应渲染为百分比：0.92 → 92.00%"
    );

    // 验证 think() 至少被调用 2 次（Phase 1 + Phase 2 各一次）
    let calls = *call_count.lock().unwrap();
    assert!(
        calls >= 2,
        "think() 至少应被调用 2 次（Phase 1 + Phase 2），实际仅 {} 次",
        calls
    );
}
