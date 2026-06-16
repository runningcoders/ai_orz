-- 通用上传文件资产表（Finance Domain / 用户资产）
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

CREATE INDEX IF NOT EXISTS idx_attachments_root_user_id ON attachments(root_user_id);
CREATE INDEX IF NOT EXISTS idx_attachments_purpose ON attachments(purpose);
CREATE INDEX IF NOT EXISTS idx_attachments_status ON attachments(status);
CREATE INDEX IF NOT EXISTS idx_attachments_created_at ON attachments(created_at);
