//! DAL 层（数据访问层）
//!
//! DAL 层是业务逻辑层，不关心具体的存储细节
//! 它组合多个 DAO 完成业务逻辑，使用业务对象而非 Po

pub mod agent;
pub mod brain;
pub mod model_provider;
pub mod organization;

pub fn init_all(){
    agent::init();
    brain::init();
    model_provider::init();
    organization::init()
}


#[cfg(test)]
pub(crate) mod agent_test;
#[cfg(test)]
pub(crate) mod brain_test;
#[cfg(test)]
pub(crate) mod model_provider_test;
