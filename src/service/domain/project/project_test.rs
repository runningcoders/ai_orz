//! Project Domain 单元测试

use super::ProjectDomain;
use crate::models::file::FileMeta;
use crate::pkg::RequestContext;
use common::enums::project::ProjectStatus;
use common::enums::task::TaskStatus;
use common::enums::{ArtifactSourceType, FileType};
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

fn new_ctx(user_id: &str, pool: sqlx::SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx(user_id, pool)
}

fn init_test_env(pool: SqlitePool) -> (Arc<dyn ProjectDomain>, RequestContext) {
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
        .project_manage()
        .create(
            ctx.clone(),
            "Test Project".to_string(),
            "Test Description".to_string(),
            1,
            vec!["test".to_string()],
            None,
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
        .project_manage()
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
            .project_manage()
            .create(
                ctx.clone(),
                format!("Project {}", i),
                format!("Description {}", i),
                1,
                vec!["test".to_string()],
                None,
                root_user_id.clone(),
                "admin".to_string(),
            )
            .await
            .unwrap();
    }

    let projects = domain
        .project_manage()
        .list_by_user(ctx.clone(), &root_user_id)
        .await
        .unwrap();

    assert_eq!(projects.len(), 3);
}

#[sqlx::test]
async fn test_project_transition_status_updates_entity(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let root_user_id = Uuid::now_v7().to_string();

    let mut project = domain
        .project_manage()
        .create(
            ctx.clone(),
            "Status Project".to_string(),
            "Status Description".to_string(),
            1,
            vec!["status".to_string()],
            None,
            root_user_id,
            "admin".to_string(),
        )
        .await
        .unwrap();

    domain
        .project_manage()
        .transition_status(ctx.clone(), &mut project, ProjectStatus::InProgress)
        .await
        .unwrap();
    assert_eq!(project.po.status, ProjectStatus::InProgress);
    assert!(project.po.start_at.is_some());

    domain
        .project_manage()
        .transition_status(ctx.clone(), &mut project, ProjectStatus::Completed)
        .await
        .unwrap();
    assert_eq!(project.po.status, ProjectStatus::Completed);
    assert!(project.po.end_at.is_some());

    let found = domain
        .project_manage()
        .get(ctx.clone(), &project.po.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.po.status, ProjectStatus::Completed);
}

#[sqlx::test]
async fn test_project_transition_status_rejects_deleted(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let root_user_id = Uuid::now_v7().to_string();

    let mut project = domain
        .project_manage()
        .create(
            ctx.clone(),
            "Delete Status Project".to_string(),
            "Status Description".to_string(),
            1,
            vec!["status".to_string()],
            None,
            root_user_id,
            "admin".to_string(),
        )
        .await
        .unwrap();

    let result = domain
        .project_manage()
        .transition_status(ctx.clone(), &mut project, ProjectStatus::Deleted)
        .await;

    assert!(result.is_err());
}

#[sqlx::test]
async fn test_task_transition_status_updates_entity(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let project_id = Uuid::now_v7().to_string();
    let root_user_id = Uuid::now_v7().to_string();
    let assignee_id = Uuid::now_v7().to_string();

    let mut task = domain
        .task_manage()
        .create(
            ctx.clone(),
            "Status Task".to_string(),
            "Status Description".to_string(),
            1,
            vec!["status".to_string()],
            root_user_id,
            common::enums::task::AssigneeType::Agent,
            assignee_id,
            Some(project_id),
            "admin".to_string(),
        )
        .await
        .unwrap();

    domain
        .task_manage()
        .transition_status(ctx.clone(), &mut task, TaskStatus::InProgress)
        .await
        .unwrap();
    assert_eq!(task.po.status, TaskStatus::InProgress);
    assert!(task.po.start_at.is_some());

    domain
        .task_manage()
        .transition_status(ctx.clone(), &mut task, TaskStatus::Completed)
        .await
        .unwrap();
    assert_eq!(task.po.status, TaskStatus::Completed);
    assert!(task.po.end_at.is_some());

    let found = domain
        .task_manage()
        .get(ctx.clone(), &task.po.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.po.status, TaskStatus::Completed);
}

