//! Tests for SqliteProjectDao

use sqlx::SqlitePool;
use common::enums::project::ProjectStatus;
use crate::error::AppError;
use crate::models::project::ProjectPo;
use crate::pkg::RequestContext;
use crate::service::dao::project::{ProjectDaoTrait, sqlite};

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    RequestContext::new_simple(user_id, pool)
}

#[sqlx::test]
async fn test_insert_project(pool: SqlitePool) -> Result<(), AppError> {
    crate::service::dao::project::init();
    let dao = sqlite::dao();
    let ctx = new_ctx("test-user", pool);
    
    let project_id = uuid::Uuid::now_v7().to_string();
    let project = ProjectPo::new(
        project_id.clone(),
        "Test Project".to_string(),
        "This is a test project".to_string(),
        None, // workflow
        None, // guidance
        0,
        vec!["test".to_string(), "demo".to_string()],
        "test-user".to_string(),
        None, // owner_agent_id
        None, // start_at
        None, // due_at
        None, // end_at
        "test-user".to_string(),
    );
    
    dao.insert(ctx, &project).await?;
    Ok(())
}

#[sqlx::test]
async fn test_find_by_id(pool: SqlitePool) -> Result<(), AppError> {
    crate::service::dao::project::init();
    let dao = sqlite::dao();
    let ctx = new_ctx("test-user", pool);
    
    let project_id = uuid::Uuid::now_v7().to_string();
    let project = ProjectPo::new(
        project_id.clone(),
        "Find Test".to_string(),
        "Test find by id".to_string(),
        None, // workflow
        None, // guidance
        0,
        vec![],
        "test-user".to_string(),
        None,
        None,
        None,
        None,
        "test-user".to_string(),
    );
    
    dao.insert(ctx.clone(), &project).await?;
    let found = dao.find_by_id(ctx, &project_id).await?;
    
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.name, "Find Test");
    assert_eq!(found.root_user_id, "test-user");
    assert_eq!(found.status, ProjectStatus::default());
    assert!(found.owner_agent_id.is_none());
    Ok(())
}

#[sqlx::test]
async fn test_list_by_root_user(pool: SqlitePool) -> Result<(), AppError> {
    crate::service::dao::project::init();
    let dao = sqlite::dao();
    let ctx = new_ctx("test-user", pool);
    
    // Insert 3 projects for user1, 1 for user2
    for i in 0..3 {
        let project_id = uuid::Uuid::now_v7().to_string();
        let project = ProjectPo::new(
            project_id,
            format!("Project {}", i),
            "".to_string(),
            None, // workflow
            None, // guidance
            i,
            vec![],
            "user1".to_string(),
            None,
            None,
            None,
            None,
            "test-user".to_string(),
        );
        dao.insert(ctx.clone(), &project).await?;
    }
    
    let project_id = uuid::Uuid::now_v7().to_string();
    let project = ProjectPo::new(
        project_id,
        "User2 Project".to_string(),
        "".to_string(),
        None, // workflow
        None, // guidance
        0,
        vec![],
        "user2".to_string(),
        None,
        None,
        None,
        None,
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &project).await?;
    
    let list = dao.list_by_root_user(ctx, "user1", Some(10)).await?;
    assert_eq!(list.len(), 3);
    // Ordered by priority desc
    assert_eq!(list[0].priority, 2);
    Ok(())
}

#[sqlx::test]
async fn test_list_by_root_user_and_status(pool: SqlitePool) -> Result<(), AppError> {
    crate::service::dao::project::init();
    let dao = sqlite::dao();
    let ctx = new_ctx("test-user", pool);
    
    // Create projects with different statuses
    let mut projects = Vec::new();
    for status in [
        ProjectStatus::Active,
        ProjectStatus::Active,
        ProjectStatus::Completed,
        ProjectStatus::Archived,
    ].iter() {
        let project_id = uuid::Uuid::now_v7().to_string();
        let mut project = ProjectPo::new(
            project_id,
            "Status Test".to_string(),
            "".to_string(),
            None, // workflow
            None, // guidance
            0,
            vec![],
            "test-user".to_string(),
            None,
            None,
            None,
            None,
            "test-user".to_string(),
        );
        project.status = *status;
        projects.push(project);
    }
    
    for p in &projects {
        dao.insert(ctx.clone(), p).await?;
    }
    
    // Filter for Active
    let list = dao.list_by_root_user_and_status(
        ctx.clone(),
        "test-user",
        vec![ProjectStatus::Active],
        Some(10)
    ).await?;
    assert_eq!(list.len(), 2);
    
    // Filter for Active and Completed
    let list = dao.list_by_root_user_and_status(
        ctx.clone(),
        "test-user",
        vec![ProjectStatus::Active, ProjectStatus::Completed],
        Some(10)
    ).await?;
    assert_eq!(list.len(), 3);
    Ok(())
}

