pub mod domain;
pub mod dal;
pub mod dao;

// 初始化所有 service 层组件（由 main 调用）
pub fn init() {
    dao::init_all();
}
