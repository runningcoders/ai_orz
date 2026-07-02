use crate::models::attachment::AttachmentPo;
use crate::pkg::request_context::RequestContext;
use crate::service::dao::attachment::{AttachmentDao, AttachmentQuery, new_with_attachments_dir};
use common::enums::FileType;
use sqlx::SqlitePool;
use std::sync::Arc;
use common::error::Result;

/// 初始化测试环境
fn init_test_env(
    pool: SqlitePool,
) -> (
    tempfile::TempDir,
    Arc<dyn AttachmentDao + Send + Sync>,
    RequestContext,
) {
    let temp_dir = tempfile::tempdir().unwrap();
    let attachments_dir = temp_dir.path().join("attachments");
    let dao = new_with_attachments_dir(attachments_dir);
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);
    (temp_dir, dao, ctx)
}

fn create_test_attachment(id: &str, original_name: &str, purpose: &str) -> AttachmentPo {
    let extension = std::path::Path::new(original_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{}", value))
        .unwrap_or_default();
    AttachmentPo::new(
        id.to_string(),
        original_name.to_string(),
        format!("{}{}", id, extension),
        format!("20260617/{}{}", id, extension),
        "text/markdown".to_string(),
        FileType::Document,
        12,
        purpose.to_string(),
        "test-user".to_string(),
        "test-user".to_string(),
    )
}

#[sqlx::test(migrations = "./migrations")]
async fn test_insert_and_find_by_id(pool: SqlitePool) -> Result<()> {
    let (_temp_dir, dao, ctx) = init_test_env(pool);
    let attachment = create_test_attachment("attachment-1", "skill.md", "skill");

    dao.insert(ctx.clone(), &attachment).await?;

    let found = dao.find_by_id(ctx, &attachment.id).await?.unwrap();
    assert_eq!(found.id, attachment.id);
    assert_eq!(found.original_name, "skill.md");
    assert_eq!(found.stored_name, "attachment-1.md");
    assert_eq!(found.file_type, FileType::Document);
    assert_eq!(found.root_user_id, "test-user");
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_query_by_root_user_and_purpose(pool: SqlitePool) -> Result<()> {
    let (_temp_dir, dao, ctx) = init_test_env(pool);
    let skill_attachment = create_test_attachment("attachment-1", "skill.md", "skill");
    let message_attachment = create_test_attachment("attachment-2", "message.md", "message");
    let other_user_attachment = AttachmentPo::new(
        "attachment-3".to_string(),
        "other.md".to_string(),
        "attachment-3.md".to_string(),
        "20260617/attachment-3.md".to_string(),
        "text/markdown".to_string(),
        FileType::Document,
        12,
        "skill".to_string(),
        "other-user".to_string(),
        "other-user".to_string(),
    );

    dao.insert(ctx.clone(), &skill_attachment).await?;
    dao.insert(ctx.clone(), &message_attachment).await?;
    dao.insert(ctx.clone(), &other_user_attachment).await?;

    let list = dao
        .query(
            ctx,
            AttachmentQuery {
                root_user_id: Some("test-user".to_string()),
                purpose: Some("skill".to_string()),
                ..Default::default()
            },
        )
        .await?;

    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "attachment-1");
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_delete_filters_find_and_query(pool: SqlitePool) -> Result<()> {
    let (_temp_dir, dao, ctx) = init_test_env(pool);
    let attachment = create_test_attachment("attachment-1", "skill.md", "skill");
    dao.insert(ctx.clone(), &attachment).await?;

    dao.delete(ctx.clone(), &attachment.id).await?;

    let found = dao.find_by_id(ctx.clone(), &attachment.id).await?;
    assert!(found.is_none());
    let list = dao
        .query(
            ctx,
            AttachmentQuery {
                root_user_id: Some("test-user".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert!(list.is_empty());
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_file_read_write_exists(pool: SqlitePool) -> Result<()> {
    let (_temp_dir, dao, _ctx) = init_test_env(pool);
    let relative_path = "20260617/attachment-1.md";
    let bytes = b"hello attachment";

    dao.write_file(relative_path, bytes)?;

    assert!(dao.file_exists(relative_path));
    assert_eq!(dao.read_file(relative_path)?, bytes);
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_update_file_and_metadata_size(pool: SqlitePool) -> Result<()> {
    let (_temp_dir, dao, ctx) = init_test_env(pool);
    let mut attachment = create_test_attachment("attachment-1", "skill.md", "skill");
    dao.write_file(&attachment.relative_path, b"old")?;
    dao.insert(ctx.clone(), &attachment).await?;

    dao.write_file(&attachment.relative_path, b"new content")?;
    dao.update_file_metadata(ctx.clone(), &attachment.id, 11)
        .await?;

    let found = dao.find_by_id(ctx, &attachment.id).await?.unwrap();
    assert_eq!(found.size, 11);
    assert_eq!(found.modified_by, "test-user");
    assert!(found.updated_at >= attachment.updated_at);
    assert_eq!(dao.read_file(&attachment.relative_path)?, b"new content");

    attachment.mark_deleted("test-user".to_string());
    assert_eq!(attachment.status, 0);
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_reject_path_traversal(pool: SqlitePool) -> Result<()> {
    let (_temp_dir, dao, _ctx) = init_test_env(pool);

    let result = dao.write_file("../escape.md", b"bad");

    assert!(result.is_err());
    Ok(())
}
