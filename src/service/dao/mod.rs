pub mod agent;
pub mod org;

pub use agent::dao as agent_dao;
pub use org::dao as org_dao;

pub(super) fn init_all() {
    agent::init();
    org::init();
    
    // 初始化 DAL 层（依赖 DAO）
    super::dal::init_agent_dal(agent_dao());
}
