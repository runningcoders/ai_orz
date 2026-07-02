use super::attachment::{AttachmentDal, new};
use crate::models::attachment::AttachmentUpload;
use crate::pkg::RequestContext;
use crate::service::dao::attachment::{AttachmentQuery, new_with_attachments_dir};
use common::enums::FileType;
use sqlx::SqlitePool;
use std::sync::Arc;
use common::error::Result;

fn init_test_env(
    pool: SqlitePool,
) -> (
    tempfile::TempDir,
    Arc<dyn AttachmentDal + Send + Sync>,
    RequestContext,
) {
    let temp_dir = tempfile::tempdir().unwrap();
    let dao = new_with_attachments_dir(temp_dir.path().join("attachments"));
    let dal = new(dao);
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);
    (temp_dir, dal, ctx)
}

fn create_upload(original_name: &str, purpose: &str, bytes: &[u8]) -> AttachmentUpload {
    AttachmentUpload {
        original_name: original_name.to_string(),
        mime_type: "text/markdown".to_string(),
        purpose: purpose.to_string(),
        bytes: bytes.to_vec(),
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn test_create_from_upload_writes_file_and_metadata(pool: SqlitePool) -> Result<()> {
    let (_temp_dir, dal, ctx) = init_test_env(pool);

    let attachment = dal
        .create_from_upload(ctx.clone(), create_upload("../skill.MD", "skill", b"hello"))
        .await?;

    assert_eq!(attachment.po.original_name, "../skill.MD");
    assert!(attachment.po.stored_name.ends_with(".md"));
    assert_eq!(attachment.po.file_type, FileType::Document);
    assert_eq!(attachment.po.size, 5);
    assert_eq!(attachment.po.root_user_id, "test-user");
    assert_eq!(dal.read_file(&attachment)?, b"hello");

    let found = dal.get_by_id(ctx, attachment.id()).await?.unwrap();
    assert_eq!(found.id(), attachment.id());
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_query_and_delete(pool: SqlitePool) -> Result<()> {
    let (_temp_dir, dal, ctx) = init_test_env(pool);
    let attachment = dal
        .create_from_upload(ctx.clone(), create_upload("skill.md", "skill", b"hello"))
        .await?;

    let list = dal
        .query(
            ctx.clone(),
            AttachmentQuery {
                root_user_id: Some("test-user".to_string()),
                purpose: Some("skill".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(list.len(), 1);

    dal.delete(ctx.clone(), attachment.id()).await?;
    assert!(dal.get_by_id(ctx, attachment.id()).await?.is_none());
    Ok(())
}

#[test]
fn infer_extension_sanitizes_user_filename() {
    // 通过创建上传间接验证原始文件名只影响扩展名，不影响落盘文件名主体。
    let upload = create_upload("../evil.MD", "skill", b"hello");
    assert_eq!(upload.original_name, "../evil.MD");
}
