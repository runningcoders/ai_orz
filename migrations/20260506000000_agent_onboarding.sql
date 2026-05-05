-- Agent 入职流程相关字段更新
-- 1. tools 表增加 tags 字段（用于标签化工具）
-- 2. agents 表 role 字段默认值调整为 '[]'（从单一角色变为角色标签数组）

ALTER TABLE tools ADD COLUMN "tags" TEXT NOT NULL DEFAULT '[]';

-- 注意：不直接 ALTER agents 表的默认值，因为 SQLite 不支持 ALTER COLUMN
-- 代码层面处理：role 为空字符串时解析为空数组
