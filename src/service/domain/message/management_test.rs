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

#[sqlx::test]
async fn test_list_by_project_id(pool: SqlitePool) {
    // 初始化依赖：dao -> dal -> domain (和线上初始化顺序一致)
    crate::service::dao::message::init();
    crate::service::dao::event_queue::init_message();
    crate::service::dal::message::init();
    super::init();
    let domain = domain();
    let ctx = new_ctx("admin", pool);

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
