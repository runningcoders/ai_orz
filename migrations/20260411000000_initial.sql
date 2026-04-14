CREATE TABLE IF NOT EXISTS agents (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    soul TEXT NOT NULL DEFAULT '',
    capabilities TEXT NOT NULL DEFAULT '',
    model_provider_id TEXT NOT NULL,
    status INTEGER NOT NULL DEFAULT 1,
    created_by TEXT NOT NULL DEFAULT '',
    modified_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS model_providers (
    id TEXT NOT NULL PRIMARY KEY,
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
) STRICT;

CREATE TABLE IF NOT EXISTS organizations (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    base_url TEXT NOT NULL DEFAULT '',
    status INTEGER NOT NULL DEFAULT 1,
    scope INTEGER NOT NULL DEFAULT 0,
    created_by TEXT NOT NULL DEFAULT '',
    modified_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS users (
    id TEXT NOT NULL PRIMARY KEY,
    organization_id TEXT NOT NULL,
    username TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL DEFAULT '',
    email TEXT NOT NULL DEFAULT '',
    password_hash TEXT NOT NULL,
    role INTEGER NOT NULL,
    status INTEGER NOT NULL DEFAULT 1,
    created_by TEXT NOT NULL DEFAULT '',
    modified_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT NOT NULL PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status INTEGER NOT NULL DEFAULT 1,
    priority INTEGER NOT NULL DEFAULT 0,
    tags TEXT NOT NULL DEFAULT '[]',
    due_at INTEGER,
    assignee_type INTEGER NOT NULL DEFAULT 1,
    assignee_id TEXT NOT NULL,
    project_id TEXT,
    created_by TEXT NOT NULL DEFAULT '',
    modified_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS short_term_memory_index (
    id TEXT NOT NULL PRIMARY KEY,
    agent_id TEXT NOT NULL,
    task_id TEXT,
    role TEXT NOT NULL,
    summary TEXT NOT NULL,
    tags TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS long_term_knowledge_node (
    id TEXT NOT NULL PRIMARY KEY,
    agent_id TEXT NOT NULL,
    node_name TEXT NOT NULL,
    node_description TEXT NOT NULL,
    node_type TEXT NOT NULL,
    summary TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS knowledge_node_relation (
    id TEXT NOT NULL PRIMARY KEY,
    source_node_id TEXT NOT NULL,
    target_node_id TEXT NOT NULL,
    relation_type TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS knowledge_reference (
    id TEXT NOT NULL PRIMARY KEY,
    knowledge_id TEXT NOT NULL,
    short_term_id TEXT NOT NULL,
    trace_id TEXT NOT NULL,
    date_path TEXT NOT NULL,
    byte_start INTEGER NOT NULL,
    byte_length INTEGER NOT NULL,
    created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS messages (
    id TEXT NOT NULL PRIMARY KEY,
    task_id TEXT NOT NULL,
    from_id TEXT NOT NULL,
    to_id TEXT NOT NULL,
    role INTEGER NOT NULL DEFAULT 0,
    message_type INTEGER NOT NULL DEFAULT 0,
    status INTEGER NOT NULL DEFAULT 0,
    content TEXT NOT NULL,
    meta_json TEXT NOT NULL DEFAULT '',
    created_by TEXT NOT NULL DEFAULT '',
    modified_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- Indexes
CREATE INDEX IF NOT EXISTS idx_organizations_id ON organizations(id);
CREATE INDEX IF NOT EXISTS idx_users_id ON users(id);
CREATE INDEX IF NOT EXISTS idx_users_organization_id ON users(organization_id);
CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
CREATE INDEX IF NOT EXISTS idx_stmi_agent_id ON short_term_memory_index(agent_id);
CREATE INDEX IF NOT EXISTS idx_stmi_created_at ON short_term_memory_index(created_at);
CREATE INDEX IF NOT EXISTS idx_stmi_tags ON short_term_memory_index(tags);
CREATE INDEX IF NOT EXISTS idx_ltkn_agent_id ON long_term_knowledge_node(agent_id);
CREATE INDEX IF NOT EXISTS idx_ltkn_node_type ON long_term_knowledge_node(node_type);
CREATE INDEX IF NOT EXISTS idx_knr_source_node_id ON knowledge_node_relation(source_node_id);
CREATE INDEX IF NOT EXISTS idx_knr_target_node_id ON knowledge_node_relation(target_node_id);
CREATE INDEX IF NOT EXISTS idx_kr_knowledge_id ON knowledge_reference(knowledge_id);
CREATE INDEX IF NOT EXISTS idx_kr_short_term_id ON knowledge_reference(short_term_id);
CREATE INDEX IF NOT EXISTS idx_kr_trace_id ON knowledge_reference(trace_id);
CREATE INDEX IF NOT EXISTS idx_messages_task_id ON messages(task_id);
CREATE INDEX IF NOT EXISTS idx_messages_from_id ON messages(from_id);
CREATE INDEX IF NOT EXISTS idx_messages_to_id ON messages(to_id);
CREATE INDEX IF NOT EXISTS idx_messages_created_at ON messages(created_at);
