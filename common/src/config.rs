//! 应用配置模块
//!
//! 默认配置在编译时嵌入二进制，首次运行自动解压生成配置文件，
//! 用户可通过修改外部配置文件自定义程序行为。

use crate::enums::SkillStatus;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 固定的基础数据根目录
/// 所有数据文件（SQLite数据库、日志、配置文件、记忆文件等）都存储在此目录下
pub const BASE_DATA_PATH: &str = ".ai_orz";

/// 环境变量名，用于覆盖默认的基础数据路径
pub const BASE_DATA_PATH_ENV: &str = "AI_ORZ_BASE_PATH";

/// 默认配置文件名（相对于 BASE_DATA_PATH）
pub const CONFIG_FILE_NAME: &str = "ai_orz.toml";

/// 应用整体配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    /// 服务器配置
    #[serde(default)]
    pub server: ServerConfig,

    /// 数据库配置
    #[serde(default)]
    pub database: DatabaseConfig,

    /// 统计数据库配置
    #[serde(default)]
    pub stats: StatsConfig,

    /// 前端配置
    #[serde(default)]
    pub frontend: FrontendConfig,

    /// 日志配置
    #[serde(default)]
    pub logging: LoggingConfig,

    /// JWT 配置
    #[serde(default)]
    pub jwt: JwtConfig,

    /// 消费者配置
    #[serde(default)]
    pub consumer: ConsumerConfig,

    /// 飞书配置
    #[serde(default)]
    pub lark: LarkConfig,
}

/// JWT 配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JwtConfig {
    /// JWT签名密钥（生产环境务必修改！也可以通过环境变量 JWT_SECRET 设置）
    pub secret: Option<String>,
    /// JWT默认过期时间（小时），默认 7 天（168小时），也可以通过环境变量 JWT_EXPIRY_HOURS 设置
    pub default_expiry_hours: Option<u32>,
}

impl Default for JwtConfig {
    fn default() -> Self {
        Self {
            secret: None,
            default_expiry_hours: None,
        }
    }
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

    /// 向量数据库文件名（相对于 base_data_path）
    #[serde(default = "default_vector_db_file_name")]
    pub vector_db_file_name: String,

    /// 向量存储后端类型
    #[serde(default)]
    pub vector_store_type: VectorStoreType,

    /// HNSW 索引持久化目录（相对于 base_data_path，仅使用 Hnsw 后端时生效）
    #[serde(default = "default_hnsw_index_dir")]
    pub hnsw_index_dir: String,
}

/// 向量存储后端类型
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VectorStoreType {
    /// LanceDB 嵌入式向量数据库（默认，高性能，生产级）
    #[default]
    LanceDb,

    /// 纯 Rust 内存向量存储（零系统依赖，用于测试）
    InMemory,

    /// HNSW 高性能近似最近邻索引
    Hnsw,

    /// SQLite VSS 向量扩展（需要系统依赖）
    SqliteVss,
}

/// 统计数据库配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StatsConfig {
    /// Stats DuckDB 文件路径（相对于 base_data_path）
    #[serde(default = "default_stats_db_file_name")]
    pub db_file_name: String,

    /// 批量写入缓冲大小
    #[serde(default = "default_stats_batch_size")]
    pub batch_size: usize,
}

impl Default for StatsConfig {
    fn default() -> Self {
        Self {
            db_file_name: default_stats_db_file_name(),
            batch_size: default_stats_batch_size(),
        }
    }
}

fn default_stats_db_file_name() -> String {
    "stats.duckdb".to_string()
}

fn default_stats_batch_size() -> usize {
    100
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
    /// 日志输出格式（"text" 或 "json"）
    #[serde(default = "default_log_format")]
    pub format: String,
    /// 日志保留天数（0 表示不清理）
    #[serde(default = "default_log_retention_days")]
    pub retention_days: u32,
}

fn default_log_format() -> String {
    "json".to_string() // 默认使用 JSON 格式，便于日志分析
}

fn default_log_retention_days() -> u32 {
    30 // 默认保留 30 天
}

fn default_db_file_name() -> String {
    "ai_orz.db".to_string()
}

fn default_vector_db_file_name() -> String {
    "ai_orz_vector.db".to_string()
}

fn default_hnsw_index_dir() -> String {
    "hnsw_index".to_string()
}

