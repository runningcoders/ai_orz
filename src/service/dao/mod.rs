pub mod agent;
pub mod cortex;
pub mod model_provider;
pub mod org;

pub use agent::dao as agent_dao;
pub use cortex::{dao as cortex_dao, CortexDao, RigCortexDao};
pub use crate::models::brain::{Brain};
pub use model_provider::dao as model_provider_dao;
pub use org::dao as org_dao;

pub fn init_all() {
    agent::init();
    cortex::init();
    model_provider::init();
    org::init();
}
