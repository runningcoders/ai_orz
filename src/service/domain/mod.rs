//! Domain 层（业务逻辑层）
//!
//! Domain 层是抽象业务逻辑层，关注通用行为逻辑
//! 组合多个 DAL 完成业务逻辑，不关心具体的实现细节

pub mod hr;
pub mod finance;

pub use hr::{domain as hr_domain, init as init_hr_domain, HrDomain};
pub use finance::{domain as finance_domain, init as init_finance_domain, FinanceDomain};

/// 初始化所有 Domain 实例
pub fn init_all() {
    hr::init(crate::service::dal::agent_dal());
    finance::init(crate::service::dal::model_provider_dal());
}
