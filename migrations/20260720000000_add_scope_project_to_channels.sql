-- 消息渠道表新增 scope_project 字段：用于限定推送范围
-- NULL = 所有项目（用户级全局渠道）
-- 非空 = 仅该项目的消息（如 A2A PushNotifications 回调）
ALTER TABLE message_channels ADD COLUMN scope_project TEXT;

-- 索引：加速按项目查询渠道
CREATE INDEX IF NOT EXISTS idx_message_channels_scope_project ON message_channels(scope_project);
