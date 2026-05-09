//! Project Domain 测试

use super::*;
use crate::pkg::RequestContext;
use common::enums::ProjectStatus;
use sqlx::SqlitePool;

/// 初始化测试环境
async fn init_test_env(pool: SqlitePool) -> (Arc<dyn ProjectDomain>, RequestContext) {
    crate::service::dao::project::init();
    crate::service::dal::project::init();
    super::init();
    let domain = domain();
    let ctx = RequestContext::new_simple("admin", pool);
    (domain, ctx)
}

#[sqlx::test]
async fn test_create_project(pool: SqlitePool) {
    let (project_domain, ctx) = init_test_env(pool).await;

    let cmd = CreateProjectCommand {
        name: "Test Project",
        description: "Test Description",
        workflow: Some("Test Workflow"),
        guidance: Some("Test Guidance"),
        priority: 1,
        tags: vec!["test".to_string(), "project".to_string()],
        root_user_id: "test_user",
        owner_agent_id: Some("agent_123"),
        start_at: Some(1000000),
        due_at: Some(2000000),
    };

    let result = project_domain.management().create_project(ctx, cmd).await;
    assert!(result.is_ok());

    let project = result.unwrap();
    assert_eq!(project.name, "Test Project");
    assert_eq!(project.description, "Test Description");
    assert_eq!(project.status, ProjectStatus::Active);
    assert_eq!(project.priority, 1);
    assert_eq!(project.root_user_id, "test_user");
    assert_eq!(project.owner_agent_id, Some("agent_123".to_string()));
}

#[sqlx::test]
async fn test_get_project_by_id(pool: SqlitePool) {
    let (project_domain, ctx) = init_test_env(pool).await;

    // First create a project
    let cmd = CreateProjectCommand {
        name: "Test Project",
        description: "Test Description",
        workflow: None,
        guidance: None,
        priority: 1,
        tags: vec!["test".to_string()],
        root_user_id: "test_user",
        owner_agent_id: None,
        start_at: None,
        due_at: None,
    };

    let project = project_domain.management().create_project(ctx.clone(), cmd).await.unwrap();

    // Then get it
    let result = project_domain.management().get_project_by_id(ctx, &project.id).await;
    assert!(result.is_ok());
    let found = result.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "Test Project");
}

#[sqlx::test]
async fn test_update_project(pool: SqlitePool) {
    let (project_domain, ctx) = init_test_env(pool).await;

    // First create a project
    let cmd = CreateProjectCommand {
        name: "Test Project",
        description: "Test Description",
        workflow: None,
        guidance: None,
        priority: 1,
        tags: vec!["test".to_string()],
        root_user_id: "test_user",
        owner_agent_id: None,
        start_at: None,
        due_at: None,
    };

    let project = project_domain.management().create_project(ctx.clone(), cmd).await.unwrap();

    // Then update
    let update_cmd = UpdateProjectCommand {
        project_id: &project.id,
        name: Some("Updated Project"),
        description: Some("Updated Description"),
        workflow: None,
        guidance: None,
        priority: Some(2),
        tags: Some(vec!["updated".to_string()]),
        owner_agent_id: Some("agent_456"),
        start_at: Some(1234567),
        due_at: Some(7654321),
    };

    let result = project_domain.management().update_project(ctx, update_cmd).await;
    assert!(result.is_ok());

    let updated = result.unwrap();
    assert_eq!(updated.name, "Updated Project");
    assert_eq!(updated.description, "Updated Description");
    assert_eq!(updated.priority, 2);
    assert_eq!(updated.owner_agent_id, Some("agent_456".to_string()));
    assert_eq!(updated.start_at, Some(1234567));
}

#[sqlx::test]
async fn test_start_project(pool: SqlitePool) {
    let (project_domain, ctx) = init_test_env(pool).await;

    // First create a project
    let cmd = CreateProjectCommand {
        name: "Test Project",
        description: "Test Description",
        workflow: None,
        guidance: None,
        priority: 1,
        tags: vec!["test".to_string()],
        root_user_id: "test_user",
        owner_agent_id: None,
        start_at: None,
        due_at: None,
    };

    let project = project_domain.management().create_project(ctx.clone(), cmd).await.unwrap();
    assert_eq!(project.status, ProjectStatus::Active);

    // Then start the project
    let result = project_domain.execution().start_project(ctx.clone(), &project.id).await;
    assert!(result.is_ok());

    // Verify status changed
    let found = project_domain.management().get_project_by_id(ctx, &project.id).await.unwrap().unwrap();
    assert_eq!(found.status, ProjectStatus::InProgress);
    assert!(found.start_at.is_some());
}

