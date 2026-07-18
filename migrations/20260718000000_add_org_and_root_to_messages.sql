-- 为 messages 表添加 organization_id 和 root_id 字段
--
-- 背景：此前这两个字段被错误地直接写入 initial.sql 和 20260428100000 migration，
-- 违反了 sqlx migration 不可变原则，导致已部署数据库 checksum 校验失败。
-- 本 migration 通过 ALTER TABLE 方式补加字段，修正此问题。
--
-- 字段说明：
-- - organization_id: 异步消费时重建 RequestContext 上下文所需
-- - root_id: 消息链根消息 ID，用于消息链追踪

ALTER TABLE messages ADD COLUMN organization_id TEXT;
ALTER TABLE messages ADD COLUMN root_id TEXT;

-- 索引：加速按组织和根消息查询
CREATE INDEX IF NOT EXISTS idx_messages_organization_id ON messages(organization_id);
CREATE INDEX IF NOT EXISTS idx_messages_root_id ON messages(root_id);
