//! 认证相关接口：登录、登出、邀请码注册

pub mod login;
pub mod logout;
pub mod register;

pub use register::{register_by_invite, validate_invite_code};
