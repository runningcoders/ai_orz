pub mod dal;
pub mod dao;
pub mod domain;

// 初始化所有 service 层组件（由 main 调用）
pub fn init() {
    // 初始化 DAO 层，它会自动初始化 DAL，DAL 会自动初始化 Domain
    dao::init_all();
}
