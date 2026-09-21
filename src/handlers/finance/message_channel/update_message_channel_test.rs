//! update_message_channel handler 测试：agent_id 清除哨兵语义
//!
//! 三条协议路径（`Option<String>` 单层可空无法表达「清除」，用
//! `CLEAR_FIELD_SENTINEL` 哨兵补齐）：
//! - `None` → 不修改，保持原绑定
//! - `Some(__clear__)` → 解除绑定（落库 `NULL`）
//! - `Some(id)` → 设置新绑定
//!
//! 渠道用 Webhook 类型：跳过 Lark/Wechat/Email 凭证引用校验，聚焦绑定语义。

use common::api::UpdateMessageChannelRequest;
use common::constants::sentinel::CLEAR_FIELD_SENTINEL;
use common::enums::ChannelType;
use sqlx::SqlitePool;

use crate::models::message_channel::{ChannelConfig, MessageChannelPo};
use crate::service::dao::message_channel;

use super::update_message_channel::update_message_channel;

fn init_test_singletons() {
    // 统一业务层初始化（幂等），对齐 update_tool_test 模式
    crate::pkg::request_context_test_support::init_service_for_test();
}

/// 构造带组织上下文的测试 ctx（handler 强制校验 org_id + user_id 归属）
fn test_ctx(pool: SqlitePool) -> crate::pkg::RequestContext {
    let storage = crate::pkg::storage::test_support::create_test_storage(pool);
    crate::pkg::RequestContext::builder()
        .user_id("editor".to_string())
        .organization_id("org-1".to_string())
        .storage(storage)
        .build()
}

async fn insert_webhook_channel(
    ctx: crate::pkg::RequestContext,
    id: &str,
    agent_id: Option<String>,
) {
    let po = MessageChannelPo::new(
        id.to_string(),
        "org-1".to_string(),
        "editor".to_string(),
        agent_id,
        ChannelType::Webhook,
        "测试渠道".to_string(),
        Some("https://example.com/hook".to_string()),
        None,
        None,
        ChannelConfig::default(),
        "editor".to_string(),
    );
    message_channel::new()
        .insert(ctx, &po)
        .await
        .expect("channel should be inserted");
}

async fn persisted_agent_id(ctx: crate::pkg::RequestContext, id: &str) -> Option<String> {
    message_channel::new()
        .find_by_id(ctx, id)
        .await
        .expect("channel lookup should succeed")
        .expect("channel should exist")
        .agent_id
}

#[sqlx::test(migrations = "./migrations")]
async fn update_channel_clear_sentinel_unbinds_agent(pool: SqlitePool) {
    init_test_singletons();
    let ctx = test_ctx(pool);
    insert_webhook_channel(ctx.clone(), "ch-clear", Some("agent-1".to_string())).await;

    update_message_channel(
        ctx.clone(),
        UpdateMessageChannelRequest {
            id: "ch-clear".to_string(),
            agent_id: Some(CLEAR_FIELD_SENTINEL.to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("sentinel update should succeed");

    assert_eq!(persisted_agent_id(ctx, "ch-clear").await, None);
}

#[sqlx::test(migrations = "./migrations")]
async fn update_channel_none_keeps_existing_binding(pool: SqlitePool) {
    init_test_singletons();
    let ctx = test_ctx(pool);
    insert_webhook_channel(ctx.clone(), "ch-keep", Some("agent-1".to_string())).await;

    update_message_channel(
        ctx.clone(),
        UpdateMessageChannelRequest {
            id: "ch-keep".to_string(),
            agent_id: None,
            ..Default::default()
        },
    )
    .await
    .expect("no-op update should succeed");

    assert_eq!(
        persisted_agent_id(ctx, "ch-keep").await,
        Some("agent-1".to_string())
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn update_channel_sets_new_agent_binding(pool: SqlitePool) {
    init_test_singletons();
    let ctx = test_ctx(pool);
    insert_webhook_channel(ctx.clone(), "ch-set", Some("agent-1".to_string())).await;

    update_message_channel(
        ctx.clone(),
        UpdateMessageChannelRequest {
            id: "ch-set".to_string(),
            agent_id: Some("agent-2".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect("set update should succeed");

    assert_eq!(
        persisted_agent_id(ctx, "ch-set").await,
        Some("agent-2".to_string())
    );
}
