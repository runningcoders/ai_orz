//! Message 管理 HTTP 接口
//! 按方法粒度拆分，每个方法单独一个文件。

pub mod send_message;

pub use send_message::send_message_handler;
