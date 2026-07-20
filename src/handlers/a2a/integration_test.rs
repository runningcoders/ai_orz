//! A2A Server 集成测试
//!
//! 测试完整流程：tasks/send → tasks/get → tasks/cancel

use crate::pkg::request_context_test_support::new_test_ctx;
use common::api::a2a::*;
use sqlx::SqlitePool;

/// 初始化测试环境
async fn init_a2a_test_env(pool: SqlitePool) -> crate::pkg::RequestContext {
    let _ = crate::config::init();

    // 初始化 ToolCallLogger（agent dal 创建时会写 trace）
    let base_path = std::env::temp_dir().join("ai_orz_a2a_test_trace");
    let _ = std::fs::create_dir_all(&base_path);
    crate::pkg::tool_tracing::logger::ToolCallLogger::init(base_path);

    // 初始化所有 DAO
    crate::service::dao::agent::init();
    crate::service::dao::tool::init();
    crate::service::dao::skill::init();
    crate::service::dao::tool_call::init();
    crate::service::dao::model_provider::init();
    crate::service::dao::cortex::init();
    crate::service::dao::memory::init();
    crate::service::dao::mcp_server::init();
    crate::service::dao::project::init();
    crate::service::dao::task::init();
    crate::service::dao::message::init();
    crate::service::dao::artifact::init();
    crate::service::dao::attachment::init();
    crate::service::dao::message_channel::init();
    // 消息渠道 DAO 初始化
    crate::service::dao::lark::init();
    crate::service::dao::wechat::init();
    crate::service::dao::slack::init();
    crate::service::dao::email::init();
    crate::service::dao::webhook::init();
    crate::service::dao::a2a_callback::init();

    // 初始化所有 DAL
    crate::service::dal::agent::init();
    crate::service::dal::tool::init();
    crate::service::dal::skill::init();
    crate::service::dal::model_provider::init();
    crate::service::dal::memory::init();
    crate::service::dal::mcp_tool::init();
    crate::service::dal::brain::init();
    crate::service::dal::project::init();
    crate::service::dal::task::init();
    crate::service::dal::message::init();
    crate::service::dal::message_channel::init();
    crate::service::dal::attachment::init();
    crate::service::dal::artifact::init();

    // 初始化所有 Domain
    crate::service::domain::hr::init();
    crate::service::domain::message::init();
    crate::service::domain::project::init();
    crate::service::domain::runtime::init();

    new_test_ctx("test-a2a-user", pool)
}

/// 创建测试用前台 Agent
async fn create_test_reception_agent(ctx: &crate::pkg::RequestContext) -> String {
    use crate::models::agent::{Agent, AgentPo};
    use common::enums::AgentStatus;

    let mut po = AgentPo::new(
        "前台 Agent".to_string(),
        vec!["feishu_reception".to_string()],
        "前台接待测试 Agent".to_string(),
        vec!["chat".to_string()],
        "测试灵魂".to_string(),
        "provider-001".to_string(),
        "test-a2a-user".to_string(),
    );
    po.id = format!("reception-{}", uuid::Uuid::now_v7());
    po.status = AgentStatus::Onboarded;

    let agent = Agent::from_po(po);
    crate::service::dal::agent::dal()
        .create(ctx.clone(), &agent)
        .await
        .expect("创建测试 Agent 失败");

    agent.po.id
}

#[sqlx::test]
async fn test_resolve_agent_returns_onboarded_agent(pool: SqlitePool) {
    let ctx = init_a2a_test_env(pool).await;
    let agent_id = create_test_reception_agent(&ctx).await;

    let found = crate::service::domain::hr::domain()
        .resolve_agent(ctx)
        .await
        .expect("查找前台 Agent 应该成功");

    assert!(found.is_some(), "应该找到前台 Agent");
    assert_eq!(found.unwrap().po.id, agent_id);
}

#[sqlx::test]
async fn test_tasks_get_returns_not_found_for_nonexistent(pool: SqlitePool) {
    let ctx = init_a2a_test_env(pool).await;

    let result = crate::handlers::a2a::get_task::handle_get_task(
        ctx,
        GetTaskParams {
            id: "nonexistent".to_string(),
            history_length: None,
        },
    )
    .await;

    assert!(result.is_err(), "查询不存在的 task 应该返回错误");
}

#[sqlx::test]
async fn test_tasks_cancel_nonexistent_returns_error(pool: SqlitePool) {
    let ctx = init_a2a_test_env(pool).await;

    let result = crate::handlers::a2a::cancel_task::handle_cancel_task(
        ctx,
        CancelTaskParams {
            id: "nonexistent".to_string(),
        },
    )
    .await;

    assert!(result.is_err(), "取消不存在的 task 应该返回错误");
}
