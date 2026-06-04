//! Message Channel 管理 HTTP 接口
//! 按方法粒度拆分，每个方法单独一个文件

pub mod create_message_channel;
pub mod delete_message_channel;
pub mod get_message_channel;
pub mod list_message_channels;
pub mod test_message_channel_connection;
pub mod update_message_channel;
pub mod update_message_channel_status;

mod response;

pub use create_message_channel::create_message_channel;
pub use delete_message_channel::delete_message_channel;
pub use get_message_channel::get_message_channel;
pub use list_message_channels::list_message_channels;
pub use test_message_channel_connection::test_message_channel_connection;
pub use update_message_channel::update_message_channel;
pub use update_message_channel_status::update_message_channel_status;
