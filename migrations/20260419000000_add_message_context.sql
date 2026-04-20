-- 迁移：增加消息上下文支持，新增项目上下文字段，允许 task_id 为空
-- 日期：2026-04-19
-- 开发阶段：直接重建消息表保证 schema 正确，SQLite 的 ALTER 能力有限

-- 1. 重命名旧表
ALTER TABLE messages RENAME TO messages_old;

-- 2. 创建新表，包含 project_id 且 task_id 允许为空
CREATE TABLE messages (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT,
    task_id TEXT,
    from_id TEXT NOT NULL,
    to_id TEXT NOT NULL,
    "role" INTEGER NOT NULL,
    message_type INTEGER NOT NULL,
    file_type INTEGER,
    "status" INTEGER NOT NULL,
    content TEXT NOT NULL,
    file_meta TEXT NOT NULL,
    created_by TEXT NOT NULL,
    modified_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- 3. 拷贝数据（task_id -> task_id，新增 project_id 为 NULL）
INSERT INTO messages (
    id, project_id, task_id, from_id, to_id, "role", message_type, 
    file_type, "status", content, file_meta, created_by, modified_by, 
    created_at, updated_at
)
SELECT 
    id, NULL, task_id, from_id, to_id, "role", message_type, 
    file_type, "status", content, file_meta, created_by, modified_by, 
    created_at, updated_at
FROM messages_old;

-- 4. 删除旧表
DROP TABLE messages_old;

-- 5. 给 projects 表增加 workflow 和 guidance 字段（允许 NULL）
ALTER TABLE projects ADD COLUMN workflow TEXT;
ALTER TABLE projects ADD COLUMN guidance TEXT;

