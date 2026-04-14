//! Task DAO SQLite 单元测试

use crate::models::task::TaskPo;
use crate::pkg::RequestContext;
use common::enums::{TaskStatus, AssigneeType};
use crate::service::dao::task::{self, TaskDaoTrait};
use uuid::Uuid;
use sqlx::SqlitePool;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    RequestContext::new_simple(user_id, pool)
}

/// 测试插入新任务并按 ID 查询
#[sqlx::test]
async fn test_insert_and_find_by_id(pool: SqlitePool) {
    crate::service::dao::task::init();
    let task_dao = crate::service::dao::task::dao();

    let task_id = Uuid::now_v7().to_string();
    let assignee_id = "user-123";
    let task = TaskPo::new(
        task_id.clone(),
        "Complete the migration to sqlx 0.8".to_string(),
        "Fix all remaining issues and add unit tests after migration".to_string(),
        5, // 高优先级
        vec!["rust".to_string(), "sqlx".to_string(), "migration".to_string()],
        None, // 无截止时间
        "test-user".to_string(), // root_user_id
        AssigneeType::User,
        assignee_id.to_string(),
        None, // 无项目
        "test-user".to_string(),
    );
    let result = task_dao.insert(new_ctx("test-user", pool.clone()), &task).await;
    assert!(result.is_ok());

    let found = task_dao.find_by_id(new_ctx("test-user", pool.clone()), &task_id).await.unwrap();
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, task_id);
    assert_eq!(found.title, "Complete the migration to sqlx 0.8");
    assert_eq!(found.description, "Fix all remaining issues and add unit tests after migration");
    assert_eq!(found.status, TaskStatus::Pending);
    assert_eq!(found.priority, 5);
    assert_eq!(found.get_tags(), vec!["rust", "sqlx", "migration"]);
    assert!(found.due_at.is_none());
    assert_eq!(found.assignee_type, AssigneeType::User);
    assert_eq!(found.assignee_id, assignee_id);
    assert_eq!(found.root_user_id, "test-user");
    assert!(found.project_id.is_none());
}

/// 测试更新任务信息
#[sqlx::test]
async fn test_update_task(pool: SqlitePool) {
    crate::service::dao::task::init();
    let task_dao = crate::service::dao::task::dao();

    let task_id = Uuid::now_v7().to_string();
    let assignee_id = "user-123";
    let mut task = TaskPo::new(
        task_id.clone(),
        "Original Title".to_string(),
        "Original Description".to_string(),
        5,
        vec![],
        None,
        "test-user".to_string(), // root_user_id
        AssigneeType::User,
        assignee_id.to_string(),
        None,
        "test-user".to_string(),
    );
    task_dao.insert(new_ctx("test-user", pool.clone()), &task).await.unwrap();

    // 更新任务
    let mut found = task_dao.find_by_id(new_ctx("test-user", pool.clone()), &task_id).await.unwrap().unwrap();
    found.title = "Updated Title".to_string();
    found.status = TaskStatus::InProgress;
    found.priority = 10;
    let update_result = task_dao.update(new_ctx("editor", pool.clone()), &found).await;
    assert!(update_result.is_ok());

    // 验证更新生效
    let updated = task_dao.find_by_id(new_ctx("editor", pool.clone()), &task_id).await.unwrap().unwrap();
    assert_eq!(updated.title, "Updated Title");
    assert_eq!(updated.status, TaskStatus::InProgress);
    assert_eq!(updated.priority, 10);
    assert_eq!(updated.modified_by, "editor");
}

/// 测试更新任务状态
#[sqlx::test]
async fn test_update_task_status(pool: SqlitePool) {
    crate::service::dao::task::init();
    let task_dao = crate::service::dao::task::dao();

    let task_id = Uuid::now_v7().to_string();
    let task = TaskPo::new(
        task_id.clone(),
        "Test task".to_string(),
        "Description".to_string(),
        5,
        vec![],
        None,
        "test-user".to_string(), // root_user_id
        AssigneeType::User,
        "user-123".to_string(),
        None,
        "test-user".to_string(),
    );
    task_dao.insert(new_ctx("test-user", pool.clone()), &task).await.unwrap();

    // 更新状态到完成
    let status_result = task_dao.update_status(new_ctx("editor", pool.clone()), &task_id, TaskStatus::Completed, "editor").await;
    assert!(status_result.is_ok());

    let updated = task_dao.find_by_id(new_ctx("editor", pool.clone()), &task_id).await.unwrap().unwrap();
    assert_eq!(updated.status, TaskStatus::Completed);
}

