//! 应用配置加载
//!
//! 默认配置在编译时嵌入二进制，首次运行自动解压生成配置文件，
//! 用户可通过修改外部配置文件自定义程序行为。

use common::config::{AppConfig, BASE_DATA_PATH, CONFIG_FILE_NAME};
use common::error::Result;
use std::sync::{Arc, OnceLock};
// ==================== 单例管理 ====================

static CONFIG: OnceLock<Arc<AppConfig>> = OnceLock::new();

/// 获取 Agent DAL 单例
pub fn get() -> Arc<AppConfig> {
    CONFIG.get().cloned().unwrap()
}

/// 尝试获取配置单例（未初始化时返回 None）
///
/// 用于测试环境或可选功能初始化场景，避免 panic。
pub fn try_get() -> Option<Arc<AppConfig>> {
    CONFIG.get().cloned()
}

/// 初始化 Agent DAL
pub fn init() -> Result<()> {
    // 加载配置（默认配置嵌入在二进制中，不存在就自动生成）
    let _ = CONFIG.set(Arc::new(load_config()?));
    Ok(())
}

/// 加载应用配置
///
/// 逻辑：
/// 1. `.ai_orz` 是固定的基础数据目录，永远不变
/// 2. 如果 `.ai_orz/ai_orz.toml` 不存在，从编译嵌入的默认配置写出到文件
/// 3. 读取解析配置文件，确保所有需要的目录都存在
pub fn load_config() -> Result<AppConfig> {
    // 先从环境变量获取 base_data_path
    let base_data_path = if let Ok(path) = std::env::var(common::config::BASE_DATA_PATH_ENV) {
        std::path::PathBuf::from(path)
    } else {
        std::path::PathBuf::from(BASE_DATA_PATH)
    };

    // 确保基础数据目录存在
    if !base_data_path.exists() {
        std::fs::create_dir_all(&base_data_path)?;
        println!("✅ Created base data directory: {:?}", base_data_path);
    }

    let config_path = base_data_path.join(CONFIG_FILE_NAME);

    // 如果配置文件不存在，写出默认配置
    if !config_path.exists() {
        std::fs::write(&config_path, DEFAULT_CONFIG_EMBEDDED)?;
        println!("✅ Generated default config file: {:?}", config_path);
    }

    // 读取配置文件
    let content = std::fs::read_to_string(&config_path)?;
    let mut config: AppConfig = toml::from_str(&content).map_err(|e: toml::de::Error| {
        common::error::Error::new(common::error::ErrorCode::ConfigInvalid, e.to_string())
    })?;

    // 安全配置：主加密密钥不可为空（渠道凭证等敏感字段落库加密依赖它）
    // 存量配置文件可能无 [security] 段，首次启动自动补写默认密钥并持久化
    if config.security.secret_key.trim().is_empty() {
        config.security.secret_key = "ai-orz-default-secret-key-change-me".to_string();
        match toml::to_string_pretty(&config) {
            Ok(serialized) => {
                let _ = std::fs::write(&config_path, serialized);
            }
            Err(e) => {
                println!(
                    "⚠️ Failed to persist auto-generated security.secret_key: {}",
                    e
                );
            }
        }
        println!(
            "⚠️ security.secret_key 缺失，已自动生成默认密钥并写入配置文件，生产环境请修改 [security] secret_key"
        );
    }

    // 确保日志目录存在
    let log_dir = config.log_dir();
    if !log_dir.exists() && config.logging.enable_file_log {
        std::fs::create_dir_all(&log_dir)?;
        println!("✅ Created log directory: {:?}", log_dir);
    }

    Ok(config)
}

/// 默认配置内容（编译时嵌入二进制）
pub const DEFAULT_CONFIG_EMBEDDED: &str = include_str!("../common/config/ai_orz.toml");
