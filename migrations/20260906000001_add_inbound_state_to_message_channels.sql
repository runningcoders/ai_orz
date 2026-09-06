-- 入站运行状态（InboundState 的 JSON 序列化：动态游标 + 动态会话，NULL = 从头开始）
-- 与 config_json 物理隔离：轮询循环只写本列，管理后台只写 config，互不覆盖。
ALTER TABLE message_channels ADD COLUMN inbound_state TEXT NULL;
