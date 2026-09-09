-- 消息外部键（消息链跨渠道贯通，飞书线程回复场景）
--
-- 背景：飞书入站回复/话题消息自带 parent_id/root_id（om_xxx 命名空间），
-- 与内部消息 ID 不同源；若不落映射，入站回复无法解析出内部父消息，
-- reply_to_id 只能悬空或丢弃，消息链在渠道侧断裂。
--
-- external_key 形如 "lark:om_xxx"（渠道前缀防跨渠道撞键）：
-- - 入站：渠道消息落库时随消息写入（经 SendToAgentCommand 透传）
-- - 出站：推送成功后按内部消息 ID 回写（DAO 拿到飞书返回的 message_id）
-- - 解析：入站 adapt 按 parent_id/root_id 反查（external_key 索引点查）
--
-- 查不到即父消息未留痕（历史消息等），上层降级为不挂链
-- （该消息自身成为新链根），不做启发式猜测。

ALTER TABLE messages ADD COLUMN external_key TEXT;

CREATE INDEX idx_messages_external_key ON messages (external_key);
