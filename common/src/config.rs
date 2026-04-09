//! 应用配置模块
//!
//! 默认配置在编译时嵌入二进制，首次运行自动解压生成配置文件，
//! 用户可通过修改外部配置文件自定义程序行为。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 默认配置文件内容（编译时嵌入二进制）
pub const DEFAULT_CONFIG_EMBEDDED: &str = include_str!("../config/ai_orz.toml");

/// 默认配置文件名
pub const CONFIG_FILE_NAME: &str = "ai_orz.toml";

/// 应用整体配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    /// 基础数据存储路径
    /// 所有数据文件（SQLite数据库、日志、记忆文件等）都基于此路径
    pub base_data_path: String,

    /// 数据库配置
    #[serde(default)]
    pub database: DatabaseConfig,

    /// 服务器配置
    #[serde(default)]
    pub server: ServerConfig,

    /// 前端配置
    #[serde(default)]
    pub frontend: FrontendConfig,

    /// 日志配置
    #[serde(default)]
    pub logging: LoggingConfig,
}

/// 服务器配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    /// 监听地址
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
}

/// 数据库配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    /// SQLite 数据库文件名（相对于 base_data_path）
    #[serde(default = "default_db_file_name")]
    pub db_file_name: String,
}

/// 前端配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FrontendConfig {
    /// 静态文件目录
    #[serde(default = "default_dist_dir")]
    pub dist_dir: String,
}

/// 日志配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingConfig {
    /// 是否启用文件日志
    #[serde(default = "default_enable_file_log")]
    pub enable_file_log: bool,
    /// 日志子目录（相对于 base_data_path）
    #[serde(default = "default_log_subdir")]
    pub log_subdir: String,
}

fn default_db_file_name() -> String {
    "ai_orz.db".to_string()
}

fn default_dist_dir() -> String {
    "dist".to_string()
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            db_file_name: default_db_file_name(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
        }
    }
}

impl Default for FrontendConfig {
    fn default() -> Self {
        Self {
            dist_dir: default_dist_dir(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            enable_file_log: default_enable_file_log(),
            log_subdir: default_log_subdir(),
        }
    }
}

fn default_listen_addr() -> String {
    "0.0.0.0:3000".to_string()
}

fn default_enable_file_log() -> bool {
    true
}

fn default_log_subdir() -> String {
    "logs".to_string()
}

impl AppConfig {
    /// 获取完整的日志目录路径
    pub fn log_dir(&self) -> PathBuf {
        Path::new(&self.base_data_path).join(&self.logging.log_subdir)
    }

    /// 获取数据库文件路径
    pub fn db_path(&self) -> PathBuf {
        Path::new(&self.base_data_path).join(&self.database.db_file_name)
    }
}
