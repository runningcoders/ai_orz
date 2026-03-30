pub mod agent_dao_sqlite;
pub mod org_dao_sqlite;
pub mod task_dao;

pub use agent_dao_sqlite::agent_dao;
pub use org_dao_sqlite::org_dao;

pub(super) fn init_all() {
    agent_dao_sqlite::init();
    org_dao_sqlite::init();
}
