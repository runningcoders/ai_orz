-- 拆分节点标签与发布标记：`user_tags` 承载用户/Agent 可写业务标签
--
-- 背景：`tags` 列长期兼任两个语义 —— 业务标签（可写）+ `published`（控制位）。
-- 「读全字段 → 原样回写」会把 `published` 当成业务标签处理，导致静默清掉发布标记。
--
-- 本迁移只做**加列 + 存量搬移**，不清洗任何数据：
--   * `user_tags` 成为唯一可写标签面（写侧双写过渡期同时维护 `tags`）
--   * `is_published` 保持 `published` 的唯一真相源
--   * `system_flags` **不落列**，由 DTO 从 `is_published` 派生
--   * `tags` 列全程保留（FTS5 触发器 knowledge_node_fts_ai/_au 依赖它），删除留待后续单
--
-- 存量脏值（`user_tags` 里已含 `published` 的节点）本迁移**不处理**，
-- 归后续数据单裁定；读侧 `published` 只认 `is_published`，这些残留值不语义生效。
--
-- 幂等：`user_tags` 有 DEFAULT '[]'，重跑时 WHERE 守卫避免覆盖已有值。
-- 回滚：`ALTER TABLE long_term_knowledge_node DROP COLUMN user_tags;`（`tags` 未删，数据零丢失）

ALTER TABLE long_term_knowledge_node ADD COLUMN user_tags TEXT NOT NULL DEFAULT '[]';

UPDATE long_term_knowledge_node
SET user_tags = tags
WHERE user_tags = '[]' AND tags != '[]';
