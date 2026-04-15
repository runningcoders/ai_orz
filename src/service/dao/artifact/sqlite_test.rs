use sqlx::SqlitePool;
use common::enums::{FileType, TaskStatus};
use common::config::AppConfig;
use crate::error::Result;
use crate::models::{ArtifactPo, FileMeta};
use crate::service::dao::artifact::{ArtifactDaoTrait, dao};
use crate::pkg::request_context::RequestContext;
use uuid::Uuid;

#[sqlx::test(migrations = "./migrations")]
async fn test_insert_artifact(pool: SqlitePool) -> Result<()> {
    let ctx = RequestContext::new_simple("test-user", pool);
    let dao = dao();
    
    let file_id = Uuid::now_v7().to_string();
    let file_meta = FileMeta::new(
        format!("20260415/{}.md", file_id),
        "text/markdown".to_string(),
        1024,
    );
    let mut artifact = ArtifactPo::new(
        file_id.clone(),
        "task-id-123".to_string(),
        "test-artifact".to_string(),
        "This is a test artifact".to_string(),
        FileType::Document,
        file_meta,
        "test-user".to_string(),
    );
    
    dao.insert(&ctx, &artifact).await?;
    let found = dao.find_by_id(&ctx, &file_id).await?;
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, file_id);
    assert_eq!(found.task_id, "task-id-123");
    assert_eq!(found.name, "test-artifact");
    assert_eq!(found.status, 1); // active
    
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_list_by_task(pool: SqlitePool) -> Result<()> {
    let ctx = RequestContext::new_simple("test-user", pool);
    let dao = dao();
    
    let task_id = "task-abc".to_string();
    
    // Insert 3 artifacts for the same task
    for i in 1..=3 {
        let file_id = Uuid::now_v7().to_string();
        let file_meta = FileMeta::new(
            format!("20260415/{}.md", file_id),
            "text/markdown".to_string(),
            100 * i,
        );
        let artifact = ArtifactPo::new(
            file_id,
            task_id.clone(),
            format!("artifact-{}", i),
            "".to_string(),
            FileType::Document,
            file_meta,
            "test-user".to_string(),
        );
        dao.insert(&ctx, &artifact).await?;
    }
    
    let list = dao.list_by_task(&ctx, &task_id).await?;
    assert_eq!(list.len(), 3);
    
    let count = dao.count_by_task(&ctx, &task_id).await?;
    assert_eq!(count, 3);
    
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_update_status(pool: SqlitePool) -> Result<()> {
    let ctx = RequestContext::new_simple("test-user", pool);
    let dao = dao();
    
    let file_id = Uuid::now_v7().to_string();
    let file_meta = FileMeta::new(
        format!("20260415/{}.png", file_id),
        "image/png".to_string(),
        204800,
    );
    let artifact = ArtifactPo::new(
        file_id.clone(),
        "task-1".to_string(),
        "test-image".to_string(),
        "".to_string(),
        FileType::Image,
        file_meta,
        "test-user".to_string(),
    );
    dao.insert(&ctx, &artifact).await?;
    
    // Update status to deleted (0)
    dao.update_status(&ctx, &file_id, 0).await?;
    
    let found = dao.find_by_id(&ctx, &file_id).await?;
    assert!(found.is_none()); // soft delete filtered out
    
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_delete_artifact(pool: SqlitePool) -> Result<()> {
    let ctx = RequestContext::new_simple("test-user", pool);
    let dao = dao();
    
    let file_id = Uuid::now_v7().to_string();
    let file_meta = FileMeta::new(
        format!("20260415/{}.bin", file_id),
        "application/octet-stream".to_string(),
        512,
    );
    let artifact = ArtifactPo::new(
        file_id.clone(),
        "task-1".to_string(),
        "binary-file".to_string(),
        "".to_string(),
        FileType::Binary,
        file_meta,
        "test-user".to_string(),
    );
    dao.insert(&ctx, &artifact).await?;
    
    let found_before = dao.find_by_id(&ctx, &file_id).await?;
    assert!(found_before.is_some());
    
    dao.delete(&ctx, &file_id).await?;
    
    let found_after = dao.find_by_id(&ctx, &file_id).await?;
    assert!(found_after.is_none());
    
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_all_file_types(pool: SqlitePool) -> Result<()> {
    let ctx = RequestContext::new_simple("test-user", pool);
    let dao = dao();
    
    let types = vec![
        FileType::Document,
        FileType::Image,
        FileType::Audio,
        FileType::Video,
        FileType::Binary,
    ];
    
    for file_type in types {
        let file_id = Uuid::now_v7().to_string();
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
            format!("20260415/{}{}", file_id, ext),
            mime.to_string(),
            1000,
        );
        let artifact = ArtifactPo::new(
            file_id.clone(),
            "task-1".to_string(),
            format!("{:?}", file_type),
            "".to_string(),
            file_type,
            file_meta,
            "test-user".to_string(),
        );
        dao.insert(&ctx, &artifact).await?;
        
        let found = dao.find_by_id(&ctx, &file_id).await?;
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.file_type, file_type as i32);
    }
    
    Ok(())
}
