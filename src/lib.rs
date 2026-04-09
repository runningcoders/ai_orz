//! ai_orz - AI 代理执行框架

use std::sync::OnceLock;

pub mod config;
pub mod error;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod pkg;
pub mod router;
pub mod service;

/// Global static reference to the loaded application config
///
/// This is initialized once at startup in main.rs and can be accessed
/// from anywhere in the application.
pub static APP_CONFIG: OnceLock<config::AppConfig> = OnceLock::new();
