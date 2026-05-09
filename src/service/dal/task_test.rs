//! Task DAL 单元测试

use common::enums::{TaskStatus, AssigneeType};
use crate::models::task::TaskPo;
use crate::pkg::RequestContext;
use crate::service::dao::task::TaskQuery;
use crate::service::dal::task::TaskDal;
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

/// 初始化测试环境
async fn init_test_env(pool: SqlitePool) -> (Arc<dyn TaskDal + Send + Sync>, RequestContext) {
    crate::service::dao::task::init();
    crate::service::dal::task::init();
    let dal = crate::service::dal::task::dal();
    let ctx = RequestContext::new_simple("admin", pool);
    (dal, ctx)
}

/// 创建测试任务
fn create_test_task(title: &str, assignee_id: &str) -> TaskPo {
    TaskPo::new(
        Uuid::now_v7().to_string(),
        title.to_string(),
        format!("Description for {}", title),
        1,
        vec![],
        None,
        None,
        None,
        vec![],
        Uuid::now_v7().to_string(), // root_user_id
        AssigneeType::User,
        assignee_id.to_string(),
        None,
        "admin".to_string(),
    )
}

#[sqlx::test]
async fn test_create_and_find_by_id(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let assignee_id = Uuid::now_v7().to_string();

    let task = create_test_task("Test Task", &assignee_id);
    let task_id = task.id.clone();
    let root_user_id = task.root_user_id.clone();

    dal.create(ctx.clone(), &task).await.unwrap();
    let found = dal.find_by_id(ctx, &task_id).await.unwrap().unwrap();

    assert_eq!(found.id, task_id);
    assert_eq!(found.title, "Test Task");
    assert_eq!(found.root_user_id, root_user_id);
    assert_eq!(found.assignee_type, AssigneeType::User);
    assert_eq!(found.assignee_id, assignee_id);
    assert_eq!(found.status, TaskStatus::Pending);
}

#[sqlx::test]
async fn test_list_by_assignee(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let assignee_id = Uuid::now_v7().to_string();
    let other_assignee_id = Uuid::now_v7().to_string();

    // Create 3 tasks for assignee 1
    for i in 0..3 {
        let task = create_test_task(&format!("Task {}", i), &assignee_id);
        dal.create(ctx.clone(), &task).await.unwrap();
    }

    // Create 1 task for assignee 2
    let task = create_test_task("Other Task", &other_assignee_id);
    dal.create(ctx.clone(), &task).await.unwrap();

    let tasks = dal.list_by_assignee(ctx, Some(AssigneeType::User), &assignee_id, None).await.unwrap();
    assert_eq!(tasks.len(), 3);
}

#[sqlx::test]
async fn test_list_by_status(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let assignee_id = Uuid::now_v7().to_string();

    // Create 2 pending tasks
    for i in 0..2 {
        let task = create_test_task(&format!("Pending Task {}", i), &assignee_id);
        dal.create(ctx.clone(), &task).await.unwrap();
    }

    // Create 1 completed task
    let completed_task = create_test_task("Completed Task", &assignee_id);
    let completed_task_id = completed_task.id.clone();
    dal.create(ctx.clone(), &completed_task).await.unwrap();
    dal.update_status(ctx.clone(), &completed_task_id, TaskStatus::Completed, "admin").await.unwrap();

    // Query only pending tasks
    let tasks = dal.list_by_status(
        ctx.clone(),
        Some(AssigneeType::User),
        &assignee_id,
        vec![TaskStatus::Pending],
        None,
    ).await.unwrap();
    assert_eq!(tasks.len(), 2);

    // Query completed tasks
    let tasks = dal.list_by_status(
        ctx,
        Some(AssigneeType::User),
        &assignee_id,
        vec![TaskStatus::Completed],
        None,
    ).await.unwrap();
    assert_eq!(tasks.len(), 1);
}

#[sqlx::test]
async fn test_query(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let assignee_id = Uuid::now_v7().to_string();

    for i in 0..3 {
        let task = create_test_task(&format!("Task {}", i), &assignee_id);
        dal.create(ctx.clone(), &task).await.unwrap();
    }

    let query = TaskQuery {
        assignee_type: Some(AssigneeType::User),
        assignee_id: Some(assignee_id),
        status_in: Some(vec![TaskStatus::Pending]),
        limit: Some(2),
    };

    let tasks = dal.query(ctx, query).await.unwrap();
    assert_eq!(tasks.len(), 2);
}

#[sqlx::test]
async fn test_update_task(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let assignee_id = Uuid::now_v7().to_string();

    let mut task = create_test_task("Original Title", &assignee_id);
    let task_id = task.id.clone();
    dal.create(ctx.clone(), &task).await.unwrap();

    // Update task
    task.title = "Updated Title".to_string();
    task.description = "Updated description".to_string();
    task.priority = 2;
    dal.update(ctx.clone(), &task).await.unwrap();

    let found = dal.find_by_id(ctx, &task_id).await.unwrap().unwrap();
    assert_eq!(found.title, "Updated Title");
    assert_eq!(found.description, "Updated description");
    assert_eq!(found.priority, 2);
}

#[sqlx::test]
async fn test_update_status_and_cancel(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let assignee_id = Uuid::now_v7().to_string();

    let task = create_test_task("Test Task", &assignee_id);
    let task_id = task.id.clone();
    dal.create(ctx.clone(), &task).await.unwrap();

    // Update status to InProgress
    dal.update_status(ctx.clone(), &task_id, TaskStatus::InProgress, "admin").await.unwrap();
    let found = dal.find_by_id(ctx.clone(), &task_id).await.unwrap().unwrap();
    assert_eq!(found.status, TaskStatus::InProgress);

    // Cancel task
    dal.cancel(ctx.clone(), &task_id, "admin").await.unwrap();
    // Cancelled tasks are soft-deleted, so find_by_id returns None
    let found = dal.find_by_id(ctx, &task_id).await.unwrap();
    assert!(found.is_none());
}

#[sqlx::test]
async fn test_count_by_assignee(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let assignee_id = Uuid::now_v7().to_string();

    for i in 0..5 {
        let task = create_test_task(&format!("Task {}", i), &assignee_id);
        dal.create(ctx.clone(), &task).await.unwrap();
    }

    let count = dal.count_by_assignee(ctx, &assignee_id).await.unwrap();
    assert_eq!(count, 5);
}

#[sqlx::test]
async fn test_count_by_assignee_and_status(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let assignee_id = Uuid::now_v7().to_string();

    // Create 3 pending tasks
    for i in 0..3 {
        let task = create_test_task(&format!("Task {}", i), &assignee_id);
        dal.create(ctx.clone(), &task).await.unwrap();
    }

    // Create 2 completed tasks
    for i in 0..2 {
        let task = create_test_task(&format!("Completed Task {}", i), &assignee_id);
        let task_id = task.id.clone();
        dal.create(ctx.clone(), &task).await.unwrap();
        dal.update_status(ctx.clone(), &task_id, TaskStatus::Completed, "admin").await.unwrap();
    }

    let pending_count = dal.count_by_assignee_and_status(
        ctx.clone(),
        &assignee_id,
        TaskStatus::Pending,
    ).await.unwrap();
    assert_eq!(pending_count, 3);

    let completed_count = dal.count_by_assignee_and_status(
        ctx,
        &assignee_id,
        TaskStatus::Completed,
    ).await.unwrap();
    assert_eq!(completed_count, 2);
}
