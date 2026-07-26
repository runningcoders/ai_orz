//! Tool (工具) 管理 HTTP 接口
//! 按方法粒度拆分，每个方法单独一个文件

pub mod bind_tool_to_agent;
pub mod create_tool;
pub mod debug_call_tool;
pub mod delete_tool;
pub mod get_tool;
pub mod get_tool_call_entry;
pub mod list_tools;
pub mod query_tool_call_entries;
pub mod query_tools;
pub mod request_tool_call;
pub mod send_tool_call_message;
pub mod unbind_tool_from_agent;
pub mod update_tool;
pub mod update_tool_status;

pub(crate) mod response;

#[cfg(test)]
mod response_test;
#[cfg(test)]
mod tool_call_entry_test;
#[cfg(test)]
mod update_tool_test;

pub use bind_tool_to_agent::bind_tool_to_agent_handler;
pub use create_tool::create_tool_handler;
pub use debug_call_tool::debug_call_tool_handler;
pub use delete_tool::delete_tool_handler;
pub use get_tool::get_tool_handler;
pub use get_tool_call_entry::get_tool_call_entry_handler;
pub use list_tools::list_tools_handler;
pub use query_tool_call_entries::query_tool_call_entries_handler;
pub use query_tools::query_tools_handler;
pub use request_tool_call::request_tool_call_handler;
pub use send_tool_call_message::send_tool_call_message_handler;
pub use unbind_tool_from_agent::unbind_tool_from_agent_handler;
pub use update_tool::update_tool_handler;
pub use update_tool_status::update_tool_status_handler;
