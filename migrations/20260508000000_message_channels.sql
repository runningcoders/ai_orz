-- 消息渠道配置表
-- 支持用户配置多个推送渠道（飞书、微信、Slack、邮件、Webhook 等）
-- 支持为特定 Agent 绑定专用渠道

CREATE TABLE IF NOT EXISTS message_channels (
    id TEXT PRIMARY KEY NOT NULL,                        -- 渠道 ID（UUID v7）
    org_id TEXT NOT NULL,                                -- 组织 ID，多租户隔离
    user_id TEXT NOT NULL,                               -- 绑定的用户 ID
    agent_id TEXT,                                       -- 关联的 Agent ID（NULL 表示用户全局默认渠道）
    channel_type INTEGER NOT NULL,                       -- 渠道类型枚举：0=飞书, 1=微信, 2=Slack, 3=邮件, 4=Webhook
    channel_name TEXT NOT NULL,                          -- 用户自定义的渠道名称
    webhook_url TEXT,                                    -- Webhook 地址（飞书、Slack、通用 Webhook 等使用）
    access_token TEXT,                                   -- 访问 Token（需要鉴权的渠道）
    secret TEXT,                                         -- 签名密钥/Secret
    config_json TEXT NOT NULL DEFAULT '{}',              -- 扩展配置 JSON（各渠道的详细配置，对应 ChannelConfig 结构体）
    is_enabled INTEGER NOT NULL DEFAULT 1,               -- 是否启用：0=禁用, 1=启用
    last_pushed_at INTEGER,                              -- 最后成功推送的时间戳（毫秒）
    last_error TEXT,                                     -- 最后一次推送的错误信息
    created_by TEXT NOT NULL,                            -- 创建人 ID
    modified_by TEXT NOT NULL,                           -- 最后修改人 ID
    created_at INTEGER NOT NULL,                         -- 创建时间戳（毫秒）
    updated_at INTEGER NOT NULL                          -- 更新时间戳（毫秒）
);

-- 索引：加速查询
CREATE INDEX IF NOT EXISTS idx_message_channels_org_id ON message_channels(org_id);
CREATE INDEX IF NOT EXISTS idx_message_channels_user_id ON message_channels(user_id);
CREATE INDEX IF NOT EXISTS idx_message_channels_agent_id ON message_channels(agent_id);
CREATE INDEX IF NOT EXISTS idx_message_channels_channel_type ON message_channels(channel_type);
CREATE INDEX IF NOT EXISTS idx_message_channels_is_enabled ON message_channels(is_enabled);
