//! 外部会话绑定 DAO SQLite 单元测试

use crate::models::message_channel_conversation::MessageChannelConversationPo;
use crate::pkg::RequestContext;
use crate::service::dao::message_channel_conversation::{self, MessageChannelConversationDao};
use common::enums::ChannelType;
use sqlx::SqlitePool;
use std::sync::Arc;

const TENANT: &str = "tenant_a";

fn new_ctx(pool: SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx("test-user", pool)
}

fn init_test_env(
    pool: SqlitePool,
) -> (
    Arc<dyn MessageChannelConversationDao + Send + Sync>,
    RequestContext,
) {
    message_channel_conversation::init();
    (message_channel_conversation::dao(), new_ctx(pool))
}

fn binding(
    id: &str,
    app_id: &str,
    tenant_key: &str,
    chat_id: &str,
    channel_id: &str,
) -> MessageChannelConversationPo {
    MessageChannelConversationPo {
        id: id.to_string(),
        channel_type: ChannelType::Lark,
        external_app_id: app_id.to_string(),
        external_tenant_key: tenant_key.to_string(),
        external_chat_id: chat_id.to_string(),
        channel_id: channel_id.to_string(),
        created_at: 1,
        updated_at: 1,
    }
}

/// 首次写入后可按会话查到渠道
#[sqlx::test]
async fn test_upsert_then_find(pool: SqlitePool) {
    let (dao, ctx) = init_test_env(pool);
    dao.upsert(
        ctx.clone(),
        &binding("b1", "cli_app", TENANT, "oc_chat", "channel_1"),
    )
    .await
    .unwrap();

    let found = dao
        .find_channel_id(ctx.clone(), ChannelType::Lark, "cli_app", TENANT, "oc_chat")
        .await
        .unwrap();
    assert_eq!(found.as_deref(), Some("channel_1"));
}

/// 同一会话再次 upsert 覆盖渠道（会话改绑）
#[sqlx::test]
async fn test_upsert_overwrites_channel(pool: SqlitePool) {
    let (dao, ctx) = init_test_env(pool);
    dao.upsert(
        ctx.clone(),
        &binding("b1", "cli_app", TENANT, "oc_chat", "channel_1"),
    )
    .await
    .unwrap();
    dao.upsert(
        ctx.clone(),
        &binding("b2", "cli_app", TENANT, "oc_chat", "channel_2"),
    )
    .await
    .unwrap();

    let found = dao
        .find_channel_id(ctx.clone(), ChannelType::Lark, "cli_app", TENANT, "oc_chat")
        .await
        .unwrap();
    assert_eq!(found.as_deref(), Some("channel_2"));
}

/// 同一 chat_id 在不同应用 / 租户下互不影响
#[sqlx::test]
async fn test_app_and_tenant_isolation(pool: SqlitePool) {
    let (dao, ctx) = init_test_env(pool);
    dao.upsert(
        ctx.clone(),
        &binding("b1", "app_a", TENANT, "oc_chat", "channel_a"),
    )
    .await
    .unwrap();
    dao.upsert(
        ctx.clone(),
        &binding("b2", "app_b", TENANT, "oc_chat", "channel_b"),
    )
    .await
    .unwrap();
    dao.upsert(
        ctx.clone(),
        &binding("b3", "app_a", "tenant_b", "oc_chat", "channel_c"),
    )
    .await
    .unwrap();

    let found_a = dao
        .find_channel_id(ctx.clone(), ChannelType::Lark, "app_a", TENANT, "oc_chat")
        .await
        .unwrap();
    let found_b = dao
        .find_channel_id(ctx.clone(), ChannelType::Lark, "app_b", TENANT, "oc_chat")
        .await
        .unwrap();
    let found_c = dao
        .find_channel_id(
            ctx.clone(),
            ChannelType::Lark,
            "app_a",
            "tenant_b",
            "oc_chat",
        )
        .await
        .unwrap();
    assert_eq!(found_a.as_deref(), Some("channel_a"));
    assert_eq!(found_b.as_deref(), Some("channel_b"));
    assert_eq!(found_c.as_deref(), Some("channel_c"));
}

/// 未绑定会话返回 None
#[sqlx::test]
async fn test_find_missing_returns_none(pool: SqlitePool) {
    let (dao, ctx) = init_test_env(pool);
    let found = dao
        .find_channel_id(
            ctx.clone(),
            ChannelType::Lark,
            "cli_app",
            TENANT,
            "oc_missing",
        )
        .await
        .unwrap();
    assert!(found.is_none());
}

/// 按渠道分页查询 + 总数统计（只返回该渠道的会话，按绑定时间倒序）
#[sqlx::test]
async fn test_query_by_channel_paginates(pool: SqlitePool) {
    let (dao, ctx) = init_test_env(pool);
    for (idx, chat) in ["oc_1", "oc_2", "oc_3"].iter().enumerate() {
        let mut b = binding("b_old", "cli_app", TENANT, chat, "channel_1");
        b.id = format!("b{}", idx);
        b.created_at = idx as i64;
        dao.upsert(ctx.clone(), &b).await.unwrap();
    }
    // 其他渠道的会话不应混入
    dao.upsert(
        ctx.clone(),
        &binding("b_other", "cli_app", TENANT, "oc_other", "channel_2"),
    )
    .await
    .unwrap();

    let first = dao
        .query_by_channel(ctx.clone(), "channel_1", 2, 0)
        .await
        .unwrap();
    assert_eq!(first.len(), 2);
    // 倒序：oc_3（created_at=2）在前
    assert_eq!(first[0].external_chat_id, "oc_3");
    assert_eq!(first[1].external_chat_id, "oc_2");

    let second = dao
        .query_by_channel(ctx.clone(), "channel_1", 2, 2)
        .await
        .unwrap();
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].external_chat_id, "oc_1");

    assert_eq!(
        dao.count_by_channel(ctx.clone(), "channel_1")
            .await
            .unwrap(),
        3
    );
    assert_eq!(dao.count_by_channel(ctx, "channel_2").await.unwrap(), 1);
}
