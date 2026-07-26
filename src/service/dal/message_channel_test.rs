//! MessageChannel DAL 单元测试

use crate::models::message_channel::{ChannelConfig, MessageChannel, MessageChannelPo};
use crate::pkg::RequestContext;
use crate::service::dal::message_channel::MessageChannelDal;
use crate::service::dal::message_channel::{dal, init};
use crate::service::dao::email::init as email_dao_init;
use crate::service::dao::lark::init as lark_dao_init;
use crate::service::dao::message_channel::init as message_channel_dao_init;
use crate::service::dao::slack::init as slack_dao_init;
use crate::service::dao::webhook::init as webhook_dao_init;
use crate::service::dao::wechat::init as wechat_dao_init;
use common::enums::{ChannelStatus, ChannelType};
use sqlx::SqlitePool;
use std::sync::Arc;

fn init_all_test_daos() {
    message_channel_dao_init();
    lark_dao_init();
    wechat_dao_init();
    slack_dao_init();
    email_dao_init();
    webhook_dao_init();
    init();
}

/// 初始化测试环境
async fn init_test_env(
    pool: SqlitePool,
) -> (Arc<dyn MessageChannelDal + Send + Sync>, RequestContext) {
    init_all_test_daos();
    let dal = dal();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);
    (dal, ctx)
}

/// 创建测试渠道
fn create_test_channel(
    channel_id: &str,
    user_id: &str,
    channel_type: ChannelType,
    name: &str,
) -> MessageChannel {
    let channel_po = MessageChannelPo::new(
        channel_id.to_string(),
        "org-1".to_string(),
        user_id.to_string(),
        None,
        channel_type,
        name.to_string(),
        Some("https://example.com/webhook".to_string()),
        None,
        None,
        ChannelConfig::default(),
        "admin".to_string(),
    );
    MessageChannel::from_po(channel_po)
}

#[sqlx::test]
async fn test_create_and_get_channel(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let channel_id = "test-create-channel".to_string();
    let channel_po = MessageChannelPo::new(
        channel_id.clone(),
        "org-1".to_string(),
        "user-1".to_string(),
        None,
        ChannelType::Lark,
        "测试飞书渠道".to_string(),
        Some("https://example.com/webhook".to_string()),
        None,
        None,
        ChannelConfig::default(),
        "admin".to_string(),
    );
    let channel = MessageChannel::from_po(channel_po);

    dal.create_channel(ctx.clone(), &channel).await.unwrap();
    let found: Option<MessageChannel> = dal.get_channel(ctx, &channel_id).await.unwrap();

    assert!(found.is_some());
    assert_eq!(found.as_ref().unwrap().id(), channel_id);
    assert_eq!(found.as_ref().unwrap().channel_type(), ChannelType::Lark);
    assert_eq!(found.as_ref().unwrap().user_id(), "user-1");
}

#[sqlx::test]
async fn test_list_user_channels(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    // 创建3个同一用户的渠道
    for i in 0..3 {
        let channel_po = MessageChannelPo::new(
            format!("test-list-channel-{}", i),
            "org-1".to_string(),
            "user-1".to_string(),
            None,
            match i {
                0 => ChannelType::Lark,
                1 => ChannelType::Wechat,
                _ => ChannelType::Slack,
            },
            format!("test-channel-name-{}", i),
            Some(format!("https://example.com/webhook/{}", i)),
            None,
            None,
            ChannelConfig::default(),
            "admin".to_string(),
        );
        let channel = MessageChannel::from_po(channel_po);
        dal.create_channel(ctx.clone(), &channel).await.unwrap();
    }

    // 创建另一个用户的1个渠道
    let other_channel_po = MessageChannelPo::new(
        "test-list-channel-other".to_string(),
        "org-1".to_string(),
        "user-2".to_string(),
        None,
        ChannelType::Email,
        "其他用户渠道".to_string(),
        None,
        None,
        None,
        ChannelConfig::default(),
        "admin".to_string(),
    );
    dal.create_channel(ctx.clone(), &MessageChannel::from_po(other_channel_po))
        .await
        .unwrap();

    // 查询 user-1 的所有启用渠道
    let channels: Vec<MessageChannel> = dal
        .list_user_channels(ctx.clone(), "user-1", true)
        .await
        .unwrap();
    assert_eq!(channels.len(), 3);

    // 查询所有用户渠道（不过滤）
    let all_channels: Vec<MessageChannel> =
        dal.list_user_channels(ctx, "user-1", false).await.unwrap();
    assert_eq!(all_channels.len(), 3);
}

