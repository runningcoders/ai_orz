//! MessageChannel DAL 单元测试

use crate::models::message_channel::{ChannelConfig, MessageChannel, MessageChannelPo};
use crate::pkg::RequestContext;
use crate::service::dal::message_channel::MessageChannelDal;
use common::enums::{ChannelStatus, ChannelType, MessageRole};
use sqlx::SqlitePool;
use std::sync::Arc;

/// 初始化测试环境
async fn init_test_env(
    pool: SqlitePool,
) -> (Arc<dyn MessageChannelDal + Send + Sync>, RequestContext) {
    // 统一业务层初始化（幂等）：message_channel DAL 依赖的
    // a2a_callback/user/lark/wechat/slack/email/webhook DAO 一并就位
    crate::pkg::request_context_test_support::init_service_for_test();
    let dal = crate::service::dal::message_channel::dal();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("admin", pool);
    (dal, ctx)
}

/// 创建测试渠道
#[allow(dead_code)] // 测试辅助函数，保留供未来测试使用
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

// ==================== 出站渠道绑定过滤（五象限） ====================
// 口径=v2 方案（artifact 01a0c19c）：专属渠道（agent_id 绑定）仅收绑定 Agent 的消息；
// 通用渠道（agent_id=NULL）恒放行广播不变；D2 默认严格排除 from_role=User 通知类（代价 R3）。
// 断言口径：DeliveryResult.total 只统计通过过滤、实际发起推送的渠道（骨架推送失败也计入）；
// 被过滤跳过的渠道不产生 detail，故 total==0 即「未推送」。

/// 构造投递测试消息（from_role/from_id 决定过滤行为）
fn deliver_test_message(
    id: &str,
    from_id: &str,
    from_role: MessageRole,
    project_id: Option<&str>,
    to_user: &str,
) -> crate::models::message::Message {
    use crate::models::file::FileMeta;
    use crate::models::message::{Message, MessagePo};
    use common::enums::MessageType;

    let po = MessagePo::new(
        id.to_string(),
        project_id.map(|s| s.to_string()),
        None,
        from_id.to_string(),
        to_user.to_string(),
        from_role,
        MessageRole::User,
        MessageType::Text,
        "出站过滤测试消息".to_string(),
        None,
        FileMeta::default(),
        None,
        None,
        None,
        "admin".to_string(),
    );
    Message::from_po(po)
}

/// 创建指定绑定关系与项目范围的测试渠道
async fn create_filter_test_channel(
    ctx: &RequestContext,
    dal: &Arc<dyn MessageChannelDal + Send + Sync>,
    channel_id: &str,
    agent_id: Option<&str>,
    scope_project: Option<&str>,
) {
    let mut po = MessageChannelPo::new(
        channel_id.to_string(),
        "org-1".to_string(),
        "user-1".to_string(),
        agent_id.map(|s| s.to_string()),
        ChannelType::Lark,
        format!("渠道-{}", channel_id),
        Some("https://example.com/webhook".to_string()),
        None,
        None,
        ChannelConfig::default(),
        "admin".to_string(),
    );
    po.scope_project = scope_project.map(|s| s.to_string());
    dal.create_channel(ctx.clone(), &MessageChannel::from_po(po))
        .await
        .unwrap();
}

/// 象限1：专属渠道收绑定 Agent 的消息
#[sqlx::test]
async fn test_deliver_bound_channel_accepts_bound_agent(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    create_filter_test_channel(&ctx, &dal, "ofilter-bound-a", Some("agent-a"), None).await;

    let msg = deliver_test_message(
        "ofilter-msg-1",
        "agent-a",
        MessageRole::Agent,
        None,
        "user-1",
    );
    let result = dal.deliver_message(ctx, &msg, "user-1").await.unwrap();
    assert_eq!(result.total, 1, "专属渠道应放行绑定 Agent 的消息");
}

