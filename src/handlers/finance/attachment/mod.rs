//! Attachment 管理 HTTP 接口
//! 按用户 action 拆分，每个接口单独一个文件。

pub mod delete_attachment;
pub mod get_attachment;
pub mod list_attachments;
pub mod upload_attachment;

mod response;

pub use delete_attachment::delete_attachment;
pub use get_attachment::get_attachment;
pub use list_attachments::list_attachments;
pub use upload_attachment::upload_attachment;
