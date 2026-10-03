-- 消息话题键（飞书「话题」贯通）
--
-- 背景：飞书「话题」形态自带 thread_id（omt_xxx 命名空间），同一话题内
-- 所有消息共享同一 thread_id。该字段与 external_key（om_xxx，单条消息 ID）
-- 不同源，需独立列承载，才可在出站时按话题回复、在同一话题内聚合上下文。
--
-- thread_id 形如 "omt_xxx"（飞书原生命名空间）：
-- - 入站：渠道消息落库时随消息写入（经 SendToAgentCommand 透传）
-- - 出站：按是否存在 thread_id 选择普通回复 / 话题回复（后续实现）
--
-- 普通私信、非话题回复无此字段，留 NULL。

ALTER TABLE messages ADD COLUMN thread_id TEXT;

CREATE INDEX idx_messages_thread_id ON messages (thread_id);