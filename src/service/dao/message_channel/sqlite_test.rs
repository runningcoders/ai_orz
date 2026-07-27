//! MessageChannel DAO SQLite 单元测试

use crate::models::message_channel::{ChannelConfig, MessageChannelPo};
use crate::pkg::RequestContext;
use crate::service::dao::message_channel::{self, MessageChannelDao, MessageChannelQuery};
use common::enums::{ChannelStatus, ChannelType};
use common::error::Result;
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx(user_id, pool)
}

/// 测试插入渠道和按 ID 查询

/// 初始化测试环境
fn init_test_env(pool: SqlitePool) -> (Arc<dyn MessageChannelDao + Send + Sync>, RequestContext) {
    message_channel::init();
    let dao = message_channel::dao();
    let ctx = new_ctx("test-user", pool);
    (dao, ctx)
}

/// 创建测试 MessageChannelPo
#[allow(dead_code)] // 测试辅助函数，保留供未来测试使用
fn create_test_channel(org_id: &str, user_id: &str, name: &str) -> MessageChannelPo {
    MessageChannelPo::new(
        Uuid::now_v7().to_string(),
        org_id.to_string(),
        user_id.to_string(),
        None,
        ChannelType::Webhook,
        name.to_string(),
        Some("https://webhook.example.com/abc".to_string()),
        None,
        None,
        ChannelConfig::default(),
        user_id.to_string(),
    )
}

#[sqlx::test(migrations = "./migrations")]
async fn test_insert_and_find_by_id(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);

    let channel = MessageChannelPo::new(
        Uuid::now_v7().to_string(),
        "org-001".to_string(),
        "user-001".to_string(),
        None,
        ChannelType::Webhook,
        "我的 Webhook 渠道".to_string(),
        Some("https://webhook.example.com/abc".to_string()),
        None,
        None,
        ChannelConfig::default(),
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &channel).await?;

    let found = dao.find_by_id(ctx.clone(), &channel.id).await?;
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, channel.id);
    assert_eq!(found.org_id, "org-001");
    assert_eq!(found.user_id, "user-001");
    assert_eq!(found.channel_type, ChannelType::Webhook);
    assert_eq!(found.channel_name, "我的 Webhook 渠道");
    assert_eq!(found.status, ChannelStatus::Active);

    Ok(())
}

/// 测试按用户 ID 查询渠道
#[sqlx::test(migrations = "./migrations")]
async fn test_list_by_user_id(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);

    // 插入 3 个用户渠道
    for i in 0..3 {
        let channel = MessageChannelPo::new(
            Uuid::now_v7().to_string(),
            "org-001".to_string(),
            "user-001".to_string(),
            None,
            ChannelType::Webhook,
            format!("渠道 {}", i),
            Some(format!("https://example.com/{}", i)),
            None,
            None,
            ChannelConfig::default(),
            "test-user".to_string(),
        );
        dao.insert(ctx.clone(), &channel).await?;
    }

    // 查询用户的所有启用渠道
    let channels = dao.list_by_user_id(ctx.clone(), "user-001", true).await?;
    assert_eq!(channels.len(), 3);

    Ok(())
}

/// 测试 Agent 专属渠道优先级
#[sqlx::test(migrations = "./migrations")]
async fn test_agent_channel_priority(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);

    // 用户通用渠道
    let user_channel = MessageChannelPo::new(
        Uuid::now_v7().to_string(),
        "org-001".to_string(),
        "user-001".to_string(),
        None, // 无 Agent
        ChannelType::Webhook,
        "用户通用渠道".to_string(),
        Some("https://example.com/user".to_string()),
        None,
        None,
        ChannelConfig::default(),
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &user_channel).await?;

    // Agent 专属渠道
    let agent_channel = MessageChannelPo::new(
        Uuid::now_v7().to_string(),
        "org-001".to_string(),
        "user-001".to_string(),
        Some("agent-001".to_string()), // 绑定 Agent
        ChannelType::Webhook,
        "Agent 专属渠道".to_string(),
        Some("https://example.com/agent".to_string()),
        None,
        None,
        ChannelConfig::default(),
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &agent_channel).await?;

    // 查询用户+Agent 的渠道（应该返回 2 条：Agent 专属 + 用户通用）
    let channels = dao
        .list_by_user_and_agent_id(ctx.clone(), "user-001", "agent-001", true)
        .await?;
    assert_eq!(channels.len(), 2);

    Ok(())
}

