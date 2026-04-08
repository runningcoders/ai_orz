//! Organization (组织管理) HTTP 接口
//!
//! 包含：
//! - 系统初始化接口
//! - organization (组织管理)
//! - user (用户管理)

pub mod initialize_system;
pub mod organization;
pub mod user;

pub use initialize_system::initialize_system;
pub use organization::*;
pub use user::*;
