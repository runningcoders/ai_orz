-- 改造 artifacts 表：添加 project_id 字段，task_id 改为可选
-- 2026-05-09

-- SQLite 不支持直接 ALTER TABLE 修改列属性，需要重建表
-- 步骤：
-- 1. 创建新表
-- 2. 迁移数据（如果有旧数据的话，project_id 可以临时用空字符串，业务层修复）
-- 3. 删除旧表
-- 4. 重命名新表

CREATE TABLE artifacts_new (
    id TEXT NOT NULL PRIMARY KEY,
    project_id TEXT NOT NULL,
    task_id TEXT,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    file_type INTEGER NOT NULL,
    file_meta TEXT NOT NULL,
    status INTEGER NOT NULL DEFAULT 1,
    created_by TEXT NOT NULL,
    modified_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

-- 迁移旧数据到新表（project_id 暂时为空字符串，需要业务层根据 task_id 回填）
INSERT INTO artifacts_new (
    id, project_id, task_id, name, description, file_type, file_meta, status,
    created_by, modified_by, created_at, updated_at
)
SELECT
    id,
    '' AS project_id,  -- 空字符串占位，需要后续根据 task_id 回填
    task_id,
    name,
    description,
    file_type,
    file_meta,
    status,
    created_by,
    modified_by,
    created_at,
    updated_at
FROM artifacts;

-- 删除旧表
DROP TABLE artifacts;

-- 重命名新表
ALTER TABLE artifacts_new RENAME TO artifacts;

-- 创建索引
CREATE INDEX idx_artifacts_project_id ON artifacts(project_id);
CREATE INDEX idx_artifacts_task_id ON artifacts(task_id);
CREATE INDEX idx_artifacts_status ON artifacts(status);
