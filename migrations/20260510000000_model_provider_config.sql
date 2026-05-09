-- 为 model_providers 添加 config 字段（JSON 存储）
ALTER TABLE model_providers ADD COLUMN config TEXT NOT NULL DEFAULT '{}';
