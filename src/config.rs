//! 应用配置加载
//!
//! 默认配置在编译时嵌入二进制，首次运行自动解压生成配置文件，
//! 用户可通过修改外部配置文件自定义程序行为。

use common::config::{AppConfig, BASE_DATA_PATH, CONFIG_FILE_NAME};
use std::fs;
use std::path::Path;
use std::sync::{Arc, OnceLock};
// ==================== 单例管理 ====================

static CONFIG: OnceLock<Arc<AppConfig>> = OnceLock::new();

/// 获取 Agent DAL 单例
pub fn get() -> Arc<AppConfig> {
    CONFIG.get().cloned().unwrap()
}

/// 初始化 Agent DAL
pub fn init()  -> Result<(), Box<dyn std::error::Error>> {
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
pub fn load_config() -> Result<AppConfig, Box<dyn std::error::Error>> {
    // 确保基础数据目录存在
    let base_data_path = Path::new(BASE_DATA_PATH);
    if !base_data_path.exists() {
        fs::create_dir_all(base_data_path)?;
        println!("✅ Created base data directory: {}", BASE_DATA_PATH);
    }

    let config_path = base_data_path.join(CONFIG_FILE_NAME);

    // 如果配置文件不存在，写出默认配置
    if !config_path.exists() {
        fs::write(&config_path, DEFAULT_CONFIG_EMBEDDED)?;
        println!("✅ Generated default config file: {}/{}", BASE_DATA_PATH, CONFIG_FILE_NAME);
    }

    // 读取配置文件
    let content = fs::read_to_string(&config_path)?;
    let config: AppConfig = toml::from_str(&content)?;

    // 确保日志目录存在
    let log_dir = config.log_dir();
    if !log_dir.exists() && config.logging.enable_file_log {
        fs::create_dir_all(&log_dir)?;
        println!("✅ Created log directory: {:?}", log_dir);
    }

    Ok(config)
}

/// 默认配置内容（编译时嵌入二进制）
pub const DEFAULT_CONFIG_EMBEDDED: &str = include_str!("../common/config/ai_orz.toml");
