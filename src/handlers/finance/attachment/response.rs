use crate::models::attachment::{Attachment, AttachmentTextContent};
use common::api::{AttachmentContentResponse, AttachmentDetail, TextContentResponse};

pub(super) fn to_detail(attachment: &Attachment) -> AttachmentDetail {
    AttachmentDetail {
        id: attachment.po.id.clone(),
        original_name: attachment.po.original_name.clone(),
        stored_name: attachment.po.stored_name.clone(),
        relative_path: attachment.po.relative_path.clone(),
        // 读侧同样归一：归一化落库上线前存下的附件可能是泛型 octet-stream，
        // 直接吐给前端会让 `is_inline_image()` 判否 → 图片退化成下载卡片。
        mime_type: attachment.po.effective_mime_type(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::attachment::AttachmentPo;
    use common::enums::FileType;

    fn detail_of(mime_type: &str, original_name: &str) -> AttachmentDetail {
        let po = AttachmentPo::new(
            "att_1".to_string(),
            original_name.to_string(),
            "att_1.png".to_string(),
            "attachments/att_1.png".to_string(),
            mime_type.to_string(),
            FileType::Image,
            3,
            "message".to_string(),
            "u1".to_string(),
            "u1".to_string(),
        );
        to_detail(&Attachment::from_po(po))
    }

    /// 出口必须走 `effective_mime_type()`：历史 octet-stream 附件补回真实类型，
    /// 具体类型原样透出。否则前端 `is_inline_image()` 判否，图片永久退化成下载卡片。
    #[test]
    fn detail_uses_effective_mime_type() {
        assert_eq!(
            detail_of("application/octet-stream", "设计稿.png").mime_type,
            "image/png"
        );
        assert_eq!(detail_of("image/webp", "cat.png").mime_type, "image/webp");
    }
}
