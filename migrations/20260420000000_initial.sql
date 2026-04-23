-- AI Orz 数据库初始 schema
-- 合并自所有迁移，开发阶段最终版本（2026-04-20）
-- 包含所有最新表结构：
--  - knowledge_reference: line_number 替代 byte_start/byte_length
--  - messages: 增加 project_id，允许 task_id 为空
--  - tools: protocol 和 status 使用 INTEGER 存储（替代 TEXT）
--  - projects: 增加 workflow 和 guidance 字段

-- 组织表
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

-- 用户表
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

-- Agent 表
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

-- 模型服务商表
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

-- 任务表
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
    created_by TEXT NOT NULL DEFAULT '',
    modified_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- 项目表
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
    created_by TEXT NOT NULL DEFAULT '',
    modified_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- 短期记忆索引表
CREATE TABLE IF NOT EXISTS short_term_memory_index (
    id TEXT NOT NULL PRIMARY KEY,
    agent_id TEXT NOT NULL,
    task_id TEXT,
    role TEXT NOT NULL,
    summary TEXT NOT NULL,
    tags TEXT NOT NULL,
    "status" INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- 长期知识节点表
CREATE TABLE IF NOT EXISTS long_term_knowledge_node (
    id TEXT NOT NULL PRIMARY KEY,
    agent_id TEXT NOT NULL,
    node_name TEXT NOT NULL,
    node_description TEXT NOT NULL,
    node_type TEXT NOT NULL,
    summary TEXT NOT NULL,
    "status" INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- 知识节点关系表
CREATE TABLE IF NOT EXISTS knowledge_node_relation (
    id TEXT NOT NULL PRIMARY KEY,
    source_node_id TEXT NOT NULL,
    target_node_id TEXT NOT NULL,
    relation_type TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- 知识引用表（使用 line_number 索引，替代 byte_start/byte_length）
CREATE TABLE IF NOT EXISTS knowledge_reference (
    id TEXT NOT NULL PRIMARY KEY,
    knowledge_id TEXT NOT NULL,
    short_term_id TEXT NOT NULL,
    trace_id TEXT NOT NULL,
    date_path TEXT NOT NULL,
    line_number INTEGER NOT NULL,
    created_at INTEGER NOT NULL
) STRICT;

-- 消息表（增加 project_id，允许 task_id 为空）
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT,
    task_id TEXT,
    from_id TEXT NOT NULL,
    to_id TEXT NOT NULL,
    from_role INTEGER NOT NULL DEFAULT 0,
    to_role INTEGER NOT NULL DEFAULT 0,
    message_type INTEGER NOT NULL DEFAULT 0,
    file_type INTEGER,
    "status" INTEGER NOT NULL DEFAULT 0,
    content TEXT NOT NULL,
    file_meta TEXT NOT NULL DEFAULT '{}',
    created_by TEXT NOT NULL DEFAULT '',
    modified_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- 工件附件表
CREATE TABLE IF NOT EXISTS artifacts (
    id TEXT NOT NULL PRIMARY KEY,
    task_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    file_type INTEGER NOT NULL,
    file_meta TEXT NOT NULL DEFAULT '{}',
    status INTEGER NOT NULL DEFAULT 1,
    created_by TEXT NOT NULL DEFAULT '',
    modified_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- 工具表（protocol 和 status 使用 INTEGER 存储）
-- ToolProtocol: 0=Builtin, 1=Http, 2=Mcp
-- ToolStatus: 0=Disabled, 1=Enabled
CREATE TABLE IF NOT EXISTS tools (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL,
    protocol INTEGER NOT NULL,
    control_mode INTEGER NOT NULL DEFAULT 0,
    config TEXT NOT NULL,
    parameters_schema TEXT,
    status INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    created_by TEXT,
    updated_by TEXT
) STRICT;

-- Agent 工具关联表
CREATE TABLE IF NOT EXISTS agent_tools (
    agent_id TEXT NOT NULL,
    tool_id TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    created_by TEXT,
    PRIMARY KEY (agent_id, tool_id)
) STRICT;

-- 技能表
CREATE TABLE IF NOT EXISTS skills (
    id TEXT NOT NULL PRIMARY KEY,                    -- 技能ID: "name-slug--hash" (名称slug--哈希前6位)
    name TEXT NOT NULL,                              -- 技能显示名称
    description TEXT NOT NULL DEFAULT '',            -- 技能描述：什么时候用这个技能
    tags TEXT NOT NULL DEFAULT '[]',                 -- JSON 数组：标签列表 ["产品", "PRD", "需求"]
    category TEXT NOT NULL DEFAULT '',               -- 单一分类："文档写作" / "问题解决" / "代码开发" / "研究分析"
    parent_skill_id TEXT NOT NULL DEFAULT '',        -- 父技能ID（继承来源，技能树演进）
    author_id TEXT NOT NULL DEFAULT '',              -- 创建人用户ID
    modifier_id TEXT NOT NULL DEFAULT '',            -- 最后修改人用户ID
    status INTEGER NOT NULL DEFAULT 2,               -- 技能状态：0=已过期 1=可用 2=待沉淀（默认待沉淀）
    created_at INTEGER NOT NULL,                     -- 创建时间戳（毫秒）
    updated_at INTEGER NOT NULL,                     -- 更新时间戳（毫秒）
    content_path TEXT NOT NULL                       -- 相对 base_data_path 的技能目录路径
) STRICT;

-- 索引
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
CREATE INDEX IF NOT EXISTS idx_skills_status ON skills(status);
CREATE INDEX IF NOT EXISTS idx_skills_category ON skills(category);
CREATE INDEX IF NOT EXISTS idx_skills_parent ON skills(parent_skill_id);
CREATE INDEX IF NOT EXISTS idx_skills_updated ON skills(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_skills_author ON skills(author_id);
