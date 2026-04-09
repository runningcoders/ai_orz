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
    base_url TEXT NOT NULL DEFAULT '',
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
/// 原始记忆细节通过 knowledge_reference.short_term_id 反向关联
/// 原始记忆细节位置信息存储在 knowledge_reference 表中
pub const SQLITE_CREATE_TABLE_SHORT_TERM_MEMORY_INDEX: &str = r#"
CREATE TABLE IF NOT EXISTS short_term_memory_index (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    role TEXT NOT NULL,
    summary TEXT NOT NULL,
    tags TEXT NOT NULL,
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
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    INDEX idx_agent_id (agent_id),
    INDEX idx_node_type (node_type),
    FULLTEXT INDEX idx_node_name (node_name),
    FULLTEXT INDEX idx_summary (summary)
)
"#;

/// SQLite: 知识节点关系表建表语句
///
/// 对应实体: [crate::models::memory::KnowledgeNodeRelationPo]
/// 专门存储知识节点之间的关系，独立表方便查询和维护
pub const SQLITE_CREATE_TABLE_KNOWLEDGE_NODE_RELATION: &str = r#"
CREATE TABLE IF NOT EXISTS knowledge_node_relation (
    id TEXT PRIMARY KEY,
    source_node_id TEXT NOT NULL,
    target_node_id TEXT NOT NULL,
    relation_type TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    INDEX idx_source_node_id (source_node_id),
    INDEX idx_target_node_id (target_node_id),
    FOREIGN KEY(source_node_id) REFERENCES long_term_knowledge_node(id),
    FOREIGN KEY(target_node_id) REFERENCES long_term_knowledge_node(id)
)
"#;

/// SQLite: 知识引用原始记忆细节表建表语句
///
/// 对应实体: [crate::models::memory::KnowledgeReferencePo]
/// 每条原始记忆细节单独一条引用记录，存储位置信息完整可追溯
pub const SQLITE_CREATE_TABLE_KNOWLEDGE_REFERENCE: &str = r#"
CREATE TABLE IF NOT EXISTS knowledge_reference (
    id TEXT PRIMARY KEY,
    knowledge_id TEXT NOT NULL,
    short_term_id TEXT NOT NULL,
    trace_id TEXT NOT NULL,
    date_path TEXT NOT NULL,
    byte_start INTEGER NOT NULL,
    byte_length INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    INDEX idx_knowledge_id (knowledge_id),
    INDEX idx_short_term_id (short_term_id),
    INDEX idx_trace_id (trace_id),
    FOREIGN KEY(knowledge_id) REFERENCES long_term_knowledge_node(id),
    FOREIGN KEY(short_term_id) REFERENCES short_term_memory_index(id)
)
"#;