#[sqlx::test]
async fn test_task_transition_status_rejects_cancelled(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let project_id = Uuid::now_v7().to_string();
    let root_user_id = Uuid::now_v7().to_string();
    let assignee_id = Uuid::now_v7().to_string();

    let mut task = domain
        .task_manage()
        .create(
            ctx.clone(),
            "Cancel Status Task".to_string(),
            "Status Description".to_string(),
            1,
            vec!["status".to_string()],
            root_user_id,
            common::enums::task::AssigneeType::Agent,
            assignee_id,
            Some(project_id),
            "admin".to_string(),
        )
        .await
        .unwrap();

    let result = domain
        .task_manage()
        .transition_status(ctx.clone(), &mut task, TaskStatus::Cancelled)
        .await;

    assert!(result.is_err());
}

#[sqlx::test]
async fn test_project_start_complete_archive(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let root_user_id = Uuid::now_v7().to_string();

    let project = domain
        .project_manage()
        .create(
            ctx.clone(),
            "Test Project".to_string(),
            "Test Description".to_string(),
            1,
            vec!["test".to_string()],
            None,
            root_user_id,
            "admin".to_string(),
        )
        .await
        .unwrap();

    let project_id = &project.po.id;

    domain
        .project_manage()
        .start(ctx.clone(), project_id, "admin".to_string())
        .await
        .unwrap();
    let started = domain
        .project_manage()
        .get(ctx.clone(), project_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(started.po.status, ProjectStatus::InProgress);

    domain
        .project_manage()
        .complete(ctx.clone(), project_id, "admin".to_string())
        .await
        .unwrap();
    let completed = domain
        .project_manage()
        .get(ctx.clone(), project_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(completed.po.status, ProjectStatus::Completed);

    domain
        .project_manage()
        .archive(ctx.clone(), project_id, "admin".to_string())
        .await
        .unwrap();
    let archived = domain
        .project_manage()
        .get(ctx.clone(), project_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(archived.po.status, ProjectStatus::Archived);
}

// ==================== TaskManage 测试 ====================

#[sqlx::test]
async fn test_task_create_and_get(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let project_id = Uuid::now_v7().to_string();
    let root_user_id = Uuid::now_v7().to_string();
    let assignee_id = Uuid::now_v7().to_string();

    let task = domain
        .task_manage()
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
        .task_manage()
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
            .task_manage()
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
        .task_manage()
        .list_by_project(ctx.clone(), &project_id)
        .await
        .unwrap();
    assert_eq!(tasks_by_project.len(), 3);

    let tasks_by_agent = domain
        .task_manage()
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
        .task_manage()
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
        .task_manage()
        .start(ctx.clone(), task_id, "admin".to_string())
        .await
        .unwrap();
    let started = domain
        .task_manage()
        .get(ctx.clone(), task_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(started.po.status, TaskStatus::InProgress);

    domain
        .task_manage()
        .complete(ctx.clone(), task_id, "admin".to_string())
        .await
        .unwrap();
    let completed = domain
        .task_manage()
        .get(ctx.clone(), task_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(completed.po.status, TaskStatus::Completed);

    let task2 = domain
        .task_manage()
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
        .task_manage()
        .cancel(ctx.clone(), &task2.po.id, "admin".to_string())
        .await
        .unwrap();

    // Cancelled 状态被当作软删除，find_by_id 查不到（设计如此）
    let canceled = domain
        .task_manage()
        .get(ctx.clone(), &task2.po.id)
        .await
        .unwrap();
    assert!(
        canceled.is_none(),
        "Cancelled task should not be found (soft delete)"
    );
}

// ==================== ArtifactManage 测试 ====================

#[sqlx::test]
async fn test_artifact_create_project_artifact_and_get(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let project = domain
        .project_manage()
        .create(
            ctx.clone(),
            "Artifact Project".to_string(),
            "Project for artifact".to_string(),
            1,
            vec!["artifact".to_string()],
            None,
            "admin".to_string(),
            "admin".to_string(),
        )
        .await
        .unwrap();
    let project_id = project.po.id.clone();

    let file_meta = FileMeta {
        file_path: "/path/to/report.pdf".to_string(),
        mime_type: "application/pdf".to_string(),
        file_size: 1024,
    };

    let artifact = domain
        .artifact_manage()
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
        .artifact_manage()
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
    let assignee_id = Uuid::now_v7().to_string();
    let project = domain
        .project_manage()
        .create(
            ctx.clone(),
            "Artifact Task Project".to_string(),
            "Project for task artifact".to_string(),
            1,
            vec!["artifact".to_string()],
            None,
            "admin".to_string(),
            "admin".to_string(),
        )
        .await
        .unwrap();
    let project_id = project.po.id.clone();

    let task = domain
        .task_manage()
        .create(
            ctx.clone(),
            "Artifact Task".to_string(),
            "Task for artifact".to_string(),
            1,
            vec![],
            "admin".to_string(),
            common::enums::task::AssigneeType::Agent,
            assignee_id,
            Some(project_id.clone()),
            "admin".to_string(),
        )
        .await
        .unwrap();
    let task_id = task.po.id.clone();

    let file_meta = FileMeta {
        file_path: "/path/to/output.pdf".to_string(),
        mime_type: "application/pdf".to_string(),
        file_size: 2048,
    };

    let artifact = domain
        .artifact_manage()
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
        .artifact_manage()
        .list_by_project(ctx.clone(), &project_id)
        .await
        .unwrap();
    assert_eq!(artifacts_by_project.len(), 1);

    let artifacts_by_task = domain
        .artifact_manage()
        .list_by_task(ctx.clone(), &task_id)
        .await
        .unwrap();
    assert_eq!(artifacts_by_task.len(), 1);
}

#[sqlx::test]
async fn test_artifact_create_attachment_artifact_validates_project_and_task(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let root_user_id = "admin".to_string();
    let assignee_id = Uuid::now_v7().to_string();

    let project = domain
        .project_manage()
        .create(
            ctx.clone(),
            "Artifact Project".to_string(),
            "Project for artifact".to_string(),
            1,
            vec!["artifact".to_string()],
            None,
            root_user_id.clone(),
            "admin".to_string(),
        )
        .await
        .unwrap();

    let task = domain
        .task_manage()
        .create(
            ctx.clone(),
            "Artifact Task".to_string(),
            "Task for artifact".to_string(),
            1,
            vec![],
            root_user_id,
            common::enums::task::AssigneeType::Agent,
            assignee_id,
            Some(project.po.id.clone()),
            "admin".to_string(),
        )
        .await
        .unwrap();

    let artifact = domain
        .artifact_manage()
        .create_attachment_artifact(
            ctx.clone(),
            project.po.id.clone(),
            Some(task.po.id.clone()),
            "Referenced Attachment".to_string(),
            "Attachment-backed artifact".to_string(),
            FileType::Document,
            FileMeta::new(
                "attachments/20260617/report.md".to_string(),
                "text/markdown".to_string(),
                128,
            ),
            vec!["report".to_string()],
            "admin".to_string(),
        )
        .await
        .unwrap();

    assert_eq!(artifact.po.project_id, project.po.id);
    assert_eq!(artifact.po.task_id, Some(task.po.id));
    assert_eq!(
        artifact.po.source_type,
        common::enums::ArtifactSourceType::Attachment
    );
    assert_eq!(
        artifact.po.file_meta.0.file_path,
        "attachments/20260617/report.md"
    );
    assert_eq!(artifact.tags(), vec!["report".to_string()]);
}

#[sqlx::test]
async fn test_artifact_list_filters_by_project_file_type_source_type_and_limit(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let root_user_id = "admin".to_string();

    let project = domain
        .project_manage()
        .create(
            ctx.clone(),
            "Artifact List Project".to_string(),
            "Project for artifact list".to_string(),
            1,
            vec!["artifact".to_string()],
            None,
            root_user_id,
            "admin".to_string(),
        )
        .await
        .unwrap();

    domain
        .artifact_manage()
        .create_attachment_artifact(
            ctx.clone(),
            project.po.id.clone(),
            None,
            "Document Artifact".to_string(),
            "Document attachment".to_string(),
            FileType::Document,
            FileMeta::new(
                "attachments/20260617/report.md".to_string(),
                "text/markdown".to_string(),
                128,
            ),
            vec!["report".to_string()],
            "admin".to_string(),
        )
        .await
        .unwrap();

    domain
        .artifact_manage()
        .create_attachment_artifact(
            ctx.clone(),
            project.po.id.clone(),
            None,
            "Image Artifact".to_string(),
            "Image attachment".to_string(),
            FileType::Image,
            FileMeta::new(
                "attachments/20260617/screenshot.png".to_string(),
                "image/png".to_string(),
                256,
            ),
            vec!["screenshot".to_string()],
            "admin".to_string(),
        )
        .await
        .unwrap();

    let artifacts = domain
        .artifact_manage()
        .list(
            ctx.clone(),
            super::artifact::ListArtifactsParams {
                project_id: project.po.id.clone(),
                task_id: None,
                file_type: Some(FileType::Document),
                source_type: Some(ArtifactSourceType::Attachment),
                pagination: common::api::PaginationParams {
                    limit: Some(10),
                    offset: None,
                },
            },
        )
        .await
        .unwrap();

    assert_eq!(artifacts.items.len(), 1);
    assert_eq!(artifacts.items[0].po.name, "Document Artifact");
    assert_eq!(artifacts.items[0].po.project_id, project.po.id);
    assert_eq!(artifacts.items[0].po.file_type, FileType::Document);
    assert_eq!(artifacts.items[0].po.source_type, ArtifactSourceType::Attachment);
}

#[sqlx::test]
async fn test_artifact_create_attachment_artifact_rejects_task_project_mismatch(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let root_user_id = "admin".to_string();
    let assignee_id = Uuid::now_v7().to_string();

    let target_project = domain
        .project_manage()
        .create(
            ctx.clone(),
            "Target Project".to_string(),
            "Target".to_string(),
            1,
            vec![],
            None,
            root_user_id.clone(),
            "admin".to_string(),
        )
        .await
        .unwrap();

    let other_project = domain
        .project_manage()
        .create(
            ctx.clone(),
            "Other Project".to_string(),
            "Other".to_string(),
            1,
            vec![],
            None,
            root_user_id.clone(),
            "admin".to_string(),
        )
        .await
        .unwrap();

    let task = domain
        .task_manage()
        .create(
            ctx.clone(),
            "Other Task".to_string(),
            "Task belongs to other project".to_string(),
            1,
            vec![],
            root_user_id,
            common::enums::task::AssigneeType::Agent,
            assignee_id,
            Some(other_project.po.id),
            "admin".to_string(),
        )
        .await
        .unwrap();

    let result = domain
        .artifact_manage()
        .create_attachment_artifact(
            ctx,
            target_project.po.id,
            Some(task.po.id),
            "Invalid Artifact".to_string(),
            "Should be rejected".to_string(),
            FileType::Document,
            FileMeta::new(
                "attachments/20260617/report.md".to_string(),
                "text/markdown".to_string(),
                128,
            ),
            vec![],
            "admin".to_string(),
        )
        .await;

    assert!(result.is_err());
}

#[sqlx::test]
async fn test_artifact_delete(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);
    let project = domain
        .project_manage()
        .create(
            ctx.clone(),
            "Artifact Delete Project".to_string(),
            "Project for artifact delete".to_string(),
            1,
            vec!["artifact".to_string()],
            None,
            "admin".to_string(),
            "admin".to_string(),
        )
        .await
        .unwrap();
    let project_id = project.po.id.clone();

    let file_meta = FileMeta {
        file_path: "/path/to/delete.pdf".to_string(),
        mime_type: "application/pdf".to_string(),
        file_size: 512,
    };

    let artifact = domain
        .artifact_manage()
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
    assert!(
        domain
            .artifact_manage()
            .get(ctx.clone(), artifact_id)
            .await
            .unwrap()
            .is_some()
    );

    domain
        .artifact_manage()
        .delete(ctx.clone(), artifact_id)
        .await
        .unwrap();

    let found = domain
        .artifact_manage()
        .get(ctx.clone(), artifact_id)
        .await
        .unwrap();
    assert!(found.is_none());
}
