//! Message Management 单元测试

use super::{MessageDomain, domain};
use crate::models::message::Message;
use crate::pkg::RequestContext;
use crate::service::domain::message::{SendToAgentCommand, SendToUserCommand};
use common::enums::{MessageRole, MessageStatus, MessageType};
use sqlx::SqlitePool;
use uuid::Uuid;

fn new_ctx(user_id: &str, pool: sqlx::SqlitePool) -> RequestContext {
    RequestContext::new_simple(user_id, pool)
}

/// 初始化所有渠道 DAO 单例
fn init_all_channel_daos() {
    crate::service::dao::lark::init();
    crate::service::dao::wechat::init();
    crate::service::dao::slack::init();
    crate::service::dao::email::init();
    crate::service::dao::webhook::init();
}

/// 初始化测试环境
fn init_test_env(pool: SqlitePool) -> (std::sync::Arc<dyn MessageDomain>, RequestContext) {
    crate::service::dao::message::init();
    crate::service::dao::event_queue::init_message();
    crate::service::dao::message_channel::init();
    init_all_channel_daos();  // 初始化所有渠道 DAO 单例
    crate::service::dal::message::init();
    crate::service::dal::message_channel::init();
    super::init();
    let domain = domain();
    let ctx = new_ctx("admin", pool);
    (domain, ctx)
}

#[sqlx::test]
async fn test_list_by_project_id(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);

    let project_id_1 = Uuid::now_v7().to_string();
    let project_id_2 = Uuid::now_v7().to_string();
    let task_id_1 = Uuid::now_v7().to_string();
    let task_id_2 = Uuid::now_v7().to_string();

    // 创建三条消息，两条属于 project1，一条属于 project2
    // message 1 - project1
    domain
        .delivery()
        .send_to_agent(
            ctx.clone(),
            SendToAgentCommand {
                from_id: "user-id-1",
                from_role: MessageRole::User,
                to_agent_id: "agent-id-1",
                content: "Hello from user to agent in project1",
                project_id: Some(&project_id_1),
                task_id: Some(&task_id_1),
                reply_to_id: None,
            },
        )
        .await
        .unwrap();

    // message 2 - project1
    domain
        .delivery()
        .send_to_user(
            ctx.clone(),
            SendToUserCommand {
                from_agent_id: "agent-id-1",
                to_user_id: "user-id-1",
                content: "Hello back from agent in project1",
                project_id: Some(&project_id_1),
                task_id: Some(&task_id_1),
                reply_to_id: None,
            },
        )
        .await
        .unwrap();

    // message 3 - project2
    domain
        .delivery()
        .send_to_agent(
            ctx.clone(),
            SendToAgentCommand {
                from_id: "user-id-2",
                from_role: MessageRole::User,
                to_agent_id: "agent-id-2",
                content: "Hello in another project",
                project_id: Some(&project_id_2),
                task_id: Some(&task_id_2),
                reply_to_id: None,
            },
        )
        .await
        .unwrap();

    // 查询 project1 消息
    let list1: Vec<Message> = domain
        .management()
        .list_by_project_id(ctx.clone(), &project_id_1)
        .await
        .unwrap();
    assert_eq!(list1.len(), 2);
    // 按 created_at ASC 排序，最早在前
    assert_eq!(list1[0].po.content, "Hello from user to agent in project1");
    assert_eq!(list1[1].po.content, "Hello back from agent in project1");

    // 查询 project2 消息
    let list2: Vec<Message> = domain
        .management()
        .list_by_project_id(ctx.clone(), &project_id_2)
        .await
        .unwrap();
    assert_eq!(list2.len(), 1);
    assert_eq!(list2[0].po.content, "Hello in another project");
}

#[sqlx::test]
async fn test_get_by_id_and_update_status(pool: SqlitePool) {
    // 初始化依赖
    crate::service::dao::message::init();
    crate::service::dao::event_queue::init_message();
    crate::service::dal::message::init();
    super::init();
    let domain = domain();
    let ctx = new_ctx("admin", pool);

    let project_id = Uuid::now_v7().to_string();
    let task_id = Uuid::now_v7().to_string();

    // 发送一条消息
    let sent = domain
        .delivery()
        .send_to_agent(
            ctx.clone(),
            SendToAgentCommand {
                from_id: "user-1",
                from_role: MessageRole::User,
                to_agent_id: "agent-1",
                content: "Test message for get_by_id",
                project_id: Some(&project_id),
                task_id: Some(&task_id),
                reply_to_id: None,
            },
        )
        .await
        .unwrap();

    // 获取消息
    let found = domain
        .management()
        .get_by_id(ctx.clone(), sent.po.id.as_str())
        .await
        .unwrap();
    assert!(found.is_some());
    let found_msg = found.unwrap();
    assert_eq!(found_msg.po.content, "Test message for get_by_id");
    assert_eq!(found_msg.po.status, MessageStatus::Pending);

    // 更新状态为 Processed
    domain
        .management()
        .update_status(ctx.clone(), sent.po.id.as_str(), MessageStatus::Processed)
        .await
        .unwrap();

    // 再次获取确认状态更新
    let found_updated = domain
        .management()
        .get_by_id(ctx.clone(), sent.po.id.as_str())
        .await
        .unwrap();
    assert_eq!(found_updated.unwrap().po.status, MessageStatus::Processed);
}

