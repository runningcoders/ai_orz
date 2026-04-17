-- Create tools table and agent_tools relation table

CREATE TABLE tools (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL,
    protocol TEXT NOT NULL,
    config JSON NOT NULL,
    parameters_schema JSON,
    status TEXT NOT NULL DEFAULT 'enabled',
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    created_by TEXT,
    updated_by TEXT
) STRICT;

CREATE TABLE agent_tools (
    agent_id TEXT NOT NULL,
    tool_id TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    created_by TEXT,
    PRIMARY KEY (agent_id, tool_id)
) STRICT;
