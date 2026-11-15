-- 任务交付责任链：把「服务的用户」与「发起者」拆成两个正交身份，
-- 并记录任务诞生于哪条消息，让派单 / 回执 / 收尾能挂在同一条消息链上。
--
--   root_user_id       = 任务最终关联给谁（归属用户，既有字段）
--   initiator_agent_id = 谁发起 / 派发这个任务，对交付结果负责（一般是前台接待 Agent）
--   trigger_message_id = 任务诞生于哪条消息；派生消息以此作为 reply_to_id 的锚点
--
-- 存量任务没有发起链，落空串表示「无交付责任方」：派发者退回归属用户
--（`task_sender_identity` 的第二档），Final 不会越级到用户，对历史数据零影响。
ALTER TABLE tasks ADD COLUMN initiator_agent_id TEXT NOT NULL DEFAULT '';
ALTER TABLE tasks ADD COLUMN trigger_message_id TEXT NOT NULL DEFAULT '';
