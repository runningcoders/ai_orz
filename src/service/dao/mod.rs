pub mod agent_dao;
pub mod org_dao;
pub mod task_dao;

pub use agent_dao::agent_dao;
pub use org_dao::org_dao;

pub(super) fn init_all() {
    agent_dao::init();
    org_dao::init();
}
