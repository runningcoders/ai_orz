//! Public handlers (no authentication required)
//!
//! These endpoints are accessible without login:
//! - health check
//! - system initialization
//! - login/logout
//! - get public config for frontend

pub mod config;

pub use self::config::*;
