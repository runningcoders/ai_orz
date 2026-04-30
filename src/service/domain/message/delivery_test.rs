//! Message Delivery 单元测试

use super::domain;
use crate::pkg::RequestContext;
use common::enums::{MessageRole, MessageStatus, MessageType};
use sqlx::SqlitePool;
use uuid::Uuid;

fn new_ctx(user_id: &str, pool: sqlx::SqlitePool) -> RequestContext {
    RequestContext::new_simple(user_id, pool)
}

#[sqlx::test]
async fn test_send_to_agent_and_send_to_user(pool: SqlitePool) {
    // 每个测试新建独立实例，保证测试隔离，避免全局单例残留影响
    let message_dao = crate::service::dao::message::sqlite::new();
    let event_queue = crate::service::dao::event_queue::in_memory::new();
    let message_dal = crate::service::dal::message::new(message_dao, event_queue);
    let domain = crate::service::domain::message::new(message_dal);
    let ctx = new_ctx("admin", pool);

    let project_id = Uuid::now_v7().to_string();
    let task_id = Uuid::now_v7().to_string();

    // 用户发送给 Agent
    let sent_to_agent = domain
        .delivery()
        .send_to_agent(
            ctx.clone(),
            "user-1",
            MessageRole::User,
            "agent-1",
            "User message to agent",
            Some(&project_id),
            Some(&task_id),
                None,
        )
        .await
        .unwrap();

    assert_eq!(sent_to_agent.po.from_id, "user-1");
    assert_eq!(sent_to_agent.po.to_id, "agent-1");
    assert_eq!(sent_to_agent.po.from_role, MessageRole::User);
    assert_eq!(sent_to_agent.po.to_role, MessageRole::Agent);
    assert_eq!(sent_to_agent.po.message_type, MessageType::Text);
    assert_eq!(sent_to_agent.po.content, "User message to agent");
    assert_eq!(sent_to_agent.po.project_id, Some(project_id.clone()));
    assert_eq!(sent_to_agent.po.task_id, Some(task_id.clone()));
    assert_eq!(sent_to_agent.po.status, MessageStatus::Pending);

    // Agent 发送给用户
    let sent_to_user = domain
        .delivery()
        .send_to_user(
            ctx.clone(),
            "agent-1",
            "user-1",
            "Agent reply to user",
            Some(&project_id),
            Some(&task_id),
                None,
        )
        .await
        .unwrap();

    assert_eq!(sent_to_user.po.from_id, "agent-1");
    assert_eq!(sent_to_user.po.to_id, "user-1");
    assert_eq!(sent_to_user.po.from_role, MessageRole::Agent);
    assert_eq!(sent_to_user.po.to_role, MessageRole::User);
    assert_eq!(sent_to_user.po.content, "Agent reply to user");
    assert_eq!(sent_to_user.po.status, MessageStatus::Pending);
}

#[sqlx::test]
async fn test_dequeue_ack_nack(pool: SqlitePool) {
    // 每个测试新建独立实例，保证测试隔离，避免全局单例残留影响
    let message_dao = crate::service::dao::message::sqlite::new();
    let event_queue = crate::service::dao::event_queue::in_memory::new();
    let message_dal = crate::service::dal::message::new(message_dao, event_queue);
    let domain = crate::service::domain::message::new(message_dal);
    let ctx = new_ctx("admin", pool);

    // 队列初始为空
    let empty = domain.delivery().dequeue_next(ctx.clone()).await.unwrap();
    assert!(empty.is_none());

    // 发送一条消息入队
    let sent = domain
        .delivery()
        .send_to_agent(
            ctx.clone(),
            "user-1",
            MessageRole::User,
            "agent-1",
            "Message for dequeue test",
            None,
                    None,
            None,
        )
        .await
        .unwrap();

    // 出队
    let dequeued = domain.delivery().dequeue_next(ctx.clone()).await.unwrap();
    assert!(dequeued.is_some());
    let dequeued_msg = dequeued.unwrap();
    assert_eq!(dequeued_msg.po.id, sent.po.id);

    // 出队已经更新数据库状态为 Processing，但返回消息来自内存缓存，状态还是 Pending
    // 重新查询确认数据库状态已更新
    let dequeued_msg_reload = domain
        .management()
        .get_by_id(ctx.clone(), dequeued_msg.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(dequeued_msg_reload.po.status, MessageStatus::Processing);

    // 再次出队，队列已空
    let empty_after_dequeue = domain.delivery().dequeue_next(ctx.clone()).await.unwrap();
    assert!(empty_after_dequeue.is_none());

    // nack - 重新入队
    domain
        .delivery()
        .nack(ctx.clone(), dequeued_msg.id())
        .await
        .unwrap();

    // 重新查询确认状态回到 Pending
    let after_nack = domain
        .management()
        .get_by_id(ctx.clone(), dequeued_msg.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after_nack.po.status, MessageStatus::Pending);

    // 可以再次出队
    let dequeued_again = domain.delivery().dequeue_next(ctx.clone()).await.unwrap();
    assert!(dequeued_again.is_some());
    assert_eq!(dequeued_again.unwrap().po.id, dequeued_msg.id());

    // ack - 确认完成
    domain
        .delivery()
        .ack(ctx.clone(), dequeued_msg.id())
        .await
        .unwrap();

    // 确认状态变为 Processed
    let after_ack = domain
        .management()
        .get_by_id(ctx.clone(), dequeued_msg.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after_ack.po.status, MessageStatus::Processed);

    // 队列再次为空
    let empty_final = domain.delivery().dequeue_next(ctx.clone()).await.unwrap();
    assert!(empty_final.is_none());
}

#[sqlx::test]
async fn test_send_without_project_and_task(pool: SqlitePool) {
    // 每个测试新建独立实例，保证测试隔离，避免全局单例残留影响
    let message_dao = crate::service::dao::message::sqlite::new();
    let event_queue = crate::service::dao::event_queue::in_memory::new();
    let message_dal = crate::service::dal::message::new(message_dao, event_queue);
    let domain = crate::service::domain::message::new(message_dal);
    let ctx = new_ctx("admin", pool);

    // 不关联项目和任务
    let sent = domain
        .delivery()
        .send_to_agent(
            ctx.clone(),
            "user-1",
            MessageRole::User,
            "agent-1",
            "Direct message without context",
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(sent.po.project_id, None);
    assert_eq!(sent.po.task_id, None);
    assert_eq!(sent.po.content, "Direct message without context");

    // 可以查询到
    let found = domain
        .management()
        .get_by_id(ctx.clone(), &sent.po.id)
        .await
        .unwrap();
    assert!(found.is_some());
}
