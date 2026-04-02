//! Domain 层（业务逻辑层）
//!
//! Domain 层是抽象业务逻辑层，关注通用行为逻辑
//! 组合多个 DAL 完成业务逻辑，不关心具体的实现细节

pub mod hr;

pub use hr::{domain as hr_domain, init as init_hr_domain, HrDomain};

/// 初始化所有 Domain 实例
pub fn init_all() {
    hr::init(crate::service::dal::agent_dal());
}