#[sqlx::test]
async fn test_update_project(pool: SqlitePool) -> Result<(), AppError> {
    crate::service::dao::project::init();
    let dao = sqlite::dao();
    let ctx = new_ctx("test-user", pool);
    
    let project_id = uuid::Uuid::now_v7().to_string();
    let mut project = ProjectPo::new(
        project_id.clone(),
        "Original Name".to_string(),
        "Original Description".to_string(),
        None, // workflow
        None, // guidance
        0,
        vec![],
        "test-user".to_string(),
        Some("agent-123".to_string()),
        None,
        None,
        None,
        "test-user".to_string(),
    );
    
    dao.insert(ctx.clone(), &project).await?;
    
    project.name = "Updated Name".to_string();
    project.description = "Updated Description".to_string();
    project.priority = 10;
    project.owner_agent_id = Some("agent-456".to_string());
    dao.update(ctx.clone(), &project).await?;
    
    let found = dao.find_by_id(ctx, &project_id).await?;
    let found = found.unwrap();
    assert_eq!(found.name, "Updated Name");
    assert_eq!(found.description, "Updated Description");
    assert_eq!(found.priority, 10);
    assert_eq!(found.owner_agent_id, Some("agent-456".to_string()));
    Ok(())
}

#[sqlx::test]
async fn test_update_status(pool: SqlitePool) -> Result<(), AppError> {
    crate::service::dao::project::init();
    let dao = sqlite::dao();
    let ctx = new_ctx("test-user", pool);
    
    let project_id = uuid::Uuid::now_v7().to_string();
    let project = ProjectPo::new(
        project_id.clone(),
        "Status Update Test".to_string(),
        "".to_string(),
        None, // workflow
        None, // guidance
        0,
        vec![],
        "test-user".to_string(),
        None,
        None,
        None,
        None,
        "test-user".to_string(),
    );
    
    dao.insert(ctx.clone(), &project).await?;
    dao.update_status(ctx.clone(), &project_id, ProjectStatus::Completed, "test-user").await?;
    
    let found = dao.find_by_id(ctx, &project_id).await?;
    let found = found.unwrap();
    assert_eq!(found.status, ProjectStatus::Completed);
    Ok(())
}

#[sqlx::test]
async fn test_count_functions(pool: SqlitePool) -> Result<(), AppError> {
    crate::service::dao::project::init();
    let dao = sqlite::dao();
    let ctx = new_ctx("test-user", pool);
    
    for i in 0..5 {
        let project_id = uuid::Uuid::now_v7().to_string();
        let mut project = ProjectPo::new(
            project_id,
            format!("Count {}", i),
            "".to_string(),
            None, // workflow
            None, // guidance
            0,
            vec![],
            "test-user".to_string(),
            None,
            None,
            None,
            None,
            "test-user".to_string(),
        );
        if i % 2 == 0 {
            project.status = ProjectStatus::Active;
        } else {
            project.status = ProjectStatus::Completed;
        }
        dao.insert(ctx.clone(), &project).await?;
    }
    
    let total = dao.count_by_root_user(ctx.clone(), "test-user").await?;
    assert_eq!(total, 5);
    
    let active = dao.count_by_root_user_and_status(ctx, "test-user", ProjectStatus::Active).await?;
    assert_eq!(active, 3);
    Ok(())
}

#[sqlx::test]
async fn test_deleted_not_found(pool: SqlitePool) -> Result<(), AppError> {
    crate::service::dao::project::init();
    let dao = sqlite::dao();
    let ctx = new_ctx("test-user", pool);
    
    let project_id = uuid::Uuid::now_v7().to_string();
    let mut project = ProjectPo::new(
        project_id.clone(),
        "To Delete".to_string(),
        "".to_string(),
        None, // workflow
        None, // guidance
        0,
        vec![],
        "test-user".to_string(),
        None,
        None,
        None,
        None,
        "test-user".to_string(),
    );
    project.status = ProjectStatus::Deleted;
    dao.insert(ctx.clone(), &project).await?;
    
    let found = dao.find_by_id(ctx, &project_id).await?;
    assert!(found.is_none());
    Ok(())
}
