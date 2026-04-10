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
    scope INTEGER NOT NULL DEFAULT 0,
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
    updated_at INTEGER NOT NULL
)
"#;

/// SQLite: Organizations 表索引
pub const SQLITE_CREATE_INDEX_ORGANIZATIONS_ID: &str = r#"
CREATE INDEX IF NOT EXISTS idx_organizations_id ON organizations(id)
"#;

/// SQLite: Users 表索引
pub const SQLITE_CREATE_INDEX_USERS_ID: &str = r#"
CREATE INDEX IF NOT EXISTS idx_users_id ON users(id)
"#;

/// SQLite: Users 表索引
pub const SQLITE_CREATE_INDEX_USERS_ORGANIZATION_ID: &str = r#"
CREATE INDEX IF NOT EXISTS idx_users_organization_id ON users(organization_id)
"#;

/// SQLite: Users 表索引
pub const SQLITE_CREATE_INDEX_USERS_USERNAME: &str = r#"
CREATE INDEX IF NOT EXISTS idx_users_username ON users(username)
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
    updated_at INTEGER NOT NULL
)
"#;

/// SQLite: 短期记忆索引索引
pub const SQLITE_CREATE_INDEX_SHORT_TERM_AGENT_ID: &str = r#"
CREATE INDEX IF NOT EXISTS idx_stmi_agent_id ON short_term_memory_index(agent_id)
"#;

/// SQLite: 短期记忆索引索引
pub const SQLITE_CREATE_INDEX_SHORT_TERM_CREATED_AT: &str = r#"
CREATE INDEX IF NOT EXISTS idx_stmi_created_at ON short_term_memory_index(created_at)
"#;

/// SQLite: 短期记忆索引索引
pub const SQLITE_CREATE_INDEX_SHORT_TERM_TAGS: &str = r#"
CREATE INDEX IF NOT EXISTS idx_stmi_tags ON short_term_memory_index(tags)
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
    updated_at INTEGER NOT NULL
)
"#;

/// SQLite: 长期知识节点索引
pub const SQLITE_CREATE_INDEX_LTKN_AGENT_ID: &str = r#"
CREATE INDEX IF NOT EXISTS idx_ltkn_agent_id ON long_term_knowledge_node(agent_id)
"#;

/// SQLite: 长期知识节点索引
pub const SQLITE_CREATE_INDEX_LTKN_NODE_TYPE: &str = r#"
CREATE INDEX IF NOT EXISTS idx_ltkn_node_type ON long_term_knowledge_node(node_type)
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
    updated_at INTEGER NOT NULL
)
"#;

/// SQLite: 知识节点关系索引
pub const SQLITE_CREATE_INDEX_KNR_SOURCE_NODE_ID: &str = r#"
CREATE INDEX IF NOT EXISTS idx_knr_source_node_id ON knowledge_node_relation(source_node_id)
"#;

/// SQLite: 知识节点关系索引
pub const SQLITE_CREATE_INDEX_KNR_TARGET_NODE_ID: &str = r#"
CREATE INDEX IF NOT EXISTS idx_knr_target_node_id ON knowledge_node_relation(target_node_id)
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
    created_at INTEGER NOT NULL
)
"#;

/// SQLite: 知识引用索引
pub const SQLITE_CREATE_INDEX_KR_KNOWLEDGE_ID: &str = r#"
CREATE INDEX IF NOT EXISTS idx_kr_knowledge_id ON knowledge_reference(knowledge_id)
"#;

/// SQLite: 知识引用索引
pub const SQLITE_CREATE_INDEX_KR_SHORT_TERM_ID: &str = r#"
CREATE INDEX IF NOT EXISTS idx_kr_short_term_id ON knowledge_reference(short_term_id)
"#;

/// SQLite: 知识引用索引
pub const SQLITE_CREATE_INDEX_KR_TRACE_ID: &str = r#"
CREATE INDEX IF NOT EXISTS idx_kr_trace_id ON knowledge_reference(trace_id)
"#;

/// SQLite: Messages 消息表建表语句
///
/// 对应实体: [crate::models::message::MessagePo]
/// 存储所有消息记录，包括用户-Agent 聊天和 Agent 之间的消息传递
/// 文本消息直接存储内容，附件消息存储元数据+文件路径
pub const SQLITE_CREATE_TABLE_MESSAGES: &str = r#"
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    from_id TEXT NOT NULL,
    to_id TEXT NOT NULL,
    role INTEGER NOT NULL DEFAULT 0,
    message_type INTEGER NOT NULL DEFAULT 0,
    status INTEGER NOT NULL DEFAULT 0,
    content TEXT NOT NULL,
    meta_json TEXT NOT NULL DEFAULT '',
    created_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
)
"#;

/// SQLite: Messages 表索引
pub const SQLITE_CREATE_INDEX_MESSAGES_TASK_ID: &str = r#"
CREATE INDEX IF NOT EXISTS idx_messages_task_id ON messages(task_id)
"#;

/// SQLite: Messages 表索引
pub const SQLITE_CREATE_INDEX_MESSAGES_FROM_ID: &str = r#"
CREATE INDEX IF NOT EXISTS idx_messages_from_id ON messages(from_id)
"#;

/// SQLite: Messages 表索引
pub const SQLITE_CREATE_INDEX_MESSAGES_TO_ID: &str = r#"
CREATE INDEX IF NOT EXISTS idx_messages_to_id ON messages(to_id)
"#;

/// SQLite: Messages 表索引
pub const SQLITE_CREATE_INDEX_MESSAGES_CREATED_AT: &str = r#"
CREATE INDEX IF NOT EXISTS idx_messages_created_at ON messages(created_at)
"#;
