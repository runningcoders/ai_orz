-- 修正 provider_type 字段类型：TEXT -> INTEGER
-- SQLite 不支持 ALTER COLUMN TYPE，需要重建表

-- 1. 重命名旧表
ALTER TABLE model_providers RENAME TO model_providers_old;

-- 2. 建新表（用正确的 INTEGER 类型）
CREATE TABLE model_providers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    provider_type INTEGER NOT NULL,
    model_name TEXT NOT NULL,
    api_key TEXT NOT NULL,
    base_url TEXT,
    description TEXT,
    status INTEGER NOT NULL DEFAULT 1,
    created_by TEXT NOT NULL,
    modified_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- 3. 迁移数据，TEXT 转 INTEGER
INSERT INTO model_providers
SELECT id, name, CAST(provider_type as INTEGER), model_name, api_key, base_url, description,
       status, created_by, modified_by, created_at, updated_at
FROM model_providers_old;

-- 4. 删除旧表
DROP TABLE model_providers_old;
