//! Tool (工具) 管理 HTTP 接口
//! 按方法粒度拆分，每个方法单独一个文件

pub mod bind_tool_to_agent;
pub mod create_tool;
pub mod delete_tool;
pub mod get_tool;
pub mod list_tools;
pub mod unbind_tool_from_agent;
pub mod update_tool;
pub mod update_tool_status;

pub(crate) mod response;

#[cfg(test)]
mod response_test;
#[cfg(test)]
mod update_tool_test;

pub use bind_tool_to_agent::bind_tool_to_agent_handler;
pub use create_tool::create_tool_handler;
pub use delete_tool::delete_tool_handler;
pub use get_tool::get_tool_handler;
pub use list_tools::list_tools_handler;
pub use unbind_tool_from_agent::unbind_tool_from_agent_handler;
pub use update_tool::update_tool_handler;
pub use update_tool_status::update_tool_status_handler;
