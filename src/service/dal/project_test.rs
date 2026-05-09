//! Project DAL 单元测试

use common::enums::ProjectStatus;
use crate::models::project::ProjectPo;
use crate::pkg::RequestContext;
use crate::service::dao::project::ProjectQuery;
use crate::service::dal::project::ProjectDal;
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

/// 初始化测试环境
async fn init_test_env(pool: SqlitePool) -> (Arc<dyn ProjectDal + Send + Sync>, RequestContext) {
    crate::service::dao::project::init();
    crate::service::dal::project::init();
    let dal = crate::service::dal::project::dal();
    let ctx = RequestContext::new_simple("admin", pool);
    (dal, ctx)
}

/// 创建测试项目
fn create_test_project(name: &str, root_user_id: &str) -> ProjectPo {
    ProjectPo::new(
        Uuid::now_v7().to_string(),
        name.to_string(),
        format!("Description for {}", name),
        None,
        None,
        1,
        vec![],
        root_user_id.to_string(),
        None,
        None,
        None,
        None,
        "admin".to_string(),
    )
}

#[sqlx::test]
async fn test_create_and_find_by_id(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let root_user_id = Uuid::now_v7().to_string();

    let project = create_test_project("Test Project", &root_user_id);
    let project_id = project.id.clone();

    dal.create(ctx.clone(), &project).await.unwrap();
    let found = dal.find_by_id(ctx, &project_id).await.unwrap().unwrap();

    assert_eq!(found.id, project_id);
    assert_eq!(found.name, "Test Project");
    assert_eq!(found.root_user_id, root_user_id);
    assert_eq!(found.priority, 1);
    assert_eq!(found.status, ProjectStatus::Active);
}

#[sqlx::test]
async fn test_list_by_root_user(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let root_user_id = Uuid::now_v7().to_string();
    let other_root_user_id = Uuid::now_v7().to_string();

    // Create 3 projects for user 1
    for i in 0..3 {
        let project = create_test_project(&format!("Project {}", i), &root_user_id);
        dal.create(ctx.clone(), &project).await.unwrap();
    }

    // Create 1 project for user 2
    let project = create_test_project("Other Project", &other_root_user_id);
    dal.create(ctx.clone(), &project).await.unwrap();

    let projects = dal.list_by_root_user(ctx, &root_user_id, None).await.unwrap();
    assert_eq!(projects.len(), 3);
}

#[sqlx::test]
async fn test_list_by_root_user_and_status(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let root_user_id = Uuid::now_v7().to_string();

    // Create 2 active projects
    for i in 0..2 {
        let project = create_test_project(&format!("Active Project {}", i), &root_user_id);
        dal.create(ctx.clone(), &project).await.unwrap();
    }

    // Create 1 archived project
    let archived_project = create_test_project("Archived Project", &root_user_id);
    let archived_project_id = archived_project.id.clone();
    dal.create(ctx.clone(), &archived_project).await.unwrap();
    dal.archive(ctx.clone(), &archived_project_id, "admin").await.unwrap();

    // Query only active projects
    let projects = dal.list_by_root_user_and_status(
        ctx.clone(),
        &root_user_id,
        vec![ProjectStatus::Active],
        None,
    ).await.unwrap();
    assert_eq!(projects.len(), 2);

    // Query archived projects
    let projects = dal.list_by_root_user_and_status(
        ctx,
        &root_user_id,
        vec![ProjectStatus::Archived],
        None,
    ).await.unwrap();
    assert_eq!(projects.len(), 1);
}

#[sqlx::test]
async fn test_query(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let root_user_id = Uuid::now_v7().to_string();

    for i in 0..3 {
        let project = create_test_project(&format!("Project {}", i), &root_user_id);
        dal.create(ctx.clone(), &project).await.unwrap();
    }

    let query = ProjectQuery {
        root_user_id: Some(root_user_id),
        status_in: Some(vec![ProjectStatus::Active]),
        limit: Some(2),
    };

    let projects = dal.query(ctx, query).await.unwrap();
    assert_eq!(projects.len(), 2);
}

#[sqlx::test]
async fn test_update_project(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let root_user_id = Uuid::now_v7().to_string();

    let mut project = create_test_project("Original Name", &root_user_id);
    let project_id = project.id.clone();
    dal.create(ctx.clone(), &project).await.unwrap();

    // Update project
    project.name = "Updated Name".to_string();
    project.description = "Updated description".to_string();
    project.priority = 2;
    dal.update(ctx.clone(), &project).await.unwrap();

    let found = dal.find_by_id(ctx, &project_id).await.unwrap().unwrap();
    assert_eq!(found.name, "Updated Name");
    assert_eq!(found.description, "Updated description");
    assert_eq!(found.priority, 2);
}

#[sqlx::test]
async fn test_update_status_and_archive(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let root_user_id = Uuid::now_v7().to_string();

    let project = create_test_project("Test Project", &root_user_id);
    let project_id = project.id.clone();
    dal.create(ctx.clone(), &project).await.unwrap();

    // Update status to InProgress
    dal.update_status(ctx.clone(), &project_id, ProjectStatus::InProgress, "admin").await.unwrap();
    let found = dal.find_by_id(ctx.clone(), &project_id).await.unwrap().unwrap();
    assert_eq!(found.status, ProjectStatus::InProgress);

    // Archive project
    dal.archive(ctx.clone(), &project_id, "admin").await.unwrap();
    let found = dal.find_by_id(ctx, &project_id).await.unwrap().unwrap();
    assert_eq!(found.status, ProjectStatus::Archived);
}

#[sqlx::test]
async fn test_count_by_root_user(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let root_user_id = Uuid::now_v7().to_string();

    for i in 0..5 {
        let project = create_test_project(&format!("Project {}", i), &root_user_id);
        dal.create(ctx.clone(), &project).await.unwrap();
    }

    let count = dal.count_by_root_user(ctx, &root_user_id).await.unwrap();
    assert_eq!(count, 5);
}

#[sqlx::test]
async fn test_count_by_root_user_and_status(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let root_user_id = Uuid::now_v7().to_string();

    // Create 3 active projects
    for i in 0..3 {
        let project = create_test_project(&format!("Project {}", i), &root_user_id);
        dal.create(ctx.clone(), &project).await.unwrap();
    }

    // Create 2 archived projects
    for i in 0..2 {
        let mut project = create_test_project(&format!("Archived Project {}", i), &root_user_id);
        let project_id = project.id.clone();
        dal.create(ctx.clone(), &project).await.unwrap();
        dal.archive(ctx.clone(), &project_id, "admin").await.unwrap();
    }

    let active_count = dal.count_by_root_user_and_status(
        ctx.clone(),
        &root_user_id,
        ProjectStatus::Active,
    ).await.unwrap();
    assert_eq!(active_count, 3);

    let archived_count = dal.count_by_root_user_and_status(
        ctx,
        &root_user_id,
        ProjectStatus::Archived,
    ).await.unwrap();
    assert_eq!(archived_count, 2);
}