/// 测试按分配人查询列表和优先级排序
#[sqlx::test]
async fn test_list_by_assignee(pool: SqlitePool) {
    crate::service::dao::task::init();
    let task_dao = crate::service::dao::task::dao();

    let assignee_id = "user-123";

    // 插入三个任务，不同优先级
    let task_id1 = Uuid::now_v7().to_string();
    let task1 = TaskPo::new(
        task_id1.clone(),
        "Low priority task".to_string(),
        "".to_string(),
        3,
        vec![],
        None,
        "test-user".to_string(), // root_user_id
        AssigneeType::User,
        assignee_id.to_string(),
        None,
        "test-user".to_string(),
    );
    task_dao.insert(new_ctx("test-user", pool.clone()), &task1).await.unwrap();

    let task_id2 = Uuid::now_v7().to_string();
    let mut task2 = TaskPo::new(
        task_id2.clone(),
        "Medium priority task".to_string(),
        "".to_string(),
        5,
        vec![],
        None,
        "test-user".to_string(), // root_user_id
        AssigneeType::User,
        assignee_id.to_string(),
        None,
        "test-user".to_string(),
    );
    task_dao.insert(new_ctx("test-user", pool.clone()), &task2).await.unwrap();

    // 更新第一个任务为更高优先级
    task2.priority = 10;
    task_dao.update(new_ctx("test-user", pool.clone()), &task2).await.unwrap();

    let task_id3 = Uuid::now_v7().to_string();
    let task3 = TaskPo::new(
        task_id3.clone(),
        "High priority task".to_string(),
        "".to_string(),
        8,
        vec![],
        None,
        "test-user".to_string(), // root_user_id
        AssigneeType::User,
        assignee_id.to_string(),
        None,
        "test-user".to_string(),
    );
    task_dao.insert(new_ctx("test-user", pool.clone()), &task3).await.unwrap();

    // 查询，验证优先级排序：10 > 8 > 3 → task2 (id2), task3 (id3), task1 (id1)
    let list = task_dao.list_by_assignee(new_ctx("test-user", pool.clone()), Some(AssigneeType::User), assignee_id, Some(10)).await.unwrap();
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].id, task_id2);
    assert_eq!(list[1].id, task_id3);
    assert_eq!(list[2].id, task_id1);
}

/// 测试按状态查询
#[sqlx::test]
async fn test_list_by_status(pool: SqlitePool) {
    crate::service::dao::task::init();
    let task_dao = crate::service::dao::task::dao();

    let assignee_id = "user-123";

    // 插入不同状态的任务
    let task_pending = TaskPo::new(
        Uuid::now_v7().to_string(),
        "Pending task".to_string(),
        "".to_string(),
        5,
        vec![],
        None,
        "test-user".to_string(), // root_user_id
        AssigneeType::User,
        assignee_id.to_string(),
        None,
        "test-user".to_string(),
    );
    task_dao.insert(new_ctx("test-user", pool.clone()), &task_pending).await.unwrap();

    let task_in_progress = TaskPo::new(
        Uuid::now_v7().to_string(),
        "InProgress task".to_string(),
        "".to_string(),
        5,
        vec![],
        None,
        "test-user".to_string(), // root_user_id
        AssigneeType::User,
        assignee_id.to_string(),
        None,
        "test-user".to_string(),
    );
    task_dao.insert(new_ctx("test-user", pool.clone()), &task_in_progress).await.unwrap();
    task_dao.update_status(new_ctx("test-user", pool.clone()), &task_in_progress.id, TaskStatus::InProgress, "test-user").await.unwrap();

    let task_completed = TaskPo::new(
        Uuid::now_v7().to_string(),
        "Completed task".to_string(),
        "".to_string(),
        5,
        vec![],
        None,
        "test-user".to_string(), // root_user_id
        AssigneeType::User,
        assignee_id.to_string(),
        None,
        "test-user".to_string(),
    );
    task_dao.insert(new_ctx("test-user", pool.clone()), &task_completed).await.unwrap();
    task_dao.update_status(new_ctx("test-user", pool.clone()), &task_completed.id, TaskStatus::Completed, "test-user").await.unwrap();

    // 查询 Pending
    let pending_list = task_dao.list_by_status(new_ctx("test-user", pool.clone()), Some(AssigneeType::User), assignee_id, vec![TaskStatus::Pending], Some(10)).await.unwrap();
    assert_eq!(pending_list.len(), 1);
    assert_eq!(pending_list[0].id, task_pending.id);

    // 查询 Completed
    let completed_list = task_dao.list_by_status(new_ctx("test-user", pool.clone()), Some(AssigneeType::User), assignee_id, vec![TaskStatus::Completed], Some(10)).await.unwrap();
    assert_eq!(completed_list.len(), 1);
    assert_eq!(completed_list[0].id, task_completed.id);
}