#[sqlx::test]
async fn test_complete_project(pool: SqlitePool) {
    let (project_domain, ctx) = init_test_env(pool).await;

    // First create a project
    let cmd = CreateProjectCommand {
        name: "Test Project",
        description: "Test Description",
        workflow: None,
        guidance: None,
        priority: 1,
        tags: vec!["test".to_string()],
        root_user_id: "test_user",
        owner_agent_id: None,
        start_at: None,
        due_at: None,
    };

    let project = project_domain.management().create_project(ctx.clone(), cmd).await.unwrap();

    // Start first
    project_domain.execution().start_project(ctx.clone(), &project.id).await.unwrap();

    // Then complete
    let result = project_domain.execution().complete_project(ctx.clone(), &project.id).await;
    assert!(result.is_ok());

    // Verify status changed
    let found = project_domain.management().get_project_by_id(ctx, &project.id).await.unwrap().unwrap();
    assert_eq!(found.status, ProjectStatus::Completed);
    assert!(found.end_at.is_some());
}

#[sqlx::test]
async fn test_reactivate_project(pool: SqlitePool) {
    let (project_domain, ctx) = init_test_env(pool).await;

    // First create a project and complete it
    let cmd = CreateProjectCommand {
        name: "Test Project",
        description: "Test Description",
        workflow: None,
        guidance: None,
        priority: 1,
        tags: vec!["test".to_string()],
        root_user_id: "test_user",
        owner_agent_id: None,
        start_at: None,
        due_at: None,
    };

    let project = project_domain.management().create_project(ctx.clone(), cmd).await.unwrap();
    project_domain.execution().start_project(ctx.clone(), &project.id).await.unwrap();
    project_domain.execution().complete_project(ctx.clone(), &project.id).await.unwrap();

    // Then reactivate
    let result = project_domain.execution().reactivate_project(ctx.clone(), &project.id).await;
    assert!(result.is_ok());

    // Verify status changed
    let found = project_domain.management().get_project_by_id(ctx, &project.id).await.unwrap().unwrap();
    assert_eq!(found.status, ProjectStatus::Active);
    assert!(found.end_at.is_none());
}

#[sqlx::test]
async fn test_archive_project(pool: SqlitePool) {
    let (project_domain, ctx) = init_test_env(pool).await;

    // First create a project
    let cmd = CreateProjectCommand {
        name: "Test Project",
        description: "Test Description",
        workflow: None,
        guidance: None,
        priority: 1,
        tags: vec!["test".to_string()],
        root_user_id: "test_user",
        owner_agent_id: None,
        start_at: None,
        due_at: None,
    };

    let project = project_domain.management().create_project(ctx.clone(), cmd).await.unwrap();

    // Then archive
    let result = project_domain.management().archive_project(ctx.clone(), &project.id).await;
    assert!(result.is_ok());

    // Verify status changed
    let found = project_domain.management().get_project_by_id(ctx, &project.id).await.unwrap().unwrap();
    assert_eq!(found.status, ProjectStatus::Archived);
}

#[sqlx::test]
async fn test_count_user_projects(pool: SqlitePool) {
    let (project_domain, ctx) = init_test_env(pool).await;

    // Create 2 projects for the same user
    for i in 0..2 {
        let cmd = CreateProjectCommand {
            name: &format!("Project {}", i),
            description: "Test Description",
            workflow: None,
            guidance: None,
            priority: 1,
            tags: vec!["test".to_string()],
            root_user_id: "test_user",
            owner_agent_id: None,
            start_at: None,
            due_at: None,
        };

        project_domain.management().create_project(ctx.clone(), cmd).await.unwrap();
    }

    // Count projects
    let count = project_domain.management().count_user_projects(ctx, "test_user").await.unwrap();
    assert_eq!(count, 2);
}

#[sqlx::test]
async fn test_list_user_projects(pool: SqlitePool) {
    let (project_domain, ctx) = init_test_env(pool).await;

    // Create 3 projects
    for i in 0..3 {
        let cmd = CreateProjectCommand {
            name: &format!("Project {}", i),
            description: "Test Description",
            workflow: None,
            guidance: None,
            priority: 1,
            tags: vec!["test".to_string()],
            root_user_id: "test_user",
            owner_agent_id: None,
            start_at: None,
            due_at: None,
        };

        project_domain.management().create_project(ctx.clone(), cmd).await.unwrap();
    }

    // List all projects
    let projects = project_domain.management().list_user_projects(ctx, "test_user", None).await.unwrap();
    assert_eq!(projects.len(), 3);
}