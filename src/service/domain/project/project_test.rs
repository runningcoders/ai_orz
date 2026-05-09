//! Project Domain 单元测试

use super::{ProjectDomainImpl, ProjectDomainProvider};
use crate::models::project::Project;
use crate::models::task::Task;
use crate::models::artifact::Artifact;
use crate::models::file::FileMeta;
use crate::pkg::RequestContext;
use common::enums::project::ProjectStatus;
use common::enums::task::TaskStatus;
use common::enums::FileType;
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

fn new_ctx(user_id: &str, pool: sqlx::SqlitePool) -> RequestContext {
    RequestContext::new_simple(user_id, pool)
}

fn init_test_env(pool: SqlitePool) -> (Arc<dyn ProjectDomainProvider>, RequestContext) {
    crate::service::dao::project::init();
    crate::service::dao::task::init();
    crate::service::dao::artifact::init();

    crate::service::dal::project::init();
    crate::service::dal::task::init();
    crate::service::dal::artifact::init();

    super::init();

    let domain = super::domain();
    let ctx = new_ctx("admin", pool);
    (domain, ctx)
}

// ==================== ProjectDomain 测试 ====================

#[sqlx::test]
async fn test_project_create_and_get(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let root_user_id = Uuid::now_v7().to_string();

    let project = domain
        .project()
        .create(
            ctx.clone(),
            "Test Project".to_string(),
            "Test Description".to_string(),
            1,
            vec!["test".to_string()],
            root_user_id.clone(),
            "admin".to_string(),
        )
        .await
        .unwrap();

    assert_eq!(project.po.name, "Test Project");
    assert_eq!(project.po.description, "Test Description");
    assert_eq!(project.po.priority, 1);
    assert_eq!(project.po.root_user_id, root_user_id);
    assert_eq!(project.po.status, ProjectStatus::Active);

    let found = domain
        .project()
        .get(ctx.clone(), &project.po.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(found.po.id, project.po.id);
    assert_eq!(found.po.name, "Test Project");
}

#[sqlx::test]
async fn test_project_list_by_user(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let root_user_id = Uuid::now_v7().to_string();

    for i in 0..3 {
        domain
            .project()
            .create(
                ctx.clone(),
                format!("Project {}", i),
                format!("Description {}", i),
                1,
                vec!["test".to_string()],
                root_user_id.clone(),
                "admin".to_string(),
            )
            .await
            .unwrap();
    }

    let projects = domain
        .project()
        .list_by_user(ctx.clone(), &root_user_id)
        .await
        .unwrap();

    assert_eq!(projects.len(), 3);
}

#[sqlx::test]
async fn test_project_start_complete_archive(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let root_user_id = Uuid::now_v7().to_string();

    let project = domain
        .project()
        .create(
            ctx.clone(),
            "Test Project".to_string(),
            "Test Description".to_string(),
            1,
            vec!["test".to_string()],
            root_user_id,
            "admin".to_string(),
        )
        .await
        .unwrap();

    let project_id = &project.po.id;

    domain
        .project()
        .start(ctx.clone(), project_id, "admin".to_string())
        .await
        .unwrap();
    let started = domain.project().get(ctx.clone(), project_id).await.unwrap().unwrap();
    assert_eq!(started.po.status, ProjectStatus::InProgress);

    domain
        .project()
        .complete(ctx.clone(), project_id, "admin".to_string())
        .await
        .unwrap();
    let completed = domain.project().get(ctx.clone(), project_id).await.unwrap().unwrap();
    assert_eq!(completed.po.status, ProjectStatus::Completed);

    domain
        .project()
        .archive(ctx.clone(), project_id, "admin".to_string())
        .await
        .unwrap();
    let archived = domain.project().get(ctx.clone(), project_id).await.unwrap().unwrap();
    assert_eq!(archived.po.status, ProjectStatus::Archived);
}

// ==================== TaskDomain 测试 ====================

#[sqlx::test]
async fn test_task_create_and_get(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let project_id = Uuid::now_v7().to_string();
    let root_user_id = Uuid::now_v7().to_string();
    let assignee_id = Uuid::now_v7().to_string();

    let task = domain
        .task()
        .create(
            ctx.clone(),
            "Test Task".to_string(),
            "Test Description".to_string(),
            1,
            vec!["test".to_string()],
            root_user_id,
            common::enums::task::AssigneeType::Agent,
            assignee_id.clone(),
            Some(project_id.clone()),
            "admin".to_string(),
        )
        .await
        .unwrap();

    assert_eq!(task.po.title, "Test Task");
    assert_eq!(task.po.project_id, Some(project_id));
    assert_eq!(task.po.assignee_id, assignee_id);
    assert_eq!(task.po.status, TaskStatus::Pending);

    let found = domain
        .task()
        .get(ctx.clone(), &task.po.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(found.po.id, task.po.id);
    assert_eq!(found.po.title, "Test Task");
}

#[sqlx::test]
async fn test_task_list_by_project_and_agent(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let project_id = Uuid::now_v7().to_string();
    let root_user_id = Uuid::now_v7().to_string();
    let agent_id = Uuid::now_v7().to_string();

    for i in 0..3 {
        domain
            .task()
            .create(
                ctx.clone(),
                format!("Task {}", i),
                format!("Description {}", i),
                1,
                vec!["test".to_string()],
                root_user_id.clone(),
                common::enums::task::AssigneeType::Agent,
                agent_id.clone(),
                Some(project_id.clone()),
                "admin".to_string(),
            )
            .await
            .unwrap();
    }

    let tasks_by_project = domain
        .task()
        .list_by_project(ctx.clone(), &project_id)
        .await
        .unwrap();
    assert_eq!(tasks_by_project.len(), 3);

    let tasks_by_agent = domain
        .task()
        .list_by_agent(ctx.clone(), &agent_id)
        .await
        .unwrap();
    assert_eq!(tasks_by_agent.len(), 3);
}

#[sqlx::test]
async fn test_task_start_complete_cancel(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let project_id = Uuid::now_v7().to_string();
    let root_user_id = Uuid::now_v7().to_string();
    let assignee_id = Uuid::now_v7().to_string();

    let task = domain
        .task()
        .create(
            ctx.clone(),
            "Test Task".to_string(),
            "Test Description".to_string(),
            1,
            vec!["test".to_string()],
            root_user_id.clone(),
            common::enums::task::AssigneeType::Agent,
            assignee_id.clone(),
            Some(project_id),
            "admin".to_string(),
        )
        .await
        .unwrap();

    let task_id = &task.po.id;

    domain
        .task()
        .start(ctx.clone(), task_id, "admin".to_string())
        .await
        .unwrap();
    let started = domain.task().get(ctx.clone(), task_id).await.unwrap().unwrap();
    assert_eq!(started.po.status, TaskStatus::InProgress);

    domain
        .task()
        .complete(ctx.clone(), task_id, "admin".to_string())
        .await
        .unwrap();
    let completed = domain.task().get(ctx.clone(), task_id).await.unwrap().unwrap();
    assert_eq!(completed.po.status, TaskStatus::Completed);

    let task2 = domain
        .task()
        .create(
            ctx.clone(),
            "Task to cancel".to_string(),
            "Description".to_string(),
            1,
            vec!["test".to_string()],
            root_user_id.clone(),
            common::enums::task::AssigneeType::Agent,
            assignee_id.clone(),
            Some(Uuid::now_v7().to_string()),
            "admin".to_string(),
        )
        .await
        .unwrap();

    domain
        .task()
        .cancel(ctx.clone(), &task2.po.id, "admin".to_string())
        .await
        .unwrap();
    
    // Cancelled 状态被当作软删除，find_by_id 查不到（设计如此）
    let canceled = domain.task().get(ctx.clone(), &task2.po.id).await.unwrap();
    assert!(canceled.is_none(), "Cancelled task should not be found (soft delete)");
}

// ==================== ArtifactDomain 测试 ====================

#[sqlx::test]
async fn test_artifact_create_project_artifact_and_get(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let project_id = Uuid::now_v7().to_string();

    let file_meta = FileMeta {
        file_path: "/path/to/report.pdf".to_string(),
        mime_type: "application/pdf".to_string(),
        file_size: 1024,
    };

    let artifact = domain
        .artifact()
        .create_project_artifact(
            ctx.clone(),
            project_id.clone(),
            "Project Report".to_string(),
            "Report description".to_string(),
            FileType::Document,
            file_meta,
            "admin".to_string(),
        )
        .await
        .unwrap();

    assert_eq!(artifact.po.name, "Project Report");
    assert_eq!(artifact.po.project_id, project_id);
    assert!(artifact.po.task_id.is_none());

    let found = domain
        .artifact()
        .get(ctx.clone(), &artifact.po.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(found.po.id, artifact.po.id);
    assert_eq!(found.po.name, "Project Report");
}

#[sqlx::test]
async fn test_artifact_create_task_artifact_and_list(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let project_id = Uuid::now_v7().to_string();
    let task_id = Uuid::now_v7().to_string();

    let file_meta = FileMeta {
        file_path: "/path/to/output.pdf".to_string(),
        mime_type: "application/pdf".to_string(),
        file_size: 2048,
    };

    let artifact = domain
        .artifact()
        .create_task_artifact(
            ctx.clone(),
            project_id.clone(),
            task_id.clone(),
            "Task Output".to_string(),
            "Output description".to_string(),
            FileType::Document,
            file_meta,
            "admin".to_string(),
        )
        .await
        .unwrap();

    assert_eq!(artifact.po.name, "Task Output");
    assert_eq!(artifact.po.project_id, project_id);
    assert_eq!(artifact.po.task_id, Some(task_id.clone()));

    let artifacts_by_project = domain
        .artifact()
        .list_by_project(ctx.clone(), &project_id)
        .await
        .unwrap();
    assert_eq!(artifacts_by_project.len(), 1);

    let artifacts_by_task = domain
        .artifact()
        .list_by_task(ctx.clone(), &task_id)
        .await
        .unwrap();
    assert_eq!(artifacts_by_task.len(), 1);
}

#[sqlx::test]
async fn test_artifact_delete(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let project_id = Uuid::now_v7().to_string();

    let file_meta = FileMeta {
        file_path: "/path/to/delete.pdf".to_string(),
        mime_type: "application/pdf".to_string(),
        file_size: 512,
    };

    let artifact = domain
        .artifact()
        .create_project_artifact(
            ctx.clone(),
            project_id,
            "To Delete".to_string(),
            "Delete description".to_string(),
            FileType::Document,
            file_meta,
            "admin".to_string(),
        )
        .await
        .unwrap();

    let artifact_id = &artifact.po.id;
    assert!(domain.artifact().get(ctx.clone(), artifact_id).await.unwrap().is_some());

    domain.artifact().delete(ctx.clone(), artifact_id).await.unwrap();

    let found = domain.artifact().get(ctx.clone(), artifact_id).await.unwrap();
    assert!(found.is_none());
}