fn default_dist_dir() -> String {
    "dist".to_string()
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            db_file_name: default_db_file_name(),
            vector_db_file_name: default_vector_db_file_name(),
            vector_store_type: VectorStoreType::default(),
            hnsw_index_dir: default_hnsw_index_dir(),
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
            format: default_log_format(),
            retention_days: default_log_retention_days(),
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
    /// 获取基础数据路径
    pub fn base_data_path(&self) -> PathBuf {
        if let Ok(path) = std::env::var(BASE_DATA_PATH_ENV) {
            Path::new(&path).to_path_buf()
        } else {
            Path::new(BASE_DATA_PATH).to_path_buf()
        }
    }

    /// 获取完整的配置文件路径
    pub fn config_path(&self) -> PathBuf {
        self.base_data_path().join(CONFIG_FILE_NAME)
    }

    /// 获取完整的日志目录路径
    pub fn log_dir(&self) -> PathBuf {
        self.base_data_path().join(&self.logging.log_subdir)
    }

    /// 获取数据库文件路径
    pub fn db_path(&self) -> PathBuf {
        self.base_data_path().join(&self.database.db_file_name)
    }

    /// 获取向量数据库文件路径
    pub fn vector_db_path(&self) -> PathBuf {
        self.base_data_path()
            .join(&self.database.vector_db_file_name)
    }

    /// 获取 HNSW 索引持久化目录路径（仅 Hnsw 后端使用）
    pub fn hnsw_index_dir(&self) -> PathBuf {
        self.base_data_path().join(&self.database.hnsw_index_dir)
    }

    /// 获取产物/附件存储根目录路径（消息附件、Agent 生成文件等）
    /// 产物和附件统一存这里，不分开存储
    pub fn attachments_dir(&self) -> PathBuf {
        self.base_data_path().join("attachments")
    }

    /// 获取附件完整路径，传入相对路径
    pub fn attachment_path(&self, relative_path: &str) -> PathBuf {
        self.attachments_dir().join(relative_path)
    }

    /// 生成按日期分层的附件相对路径，用于存储到数据库
    pub fn generate_attachment_relative_path(&self, file_id: &str, extension: &str) -> String {
        let now = chrono::Utc::now();
        let date = now.format("%Y%m%d");
        format!("{}/{}{}", date, file_id, extension)
    }

    // ==================== 项目产物路径（Artifact）====================
    // 按项目组织: .ai_orz/artifacts/projects/{project_id}/{artifact_id}

    /// 项目产物存储根目录
    pub fn artifacts_dir(&self) -> PathBuf {
        self.base_data_path().join("artifacts")
    }

    /// 指定项目的产物目录
    pub fn artifact_project_dir(&self, project_id: &str) -> PathBuf {
        self.artifacts_dir().join("projects").join(project_id)
    }

    /// 生成产物相对路径（用于存储到数据库）
    pub fn generate_artifact_relative_path(&self, project_id: &str, artifact_id: &str) -> String {
        format!("projects/{}/{}", project_id, artifact_id)
    }

    /// 获取产物完整路径
    pub fn artifact_path(&self, project_id: &str, artifact_id: &str) -> PathBuf {
        self.artifact_project_dir(project_id).join(artifact_id)
    }

    /// 获取指定 Agent 的数据目录路径：base_data_path/agents/{agent_id}
    pub fn agent_data_dir(&self, agent_id: &str) -> PathBuf {
        self.base_data_path().join("agents").join(agent_id)
    }

    /// 获取指定 Agent 的记忆数据目录：base_data_path/agents/{agent_id}/memory
    pub fn agent_memory_dir(&self, agent_id: &str) -> PathBuf {
        self.agent_data_dir(agent_id).join("memory")
    }

    /// 获取所有技能的根目录（共享技能）
    pub fn skills_root_dir(&self) -> PathBuf {
        self.base_data_path().join("skills")
    }

    /// 获取 Agent 自有技能根目录
    pub fn agent_skills_root_dir(&self, agent_id: &str) -> PathBuf {
        self.agent_data_dir(agent_id).join("skills")
    }

    /// 获取 Agent 自有技能目录
    pub fn agent_skill_dir(&self, agent_id: &str, skill_id: &str) -> PathBuf {
        self.agent_skills_root_dir(agent_id).join(skill_id)
    }

    /// 获取 Agent 自有技能内容文件路径
    pub fn agent_skill_content_path(&self, agent_id: &str, skill_id: &str) -> PathBuf {
        self.agent_skill_dir(agent_id, skill_id).join("skill.md")
    }

    /// 获取 Agent 自有技能相对路径（相对于 base_data_path，用于存储到数据库）
    pub fn agent_skill_relative_path(&self, agent_id: &str, skill_id: &str) -> String {
        format!("agents/{}/skills/{}", agent_id, skill_id)
    }

    /// 获取共享技能目录
    pub fn shared_skill_dir(&self, skill_id: &str) -> PathBuf {
        self.skills_root_dir().join(skill_id)
    }

    /// 获取共享技能内容文件路径
    pub fn shared_skill_content_path(&self, skill_id: &str) -> PathBuf {
        self.shared_skill_dir(skill_id).join("skill.md")
    }

    /// 获取共享技能相对路径（相对于 base_data_path，用于存储到数据库）
    pub fn shared_skill_relative_path(&self, skill_id: &str) -> String {
        format!("skills/{}", skill_id)
    }

    /// 根据技能状态获取正确的内容文件绝对路径
    pub fn skill_content_path(
        &self,
        agent_id: &str,
        skill_id: &str,
        status: SkillStatus,
    ) -> PathBuf {
        match status {
            SkillStatus::Draft => self.agent_skill_content_path(agent_id, skill_id),
            SkillStatus::Published => self.shared_skill_content_path(skill_id),
            SkillStatus::Expired => self.shared_skill_content_path(skill_id),
        }
    }

    /// 根据技能状态获取正确的相对路径（存储到数据库）
    pub fn skill_relative_path(
        &self,
        agent_id: &str,
        skill_id: &str,
        status: SkillStatus,
    ) -> String {
        match status {
            SkillStatus::Draft => self.agent_skill_relative_path(agent_id, skill_id),
            SkillStatus::Published => self.shared_skill_relative_path(skill_id),
            SkillStatus::Expired => self.shared_skill_relative_path(skill_id),
        }
    }

    /// 获取指定工具的调用追踪日志目录
    /// 路径: {base_data_path}/tools/{tool_id}/call_trace
    pub fn tool_call_trace_dir(&self, tool_id: &str) -> PathBuf {
        self.base_data_path()
            .join("tools")
            .join(tool_id)
            .join("call_trace")
    }

    /// 获取指定工具的运行日志输出目录
    /// 路径: {base_data_path}/tools/{tool_id}/logs
    pub fn tool_logs_dir(&self, tool_id: &str) -> PathBuf {
        self.base_data_path()
            .join("tools")
            .join(tool_id)
            .join("logs")
    }
}

