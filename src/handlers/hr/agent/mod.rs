//! Agent 管理 HTTP 接口
//! 按方法粒度拆分，每个方法单独一个文件

pub mod create_agent;
pub mod create_memory;
pub mod delete_agent;
pub mod delete_memory;
pub mod get_agent;
pub mod install_tool_pack;
pub mod list_agents;
pub mod list_installed_tool_packs;
pub mod query_memory;
pub mod save_long_term_memory;
pub mod save_short_term_memory;
pub mod search_memory;
pub mod settle_memory;
pub mod uninstall_tool_pack;
pub mod update_agent;
pub mod update_agent_status;
pub mod update_memory;

pub use create_agent::create_agent_handler;
pub use create_memory::create_memory_handler;
pub use delete_agent::delete_agent_handler;
pub use delete_memory::delete_memory_handler;
pub use get_agent::get_agent_handler;
pub use install_tool_pack::install_tool_pack_handler;
pub use list_agents::list_agents_handler;
pub use list_installed_tool_packs::list_installed_tool_packs_handler;
pub use query_memory::query_memory_handler;
pub use save_long_term_memory::save_long_term_memory_handler;
pub use save_short_term_memory::save_short_term_memory_handler;
pub use search_memory::search_memory_handler;
pub use settle_memory::settle_memory_handler;
pub use uninstall_tool_pack::uninstall_tool_pack_handler;
pub use update_agent::update_agent_handler;
pub use update_agent_status::update_agent_status_handler;
pub use update_memory::update_memory_handler;
