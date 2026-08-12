-- users 表新增 identity_credentials 字段：用户身份凭证库（JSON）
--
-- 背景：
-- - 飞书等外部平台的应用凭证（app_id/app_secret 等）属于用户级资产，
--   与具体消息渠道解耦：一条凭证可被多条渠道引用（ChannelConfig.lark_credential_id）
-- - 凭证类型化结构体约束（common::models::identity_credentials），
--   按 kind 区分凭证类型（当前仅 LarkApp，可扩展），secret 类字段落库前加密
--
-- 注意：
-- - users 为 STRICT 表，空字符串表示无凭证；非空为 JSON 数组结构

ALTER TABLE users ADD COLUMN identity_credentials TEXT NOT NULL DEFAULT '';
