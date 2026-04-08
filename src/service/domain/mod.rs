//! Domain 层（业务逻辑层）
//!
//! 分类存放不同业务领域：
//! - hr → 人力资源（智能体管理）
//! - finance → 财务管理（模型提供商管理）
//! - organization → 组织管理（组织和用户管理）

pub mod hr;
pub mod finance;
pub mod organization;

pub use hr::{domain as hr_domain, init as init_hr_domain, HrDomain};
pub use finance::{domain as finance_domain, init as init_finance_domain, FinanceDomain};
pub use organization::{domain as organization_domain, OrganizationDomain};

/// 初始化所有 Domain
pub fn init_all() {
    init_hr_domain(crate::service::dal::agent_dal());
    init_finance_domain(crate::service::dal::model_provider_dal());
    organization::init(crate::service::dal::organization_dal());
}
