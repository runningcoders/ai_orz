//! SQLite SQL 常量定义

/// SQLite: Agent 表建表语句
/// 
/// 对应实体: [crate::models::agent::AgentPo]
/// 新增 `soul` 和 `capability` 字段存储 core memory
pub const SQLITE_CREATE_TABLE_AGENTS: &str = r#"
CREATE TABLE IF NOT EXISTS agents (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    soul TEXT NOT NULL DEFAULT '',
    capability TEXT NOT NULL DEFAULT '',
    model_provider_id TEXT NOT NULL,
    status INTEGER NOT NULL DEFAULT 1,
    created_by TEXT NOT NULL DEFAULT '',
    modified_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
)
"#;

/// SQLite: 模型提供商表建表语句
///
/// 对应实体: [crate::models::model_provider::ModelProviderPo]
pub const SQLITE_CREATE_TABLE_MODEL_PROVIDERS: &str = r#"
CREATE TABLE IF NOT EXISTS model_providers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    provider_type TEXT NOT NULL,
    model_name TEXT NOT NULL,
    api_key TEXT NOT NULL,
    base_url TEXT,
    description TEXT,
    status INTEGER NOT NULL DEFAULT 1,
    created_by TEXT NOT NULL DEFAULT '',
    modified_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
)
"#;

/// SQLite: Organization 表建表语句
///
/// 对应实体: [crate::models::organization::OrganizationPo]
pub const SQLITE_CREATE_TABLE_ORGANIZATIONS: &str = r#"
CREATE TABLE IF NOT EXISTS organizations (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status INTEGER NOT NULL DEFAULT 1,
    created_by TEXT NOT NULL DEFAULT '',
    modified_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
)
"#;

/// SQLite: Users 表建表语句
///
/// 对应实体: [crate::models::user::UserPo]
pub const SQLITE_CREATE_TABLE_USERS: &str = r#"
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    organization_id TEXT NOT NULL,
    username TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL DEFAULT '',
    email TEXT NOT NULL DEFAULT '',
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL,
    status INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    INDEX idx_organization_id (organization_id),
    INDEX idx_username (username)
)
"#;

/// SQLite: Task 表建表语句
///
/// 对应实体: [crate::models::task::Task]
pub const SQLITE_CREATE_TABLE_TASKS: &str = r#"
CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status INTEGER NOT NULL DEFAULT 1,
    created_by TEXT NOT NULL DEFAULT '',
    modified_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
)
"#;

/// SQLite: 短期记忆索引表建表语句
///
/// 对应实体: [crate::models::memory::ShortTermMemoryIndexPo]
pub const SQLITE_CREATE_TABLE_SHORT_TERM_MEMORY_INDEX: &str = r#"
CREATE TABLE IF NOT EXISTS short_term_memory_index (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    role TEXT NOT NULL,
    summary TEXT NOT NULL,
    tags TEXT NOT NULL,
    date_path TEXT NOT NULL,
    byte_start INTEGER NOT NULL,
    byte_length INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    INDEX idx_agent_id (agent_id),
    INDEX idx_created_at (created_at),
    INDEX idx_tags (tags),
    FULLTEXT INDEX idx_summary (summary)
)
"#;

/// SQLite: 长期知识图谱节点表建表语句
///
/// 对应实体: [crate::models::memory::LongTermKnowledgeNodePo]
pub const SQLITE_CREATE_TABLE_LONG_TERM_KNOWLEDGE_NODE: &str = r#"
CREATE TABLE IF NOT EXISTS long_term_knowledge_node (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    node_name TEXT NOT NULL,
    node_description TEXT NOT NULL,
    node_type TEXT NOT NULL,
    summary TEXT NOT NULL,
    relations TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    INDEX idx_agent_id (agent_id),
    INDEX idx_node_type (node_type),
    FULLTEXT INDEX idx_node_name (node_name),
    FULLTEXT INDEX idx_summary (summary)
)
"#;

/// SQLite: 知识节点引用原始短期记忆表建表语句
///
/// 对应实体: [crate::models::memory::KnowledgeReferencePo]
pub const SQLITE_CREATE_TABLE_KNOWLEDGE_REFERENCE: &str = r#"
CREATE TABLE IF NOT EXISTS knowledge_reference (
    id TEXT PRIMARY KEY,
    knowledge_id TEXT NOT NULL,
    short_term_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    INDEX idx_knowledge_id (knowledge_id),
    FOREIGN KEY(knowledge_id) REFERENCES long_term_knowledge_node(id),
    FOREIGN KEY(short_term_id) REFERENCES short_term_memory_index(id)
)
"#;
