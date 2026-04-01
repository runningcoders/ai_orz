pub mod agent;
pub mod brain;
pub mod model_provider;
pub mod org;

pub use agent::dao as agent_dao;
pub use brain::{Brain, RigAgent};
pub use model_provider::dao as model_provider_dao;
pub use org::dao as org_dao;

pub fn init_all() {
    agent::init();
    model_provider::init();
    org::init();
}
