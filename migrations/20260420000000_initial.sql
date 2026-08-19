-- AI Orz 数据库初始 Schema（Squash Migration）
-- ============================================================
-- 本文件为合并所有迁移后的最终数据库结构（2026-08-19）
-- 包含 28 个迁移文件的最终状态：
--   - 所有主表 CREATE TABLE（无 ALTER TABLE / DROP TABLE）
--   - 所有 FTS5 全文索引虚拟表及触发器
--   - 所有索引
--   - 时间戳统一使用毫秒级：CAST(strftime('%s', 'now') * 1000 AS INTEGER)
--
-- 新增/变更摘要（相对于最初版本）：
--   - users: 新增 preferences 字段（identity_credentials 已拆至 user_credentials 表）
--   - agents: 新增 kind 字段
--   - model_providers: provider_type 改为 INTEGER，新增 config、capability 字段
--   - tasks: 新增 progress、execution_plan、execution_result 字段
--   - projects: 新增 execution_plan、execution_result、last_followup_at 字段
--   - messages: 重建（含 reply_to_id），新增 organization_id、root_id 字段
--   - artifacts: 重建（含 project_id），新增 source_type 字段
--   - tools: 新增 tags 字段，时间戳改为毫秒级
--   - agent_tools: 时间戳改为毫秒级
--   - skills: 重建（含 author_type）
--   - long_term_knowledge_node: 新增 tags、is_published 字段
--   - 新增表：vector_metadata, message_channels, attachments, mcp_servers,
--             cron_triggers, user_credentials
--   - 新增 FTS5 全文索引：skills_fts, tools_fts, messages_fts, tasks_fts,
--     projects_fts, agents_fts, short_term_memory_fts, knowledge_node_fts
-- ============================================================

-- ============================================================
-- 1. 组织表
-- ============================================================
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

