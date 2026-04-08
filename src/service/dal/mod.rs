//! DAL 层（数据访问层）
//!
//! DAL 层是业务逻辑层，不关心具体的存储细节
//! 它组合多个 DAO 完成业务逻辑，使用业务对象而非 Po


pub mod agent;
pub mod brain;
pub mod model_provider;
pub mod organization;

pub use agent::dal as agent_dal;
pub use model_provider::dal as model_provider_dal;
pub use organization::dal as organization_dal;

/// 初始化所有 DAL 实例
pub fn init_all() {
    agent::init(crate::service::dao::agent_dao());
    brain::init(crate::service::dao::cortex::dao());
    model_provider::init(crate::service::dao::model_provider_dao());
    organization::init();
}