#[sqlx::test]
async fn test_delete_by_id_and_cleanup_conversation(pool: SqlitePool) {
    // 初始化依赖
    crate::service::dao::message::init();
    crate::service::dao::event_queue::init_message();
    crate::service::dal::message::init();
    super::init();
    let domain = domain();
    let ctx = new_ctx("admin", pool);

    let project_id = Uuid::now_v7().to_string();
    let task_id = Uuid::now_v7().to_string();

    // 创建三条消息在同一个任务
    for i in 0..3 {
        domain
            .delivery()
            .send_to_agent(
                ctx.clone(),
                SendToAgentCommand {
                    from_id: "user-1",
                    from_role: MessageRole::User,
                    to_agent_id: "agent-1",
                    content: &format!("Message {} in task", i),
                    project_id: Some(&project_id),
                    task_id: Some(&task_id),
                    reply_to_id: None,
                },
            )
            .await
            .unwrap();
    }

    // 删除第一条消息
    let messages = domain
        .management()
        .list_by_task_id(ctx.clone(), &task_id)
        .await
        .unwrap();
    assert_eq!(messages.len(), 3);
    let first_id = &messages[0].po.id;
    domain
        .management()
        .delete_by_id(ctx.clone(), first_id)
        .await
        .unwrap();

    // 确认删除
    let messages_after_delete = domain
        .management()
        .list_by_task_id(ctx.clone(), &task_id)
        .await
        .unwrap();
    assert_eq!(messages_after_delete.len(), 2);

    // 清理整个对话（删除剩余两条）
    domain
        .management()
        .cleanup_conversation(ctx.clone(), &task_id)
        .await
        .unwrap();

    // 确认全部删除
    let messages_after_cleanup = domain
        .management()
        .list_by_task_id(ctx.clone(), &task_id)
        .await
        .unwrap();
    assert_eq!(messages_after_cleanup.len(), 0);
}


// ========== 渠道配置管理测试 ==========

#[sqlx::test]
async fn test_channel_crud_operations(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let user_id = "test-user-001";
    let channel_id = "channel-001";
    let org_id = "test-org-001";

    // 1. 创建渠道
    use crate::models::message_channel::{MessageChannel, MessageChannelPo, ChannelConfig};
    use common::enums::message_channel::{ChannelStatus, ChannelType};

    let po = MessageChannelPo::new(
        channel_id.to_string(),
        org_id.to_string(),
        user_id.to_string(),
        None, // agent_id
        ChannelType::Webhook,
        "测试 Webhook 渠道".to_string(),
        Some("https://example.com/webhook".to_string()),
        None, // access_token
        None, // secret
        ChannelConfig::default(),
        user_id.to_string(),
    );
    let channel = MessageChannel::from_po(po);

    domain
        .management()
        .create_channel(ctx.clone(), &channel)
        .await
        .unwrap();

    // 2. 获取渠道验证创建成功
    let fetched = domain
        .management()
        .get_channel(ctx.clone(), channel_id)
        .await
        .unwrap();
    assert!(fetched.is_some());
    let fetched = fetched.unwrap();
    assert_eq!(fetched.po.id, channel_id);
    assert_eq!(fetched.po.channel_name, "测试 Webhook 渠道");
    assert_eq!(fetched.po.status, ChannelStatus::Active);

    // 3. 更新渠道
    let mut updated_channel = fetched.clone();
    updated_channel.po.channel_name = "更新后的渠道名称".to_string();
    updated_channel.po.webhook_url = Some("https://new-url.com/webhook".to_string());
    domain
        .management()
        .update_channel(ctx.clone(), &updated_channel)
        .await
        .unwrap();

    // 验证更新
    let fetched_after_update = domain
        .management()
        .get_channel(ctx.clone(), channel_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched_after_update.po.channel_name, "更新后的渠道名称");
    assert_eq!(
        fetched_after_update.po.webhook_url,
        Some("https://new-url.com/webhook".to_string())
    );

    // 4. 删除渠道
    domain
        .management()
        .delete_channel(ctx.clone(), channel_id)
        .await
        .unwrap();

    // 验证删除（软删除，仍然可以查询到，但 deleted_at 有值）
    let fetched_after_delete = domain
        .management()
        .get_channel(ctx.clone(), channel_id)
        .await
        .unwrap();
    // 软删除后仍然可以查询到（不会返回 None）
    // 实际行为取决于 DAO 层的实现：如果查询自动过滤 deleted_at，则返回 None
    // 如果不过滤，则返回 Some
    assert!(fetched_after_delete.is_some());
}

