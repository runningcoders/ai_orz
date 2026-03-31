//! Domain 层（业务逻辑层）
//!
//! Domain 层是抽象业务层，关注通用行为逻辑
//! 组合多个 DAL 完成业务逻辑，不关心具体的实现细节

pub mod agent;

pub use agent::{domain as agent_domain, init as init_agent_domain, AgentDomain};

/// 初始化所有 Domain 实例
pub fn init_all() {
    agent::init(crate::service::dal::agent_dal());
}
