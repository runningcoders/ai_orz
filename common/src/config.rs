//! 应用配置模块
//!
//! 默认配置在编译时嵌入二进制，首次运行自动解压生成配置文件，
//! 用户可通过修改外部配置文件自定义程序行为。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 固定的基础数据根目录
/// 所有数据文件（SQLite数据库、日志、配置文件、记忆文件等）都存储在此目录下
pub const BASE_DATA_PATH: &str = ".ai_orz";

/// 环境变量名，用于覆盖默认的基础数据路径
pub const BASE_DATA_PATH_ENV: &str = "AI_ORZ_BASE_PATH";

/// 环境变量名，用于覆盖监听地址（与配置字段 server.listen_addr 完全对齐）
pub const LISTEN_ADDR_ENV: &str = "AI_ORZ_LISTEN_ADDR";

/// 环境变量名，用于覆盖敏感数据加密密钥（映射 security.secret_key）
pub const SECRET_KEY_ENV: &str = "SECRET_KEY";

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

    /// 安全配置（敏感数据加密密钥）
    #[serde(default)]
    pub security: SecurityConfig,

    /// A2A Server 配置
    #[serde(default)]
    pub a2a_server: A2aServerConfig,

    /// Agent 运行时默认配置（系统级，可被 Agent 实体的 runtime_config 覆盖）
    #[serde(default)]
    pub agent: AgentConfig,

    /// 工具运行日志配置（shell_exec 等工具的运行时输出清理策略）
    #[serde(default)]
    pub tool_log: ToolLogConfig,
}

/// Agent 运行时配置（系统级默认值）
///
/// 单个 Agent 可通过 `agents.runtime_config` JSON 覆盖任意字段。
/// 约定：Agent 级配置值为 0 时，回退到此处系统默认值。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentConfig {
    /// 单次唤醒最大思考轮次（跨压缩累计），默认 120
    #[serde(default = "default_agent_max_thinking_rounds")]
    pub max_thinking_rounds: usize,

    /// 意图识别阶段最大思考轮次，默认 10
    #[serde(default = "default_agent_intent_analyze_max_rounds")]
    pub intent_analyze_max_rounds: usize,

    /// 总结退出阶段最大思考轮次，默认 20
    #[serde(default = "default_agent_summary_max_rounds")]
    pub summary_max_rounds: usize,

    /// 思考超时（秒），0 = 不限制，默认 0
    #[serde(default)]
    pub think_timeout_secs: u64,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_thinking_rounds: 365,
            intent_analyze_max_rounds: 365,
            summary_max_rounds: 365,
            think_timeout_secs: 0,
        }
    }
}

fn default_agent_max_thinking_rounds() -> usize {
    365
}

fn default_agent_intent_analyze_max_rounds() -> usize {
    365
}

fn default_agent_summary_max_rounds() -> usize {
    365
}

/// JWT 配置
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct JwtConfig {
    /// JWT签名密钥（生产环境务必修改！也可以通过环境变量 JWT_SECRET 设置）
    pub secret: Option<String>,
    /// JWT默认过期时间（小时），默认 7 天（168小时），也可以通过环境变量 JWT_EXPIRY_HOURS 设置
    pub default_expiry_hours: Option<u32>,
}

/// 服务器配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    /// 监听地址
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,

    /// 系统时区（IANA 时区名，用于 cron 表达式解析等时间相关功能）
    #[serde(default = "default_timezone")]
    pub timezone: String,
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

/// 工具运行日志配置（shell_exec 等工具的 ① 运行时输出，见 docs/design/tool_output_boundary_design.md）
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolLogConfig {
    /// 工具日志保留天数（0 = 不清理），默认 30
    #[serde(default = "default_tool_log_retention_days")]
    pub retention_days: u32,
}

impl Default for ToolLogConfig {
    fn default() -> Self {
        Self {
            retention_days: default_tool_log_retention_days(),
        }
    }
}

fn default_tool_log_retention_days() -> u32 {
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
            timezone: default_timezone(),
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

fn default_timezone() -> String {
    "Asia/Shanghai".to_string()
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

// ==================== 安全配置 ====================

/// 安全配置（数据库敏感字段加密密钥）
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SecurityConfig {
    /// 主加密密钥（用于渠道凭证等敏感字段落库加密，启动时校验非空）
    #[serde(default)]
    pub secret_key: String,
}

/// A2A Server 配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct A2aServerConfig {
    /// 是否启用 A2A Server
    #[serde(default)]
    pub enabled: bool,
    /// 协议版本
    #[serde(default = "default_a2a_protocol_version")]
    pub protocol_version: String,
    /// JSON-RPC 端点路径
    #[serde(default = "default_a2a_endpoint")]
    pub endpoint: String,
    /// Agent Card 路径
    #[serde(default = "default_a2a_card_path")]
    pub card_path: String,
}

impl Default for A2aServerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            protocol_version: default_a2a_protocol_version(),
            endpoint: default_a2a_endpoint(),
            card_path: default_a2a_card_path(),
        }
    }
}

fn default_a2a_protocol_version() -> String {
    "0.3.0".to_string()
}

fn default_a2a_endpoint() -> String {
    "/a2a".to_string()
}

fn default_a2a_card_path() -> String {
    "/.well-known/agent.json".to_string()
}
