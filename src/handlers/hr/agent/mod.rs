//! Agent 管理 HTTP 接口
//! 按方法粒度拆分，每个方法单独一个文件

pub mod create_agent;
pub mod delete_agent;
pub mod get_agent;
pub mod list_agents;
pub mod update_agent;
pub mod update_agent_status;

pub use create_agent::create_agent_handler;
pub use delete_agent::delete_agent_handler;
pub use get_agent::get_agent_handler;
pub use list_agents::list_agents_handler;
pub use update_agent::update_agent_handler;
pub use update_agent_status::update_agent_status_handler;