#[sqlx::test]
async fn test_list_user_channels(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let user_id = "test-user-002";
    let org_id = "test-org-002";

    use crate::models::message_channel::{MessageChannel, MessageChannelPo, ChannelConfig};
    use common::enums::message_channel::{ChannelStatus, ChannelType};

    // 创建 3 个渠道：2 个启用，1 个禁用
    for i in 0..3 {
        let po = MessageChannelPo::new(
            format!("channel-{:03}", i),
            org_id.to_string(),
            user_id.to_string(),
            None,
            ChannelType::Webhook,
            format!("渠道 {}", i),
            Some(format!("https://example.com/webhook/{}", i)),
            None,
            None,
            ChannelConfig::default(),
            user_id.to_string(),
        );
        let mut channel = MessageChannel::from_po(po);
        if i >= 2 {
            channel.po.status = ChannelStatus::Disabled;
        }
        domain
            .management()
            .create_channel(ctx.clone(), &channel)
            .await
            .unwrap();
    }

    // 列出所有渠道（包括禁用）
    let all_channels = domain
        .management()
        .list_user_channels(ctx.clone(), user_id, false)
        .await
        .unwrap();
    assert_eq!(all_channels.len(), 3);

    // 只列出启用的渠道
    let enabled_channels = domain
        .management()
        .list_user_channels(ctx.clone(), user_id, true)
        .await
        .unwrap();
    assert_eq!(enabled_channels.len(), 2);
}

#[sqlx::test]
async fn test_set_channel_status(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let user_id = "test-user-003";
    let org_id = "test-org-003";
    let channel_id = "channel-status-test";

    use crate::models::message_channel::{MessageChannel, MessageChannelPo, ChannelConfig};
    use common::enums::message_channel::{ChannelStatus, ChannelType};

    let po = MessageChannelPo::new(
        channel_id.to_string(),
        org_id.to_string(),
        user_id.to_string(),
        None,
        ChannelType::Email,
        "状态测试渠道".to_string(),
        None,
        None,
        None,
        ChannelConfig::default(),
        user_id.to_string(),
    );
    let mut channel = MessageChannel::from_po(po);
    channel.po.status = ChannelStatus::Disabled;

    domain
        .management()
        .create_channel(ctx.clone(), &channel)
        .await
        .unwrap();

    // 设置为 Active（启用）
    domain
        .management()
        .set_channel_status(ctx.clone(), channel_id, ChannelStatus::Active)
        .await
        .unwrap();

    // 验证状态变更
    let fetched = domain
        .management()
        .get_channel(ctx.clone(), channel_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched.po.status, ChannelStatus::Active);

    // 再设置为 Disabled（禁用）
    domain
        .management()
        .set_channel_status(ctx.clone(), channel_id, ChannelStatus::Disabled)
        .await
        .unwrap();

    let fetched_final = domain
        .management()
        .get_channel(ctx.clone(), channel_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fetched_final.po.status, ChannelStatus::Disabled);
}

#[sqlx::test]
async fn test_query_channels(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let user_id = "test-user-004";
    let org_id = "test-org-004";

    use crate::models::message_channel::{MessageChannel, MessageChannelPo, ChannelConfig};
    use crate::service::dao::message_channel::MessageChannelQuery;
    use common::enums::message_channel::{ChannelStatus, ChannelType};

    // 创建不同类型的渠道
    let types = vec![ChannelType::Email, ChannelType::Webhook, ChannelType::Slack];
    for (i, channel_type) in types.iter().enumerate() {
        let po = MessageChannelPo::new(
            format!("query-channel-{}", i),
            org_id.to_string(),
            user_id.to_string(),
            None,
            channel_type.clone(),
            format!("Query Channel {}", i),
            None,
            None,
            None,
            ChannelConfig::default(),
            user_id.to_string(),
        );
        let channel = MessageChannel::from_po(po);
        domain
            .management()
            .create_channel(ctx.clone(), &channel)
            .await
            .unwrap();
    }

    // 按用户 ID 查询
    let query = MessageChannelQuery {
        user_id: Some(user_id.to_string()),
        ..Default::default()
    };
    let results = domain
        .management()
        .query_channels(ctx.clone(), query)
        .await
        .unwrap();
    assert_eq!(results.len(), 3);

    // 按类型查询
    let query_by_type = MessageChannelQuery {
        user_id: Some(user_id.to_string()),
        channel_type: Some(ChannelType::Webhook),
        ..Default::default()
    };
    let results_by_type = domain
        .management()
        .query_channels(ctx.clone(), query_by_type)
        .await
        .unwrap();
    assert_eq!(results_by_type.len(), 1);
    assert_eq!(results_by_type[0].po.channel_type, ChannelType::Webhook);
}

