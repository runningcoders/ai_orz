//! Organization domain 领域模块
//!
//! 组织管理领域：包含组织设置管理和用户管理

pub mod domain;
pub mod organization;
pub mod user;

use std::sync::Arc;

use crate::service::dal;

pub use domain::{OrganizationDomain, OrganizationDomainImpl};
pub use organization::*;
pub use user::*;

/// 初始化 Organization domain
pub fn init(
    dal: Arc<dyn dal::organization::OrganizationDalTrait + Send + Sync>,
) -> Arc<dyn OrganizationDomain + Send + Sync> {
    Arc::new(OrganizationDomainImpl::new(dal))
}
