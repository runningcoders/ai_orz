//! DAL 层（数据访问层）
//!
//! DAL 层是业务逻辑层，不关心具体的存储细节
//! 它组合多个 DAO 完成业务逻辑，使用业务对象而非 Po

use std::sync::Arc;
use crate::service::dao::agent::AgentDaoTrait;

pub mod agent;

pub use agent::{dal as agent_dal, AgentDal, AgentDalTrait};

/// 初始化 Agent DAL 并自动初始化 Domain
pub fn init_agent_dal(agent_dao: Arc<dyn AgentDaoTrait>) {
    agent::init(agent_dao);
    // 初始化 Domain 层（依赖 DAL）
    super::domain::init_agent_domain(agent::dal());
}
