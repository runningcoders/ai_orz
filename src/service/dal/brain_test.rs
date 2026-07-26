//! Brain DAL 单元测试
//! 测试 Brain DAL 的 wake_brain 和 test_connection 功能

use crate::models::{agent::AgentPo, brain::*, memory::*, model_provider::*, tool::Tool};
use crate::pkg::request_context::RequestContext;
use crate::service::dal::brain::BrainDal;
use crate::service::dao::cortex;
use common::enums::{AgentKind, ModelCapability, ProviderType};
use sqlx::SqlitePool;
use std::sync::Arc;

/// 初始化测试环境
async fn init_test_env(pool: SqlitePool) -> (Arc<dyn BrainDal + Send + Sync>, RequestContext) {
    cortex::init();
    crate::service::dao::tool_call::init();
    crate::service::dao::model_provider::init();
    crate::service::dal::brain::init();
    let cortex_dao = cortex::dao();
    let tool_call_dao = crate::service::dao::tool_call::dao();
    let model_provider_dao = crate::service::dao::model_provider::dao();
    let http_client = reqwest::Client::new();
    let brain_dal =
        crate::service::dal::brain::new(cortex_dao, tool_call_dao, model_provider_dao, http_client);
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);
    (brain_dal, ctx)
}

/// 创建测试 ModelProvider
fn create_test_provider() -> ModelProvider {
    let provider_po = ModelProviderPo::new(
        "OpenAI GPT-4o".to_string(),
        ProviderType::OpenAI,
        ModelCapability::Agent,
        "gpt-4o".to_string(),
        "test-key".to_string(),
        Some("https://api.openai.com/v1".to_string()),
        Some("OpenAI GPT-4o Official".to_string()),
        "test".to_string(),
    );
    ModelProvider::from_po(provider_po)
}

/// 创建测试 Local AgentPo
fn create_test_local_agent(provider_id: &str) -> AgentPo {
    let mut po = AgentPo::new(
        "Test Agent".to_string(),
        vec!["assistant".to_string()],
        "Test description".to_string(),
        vec!["chat".to_string()],
        "Test soul".to_string(),
        provider_id.to_string(),
        "test-user".to_string(),
    );
    po.id = "test-agent".to_string();
    po.kind = AgentKind::Local;
    po
}

/// 测试 Brain DAL 创建 wake_brain 功能（Local agent）
#[sqlx::test]
async fn test_wake_brain_local(pool: SqlitePool) {
    let (brain_dal, ctx) = init_test_env(pool).await;

    // 先插入一个 ModelProvider
    let provider = create_test_provider();
    crate::service::dao::model_provider::dao()
        .insert(ctx.clone(), &provider.po)
        .await
        .unwrap();

    let agent = create_test_local_agent(&provider.po.id);

    let now = chrono::Utc::now().timestamp();
    let short_term_po = ShortTermMemoryIndexPo {
        id: "test-memory-1".to_string(),
        agent_id: "test-agent".to_string(),
        task_id: None,
        role: "system".to_string(),
        summary: "你是一个有用的AI助手".to_string(),
        tags: "[\"chat\", \"question\"]".to_string(),
        trace_ids: "[]".to_string(),
        status: common::enums::MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };
    let memory = Memory::new(MemoryPo::ShortTerm(short_term_po));
    let memories = vec![memory];

    let tools: Vec<Tool> = vec![];
    let result = brain_dal
        .wake_brain(ctx.clone(), &agent, memories, tools)
        .await;

    assert!(result.is_ok());
    let brain = result.unwrap();
    assert!(brain.is_local());
    assert!(brain.cortex().is_some());
    assert_eq!(brain.agent_id, "test-agent");
    assert_eq!(brain.agent_name, "Test Agent");
}

/// 测试 Brain DAL test_connection 功能
#[sqlx::test]
async fn test_test_connection(pool: SqlitePool) {
    let (brain_dal, ctx) = init_test_env(pool).await;

    let provider = create_test_provider();

    let result = brain_dal.test_connection(ctx, &provider, "Hello!").await;

    assert!(result.is_err());
}
