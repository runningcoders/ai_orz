//! Domain 层（业务逻辑层）
//!
//! 分类存放不同业务领域：
//! - hr → 人力资源（智能体管理）
//! - finance → 财务管理（模型提供商管理）
//! - organization → 组织管理（组织和用户管理）

pub mod hr;
pub mod finance;
pub mod organization;

// Tests are located in subdirectories: finance/model_provider_test.rs and hr/agent_test.rs
// No need to declare them here because mod rs already declared in subdirectories

pub use hr::init as init_hr_domain;
pub use finance::init as init_finance_domain;

/// 初始化所有 Domain
pub fn init_all() {
    init_hr_domain(crate::service::dal::agent_dal());
    init_finance_domain(crate::service::dal::model_provider_dal());
    organization::init(crate::service::dal::organization_dal());
}