/// 测试计数功能
#[sqlx::test]
async fn test_count_functions(pool: SqlitePool) {
    crate::service::dao::task::init();
    let task_dao = crate::service::dao::task::dao();

    let assignee_id = "user-123";

    // 插入三个任务，两个 Pending，一个 Completed
    for i in 0..2 {
        let task = TaskPo::new(
            Uuid::now_v7().to_string(),
            format!("Pending task {}", i),
            "".to_string(),
            5,
            vec![],
            None,
            "test-user".to_string(), // root_user_id
            AssigneeType::User,
            assignee_id.to_string(),
            None,
            "test-user".to_string(),
        );
        task_dao.insert(new_ctx("test-user", pool.clone()), &task).await.unwrap();
    }

    let task_completed = TaskPo::new(
        Uuid::now_v7().to_string(),
        "Completed task".to_string(),
        "".to_string(),
        5,
        vec![],
        None,
        "test-user".to_string(), // root_user_id
        AssigneeType::User,
        assignee_id.to_string(),
        None,
        "test-user".to_string(),
    );
    task_dao.insert(new_ctx("test-user", pool.clone()), &task_completed).await.unwrap();
    task_dao.update_status(new_ctx("test-user", pool.clone()), &task_completed.id, TaskStatus::Completed, "test-user").await.unwrap();

    // 总数
    let total_count = task_dao.count_by_assignee(new_ctx("test-user", pool.clone()), assignee_id).await.unwrap();
    assert_eq!(total_count, 3);

    // 按状态计数
    let pending_count = task_dao.count_by_assignee_and_status(new_ctx("test-user", pool.clone()), assignee_id, TaskStatus::Pending).await.unwrap();
    assert_eq!(pending_count, 2);

    let completed_count = task_dao.count_by_assignee_and_status(new_ctx("test-user", pool.clone()), assignee_id, TaskStatus::Completed).await.unwrap();
    assert_eq!(completed_count, 1);
}

/// 测试取消任务（软删除，status = 0 = Cancelled）
#[sqlx::test]
async fn test_cancel_task(pool: SqlitePool) {
    crate::service::dao::task::init();
    let task_dao = crate::service::dao::task::dao();

    let assignee_id = "user-123";

    // 插入两个 pending 任务
    let task1_id = Uuid::now_v7().to_string();
    let task1 = TaskPo::new(
        task1_id.clone(),
        "Task to keep".to_string(),
        "".to_string(),
        5,
        vec![],
        None,
        "test-user".to_string(), // root_user_id
        AssigneeType::User,
        assignee_id.to_string(),
        None,
        "test-user".to_string(),
    );
    task_dao.insert(new_ctx("test-user", pool.clone()), &task1).await.unwrap();

    let task2_id = Uuid::now_v7().to_string();
    let task2 = TaskPo::new(
        task2_id.clone(),
        "Task to cancel".to_string(),
        "".to_string(),
        5,
        vec![],
        None,
        "test-user".to_string(), // root_user_id
        AssigneeType::User,
        assignee_id.to_string(),
        None,
        "test-user".to_string(),
    );
    task_dao.insert(new_ctx("test-user", pool.clone()), &task2).await.unwrap();

    // 取消 task2
    let cancel_result = task_dao.update_status(new_ctx("editor", pool.clone()), &task2_id, TaskStatus::Cancelled, "editor").await;
    assert!(cancel_result.is_ok());

    // 验证取消后查询不到
    let found_cancelled = task_dao.find_by_id(new_ctx("editor", pool.clone()), &task2_id).await.unwrap();
    assert!(found_cancelled.is_none());

    // 验证总数减少为 1
    let total_count_after_cancel = task_dao.count_by_assignee(new_ctx("test-user", pool.clone()), assignee_id).await.unwrap();
    assert_eq!(total_count_after_cancel, 1);
}

/// 测试空列表边界情况
#[sqlx::test]
async fn test_empty_task_list(pool: SqlitePool) {
    crate::service::dao::task::init();
    let task_dao = crate::service::dao::task::dao();

    let list = task_dao.list_by_assignee(new_ctx("test-user", pool.clone()), None, "nonexistent-user", Some(10)).await.unwrap();
    assert!(list.is_empty());

    let count = task_dao.count_by_assignee(new_ctx("test-user", pool.clone()), "nonexistent-user").await.unwrap();
    assert_eq!(count, 0);
}
