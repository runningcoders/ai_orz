-- users 表新增 preferences 字段：用户自述偏好（声明式画像）
--
-- 背景：
-- - 用户偏好双源机制：users.preferences 承载用户本人声明的偏好（权威自述），
--   Agent 观察总结的偏好走知识图谱（user_preference tag），两者独立演进
-- - 该字段只允许用户本人通过 update_current_user 修改，Agent 无写入路径
-- - 内容为自由文本（Markdown 书写），前端展示态统一 Markdown 渲染
--
-- 注意：
-- - users 为 STRICT 表，空字符串表示未设置偏好

ALTER TABLE users ADD COLUMN preferences TEXT NOT NULL DEFAULT '';
