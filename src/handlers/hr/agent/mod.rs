//! Agent 管理 HTTP 接口
//! 按方法粒度拆分，每个方法单独一个文件

pub mod create_agent;
pub mod delete_agent;
pub mod get_agent;
pub mod list_agents;
pub mod update_agent;

pub use create_agent::{create_agent, CreateAgentRequest, CreateAgentResponse};
pub use delete_agent::delete_agent;
pub use get_agent::{get_agent, GetAgentResponse};
pub use list_agents::{list_agents, AgentListItem};
pub use update_agent::{update_agent, UpdateAgentRequest, UpdateAgentResponse};
