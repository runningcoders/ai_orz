pub mod dal;
pub mod dao;
pub mod domain;

// 初始化所有 service 层组件（由 main 调用）
pub fn init() {
    // 初始化 DAO 层
    dao::init_all();

    // 初始化 DAL 层（依赖 DAO）
    dal::init_all();

    // 初始化 Domain 层（依赖 DAL）
    domain::init_all();
}
