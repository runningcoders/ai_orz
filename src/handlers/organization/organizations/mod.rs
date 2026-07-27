//! Organization (组织) 管理 HTTP 接口

pub mod delete_organization;
pub mod get_organization;
pub mod list_organizations;
pub mod update_organization;

pub use delete_organization::delete_organization_handler;
pub use get_organization::get_organization_handler;
pub use list_organizations::list_organizations_handler;
pub use update_organization::update_organization_handler;
