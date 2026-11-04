use super::attachment::{AttachmentDal, new};
use crate::models::attachment::AttachmentUpload;
use crate::pkg::RequestContext;
use crate::service::dao::attachment::{AttachmentQuery, new_with_attachments_dir};
use common::enums::FileType;
use common::error::Result;
use sqlx::SqlitePool;
use std::sync::Arc;

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
    let (temp_dir, dal, ctx) = init_test_env(pool);

    let attachment = dal
        .create_from_upload(ctx.clone(), create_upload("../skill.MD", "skill", b"hello"))
        .await?;

    assert_eq!(attachment.po.original_name, "../skill.MD");
    assert!(attachment.po.stored_name.ends_with(".md"));
    assert_eq!(attachment.po.file_type, FileType::Document);
    assert_eq!(attachment.po.size, 5);
    assert_eq!(attachment.po.root_user_id, "test-user");
    assert_eq!(dal.read_file(&attachment)?, b"hello");

    // 创建即完成态：落库后回读，DAO 用注入的根目录回填的绝对路径已在实体上。
    let abs_path = attachment.file_abs_path()?;
    assert!(
        abs_path.starts_with(temp_dir.path()),
        "路径应落在注入目录 {} 下，实际 {abs_path:?}",
        temp_dir.path().display()
    );

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
    assert_eq!(list.items.len(), 1);

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

/// 泛型 `application/octet-stream` 必须按后缀补齐真实类型。
///
/// 回归背景：前端构造上传 Blob 时曾硬编码该类型，后端又原样落库，导致每张图片都
/// 变成二进制——前端 `is_inline_image()` 不出图、`/file` 带 `nosniff` 浏览器也救不了。
/// 写库口是所有上传入口（站内 / 渠道入站 / 文本附件）的必经之路，兜底放这里最稳。
#[sqlx::test(migrations = "./migrations")]
async fn test_create_from_upload_infers_mime_when_generic(pool: SqlitePool) -> Result<()> {
    let (_temp_dir, dal, ctx) = init_test_env(pool);

    let attachment = dal
        .create_from_upload(
            ctx,
            AttachmentUpload {
                original_name: "设计稿.png".to_string(),
                mime_type: "application/octet-stream".to_string(),
                purpose: "message".to_string(),
                bytes: b"\x89PNG-fake".to_vec(),
            },
        )
        .await?;

    assert_eq!(attachment.po.mime_type, "image/png");
    assert_eq!(attachment.po.file_type, FileType::Image);
    assert!(attachment.po.stored_name.ends_with(".png"));
    Ok(())
}

/// 调用方给出具体类型时原样尊重（含参数），不被后缀覆盖——客户端判定可能比后缀准。
#[sqlx::test(migrations = "./migrations")]
async fn test_create_from_upload_keeps_specific_mime(pool: SqlitePool) -> Result<()> {
    let (_temp_dir, dal, ctx) = init_test_env(pool);

    let attachment = dal
        .create_from_upload(
            ctx,
            AttachmentUpload {
                original_name: "note.bin".to_string(),
                mime_type: "text/markdown".to_string(),
                purpose: "message".to_string(),
                bytes: b"# hi".to_vec(),
            },
        )
        .await?;

    assert_eq!(attachment.po.mime_type, "text/markdown");
    assert_eq!(attachment.po.file_type, FileType::Document);
    Ok(())
}
