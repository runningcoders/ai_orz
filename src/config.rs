//! 应用配置模块
//!
//! 默认配置在编译时嵌入二进制，首次运行自动解压生成配置文件，
//! 用户可通过修改外部配置文件自定义程序行为。

use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// 应用整体配置
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    /// 基础数据存储路径
    /// 所有数据文件（SQLite数据库、日志、记忆文件等）都基于此路径
    pub base_data_path: String,

    /// 服务器配置
    #[serde(default)]
    pub server: ServerConfig,

    /// 日志配置
    #[serde(default)]
    pub logging: LoggingConfig,
}

/// 服务器配置
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// 监听地址
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
}

/// 日志配置
#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    /// 是否启用文件日志
    #[serde(default = "default_enable_file_log")]
    pub enable_file_log: bool,
    /// 日志子目录（相对于 base_data_path）
    #[serde(default = "default_log_subdir")]
    pub log_subdir: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
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
        Path::new(&self.base_data_path).join("ai_orz.db")
    }
}

/// 加载应用配置
///
/// 逻辑：
/// 1. 如果当前目录存在 `ai_orz.toml`，直接读取解析
/// 2. 如果不存在，从编译嵌入的默认配置写出到文件，然后读取
pub fn load_config() -> Result<AppConfig, Box<dyn std::error::Error>> {
    const DEFAULT_CONFIG_EMBEDDED: &str = include_str!("../config/ai_orz.toml");
    const CONFIG_FILE_NAME: &str = "ai_orz.toml";

    let config_path = Path::new(CONFIG_FILE_NAME);

    // 如果配置文件不存在，写出默认配置
    if !config_path.exists() {
        fs::write(config_path, DEFAULT_CONFIG_EMBEDDED)?;
        println!("✅ Generated default config file: {}", CONFIG_FILE_NAME);
    }

    // 读取配置文件
    let content = fs::read_to_string(config_path)?;
    let config: AppConfig = toml::from_str(&content)?;

    // 确保基础数据目录存在
    let base_data_path = Path::new(&config.base_data_path);
    if !base_data_path.exists() {
        fs::create_dir_all(base_data_path)?;
        println!("✅ Created base data directory: {}", config.base_data_path);
    }

    // 确保日志目录存在
    let log_dir = config.log_dir();
    if !log_dir.exists() && config.logging.enable_file_log {
        fs::create_dir_all(&log_dir)?;
        println!("✅ Created log directory: {:?}", log_dir);
    }

    Ok(config)
}
