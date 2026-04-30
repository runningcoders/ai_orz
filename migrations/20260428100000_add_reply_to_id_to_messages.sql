-- 为 messages 表添加 reply_to_id 字段，支持消息链/引用关系

-- SQLite 不支持直接 ALTER TABLE ADD COLUMN 后设置默认值的复杂操作
-- 所以我们使用重建表的方式（更干净，符合我们之前的迁移实践）

-- 1. 创建新表
CREATE TABLE IF NOT EXISTS messages_new (
    id TEXT PRIMARY KEY NOT NULL,
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
    reply_to_id TEXT,  -- 新增：引用的父消息 ID
    created_by TEXT NOT NULL,
    modified_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- 2. 复制数据
INSERT INTO messages_new (
    id, project_id, task_id, from_id, to_id, from_role, to_role,
    message_type, file_type, status, content, file_meta,
    created_by, modified_by, created_at, updated_at
)
SELECT
    id, project_id, task_id, from_id, to_id, from_role, to_role,
    message_type, file_type, status, content, file_meta,
    created_by, modified_by, created_at, updated_at
FROM messages;

-- 3. 删除旧表
DROP TABLE messages;

-- 4. 重命名新表
ALTER TABLE messages_new RENAME TO messages;

-- 5. 重建索引
CREATE INDEX IF NOT EXISTS idx_messages_project_id ON messages(project_id);
CREATE INDEX IF NOT EXISTS idx_messages_task_id ON messages(task_id);
CREATE INDEX IF NOT EXISTS idx_messages_from_id ON messages(from_id);
CREATE INDEX IF NOT EXISTS idx_messages_to_id ON messages(to_id);
CREATE INDEX IF NOT EXISTS idx_messages_status ON messages(status);
CREATE INDEX IF NOT EXISTS idx_messages_created_at ON messages(created_at);
CREATE INDEX IF NOT EXISTS idx_messages_reply_to_id ON messages(reply_to_id);