/// 测试设置启用/禁用
#[sqlx::test(migrations = "./migrations")]
async fn test_set_enabled(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);

    let channel = MessageChannelPo::new(
        Uuid::now_v7().to_string(),
        "org-001".to_string(),
        "user-001".to_string(),
        None,
        ChannelType::Webhook,
        "测试渠道".to_string(),
        None,
        None,
        None,
        ChannelConfig::default(),
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &channel).await?;

    // 禁用
    dao.set_status(ctx.clone(), &channel.id, ChannelStatus::Disabled)
        .await?;
    let found = dao.find_by_id(ctx.clone(), &channel.id).await?.unwrap();
    assert_eq!(found.status, ChannelStatus::Disabled);

    // 重新启用
    dao.set_status(ctx.clone(), &channel.id, ChannelStatus::Active)
        .await?;
    let found = dao.find_by_id(ctx.clone(), &channel.id).await?.unwrap();
    assert_eq!(found.status, ChannelStatus::Active);

    Ok(())
}

/// 测试软删除
#[sqlx::test(migrations = "./migrations")]
async fn test_delete(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);

    let channel = MessageChannelPo::new(
        Uuid::now_v7().to_string(),
        "org-001".to_string(),
        "user-001".to_string(),
        None,
        ChannelType::Webhook,
        "待删除渠道".to_string(),
        None,
        None,
        None,
        ChannelConfig::default(),
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &channel).await?;

    // 删除
    dao.delete(ctx.clone(), &channel.id).await?;

    // 仍然可以查到（软删除）
    let found = dao.find_by_id(ctx.clone(), &channel.id).await?;
    assert!(found.is_some());

    Ok(())
}

/// 测试通用查询
#[sqlx::test(migrations = "./migrations")]
async fn test_query(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);

    // 插入测试数据
    for i in 0..5 {
        let channel = MessageChannelPo::new(
            Uuid::now_v7().to_string(),
            "org-001".to_string(),
            format!("user-{}", i % 2),
            None,
            if i % 2 == 0 {
                ChannelType::Webhook
            } else {
                ChannelType::Lark
            },
            format!("渠道 {}", i),
            None,
            None,
            None,
            ChannelConfig::default(),
            "test-user".to_string(),
        );
        dao.insert(ctx.clone(), &channel).await?;
    }

    // 按用户 ID 查询
    let query = MessageChannelQuery {
        user_id: Some("user-0".to_string()),
        ..Default::default()
    };
    let page = dao.query(ctx.clone(), query).await?;
    assert_eq!(page.items.len(), 3); // user-0 有 3 条
    assert_eq!(page.total, 3);

    // 按渠道类型查询
    let query = MessageChannelQuery {
        channel_type: Some(ChannelType::Lark),
        ..Default::default()
    };
    let page = dao.query(ctx.clone(), query).await?;
    assert_eq!(page.items.len(), 2); // Lark 有 2 条
    assert_eq!(page.total, 2);

    // 只查询启用的
    let query = MessageChannelQuery {
        only_enabled: true,
        ..Default::default()
    };
    let page = dao.query(ctx.clone(), query).await?;
    assert_eq!(page.items.len(), 5); // 全部启用
    assert_eq!(page.total, 5);

    // 测试分页
    let query = MessageChannelQuery {
        pagination: common::api::PaginationParams {
            limit: Some(2),
            offset: Some(1),
        },
        ..Default::default()
    };
    let page = dao.query(ctx.clone(), query).await?;
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.total, 5);

    Ok(())
}

/// 测试标记推送成功/失败
#[sqlx::test(migrations = "./migrations")]
async fn test_mark_push_status(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);

    let channel = MessageChannelPo::new(
        Uuid::now_v7().to_string(),
        "org-001".to_string(),
        "user-001".to_string(),
        None,
        ChannelType::Webhook,
        "测试推送".to_string(),
        None,
        None,
        None,
        ChannelConfig::default(),
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &channel).await?;

    // 标记推送失败
    dao.mark_push_failed(ctx.clone(), &channel.id, "连接超时")
        .await?;
    let found = dao.find_by_id(ctx.clone(), &channel.id).await?.unwrap();
    assert_eq!(found.last_error, Some("连接超时".to_string()));
    assert!(found.last_pushed_at.is_some());

    // 标记推送成功（错误信息应该清空）
    dao.mark_push_success(ctx.clone(), &channel.id).await?;
    let found = dao.find_by_id(ctx.clone(), &channel.id).await?.unwrap();
    assert_eq!(found.last_error, None);

    Ok(())
}
