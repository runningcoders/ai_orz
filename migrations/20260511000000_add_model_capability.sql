-- 添加模型能力类型字段，区分 Agent / Embedding
ALTER TABLE model_providers ADD COLUMN capability INTEGER NOT NULL DEFAULT 0;