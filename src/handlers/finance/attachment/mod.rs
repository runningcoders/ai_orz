//! Attachment 管理 HTTP 接口
//! 按用户 action 拆分，每个接口单独一个文件。

pub mod create_text_attachment;
pub mod delete_attachment;
pub mod get_attachment;
pub mod get_attachment_content;
pub mod list_attachments;
pub mod update_attachment_content;
pub mod upload_attachment;

mod response;

pub use create_text_attachment::create_text_attachment;
pub use delete_attachment::delete_attachment;
pub use get_attachment::get_attachment;
pub use get_attachment_content::get_attachment_content;
pub use list_attachments::list_attachments;
pub use update_attachment_content::update_attachment_content;
pub use upload_attachment::upload_attachment;
