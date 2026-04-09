//! Organization (组织管理) HTTP 接口
//!
//! 包含：
//! - 系统初始化接口
//! - organization (组织管理)
//! - user (用户管理)
//! - auth (登录/登出)
//! - organization_me (当前用户所在组织信息管理)

pub mod auth;
pub mod initialize_system;
pub mod organization;
pub mod organization_me;
pub mod user;

