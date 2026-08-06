-- 为 projects 表添加执行计划/结果/跟进时间字段
ALTER TABLE projects ADD COLUMN execution_plan TEXT;
ALTER TABLE projects ADD COLUMN execution_result TEXT;
ALTER TABLE projects ADD COLUMN last_followup_at INTEGER;

-- 为 tasks 表添加执行计划/结果字段
ALTER TABLE tasks ADD COLUMN execution_plan TEXT;
ALTER TABLE tasks ADD COLUMN execution_result TEXT;