// ==================== 消费者配置 ====================

/// 消费者全局配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConsumerConfig {
    /// 全局默认并发数
    #[serde(default = "default_consumer_concurrency")]
    pub concurrency: usize,

    /// 全局默认空队列睡眠时间（ms）
    #[serde(default = "default_consumer_empty_sleep")]
    pub empty_queue_sleep_ms: u64,

    /// 全局默认错误重试睡眠时间（ms）
    #[serde(default = "default_consumer_error_sleep")]
    pub error_retry_sleep_ms: u64,

    /// Topic 专属配置（覆盖全局）
    #[serde(default)]
    pub topics: HashMap<String, TopicConsumerConfig>,
}

impl Default for ConsumerConfig {
    fn default() -> Self {
        Self {
            concurrency: default_consumer_concurrency(),
            empty_queue_sleep_ms: default_consumer_empty_sleep(),
            error_retry_sleep_ms: default_consumer_error_sleep(),
            topics: HashMap::default(),
        }
    }
}

/// Topic 专属消费者配置
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TopicConsumerConfig {
    /// 并发数（None 表示继承全局配置）
    pub concurrency: Option<usize>,
    /// 空队列睡眠时间（ms，None 表示继承全局配置）
    pub empty_queue_sleep_ms: Option<u64>,
    /// 错误重试睡眠时间（ms，None 表示继承全局配置）
    pub error_retry_sleep_ms: Option<u64>,
}

impl ConsumerConfig {
    /// 获取指定 topic 合并后的配置（topic 优先，全局兜底）
    pub fn for_topic(&self, topic: &str) -> TopicConsumerConfig {
        let topic_config = self.topics.get(topic).cloned().unwrap_or_default();
        TopicConsumerConfig {
            concurrency: topic_config.concurrency.or(Some(self.concurrency)),
            empty_queue_sleep_ms: topic_config
                .empty_queue_sleep_ms
                .or(Some(self.empty_queue_sleep_ms)),
            error_retry_sleep_ms: topic_config
                .error_retry_sleep_ms
                .or(Some(self.error_retry_sleep_ms)),
        }
    }
}

fn default_consumer_concurrency() -> usize {
    1
}

fn default_consumer_empty_sleep() -> u64 {
    100
}

fn default_consumer_error_sleep() -> u64 {
    1000
}

// ==================== 飞书配置 ====================

/// 飞书应用配置（全局共享，应用级凭证）
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LarkConfig {
    /// 是否启用飞书渠道
    #[serde(default)]
    pub enabled: bool,
    /// 飞书 App ID
    #[serde(default)]
    pub app_id: String,
    /// 飞书 App Secret
    #[serde(default)]
    pub app_secret: String,
    /// 飞书加密密钥（可选，事件订阅加密用）
    pub encrypt_key: Option<String>,
    /// 飞书验证令牌（可选，事件订阅校验用）
    pub verification_token: Option<String>,
}

impl Default for LarkConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            app_id: String::new(),
            app_secret: String::new(),
            encrypt_key: None,
            verification_token: None,
        }
    }
}
