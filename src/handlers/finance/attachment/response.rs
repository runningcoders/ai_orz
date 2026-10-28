use crate::models::attachment::{Attachment, AttachmentTextContent};
use common::api::{AttachmentContentResponse, AttachmentDetail, TextContentResponse};

pub(super) fn to_detail(attachment: &Attachment) -> AttachmentDetail {
    AttachmentDetail {
        id: attachment.po.id.clone(),
        original_name: attachment.po.original_name.clone(),
        stored_name: attachment.po.stored_name.clone(),
        relative_path: attachment.po.relative_path.clone(),
        mime_type: attachment.po.mime_type.clone(),
        file_type: attachment.po.file_type,
        size: attachment.po.size as u64,
        purpose: attachment.po.purpose.clone(),
        description: attachment.po.description.clone(),
        description_usage: attachment.po.description_usage.clone(),
        description_source: attachment.po.description_source.clone(),
        description_model: attachment.po.description_model.clone(),
        description_message_id: attachment.po.description_message_id.clone(),
        described_at: attachment.po.described_at,
        root_user_id: attachment.po.root_user_id.clone(),
        created_by: attachment.po.created_by.clone(),
        created_at: attachment.po.created_at,
        updated_at: attachment.po.updated_at,
    }
}

pub(super) fn to_content_response(content: &AttachmentTextContent) -> AttachmentContentResponse {
    AttachmentContentResponse {
        attachment: to_detail(&content.attachment),
        text: TextContentResponse {
            content: content.content.clone(),
            encoding: content.encoding.clone(),
            size: content.size,
            updated_at: content.updated_at,
        },
    }
}
