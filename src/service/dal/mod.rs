//! DAL 层（数据访问层）
//!
//! DAL 层是业务逻辑层，不关心具体的存储细节
//! 它组合多个 DAO 完成业务逻辑，使用业务对象而非 Po

use std::sync::Arc;

pub mod agent;
pub mod model_provider;

pub use agent::{dal as agent_dal, AgentDal, AgentDalTrait};
pub use model_provider::{dal as model_provider_dal, ModelProviderDal, ModelProviderDalTrait};

/// 初始化所有 DAL 实例
pub fn init_all() {
    let cortex_dao = crate::service::dao::cortex::dao();
    agent::init(crate::service::dao::agent_dao());
    model_provider::init(crate::service::dao::model_provider_dao(), cortex_dao);
}
