-- 外部会话 → 内部渠道绑定（入站路由）
--
-- 背景：飞书 / 其他 IM 的入站消息都带会话 ID（飞书 `chat_id`，私聊与群聊均有）。
-- 以「应用 + 会话」作为入站路由主键，可以让同一会话的消息稳定落到同一个渠道 /
-- Agent，而不依赖「发送者是否绑定过渠道」；发送者身份只用于 `from_id` 映射。
--
-- 绑定由入站适配层在首次成功解析渠道后写入（upsert）。`external_app_id` +
-- `external_tenant_key` 参与唯一键：同一 chat_id 可能在不同应用 / 租户下重复，
-- 必须按应用 + 租户隔离。表是渠道无关的：`channel_type` 区分平台，
-- 未来 Slack / 企微等直接复用。

CREATE TABLE IF NOT EXISTS message_channel_conversations (
    id TEXT PRIMARY KEY NOT NULL,
    channel_type INTEGER NOT NULL,
    external_app_id TEXT NOT NULL DEFAULT '',
    external_tenant_key TEXT NOT NULL DEFAULT '',
    external_chat_id TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_mcc_conversation_unique
    ON message_channel_conversations (
        channel_type, external_app_id, external_tenant_key, external_chat_id
    );

CREATE INDEX IF NOT EXISTS idx_mcc_conversation_channel
    ON message_channel_conversations (channel_id);
