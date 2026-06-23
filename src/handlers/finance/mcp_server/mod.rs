//! MCP Server 管理 HTTP 接口
//! 按方法粒度拆分，每个方法单独一个文件。

pub mod create_mcp_server;
pub mod delete_mcp_server;
pub mod get_mcp_server;
pub mod list_mcp_servers;
pub mod update_mcp_server;
pub mod update_mcp_server_status;

mod response;

#[cfg(test)]
mod list_mcp_servers_test;
#[cfg(test)]
mod response_test;

pub use create_mcp_server::create_mcp_server_handler;
pub use delete_mcp_server::delete_mcp_server_handler;
pub use get_mcp_server::get_mcp_server_handler;
pub use list_mcp_servers::list_mcp_servers_handler;
pub use update_mcp_server::update_mcp_server_handler;
pub use update_mcp_server_status::update_mcp_server_status_handler;