-- ============================================================
-- 2. 用户表
-- ============================================================
CREATE TABLE IF NOT EXISTS users (
    id TEXT NOT NULL PRIMARY KEY,
    organization_id TEXT NOT NULL,
    username TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL DEFAULT '',
    email TEXT NOT NULL DEFAULT '',
    password_hash TEXT NOT NULL,
    role INTEGER NOT NULL,
    status INTEGER NOT NULL DEFAULT 1,
    preferences TEXT NOT NULL DEFAULT '',
    created_by TEXT NOT NULL DEFAULT '',
    modified_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- ============================================================
-- 3. Agent 表
-- ============================================================
CREATE TABLE IF NOT EXISTS agents (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    soul TEXT NOT NULL DEFAULT '',
    capabilities TEXT NOT NULL DEFAULT '',
    runtime_config TEXT NOT NULL DEFAULT '{}',
    model_provider_id TEXT NOT NULL,
    status INTEGER NOT NULL DEFAULT 1,
    kind INTEGER NOT NULL DEFAULT 0,
    created_by TEXT NOT NULL DEFAULT '',
    modified_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- ============================================================
-- 4. 模型服务商表
-- ============================================================
CREATE TABLE IF NOT EXISTS model_providers (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    provider_type INTEGER NOT NULL,
    model_name TEXT NOT NULL,
    api_key TEXT NOT NULL,
    base_url TEXT,
    description TEXT,
    config TEXT NOT NULL DEFAULT '{}',
    capability INTEGER NOT NULL DEFAULT 0,
    status INTEGER NOT NULL DEFAULT 1,
    created_by TEXT NOT NULL,
    modified_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- ============================================================
-- 5. 任务表
-- ============================================================
CREATE TABLE IF NOT EXISTS tasks (
    id TEXT NOT NULL PRIMARY KEY,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status INTEGER NOT NULL DEFAULT 1,
    priority INTEGER NOT NULL DEFAULT 0,
    tags TEXT NOT NULL DEFAULT '[]',
    due_at INTEGER,
    start_at INTEGER,
    end_at INTEGER,
    dependencies TEXT,
    root_user_id TEXT NOT NULL,
    assignee_type INTEGER NOT NULL DEFAULT 1,
    assignee_id TEXT NOT NULL,
    project_id TEXT,
    thinking_depth INTEGER NOT NULL DEFAULT 0,
    progress INTEGER NOT NULL DEFAULT 0,
    execution_plan TEXT,
    execution_result TEXT,
    created_by TEXT NOT NULL DEFAULT '',
    modified_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- ============================================================
-- 6. 项目表
-- ============================================================
CREATE TABLE IF NOT EXISTS projects (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    status INTEGER NOT NULL DEFAULT 1,
    priority INTEGER NOT NULL DEFAULT 0,
    tags TEXT NOT NULL DEFAULT '[]',
    root_user_id TEXT NOT NULL,
    owner_agent_id TEXT,
    workflow TEXT,
    guidance TEXT,
    start_at INTEGER,
    due_at INTEGER,
    end_at INTEGER,
    execution_plan TEXT,
    execution_result TEXT,
    last_followup_at INTEGER,
    created_by TEXT NOT NULL DEFAULT '',
    modified_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- ============================================================
-- 7. 短期记忆索引表
-- ============================================================
CREATE TABLE IF NOT EXISTS short_term_memory_index (
    id TEXT NOT NULL PRIMARY KEY,
    agent_id TEXT NOT NULL,
    task_id TEXT,
    role TEXT NOT NULL,
    summary TEXT NOT NULL,
    tags TEXT NOT NULL,
    trace_ids TEXT NOT NULL DEFAULT '[]',
    status INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- ============================================================
-- 8. 长期知识节点表
-- ============================================================
CREATE TABLE IF NOT EXISTS long_term_knowledge_node (
    id TEXT NOT NULL PRIMARY KEY,
    agent_id TEXT NOT NULL,
    node_name TEXT NOT NULL,
    node_description TEXT NOT NULL,
    node_type TEXT NOT NULL,
    summary TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',
    is_published INTEGER NOT NULL DEFAULT 0,
    status INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- ============================================================
-- 9. 知识节点关系表
-- ============================================================
CREATE TABLE IF NOT EXISTS knowledge_node_relation (
    id TEXT NOT NULL PRIMARY KEY,
    source_node_id TEXT NOT NULL,
    target_node_id TEXT NOT NULL,
    relation_type TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- ============================================================
-- 10. 知识引用表
-- ============================================================
CREATE TABLE IF NOT EXISTS knowledge_reference (
    id TEXT NOT NULL PRIMARY KEY,
    knowledge_id TEXT NOT NULL,
    short_term_id TEXT NOT NULL,
    trace_id TEXT NOT NULL,
    date_path TEXT NOT NULL,
    line_number INTEGER NOT NULL,
    created_at INTEGER NOT NULL
) STRICT;

-- ============================================================
-- 11. 消息表
-- ============================================================
CREATE TABLE IF NOT EXISTS messages (
    id TEXT NOT NULL PRIMARY KEY,
    project_id TEXT,
    task_id TEXT,
    from_id TEXT NOT NULL,
    to_id TEXT NOT NULL,
    from_role INTEGER NOT NULL,
    to_role INTEGER NOT NULL,
    message_type INTEGER NOT NULL,
    file_type INTEGER,
    status INTEGER NOT NULL DEFAULT 0,
    content TEXT NOT NULL,
    file_meta TEXT NOT NULL DEFAULT '{}',
    reply_to_id TEXT,
    organization_id TEXT,
    root_id TEXT,
    created_by TEXT NOT NULL,
    modified_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- ============================================================
-- 12. 工件附件表
-- ============================================================
CREATE TABLE IF NOT EXISTS artifacts (
    id TEXT NOT NULL PRIMARY KEY,
    project_id TEXT NOT NULL,
    task_id TEXT,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    file_type INTEGER NOT NULL,
    file_meta TEXT NOT NULL DEFAULT '{}',
    tags TEXT NOT NULL DEFAULT '[]',
    source_type INTEGER NOT NULL DEFAULT 1,
    status INTEGER NOT NULL DEFAULT 1,
    created_by TEXT NOT NULL DEFAULT '',
    modified_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- ============================================================
-- 13. 工具表
-- ============================================================
CREATE TABLE IF NOT EXISTS tools (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL,
    protocol INTEGER NOT NULL,
    control_mode INTEGER NOT NULL DEFAULT 0,
    config TEXT NOT NULL,
    parameters_schema TEXT,
    status INTEGER NOT NULL DEFAULT 1,
    tags TEXT NOT NULL DEFAULT '[]',
    created_at INTEGER NOT NULL DEFAULT (CAST(strftime('%s', 'now') * 1000 AS INTEGER)),
    updated_at INTEGER NOT NULL DEFAULT (CAST(strftime('%s', 'now') * 1000 AS INTEGER)),
    created_by TEXT,
    updated_by TEXT
) STRICT;

-- ============================================================
-- 14. Agent 工具关联表
-- ============================================================
CREATE TABLE IF NOT EXISTS agent_tools (
    agent_id TEXT NOT NULL,
    tool_id TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (CAST(strftime('%s', 'now') * 1000 AS INTEGER)),
    created_by TEXT,
    PRIMARY KEY (agent_id, tool_id)
) STRICT;

-- ============================================================
-- 15. 技能表
-- ============================================================
CREATE TABLE IF NOT EXISTS skills (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    tags TEXT NOT NULL DEFAULT '[]',
    category TEXT NOT NULL DEFAULT '',
    parent_skill_id TEXT NOT NULL DEFAULT '',
    author_id TEXT NOT NULL DEFAULT '',
    author_type INTEGER NOT NULL DEFAULT 0,
    modifier_id TEXT NOT NULL DEFAULT '',
    status INTEGER NOT NULL DEFAULT 2,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    content_path TEXT NOT NULL
) STRICT;

-- ============================================================
-- 16. 向量索引元数据表
-- ============================================================
CREATE TABLE IF NOT EXISTS vector_metadata (
    collection TEXT NOT NULL,
    source_id TEXT NOT NULL,
    content_hash TEXT,
    model TEXT,
    dimensions INTEGER,
    indexed_at INTEGER NOT NULL DEFAULT (CAST(strftime('%s', 'now') * 1000 AS INTEGER)),
    expire_at INTEGER,
    PRIMARY KEY (collection, source_id)
) STRICT;

-- ============================================================
-- 17. 消息渠道配置表
-- ============================================================
CREATE TABLE IF NOT EXISTS message_channels (
    id TEXT NOT NULL PRIMARY KEY,
    org_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    agent_id TEXT,
    channel_type INTEGER NOT NULL,
    channel_name TEXT NOT NULL,
    webhook_url TEXT,
    access_token TEXT,
    secret TEXT,
    config_json TEXT NOT NULL DEFAULT '{}',
    status INTEGER NOT NULL DEFAULT 1,
    last_pushed_at INTEGER,
    last_error TEXT,
    scope_project TEXT,
    created_by TEXT NOT NULL,
    modified_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- ============================================================
-- 18. 通用上传文件资产表
-- ============================================================
CREATE TABLE IF NOT EXISTS attachments (
    id TEXT NOT NULL PRIMARY KEY,
    original_name TEXT NOT NULL,
    stored_name TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    mime_type TEXT NOT NULL DEFAULT '',
    file_type INTEGER NOT NULL,
    size INTEGER NOT NULL DEFAULT 0,
    purpose TEXT NOT NULL DEFAULT '',
    status INTEGER NOT NULL DEFAULT 1,
    root_user_id TEXT NOT NULL,
    created_by TEXT NOT NULL DEFAULT '',
    modified_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- ============================================================
-- 19. MCP Server 配置表
-- ============================================================
CREATE TABLE IF NOT EXISTS mcp_servers (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    transport INTEGER NOT NULL CHECK (transport IN (0, 1)),
    config TEXT NOT NULL DEFAULT '{}',
    status INTEGER NOT NULL DEFAULT 1 CHECK (status IN (0, 1, 2)),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    created_by TEXT,
    updated_by TEXT
) STRICT;

-- ============================================================
-- 20. Cron 触发器表
-- ============================================================
CREATE TABLE IF NOT EXISTS cron_triggers (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    trigger_type INTEGER NOT NULL CHECK (trigger_type IN (0, 1, 2)),
    cron_expression TEXT,
    interval_seconds INTEGER,
    run_at INTEGER,
    next_run_at INTEGER NOT NULL,
    is_enabled INTEGER NOT NULL DEFAULT 1 CHECK (is_enabled IN (0, 1)),
    payload TEXT NOT NULL DEFAULT '{}',
    last_run_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    created_by TEXT,
    updated_by TEXT
) STRICT;

-- ============================================================
-- 21. 用户凭证表
-- ============================================================
CREATE TABLE IF NOT EXISTS user_credentials (
    id TEXT NOT NULL PRIMARY KEY,
    org_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    name TEXT NOT NULL,
    detail TEXT NOT NULL,
    visibility TEXT NOT NULL DEFAULT 'private',
    is_default INTEGER NOT NULL DEFAULT 0,
    status INTEGER NOT NULL DEFAULT 1,
    created_by TEXT NOT NULL,
    modified_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- ============================================================
-- FTS5 全文索引虚拟表
-- ============================================================

-- 短期记忆全文索引
CREATE VIRTUAL TABLE IF NOT EXISTS short_term_memory_fts USING fts5(
    summary,
    tags,
    tokenize = 'trigram'
);

-- 知识节点全文索引
CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_node_fts USING fts5(
    node_name,
    summary,
    node_description,
    tags,
    tokenize = 'trigram'
);

-- skills 全文索引
CREATE VIRTUAL TABLE IF NOT EXISTS skills_fts USING fts5(
    name,
    description,
    tags,
    tokenize = 'trigram'
);

-- tools 全文索引
CREATE VIRTUAL TABLE IF NOT EXISTS tools_fts USING fts5(
    name,
    description,
    tags,
    tokenize = 'trigram'
);

-- messages 全文索引
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    content,
    tokenize = 'trigram'
);

-- tasks 全文索引
CREATE VIRTUAL TABLE IF NOT EXISTS tasks_fts USING fts5(
    title,
    description,
    tags,
    tokenize = 'trigram'
);

-- projects 全文索引
CREATE VIRTUAL TABLE IF NOT EXISTS projects_fts USING fts5(
    name,
    description,
    workflow,
    guidance,
    tags,
    tokenize = 'trigram'
);

-- agents 全文索引
CREATE VIRTUAL TABLE IF NOT EXISTS agents_fts USING fts5(
    name,
    role,
    description,
    capabilities,
    tokenize = 'trigram'
);

-- ============================================================
-- FTS5 触发器：短期记忆同步
-- ============================================================

CREATE TRIGGER IF NOT EXISTS short_term_memory_fts_ai
AFTER INSERT ON short_term_memory_index BEGIN
    INSERT INTO short_term_memory_fts(rowid, summary, tags)
    VALUES (new.rowid, new.summary, new.tags);
END;

CREATE TRIGGER IF NOT EXISTS short_term_memory_fts_ad
AFTER DELETE ON short_term_memory_index BEGIN
    DELETE FROM short_term_memory_fts WHERE rowid = old.rowid;
END;

CREATE TRIGGER IF NOT EXISTS short_term_memory_fts_au
AFTER UPDATE ON short_term_memory_index BEGIN
    DELETE FROM short_term_memory_fts WHERE rowid = old.rowid;
    INSERT INTO short_term_memory_fts(rowid, summary, tags)
    VALUES (new.rowid, new.summary, new.tags);
END;

-- ============================================================
-- FTS5 触发器：知识节点同步
-- ============================================================

CREATE TRIGGER IF NOT EXISTS knowledge_node_fts_ai
AFTER INSERT ON long_term_knowledge_node BEGIN
    INSERT INTO knowledge_node_fts(rowid, node_name, summary, node_description, tags)
    VALUES (new.rowid, new.node_name, new.summary, new.node_description, new.tags);
END;

CREATE TRIGGER IF NOT EXISTS knowledge_node_fts_ad
AFTER DELETE ON long_term_knowledge_node BEGIN
    DELETE FROM knowledge_node_fts WHERE rowid = old.rowid;
END;

CREATE TRIGGER IF NOT EXISTS knowledge_node_fts_au
AFTER UPDATE ON long_term_knowledge_node BEGIN
    DELETE FROM knowledge_node_fts WHERE rowid = old.rowid;
    INSERT INTO knowledge_node_fts(rowid, node_name, summary, node_description, tags)
    VALUES (new.rowid, new.node_name, new.summary, new.node_description, new.tags);
END;

-- ============================================================
-- FTS5 触发器：skills 同步
-- ============================================================

CREATE TRIGGER IF NOT EXISTS skills_fts_ai
AFTER INSERT ON skills BEGIN
    INSERT INTO skills_fts(rowid, name, description, tags)
    VALUES (new.rowid, new.name, new.description, new.tags);
END;

CREATE TRIGGER IF NOT EXISTS skills_fts_ad
AFTER DELETE ON skills BEGIN
    DELETE FROM skills_fts WHERE rowid = old.rowid;
END;

CREATE TRIGGER IF NOT EXISTS skills_fts_au
AFTER UPDATE ON skills BEGIN
    DELETE FROM skills_fts WHERE rowid = old.rowid;
    INSERT INTO skills_fts(rowid, name, description, tags)
    VALUES (new.rowid, new.name, new.description, new.tags);
END;

-- ============================================================
-- FTS5 触发器：tools 同步
-- ============================================================

CREATE TRIGGER IF NOT EXISTS tools_fts_ai
AFTER INSERT ON tools BEGIN
    INSERT INTO tools_fts(rowid, name, description, tags)
    VALUES (new.rowid, new.name, new.description, new.tags);
END;

CREATE TRIGGER IF NOT EXISTS tools_fts_ad
AFTER DELETE ON tools BEGIN
    DELETE FROM tools_fts WHERE rowid = old.rowid;
END;

CREATE TRIGGER IF NOT EXISTS tools_fts_au
AFTER UPDATE ON tools BEGIN
    DELETE FROM tools_fts WHERE rowid = old.rowid;
    INSERT INTO tools_fts(rowid, name, description, tags)
    VALUES (new.rowid, new.name, new.description, new.tags);
END;

-- ============================================================
-- FTS5 触发器：messages 同步
-- ============================================================

CREATE TRIGGER IF NOT EXISTS messages_fts_ai
AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, content)
    VALUES (new.rowid, new.content);
END;

CREATE TRIGGER IF NOT EXISTS messages_fts_ad
AFTER DELETE ON messages BEGIN
    DELETE FROM messages_fts WHERE rowid = old.rowid;
END;

CREATE TRIGGER IF NOT EXISTS messages_fts_au
AFTER UPDATE ON messages BEGIN
    DELETE FROM messages_fts WHERE rowid = old.rowid;
    INSERT INTO messages_fts(rowid, content)
    VALUES (new.rowid, new.content);
END;

-- ============================================================
-- FTS5 触发器：tasks 同步
-- ============================================================

CREATE TRIGGER IF NOT EXISTS tasks_fts_ai
AFTER INSERT ON tasks BEGIN
    INSERT INTO tasks_fts(rowid, title, description, tags)
    VALUES (new.rowid, new.title, new.description, new.tags);
END;

CREATE TRIGGER IF NOT EXISTS tasks_fts_ad
AFTER DELETE ON tasks BEGIN
    DELETE FROM tasks_fts WHERE rowid = old.rowid;
END;

CREATE TRIGGER IF NOT EXISTS tasks_fts_au
AFTER UPDATE ON tasks BEGIN
    DELETE FROM tasks_fts WHERE rowid = old.rowid;
    INSERT INTO tasks_fts(rowid, title, description, tags)
    VALUES (new.rowid, new.title, new.description, new.tags);
END;

-- ============================================================
-- FTS5 触发器：projects 同步
-- ============================================================

CREATE TRIGGER IF NOT EXISTS projects_fts_ai
AFTER INSERT ON projects BEGIN
    INSERT INTO projects_fts(rowid, name, description, workflow, guidance, tags)
    VALUES (new.rowid, new.name, new.description, new.workflow, new.guidance, new.tags);
END;

CREATE TRIGGER IF NOT EXISTS projects_fts_ad
AFTER DELETE ON projects BEGIN
    DELETE FROM projects_fts WHERE rowid = old.rowid;
END;

CREATE TRIGGER IF NOT EXISTS projects_fts_au
AFTER UPDATE ON projects BEGIN
    DELETE FROM projects_fts WHERE rowid = old.rowid;
    INSERT INTO projects_fts(rowid, name, description, workflow, guidance, tags)
    VALUES (new.rowid, new.name, new.description, new.workflow, new.guidance, new.tags);
END;

-- ============================================================
-- FTS5 触发器：agents 同步
-- ============================================================

CREATE TRIGGER IF NOT EXISTS agents_fts_ai
AFTER INSERT ON agents BEGIN
    INSERT INTO agents_fts(rowid, name, role, description, capabilities)
    VALUES (new.rowid, new.name, new.role, new.description, new.capabilities);
END;

CREATE TRIGGER IF NOT EXISTS agents_fts_ad
AFTER DELETE ON agents BEGIN
    DELETE FROM agents_fts WHERE rowid = old.rowid;
END;

CREATE TRIGGER IF NOT EXISTS agents_fts_au
AFTER UPDATE ON agents BEGIN
    DELETE FROM agents_fts WHERE rowid = old.rowid;
    INSERT INTO agents_fts(rowid, name, role, description, capabilities)
    VALUES (new.rowid, new.name, new.role, new.description, new.capabilities);
END;

-- ============================================================
-- 索引
-- ============================================================

-- organizations
CREATE INDEX IF NOT EXISTS idx_organizations_id ON organizations(id);

-- users
CREATE INDEX IF NOT EXISTS idx_users_id ON users(id);
CREATE INDEX IF NOT EXISTS idx_users_organization_id ON users(organization_id);
CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);

-- short_term_memory_index
CREATE INDEX IF NOT EXISTS idx_stmi_agent_id ON short_term_memory_index(agent_id);
CREATE INDEX IF NOT EXISTS idx_stmi_created_at ON short_term_memory_index(created_at);
CREATE INDEX IF NOT EXISTS idx_stmi_tags ON short_term_memory_index(tags);

-- long_term_knowledge_node
CREATE INDEX IF NOT EXISTS idx_ltkn_agent_id ON long_term_knowledge_node(agent_id);
CREATE INDEX IF NOT EXISTS idx_ltkn_node_type ON long_term_knowledge_node(node_type);
CREATE INDEX IF NOT EXISTS idx_ltkn_tags ON long_term_knowledge_node(tags);
CREATE INDEX IF NOT EXISTS idx_ltkn_is_published ON long_term_knowledge_node(is_published) WHERE is_published = 1;

-- knowledge_node_relation
CREATE INDEX IF NOT EXISTS idx_knr_source_node_id ON knowledge_node_relation(source_node_id);
CREATE INDEX IF NOT EXISTS idx_knr_target_node_id ON knowledge_node_relation(target_node_id);

-- knowledge_reference
CREATE INDEX IF NOT EXISTS idx_kr_knowledge_id ON knowledge_reference(knowledge_id);
CREATE INDEX IF NOT EXISTS idx_kr_short_term_id ON knowledge_reference(short_term_id);
CREATE INDEX IF NOT EXISTS idx_kr_trace_id ON knowledge_reference(trace_id);

-- messages
CREATE INDEX IF NOT EXISTS idx_messages_project_id ON messages(project_id);
CREATE INDEX IF NOT EXISTS idx_messages_task_id ON messages(task_id);
CREATE INDEX IF NOT EXISTS idx_messages_from_id ON messages(from_id);
CREATE INDEX IF NOT EXISTS idx_messages_to_id ON messages(to_id);
CREATE INDEX IF NOT EXISTS idx_messages_status ON messages(status);
CREATE INDEX IF NOT EXISTS idx_messages_created_at ON messages(created_at);
CREATE INDEX IF NOT EXISTS idx_messages_reply_to_id ON messages(reply_to_id);
CREATE INDEX IF NOT EXISTS idx_messages_organization_id ON messages(organization_id);
CREATE INDEX IF NOT EXISTS idx_messages_root_id ON messages(root_id);

-- skills
CREATE INDEX IF NOT EXISTS idx_skills_status ON skills(status);
CREATE INDEX IF NOT EXISTS idx_skills_category ON skills(category);
CREATE INDEX IF NOT EXISTS idx_skills_parent ON skills(parent_skill_id);
CREATE INDEX IF NOT EXISTS idx_skills_updated ON skills(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_skills_author ON skills(author_id);

-- artifacts
CREATE INDEX IF NOT EXISTS idx_artifacts_project_id ON artifacts(project_id);
CREATE INDEX IF NOT EXISTS idx_artifacts_task_id ON artifacts(task_id);
CREATE INDEX IF NOT EXISTS idx_artifacts_status ON artifacts(status);
CREATE INDEX IF NOT EXISTS idx_artifacts_source_type ON artifacts(source_type);

-- vector_metadata
CREATE INDEX IF NOT EXISTS idx_vector_metadata_expire ON vector_metadata(expire_at);

-- message_channels
CREATE INDEX IF NOT EXISTS idx_message_channels_org_id ON message_channels(org_id);
CREATE INDEX IF NOT EXISTS idx_message_channels_user_id ON message_channels(user_id);
CREATE INDEX IF NOT EXISTS idx_message_channels_agent_id ON message_channels(agent_id);
CREATE INDEX IF NOT EXISTS idx_message_channels_channel_type ON message_channels(channel_type);
CREATE INDEX IF NOT EXISTS idx_message_channels_status ON message_channels(status);
CREATE INDEX IF NOT EXISTS idx_message_channels_scope_project ON message_channels(scope_project);

-- attachments
CREATE INDEX IF NOT EXISTS idx_attachments_root_user_id ON attachments(root_user_id);
CREATE INDEX IF NOT EXISTS idx_attachments_purpose ON attachments(purpose);
CREATE INDEX IF NOT EXISTS idx_attachments_status ON attachments(status);
CREATE INDEX IF NOT EXISTS idx_attachments_created_at ON attachments(created_at);

-- mcp_servers
CREATE UNIQUE INDEX IF NOT EXISTS idx_mcp_servers_active_name_unique ON mcp_servers(name) WHERE status != 0;
CREATE INDEX IF NOT EXISTS idx_mcp_servers_transport ON mcp_servers(transport);
CREATE INDEX IF NOT EXISTS idx_mcp_servers_status ON mcp_servers(status);
CREATE INDEX IF NOT EXISTS idx_mcp_servers_created_at ON mcp_servers(created_at DESC);

-- cron_triggers
CREATE INDEX IF NOT EXISTS idx_cron_triggers_next_run_at ON cron_triggers(next_run_at);
CREATE INDEX IF NOT EXISTS idx_cron_triggers_is_enabled ON cron_triggers(is_enabled);
CREATE INDEX IF NOT EXISTS idx_cron_triggers_trigger_type ON cron_triggers(trigger_type);
CREATE INDEX IF NOT EXISTS idx_cron_triggers_created_at ON cron_triggers(created_at DESC);

-- user_credentials
CREATE UNIQUE INDEX IF NOT EXISTS uq_user_credentials_default_private
ON user_credentials(user_id, kind)
WHERE is_default = 1 AND visibility = 'private' AND status = 1;

CREATE UNIQUE INDEX IF NOT EXISTS uq_user_credentials_default_public
ON user_credentials(org_id, kind)
WHERE is_default = 1 AND visibility = 'public' AND status = 1;

CREATE INDEX IF NOT EXISTS idx_user_credentials_org_id ON user_credentials(org_id);
CREATE INDEX IF NOT EXISTS idx_user_credentials_user_id ON user_credentials(user_id);
CREATE INDEX IF NOT EXISTS idx_user_credentials_kind ON user_credentials(kind);
CREATE INDEX IF NOT EXISTS idx_user_credentials_visibility ON user_credentials(visibility);
CREATE INDEX IF NOT EXISTS idx_user_credentials_status ON user_credentials(status);