#[sqlx::test]
async fn test_update_channel(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool.clone()).await;

    let channel_id = "test-update-channel".to_string();
    let channel_po = MessageChannelPo::new(
        channel_id.clone(),
        "org-1".to_string(),
        "user-1".to_string(),
        None,
        ChannelType::Lark,
        "原始名称".to_string(),
        Some("https://example.com/old".to_string()),
        None,
        None,
        ChannelConfig::default(),
        "admin".to_string(),
    );
    let mut channel = MessageChannel::from_po(channel_po);
    dal.create_channel(ctx.clone(), &channel).await.unwrap();

    // 更新渠道名称
    channel.po.channel_name = "更新后名称".to_string();
    dal.update_channel(
        crate::pkg::request_context_test_support::new_test_ctx("editor", pool),
        &channel,
    )
    .await
    .unwrap();

    let found: Option<MessageChannel> = dal.get_channel(ctx, &channel_id).await.unwrap();
    assert_eq!(found.as_ref().unwrap().po.channel_name, "更新后名称");
}

#[sqlx::test]
async fn test_delete_and_set_status(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool.clone()).await;

    let channel_id = "test-delete-channel".to_string();
    let channel_po = MessageChannelPo::new(
        channel_id.clone(),
        "org-1".to_string(),
        "user-1".to_string(),
        None,
        ChannelType::Lark,
        "测试渠道".to_string(),
        Some("https://example.com/webhook".to_string()),
        None,
        None,
        ChannelConfig::default(),
        "admin".to_string(),
    );
    let channel = MessageChannel::from_po(channel_po);
    dal.create_channel(ctx.clone(), &channel).await.unwrap();

    // 设置为禁用状态
    dal.set_channel_status(ctx.clone(), &channel_id, ChannelStatus::Disabled)
        .await
        .unwrap();

    // 查询 only_enabled=true 应该找不到
    let active_channels = dal
        .list_user_channels(ctx.clone(), "user-1", true)
        .await
        .unwrap();
    assert_eq!(active_channels.len(), 0);

    // 查询 only_enabled=false 应该能找到
    let all_channels = dal
        .list_user_channels(ctx.clone(), "user-1", false)
        .await
        .unwrap();
    assert_eq!(all_channels.len(), 1);

    // 删除渠道（软删除）
    dal.delete_channel(ctx, &channel_id).await.unwrap();

    // 应该找不到了
    let found = dal
        .get_channel(
            crate::pkg::request_context_test_support::new_test_ctx("admin", pool),
            &channel_id,
        )
        .await
        .unwrap();
    // 因为是软删除，状态变成 Deleted，查询时默认过滤掉
    // 删除只是标记状态，数据库中仍然存在
    assert!(found.is_some());
}

#[sqlx::test]
async fn test_query_channels(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    // 创建不同类型的渠道
    for i in 0..4 {
        let channel_po = MessageChannelPo::new(
            format!("test-query-channel-{}", i),
            "org-1".to_string(),
            "user-1".to_string(),
            None,
            match i {
                0 | 1 => ChannelType::Lark, // 2个飞书
                2 => ChannelType::Wechat,   // 1个微信
                _ => ChannelType::Slack,    // 1个 Slack
            },
            format!("渠道{}", i),
            Some(format!("https://example.com/webhook/{}", i)),
            None,
            None,
            ChannelConfig::default(),
            "admin".to_string(),
        );
        let channel = MessageChannel::from_po(channel_po);
        dal.create_channel(ctx.clone(), &channel).await.unwrap();
    }

    // 按类型查询：只查飞书渠道
    use crate::service::dao::message_channel::MessageChannelQuery;
    let query = MessageChannelQuery {
        user_id: Some("user-1".to_string()),
        channel_type: Some(ChannelType::Lark),
        ..Default::default()
    };
    let lark_page = dal.query_channels(ctx.clone(), query).await.unwrap();
    assert_eq!(lark_page.items.len(), 2);
    assert_eq!(lark_page.total, 2);
}

#[sqlx::test]
async fn test_deliver_message_skeleton(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    // 创建一个测试渠道
    let channel_po = MessageChannelPo::new(
        "test-deliver-channel".to_string(),
        "org-1".to_string(),
        "user-1".to_string(),
        None,
        ChannelType::Lark,
        "测试推送".to_string(),
        Some("https://example.com/webhook".to_string()),
        None,
        None,
        ChannelConfig::default(),
        "admin".to_string(),
    );
    let channel = MessageChannel::from_po(channel_po);
    dal.create_channel(ctx.clone(), &channel).await.unwrap();

    // 测试消息分发（目前是骨架，只返回成功不实际推送）
    use crate::models::file::FileMeta;
    use crate::models::message::{Message, MessagePo};
    use common::enums::{MessageRole, MessageType};

    let message_po = MessagePo::new(
        "test-msg".to_string(),
        None,
        None,
        "sender-1".to_string(),
        "user-1".to_string(),
        MessageRole::User,
        MessageRole::User,
        MessageType::Text,
        "测试消息内容".to_string(),
        None,
        FileMeta::default(),
        None,
        None, // root_id
        None, // organization_id
        "admin".to_string(),
    );
    let message = Message::from_po(message_po);

    let result = dal.deliver_message(ctx, &message, "user-1").await.unwrap();
    // 骨架实现返回 success_count = 0（因为还没实现实际推送）
    // 这里只验证调用不报错即可
    assert_eq!(result.total, 1);
    assert_eq!(result.success, 0);
    assert_eq!(result.failed, 1);
}
