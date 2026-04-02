//! HR (Human Resources) Handlers module
//!
//! 人力资源模块 HTTP 接口
//! - Agent 管理
//! - 员工管理 (预留未来扩展)

pub mod agent;

pub use self::agent::{create_agent, delete_agent, get_agent, list_agents, update_agent};
pub use self::agent::{AgentResponse, CreateAgentRequest, UpdateAgentRequest};
