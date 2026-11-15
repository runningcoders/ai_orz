-- 消息三身份模型：所属用户独立成列
--
-- 背景：messages 只有 from/to 两个身份位，而一条消息实际涉及三个身份 -
-- - 派发者：Agent 或 User
-- - 接收者：Agent 或 User
-- - 所属用户：恒为 User（工具凭据 / 用户画像 / 环境变量的来源）
--
-- 历史上为了让被唤醒 Agent 的 ctx.user_id 有值，派单与通知把「所属用户」
-- 塞进了 from 位（用户身份中继：from_id=用户, from_role=User），代价是真实
-- 派发者不可见、Final 路由需要特例改道、`ReplyObligation` 对派单不生效。
--
-- 本列落「这条消息归属哪个用户」：
-- - 用户发来的消息       → 发送者本人
-- - Agent 发给用户       → 收件用户
-- - Agent 发给 Agent     → 交付链的归属用户（派单 / 协作 / 通知）
--
-- 空串 = 无归属（A2A、纯系统触发）。消费侧 `rebuild_context` 三档回落：
-- root_user_id → from_role=User 时取 from_id（存量消息）→ carrier.user_id。
--
-- 历史回填只对 `from_role = 0`（User）有意义：存量中继消息的 from_id 就是
-- 归属用户；Agent 间消息没有可用信息，留空交给 carrier 兜底。

ALTER TABLE messages ADD COLUMN root_user_id TEXT NOT NULL DEFAULT '';

UPDATE messages SET root_user_id = from_id WHERE from_role = 0 AND root_user_id = '';

CREATE INDEX idx_messages_root_user_id ON messages (root_user_id);
