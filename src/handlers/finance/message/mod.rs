//! Message 管理 HTTP 接口
//! 按方法粒度拆分，每个方法单独一个文件。

pub mod send_message;
pub mod send_task_assignment_message;

pub use send_message::send_message_handler;
pub use send_task_assignment_message::send_task_assignment_message_handler;
