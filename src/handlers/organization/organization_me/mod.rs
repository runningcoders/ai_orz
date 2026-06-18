//! Current organization (当前组织) 管理 HTTP 接口

pub mod get_current_organization;
pub mod update_current_organization;

pub use get_current_organization::get_current_organization_handler;
pub use update_current_organization::update_current_organization_handler;