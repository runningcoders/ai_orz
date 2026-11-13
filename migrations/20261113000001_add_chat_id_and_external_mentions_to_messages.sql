-- 消息群会话键 + 外部提及存档（飞书群消息支持）
--
-- 背景：飞书入站消息事件自带 chat_id（会话唯一标识 oc_xxx），私聊与群聊都有。
-- 落库后用于：1) 按「应用 + 租户 + 会话」做入站路由（见 message_channel_conversations）；
-- 2) 出站无引用回复目标时按会话直发（飞书 receive_id_type=chat_id）；
-- 3) 按会话聚合上下文。
--
-- external_mentions 存平台侧 @ 提及的原始信息（JSON 数组），与内部提及协议
-- （content 中的 `[@name](agent:<id>)`）并存：
-- - 命中内部 Agent 的提及：content 替换为内部协议，同时保留平台原始信息
-- - 未命中的提及：content 保留 `@name` 展示，平台原始 ID 存档备查
-- 统一以 `lark:` 等渠道前缀标注来源，防跨渠道撞键。
--
-- 列默认 '[]'，历史消息无需回填。

ALTER TABLE messages ADD COLUMN chat_id TEXT;

CREATE INDEX idx_messages_chat_id ON messages (chat_id);

ALTER TABLE messages ADD COLUMN external_mentions TEXT NOT NULL DEFAULT '[]';
