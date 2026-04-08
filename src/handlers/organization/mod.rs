//! Organization 组织管理 HTTP 接口
//!
//! 包含：
//! - 系统初始化接口
//! - 组织管理 CRUD 接口
//! - 用户管理 CRUD 接口

pub mod create_user;
pub mod delete_organization;
pub mod delete_user;
pub mod get_organization;
pub mod get_user_by_username;
pub mod initialize_system;
pub mod list_organizations;
pub mod list_users_by_organization;
pub mod update_organization;
pub mod update_user;
pub mod create_organization;

pub use create_user::{create_user, CreateUserRequest, CreateUserResponse};
pub use delete_organization::{delete_organization, DeleteOrganizationRequest, DeleteOrganizationResponse};
pub use delete_user::{delete_user, DeleteUserRequest, DeleteUserResponse};
pub use get_organization::{get_organization, GetOrganizationRequest, GetOrganizationResponse};
pub use get_user_by_username::{get_user_by_username, GetUserByUsernameResponse};
pub use initialize_system::{initialize_system, InitializeSystemRequest, InitializeSystemResponse};
pub use list_organizations::{list_organizations, ListOrganizationsResponse};
pub use list_users_by_organization::{list_users_by_organization, ListUsersByOrganizationResponse};
pub use update_organization::{update_organization, UpdateOrganizationRequest, UpdateOrganizationResponse};
pub use update_user::{update_user, UpdateUserRequest, UpdateUserResponse};
