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

    /// 产物存储配置（消息附件、Agent 生成的文件等都存在这里）
    #[serde(default)]
    pub artifact: ArtifactConfig,
}

/// 产物存储配置（消息附件、Agent 生成的文件等）
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ArtifactConfig {
    /// 产物存储子目录（相对于 base_data_path）
    /// 产物会按日期分层存储：YYYY/MM/DD/
    #[serde(default = "default_artifact_subdir")]
    pub artifact_subdir: String,
}

impl Default for ArtifactConfig {
    fn default() -> Self {
        Self {
            artifact_subdir: default_artifact_subdir(),
        }
    }
}

fn default_artifact_subdir() -> String {
    "attachments".to_string()
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

    /// 获取产物/附件存储根目录路径（消息附件、Agent 生成文件等）
    /// 产物和附件统一存这里，不分开存储
    pub fn attachments_dir(&self) -> PathBuf {
        Path::new(&self.base_data_path).join(&self.artifact.artifact_subdir)
    }

    /// 获取附件完整路径，传入相对路径
    pub fn attachment_path(&self, relative_path: &str) -> PathBuf {
        self.attachments_dir().join(relative_path)
    }

    /// 获取产物存储根目录路径（别名，底层复用 attachments 存储）
    pub fn artifacts_dir(&self) -> PathBuf {
        self.attachments_dir()
    }

    /// 获取产物完整路径，传入相对路径（别名，底层复用 attachments 存储）
    pub fn artifact_path(&self, relative_path: &str) -> PathBuf {
        self.attachment_path(relative_path)
    }

    /// 获取指定 Agent 的数据目录路径：base_data_path/agents/{agent_id}
    pub fn agent_data_dir(&self, agent_id: &str) -> PathBuf {
        Path::new(&self.base_data_path).join("agents").join(agent_id)
    }

    /// 获取指定 Agent 的记忆数据目录：base_data_path/agents/{agent_id}/memory
    pub fn agent_memory_dir(&self, agent_id: &str) -> PathBuf {
        self.agent_data_dir(agent_id).join("memory")
    }

    /// 生成按日期分层的相对路径：YYYYMMDD/{file_id}{ext}
    /// 用于附件和产物存储，按天分一层子目录，同一天的文件放在一起
    pub fn generate_date_relative_path(&self, file_id: &str, extension: &str) -> String {
        let now = chrono::Utc::now();
        let date = now.format("%Y%m%d");
        format!("{}/{}{}", date, file_id, extension)
    }
}
