use crate::models::artifact::ArtifactPo;
use crate::models::file::FileMeta;
use crate::pkg::request_context::RequestContext;
use crate::service::dao::artifact::{ArtifactDao, new};
use common::enums::FileType;
use common::error::Result;
use sqlx::SqlitePool;
use std::sync::Arc;
use uuid::Uuid;

/// 初始化测试环境
fn init_test_env(pool: SqlitePool) -> (Arc<dyn ArtifactDao + Send + Sync>, RequestContext) {
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);
    let dao = new();
    (dao, ctx)
}

#[sqlx::test(migrations = "./migrations")]
async fn test_insert_artifact(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);

    let file_meta = FileMeta::new(
        "20260415/test.md".to_string(),
        "text/markdown".to_string(),
        1024,
    );
    let artifact = ArtifactPo::new_task(
        "project-id-123".to_string(),
        "task-id-123".to_string(),
        "test-artifact".to_string(),
        "Test artifact for insertion".to_string(),
        FileType::Document,
        file_meta,
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &artifact).await?;

    let found = dao.find_by_id(ctx.clone(), &artifact.id).await?;
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, artifact.id);
    assert_eq!(found.project_id, "project-id-123");
    assert_eq!(found.task_id, Some("task-id-123".to_string()));
    assert_eq!(found.name, "test-artifact");
    assert_eq!(found.file_type, FileType::Document);

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_insert_project_level_artifact(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);

    let file_meta = FileMeta::new(
        "20260415/project-overview.md".to_string(),
        "text/markdown".to_string(),
        2048,
    );
    let artifact = ArtifactPo::new_project(
        "project-id-123".to_string(),
        "project-overview".to_string(),
        "Project level documentation".to_string(),
        FileType::Document,
        file_meta,
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &artifact).await?;

    let found = dao.find_by_id(ctx.clone(), &artifact.id).await?;
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, artifact.id);
    assert_eq!(found.project_id, "project-id-123");
    assert_eq!(found.task_id, None);
    assert_eq!(found.name, "project-overview");

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_list_by_project(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);

    let project_id = "project-id-123".to_string();
    for i in 1..=3 {
        let file_meta = FileMeta::new(
            format!("20260415/{}.md", Uuid::now_v7()),
            "text/markdown".to_string(),
            100 * i,
        );
        let artifact = ArtifactPo::new_task(
            project_id.clone(),
            format!("task-id-{}", i),
            format!("artifact-{}", i),
            "".to_string(),
            FileType::Document,
            file_meta,
            "test-user".to_string(),
        );
        dao.insert(ctx.clone(), &artifact).await?;
    }

    // Add one project-level artifact
    let file_meta = FileMeta::new(
        format!("20260415/{}.md", Uuid::now_v7()),
        "text/markdown".to_string(),
        500,
    );
    let artifact = ArtifactPo::new_project(
        project_id.clone(),
        "project-overview".to_string(),
        "".to_string(),
        FileType::Document,
        file_meta,
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &artifact).await?;

    let list = dao.list_by_project(ctx.clone(), &project_id).await?;
    assert_eq!(list.len(), 4);

    let count = dao.count_by_project(ctx, &project_id).await?;
    assert_eq!(count, 4);

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_list_by_task(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);

    let project_id = "project-id-123".to_string();
    let task_id = "task-id-123".to_string();
    for i in 1..=3 {
        let file_meta = FileMeta::new(
            format!("20260415/{}.md", Uuid::now_v7()),
            "text/markdown".to_string(),
            100 * i,
        );
        let artifact = ArtifactPo::new_task(
            project_id.clone(),
            task_id.clone(),
            format!("artifact-{}", i),
            "".to_string(),
            FileType::Document,
            file_meta,
            "test-user".to_string(),
        );
        dao.insert(ctx.clone(), &artifact).await?;
    }

    let list = dao.list_by_task(ctx.clone(), &task_id).await?;
    assert_eq!(list.len(), 3);

    let count = dao.count_by_task(ctx, &task_id).await?;
    assert_eq!(count, 3);

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_update_status(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);

    let file_meta = FileMeta::new(
        "20260415/test.png".to_string(),
        "image/png".to_string(),
        204800,
    );
    let artifact = ArtifactPo::new_task(
        "project-id-123".to_string(),
        "task-1".to_string(),
        "test-image".to_string(),
        "".to_string(),
        FileType::Image,
        file_meta,
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &artifact).await?;

    // Update status to deleted (0)
    dao.update_status(ctx.clone(), &artifact.id, 0).await?;

    let found = dao.find_by_id(ctx.clone(), &artifact.id).await?;
    // Deleted items are filtered out by find_by_id, so should be None
    assert!(found.is_none());

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_delete_artifact(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);

    let file_meta = FileMeta::new(
        "20260415/test.bin".to_string(),
        "application/octet-stream".to_string(),
        1000,
    );
    let artifact = ArtifactPo::new_task(
        "project-id-123".to_string(),
        "task-1".to_string(),
        "binary-file".to_string(),
        "".to_string(),
        FileType::Binary,
        file_meta,
        "test-user".to_string(),
    );
    dao.insert(ctx.clone(), &artifact).await?;

    let found_before = dao.find_by_id(ctx.clone(), &artifact.id).await?;
    assert!(found_before.is_some());

    dao.delete(ctx.clone(), &artifact.id).await?;

    let found_after = dao.find_by_id(ctx.clone(), &artifact.id).await?;
    assert!(found_after.is_none());

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_all_file_types(pool: SqlitePool) -> Result<()> {
    let (dao, ctx) = init_test_env(pool);

    let types = vec![
        FileType::Document,
        FileType::Image,
        FileType::Audio,
        FileType::Video,
        FileType::Binary,
    ];

    for file_type in types {
        let ext = match file_type {
            FileType::Document => ".md",
            FileType::Image => ".png",
            FileType::Audio => ".mp3",
            FileType::Video => ".mp4",
            FileType::Binary => ".bin",
        };
        let mime = match file_type {
            FileType::Document => "text/markdown",
            FileType::Image => "image/png",
            FileType::Audio => "audio/mpeg",
            FileType::Video => "video/mp4",
            FileType::Binary => "application/octet-stream",
        };
        let file_meta = FileMeta::new(
            format!("20260415/{}{}", Uuid::now_v7(), ext),
            mime.to_string(),
            1000,
        );
        let artifact = ArtifactPo::new_task(
            "project-id-123".to_string(),
            "task-1".to_string(),
            format!("{:?}", file_type),
            "".to_string(),
            file_type,
            file_meta,
            "test-user".to_string(),
        );
        dao.insert(ctx.clone(), &artifact).await?;

        let found = dao.find_by_id(ctx.clone(), &artifact.id).await?;
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.file_type, file_type);
    }

    let list = dao.list_by_task(ctx.clone(), "task-1").await?;
    assert_eq!(list.len(), 5);

    Ok(())
}
