-- 组织级可扩展配置（JSON 文本）
--
-- 用于承载组织维度的可配置开关，例如「是否对普通消息构建向量索引」。
-- 后续新增组织级配置项时，只需在 OrganizationConfig 结构体中追加字段（带默认值），
-- 无需再改动表结构。
--
-- 默认值为 '{}'（空 JSON 对象），代码侧解析为空配置（enable_message_vector = false）。
ALTER TABLE organizations ADD COLUMN config TEXT NOT NULL DEFAULT '{}';
