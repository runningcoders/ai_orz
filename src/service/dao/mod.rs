pub mod agent;          // Trait 定义
pub mod agent_sqlite;  // SQLite 实现
pub mod org;           // Trait 定义
pub mod org_sqlite;    // SQLite 实现
pub mod task_dao;

pub use agent_sqlite::dao as agent_dao;
pub use org_sqlite::dao as org_dao;

pub(super) fn init_all() {
    agent_sqlite::init();
    org_sqlite::init();
}
