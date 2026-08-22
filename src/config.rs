//! 应用配置加载
//!
//! 默认配置在编译时嵌入二进制，首次运行自动解压生成配置文件，
//! 用户可通过修改外部配置文件自定义程序行为。

use common::config::{AppConfig, BASE_DATA_PATH, CONFIG_FILE_NAME};
use common::error::Result;
use std::path::Path;
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
/// 4. 首次初始化时，若设置了对应环境变量，把用户偏好固化进配置文件（后续环境变量仅内存覆盖）
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
    // 首次初始化标记：本次启动前配置文件不存在（用于把环境变量偏好固化进文件）
    let config_created = !config_path.exists();

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

    // 环境变量 AI_ORZ_LISTEN_ADDR 覆盖监听地址（与配置字段 server.listen_addr 完全对齐，无需转换）
    if let Some(addr) = std::env::var(common::config::LISTEN_ADDR_ENV)
        .ok()
        .filter(|v| !v.trim().is_empty())
    {
        config.server.listen_addr = validate_listen_addr(addr.trim())?;
    }

    // 环境变量 SECRET_KEY 覆盖敏感数据加密密钥（数据库敏感字段加密）
    if let Some(key) = std::env::var(common::config::SECRET_KEY_ENV)
        .ok()
        .filter(|v| !v.trim().is_empty())
    {
        config.security.secret_key = key.trim().to_string();
    }

    // 首次初始化：把用户通过环境变量指定的偏好固化进配置文件（定向替换，保留模板注释）
    persist_first_init_env_prefs(config_created, &config_path);

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

/// 校验监听地址（host:port，host 可为 IP/主机名/IPv6），返回原样字符串
fn validate_listen_addr(addr: &str) -> Result<String> {
    let port = addr.rsplit_once(':').map(|(_, p)| p).unwrap_or("");
    let valid =
        !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) && port.parse::<u16>().is_ok();
    if !valid {
        return Err(common::error::Error::new(
            common::error::ErrorCode::ConfigInvalid,
            format!(
                "{} 必须是合法监听地址(host:port)，当前值: {addr}",
                common::config::LISTEN_ADDR_ENV
            ),
        ));
    }
    Ok(addr.to_string())
}

