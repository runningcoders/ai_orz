//! A2A Server 集成测试
//!
//! 测试完整流程：tasks/send → tasks/get → tasks/cancel

use crate::pkg::request_context_test_support::{init_service_for_test, new_test_ctx};
use common::api::AgentMatchCriteria;
use common::api::a2a::*;
use sqlx::SqlitePool;

/// 初始化测试环境
async fn init_a2a_test_env(pool: SqlitePool) -> crate::pkg::RequestContext {
    // 统一业务层初始化（config + ToolCallLogger + dao/dal/domain init_all）
    init_service_for_test();

    new_test_ctx("test-a2a-user", pool)
}

/// 创建测试用前台 Agent
async fn create_test_reception_agent(ctx: &crate::pkg::RequestContext) -> String {
    use crate::models::agent::{Agent, AgentPo};
    use common::constants::agent_roles::ROLE_A2A_GATEWAY;
    use common::enums::AgentStatus;

    let mut po = AgentPo::new(
        "前台 Agent".to_string(),
        vec![ROLE_A2A_GATEWAY.to_string()],
        "前台接待测试 Agent".to_string(),
        vec!["chat".to_string()],
        "测试灵魂".to_string(),
        "provider-001".to_string(),
        "test-a2a-user".to_string(),
    );
    po.id = format!("reception-{}", uuid::Uuid::now_v7());
    po.status = AgentStatus::Onboarded;
    let expected_id = po.id.clone();

    let agent = Agent::from_po(po);
    crate::service::dal::agent::dal()
        .create(ctx.clone(), &agent)
        .await
        .expect("创建测试 Agent 失败");

    expected_id
}

#[sqlx::test]
async fn test_resolve_agent_returns_onboarded_agent(pool: SqlitePool) {
    let ctx = init_a2a_test_env(pool).await;
    let agent_id = create_test_reception_agent(&ctx).await;

    // 测试场景：按 A2A_GATEWAY 角色匹配，空 criteria 也应 fallback 到任意 Onboarded
    let found = crate::service::domain::hr::domain()
        .resolve_agent(ctx, AgentMatchCriteria::default())
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
