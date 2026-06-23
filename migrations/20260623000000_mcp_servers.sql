-- MCP Server 配置表
-- 仅保存 MCP Server 连接配置；不在 DAO 初始化阶段启动 client/session。

CREATE TABLE IF NOT EXISTS mcp_servers (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    transport INTEGER NOT NULL CHECK (transport IN (0, 1)), -- 0=stdio, 1=streamable_http
    config TEXT NOT NULL DEFAULT '{}',      -- JSON serialized McpServerConfig
    status INTEGER NOT NULL DEFAULT 1 CHECK (status IN (0, 1, 2)), -- 0=Deleted, 1=Enabled, 2=Disabled
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    created_by TEXT,
    updated_by TEXT
) STRICT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_mcp_servers_active_name_unique ON mcp_servers(name) WHERE status != 0;
CREATE INDEX IF NOT EXISTS idx_mcp_servers_transport ON mcp_servers(transport);
CREATE INDEX IF NOT EXISTS idx_mcp_servers_status ON mcp_servers(status);
CREATE INDEX IF NOT EXISTS idx_mcp_servers_created_at ON mcp_servers(created_at DESC);
