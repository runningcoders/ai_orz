//! Artifact DAL 单元测试

use common::enums::FileType;
use crate::models::artifact::ArtifactPo;
use crate::models::file::FileMeta;
use crate::pkg::RequestContext;
use crate::service::dao::artifact::ArtifactQuery;
use crate::service::dal::artifact::ArtifactDal;
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

/// 初始化测试环境
async fn init_test_env(pool: SqlitePool) -> (Arc<dyn ArtifactDal + Send + Sync>, RequestContext) {
    crate::service::dao::artifact::init();
    crate::service::dal::artifact::init();
    let dal = crate::service::dal::artifact::dal();
    let ctx = RequestContext::new_simple("admin", pool);
    (dal, ctx)
}

/// 创建测试 FileMeta
fn create_test_file_meta(filename: &str) -> FileMeta {
    FileMeta::new(
        format!("/path/to/{}", filename),
        "application/pdf".to_string(),
        1024,
    )
}

/// 创建测试项目级产物
fn create_project_artifact(name: &str, project_id: &str) -> ArtifactPo {
    ArtifactPo::new_project(
        project_id.to_string(),
        name.to_string(),
        format!("Description for {}", name),
        FileType::Document,
        create_test_file_meta(name),
        "admin".to_string(),
    )
}

/// 创建测试任务级产物
fn create_task_artifact(name: &str, project_id: &str, task_id: &str) -> ArtifactPo {
    ArtifactPo::new_task(
        project_id.to_string(),
        task_id.to_string(),
        name.to_string(),
        format!("Description for {}", name),
        FileType::Document,
        create_test_file_meta(name),
        "admin".to_string(),
    )
}

#[sqlx::test]
async fn test_create_and_find_by_id(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let project_id = Uuid::now_v7().to_string();

    let artifact = create_project_artifact("Test Report", &project_id);
    let artifact_id = artifact.id.clone();

    dal.create(ctx.clone(), &artifact).await.unwrap();
    let found = dal.find_by_id(ctx, &artifact_id).await.unwrap().unwrap();

    assert_eq!(found.id, artifact_id);
    assert_eq!(found.project_id, project_id);
    assert_eq!(found.name, "Test Report");
    assert_eq!(found.file_type, FileType::Document);
    assert_eq!(found.status, 1);
}

#[sqlx::test]
async fn test_list_by_project(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let project_id = Uuid::now_v7().to_string();
    let other_project_id = Uuid::now_v7().to_string();

    // Create 3 artifacts for project 1
    for i in 0..3 {
        let artifact = create_project_artifact(&format!("Report {}", i), &project_id);
        dal.create(ctx.clone(), &artifact).await.unwrap();
    }

    // Create 1 artifact for project 2
    let artifact = create_project_artifact("Other Report", &other_project_id);
    dal.create(ctx.clone(), &artifact).await.unwrap();

    let artifacts = dal.list_by_project(ctx, &project_id).await.unwrap();
    assert_eq!(artifacts.len(), 3);
}

#[sqlx::test]
async fn test_list_by_task(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let project_id = Uuid::now_v7().to_string();
    let task_id = Uuid::now_v7().to_string();
    let other_task_id = Uuid::now_v7().to_string();

    // Create 2 artifacts for task 1
    for i in 0..2 {
        let artifact = create_task_artifact(&format!("Report {}", i), &project_id, &task_id);
        dal.create(ctx.clone(), &artifact).await.unwrap();
    }

    // Create 1 artifact for task 2
    let artifact = create_task_artifact("Other Report", &project_id, &other_task_id);
    dal.create(ctx.clone(), &artifact).await.unwrap();

    let artifacts = dal.list_by_task(ctx, &task_id).await.unwrap();
    assert_eq!(artifacts.len(), 2);
}

#[sqlx::test]
async fn test_query_by_project(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let project_id = Uuid::now_v7().to_string();

    for i in 0..3 {
        let artifact = create_project_artifact(&format!("Report {}", i), &project_id);
        dal.create(ctx.clone(), &artifact).await.unwrap();
    }

    let query = ArtifactQuery {
        project_id: Some(project_id),
        task_id: None,
        limit: Some(2),
    };

    let artifacts = dal.query(ctx, query).await.unwrap();
    assert_eq!(artifacts.len(), 2);
}

#[sqlx::test]
async fn test_update_status(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let project_id = Uuid::now_v7().to_string();

    let artifact = create_project_artifact("Test Report", &project_id);
    let artifact_id = artifact.id.clone();
    dal.create(ctx.clone(), &artifact).await.unwrap();

    // Update status to 2 (archived)
    dal.update_status(ctx.clone(), &artifact_id, 2).await.unwrap();
    let found = dal.find_by_id(ctx.clone(), &artifact_id).await.unwrap().unwrap();
    assert_eq!(found.status, 2);

    // Delete (soft delete)
    dal.delete(ctx.clone(), &artifact_id).await.unwrap();
    let found = dal.find_by_id(ctx, &artifact_id).await.unwrap();
    assert!(found.is_none()); // Soft deleted, should not be found
}

#[sqlx::test]
async fn test_count_by_project_and_task(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;
    let project_id = Uuid::now_v7().to_string();
    let task_id = Uuid::now_v7().to_string();

    // Create 3 project-level artifacts
    for i in 0..3 {
        let artifact = create_project_artifact(&format!("Project Report {}", i), &project_id);
        dal.create(ctx.clone(), &artifact).await.unwrap();
    }

    // Create 2 task-level artifacts
    for i in 0..2 {
        let artifact = create_task_artifact(&format!("Task Report {}", i), &project_id, &task_id);
        dal.create(ctx.clone(), &artifact).await.unwrap();
    }

    let project_count = dal.count_by_project(ctx.clone(), &project_id).await.unwrap();
    assert_eq!(project_count, 5); // 3 + 2

    let task_count = dal.count_by_task(ctx, &task_id).await.unwrap();
    assert_eq!(task_count, 2);
}
