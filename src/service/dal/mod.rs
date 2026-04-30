//! DAL 层（数据访问层）
//!
//! DAL 层是业务逻辑层，不关心具体的存储细节
//! 它组合多个 DAO 完成业务逻辑，使用业务对象而非 Po

pub mod agent;
pub mod brain;
pub mod message;
pub mod model_provider;
pub mod organization;
pub mod tool;
pub mod user;

pub fn init_all(){
    agent::init();
    brain::init();
    message::init();
    model_provider::init();
    organization::init();
    tool::init();
    user::init();
}


#[cfg(test)]
pub(crate) mod agent_test;
#[cfg(test)]
pub(crate) mod brain_test;
#[cfg(test)]
pub(crate) mod message_test;
#[cfg(test)]
pub(crate) mod model_provider_test;
#[cfg(test)]
pub(crate) mod tool_test;
#[cfg(test)]
pub(crate) mod user_test;
