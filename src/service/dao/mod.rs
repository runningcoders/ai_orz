pub mod agent_dao;
pub mod org_dao;
pub mod task_dao;

// DAO 初始化函数（由 service 层统一调用）
pub use agent_dao::get_agent_dao;
pub use org_dao::get_org_dao;

pub(super) fn init_all() {
    agent_dao::init();
    org_dao::init();
    // task_dao::init();
}
