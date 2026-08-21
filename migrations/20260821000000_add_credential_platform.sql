-- 用户凭证表增加 platform 维度（generic 类凭据按 (kind, platform) 匹配）
ALTER TABLE user_credentials ADD COLUMN platform TEXT NULL;

-- 默认唯一索引升级：加 platform 维度（存量专用 kind 行 platform 为 NULL，语义不变）
DROP INDEX IF EXISTS uq_user_credentials_default_private;
CREATE UNIQUE INDEX uq_user_credentials_default_private
ON user_credentials(user_id, kind, platform)
WHERE is_default = 1 AND visibility = 'private' AND status = 1;

DROP INDEX IF EXISTS uq_user_credentials_default_public;
CREATE UNIQUE INDEX uq_user_credentials_default_public
ON user_credentials(org_id, kind, platform)
WHERE is_default = 1 AND visibility = 'public' AND status = 1;

CREATE INDEX IF NOT EXISTS idx_user_credentials_platform ON user_credentials(platform);
