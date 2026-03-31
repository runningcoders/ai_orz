//! DAL 层（数据访问层）
//!
//! DAL 层是业务逻辑层，不关心具体的存储细节
//! 它组合多个 DAO 完成业务逻辑，使用业务对象而非 Po

use std::sync::Arc;
use crate::service::dao::agent::AgentDaoTrait;

pub mod agent;

pub use agent::{dal as agent_dal, AgentDal, AgentDalTrait};

/// 初始化所有 DAL 实例
pub fn init_all() {
    agent::init(crate::service::dao::agent_dao());
}
