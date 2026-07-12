-- 为 tasks 表添加 progress 进度字段
-- progress: 0-100 整数，表示任务完成百分比
ALTER TABLE tasks ADD COLUMN progress INTEGER NOT NULL DEFAULT 0;
