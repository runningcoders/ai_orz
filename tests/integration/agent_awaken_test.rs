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
use ai_orz::pkg::aop::Consumer;
use ai_orz::pkg::agent_runtime_state::AgentRuntimeStateManager;
use serde_json::json;
use sqlx::SqlitePool;

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
