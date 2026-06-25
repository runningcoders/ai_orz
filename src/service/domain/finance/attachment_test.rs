//! Attachment 管理测试
//!
//! 通用上传文件资产 CRUD 测试，属于 Finance Domain。

use crate::error::{common::error::Error, Result};
use crate::models::attachment::{
    Attachment, AttachmentGetOptions, AttachmentReadResult, AttachmentTextContent,
    AttachmentUpload, TextAttachmentCreate, TextContentUpdate,
};
use crate::pkg::RequestContext;
use crate::service::dao::attachment::AttachmentQuery;
use crate::service::domain::finance;
use sqlx::SqlitePool;
use std::sync::Arc;
use common::error::Result;
use common::bail_err;

fn init_test_env(
    pool: SqlitePool,
) -> (
    tempfile::TempDir,
    Arc<dyn finance::FinanceDomain>,
    RequestContext,
) {
    let temp_dir = tempfile::tempdir().unwrap();
    let attachment_dao = crate::service::dao::attachment::new_with_attachments_dir(
        temp_dir.path().join("attachments"),
    );
    let attachment_dal = crate::service::dal::attachment::new(attachment_dao);

    // Attachment 测试只使用 AttachmentManage；其它依赖使用单例保持构造兼容。
    crate::service::dao::model_provider::init();
    crate::service::dao::message_channel::init();
    crate::service::dao::mcp_server::init();
    crate::service::dao::tool::init();
    crate::service::dao::tool_call::init();
    crate::service::dao::cortex::init();
    crate::service::dao::lark::init();
    crate::service::dao::wechat::init();
    crate::service::dao::slack::init();
    crate::service::dao::email::init();
    crate::service::dao::webhook::init();
    crate::service::dal::model_provider::init();
    crate::service::dal::message_channel::init();
    crate::service::dal::mcp_server::init();
    crate::service::dal::mcp_tool::init();
    crate::service::dal::tool::init();
    crate::service::dal::brain::init();

    let domain = finance::new(
        crate::service::dal::model_provider::dal(),
        crate::service::dal::message_channel::dal(),
        crate::service::dal::mcp_server::dal(),
        crate::service::dal::mcp_tool::dal(),
        crate::service::dal::tool::dal(),
        crate::service::dal::brain::dal(),
        attachment_dal,
    );
    let ctx = RequestContext::new_simple("test-user", pool);
    (temp_dir, domain, ctx)
}

fn create_upload() -> AttachmentUpload {
    AttachmentUpload {
        original_name: "skill.md".to_string(),
        mime_type: "text/markdown".to_string(),
        purpose: "skill".to_string(),
        bytes: b"# Skill".to_vec(),
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn test_attachment_create_query_get_delete(pool: SqlitePool) -> Result<()> {
    let (_temp_dir, domain, ctx) = init_test_env(pool);

    let attachment = domain
        .attachment_manage()
        .create_attachment(ctx.clone(), create_upload())
        .await?;

    assert_eq!(attachment.po.original_name, "skill.md");
    assert_eq!(attachment.po.root_user_id, "test-user");

    let found = domain
        .attachment_manage()
        .get_attachment(
            ctx.clone(),
            attachment.id(),
            AttachmentGetOptions::default(),
        )
        .await?
        .unwrap();
    assert_eq!(found.id(), attachment.id());
    assert!(found.read_results.is_empty());

    let list = domain
        .attachment_manage()
        .query_attachments(
            ctx.clone(),
            AttachmentQuery {
                root_user_id: Some("test-user".to_string()),
                purpose: Some("skill".to_string()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(list.len(), 1);

    domain
        .attachment_manage()
        .delete_attachment(ctx.clone(), attachment.id())
        .await?;
    assert!(
        domain
            .attachment_manage()
            .get_attachment(ctx, attachment.id(), AttachmentGetOptions::default())
            .await?
            .is_none()
    );
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_get_attachment_can_include_file_content(pool: SqlitePool) -> Result<()> {
    let (_temp_dir, domain, ctx) = init_test_env(pool);

    let attachment = domain
        .attachment_manage()
        .create_attachment(ctx.clone(), create_upload())
        .await?;

    let found = domain
        .attachment_manage()
        .get_attachment(
            ctx,
            attachment.id(),
            AttachmentGetOptions {
                include_file_content: true,
            },
        )
        .await?
        .unwrap();

    assert_eq!(found.read_results.len(), 1);
    let read_result = &found.read_results[0];
    assert_eq!(read_result.relative_path, attachment.po.relative_path);
    assert_eq!(read_result.bytes, b"# Skill".to_vec());
    assert_eq!(read_result.size, 7);
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_get_attachment_hides_cross_root_user_assets(pool: SqlitePool) -> Result<()> {
    let (_temp_dir, domain, owner_ctx) = init_test_env(pool.clone());

    let attachment = domain
        .attachment_manage()
        .create_attachment(owner_ctx, create_upload())
        .await?;
    let other_ctx = RequestContext::new_simple("other-user", pool);

    let found = domain
        .attachment_manage()
        .get_attachment(
            other_ctx,
            attachment.id(),
            AttachmentGetOptions {
                include_file_content: true,
            },
        )
        .await?;

    assert!(found.is_none());
    Ok(())
}

#[test]
fn text_attachment_domain_contract_types_are_available() {
    let create = TextAttachmentCreate {
        file_name: "notes.md".to_string(),
        content: "# Notes".to_string(),
        mime_type: None,
        purpose: Some("skill".to_string()),
    };
    assert_eq!(create.file_name, "notes.md");

    let update = TextContentUpdate {
        content: "updated".to_string(),
        expected_updated_at: Some(42),
    };
    assert_eq!(update.expected_updated_at, Some(42));

    let upload = AttachmentUpload {
        original_name: "notes.md".to_string(),
        mime_type: "text/markdown".to_string(),
        purpose: "skill".to_string(),
        bytes: b"updated".to_vec(),
    };
    let po = crate::models::attachment::AttachmentPo::new(
        "att-1".to_string(),
        upload.original_name,
        "att-1.md".to_string(),
        "20260617/att-1.md".to_string(),
        upload.mime_type,
        common::enums::FileType::Document,
        upload.bytes.len() as i64,
        upload.purpose,
        "test-user".to_string(),
        "test-user".to_string(),
    );
    let attachment = Attachment::from_po(po).with_read_result(AttachmentReadResult {
        relative_path: "20260617/att-1.md".to_string(),
        bytes: b"updated".to_vec(),
        size: 7,
    });
    let content = AttachmentTextContent {
        attachment,
        content: "updated".to_string(),
        encoding: "utf-8".to_string(),
        size: 7,
        updated_at: 42,
    };
    assert_eq!(content.encoding, "utf-8");
    assert_eq!(content.attachment.read_results.len(), 1);

    assert_eq!(common::error::Error::conflict("stale".to_string()).code(), 409);
    assert_eq!(
        common::error::Error::PayloadTooLarge("too large".to_string()).code(),
        413
    );
}
