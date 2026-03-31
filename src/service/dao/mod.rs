pub mod agent;
pub mod org;

pub use agent::dao as agent_dao;
pub use org::dao as org_dao;

pub fn init_all() {
    agent::init();
    org::init();
}