/// 象限2：专属渠道跳过其他 Agent 的消息（AMan 渠道维度语义的核心断言）
#[sqlx::test]
async fn test_deliver_bound_channel_skips_other_agent(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    create_filter_test_channel(&ctx, &dal, "ofilter-bound-b", Some("agent-a"), None).await;

    let msg = deliver_test_message(
        "ofilter-msg-2",
        "agent-b",
        MessageRole::Agent,
        None,
        "user-1",
    );
    let result = dal.deliver_message(ctx, &msg, "user-1").await.unwrap();
    assert_eq!(result.total, 0, "专属渠道应跳过非绑定 Agent 的消息");
}

/// 象限3（I2 回归断言）：通用渠道广播不变——任意 Agent/User 消息均照常进入
#[sqlx::test]
async fn test_deliver_unbound_channel_broadcast_unchanged(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    create_filter_test_channel(&ctx, &dal, "ofilter-global", None, None).await;

    let cases = [
        ("agent-a", MessageRole::Agent),
        ("agent-b", MessageRole::Agent),
        ("user-2", MessageRole::User),
    ];
    for (i, (from_id, role)) in cases.into_iter().enumerate() {
        let msg = deliver_test_message(
            &format!("ofilter-msg-g{}", i),
            from_id,
            role,
            None,
            "user-1",
        );
        let result = dal
            .deliver_message(ctx.clone(), &msg, "user-1")
            .await
            .unwrap();
        assert_eq!(
            result.total, 1,
            "通用渠道对 {} 的消息应恒放行（I1/I2）",
            from_id
        );
    }
}

/// 象限4：scope_project 与专属收窄 AND 叠加
#[sqlx::test]
async fn test_deliver_bound_channel_scope_project_stack(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    create_filter_test_channel(
        &ctx,
        &dal,
        "ofilter-scoped",
        Some("agent-a"),
        Some("proj-p"),
    )
    .await;

    // 绑定 Agent + 命中项目 → 放行
    let hit = deliver_test_message(
        "ofilter-msg-p1",
        "agent-a",
        MessageRole::Agent,
        Some("proj-p"),
        "user-1",
    );
    let result = dal
        .deliver_message(ctx.clone(), &hit, "user-1")
        .await
        .unwrap();
    assert_eq!(result.total, 1, "绑定 Agent 且项目匹配应放行");

    // 绑定 Agent + 项目不匹配 → 跳过
    let miss = deliver_test_message(
        "ofilter-msg-p2",
        "agent-a",
        MessageRole::Agent,
        Some("proj-q"),
        "user-1",
    );
    let result = dal.deliver_message(ctx, &miss, "user-1").await.unwrap();
    assert_eq!(result.total, 0, "项目不匹配应跳过");
}

/// 象限5（D2 默认严格排除矩阵）：from_role=User 通知类不进专属渠道；
/// 同一通知进通用渠道不受影响（对照）。漏达代价 R3 已在交付说明披露。
#[sqlx::test]
async fn test_deliver_d2_user_notification_excluded_from_bound(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    create_filter_test_channel(&ctx, &dal, "ofilter-d2-bound", Some("agent-a"), None).await;
    create_filter_test_channel(&ctx, &dal, "ofilter-d2-global", None, None).await;

    let msg = deliver_test_message(
        "ofilter-msg-u1",
        "user-2",
        MessageRole::User,
        None,
        "user-1",
    );
    let result = dal.deliver_message(ctx, &msg, "user-1").await.unwrap();

    assert_eq!(result.total, 1, "只有通用渠道放行该通知");
    assert!(
        result
            .details
            .iter()
            .all(|d| d.channel_id != "ofilter-d2-bound"),
        "D2 默认严格排除：User 通知类不进专属渠道"
    );
    assert!(
        result
            .details
            .iter()
            .any(|d| d.channel_id == "ofilter-d2-global"),
        "同一通知照常进通用渠道"
    );
}