/// 在配置文本中定位 `key = ` 或 `# key = ` 行并替换为 new_line（保留首个匹配行的缩进）
fn upsert_config_line(content: &mut String, key: &str, new_line: &str) -> bool {
    let needle = format!("{key} = ");
    let commented = format!("# {key} = ");
    let mut replaced = false;
    let mut out = String::with_capacity(content.len() + new_line.len());
    for line in content.lines() {
        let ls = line.trim_start();
        if !replaced && (ls.starts_with(&needle) || ls.starts_with(&commented)) {
            let indent = &line[..line.len() - ls.len()];
            out.push_str(indent);
            out.push_str(new_line);
            replaced = true;
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    if replaced {
        *content = out;
    }
    replaced
}

/// TOML 基础字符串转义（防用户值含引号/反斜杠破坏配置）
fn escape_toml_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// 首次初始化时，把用户通过环境变量指定的偏好固化进生成的配置文件。
/// 只在 config_created（本次启动首次生成配置）时执行；定向替换具体行，保留模板注释。
fn persist_first_init_env_prefs(config_created: bool, config_path: &Path) {
    if !config_created {
        return;
    }
    let Ok(mut content) = std::fs::read_to_string(config_path) else {
        return;
    };
    let mut changed = false;

    // server.listen_addr ← AI_ORZ_LISTEN_ADDR
    if let Some(addr) = std::env::var(common::config::LISTEN_ADDR_ENV)
        .ok()
        .filter(|v| !v.trim().is_empty())
    {
        changed |= upsert_config_line(
            &mut content,
            "listen_addr",
            &format!("listen_addr = \"{}\"", escape_toml_string(addr.trim())),
        );
    }
    // jwt.secret ← JWT_SECRET
    if let Some(secret) = std::env::var("JWT_SECRET")
        .ok()
        .filter(|v| !v.trim().is_empty())
    {
        changed |= upsert_config_line(
            &mut content,
            "secret",
            &format!("secret = \"{}\"", escape_toml_string(secret.trim())),
        );
    }
    // jwt.default_expiry_hours ← JWT_EXPIRY_HOURS
    if let Some(n) = std::env::var("JWT_EXPIRY_HOURS")
        .ok()
        .and_then(|v| v.trim().parse::<u32>().ok())
    {
        changed |= upsert_config_line(
            &mut content,
            "default_expiry_hours",
            &format!("default_expiry_hours = {n}"),
        );
    }
    // frontend.dist_dir ← FRONTEND_DIST_DIR
    if let Some(dir) = std::env::var("FRONTEND_DIST_DIR")
        .ok()
        .filter(|v| !v.trim().is_empty())
    {
        changed |= upsert_config_line(
            &mut content,
            "dist_dir",
            &format!("dist_dir = \"{}\"", escape_toml_string(dir.trim())),
        );
    }
    // security.secret_key ← SECRET_KEY
    if let Some(key) = std::env::var(common::config::SECRET_KEY_ENV)
        .ok()
        .filter(|v| !v.trim().is_empty())
    {
        changed |= upsert_config_line(
            &mut content,
            "secret_key",
            &format!("secret_key = \"{}\"", escape_toml_string(key.trim())),
        );
    }

    if changed {
        match std::fs::write(config_path, content) {
            Ok(()) => println!("✅ 首次初始化：已把环境变量偏好固化进配置文件"),
            Err(e) => println!("⚠️ 首次初始化环境变量固化写入失败: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listen_addr_override_accepts_valid() {
        assert_eq!(
            validate_listen_addr("0.0.0.0:8080").unwrap(),
            "0.0.0.0:8080"
        );
        assert_eq!(
            validate_listen_addr("127.0.0.1:8080").unwrap(),
            "127.0.0.1:8080"
        );
        assert_eq!(
            validate_listen_addr("localhost:8080").unwrap(),
            "localhost:8080"
        );
        assert_eq!(validate_listen_addr("[::1]:8080").unwrap(), "[::1]:8080");
    }

    #[test]
    fn listen_addr_override_rejects_invalid() {
        assert!(validate_listen_addr("8080").is_err());
        assert!(validate_listen_addr("0.0.0.0:notaport").is_err());
        assert!(validate_listen_addr("0.0.0.0:99999").is_err());
        assert!(validate_listen_addr("").is_err());
    }

    #[test]
    fn upsert_replaces_active_and_commented_line() {
        let tpl = "[server]\nlisten_addr = \"0.0.0.0:3000\"\n\
                   [jwt]\n# secret = \"x\"\n# default_expiry_hours = 168\n\
                   [frontend]\ndist_dir = \"dist\"\n\
                   [security]\nsecret_key = \"default\"\n";
        let mut c = tpl.to_string();
        assert!(upsert_config_line(
            &mut c,
            "listen_addr",
            "listen_addr = \"0.0.0.0:8080\""
        ));
        assert!(upsert_config_line(&mut c, "secret", "secret = \"mykey\""));
        assert!(upsert_config_line(
            &mut c,
            "default_expiry_hours",
            "default_expiry_hours = 72"
        ));
        assert!(upsert_config_line(&mut c, "dist_dir", "dist_dir = \"web\""));
        assert!(upsert_config_line(
            &mut c,
            "secret_key",
            "secret_key = \"mysec\""
        ));
        assert!(c.contains("listen_addr = \"0.0.0.0:8080\""));
        assert!(c.contains("secret = \"mykey\""));
        assert!(c.contains("default_expiry_hours = 72"));
        assert!(c.contains("dist_dir = \"web\""));
        assert!(c.contains("secret_key = \"mysec\""));
        assert!(!c.contains("0.0.0.0:3000\""));
        assert!(!c.contains("# secret = \"x\""));
        assert!(!c.contains("secret_key = \"default\""));
    }

    #[test]
    fn upsert_ignores_unknown_key() {
        let tpl = "[server]\nlisten_addr = \"0.0.0.0:3000\"\n";
        let mut c = tpl.to_string();
        assert!(!upsert_config_line(&mut c, "nonexistent", "x = 1"));
        assert_eq!(c, tpl);
    }

    #[test]
    fn escape_toml_string_escapes_quotes_and_backslash() {
        assert_eq!(escape_toml_string("a\"b\\c"), "a\\\"b\\\\c");
    }
}
