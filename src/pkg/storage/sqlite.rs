use rusqlite::Connection;
use std::path::Path;

/// 数据库配置
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub db_type: String,
    pub path: String,
}

impl DatabaseConfig {
    pub fn from_toml(table: &toml::Table) -> Result<Self, String> {
        let db = table.get("database")
            .ok_or("missing [database] section")?
            .as_table()
            .ok_or("[database] must be a table")?;

        Ok(Self {
            db_type: db.get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("sqlite")
                .to_string(),
            path: db.get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("data/ai_orz.db")
                .to_string(),
        })
    }
}

/// SQLite 连接持有者
pub struct Database {
    pub conn: Connection,
}

impl Database {
    /// 初始化数据库
    pub fn init(config: DatabaseConfig) -> Result<Self, String> {
        // 确保目录存在
        if let Some(parent) = Path::new(&config.path).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create data dir failed: {}", e))?;
        }

        // 创建连接
        let conn = Connection::open(&config.path)
            .map_err(|e| format!("open database failed: {}", e))?;

        // 初始化表结构
        init_tables(&conn)?;

        Ok(Self { conn })
    }
}

/// 初始化表结构
fn init_tables(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS agents (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT '',
            capabilities TEXT NOT NULL DEFAULT '[]',
            status TEXT NOT NULL DEFAULT 'idle',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS organizations (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS tasks (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            org_id TEXT NOT NULL,
            assigned_to TEXT,
            status TEXT NOT NULL DEFAULT 'pending',
            priority INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            from_agent_id TEXT NOT NULL,
            to_agent_id TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );
        "#,
    )
    .map_err(|e| format!("init tables failed: {}", e))
}

/// 从 toml 配置文件加载并初始化数据库
pub fn init_from_config(config_path: &str) -> Result<Database, String> {
    let content = std::fs::read_to_string(config_path)
        .map_err(|e| format!("read config failed: {}", e))?;

    let config: toml::Table = content.parse()
        .map_err(|e| format!("parse toml failed: {}", e))?;

    let db_config = DatabaseConfig::from_toml(&config)?;

    Database::init(db_config)
}
